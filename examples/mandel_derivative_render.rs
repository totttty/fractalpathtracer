//! Isolated formula-85 diagnostic images; production code and defaults unchanged.
use anyhow::{Context, Result, ensure};
use fpt_metal::{ffi::*, mandelbulber::compiler::retain_metal_kernels, scene};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    ffi::{CStr, CString},
    fs,
};

#[path = "precision/analytic85.rs"]
mod analytic85;
#[path = "precision/beauty_probe.rs"]
mod beauty_probe;
#[path = "precision/ray_exit_probe.rs"]
mod ray_exit_probe;

fn main() -> Result<()> {
    let mut raw: Vec<_> = std::env::args().skip(1).collect();
    let beauty = raw.first().is_some_and(|v| v == "beauty");
    if beauty {
        raw.remove(0);
    }
    let args = scene::parse_render_args(&raw)?;
    ensure!(!args.out_dir.exists(), "use a new output directory");
    let mut loaded = scene::load_scene_config(&args)?;
    scene::apply_camera_args(&mut loaded.config, &args);
    loaded.config.preview = u32::from(!beauty);
    loaded.config.samples = args.samples.unwrap_or(1);
    if beauty {
        loaded.config.sdf_accumulation_mode = args.sdf_accumulation_mode as u32;
    }
    scene::apply_optimization_args(&mut loaded.config, &args);
    if beauty {
        loaded.config.sdf_runtime_source_bytecode = 0;
    }
    let cfg = &loaded.config;
    ensure!(
        cfg.renderer_backend != RENDERER_VOXEL,
        "continuous diagnostic only"
    );
    ensure!(
        cfg.width <= 512 && cfg.height <= 512,
        "diagnostic size limited to 512 per axis"
    );
    let source = String::from_utf8(
        loaded
            .runtime_metal_source
            .context("missing generated source")?,
    )?;
    let mut kernels = if beauty {
        vec!["accumulate_chunk_kernel", "present_kernel"]
    } else {
        vec!["sdf_diagnostic_kernel", "sdf_structural_diagnostic_kernel"]
    };
    if beauty && cfg.focus_distance <= 0.0 {
        kernels.push("estimate_focus_distance_kernel");
    }
    let mut shader = retain_metal_kernels(&source, &kernels)?;
    let analytic = match std::env::var("FPT_NORMAL_PROBE_ANALYTIC85").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("0") => false,
        Ok("1") => true,
        _ => anyhow::bail!("FPT_NORMAL_PROBE_ANALYTIC85 must be 0 or 1"),
    };
    if analytic {
        shader = analytic85::specialize(&shader, cfg.vset_values[101], cfg.vset_values[104])?;
    }
    fs::create_dir_all(&args.out_dir)?;
    if let Some(input) = std::env::var_os("FPT_DERIVATIVE_RAY_PIXELS") {
        ensure!(!beauty, "ray exit probe requires diagnostic mode");
        return ray_exit_probe::run(
            &shader,
            cfg,
            std::path::Path::new(&input),
            &args.out_dir,
            analytic,
        );
    }
    let image = CString::new(
        args.out_dir
            .join("render.png")
            .to_str()
            .context("UTF-8 output path required")?,
    )?;
    let hits = CString::new(
        args.out_dir
            .join("hits.bin")
            .to_str()
            .context("UTF-8 output path required")?,
    )?;
    let diagnostic = FptDiagnosticConfig {
        mode: args.diagnostic_mode as u32,
        max_distance: cfg.render[4],
        normal_mix: 1.0,
        ..Default::default()
    };
    let mut elapsed = 0.0;
    let mut error = [0i8; 4096];
    let status = if beauty {
        elapsed = beauty_probe::render(&shader, cfg, &args.out_dir)?;
        0
    } else {
        unsafe {
            fpt_metal_diagnostic_render(
                c"unused.metallib".as_ptr(),
                image.as_ptr(),
                hits.as_ptr(),
                shader.as_ptr().cast(),
                shader.len(),
                cfg,
                &diagnostic,
                &mut elapsed,
                error.as_mut_ptr(),
                error.len(),
            )
        }
    };
    if !beauty {
        ensure!(
            status == 0,
            "{}",
            unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy()
        );
        ensure!(
            fs::metadata(args.out_dir.join("hits.bin"))?.len()
                == u64::from(cfg.width) * u64::from(cfg.height) * 80,
            "invalid structural output size"
        );
    }
    let summary = json!({
        "scope": "Isolated derivative A/B using unchanged renderer kernels. Beauty uses offline default optimization/fast-math; diagnostics use dynamic compilation. Not a production timing gate.",
        "beauty": beauty,
        "analytic85": analytic, "width": cfg.width, "height": cfg.height,
        "world_scale": cfg.set_values[0], "samples": cfg.samples,
        "diagnostic_mode": diagnostic.mode, "diagnostic_gpu_ms": elapsed,
        "bounce_cap": cfg.sdf_bounce_cap,
        "scene_sha256": format!("{:x}", Sha256::digest(fs::read(&args.scene_path)?)),
        "shader_sha256": format!("{:x}", Sha256::digest(shader.as_bytes())),
        "binary_sha256": format!("{:x}", Sha256::digest(fs::read(std::env::current_exe()?)?)),
        "arguments": raw,
    });
    fs::write(
        args.out_dir.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    Ok(())
}
