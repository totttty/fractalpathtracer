//! Isolated derivative and ray-march diagnostics; production defaults unchanged.
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
#[path = "precision/ifs_path.rs"]
mod ifs_path;
#[path = "precision/march_controls.rs"]
mod march_controls;
#[path = "precision/parametric_march.rs"]
mod parametric_march;
#[path = "precision/ray_exit_probe.rs"]
mod ray_exit_probe;

fn main() -> Result<()> {
    let mut raw: Vec<_> = std::env::args().skip(1).collect();
    let beauty = raw.first().is_some_and(|v| v == "beauty");
    if beauty {
        raw.remove(0);
    }
    let safe_math = match std::env::var("FPT_BEAUTY_PROBE_SAFE_MATH").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("0") => false,
        Ok("1") => true,
        _ => anyhow::bail!("FPT_BEAUTY_PROBE_SAFE_MATH must be 0 or 1"),
    };
    ensure!(
        !safe_math || beauty,
        "safe math control requires beauty mode"
    );
    let args = scene::parse_render_args(&raw)?;
    let precision_path = std::env::var_os("FPT_IFS_PRECISION_SOURCE");
    let precision_headlight = std::env::var_os("FPT_IFS_PRECISION_HEADLIGHT").is_some();
    ensure!(
        !precision_headlight || precision_path.is_some(),
        "headlight requires precision path"
    );
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
    if precision_path.is_some() {
        for flag in raw.iter().filter(|v| v.starts_with("--")) {
            ensure!(
                [
                    "--mandelbulber-root",
                    "--width",
                    "--height",
                    "--samples",
                    "--sdf-accumulation",
                    "--sdf-chunk-samples",
                    "--mandel-appearance",
                    "--sdf-bounce-cap",
                    "--out"
                ]
                .contains(&flag.as_str()),
                "unsupported precision override: {flag}"
            );
        }
        ensure!(beauty, "IFS precision is a beauty-only experiment");
        ensure!(
            format!("{:x}", Sha256::digest(fs::read(&args.scene_path)?))
                == "170ca7e9d8e4f6e404de61df2c978532b738a024ad99c442b528bccf168ddf7f",
            "precision scene mismatch"
        );
        ensure!(
            loaded.config.camera_dof == 0.0 && loaded.config.mandel_appearance_mode == 2,
            "precision requires authored-path without DOF"
        );
        ensure!(
            loaded.config.set_values[0] == 1024.0,
            "precision world scale changed"
        );
        ensure!(
            loaded.config.width <= 320 && loaded.config.height <= 240,
            "precision image limited to 320x240"
        );
        ensure!(
            (loaded.config.width <= 160 && loaded.config.height <= 96)
                || std::env::var_os("FPT_MANDEL_TILED_DISPATCH").is_some(),
            "larger precision images require tiled dispatch"
        );
        loaded.config.sdf_chunk_samples = 1;
        loaded.config.mandel_appearance[0] = 0.0;
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
        vec![
            "accumulate_chunk_kernel",
            "accumulate_chunk_tile_kernel",
            "present_kernel",
        ]
    } else {
        vec!["sdf_diagnostic_kernel", "sdf_structural_diagnostic_kernel"]
    };
    if beauty && cfg.focus_distance <= 0.0 {
        kernels.push("estimate_focus_distance_kernel");
    }
    // Keep historical experiment baselines deterministic; ordinary rendering
    // uses the promoted sampled path. Explicit controls are applied below.
    let source = march_controls::deterministic_path(&source)?;
    let mut shader = retain_metal_kernels(&source, &kernels)?;
    let analytic = match std::env::var("FPT_NORMAL_PROBE_ANALYTIC85").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("0") => false,
        Ok("1") => true,
        _ => anyhow::bail!("FPT_NORMAL_PROBE_ANALYTIC85 must be 0 or 1"),
    };
    if analytic {
        shader = analytic85::specialize(&shader, cfg.vset_values[101], cfg.vset_values[104])?;
    }
    let parametric = match std::env::var("FPT_MARCH_PROBE_PARAMETRIC").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("0") => false,
        Ok("1") => true,
        _ => anyhow::bail!("FPT_MARCH_PROBE_PARAMETRIC must be 0 or 1"),
    };
    if parametric {
        ensure!(!analytic, "keep numerical experiments isolated");
        ensure!(
            std::env::var_os("FPT_DERIVATIVE_RAY_PIXELS").is_none(),
            "exit probe expects the unchanged primary loop"
        );
        shader = parametric_march::specialize(&shader)?;
    }
    if let Some(path) = &precision_path {
        ensure!(
            !analytic && !parametric && std::env::var_os("FPT_DERIVATIVE_RAY_PIXELS").is_none(),
            "keep precision experiments isolated"
        );
        shader = ifs_path::specialize(&shader, &fs::read_to_string(path)?, precision_headlight)?;
    }
    let step_seed: Option<u32> = std::env::var("FPT_BEAUTY_PROBE_STEP_SEED")
        .ok()
        .map(|v| v.parse())
        .transpose()?;
    let sampling_seed: Option<u32> = std::env::var("FPT_BEAUTY_PROBE_SAMPLING_SEED")
        .ok()
        .map(|v| v.parse())
        .transpose()?;
    ensure!(
        step_seed.is_none() || sampling_seed.is_none(),
        "choose fixed or dimensioned jitter, not both"
    );
    let dynamic_threshold = match std::env::var("FPT_BEAUTY_PROBE_DYNAMIC_THRESHOLD").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("0") => false,
        Ok("1") => true,
        _ => anyhow::bail!("FPT_BEAUTY_PROBE_DYNAMIC_THRESHOLD must be 0 or 1"),
    };
    if step_seed.is_some() || sampling_seed.is_some() || dynamic_threshold {
        ensure!(
            beauty && !analytic && !parametric && precision_path.is_none(),
            "march controls require isolated ordinary beauty mode"
        );
        shader = if let Some(seed) = sampling_seed {
            march_controls::dimensioned(&shader, seed, dynamic_threshold)?
        } else {
            march_controls::specialize(&shader, step_seed, dynamic_threshold)?
        };
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
        elapsed = if precision_path.is_some() || safe_math {
            beauty_probe::render_with_math(&shader, cfg, &args.out_dir, true)?
        } else {
            beauty_probe::render(&shader, cfg, &args.out_dir)?
        };
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
        "scope": "Isolated experiment, not a production timing gate. Precision uses safe math and excludes AO/fog/clouds; other beauty probes use the recorded math control.",
        "safe_math": precision_path.is_some() || safe_math,
        "ifs_precision": precision_path.is_some(), "ifs_precision_headlight": precision_headlight,
        "precision_helper_sha256": precision_path.as_ref().map(|p| fs::read(p).map(|bytes| format!("{:x}",Sha256::digest(bytes)))).transpose()?,
        "beauty": beauty,
        "analytic85": analytic, "width": cfg.width, "height": cfg.height,
        "parametric_primary": parametric,
        "step_jitter_seed_per_march": step_seed,
        "dimensioned_step_sampling_seed": sampling_seed,
        "production_step_sampling_disabled_for_control": true,
        "dynamic_refinement_threshold": dynamic_threshold,
        "world_scale": cfg.set_values[0], "samples": cfg.samples,
        "diagnostic_mode": diagnostic.mode,
        "diagnostic_gpu_ms": if beauty { None } else { Some(elapsed) },
        "beauty_render_wall_ms": if beauty { Some(elapsed) } else { None },
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
