//! Opt-in, headless scene-578 experiment using the existing FPT path integrator.
use anyhow::{Context, Result, ensure};
use fpt_metal::{ffi::*, mandelbulber::compiler::retain_metal_kernels, scene};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;

#[path = "precision/beauty_probe.rs"]
mod beauty_probe;
#[path = "precision/hybrid_path.rs"]
mod hybrid_path;

fn main() -> Result<()> {
    let mut raw: Vec<_> = std::env::args().skip(1).collect();
    let mut control = "two-term".to_owned();
    if let Some(i) = raw.iter().position(|v| v == "--precision-control") {
        ensure!(i + 1 < raw.len(), "missing precision control");
        control = raw.remove(i + 1);
        raw.remove(i);
    }
    ensure!(
        ["two-term", "float", "safe-float"].contains(&control.as_str()),
        "unknown precision control"
    );
    for flag in raw.iter().filter(|v| v.starts_with("--")) {
        ensure!(
            [
                "--mandelbulber-root",
                "--width",
                "--height",
                "--samples",
                "--mandel-appearance",
                "--sdf-bounce-cap",
                "--out"
            ]
            .contains(&flag.as_str()),
            "unsupported precision option: {flag}"
        );
    }
    let mut args = scene::parse_render_args(&raw)?;
    args.width = Some(args.width.unwrap_or(160));
    args.height = Some(args.height.unwrap_or(120));
    args.samples = Some(args.samples.unwrap_or(1));
    args.sdf_bounce_cap = Some(args.sdf_bounce_cap.unwrap_or(1));
    ensure!(!args.out_dir.exists(), "use a new output directory");
    ensure!(
        std::env::var_os("FPT_MANDEL_TILED_DISPATCH").is_some(),
        "set FPT_MANDEL_TILED_DISPATCH=1"
    );
    let rows: u32 = std::env::var("FPT_MANDEL_TILE_ROWS")
        .context("set FPT_MANDEL_TILE_ROWS=1")?
        .parse()?;
    ensure!(
        (1..=4).contains(&rows),
        "precision tiles must use 1..4 rows"
    );
    let helper = hybrid_path::helper(
        &args.scene_path,
        args.mandelbulber_root
            .as_deref()
            .context("--mandelbulber-root required")?,
    )?;
    let mut loaded = scene::load_scene_config(&args)?;
    let cfg = &mut loaded.config;
    scene::apply_optimization_args(cfg, &args);
    ensure!(
        cfg.width > 0
            && cfg.width <= 320
            && cfg.height > 0
            && cfg.height <= 240
            && cfg.width * 3 == cfg.height * 4,
        "require authored 4:3 aspect, at most 320x240"
    );
    ensure!(
        cfg.samples <= 16 && cfg.camera_dof == 0.0 && cfg.set_values[0] == 1024.0,
        "require <=16 SPP, no DOF and unchanged world scale"
    );
    ensure!(
        cfg.mandel_appearance_mode == 1 || cfg.mandel_appearance_mode == 2,
        "require geometry or authored-path appearance"
    );
    cfg.sdf_accumulation_mode = SDF_ACCUMULATION_CHUNKED;
    cfg.sdf_chunk_samples = 1;
    cfg.sdf_runtime_source_bytecode = 0;
    cfg.focus_distance = 1.0; // DOF is off; do not run an unrelated float focus probe.
    let source = String::from_utf8(
        loaded
            .runtime_metal_source
            .context("missing generated source")?,
    )?;
    let source = if control == "two-term" {
        hybrid_path::specialize(&source, &helper)?
    } else {
        source
    };
    let shader = retain_metal_kernels(
        &source,
        &[
            "accumulate_chunk_kernel",
            "accumulate_chunk_tile_kernel",
            "present_kernel",
        ],
    )?;
    fs::create_dir_all(&args.out_dir)?;
    let elapsed = if control == "float" {
        beauty_probe::render(&shader, cfg, &args.out_dir)?
    } else {
        beauty_probe::render_with_math(&shader, cfg, &args.out_dir, true)?
    };
    let linear = fs::read(args.out_dir.join("linear.f32"))?;
    let invalid = linear
        .chunks_exact(16)
        .filter(|p| f32::from_le_bytes(p[..4].try_into().unwrap()) > 8.01)
        .count();
    let report = json!({
        "scope": "Scene-578 opt-in headless integration experiment; not production support or a timing gate",
        "control": control, "safe_math": control != "float", "native_runtime_required": false,
        "width": cfg.width, "height": cfg.height, "samples": cfg.samples, "bounce_cap": cfg.sdf_bounce_cap,
        "appearance": if args.mandel_authored_path { "authored-path" } else { "geometry" },
        "authored_ambient": loaded.mandel_ambient, "authored_auxiliary": loaded.mandel_auxiliary.as_ref().map(|v| &v.unsupported),
        "tile_rows": rows, "chunk_samples": 1, "sampled_march": true,
        "invalid_transport_pixels": invalid, "healthy": invalid == 0,
        "render_wall_ms": elapsed,
        "scene_sha256": hybrid_path::SCENE_SHA,
        "helper_sha256": format!("{:x}", Sha256::digest(helper.as_bytes())),
        "shader_sha256": format!("{:x}", Sha256::digest(shader.as_bytes())),
        "image_sha256": format!("{:x}", Sha256::digest(fs::read(args.out_dir.join("render.png"))?)),
        "linear_sha256": format!("{:x}", Sha256::digest(&linear)),
        "binary_sha256": format!("{:x}", Sha256::digest(fs::read(std::env::current_exe()?)?)),
        "arguments": raw,
    });
    fs::write(
        args.out_dir.join("summary.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    ensure!(
        invalid == 0,
        "precision transport failed at {invalid} pixels; see summary.json"
    );
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
