//! Evaluate production normals at supplied world points without changing traversal.
use anyhow::{Context, Result, ensure};
use fpt_metal::{ffi::*, mandelbulber::compiler::retain_metal_kernels, scene};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{ffi::CStr, fs, path::Path};

#[path = "precision/analytic85.rs"]
mod analytic85;

const KERNEL: &str = r#"
kernel void mandelbulber_field_sample_kernel(
    device const float4 *points [[buffer(0)]], device float4 *samples [[buffer(1)]],
    constant FptRenderConfig &cfg [[buffer(2)]], uint gid [[thread_position_in_grid]]) {
    float3 p = points[gid].xyz;
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
            .all(|v| v[3] >= 0.0 && v[3] <= 4099.0 && v[3].fract() == 0.0),
        "unknown point mode"
    );
    let source = String::from_utf8(
        loaded
            .runtime_metal_source
            .context("missing Mandel shader")?,
    )?;
    let mut shader = retain_metal_kernels(&source, &[])?;
    if let Some(path) = std::env::var_os("FPT_NORMAL_PROBE_SOURCE") {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        std::io::Write::write_all(&mut file, shader.as_bytes())?;
    }
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
    shader.push_str(KERNEL);
    let mut output = vec![FptMandelbulberFieldSample::default(); points.len()];
    let mut error = [0i8; 4096];
    let status = unsafe {
        fpt_mandelbulber_sample_field(
            c"unused.metallib".as_ptr(),
            shader.as_ptr().cast(),
            shader.len(),
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
            "scope": "At float32 world points: w=0 returns normalAt XYZ and epsilon; w=1 returns normal-mode distance in X; w=2 returns unforced orbit; integer w>=3 forces w-3 iterations. Orbit output is radius, completed iterations, inner delta, escaped. Separate diagnostic entry point, not a production compiler-parity guarantee.",
            "samples": values,
        }))?,
    )?;
    Ok(())
}
