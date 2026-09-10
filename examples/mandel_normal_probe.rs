//! Evaluate production normals at supplied world points without changing traversal.
use anyhow::{Context, Result, ensure};
use fpt_metal::{ffi::*, mandelbulber::compiler::retain_metal_kernels, scene};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    ffi::{CStr, CString},
    fs,
    path::Path,
};

#[path = "precision/analytic85.rs"]
mod analytic85;
#[path = "precision/march_controls.rs"]
mod march_controls;

const KERNEL: &str = r#"
kernel void mandelbulber_field_sample_kernel(
    device const float4 *points [[buffer(0)]], device float4 *samples [[buffer(1)]],
    constant FptRenderConfig &cfg [[buffer(2)]], uint gid [[thread_position_in_grid]]) {
    float3 p = points[gid].xyz;
    if (points[gid].w == -5.0f || points[gid].w == -6.0f) {
        MandelbulberMarchResult hit = points[gid].w == -5.0f
            ? marchMandelbulber(p, cameraPos(cfg), int(cfg.render[1]), cfg)
            : probeMarchBeforeRefinement(p, cameraPos(cfg), int(cfg.render[1]), cfg);
        samples[gid] = float4(hit.position, float(hit.found));
        return;
    }
    if (points[gid].w == -1.0f) {
        samples[gid] = float4(deMandelbulber(p, cfg).d,
            mandelbulberMarchThreshold(p, cfg), 0.0f, 0.0f);
        return;
    }
    if (points[gid].w == -2.0f) {
        samples[gid] = mandelbulberFieldSample(p, cfg, 1);
        return;
    }
    if (points[gid].w == -3.0f) {
        samples[gid] = float4(mandelbulberCameraRay(p.xy, cfg), 0.0f);
        return;
    }
    if (points[gid].w == -4.0f) {
        float3 ray = mandelbulberCameraRay(p.xy, cfg);
        MandelbulberMarchResult hit = marchMandelbulber(ray, cameraPos(cfg), int(cfg.render[1]), cfg);
        samples[gid] = float4(hit.position, float(hit.found));
        return;
    }
#if defined(FPT_NORMAL_ORBIT_PROBE)
    if (points[gid].w >= 2.0f) {
        float3 scaled = mandelbulberGlobalPoint(p, cfg) / max(setv(cfg, 0), 1.0f);
        int forced = points[gid].w == 2.0f ? -1 : int(points[gid].w) - 3;
        MandelDeltaOrbitResult value = mandelDeltaOrbit(scaled, cfg, forced, 0);
        samples[gid] = float4(value.radius, value.iterations,
            max(1.0e-7f, 5.0e-7f * length(scaled)), float(value.escaped));
        return;
    }
#endif
    if (points[gid].w == 1.0f) {
        samples[gid] = float4(mandelbulberNormalDistance(p, cfg), 0, 0, 0);
        return;
    }
    samples[gid] = float4(normalAt(p, cfg),
        mandelbulberMarchThreshold(p, cfg) * cfg.vset_values[114]);
}
"#;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 6,
        "usage: mandel_normal_probe scene.fract mandel-root width height points.json NEW-output.json"
    );
    let out = Path::new(&args[5]);
    ensure!(!out.exists(), "refusing to overwrite probe output");
    let render_args = scene::parse_render_args(&[
        args[0].clone(),
        "--mandelbulber-root".into(),
        args[1].clone(),
        "--width".into(),
        args[2].clone(),
        "--height".into(),
        args[3].clone(),
        "--samples".into(),
        "1".into(),
    ])?;
    let loaded = scene::load_scene_config(&render_args)?;
    let mut cfg = loaded.config;
    scene::apply_camera_args(&mut cfg, &render_args);
    scene::apply_optimization_args(&mut cfg, &render_args);
    let points: Vec<[f32; 4]> = serde_json::from_slice(&fs::read(&args[4])?)?;
    ensure!(
        !points.is_empty() && points.len() <= 300_000,
        "invalid point count"
    );
    ensure!(
        points.iter().flatten().all(|v| v.is_finite()),
        "non-finite point"
    );
    ensure!(
        points
            .iter()
            .all(|v| v[3] >= -6.0 && v[3] <= 4099.0 && v[3].fract() == 0.0),
        "unknown point mode"
    );
    ensure!(
        points.iter().filter(|p| p[3] <= -5.0).all(|p| {
            let length = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            (0.99..=1.01).contains(&length)
        }),
        "direct march probes require near-unit ray directions"
    );
    let source = String::from_utf8(
        loaded
            .runtime_metal_source
            .context("missing Mandel shader")?,
    )?;
    let mut shader = retain_metal_kernels(&source, &[])?;
    if points.iter().any(|v| v[3] >= 2.0) {
        ensure!(
            shader.contains("MandelDeltaOrbitResult")
                && shader.contains("    float delta = max(1.0e-7f, 5.0e-7f * length(scaled));"),
            "orbit probe requires formula 85"
        );
        shader.insert_str(0, "#define FPT_NORMAL_ORBIT_PROBE 1\n");
    }
    let delta_scale: f32 = std::env::var("FPT_NORMAL_PROBE_DELTA_SCALE")
        .ok()
        .map(|v| v.parse())
        .transpose()?
        .unwrap_or(1.0);
    ensure!(
        delta_scale.is_finite() && delta_scale > 0.0 && delta_scale <= 16.0,
        "invalid diagnostic delta scale"
    );
    if delta_scale != 1.0 {
        let marker = "    float delta = max(1.0e-7f, 5.0e-7f * length(scaled));";
        ensure!(
            shader.matches(marker).count() == 1,
            "delta probe requires formula 85 specialization"
        );
        shader = shader.replace(
            marker,
            &format!(
                "    float delta = {:.9e}f * max(1.0e-7f, 5.0e-7f * length(scaled));",
                delta_scale
            ),
        );
    }
    let analytic = match std::env::var("FPT_NORMAL_PROBE_ANALYTIC85").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("0") => false,
        Ok("1") => true,
        _ => anyhow::bail!("FPT_NORMAL_PROBE_ANALYTIC85 must be 0 or 1"),
    };
    if analytic {
        ensure!(
            delta_scale == 1.0,
            "analytic and delta-scale probes are mutually exclusive"
        );
        shader = analytic85::specialize(&shader, cfg.vset_values[101], cfg.vset_values[104])?;
    }
    let jitter_seed: Option<u32> = std::env::var("FPT_NORMAL_PROBE_STEP_SEED")
        .ok()
        .map(|v| v.parse())
        .transpose()?;
    let dynamic_threshold = match std::env::var("FPT_NORMAL_PROBE_DYNAMIC_THRESHOLD").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("0") => false,
        Ok("1") => true,
        _ => anyhow::bail!("FPT_NORMAL_PROBE_DYNAMIC_THRESHOLD must be 0 or 1"),
    };
    if jitter_seed.is_some() || dynamic_threshold {
        ensure!(
            std::env::var_os("FPT_NORMAL_PROBE_LIBRARY").is_none(),
            "march controls require source compilation, not an arbitrary offline library"
        );
        shader = march_controls::specialize(&shader, jitter_seed, dynamic_threshold)?;
    }
    let start = shader
        .find("static MandelbulberMarchResult marchMandelbulber(")
        .context("missing production primary marcher")?;
    let end = start
        + shader[start..]
            .find("    if (!found) return MandelbulberMarchResult{position, false};")
            .context("production primary exit changed")?;
    let unrefined =
        shader[start..end].replacen("marchMandelbulber(", "probeMarchBeforeRefinement(", 1)
            + "    return MandelbulberMarchResult{position, found};\n}\n";
    shader.push_str(&unrefined);
    shader.push_str(KERNEL);
    if let Some(path) = std::env::var_os("FPT_NORMAL_PROBE_SOURCE") {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        std::io::Write::write_all(&mut file, shader.as_bytes())?;
    }
    let offline_library = std::env::var_os("FPT_NORMAL_PROBE_LIBRARY");
    let library_hash = offline_library
        .as_ref()
        .map(|path| fs::read(path).map(|bytes| format!("{:x}", Sha256::digest(bytes))))
        .transpose()?;
    let library_path = CString::new(
        offline_library
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unused.metallib".into()),
    )?;
    let mut output = vec![FptMandelbulberFieldSample::default(); points.len()];
    let mut error = [0i8; 4096];
    let status = unsafe {
        fpt_mandelbulber_sample_field(
            library_path.as_ptr(),
            if offline_library.is_some() {
                std::ptr::null()
            } else {
                shader.as_ptr().cast()
            },
            if offline_library.is_some() {
                0
            } else {
                shader.len()
            },
            &cfg,
            points.as_ptr().cast(),
            points.len(),
            output.as_mut_ptr(),
            error.as_mut_ptr(),
            error.len(),
        )
    };
    ensure!(
        status == 0,
        "{}",
        unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy()
    );
    let values: Vec<_> = output
        .iter()
        .map(|v| [v.distance, v.radius, v.derivative, v.iterations])
        .collect();
    ensure!(
        values.iter().flatten().all(|v| v.is_finite()),
        "non-finite normal result"
    );
    fs::write(
        out,
        serde_json::to_vec(&json!({
            "scene_sha256": format!("{:x}", Sha256::digest(fs::read(&args[0])?)),
            "points_sha256": format!("{:x}", Sha256::digest(fs::read(&args[4])?)),
            "shader_sha256": format!("{:x}", Sha256::digest(shader.as_bytes())),
            "binary_sha256": format!("{:x}", Sha256::digest(fs::read(std::env::current_exe()?)?)),
            "diagnostic_delta_scale": delta_scale,
            "diagnostic_analytic85": analytic,
            "diagnostic_step_seed": jitter_seed,
            "diagnostic_dynamic_threshold": dynamic_threshold,
            "offline_library_sha256": library_hash,
            "camera_world": cfg.camera_position,
            "world_scale": cfg.set_values[0],
            "scope": "At float32 world points: w=0 returns normalAt XYZ and epsilon; w=1 normal-mode distance; w=-1 primary distance and threshold; w=-2 raw field; w=-3 camera ray from normalized XY; w=-4 march position and hit flag from normalized XY; w=-5 direct-direction refined hit; w=-6 direct-direction unrefined hit. Formula85 only: w=2 unforced orbit, w>=3 forces w-3 iterations. Separate diagnostic entry point, not a production compiler-parity guarantee.",
            "samples": values,
        }))?,
    )?;
    Ok(())
}
