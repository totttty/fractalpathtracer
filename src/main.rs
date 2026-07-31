mod ffi;
mod scene;
mod tools;

use anyhow::{Context, Result, anyhow, bail};
use ffi::*;
use scene::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::ffi::{CStr, CString};
use std::fs;
use std::path::{Path, PathBuf};

const METALLIB_BYTES: &[u8] = include_bytes!(env!("FPT_METALLIB_PATH"));
const UPSTREAM_SHA: &str = "12c242d25e28c21b1cfa7842f554458996263891";

fn usage() {
    eprintln!(
        "Usage:\n\
  fpt-metal render <scene.json> --out <dir> [--renderer sdf|voxel] [--voxel-resolution N] [--voxel-normal face|smooth] [--voxel-storage dense|sparse-bricks] [--voxel-surface-band N] [--fpt-root <dir>] [--preview] [--glass-mode analytic|pathtrace] [--sdf-accumulation auto|per-sample|batch|chunked] [--sdf-normal-mode auto|central|tetra|program-gradient] [--sdf-chunk-samples N] [--width N] [--height N] [--samples N]\n\
  fpt-metal diagnostic <scene.json> --out <dir> --mode <mode> [--fpt-root <dir>] [--width N] [--height N]\n\
  fpt-metal preview <scene.json> [--renderer sdf|voxel] [--voxel-resolution N] [--voxel-normal face|smooth] [--voxel-storage dense|sparse-bricks] [--fpt-root <dir>] [--pathtrace] [--sdf-profile] [--width N] [--height N] [--samples N]\n\
  fpt-metal compare <baseline.png> <candidate.png> --report <report.json> [--strict]\n\
  fpt-metal contact-sheet <out.png> <images...>\n\
  fpt-metal report-index <report-dir>\n\
  fpt-metal check-parity-reports <report-dirs...>\n\
  fpt-metal readme-comparison <originals-dir> <generated-dir> <out.png>\n\
  fpt-metal capability-fixtures <fpt-root> <out-dir>\n\
  fpt-metal path-cost-summary <out-dir>\n\
  fpt-metal bounce-summary <out-dir> <bounce...>\n\
  fpt-metal optimization-summary <out-dir> <max-mae> <max-rmse> <min-ssim> <min-lf-ssim> <runs> [expected-scenes]\n\
  fpt-metal voxel-summary <report.json> <sdf.render.json> <voxel.render.json>...\n\
  fpt-metal list-scenes\n\
  fpt-metal clean-reports"
    );
}

fn list_scenes() {
    eprintln!(
        "Supported presets:\n  Cornell_Box\n  Glass_Ball\n  Ball_Fractal\n  Cage_Fractal\n  IFS_Fractal\n  Mandelbox_Fractal\n  Menger_Sponge\n  Tower_Fractal\n  Tree_Fractal\n  Any JSON scene containing a typed sdf_program\n  Gradient_Example.fpt (compatibility compiler)"
    );
}

fn c_path(path: &Path) -> Result<CString> {
    CString::new(path.to_string_lossy().as_bytes()).context("path contains a null byte")
}

fn bridge_error(buffer: &[i8]) -> String {
    unsafe {
        CStr::from_ptr(buffer.as_ptr())
            .to_string_lossy()
            .into_owned()
    }
}

fn default_metallib_path() -> Result<PathBuf> {
    let digest = format!("{:x}", Sha256::digest(METALLIB_BYTES));
    let directory = std::env::temp_dir().join("fpt-metal");
    let path = directory.join(format!("Shaders-{}.metallib", &digest[..16]));
    if !path.exists() {
        fs::create_dir_all(&directory)?;
        fs::write(&path, METALLIB_BYTES)?;
    }
    Ok(path)
}

fn metallib_path(args: &RenderArgs) -> Result<PathBuf> {
    args.metallib
        .clone()
        .map(Ok)
        .unwrap_or_else(default_metallib_path)
}

fn output_path(args: &RenderArgs, name: &str) -> PathBuf {
    args.out_dir.join(name)
}

fn effective_accumulation(config: &FptRenderConfig) -> &'static str {
    if config.preview != 0 {
        "preview"
    } else if config.sdf_accumulation_mode == SDF_ACCUMULATION_BATCH {
        "batch"
    } else if config.sdf_accumulation_mode == SDF_ACCUMULATION_PER_SAMPLE {
        "per-sample"
    } else if config.sdf_accumulation_mode == SDF_ACCUMULATION_CHUNKED
        || config.samples > 16
        || config.sdf_id == SDF_CAGE_FRACTAL
    {
        "chunked"
    } else {
        "batch"
    }
}

fn write_render_metadata(
    output: &Path,
    scene: &Path,
    config: &FptRenderConfig,
    build_ms: f64,
    elapsed_ms: f64,
    voxel_memory_bytes: u64,
    voxel_active_bricks: u32,
) -> Result<()> {
    let scene_data = fs::read(scene)?;
    let scene_sha = format!("{:x}", Sha256::digest(scene_data));
    let mut gpu = [0_i8; 256];
    unsafe { fpt_metal_device_name(gpu.as_mut_ptr(), gpu.len()) };
    let gpu = bridge_error(&gpu);
    let accumulation = match config.sdf_accumulation_mode {
        SDF_ACCUMULATION_PER_SAMPLE => "per-sample",
        SDF_ACCUMULATION_BATCH => "batch",
        SDF_ACCUMULATION_CHUNKED => "chunked",
        _ => "auto",
    };
    let throughput = if elapsed_ms > 0.0 {
        config.width as f64 * config.height as f64 * config.samples.max(1) as f64
            / elapsed_ms
            / 1000.0
    } else {
        0.0
    };
    let metadata = json!({
        "scene": scene,
        "scene_sha256": scene_sha,
        "upstream_sha": UPSTREAM_SHA,
        "metal_device": gpu,
        "output": output,
        "backend": if config.renderer_backend == RENDERER_VOXEL { "voxel_pathtrace" } else { "sdf_pathtrace" },
        "geometry": if config.renderer_backend == RENDERER_VOXEL { "voxel_field" } else { "procedural_sdf" },
        "renderer": if config.renderer_backend == RENDERER_VOXEL { "voxel" } else { "sdf" },
        "width": config.width,
        "height": config.height,
        "samples": config.samples,
        "preview": config.preview != 0,
        "glass_mode": if config.glass_mode == GlassMode::Analytic as u32 { "analytic" } else { "pathtrace" },
        "sdf_accumulation": accumulation,
        "effective_sdf_accumulation": effective_accumulation(config),
        "sdf_chunk_samples": config.sdf_chunk_samples,
        "sdf_bounce_cap": config.sdf_bounce_cap,
        "sdf_russian_roulette": config.sdf_russian_roulette != 0,
        "sdf_rr_start": config.sdf_rr_start,
        "sdf_rr_min_prob": config.sdf_rr_min_prob,
        "sdf_normal_mode": match config.sdf_normal_mode {
            value if value == SdfNormalMode::Tetra as u32 => "tetra",
            value if value == SdfNormalMode::ProgramGradient as u32 => "program-gradient",
            value if value == SdfNormalMode::Central as u32 => "central",
            _ => "auto",
        },
        "elapsed_ms": elapsed_ms,
        "voxel_build_ms": build_ms,
        "voxel_resolution": config.voxel_resolution,
        "voxel_normal": if config.voxel_normal_mode == VOXEL_NORMAL_SMOOTH { "smooth" } else { "face" },
        "voxel_storage": if config.voxel_storage == VOXEL_STORAGE_SPARSE_BRICKS { "sparse-bricks" } else { "dense" },
        "voxel_memory_bytes": voxel_memory_bytes,
        "voxel_active_bricks": voxel_active_bricks,
        "megapixel_samples_per_second": throughput,
    });
    let metadata_path = PathBuf::from(format!("{}.render.json", output.display()));
    fs::write(
        metadata_path,
        format!("{}\n", serde_json::to_string_pretty(&metadata)?),
    )?;
    Ok(())
}

fn render(args: &RenderArgs) -> Result<()> {
    let mut loaded = load_scene_config(args)?;
    loaded.config.sdf_accumulation_mode = args.sdf_accumulation_mode as u32;
    apply_optimization_args(&mut loaded.config, args);
    fs::create_dir_all(&args.out_dir)?;
    let output = output_path(args, &loaded.output_name);
    let metallib = metallib_path(args)?;
    let metallib_c = c_path(&metallib)?;
    let output_c = c_path(&output)?;
    let mut elapsed_ms = 0.0;
    let mut build_ms = 0.0;
    let mut voxel_memory_bytes = 0_u64;
    let mut voxel_active_bricks = 0_u32;
    let mut error = [0_i8; 4096];
    let status = unsafe {
        fpt_metal_render(
            metallib_c.as_ptr(),
            output_c.as_ptr(),
            &loaded.config,
            &mut build_ms,
            &mut elapsed_ms,
            &mut voxel_memory_bytes,
            &mut voxel_active_bricks,
            error.as_mut_ptr(),
            error.len(),
        )
    };
    if status != 0 {
        bail!("{}", bridge_error(&error));
    }
    write_render_metadata(
        &output,
        &args.scene_path,
        &loaded.config,
        build_ms,
        elapsed_ms,
        voxel_memory_bytes,
        voxel_active_bricks,
    )?;
    eprintln!(
        "rendered {} -> {} (build {build_ms:.2} ms, render {elapsed_ms:.2} ms)",
        args.scene_path.display(),
        output.display()
    );
    Ok(())
}

fn diagnostic(args: &RenderArgs) -> Result<()> {
    let mut loaded = load_scene_config(args)?;
    loaded.config.preview = 1;
    loaded.config.samples = args.samples.unwrap_or(1);
    apply_optimization_args(&mut loaded.config, args);
    fs::create_dir_all(&args.out_dir)?;
    let output = output_path(args, &loaded.output_name);
    let metallib_c = c_path(&metallib_path(args)?)?;
    let output_c = c_path(&output)?;
    let diagnostic = FptDiagnosticConfig {
        mode: args.diagnostic_mode as u32,
        _pad0: args.sdf_bounce_index,
        max_distance: loaded.config.render[4],
        normal_mix: 1.0,
    };
    let mut elapsed_ms = 0.0;
    let mut error = [0_i8; 4096];
    let status = unsafe {
        fpt_metal_diagnostic_render(
            metallib_c.as_ptr(),
            output_c.as_ptr(),
            &loaded.config,
            &diagnostic,
            &mut elapsed_ms,
            error.as_mut_ptr(),
            error.len(),
        )
    };
    if status != 0 {
        bail!("{}", bridge_error(&error));
    }
    write_render_metadata(
        &output,
        &args.scene_path,
        &loaded.config,
        0.0,
        elapsed_ms,
        0,
        0,
    )?;
    eprintln!(
        "rendered {} diagnostic {} -> {} ({elapsed_ms:.2} ms)",
        if loaded.config.renderer_backend == RENDERER_VOXEL {
            "voxel"
        } else {
            "SDF"
        },
        args.scene_path.display(),
        output.display()
    );
    Ok(())
}

fn preview(args: &RenderArgs) -> Result<()> {
    let mut loaded = load_scene_config(args)?;
    loaded.config.preview = u32::from(!args.live_pathtrace);
    loaded.config.samples = args.samples.unwrap_or(512);
    apply_optimization_args(&mut loaded.config, args);
    if args.width.is_none() {
        loaded.config.width = loaded.config.width.min(1280);
    }
    if args.height.is_none() {
        loaded.config.height = loaded.config.height.min(800);
    }
    let (scenes, selected) = preview_scene_list(args, &loaded.config)?;
    let metallib_c = c_path(&metallib_path(args)?)?;
    let mut error = [0_i8; 4096];
    let status = unsafe {
        fpt_metal_preview(
            metallib_c.as_ptr(),
            &loaded.config,
            scenes.as_ptr(),
            scenes.len() as u32,
            selected,
            error.as_mut_ptr(),
            error.len(),
        )
    };
    if status != 0 {
        bail!("{}", bridge_error(&error));
    }
    Ok(())
}

fn compare(args: &[String]) -> Result<()> {
    let baseline = args
        .first()
        .ok_or_else(|| anyhow!("missing baseline PNG"))?;
    let candidate = args
        .get(1)
        .ok_or_else(|| anyhow!("missing candidate PNG"))?;
    let mut report = PathBuf::from("compare-report.json");
    let mut strict = false;
    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "--report" => {
                index += 1;
                report = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--report requires a path"))?
                    .into();
            }
            "--strict" => strict = true,
            value => bail!("unknown argument: {value}"),
        }
        index += 1;
    }
    if let Some(parent) = report
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let baseline_c = c_path(Path::new(baseline))?;
    let candidate_c = c_path(Path::new(candidate))?;
    let report_c = c_path(&report)?;
    let mut error = [0_i8; 4096];
    let status = unsafe {
        fpt_compare_images(
            baseline_c.as_ptr(),
            candidate_c.as_ptr(),
            report_c.as_ptr(),
            error.as_mut_ptr(),
            error.len(),
        )
    };
    if status != 0 {
        bail!("{}", bridge_error(&error));
    }
    eprintln!("wrote {}", report.display());
    if strict {
        let value: Value = serde_json::from_slice(&fs::read(&report)?)?;
        if value.get("strict_gate").and_then(Value::as_bool) != Some(true) {
            bail!("strict comparison failed");
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        usage();
        bail!("missing command")
    };
    let tail = &args[1..];
    match command {
        "render" => render(&parse_render_args(tail)?),
        "diagnostic" => diagnostic(&parse_render_args(tail)?),
        "preview" => preview(&parse_render_args(tail)?),
        "compare" => compare(tail),
        "contact-sheet" => tools::contact_sheet_command(tail),
        "report-index" => tools::report_index_command(tail),
        "check-parity-reports" => tools::check_parity_reports_command(tail),
        "readme-comparison" => tools::readme_comparison_command(tail),
        "capability-fixtures" => tools::capability_fixtures_command(tail),
        "path-cost-summary" => tools::path_cost_summary_command(tail),
        "bounce-summary" => tools::bounce_summary_command(tail),
        "optimization-summary" => tools::optimization_summary_command(tail),
        "voxel-summary" => tools::voxel_summary_command(tail),
        "list-scenes" => {
            list_scenes();
            Ok(())
        }
        "clean-reports" => {
            if Path::new("reports").exists() {
                fs::remove_dir_all("reports")?;
                eprintln!("removed reports");
            } else {
                eprintln!("reports already clean");
            }
            Ok(())
        }
        _ => {
            usage();
            Err(anyhow!("unknown command: {command}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_accumulation_reports_the_dispatched_kernel() {
        let mut config = FptRenderConfig {
            samples: 1,
            sdf_id: SDF_TOWER_FRACTAL,
            ..Default::default()
        };
        assert_eq!(effective_accumulation(&config), "batch");

        config.sdf_id = SDF_CAGE_FRACTAL;
        assert_eq!(effective_accumulation(&config), "chunked");

        config.sdf_id = SDF_TOWER_FRACTAL;
        config.samples = 17;
        assert_eq!(effective_accumulation(&config), "chunked");
    }
}
