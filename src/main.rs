#![recursion_limit = "512"]

use anyhow::{Context, Result, anyhow, bail, ensure};
use fpt_metal::ffi::*;
use fpt_metal::mandelbulber::{self, MandelbulberScene};
use fpt_metal::scene::*;
use fpt_metal::tools;
use fpt_metal::{
    Aabb, CoordinateSystem, FractalScene, VoxelCell, VoxelGrid, VoxelizationParameters,
    VoxelizationRequest, export_glb, voxelize,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const METALLIB_BYTES: &[u8] = include_bytes!(env!("FPT_METALLIB_PATH"));
const BUILTIN_METALLIB_BYTES: &[u8] = include_bytes!(env!("FPT_BUILTIN_METALLIB_PATH"));
const CAGE_METALLIB_BYTES: &[u8] = include_bytes!(env!("FPT_CAGE_METALLIB_PATH"));
const TOWER_METALLIB_BYTES: &[u8] = include_bytes!(env!("FPT_TOWER_METALLIB_PATH"));
const STITCH_METALLIB_BYTES: &[u8] = include_bytes!(env!("FPT_STITCH_METALLIB_PATH"));
const METALLIB_SHA: &str = env!("FPT_METALLIB_SHA");
const BUILTIN_METALLIB_SHA: &str = env!("FPT_BUILTIN_METALLIB_SHA");
const CAGE_METALLIB_SHA: &str = env!("FPT_CAGE_METALLIB_SHA");
const TOWER_METALLIB_SHA: &str = env!("FPT_TOWER_METALLIB_SHA");
const STITCH_METALLIB_SHA: &str = env!("FPT_STITCH_METALLIB_SHA");
const METAL_COMPILER_IDENTITY: &str = env!("FPT_METAL_COMPILER_IDENTITY");
const METAL_SOURCE_BYTES: &[u8] = include_bytes!("../shaders/Shaders.metal");
const UPSTREAM_SHA: &str = "12c242d25e28c21b1cfa7842f554458996263891";
const SDF_BACKEND_SELECTION_VERSION: u32 = 8;
const SDF_BACKEND_MARGIN: f64 = 0.15;
const SDF_GENERATED_SURFACE_REQUIRED_SPEEDUP: f64 = 1.10;
const SDF_BACKEND_PROBE_WIDTH: u32 = 1920;
const SDF_BACKEND_PROBE_SAMPLES: u32 = 64;
const SDF_BACKEND_PROBE_RUNS: usize = 3;
const SDF_BACKEND_ADAPTIVE_RUNS: usize = 4;
const SDF_BACKEND_NEAR_GATE_FRACTION: f64 = 0.05;
const SDF_BACKEND_PARITY_MAE: f64 = 1.0e-6;
const SDF_BACKEND_PARITY_OUTLIER_THRESHOLD: f64 = 1.0e-3;
const SDF_BACKEND_PARITY_MAX_OUTLIER_FRACTION: f64 = 2.0e-5;

#[cfg(test)]
static METAL_TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub(crate) fn metal_test_guard() -> std::sync::MutexGuard<'static, ()> {
    METAL_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Default)]
struct MetalRenderStats {
    build_ms: f64,
    elapsed_ms: f64,
    voxel_memory_bytes: u64,
    voxel_active_bricks: u32,
    voxel_active_cells: u64,
    voxel_rejected_bricks: u32,
    bound_grid: FptBoundGridStats,
    stitch_cache_status: u32,
    stitch_validation: FptStitchValidationStats,
    stitch_pipeline: FptStitchPipelineStats,
    sdf_profile: FptSdfProfileStats,
}

#[derive(Clone, Debug, Serialize)]
struct MandelBenchmarkImageHealth {
    width: u32,
    height: u32,
    mean_luminance: f64,
    luminance_stddev: f64,
    non_black_fraction: f64,
}

#[derive(Clone, Debug, Serialize)]
struct MandelBenchmarkEntry {
    corpus_index: usize,
    path: String,
    hybrid: Option<bool>,
    boolean: Option<bool>,
    delta_de: Option<bool>,
    formula_ids: Vec<u32>,
    status: String,
    source_bytes: Option<usize>,
    generated_source_bytes: Option<usize>,
    generation_ms: Option<f64>,
    cold_build_ms: Option<f64>,
    warm_build_ms: Vec<f64>,
    cold_render_ms: Option<f64>,
    warm_render_ms: Vec<f64>,
    cold_execution_wall_ms: Option<f64>,
    warm_execution_wall_ms: Vec<f64>,
    first_render_total_ms: Option<f64>,
    output: Option<PathBuf>,
    image_health: Option<MandelBenchmarkImageHealth>,
    error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct MandelBenchmarkSummary {
    successful: usize,
    failed: usize,
    generation_median_ms: f64,
    cold_build_median_ms: f64,
    cold_build_p90_ms: f64,
    cold_render_median_ms: f64,
    cold_render_p90_ms: f64,
    warm_build_median_ms: f64,
    warm_render_median_ms: f64,
    cold_execution_wall_median_ms: f64,
    warm_execution_wall_median_ms: f64,
    first_render_total_median_ms: f64,
    blank_or_near_blank_images: usize,
}

#[derive(Clone, Debug, Serialize)]
struct MandelBenchmarkReport {
    schema_version: u32,
    renderer_revision: String,
    mandelbulber_root: PathBuf,
    metal_device: String,
    kernel_set: String,
    corpus_scenes: usize,
    selected_scenes: usize,
    offset: usize,
    stride: usize,
    width: u32,
    height: u32,
    samples: u32,
    warm_runs: usize,
    elapsed_ms: f64,
    summary: MandelBenchmarkSummary,
    entries: Vec<MandelBenchmarkEntry>,
}

struct CachedMandelRenderArtifacts {
    metallib: PathBuf,
    pipeline_archive: PathBuf,
    compile_ms: f64,
    cache_hit: bool,
    optimization: MandelMetalOptimization,
    source_bytes: usize,
    fallback_marker: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MandelMetalOptimization {
    O0,
    O1,
    Default,
}

impl MandelMetalOptimization {
    fn cache_tag(self) -> &'static [u8] {
        match self {
            Self::O0 => b"o0",
            Self::O1 => b"o1",
            Self::Default => b"default",
        }
    }

    fn metadata_label(self) -> &'static str {
        match self {
            Self::O0 => "o0-exact",
            Self::O1 => "o1-exact",
            Self::Default => "optimized",
        }
    }

    fn diagnostic_label(self) -> &'static str {
        match self {
            Self::O0 => "-O0",
            Self::O1 => "-O1",
            Self::Default => "optimized",
        }
    }

    fn apply(self, command: &mut Command) {
        match self {
            Self::O0 => {
                command.arg("-O0");
            }
            Self::O1 => {
                command.arg("-O1");
            }
            Self::Default => {}
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SdfBackendCandidateMeasurement {
    probe_ms: Option<f64>,
    cold_build_ms: Option<f64>,
    warm_build_ms: Option<f64>,
    predicted_render_ms: Option<f64>,
    predicted_amortized_ms: Option<f64>,
    speedup: Option<f64>,
    parity_mean_absolute_error: Option<f64>,
    parity_max_absolute_error: Option<f64>,
    parity_outlier_fraction: Option<f64>,
    qualified: bool,
    rejection_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SdfBackendSelection {
    version: u32,
    requested: String,
    selected: String,
    decision_source: String,
    direct_evaluator: String,
    topology_key: String,
    workload_key: String,
    cache_key: String,
    probe_width: u32,
    probe_height: u32,
    probe_samples: u32,
    probe_runs: usize,
    warmup_runs: usize,
    adaptive_probe_runs: usize,
    selection_elapsed_ms: f64,
    direct_probe_ms: f64,
    predicted_direct_ms: f64,
    required_speedup: f64,
    generated_surface_required_speedup: f64,
    candidates: BTreeMap<String, SdfBackendCandidateMeasurement>,
    rejection_reason: Option<String>,
}

struct ProbeDirectory(PathBuf);

impl Drop for ProbeDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn usage() {
    eprintln!(
        "Usage:\n\
  fpt-metal render <scene.json> --out <dir> [--renderer sdf|voxel|bound-grid|regional] [--mandelbulber-root <dir>] [--sdf-backend auto] [--regional-program-resolution 16|32] [--bound-grid-resolution 32|64] [--bound-grid-directional] [--bound-grid-fp16] [--bound-grid-profile] [--bound-grid-profile-stride N] [--bound-grid-cage-bounds] [--no-sdf-geometry-split] [--no-sdf-canonical-ir] [--sdf-topology-specialization] [--sdf-tiny-linked-helper] [--sdf-canonical-topology-specialization] [--sdf-compact-canonical-topology-specialization] [--sdf-shared-transform-topology-specialization] [--sdf-affine-index-topology-specialization] [--no-sdf-generated-surface] [--sdf-runtime-source-bytecode] [--sdf-function-stitching normal|inline|auto] [--sdf-stitch-distance-only] [--sdf-stitch-split-graph] [--sdf-stitch-fusion off|one-pair|pairs|double-pairs] [--sdf-program-validation] [--sdf-flat-union] [--sdf-typed-soa] [--voxel-resolution N] [--voxel-normal face|smooth|exact] [--voxel-material stored|exact] [--voxel-offset legacy|precision] [--voxel-storage dense|sparse-bricks|template-bricks] [--voxel-surface-band N] [--voxel-coverage legacy|lipschitz|interval] [--voxel-build staging|direct] [--voxel-brick-rejection] [--voxel-leaf-refinement none|secant-bisection|restricted-trace|fixed-de] [--fpt-root <dir>] [--preview] [--glass-mode analytic|pathtrace] [--sdf-accumulation auto|per-sample|batch|chunked] [--sdf-normal-mode auto|central|tetra|program-gradient] [--sdf-program-optimization off|basic] [--sdf-chunk-samples N] [--mandel-iteration-scale X] [--mandel-screen-lod-rate X] [--mandel-optimization exact|auto] [--mandel-selection-cache PATH] [--width N] [--height N] [--samples N]\n\
  --sdf-backend auto reuses cached decisions; --sdf-backend probe measures on a cache miss\n\
  Research-only: function-stitching variants, canonical/shared/affine generated forms, dual/tiny libraries, bound-grid, and regional backends\n\
  fpt-metal render-batch <jobs.json>\n\
  fpt-metal diagnostic-batch <jobs.json> [--report <report.json>] [--workers N] [--offset N] [--limit N]\n\
  fpt-metal diagnostic <scene.json> --out <dir> --mode <mode> [--max-distance N] [--fpt-root <dir>] [--width N] [--height N]\n\
  fpt-metal preview <scene.json> [--renderer sdf|voxel] [--sdf-backend auto] [--sdf-function-stitching normal|inline] [--no-sdf-stitched-surface] [--voxel-resolution N] [--voxel-normal face|smooth|exact] [--voxel-material stored|exact] [--voxel-offset legacy|precision] [--voxel-storage dense|sparse-bricks|template-bricks] [--voxel-leaf-refinement none|secant-bisection|restricted-trace|fixed-de] [--fpt-root <dir>] [--pathtrace] [--sdf-profile] [--width N] [--height N] [--samples N]\n\
  fpt-metal voxel-export <scene|builtin:menger-sponge> --out <scene.glb> --voxel-resolution N [--mandelbulber-root <dir>] [--fpt-root <dir>] [--bounds-min x,y,z] [--bounds-max x,y,z] [--surface-band N] [--fill-interior]\n\
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
  fpt-metal mandel-catalog <mandelbulber-source> --out <directory>\n\
  fpt-metal mandel-coverage <mandelbulber-source> [--catalog-out <directory>] [--report <report.json>]\n\
  fpt-metal mandel-audit <mandelbulber-source> --report <report.json> [--metal-check [--metal-out <directory>]]\n\
  fpt-metal mandel-scene-audit <mandelbulber-source> --report <report.json> [--metal-check]\n\
  fpt-metal mandel-benchmark <mandelbulber-source> --report <report.json> [--out <directory>] [--kernel-set full|render|offline-render|offline-render-o0] [--offset N] [--limit N] [--stride N] [--width N] [--height N] [--samples N] [--warm-runs N]\n\
  fpt-metal mandel-compile <mandelbulber-source> <formula-id-or-symbol> --out <formula.metal> [--report <report.json>] [--no-check]\n\
  fpt-metal mandel-scene-compile <scene.fract> --mandelbulber-root <dir> --out <scene.metal>\n\
  fpt-metal mandel-formula-policy-audit <mandelbulber-root> --report <report.json>\n\
  fpt-metal mandel-parity <scene.fract> --report <report.json> [--samples N] [--mandelbulber-root <dir>]\n\
  fpt-metal list-scenes\n\
  fpt-metal clean-reports"
    );
}

fn parse_csv_vec3(value: &str, flag: &str) -> Result<[f32; 3]> {
    let values = value
        .split(',')
        .map(str::parse::<f32>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(values.len() == 3, "{flag} requires x,y,z");
    Ok([values[0], values[1], values[2]])
}

fn cached_mandel_voxel_metallib(
    generated_source: &[u8],
    optimization: MandelMetalOptimization,
) -> Result<(PathBuf, f64, bool)> {
    let retained = mandelbulber::compiler::retain_metal_kernels(
        std::str::from_utf8(generated_source)?,
        &["voxel_build_kernel"],
    )?;
    let mut digest = Sha256::new();
    digest.update(b"fpt-mandel-voxel-export-v1\0");
    digest.update(METAL_COMPILER_IDENTITY.as_bytes());
    digest.update(std::env::consts::ARCH.as_bytes());
    digest.update(optimization.cache_tag());
    digest.update(b"\0fast-math\0");
    digest.update(retained.as_bytes());
    let key = format!("{:x}", digest.finalize());
    let directory = mandel_render_cache_directory().join("voxel-export-v1");
    fs::create_dir_all(&directory)?;
    let metallib = directory.join(format!("{key}.metallib"));
    if metallib.is_file() {
        return Ok((metallib, 0.0, true));
    }
    let label = format!(
        "{key}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let (temporary, compile_ms) =
        compile_mandel_metallib(retained.as_bytes(), &directory, &label, optimization)?;
    if metallib.is_file() {
        let _ = fs::remove_file(&temporary);
    } else {
        fs::rename(temporary, &metallib)?;
    }
    Ok((metallib, compile_ms, false))
}

fn voxel_export_command(args: &[String]) -> Result<()> {
    let scene_argument = args
        .first()
        .ok_or_else(|| anyhow!("voxel-export requires a scene"))?;
    let mut output = None::<PathBuf>;
    let mut resolution = None::<u32>;
    let mut mandelbulber_root = None::<PathBuf>;
    let mut fpt_root = None::<PathBuf>;
    let mut bounds_min = None::<[f32; 3]>;
    let mut bounds_max = None::<[f32; 3]>;
    let mut surface_band = 1.0_f32;
    let mut fill_interior = false;
    let mut index = 1usize;
    while index < args.len() {
        let next = |index: &mut usize, flag: &str| -> Result<&str> {
            *index += 1;
            args.get(*index)
                .map(String::as_str)
                .ok_or_else(|| anyhow!("{flag} requires a value"))
        };
        match args[index].as_str() {
            "--out" => output = Some(next(&mut index, "--out")?.into()),
            "--voxel-resolution" => {
                let value = next(&mut index, "--voxel-resolution")?.parse()?;
                ensure!(
                    (1..=512).contains(&value),
                    "voxel resolution must be 1..512"
                );
                resolution = Some(value);
            }
            "--mandelbulber-root" => {
                mandelbulber_root = Some(next(&mut index, "--mandelbulber-root")?.into())
            }
            "--fpt-root" => fpt_root = Some(next(&mut index, "--fpt-root")?.into()),
            "--bounds-min" => {
                bounds_min = Some(parse_csv_vec3(
                    next(&mut index, "--bounds-min")?,
                    "--bounds-min",
                )?)
            }
            "--bounds-max" => {
                bounds_max = Some(parse_csv_vec3(
                    next(&mut index, "--bounds-max")?,
                    "--bounds-max",
                )?)
            }
            "--surface-band" => {
                surface_band = next(&mut index, "--surface-band")?.parse()?;
                ensure!(
                    surface_band.is_finite() && (0.25..=4.0).contains(&surface_band),
                    "surface band must be 0.25..4"
                );
            }
            "--fill-interior" => fill_interior = true,
            flag => bail!("unknown voxel-export option: {flag}"),
        }
        index += 1;
    }
    let output = output.ok_or_else(|| anyhow!("voxel-export requires --out <scene.glb>"))?;
    let resolution =
        resolution.ok_or_else(|| anyhow!("voxel-export requires --voxel-resolution N"))?;

    if scene_argument == "builtin:menger-sponge" {
        let bounds = Aabb::new(
            bounds_min.unwrap_or([-1.25; 3]),
            bounds_max.unwrap_or([1.25; 3]),
        );
        let mut voxelization = VoxelizationParameters::cubic(resolution, bounds);
        voxelization.surface_band = surface_band;
        voxelization.fill_interior = fill_interior;
        let request = VoxelizationRequest {
            scene: FractalScene::menger_sponge(),
            voxelization,
        };
        let grid = voxelize(&request)?;
        ensure!(
            grid.occupied_voxels() > 0,
            "voxel build produced no occupied cells; adjust --bounds-min/--bounds-max or --surface-band"
        );
        let summary = export_glb(&grid, &output)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "contract_version":1,
                "evaluator":"cpu-reference",
                "output":output,
                "resolution":[resolution,resolution,resolution],
                "bounds_min":bounds.min,
                "bounds_max":bounds.max,
                "summary":summary
            }))?
        );
        return Ok(());
    }

    let scene_path = PathBuf::from(scene_argument);
    ensure!(
        scene_path.is_file(),
        "scene does not exist: {}",
        scene_path.display()
    );
    let mut render_arguments = vec![scene_argument.clone()];
    if let Some(root) = mandelbulber_root.as_ref() {
        render_arguments.extend(["--mandelbulber-root".to_owned(), root.display().to_string()]);
    }
    if let Some(root) = fpt_root.as_ref() {
        render_arguments.extend(["--fpt-root".to_owned(), root.display().to_string()]);
    }
    let render_args = parse_render_args(&render_arguments)?;
    let mut loaded = load_scene_config(&render_args)?;
    let is_mandel = loaded.config.sdf_id == SDF_MANDELBULBER;
    let default_bounds = if is_mandel {
        Aabb::new([-4.0; 3], [4.0; 3])
    } else {
        Aabb::new(
            loaded.config.voxel_bounds_min,
            loaded.config.voxel_bounds_max,
        )
    };
    let export_bounds = Aabb::new(
        bounds_min.unwrap_or(default_bounds.min),
        bounds_max.unwrap_or(default_bounds.max),
    );
    ensure!(
        export_bounds
            .min
            .iter()
            .zip(export_bounds.max)
            .all(|(min, max)| min.is_finite() && max.is_finite() && *min < max),
        "invalid voxel bounds"
    );
    let world_scale = if is_mandel {
        loaded.config.set_values[mandelbulber::PARAM_WORLD_SCALE].max(1.0)
    } else {
        1.0
    };
    loaded.config.renderer_backend = RENDERER_VOXEL;
    loaded.config.voxel_resolution = resolution;
    loaded.config.voxel_storage = VOXEL_STORAGE_DENSE;
    loaded.config.voxel_build_mode = VOXEL_BUILD_STAGING;
    loaded.config.voxel_coverage_mode = VOXEL_COVERAGE_LEGACY;
    loaded.config.voxel_surface_band = surface_band;
    loaded.config.voxel_fill_interior = u32::from(fill_interior);
    loaded.config.voxel_bounds_min = export_bounds.min.map(|value| value * world_scale);
    loaded.config.voxel_bounds_max = export_bounds.max.map(|value| value * world_scale);
    loaded.config.sdf_runtime_source_bytecode = 0;

    let (metallib, compile_ms, cache_hit) =
        if let Some(source) = loaded.runtime_metal_source.as_deref() {
            cached_mandel_voxel_metallib(source, MandelMetalOptimization::Default)?
        } else {
            (default_metallib_path()?, 0.0, true)
        };
    let cell_count = (resolution as usize)
        .checked_pow(3)
        .ok_or_else(|| anyhow!("voxel cell count overflow"))?;
    let mut cells = vec![VoxelCell::default(); cell_count];
    let mut build_ms = 0.0_f64;
    let mut error = [0_i8; 1024];
    let status = unsafe {
        fpt_metal_voxel_build(
            c_path(&metallib)?.as_ptr(),
            &loaded.config,
            cells.as_mut_ptr().cast(),
            cells.len() * std::mem::size_of::<VoxelCell>(),
            &mut build_ms,
            error.as_mut_ptr(),
            error.len(),
        )
    };
    ensure!(
        status == 0,
        "Metal voxel export failed: {}",
        bridge_error(&error)
    );
    let scene_bytes = fs::read(&scene_path)?;
    let source_sha256 = format!("{:x}", Sha256::digest(&scene_bytes));
    let source_label = scene_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("fractal");
    let grid = VoxelGrid::from_dense_cells(
        [resolution; 3],
        export_bounds,
        CoordinateSystem::YUpRightHanded,
        source_label,
        source_sha256,
        &cells,
    )?;
    ensure!(
        grid.occupied_voxels() > 0,
        "voxel build produced no occupied cells; adjust --bounds-min/--bounds-max or --surface-band"
    );
    drop(cells);
    let summary = export_glb(&grid, &output)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "contract_version":1,
            "evaluator":"metal-voxel-build-kernel",
            "output":output,
            "resolution":grid.resolution,
            "bounds_min":grid.bounds.min,
            "bounds_max":grid.bounds.max,
            "metal_world_scale":world_scale,
            "metal_compile_ms":compile_ms,
            "metal_build_ms":build_ms,
            "metal_cache_hit":cache_hit,
            "summary":summary
        }))?
    );
    Ok(())
}

fn list_scenes() {
    eprintln!(
        "Supported presets:\n  Cornell_Box\n  Glass_Ball\n  Ball_Fractal\n  Cage_Fractal\n  IFS_Fractal\n  Mandelbox_Fractal\n  Menger_Sponge\n  Mandelbulber 2.x generated analytic formulas (.fract; use --mandelbulber-root)\n  Tower_Fractal\n  Tree_Fractal\n  Any JSON scene containing a typed sdf_program\n  Gradient_Example.fpt (compatibility compiler)"
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

fn materialize_metallib(bytes: &[u8], label: &str, digest: &str) -> Result<PathBuf> {
    static MATERIALIZATION_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

    let directory = std::env::temp_dir().join("fpt-metal");
    let path = directory.join(format!("{label}-{}.metallib", &digest[..16]));
    fs::create_dir_all(&directory)?;
    let is_complete = || {
        fs::metadata(&path)
            .map(|metadata| metadata.is_file() && metadata.len() == bytes.len() as u64)
            .unwrap_or(false)
    };
    if is_complete() {
        return Ok(path);
    }

    // Tests and parallel render jobs can request the same embedded library at once.
    // Publish a fully written sibling atomically so readers never observe a partial file.
    let sequence = MATERIALIZATION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = directory.join(format!(
        ".{label}-{}-{}-{sequence}.tmp",
        &digest[..16],
        std::process::id()
    ));
    fs::write(&temporary, bytes)?;
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        if !is_complete() {
            return Err(error).with_context(|| {
                format!("failed to publish embedded metallib {}", path.display())
            });
        }
    }
    Ok(path)
}

fn default_metallib_path() -> Result<PathBuf> {
    materialize_metallib(METALLIB_BYTES, "Shaders", METALLIB_SHA)
}

fn cached_diagnostic_metallib(
    source: &str,
    optimization: MandelMetalOptimization,
) -> Result<PathBuf> {
    let mut digest = Sha256::new();
    digest.update(b"fpt-mandel-diagnostic-metallib-v4\0");
    digest.update(b"macos-metal2.4\0");
    digest.update(optimization.cache_tag());
    digest.update(b"\0-ffast-math\0");
    digest.update(std::env::consts::ARCH.as_bytes());
    digest.update(source.as_bytes());
    let key = format!("{:x}", digest.finalize());
    let directory = std::env::var_os("FPT_MANDEL_DIAGNOSTIC_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir()
                .join("fpt-metal")
                .join("mandel-diagnostic-v1")
        });
    fs::create_dir_all(&directory)?;
    let metallib = directory.join(format!("{key}.metallib"));
    if metallib.is_file() {
        return Ok(metallib);
    }

    let unique = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let metal_source = directory.join(format!("{key}-{unique}.metal"));
    let air = directory.join(format!("{key}-{unique}.air"));
    let temporary_metallib = directory.join(format!("{key}-{unique}.metallib"));
    fs::write(&metal_source, source)?;
    let mut compile_command = Command::new("xcrun");
    compile_command.args(["-sdk", "macosx", "metal", "-std=macos-metal2.4"]);
    optimization.apply(&mut compile_command);
    let compile = compile_command
        .args(["-ffast-math", "-c"])
        .arg(&metal_source)
        .arg("-o")
        .arg(&air)
        .output()
        .context("launch diagnostic Metal compiler")?;
    if !compile.status.success() {
        let _ = fs::remove_file(&metal_source);
        let _ = fs::remove_file(&air);
        bail!(
            "diagnostic Metal compilation failed:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );
    }
    let mut link = Command::new("xcrun")
        .args(["-sdk", "macosx", "metallib"])
        .arg(&air)
        .arg("-o")
        .arg(&temporary_metallib)
        .output()
        .context("launch diagnostic Metal linker")?;
    if optimization == MandelMetalOptimization::O0
        && !link.status.success()
        && String::from_utf8_lossy(&link.stderr).contains("llvm.global_ctors")
    {
        // Most generated formulas compile dramatically faster at O0. A small
        // set of aggregate-heavy parameter blocks (notably kaleidoscopic IFS)
        // leave global constructors in the AIR at O0, which Metal cannot link
        // into a kernel. O1 folds those constructors while remaining much
        // cheaper than the toolchain's default whole-program optimisation.
        let _ = fs::remove_file(&air);
        let _ = fs::remove_file(&temporary_metallib);
        let fallback_compile = Command::new("xcrun")
            .args([
                "-sdk",
                "macosx",
                "metal",
                "-std=macos-metal2.4",
                "-O1",
                "-ffast-math",
                "-c",
            ])
            .arg(&metal_source)
            .arg("-o")
            .arg(&air)
            .output()
            .context("launch diagnostic Metal O1 fallback compiler")?;
        if !fallback_compile.status.success() {
            let _ = fs::remove_file(&metal_source);
            let _ = fs::remove_file(&air);
            bail!(
                "diagnostic Metal O1 fallback compilation failed:\n{}",
                String::from_utf8_lossy(&fallback_compile.stderr)
            );
        }
        link = Command::new("xcrun")
            .args(["-sdk", "macosx", "metallib"])
            .arg(&air)
            .arg("-o")
            .arg(&temporary_metallib)
            .output()
            .context("launch diagnostic Metal O1 fallback linker")?;
    }
    let _ = fs::remove_file(&metal_source);
    let _ = fs::remove_file(&air);
    if !link.status.success() {
        let _ = fs::remove_file(&temporary_metallib);
        bail!(
            "diagnostic Metal link failed:\n{}",
            String::from_utf8_lossy(&link.stderr)
        );
    }
    if metallib.exists() {
        fs::remove_file(&temporary_metallib)?;
    } else {
        fs::rename(&temporary_metallib, &metallib)?;
    }
    Ok(metallib)
}

fn default_builtin_metallib_path() -> Result<PathBuf> {
    materialize_metallib(
        BUILTIN_METALLIB_BYTES,
        "ShadersBuiltin",
        BUILTIN_METALLIB_SHA,
    )
}

fn default_cage_metallib_path() -> Result<PathBuf> {
    materialize_metallib(CAGE_METALLIB_BYTES, "ShadersCage", CAGE_METALLIB_SHA)
}

fn default_tower_metallib_path() -> Result<PathBuf> {
    materialize_metallib(TOWER_METALLIB_BYTES, "ShadersTower", TOWER_METALLIB_SHA)
}

fn default_stitch_metallib_path() -> Result<PathBuf> {
    materialize_metallib(
        STITCH_METALLIB_BYTES,
        "ShadersStitchHost",
        STITCH_METALLIB_SHA,
    )
}

fn stitch_archive_path(config: &FptRenderConfig) -> Result<Option<PathBuf>> {
    if config.sdf_function_stitching == 0
        || config.sdf_function_stitching == SdfFunctionStitching::Auto as u32
    {
        return Ok(None);
    }
    let mut digest = Sha256::new();
    digest.update(b"fpt-metal-stitched-pipeline-v2\0");
    digest.update(STITCH_METALLIB_SHA.as_bytes());
    digest.update(config.sdf_function_stitching.to_le_bytes());
    digest.update(config.sdf_stitched_surface.to_le_bytes());
    digest.update(config.sdf_stitch_validation.to_le_bytes());
    digest.update(config.sdf_stitch_distance_only.to_le_bytes());
    digest.update(config.sdf_stitch_split_graph.to_le_bytes());
    digest.update(config.sdf_stitch_fusion.to_le_bytes());
    digest.update(config.sdf_program_count.to_le_bytes());
    for instruction in config
        .sdf_program
        .iter()
        .take(config.sdf_program_count as usize)
    {
        digest.update(instruction.opcode.to_le_bytes());
        digest.update(instruction.flags.to_le_bytes());
    }
    digest.update(effective_accumulation(config).as_bytes());
    digest.update(config.preview.to_le_bytes());
    digest.update(u32::from(config.focus_distance <= 0.0).to_le_bytes());
    let key = format!("{:x}", digest.finalize());
    let directory = std::env::var_os("FPT_STITCH_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("fpt-metal/stitched-pipeline-cache"));
    fs::create_dir_all(&directory)?;
    Ok(Some(directory.join(format!("{key}.metallibarchive"))))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrecompiledMetallib {
    Full,
    Builtin,
    Cage,
    Tower,
}

fn precompiled_metallib(config: &FptRenderConfig) -> PrecompiledMetallib {
    if config.renderer_backend != RENDERER_SDF || config.sdf_id == SDF_PROGRAM {
        PrecompiledMetallib::Full
    } else if config.sdf_id == SDF_CAGE_FRACTAL {
        PrecompiledMetallib::Cage
    } else if config.sdf_id == SDF_TOWER_FRACTAL {
        PrecompiledMetallib::Tower
    } else {
        PrecompiledMetallib::Builtin
    }
}

fn metallib_path(args: &RenderArgs, config: &FptRenderConfig) -> Result<PathBuf> {
    if let Some(path) = &args.metallib {
        return Ok(path.clone());
    }
    match precompiled_metallib(config) {
        PrecompiledMetallib::Full => default_metallib_path(),
        PrecompiledMetallib::Builtin => default_builtin_metallib_path(),
        PrecompiledMetallib::Cage => default_cage_metallib_path(),
        PrecompiledMetallib::Tower => default_tower_metallib_path(),
    }
}

fn direct_evaluator_name(config: &FptRenderConfig) -> &'static str {
    if config.sdf_topology_specialization == 5 {
        "generated-affine-index-runs"
    } else if config.sdf_topology_specialization == 4 {
        "generated-shared-transform-dag"
    } else if config.sdf_topology_specialization == 2 {
        "generated-canonical-ir"
    } else if config.sdf_topology_specialization == 3 {
        "generated-compact-canonical-ir"
    } else if config.sdf_topology_specialization == 1 {
        "generated-optimized-program"
    } else if config.sdf_typed_soa.sphere_count
        + config.sdf_typed_soa.box_count
        + config.sdf_typed_soa.plane_count
        > 0
    {
        "typed-soa"
    } else if config.sdf_canonical_count > 0 {
        "canonical-ir"
    } else {
        "optimized-bytecode"
    }
}

fn metal_device_name() -> String {
    let mut gpu = [0_i8; 256];
    unsafe { fpt_metal_device_name(gpu.as_mut_ptr(), gpu.len()) };
    bridge_error(&gpu)
}

fn hash_f32_slice(digest: &mut Sha256, values: &[f32]) {
    for value in values {
        digest.update(value.to_bits().to_le_bytes());
    }
}

fn hash_u32_slice(digest: &mut Sha256, values: &[u32]) {
    for value in values {
        digest.update(value.to_le_bytes());
    }
}

fn hash_u16_slice(digest: &mut Sha256, values: &[u16]) {
    for value in values {
        digest.update(value.to_le_bytes());
    }
}

fn hash_instruction(digest: &mut Sha256, instruction: &FptSdfInstruction) {
    hash_u32_slice(
        digest,
        &[
            instruction.opcode,
            instruction.flags,
            instruction.material_index,
        ],
    );
    hash_f32_slice(digest, &instruction.data);
}

fn backend_topology_key(config: &FptRenderConfig, device_name: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"fpt-metal-sdf-backend-topology-v8\0");
    digest.update(SDF_BACKEND_SELECTION_VERSION.to_le_bytes());
    digest.update(device_name.as_bytes());
    digest.update(METALLIB_SHA.as_bytes());
    digest.update(STITCH_METALLIB_SHA.as_bytes());
    digest.update(METAL_COMPILER_IDENTITY.as_bytes());
    hash_u32_slice(
        &mut digest,
        &[
            config.sdf_id,
            config.sdf_program_count,
            config.sdf_program_optimization,
            config.sdf_geometry_split,
            config.sdf_runtime_source_bytecode,
            config.sdf_shading_program_count,
            config.sdf_flat_union_count,
            config.sdf_canonical_count,
            config.sdf_canonical_transform_count,
            config.sdf_typed_soa.sphere_count,
            config.sdf_typed_soa.box_count,
            config.sdf_typed_soa.plane_count,
        ],
    );
    for instruction in config
        .sdf_program
        .iter()
        .take(config.sdf_program_count as usize)
    {
        hash_u32_slice(&mut digest, &[instruction.opcode, instruction.flags]);
    }
    format!("{:x}", digest.finalize())
}

fn backend_workload_key(config: &FptRenderConfig, device_name: &str) -> String {
    let topology_key = backend_topology_key(config, device_name);
    let mut digest = Sha256::new();
    digest.update(b"fpt-metal-sdf-backend-workload-v8\0");
    digest.update(SDF_BACKEND_SELECTION_VERSION.to_le_bytes());
    digest.update(topology_key.as_bytes());
    hash_u32_slice(
        &mut digest,
        &[
            SDF_BACKEND_PROBE_WIDTH,
            SDF_BACKEND_PROBE_SAMPLES,
            SDF_BACKEND_PROBE_RUNS as u32,
            SDF_BACKEND_ADAPTIVE_RUNS as u32,
        ],
    );
    for value in [
        SDF_BACKEND_MARGIN,
        SDF_GENERATED_SURFACE_REQUIRED_SPEEDUP,
        SDF_BACKEND_NEAR_GATE_FRACTION,
        SDF_BACKEND_PARITY_MAE,
        SDF_BACKEND_PARITY_OUTLIER_THRESHOLD,
        SDF_BACKEND_PARITY_MAX_OUTLIER_FRACTION,
    ] {
        digest.update(value.to_le_bytes());
    }
    hash_u32_slice(
        &mut digest,
        &[
            config.width,
            config.height,
            config.samples,
            config.preview,
            config.sdf_id,
            config.glass_mode,
            config.sdf_accumulation_mode,
            config.sdf_profile,
            config.sdf_bounce_cap,
            config.sdf_russian_roulette,
            config.sdf_normal_mode,
            config.sdf_chunk_samples,
            config.sdf_program_count,
            config.gradient_count,
            config.material_mode,
            config.hdri_enabled,
            config.hdri_width,
            config.hdri_height,
            config.fractal_style_mode,
            config.sdf_program_source_count,
            config.sdf_program_optimization,
            config.sdf_flat_union_count,
            config.sdf_shading_program_count,
            config.sdf_geometry_split,
            config.sdf_canonical_count,
            config.sdf_canonical_source_count,
            config.sdf_canonical_transform_count,
            config.sdf_typed_soa.sphere_count,
            config.sdf_typed_soa.box_count,
            config.sdf_typed_soa.plane_count,
        ],
    );
    hash_f32_slice(
        &mut digest,
        &[
            config.sdf_rr_start,
            config.sdf_rr_min_prob,
            config.mandel_iteration_scale,
        ],
    );
    hash_f32_slice(&mut digest, &config.camera_position);
    hash_f32_slice(&mut digest, &config.camera_yaw_pitch);
    hash_f32_slice(
        &mut digest,
        &[
            config.camera_roll,
            config.camera_fov,
            config.camera_dof,
            config.focus_distance,
        ],
    );
    hash_f32_slice(&mut digest, &config.render);
    hash_f32_slice(&mut digest, &config.world);
    hash_f32_slice(&mut digest, &config.world_one_color);
    hash_f32_slice(&mut digest, &config.sun);
    hash_f32_slice(&mut digest, &config.sun_color);
    hash_f32_slice(&mut digest, &config.background_gradient);
    hash_f32_slice(&mut digest, &config.post);
    hash_f32_slice(&mut digest, &config.set_values);
    hash_f32_slice(&mut digest, &config.vset_values);
    hash_f32_slice(&mut digest, &config.fractal_style);
    hash_f32_slice(&mut digest, &config.program_material);
    for stop in config
        .gradient_stops
        .iter()
        .take(config.gradient_count as usize)
    {
        hash_f32_slice(&mut digest, stop);
    }
    digest.update(config.hdri_path);
    if config.hdri_enabled != 0 {
        hash_u16_slice(&mut digest, &config.hdri_lut);
    }
    for instruction in config
        .sdf_program
        .iter()
        .take(config.sdf_program_count as usize)
    {
        hash_instruction(&mut digest, instruction);
    }
    for instruction in config
        .sdf_shading_program
        .iter()
        .take(config.sdf_shading_program_count as usize)
    {
        hash_instruction(&mut digest, instruction);
    }
    format!("{:x}", digest.finalize())
}

fn backend_selection_cache_path(config: &FptRenderConfig) -> Result<(String, String, PathBuf)> {
    let device_name = metal_device_name();
    let topology_key = backend_topology_key(config, &device_name);
    let workload_key = backend_workload_key(config, &device_name);
    let directory = std::env::var_os("FPT_SDF_BACKEND_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("fpt-metal/backend-selection-cache"));
    fs::create_dir_all(&directory)?;
    Ok((
        topology_key,
        workload_key.clone(),
        directory.join(format!("{workload_key}.json")),
    ))
}

fn topology_specialization_supported(config: &FptRenderConfig) -> bool {
    config.sdf_id == SDF_PROGRAM
        && config.sdf_program_count > 0
        && config
            .sdf_program
            .iter()
            .take(config.sdf_program_count as usize)
            .all(|instruction| match instruction.opcode {
                SDF_OP_ABS | SDF_OP_TRANSLATE | SDF_OP_SCALE | SDF_OP_ROTATE_X
                | SDF_OP_ROTATE_Y | SDF_OP_ROTATE_Z | SDF_OP_REPEAT | SDF_OP_SORT_DESC => true,
                SDF_OP_SPHERE | SDF_OP_BOX | SDF_OP_PLANE => instruction.flags <= 2,
                _ => false,
            })
}

fn direct_backend_config(config: &FptRenderConfig) -> FptRenderConfig {
    let mut direct = *config;
    direct.sdf_function_stitching = SdfFunctionStitching::Off as u32;
    direct.sdf_topology_specialization = 0;
    direct.sdf_runtime_source_bytecode = 0;
    direct.sdf_stitch_validation = 0;
    direct
}

fn generated_backend_config(config: &FptRenderConfig, analytic_surface: bool) -> FptRenderConfig {
    let dual_generated_library = config.sdf_runtime_source_bytecode == 2;
    let mut generated = *config;
    generated.sdf_typed_soa = FptTypedSoAProgram::default();
    generated.sdf_flat_union_count = 0;
    generated.sdf_function_stitching = SdfFunctionStitching::Off as u32;
    generated.sdf_topology_specialization = 1;
    generated.sdf_runtime_source_bytecode = if dual_generated_library { 2 } else { 0 };
    generated.sdf_stitched_surface = u32::from(analytic_surface);
    generated.sdf_stitch_validation = 0;
    generated.sdf_stitch_distance_only = SDF_STITCH_STATE_FULL;
    generated.sdf_stitch_split_graph = 0;
    generated.sdf_stitch_fusion = 0;
    generated
}

fn apply_cached_backend(config: &FptRenderConfig, selected: &str) -> Option<FptRenderConfig> {
    match selected {
        "direct" => Some(direct_backend_config(config)),
        "generated-distance" => Some(generated_backend_config(config, false)),
        "generated-surface" => Some(generated_backend_config(config, true)),
        _ => None,
    }
}

fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

fn backend_probe_order(run: usize) -> [usize; 3] {
    match run % 3 {
        0 => [0, 1, 2],
        1 => [2, 0, 1],
        _ => [1, 2, 0],
    }
}

fn backend_gate_is_near(ratio: f64, gate: f64) -> bool {
    ((ratio / gate) - 1.0).abs() <= SDF_BACKEND_NEAR_GATE_FRACTION
}

fn candidate_probe_qualifies(
    direct_ms: f64,
    candidate_ms: f64,
    mae: f64,
    outlier_fraction: f64,
) -> bool {
    mae <= SDF_BACKEND_PARITY_MAE
        && outlier_fraction <= SDF_BACKEND_PARITY_MAX_OUTLIER_FRACTION
        && direct_ms / candidate_ms >= 1.0 / (1.0 - SDF_BACKEND_MARGIN)
}

fn choose_measured_backend(
    direct_ms: f64,
    candidates: &BTreeMap<String, SdfBackendCandidateMeasurement>,
) -> String {
    let mut best_non_surface = ("direct", direct_ms);
    if let Some(candidate) = candidates.get("generated-distance")
        && candidate.qualified
        && let Some(predicted_ms) = candidate.predicted_amortized_ms
        && predicted_ms < best_non_surface.1
    {
        best_non_surface = ("generated-distance", predicted_ms);
    }
    if let Some(surface) = candidates.get("generated-surface")
        && surface.qualified
        && let Some(surface_ms) = surface.predicted_amortized_ms
        && best_non_surface.1 / surface_ms >= SDF_GENERATED_SURFACE_REQUIRED_SPEEDUP
    {
        return "generated-surface".to_owned();
    }
    best_non_surface.0.to_owned()
}

fn probe_dimensions(config: &FptRenderConfig) -> (u32, u32) {
    let width = config.width.clamp(1, SDF_BACKEND_PROBE_WIDTH);
    let height = ((u64::from(config.height) * u64::from(width)
        + u64::from(config.width.max(1)) / 2)
        / u64::from(config.width.max(1)))
    .clamp(1, u64::from(config.height.max(1))) as u32;
    (width, height)
}

fn probe_linear_error(lhs: &[f32], rhs: &[f32]) -> Result<(f64, f64, f64)> {
    if lhs.len() != rhs.len() {
        bail!("backend probes produced different linear buffer lengths");
    }
    let mut absolute_error = 0.0_f64;
    let mut maximum = 0.0_f64;
    let mut outliers = 0_usize;
    for (left, right) in lhs.iter().zip(rhs) {
        if !left.is_finite() || !right.is_finite() {
            bail!("backend probe produced a non-finite linear component");
        }
        let difference = f64::from((left - right).abs());
        absolute_error += difference;
        maximum = maximum.max(difference);
        outliers += usize::from(difference > SDF_BACKEND_PARITY_OUTLIER_THRESHOLD);
    }
    Ok((
        absolute_error / lhs.len().max(1) as f64,
        maximum,
        outliers as f64 / lhs.len().max(1) as f64,
    ))
}

fn execute_metal_render_internal(
    config: &FptRenderConfig,
    metallib: &Path,
    stitch_metallib: &Path,
    stitch_archive: Option<&Path>,
    output: &Path,
    shader_source: &[u8],
    linear_output: Option<&mut [f32]>,
) -> Result<MetalRenderStats> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let metallib_c = c_path(metallib)?;
    let stitch_metallib_c = c_path(stitch_metallib)?;
    let stitch_archive_c = c_path(stitch_archive.unwrap_or_else(|| Path::new("")))?;
    let output_c = c_path(output)?;
    let mut stats = MetalRenderStats::default();
    let mut error = [0_i8; 4096];
    let (linear_output_ptr, linear_output_len) = linear_output
        .map(|output| (output.as_mut_ptr(), output.len()))
        .unwrap_or((std::ptr::null_mut(), 0));
    let status = unsafe {
        fpt_metal_render(
            metallib_c.as_ptr(),
            stitch_metallib_c.as_ptr(),
            stitch_archive_c.as_ptr(),
            output_c.as_ptr(),
            shader_source.as_ptr().cast(),
            shader_source.len(),
            config,
            &mut stats.build_ms,
            &mut stats.elapsed_ms,
            &mut stats.voxel_memory_bytes,
            &mut stats.voxel_active_bricks,
            &mut stats.voxel_active_cells,
            &mut stats.voxel_rejected_bricks,
            &mut stats.bound_grid,
            &mut stats.stitch_cache_status,
            &mut stats.stitch_validation,
            &mut stats.stitch_pipeline,
            &mut stats.sdf_profile,
            linear_output_ptr,
            linear_output_len,
            error.as_mut_ptr(),
            error.len(),
        )
    };
    if status != 0 {
        bail!("{}", bridge_error(&error));
    }
    Ok(stats)
}

fn execute_metal_render(
    config: &FptRenderConfig,
    metallib: &Path,
    stitch_metallib: &Path,
    stitch_archive: Option<&Path>,
    output: &Path,
) -> Result<MetalRenderStats> {
    execute_metal_render_internal(
        config,
        metallib,
        stitch_metallib,
        stitch_archive,
        output,
        METAL_SOURCE_BYTES,
        None,
    )
}

fn select_sdf_backend(
    config: &FptRenderConfig,
    metallib: &Path,
    stitch_metallib: &Path,
    probe_on_cache_miss: bool,
) -> Result<(FptRenderConfig, SdfBackendSelection)> {
    let direct = direct_backend_config(config);
    let direct_evaluator = direct_evaluator_name(&direct).to_owned();
    let (topology_key, workload_key, cache_path) = backend_selection_cache_path(config)?;
    if let Ok(data) = fs::read(&cache_path)
        && let Ok(mut cached) = serde_json::from_slice::<SdfBackendSelection>(&data)
        && cached.version == SDF_BACKEND_SELECTION_VERSION
        && cached.topology_key == topology_key
        && cached.workload_key == workload_key
        && let Some(selected) = apply_cached_backend(config, &cached.selected)
    {
        cached.decision_source = "cache".to_owned();
        return Ok((selected, cached));
    }

    let (probe_width, probe_height) = probe_dimensions(config);
    let base_selection = SdfBackendSelection {
        version: SDF_BACKEND_SELECTION_VERSION,
        requested: "auto".to_owned(),
        selected: "direct".to_owned(),
        decision_source: "static".to_owned(),
        direct_evaluator,
        topology_key,
        workload_key: workload_key.clone(),
        cache_key: workload_key,
        probe_width,
        probe_height,
        probe_samples: config.samples.clamp(1, SDF_BACKEND_PROBE_SAMPLES),
        probe_runs: SDF_BACKEND_PROBE_RUNS,
        warmup_runs: 1,
        adaptive_probe_runs: 0,
        selection_elapsed_ms: 0.0,
        direct_probe_ms: 0.0,
        predicted_direct_ms: 0.0,
        required_speedup: 1.0 / (1.0 - SDF_BACKEND_MARGIN),
        generated_surface_required_speedup: SDF_GENERATED_SURFACE_REQUIRED_SPEEDUP,
        candidates: BTreeMap::new(),
        rejection_reason: None,
    };
    if !topology_specialization_supported(config) {
        let mut selection = base_selection;
        selection.rejection_reason =
            Some("topology is not supported by generated MSL or stitching".to_owned());
        fs::write(&cache_path, serde_json::to_vec_pretty(&selection)?)?;
        return Ok((direct, selection));
    }
    if !probe_on_cache_miss {
        let mut selection = base_selection;
        selection.decision_source = "policy".to_owned();
        selection.rejection_reason = Some(
            "offline auto found no workload decision; using direct without probing (use --sdf-backend probe to measure and cache)"
                .to_owned(),
        );
        return Ok((direct, selection));
    }

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let probe_directory = ProbeDirectory(std::env::temp_dir().join(format!(
        "fpt-metal/backend-probe-{}-{nonce}",
        std::process::id()
    )));
    fs::create_dir_all(&probe_directory.0)?;
    let mut direct_probe = direct;
    direct_probe.width = probe_width;
    direct_probe.height = probe_height;
    direct_probe.samples = base_selection.probe_samples;
    direct_probe.sdf_accumulation_mode = match effective_accumulation(config) {
        "per-sample" => SDF_ACCUMULATION_PER_SAMPLE,
        "chunked" => SDF_ACCUMULATION_CHUNKED,
        _ => SDF_ACCUMULATION_BATCH,
    };

    struct CandidateProbe {
        name: &'static str,
        config: FptRenderConfig,
        times: Vec<f64>,
        builds: Vec<f64>,
        cold_build_ms: Option<f64>,
        parity_output: Option<Vec<f32>>,
        failure: Option<String>,
    }

    let prepare_candidate = |name: &'static str, mut candidate: FptRenderConfig| {
        candidate.width = probe_width;
        candidate.height = probe_height;
        candidate.samples = base_selection.probe_samples;
        candidate.sdf_accumulation_mode = direct_probe.sdf_accumulation_mode;
        CandidateProbe {
            name,
            config: candidate,
            times: Vec::with_capacity(SDF_BACKEND_PROBE_RUNS),
            builds: Vec::with_capacity(SDF_BACKEND_PROBE_RUNS),
            cold_build_ms: None,
            parity_output: None,
            failure: None,
        }
    };
    let mut candidates = vec![
        prepare_candidate(
            "generated-distance",
            generated_backend_config(config, false),
        ),
        prepare_candidate("generated-surface", generated_backend_config(config, true)),
    ];

    let component_count = probe_width as usize * probe_height as usize * 4;
    let mut direct_parity_output = vec![0.0_f32; component_count];
    execute_metal_render_internal(
        &direct_probe,
        metallib,
        stitch_metallib,
        None,
        &probe_directory.0.join("direct-warmup.png"),
        METAL_SOURCE_BYTES,
        Some(&mut direct_parity_output),
    )?;
    for candidate in &mut candidates {
        let mut parity_output = vec![0.0_f32; component_count];
        let output = probe_directory
            .0
            .join(format!("{}-warmup.png", candidate.name));
        match execute_metal_render_internal(
            &candidate.config,
            metallib,
            stitch_metallib,
            None,
            &output,
            METAL_SOURCE_BYTES,
            Some(&mut parity_output),
        ) {
            Ok(stats) => {
                candidate.cold_build_ms = Some(stats.build_ms);
                candidate.parity_output = Some(parity_output);
            }
            Err(error) => candidate.failure = Some(error.to_string()),
        }
    }

    let mut direct_times = Vec::with_capacity(SDF_BACKEND_PROBE_RUNS + SDF_BACKEND_ADAPTIVE_RUNS);
    macro_rules! run_probes {
        ($start_run:expr, $run_count:expr) => {{
            for run in $start_run..$start_run + $run_count {
                let order = backend_probe_order(run);
                for backend in order {
                    if backend == 0 {
                        let output = probe_directory.0.join(format!("direct-{run}.png"));
                        let stats = execute_metal_render(
                            &direct_probe,
                            metallib,
                            stitch_metallib,
                            None,
                            &output,
                        )?;
                        direct_times.push(stats.elapsed_ms);
                        continue;
                    }
                    let candidate = &mut candidates[backend - 1];
                    if candidate.failure.is_some() {
                        continue;
                    }
                    let output = probe_directory
                        .0
                        .join(format!("{}-{run}.png", candidate.name));
                    match execute_metal_render(
                        &candidate.config,
                        metallib,
                        stitch_metallib,
                        None,
                        &output,
                    ) {
                        Ok(stats) => {
                            candidate.times.push(stats.elapsed_ms);
                            candidate.builds.push(stats.build_ms);
                        }
                        Err(error) => candidate.failure = Some(error.to_string()),
                    }
                }
            }
            Ok::<(), anyhow::Error>(())
        }};
    }
    run_probes!(0, SDF_BACKEND_PROBE_RUNS)?;

    let workload_scale =
        f64::from(config.width) * f64::from(config.height) * f64::from(config.samples.max(1))
            / (f64::from(probe_width)
                * f64::from(probe_height)
                * f64::from(base_selection.probe_samples));
    let initial_direct_ms = median(direct_times.clone()) * workload_scale;
    let initial_candidate_ms = candidates
        .iter()
        .map(|candidate| {
            if candidate.failure.is_some() || candidate.times.is_empty() {
                None
            } else {
                Some(
                    median(candidate.times.clone()) * workload_scale
                        + median(candidate.builds.clone()),
                )
            }
        })
        .collect::<Vec<_>>();
    let required_speedup = 1.0 / (1.0 - SDF_BACKEND_MARGIN);
    let mut adaptive = initial_candidate_ms.iter().flatten().any(|candidate_ms| {
        backend_gate_is_near(initial_direct_ms / candidate_ms, required_speedup)
    });
    if let Some(surface_ms) = initial_candidate_ms.get(1).copied().flatten() {
        let best_non_surface_ms = initial_candidate_ms
            .first()
            .copied()
            .flatten()
            .unwrap_or(initial_direct_ms)
            .min(initial_direct_ms);
        adaptive |= backend_gate_is_near(
            best_non_surface_ms / surface_ms,
            SDF_GENERATED_SURFACE_REQUIRED_SPEEDUP,
        );
    }
    if adaptive {
        run_probes!(SDF_BACKEND_PROBE_RUNS, SDF_BACKEND_ADAPTIVE_RUNS)?;
    }

    let mut selection = base_selection;
    selection.decision_source = "probe".to_owned();
    selection.probe_runs = direct_times.len();
    selection.adaptive_probe_runs = usize::from(adaptive) * SDF_BACKEND_ADAPTIVE_RUNS;
    selection.direct_probe_ms = median(direct_times);
    selection.predicted_direct_ms = selection.direct_probe_ms * workload_scale;
    for candidate in candidates {
        let measurement = if let Some(failure) = candidate.failure {
            SdfBackendCandidateMeasurement {
                probe_ms: None,
                cold_build_ms: candidate.cold_build_ms,
                warm_build_ms: None,
                predicted_render_ms: None,
                predicted_amortized_ms: None,
                speedup: None,
                parity_mean_absolute_error: None,
                parity_max_absolute_error: None,
                parity_outlier_fraction: None,
                qualified: false,
                rejection_reason: Some(format!("probe failed: {failure}")),
            }
        } else {
            let probe_ms = median(candidate.times);
            let cold_build_ms = candidate.cold_build_ms;
            let warm_build_ms_value = median(candidate.builds);
            let warm_build_ms = Some(warm_build_ms_value);
            let predicted_render_ms = probe_ms * workload_scale;
            // The cold compile is discovery work already paid by this probe.
            // The final render and cached selections reuse the topology library,
            // so only the observed warm lookup/compile cost is charged here.
            let predicted_amortized_ms = predicted_render_ms + warm_build_ms_value;
            let speedup = selection.predicted_direct_ms / predicted_amortized_ms;
            let (mae, maximum, outlier_fraction) = probe_linear_error(
                &direct_parity_output,
                candidate
                    .parity_output
                    .as_deref()
                    .expect("successful candidate probe linear output"),
            )?;
            let qualified = candidate_probe_qualifies(
                selection.predicted_direct_ms,
                predicted_amortized_ms,
                mae,
                outlier_fraction,
            );
            let rejection_reason = if mae > SDF_BACKEND_PARITY_MAE
                || outlier_fraction > SDF_BACKEND_PARITY_MAX_OUTLIER_FRACTION
            {
                Some(format!(
                    "probe parity exceeded tolerance (MAE {mae:.9}, max {maximum}, outliers {outlier_fraction:.9})"
                ))
            } else if !qualified {
                Some(format!(
                    "speedup {speedup:.3}x is below the required {:.3}x",
                    selection.required_speedup
                ))
            } else {
                None
            };
            SdfBackendCandidateMeasurement {
                probe_ms: Some(probe_ms),
                cold_build_ms,
                warm_build_ms,
                predicted_render_ms: Some(predicted_render_ms),
                predicted_amortized_ms: Some(predicted_amortized_ms),
                speedup: Some(speedup),
                parity_mean_absolute_error: Some(mae),
                parity_max_absolute_error: Some(maximum),
                parity_outlier_fraction: Some(outlier_fraction),
                qualified,
                rejection_reason,
            }
        };
        selection
            .candidates
            .insert(candidate.name.to_owned(), measurement);
    }
    selection.selected =
        choose_measured_backend(selection.predicted_direct_ms, &selection.candidates);
    if selection.selected == "direct" {
        selection.rejection_reason = Some(
            "no specialized candidate cleared the parity and end-to-end margin gates".to_owned(),
        );
    } else {
        selection.rejection_reason = None;
    }
    fs::write(&cache_path, serde_json::to_vec_pretty(&selection)?)?;
    let selected = apply_cached_backend(config, &selection.selected)
        .expect("selector only emits supported backend names");
    Ok((selected, selection))
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

fn mandel_tile_rows() -> u32 {
    std::env::var("FPT_MANDEL_TILE_ROWS")
        .ok()
        .and_then(|value| value.parse().ok())
        .map(|value: u32| value.clamp(1, 32))
        .unwrap_or(32)
}

struct RenderMetadataInput<'a> {
    output: &'a Path,
    scene: &'a Path,
    config: &'a FptRenderConfig,
    stats: &'a MetalRenderStats,
    stitch_cache_key: Option<&'a str>,
    backend_selection: Option<&'a SdfBackendSelection>,
    mandel_kernel_mode: Option<&'a str>,
    mandel_compile_mode: Option<&'a str>,
    mandel_dispatch_mode: Option<&'a str>,
    mandel_formula_dispatch_mode: Option<&'a str>,
    mandel_cache_status: Option<&'a str>,
    mandel_source_bytes: Option<usize>,
    mandel_offline_compile_ms: Option<f64>,
}

fn write_render_metadata(input: RenderMetadataInput<'_>) -> Result<()> {
    let RenderMetadataInput {
        output,
        scene,
        config,
        stats,
        stitch_cache_key,
        backend_selection,
        mandel_kernel_mode,
        mandel_compile_mode,
        mandel_dispatch_mode,
        mandel_formula_dispatch_mode,
        mandel_cache_status,
        mandel_source_bytes,
        mandel_offline_compile_ms,
    } = input;
    let MetalRenderStats {
        build_ms,
        elapsed_ms,
        voxel_memory_bytes,
        voxel_active_bricks,
        voxel_active_cells,
        voxel_rejected_bricks,
        bound_grid: bound_grid_stats,
        stitch_cache_status,
        stitch_validation,
        stitch_pipeline,
        sdf_profile,
    } = stats;
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
    let throughput = if *elapsed_ms > 0.0 {
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
        "backend": match config.renderer_backend {
            RENDERER_VOXEL => "voxel_pathtrace",
            RENDERER_BOUND_GRID => "procedural_bound_grid",
            RENDERER_REGIONAL => "procedural_regional",
            _ => "sdf_pathtrace",
        },
        "geometry": if config.renderer_backend == RENDERER_VOXEL { "voxel_field" } else { "procedural_sdf" },
        "renderer": match config.renderer_backend {
            RENDERER_VOXEL => "voxel",
            RENDERER_BOUND_GRID => "bound-grid",
            RENDERER_REGIONAL => "regional",
            _ => "sdf",
        },
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
        "mandel_iteration_scale": config.mandel_iteration_scale,
        "mandel_screen_lod_rate": config.vset_values[mandelbulber::VPARAM_SCREEN_LOD_RATE],
        "sdf_normal_mode": match config.sdf_normal_mode {
            value if value == SdfNormalMode::Tetra as u32 => "tetra",
            value if value == SdfNormalMode::ProgramGradient as u32 => "program-gradient",
            value if value == SdfNormalMode::Central as u32 => "central",
            _ => "auto",
        },
        "sdf_program_optimization": if config.sdf_program_optimization == SdfProgramOptimization::Basic as u32 { "basic" } else { "off" },
        "sdf_program_source_instruction_count": config.sdf_program_source_count,
        "sdf_program_instruction_count": config.sdf_program_count,
        "sdf_shading_program_instruction_count": config.sdf_shading_program_count,
        "sdf_canonical_source_primitive_count": config.sdf_canonical_source_count,
        "sdf_canonical_primitive_count": config.sdf_canonical_count,
        "sdf_canonical_transform_count": config.sdf_canonical_transform_count,
        "sdf_canonical_transform_reuse": if config.sdf_canonical_transform_count > 0 {
            Some(config.sdf_canonical_count as f64 / config.sdf_canonical_transform_count as f64)
        } else {
            None
        },
        "sdf_canonical_wide_parameter_bytes": config.sdf_canonical_count as u64 * 80,
        "sdf_canonical_indexed_parameter_bytes": config.sdf_canonical_transform_count as u64 * 64
            + config.sdf_canonical_count as u64 * 32,
        "sdf_direct_evaluator": direct_evaluator_name(config),
        "sdf_geometry_split": config.sdf_geometry_split != 0,
        "sdf_topology_specialization": config.sdf_topology_specialization != 0,
        "sdf_topology_source": match config.sdf_topology_specialization {
            5 => "affine-index-runs",
            4 => "shared-transform-dag",
            3 => "compact-canonical-ir",
            2 => "canonical-ir",
            1 => "optimized-program",
            _ => "none",
        },
        "sdf_runtime_source_bytecode": config.sdf_runtime_source_bytecode == 1,
        "sdf_generated_library_layout": match config.sdf_runtime_source_bytecode {
            3 => "tiny-private-linked-helper",
            2 => "dual-function-constant",
            _ => "single-variant",
        },
        "sdf_function_stitching": match config.sdf_function_stitching {
            1 => "normal",
            2 => "always-inline",
            _ => "off",
        },
        "sdf_stitched_surface": config.sdf_stitched_surface != 0,
        "sdf_generated_surface": config.sdf_topology_specialization != 0 && config.sdf_stitched_surface != 0,
        "sdf_surface_evaluator": if matches!(config.sdf_topology_specialization, 3..=5) && config.sdf_stitched_surface == 0 {
            "canonical-ir"
        } else if config.sdf_topology_specialization != 0 && config.sdf_stitched_surface != 0 {
            "generated-analytic"
        } else if config.sdf_topology_specialization != 0 {
            "interpreted-program"
        } else if config.sdf_canonical_count > 0 {
            "canonical-ir"
        } else {
            "interpreted-program"
        },
        "sdf_stitch_distance_state": if config.sdf_stitch_split_graph != 0 {
            "lean-split-graph"
        } else if config.sdf_stitch_distance_only != 0 {
            "lean-distance"
        } else {
            "full-surface"
        },
        "sdf_stitch_validation": config.sdf_stitch_validation != 0,
        "sdf_program_validation": config.sdf_stitch_validation != 0,
        "sdf_profile": if config.sdf_profile != 0 {
            json!({
                "stride": 4,
                "sampled_pixels": sdf_profile.pixels,
                "primary_steps": sdf_profile.primary_steps,
                "secondary_steps": sdf_profile.secondary_steps,
                "shadow_steps": sdf_profile.shadow_steps,
                "normal_evaluations": sdf_profile.normal_evals,
                "bounces": sdf_profile.bounces,
                "distance_evaluations": sdf_profile.distance_evals,
                "march_orbit_iterations": sdf_profile.march_orbit_iterations,
                "refinement_steps": sdf_profile.refinement_steps,
                "normal_field_evaluations": sdf_profile.normal_field_evals,
                "material_evaluations": sdf_profile.material_evals,
                "max_ray_steps": sdf_profile.max_ray_steps,
                "max_pixel_steps": sdf_profile.max_pixel_steps,
                "ray_phases": ["primary", "secondary", "shadow", "normal"],
                "distance_evaluations_by_phase": sdf_profile.distance_evals_by_phase,
                "orbit_iterations_by_phase": sdf_profile.orbit_iterations_by_phase,
                "formula_slot_iterations": sdf_profile.formula_slot_iterations,
                "refinement_distance_evaluations": sdf_profile.refinement_distance_evals,
                "estimated_ms": {
                    "primary": sdf_profile.primary_ms_estimate,
                    "secondary": sdf_profile.secondary_ms_estimate,
                    "shadow": sdf_profile.shadow_ms_estimate,
                    "normal": sdf_profile.normal_ms_estimate,
                    "bounce": sdf_profile.bounce_ms_estimate,
                },
                "estimate_note": "render time apportioned by counted work units; not isolated GPU timers"
            })
        } else { Value::Null },
        "sdf_stitch_fusion": match config.sdf_stitch_fusion {
            1 => "one-pair",
            2 => "pairs",
            3 => "double-pairs",
            _ => "off",
        },
        "sdf_library_mode": if config.sdf_runtime_source_bytecode == 3 {
            "precompiled-host-tiny-linked-helper"
        } else if config.sdf_function_stitching != 0 {
            "function-stitched"
        } else if config.sdf_topology_specialization != 0 {
            "runtime-source-topology"
        } else if config.sdf_runtime_source_bytecode != 0 {
            "runtime-source-bytecode"
        } else {
            "offline-metallib"
        },
        "metal_language_version": "2.4",
        "metal_math_mode": "fast",
        "mandel_kernel_mode": mandel_kernel_mode,
        "mandel_compile_mode": mandel_compile_mode,
        "mandel_dispatch_mode": mandel_dispatch_mode,
        "mandel_formula_dispatch_mode": mandel_formula_dispatch_mode,
        "mandel_cache_status": mandel_cache_status,
        "mandel_source_bytes": mandel_source_bytes,
        "mandel_offline_compile_ms": mandel_offline_compile_ms,
        "mandel_pipeline_build_ms": mandel_offline_compile_ms.map(|compile_ms| (build_ms - compile_ms).max(0.0)),
        "sdf_flat_union_primitive_count": config.sdf_flat_union_count,
        "sdf_typed_soa_sphere_count": config.sdf_typed_soa.sphere_count,
        "sdf_typed_soa_box_count": config.sdf_typed_soa.box_count,
        "sdf_typed_soa_plane_count": config.sdf_typed_soa.plane_count,
        "sdf_typed_soa_primitive_count": config.sdf_typed_soa.sphere_count
            + config.sdf_typed_soa.box_count + config.sdf_typed_soa.plane_count,
        "sdf_distance_evaluator": if config.sdf_runtime_source_bytecode == 3 {
            "tiny-linked-topology-msl"
        } else if config.sdf_typed_soa.sphere_count
            + config.sdf_typed_soa.box_count + config.sdf_typed_soa.plane_count > 0 {
            "typed-soa"
        } else if config.sdf_flat_union_count > 0 {
            "primitive-list"
        } else if config.sdf_function_stitching != 0 {
            "topology-stitched"
        } else if config.sdf_topology_specialization != 0 {
            "topology-msl"
        } else if config.sdf_geometry_split != 0 {
            "geometry-bytecode"
        } else {
            "bytecode"
        },
        "elapsed_ms": elapsed_ms,
        "acceleration_build_ms": build_ms,
        "sdf_runtime_library_build_ms": if config.sdf_topology_specialization != 0 || config.sdf_runtime_source_bytecode != 0 { Some(build_ms) } else { None },
        "sdf_stitched_library_build_ms": if config.sdf_function_stitching != 0 { Some(build_ms) } else { None },
        "sdf_stitch_cache_status": match stitch_cache_status {
            1 => "populated",
            2 => "loaded",
            _ => "disabled",
        },
        "sdf_stitch_cache_key": stitch_cache_key,
        "sdf_stitch_validation_stats": if config.sdf_stitch_validation != 0 {
            Some(json!({
                "sample_count": stitch_validation.sample_count,
                "distance_failures": stitch_validation.distance_failures,
                "gradient_failures": stitch_validation.gradient_failures,
                "max_distance_error": stitch_validation.max_distance_error,
                "max_gradient_error": stitch_validation.max_gradient_error,
            }))
        } else { None },
        "sdf_stitch_pipeline_stats": if config.sdf_function_stitching != 0 {
            Some(json!({
                "instruction_count": stitch_pipeline.instruction_count,
                "transform_instruction_count": stitch_pipeline.transform_instruction_count,
                "primitive_instruction_count": stitch_pipeline.primitive_instruction_count,
                "primitive_type_runs": stitch_pipeline.primitive_type_runs,
                "union_count": stitch_pipeline.union_count,
                "intersection_count": stitch_pipeline.intersection_count,
                "subtraction_count": stitch_pipeline.subtraction_count,
                "thread_execution_width": stitch_pipeline.thread_execution_width,
                "max_total_threads_per_threadgroup": stitch_pipeline.max_total_threads_per_threadgroup,
                "static_threadgroup_memory_bytes": stitch_pipeline.static_threadgroup_memory_bytes,
                "graph_node_count": stitch_pipeline.graph_node_count,
                "runtime_source_bytes": stitch_pipeline.runtime_source_bytes,
                "runtime_library_compile_ms": stitch_pipeline.runtime_library_compile_ms,
                "runtime_pipeline_link_ms": stitch_pipeline.runtime_pipeline_link_ms,
            }))
        } else { None },
        "sdf_pipeline_resource_stats": {
            "thread_execution_width": stitch_pipeline.thread_execution_width,
            "max_total_threads_per_threadgroup": stitch_pipeline.max_total_threads_per_threadgroup,
            "static_threadgroup_memory_bytes": stitch_pipeline.static_threadgroup_memory_bytes,
        },
        "sdf_runtime_source_bytes": if config.sdf_topology_specialization != 0 || config.sdf_runtime_source_bytecode != 0 {
            Some(stitch_pipeline.runtime_source_bytes)
        } else { None },
        "sdf_runtime_library_compile_ms": if config.sdf_topology_specialization != 0 || config.sdf_runtime_source_bytecode != 0 {
            Some(stitch_pipeline.runtime_library_compile_ms)
        } else { None },
        "sdf_runtime_pipeline_link_ms": if config.sdf_topology_specialization != 0 || config.sdf_runtime_source_bytecode != 0 {
            Some(stitch_pipeline.runtime_pipeline_link_ms)
        } else { None },
        "sdf_backend_selection": backend_selection,
        "sdf_topology_build_ms": if config.sdf_topology_specialization != 0 { Some(build_ms) } else { None },
        "voxel_build_ms": if config.renderer_backend == RENDERER_VOXEL { Some(build_ms) } else { None },
        "voxel_resolution": config.voxel_resolution,
        "voxel_normal": match config.voxel_normal_mode {
            VOXEL_NORMAL_SMOOTH => "smooth",
            VOXEL_NORMAL_EXACT => "exact",
            _ => "face",
        },
        "voxel_storage": match config.voxel_storage {
            VOXEL_STORAGE_SPARSE_BRICKS => "sparse-bricks",
            VOXEL_STORAGE_TEMPLATE_BRICKS => "template-bricks",
            _ => "dense",
        },
        "voxel_coverage": match config.voxel_coverage_mode {
            VOXEL_COVERAGE_LIPSCHITZ => "lipschitz",
            VOXEL_COVERAGE_INTERVAL => "interval",
            _ => "legacy",
        },
        "voxel_build": if config.voxel_build_mode == VOXEL_BUILD_DIRECT { "direct" } else { "staging" },
        "voxel_leaf_refinement": match config.voxel_leaf_refinement {
            VOXEL_LEAF_REFINEMENT_SECANT_BISECTION => "secant-bisection",
            VOXEL_LEAF_REFINEMENT_RESTRICTED_TRACE => "restricted-trace",
            VOXEL_LEAF_REFINEMENT_FIXED_DE => "fixed-de",
            _ => "none",
        },
        "voxel_material": if config.voxel_material_mode == VOXEL_MATERIAL_EXACT { "exact" } else { "stored" },
        "voxel_offset": if config.voxel_offset_mode == VOXEL_OFFSET_PRECISION { "precision" } else { "legacy" },
        "acceleration_memory_bytes": voxel_memory_bytes,
        "acceleration_bounds_min": config.voxel_bounds_min,
        "acceleration_bounds_max": config.voxel_bounds_max,
        "voxel_memory_bytes": if config.renderer_backend == RENDERER_VOXEL { Some(voxel_memory_bytes) } else { None },
        "voxel_active_bricks": if config.renderer_backend == RENDERER_VOXEL { Some(voxel_active_bricks) } else { None },
        "voxel_active_cells": if config.renderer_backend == RENDERER_VOXEL { Some(voxel_active_cells) } else { None },
        "voxel_rejected_bricks": if config.renderer_backend == RENDERER_VOXEL { Some(voxel_rejected_bricks) } else { None },
        "bound_grid_resolution": config.bound_grid_resolution,
        "bound_grid_mode": if config.bound_grid_directional != 0 {
            "directional"
        } else {
            "range"
        },
        "bound_grid_format": if config.bound_grid_fp16 != 0 { "packed-fp16" } else { "fp32" },
        "bound_grid_build_ms": if config.renderer_backend == RENDERER_BOUND_GRID { Some(build_ms) } else { None },
        "bound_grid_memory_bytes": if config.renderer_backend == RENDERER_BOUND_GRID { Some(voxel_memory_bytes) } else { None },
        "regional_program_resolution": config.regional_program_resolution,
        "regional_program_lowering": if config.sdf_flat_union_count > 0 { "primitive-list" } else { "bytecode" },
        "regional_program_build_ms": if config.renderer_backend == RENDERER_REGIONAL { Some(build_ms) } else { None },
        "regional_program_memory_bytes": if config.renderer_backend == RENDERER_REGIONAL { Some(voxel_memory_bytes) } else { None },
        "regional_program_profile": if config.renderer_backend == RENDERER_REGIONAL && bound_grid_stats.regional_cells > 0 {
            Some(json!({
                "cells": bound_grid_stats.regional_cells,
                "fallback_cells": bound_grid_stats.regional_fallback_cells,
                "unique_programs": bound_grid_stats.regional_unique_programs,
                "source_instruction_count": config.sdf_program_count,
                "average_retained_operation_count": bound_grid_stats.regional_retained_instructions as f64 / bound_grid_stats.regional_cells as f64,
                "average_retained_instruction_count": if config.sdf_flat_union_count == 0 {
                    Some(bound_grid_stats.regional_retained_instructions as f64 / bound_grid_stats.regional_cells as f64)
                } else { None },
                "average_retained_primitive_count": if config.sdf_flat_union_count > 0 {
                    Some(bound_grid_stats.regional_retained_instructions as f64 / bound_grid_stats.regional_cells as f64)
                } else { None },
                "sampled_distance_failures": bound_grid_stats.regional_sampled_distance_failures,
                "proofs": if config.bound_grid_profile != 0 {
                    Some(json!({
                        "strict_dominance_pruned_cells": bound_grid_stats.regional_pruned_cells,
                        "pruned_primitive_branches": bound_grid_stats.regional_pruned_primitives,
                        "repeat_seam_fallback_cells": bound_grid_stats.regional_repeat_seam_fallback_cells,
                        "unsupported_fallback_cells": bound_grid_stats.regional_unsupported_fallback_cells,
                        "certified_without_dominance_cells": bound_grid_stats.regional_no_dominance_cells,
                    }))
                } else { None },
                "ray_weighted": if config.bound_grid_profile != 0 {
                    Some(json!({
                        "stride": config.bound_grid_profile_stride,
                        "ray_classes": ["primary", "shadow", "secondary"],
                        "profiled_paths": bound_grid_stats.regional_profiled_paths,
                        "distance_evaluations": bound_grid_stats.regional_distance_evaluations,
                        "atlas_evaluations": bound_grid_stats.regional_atlas_evaluations,
                        "full_program_fallback_evaluations": bound_grid_stats.regional_full_program_evaluations,
                        "cell_entries": bound_grid_stats.regional_cell_entries,
                        "same_cell_reuses": bound_grid_stats.regional_same_cell_reuses,
                        "same_program_reuses": bound_grid_stats.regional_same_program_reuses,
                        "program_id_loads": bound_grid_stats.regional_program_id_loads,
                        "header_loads": bound_grid_stats.regional_header_loads,
                        "dynamic_residual_operations": bound_grid_stats.regional_dynamic_instructions,
                        "dynamic_residual_instructions": if config.sdf_flat_union_count == 0 {
                            Some(bound_grid_stats.regional_dynamic_instructions)
                        } else { None },
                        "dynamic_residual_primitives": if config.sdf_flat_union_count > 0 {
                            Some(bound_grid_stats.regional_dynamic_instructions)
                        } else { None },
                    }))
                } else { None },
            }))
        } else { None },
        "bound_grid_profile": if config.renderer_backend == RENDERER_BOUND_GRID && config.bound_grid_profile != 0 {
            Some(json!({
                "stride": config.bound_grid_profile_stride,
                "cage_bounds": config.bound_grid_cage_bounds != 0,
                "ray_classes": ["primary", "shadow", "secondary"],
                "macro_cells": bound_grid_stats.macro_cells,
                "certified_skips": bound_grid_stats.certified_skips,
                "candidate_intervals": bound_grid_stats.candidate_intervals,
                "candidate_misses": bound_grid_stats.candidate_misses,
                "candidate_hits": bound_grid_stats.candidate_hits,
                "unknown_intervals": bound_grid_stats.unknown_intervals,
                "field_evaluations": bound_grid_stats.field_evaluations,
                "directional_steps": bound_grid_stats.directional_steps,
                "cell_exit_clamps": bound_grid_stats.cell_exit_clamps,
                "unknown_derivative_intervals": bound_grid_stats.unknown_derivative_intervals,
                "profiled_paths": bound_grid_stats.profiled_paths,
                "certified_cells": bound_grid_stats.certified_cells,
                "unknown_cells": bound_grid_stats.unknown_cells,
                "sampled_bound_failures": bound_grid_stats.sampled_bound_failures,
                "sampled_false_skips": bound_grid_stats.sampled_false_skips,
                "certified_derivative_cells": bound_grid_stats.certified_derivative_cells,
                "unknown_derivative_cells": bound_grid_stats.unknown_derivative_cells,
                "sampled_derivative_failures": bound_grid_stats.sampled_derivative_failures,
            }))
        } else { None },
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
    if args.sdf_function_stitching == SdfFunctionStitching::Auto && loaded.config.preview != 0 {
        bail!("automatic backend selection currently requires an offline render");
    }
    if args.sdf_function_stitching == SdfFunctionStitching::Auto && args.sdf_stitch_validation {
        bail!("automatic backend selection cannot be combined with program validation");
    }
    fs::create_dir_all(&args.out_dir)?;
    let output = output_path(args, &loaded.output_name);
    let base_metallib = metallib_path(args, &loaded.config)?;
    let stitch_metallib = default_stitch_metallib_path()?;
    let backend_selection = if args.sdf_function_stitching == SdfFunctionStitching::Auto {
        let selection_start = Instant::now();
        let (selected, mut selection) = select_sdf_backend(
            &loaded.config,
            &base_metallib,
            &stitch_metallib,
            args.sdf_backend_probe,
        )?;
        selection.selection_elapsed_ms = selection_start.elapsed().as_secs_f64() * 1000.0;
        loaded.config = selected;
        eprintln!(
            "SDF backend selector: {} via {} (direct {}, speedup {})",
            selection.selected,
            selection.decision_source,
            selection.direct_evaluator,
            selection
                .candidates
                .get(&selection.selected)
                .and_then(|candidate| candidate.speedup)
                .map(|speedup| format!("{speedup:.3}x"))
                .unwrap_or_else(|| "not measured".to_owned())
        );
        Some(selection)
    } else {
        None
    };
    let generated_source = loaded.runtime_metal_source.take();
    let use_cached_mandel = generated_source.is_some()
        && loaded.config.sdf_id == SDF_MANDELBULBER
        && loaded.config.renderer_backend == RENDERER_SDF
        && args.metallib.is_none();
    let mut mandel_artifacts = if use_cached_mandel {
        let source = generated_source.as_deref().expect("checked above");
        let force_o1 = std::env::var_os("FPT_MANDEL_FORCE_O1").is_some();
        let force_o0 = std::env::var_os("FPT_MANDEL_FORCE_O0").is_some()
            || (!force_o1 && mandel_scene_requires_o0(&args.scene_path)?);
        if force_o0 {
            Some(cached_mandel_render_artifacts(
                source,
                &loaded.config,
                MandelMetalOptimization::O0,
            )?)
        } else if force_o1 {
            Some(cached_mandel_render_artifacts(
                source,
                &loaded.config,
                MandelMetalOptimization::O1,
            )?)
        } else {
            let optimized = cached_mandel_render_artifacts(
                source,
                &loaded.config,
                MandelMetalOptimization::Default,
            )?;
            if optimized.fallback_marker.is_file() {
                Some(cached_mandel_render_artifacts(
                    source,
                    &loaded.config,
                    MandelMetalOptimization::O0,
                )?)
            } else {
                Some(optimized)
            }
        }
    } else {
        None
    };
    if mandel_artifacts.is_some() {
        loaded.config.sdf_runtime_source_bytecode = 0;
    }
    let mut stitch_archive = if let Some(artifacts) = mandel_artifacts.as_ref() {
        Some(artifacts.pipeline_archive.clone())
    } else {
        stitch_archive_path(&loaded.config)?
    };
    let stats = if let Some(artifacts) = mandel_artifacts.as_ref() {
        let initial_compile_ms = artifacts.compile_ms;
        let initial_pipeline_started = Instant::now();
        let result = execute_metal_render_internal(
            &loaded.config,
            &artifacts.metallib,
            &stitch_metallib,
            stitch_archive.as_deref(),
            &output,
            &[],
            None,
        );
        let initial_pipeline_attempt_ms = initial_pipeline_started.elapsed().as_secs_f64() * 1000.0;
        match result {
            Ok(mut stats) => {
                stats.build_ms += artifacts.compile_ms;
                stats
            }
            Err(error)
                if artifacts.optimization == MandelMetalOptimization::Default
                    && format!("{error:#}").contains("XPC_ERROR_CONNECTION_INTERRUPTED") =>
            {
                let source = generated_source.as_deref().expect("cached source exists");
                fs::write(
                    &artifacts.fallback_marker,
                    b"optimized Metal pipeline creation failed; use -O0\n",
                )?;
                eprintln!(
                    "optimized Mandel pipeline failed; retrying with cached -O0 fallback: {error:#}"
                );
                let fallback = cached_mandel_render_artifacts(
                    source,
                    &loaded.config,
                    MandelMetalOptimization::O0,
                )?;
                stitch_archive = Some(fallback.pipeline_archive.clone());
                let mut stats = execute_metal_render_internal(
                    &loaded.config,
                    &fallback.metallib,
                    &stitch_metallib,
                    stitch_archive.as_deref(),
                    &output,
                    &[],
                    None,
                )?;
                stats.build_ms +=
                    fallback.compile_ms + initial_compile_ms + initial_pipeline_attempt_ms;
                mandel_artifacts = Some(fallback);
                stats
            }
            Err(error) => return Err(error),
        }
    } else if let Some(shader_source) = generated_source.as_deref() {
        execute_metal_render_internal(
            &loaded.config,
            &base_metallib,
            &stitch_metallib,
            stitch_archive.as_deref(),
            &output,
            shader_source,
            None,
        )?
    } else {
        execute_metal_render(
            &loaded.config,
            &base_metallib,
            &stitch_metallib,
            stitch_archive.as_deref(),
            &output,
        )?
    };
    let stitch_cache_key = stitch_archive
        .as_ref()
        .and_then(|path| path.file_stem())
        .and_then(|stem| stem.to_str())
        .map(str::to_owned);
    if let Some(artifacts) = mandel_artifacts.as_ref() {
        eprintln!(
            "Mandel pipeline cache: {} {} source ({} bytes)",
            if artifacts.cache_hit {
                "hit"
            } else {
                "populated"
            },
            artifacts.optimization.diagnostic_label(),
            artifacts.source_bytes,
        );
    }
    let mandel_kernel_mode = if use_cached_mandel {
        Some(
            if mandelbulber::compiler::scene_uses_kernel_specialization(&args.scene_path)? {
                "specialized"
            } else {
                "generic-exact"
            },
        )
    } else {
        None
    };
    let mandel_compile_mode = mandel_artifacts
        .as_ref()
        .map(|artifacts| artifacts.optimization.metadata_label());
    let mandel_dispatch_label = if use_cached_mandel {
        Some(if std::env::var_os("FPT_MANDEL_TILED_DISPATCH").is_some() {
            format!("tiled-{}-row", mandel_tile_rows())
        } else {
            "single-command-buffer".to_owned()
        })
    } else {
        None
    };
    let mandel_formula_dispatch_mode = if use_cached_mandel {
        Some(
            if mandelbulber::compiler::scene_uses_direct_hybrid_loop(&args.scene_path)? {
                "direct-homogeneous"
            } else if mandelbulber::compiler::scene_formula_optimization_policy(&args.scene_path)?
                .unrolled_periodic_hybrid_loop
            {
                "unrolled-periodic-mixed"
            } else if mandelbulber::compiler::scene_formula_optimization_policy(&args.scene_path)?
                .periodic_hybrid_loop
            {
                "periodic-mixed"
            } else {
                "dynamic-or-standalone"
            },
        )
    } else {
        None
    };
    let mandel_cache_status = mandel_artifacts.as_ref().map(|artifacts| {
        if artifacts.cache_hit {
            "hit"
        } else {
            "populated"
        }
    });
    let mandel_source_bytes = mandel_artifacts
        .as_ref()
        .map(|artifacts| artifacts.source_bytes);
    let mandel_offline_compile_ms = mandel_artifacts
        .as_ref()
        .map(|artifacts| artifacts.compile_ms);
    write_render_metadata(RenderMetadataInput {
        output: &output,
        scene: &args.scene_path,
        config: &loaded.config,
        stats: &stats,
        stitch_cache_key: stitch_cache_key.as_deref(),
        backend_selection: backend_selection.as_ref(),
        mandel_kernel_mode,
        mandel_compile_mode,
        mandel_dispatch_mode: mandel_dispatch_label.as_deref(),
        mandel_formula_dispatch_mode,
        mandel_cache_status,
        mandel_source_bytes,
        mandel_offline_compile_ms,
    })?;
    eprintln!(
        "rendered {} -> {} (build {:.2} ms, render {:.2} ms)",
        args.scene_path.display(),
        output.display(),
        stats.build_ms,
        stats.elapsed_ms,
    );
    Ok(())
}

fn mandel_scene_requires_o0(scene: &Path) -> Result<bool> {
    // These corpus fixtures previously triggered Metal's optimized-pipeline
    // fallback on Apple Silicon. Re-enabling optimization after a shader
    // source change can alter their highly sensitive march decisions, even
    // when the generated field program is otherwise equivalent. Key the
    // conservative mode by content rather than path so renamed scenes retain
    // the exact renderer selected by the frozen compatibility corpus.
    const EXACT_O0_SCENES: [&str; 5] = [
        "dc67ed9066cac84403ed28cea04458bf332bdc4e5df5ef118bad9ebd541c49cd",
        "8bbd267428bb7fe674b4f84f549ff9af495850b27bcf12b118d7a27f187989d8",
        "7f40aeb3c2cd5ce44f59e62b7b6a3c3bbd0a36afd6aac629eca6f7194bb985d3",
        "03d8c4943e271e7b2d803108aec197d83ef894c4ba67b2ce15c54cdd4701109c",
        "c1f596cb79c5566ef078fb5b719e919e85dd597e085c612d0339640d237e2bd7",
    ];
    let digest = format!("{:x}", Sha256::digest(fs::read(scene)?));
    Ok(EXACT_O0_SCENES.contains(&digest.as_str()))
}

fn diagnostic(args: &RenderArgs) -> Result<()> {
    if args.sdf_function_stitching == SdfFunctionStitching::Auto {
        bail!("automatic backend selection is unavailable for diagnostic renders");
    }
    let mut loaded = load_scene_config(args)?;
    loaded.config.preview = 1;
    loaded.config.samples = args.samples.unwrap_or(1);
    apply_optimization_args(&mut loaded.config, args);
    fs::create_dir_all(&args.out_dir)?;
    let output = output_path(args, &loaded.output_name);
    let output_c = c_path(&output)?;
    let diagnostic = FptDiagnosticConfig {
        mode: args.diagnostic_mode as u32,
        _pad0: args.sdf_bounce_index,
        max_distance: args
            .diagnostic_max_distance
            .unwrap_or(loaded.config.render[4]),
        normal_mix: 1.0,
        dispatch_origin: [0, 0],
    };
    let mut elapsed_ms = 0.0;
    let mut error = [0_i8; 4096];
    let diagnostic_shader_source = loaded
        .runtime_metal_source
        .as_deref()
        .map(|source| {
            let source = std::str::from_utf8(source)?;
            let retained = if loaded.config.renderer_backend == RENDERER_VOXEL {
                &["voxel_build_kernel", "voxel_diagnostic_kernel"][..]
            } else {
                &["sdf_diagnostic_kernel"][..]
            };
            mandelbulber::compiler::retain_metal_kernels(source, retained)
        })
        .transpose()?;
    let production_compile = std::env::var_os("FPT_MANDEL_DIAGNOSTIC_PRODUCTION_COMPILE").is_some();
    let diagnostic_optimization = if std::env::var_os("FPT_MANDEL_DIAGNOSTIC_O1").is_some() {
        MandelMetalOptimization::O1
    } else if std::env::var_os("FPT_MANDEL_DIAGNOSTIC_OPTIMIZED").is_some()
        || (production_compile && !mandel_scene_requires_o0(&args.scene_path)?)
    {
        MandelMetalOptimization::Default
    } else {
        MandelMetalOptimization::O0
    };
    let diagnostic_metallib = diagnostic_shader_source
        .as_deref()
        .map(|source| cached_diagnostic_metallib(source, diagnostic_optimization))
        .transpose()?;
    let diagnostic_formula_dispatch_mode = if diagnostic_shader_source.is_some() {
        Some(
            if mandelbulber::compiler::scene_uses_direct_hybrid_loop(&args.scene_path)? {
                "direct-homogeneous"
            } else if mandelbulber::compiler::scene_formula_optimization_policy(&args.scene_path)?
                .unrolled_periodic_hybrid_loop
            {
                "unrolled-periodic-mixed"
            } else if mandelbulber::compiler::scene_formula_optimization_policy(&args.scene_path)?
                .periodic_hybrid_loop
            {
                "periodic-mixed"
            } else {
                "dynamic-or-standalone"
            },
        )
    } else {
        None
    };
    let metallib = diagnostic_metallib.unwrap_or(metallib_path(args, &loaded.config)?);
    let metallib_c = c_path(&metallib)?;
    let status = unsafe {
        fpt_metal_diagnostic_render(
            metallib_c.as_ptr(),
            output_c.as_ptr(),
            std::ptr::null(),
            0,
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
    let stats = MetalRenderStats {
        elapsed_ms,
        ..MetalRenderStats::default()
    };
    write_render_metadata(RenderMetadataInput {
        output: &output,
        scene: &args.scene_path,
        config: &loaded.config,
        stats: &stats,
        stitch_cache_key: None,
        backend_selection: None,
        mandel_kernel_mode: None,
        mandel_compile_mode: diagnostic_shader_source
            .as_ref()
            .map(|_| diagnostic_optimization.diagnostic_label()),
        mandel_dispatch_mode: None,
        mandel_formula_dispatch_mode: diagnostic_formula_dispatch_mode,
        mandel_cache_status: None,
        mandel_source_bytes: None,
        mandel_offline_compile_ms: None,
    })?;
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
    if args.sdf_function_stitching == SdfFunctionStitching::Auto && args.live_pathtrace {
        bail!("automatic backend selection currently requires viewport preview mode");
    }
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
    // Preview can cycle between built-in and typed-program scenes, so it keeps
    // the full-capacity library unless the caller explicitly supplies one.
    let preview_metallib = args
        .metallib
        .clone()
        .map(Ok)
        .unwrap_or_else(default_metallib_path)?;
    let metallib_c = c_path(&preview_metallib)?;
    let stitch_metallib_c = c_path(&default_stitch_metallib_path()?)?;
    let stitch_archive_paths = scenes
        .iter()
        .map(stitch_archive_path)
        .collect::<Result<Vec<_>>>()?;
    let stitch_archive_c = stitch_archive_paths
        .iter()
        .map(|path| c_path(path.as_deref().unwrap_or_else(|| Path::new(""))))
        .collect::<Result<Vec<_>>>()?;
    let stitch_archive_ptrs = stitch_archive_c
        .iter()
        .map(|path| path.as_ptr())
        .collect::<Vec<_>>();
    let mut error = [0_i8; 4096];
    let preview_shader_source = loaded
        .runtime_metal_source
        .as_deref()
        .unwrap_or(METAL_SOURCE_BYTES);
    let status = unsafe {
        fpt_metal_preview(
            metallib_c.as_ptr(),
            stitch_metallib_c.as_ptr(),
            stitch_archive_ptrs.as_ptr(),
            preview_shader_source.as_ptr().cast(),
            preview_shader_source.len(),
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

fn render_batch(args: &[String]) -> Result<()> {
    if args.len() != 1 {
        bail!("render-batch expects one JSON file containing arrays of render arguments");
    }
    let jobs: Vec<Vec<String>> = serde_json::from_slice(&fs::read(&args[0])?)
        .context("render-batch manifest must be an array of argument arrays")?;
    for (index, job) in jobs.iter().enumerate() {
        eprintln!("render-batch job {}/{}", index + 1, jobs.len());
        render(
            &parse_render_args(job)
                .with_context(|| format!("invalid render-batch job {}", index + 1))?,
        )?;
    }
    Ok(())
}

fn mandel_benchmark_image_health(path: &Path) -> Result<MandelBenchmarkImageHealth> {
    let image = image::open(path)
        .with_context(|| format!("open benchmark image {}", path.display()))?
        .to_rgb8();
    let (width, height) = image.dimensions();
    let pixels = image.as_raw().chunks_exact(3);
    let count = pixels.len().max(1) as f64;
    let mut sum = 0.0;
    let mut sum_squared = 0.0;
    let mut non_black = 0usize;
    for pixel in pixels {
        let luminance = (0.2126 * f64::from(pixel[0])
            + 0.7152 * f64::from(pixel[1])
            + 0.0722 * f64::from(pixel[2]))
            / 255.0;
        sum += luminance;
        sum_squared += luminance * luminance;
        non_black += usize::from(pixel.iter().copied().max().unwrap_or(0) > 2);
    }
    let mean_luminance = sum / count;
    let variance = (sum_squared / count - mean_luminance * mean_luminance).max(0.0);
    Ok(MandelBenchmarkImageHealth {
        width,
        height,
        mean_luminance,
        luminance_stddev: variance.sqrt(),
        non_black_fraction: non_black as f64 / count,
    })
}

fn compile_mandel_metallib(
    source: &[u8],
    directory: &Path,
    label: &str,
    optimization: MandelMetalOptimization,
) -> Result<(PathBuf, f64)> {
    fs::create_dir_all(directory)?;
    let metal_path = directory.join(format!("{label}.metal"));
    let air_path = directory.join(format!("{label}.air"));
    let metallib_path = directory.join(format!("{label}.metallib"));
    fs::write(&metal_path, source)?;
    let started = Instant::now();
    let mut compile_command = Command::new("xcrun");
    compile_command.args(["-sdk", "macosx", "metal", "-std=macos-metal2.4"]);
    optimization.apply(&mut compile_command);
    let compile = compile_command
        .args(["-ffast-math", "-c"])
        .arg(&metal_path)
        .arg("-o")
        .arg(&air_path)
        .output()
        .context("launch offline Metal compiler")?;
    if !compile.status.success() {
        bail!(
            "offline Metal compilation failed:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );
    }
    let link = Command::new("xcrun")
        .args(["-sdk", "macosx", "metallib"])
        .arg(&air_path)
        .arg("-o")
        .arg(&metallib_path)
        .output()
        .context("launch Metal library linker")?;
    if !link.status.success() {
        bail!(
            "offline Metal library link failed:\n{}",
            String::from_utf8_lossy(&link.stderr)
        );
    }
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    let _ = fs::remove_file(&metal_path);
    let _ = fs::remove_file(&air_path);
    Ok((metallib_path, elapsed_ms))
}

fn mandel_render_kernel_names(config: &FptRenderConfig) -> Vec<&'static str> {
    if config.preview != 0 {
        return vec!["preview_linear_kernel", "present_kernel"];
    }
    let tiled_batch = std::env::var_os("FPT_MANDEL_TILED_DISPATCH").is_some()
        && effective_accumulation(config) == "batch";
    let mut kernels = vec![
        if tiled_batch {
            "accumulate_all_tile_kernel"
        } else {
            match effective_accumulation(config) {
                "per-sample" => "accumulate_kernel",
                "chunked" => "accumulate_chunk_kernel",
                _ => "accumulate_all_kernel",
            }
        },
        "present_kernel",
    ];
    if config.focus_distance <= 0.0 {
        kernels.push("estimate_focus_distance_kernel");
    }
    if config.sdf_profile != 0 {
        kernels.push("sdf_profile_kernel");
    }
    kernels
}

fn mandel_render_cache_directory() -> PathBuf {
    std::env::var_os("FPT_MANDEL_RENDER_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir()
                .join("fpt-metal")
                .join("mandel-render-v1")
        })
}

fn cached_mandel_render_artifacts(
    generated_source: &[u8],
    config: &FptRenderConfig,
    optimization: MandelMetalOptimization,
) -> Result<CachedMandelRenderArtifacts> {
    let retained = mandelbulber::compiler::retain_metal_kernels(
        std::str::from_utf8(generated_source)?,
        &mandel_render_kernel_names(config),
    )?;
    let mut base_digest = Sha256::new();
    base_digest.update(b"fpt-mandel-render-pipeline-v1\0");
    base_digest.update(METAL_COMPILER_IDENTITY.as_bytes());
    base_digest.update(std::env::consts::ARCH.as_bytes());
    base_digest.update(effective_accumulation(config).as_bytes());
    base_digest.update(b"fast-math\0");
    base_digest.update(retained.as_bytes());
    let base_key = format!("{:x}", base_digest.finalize());
    let directory = mandel_render_cache_directory();
    fs::create_dir_all(&directory)?;
    let fallback_marker = directory.join(format!("{base_key}.requires-o0"));
    let mut mode_digest = Sha256::new();
    mode_digest.update(base_key.as_bytes());
    mode_digest.update(optimization.cache_tag());
    let key = format!("{:x}", mode_digest.finalize());
    let metallib = directory.join(format!("{key}.metallib"));
    let pipeline_archive = directory.join(format!("{key}.metallibarchive"));
    let cache_hit = metallib.is_file();
    let compile_ms = if cache_hit {
        0.0
    } else {
        let unique = format!(
            "{key}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let (temporary, elapsed_ms) =
            compile_mandel_metallib(retained.as_bytes(), &directory, &unique, optimization)?;
        if metallib.is_file() {
            let _ = fs::remove_file(&temporary);
        } else {
            fs::rename(&temporary, &metallib)?;
        }
        elapsed_ms
    };
    Ok(CachedMandelRenderArtifacts {
        metallib,
        pipeline_archive,
        compile_ms,
        cache_hit,
        optimization,
        source_bytes: retained.len(),
        fallback_marker,
    })
}

fn mandel_benchmark_summary(entries: &[MandelBenchmarkEntry]) -> MandelBenchmarkSummary {
    let successful = entries.iter().filter(|entry| entry.status == "ok").count();
    let mut generation = entries
        .iter()
        .filter_map(|entry| entry.generation_ms)
        .collect::<Vec<_>>();
    let mut cold_build = entries
        .iter()
        .filter_map(|entry| entry.cold_build_ms)
        .collect::<Vec<_>>();
    let mut cold_render = entries
        .iter()
        .filter_map(|entry| entry.cold_render_ms)
        .collect::<Vec<_>>();
    let warm_build = entries
        .iter()
        .flat_map(|entry| entry.warm_build_ms.iter().copied())
        .collect::<Vec<_>>();
    let warm_render = entries
        .iter()
        .flat_map(|entry| entry.warm_render_ms.iter().copied())
        .collect::<Vec<_>>();
    let first_total = entries
        .iter()
        .filter_map(|entry| entry.first_render_total_ms)
        .collect::<Vec<_>>();
    let cold_execution_wall = entries
        .iter()
        .filter_map(|entry| entry.cold_execution_wall_ms)
        .collect::<Vec<_>>();
    let warm_execution_wall = entries
        .iter()
        .flat_map(|entry| entry.warm_execution_wall_ms.iter().copied())
        .collect::<Vec<_>>();
    let generation_median_ms = median(generation.clone());
    let cold_build_median_ms = median(cold_build.clone());
    let cold_build_p90_ms = percentile(&mut cold_build, 0.9);
    let cold_render_median_ms = median(cold_render.clone());
    let cold_render_p90_ms = percentile(&mut cold_render, 0.9);
    generation.clear();
    cold_render.clear();
    MandelBenchmarkSummary {
        successful,
        failed: entries.len() - successful,
        generation_median_ms,
        cold_build_median_ms,
        cold_build_p90_ms,
        cold_render_median_ms,
        cold_render_p90_ms,
        warm_build_median_ms: median(warm_build),
        warm_render_median_ms: median(warm_render),
        cold_execution_wall_median_ms: median(cold_execution_wall),
        warm_execution_wall_median_ms: median(warm_execution_wall),
        first_render_total_median_ms: median(first_total),
        blank_or_near_blank_images: entries
            .iter()
            .filter_map(|entry| entry.image_health.as_ref())
            .filter(|health| health.non_black_fraction < 0.001 || health.luminance_stddev < 0.001)
            .count(),
    }
}

fn mandel_benchmark(args: &[String]) -> Result<()> {
    let root = args
        .first()
        .ok_or_else(|| anyhow!("mandel-benchmark requires a Mandelbulber source directory"))?;
    let root = PathBuf::from(root);
    let mut report_path = PathBuf::from("reports/mandel-benchmark/report.json");
    let mut output_dir = PathBuf::from("reports/mandel-benchmark/images");
    let mut kernel_set = "full".to_owned();
    let mut offset = 0usize;
    let mut limit = None::<usize>;
    let mut stride = 1usize;
    let mut width = 120u32;
    let mut height = 68u32;
    let mut samples = 1u32;
    let mut warm_runs = 1usize;
    let mut index = 1usize;
    while index < args.len() {
        match args[index].as_str() {
            "--report" => {
                index += 1;
                report_path = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--report requires a path"))?
                    .into();
            }
            "--out" => {
                index += 1;
                output_dir = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--out requires a directory"))?
                    .into();
            }
            "--kernel-set" => {
                index += 1;
                kernel_set = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--kernel-set requires full or render"))?
                    .to_owned();
                ensure!(
                    matches!(
                        kernel_set.as_str(),
                        "full" | "render" | "offline-render" | "offline-render-o0"
                    ),
                    "--kernel-set must be full, render, offline-render, or offline-render-o0"
                );
            }
            "--offset" => {
                index += 1;
                offset = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--offset requires a value"))?
                    .parse()?;
            }
            "--limit" => {
                index += 1;
                limit = Some(
                    args.get(index)
                        .ok_or_else(|| anyhow!("--limit requires a value"))?
                        .parse()?,
                );
            }
            "--stride" => {
                index += 1;
                stride = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--stride requires a value"))?
                    .parse()?;
                ensure!(stride > 0, "--stride must be positive");
            }
            "--width" => {
                index += 1;
                width = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--width requires a value"))?
                    .parse()?;
            }
            "--height" => {
                index += 1;
                height = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--height requires a value"))?
                    .parse()?;
            }
            "--samples" => {
                index += 1;
                samples = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--samples requires a value"))?
                    .parse()?;
            }
            "--warm-runs" => {
                index += 1;
                warm_runs = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--warm-runs requires a value"))?
                    .parse()?;
            }
            value => bail!("unknown mandel-benchmark argument: {value}"),
        }
        index += 1;
    }
    ensure!((16..=4096).contains(&width), "--width must be 16..4096");
    ensure!((16..=4096).contains(&height), "--height must be 16..4096");
    ensure!((1..=512).contains(&samples), "--samples must be 1..512");
    ensure!(warm_runs <= 10, "--warm-runs must be 0..10");

    let paths = mandelbulber::catalog::example_scene_paths(&root)?;
    ensure!(offset <= paths.len(), "--offset exceeds the scene corpus");
    let selected = paths
        .iter()
        .enumerate()
        .skip(offset)
        .step_by(stride)
        .take(limit.unwrap_or(usize::MAX))
        .collect::<Vec<_>>();
    fs::create_dir_all(&output_dir)?;
    let benchmark_started = Instant::now();
    let benchmark_nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let base_metallib = default_builtin_metallib_path()?;
    let stitch_metallib = default_stitch_metallib_path()?;
    let mut entries = Vec::with_capacity(selected.len());

    for (selected_index, (corpus_index, path)) in selected.iter().enumerate() {
        eprintln!(
            "Mandel benchmark {}/{}: {}",
            selected_index + 1,
            selected.len(),
            path.display()
        );
        let relative_path = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .display()
            .to_string();
        let parsed_scene = MandelbulberScene::load(path);
        let (hybrid, boolean, delta_de, formula_ids) = parsed_scene
            .as_ref()
            .map(|scene| {
                (
                    Some(scene.hybrid_enabled),
                    Some(scene.boolean_enabled),
                    Some(scene.force_delta_de),
                    scene
                        .formula_slots
                        .iter()
                        .filter(|slot| slot.active())
                        .map(|slot| slot.formula_id)
                        .collect::<Vec<_>>(),
                )
            })
            .unwrap_or((None, None, None, Vec::new()));
        let run = (|| -> Result<_> {
            parsed_scene?;
            let entry_dir = output_dir.join(format!("{:04}", corpus_index + 1));
            let render_arguments = vec![
                path.display().to_string(),
                "--out".to_owned(),
                entry_dir.display().to_string(),
                "--renderer".to_owned(),
                "sdf".to_owned(),
                "--mandelbulber-root".to_owned(),
                root.display().to_string(),
                "--width".to_owned(),
                width.to_string(),
                "--height".to_owned(),
                height.to_string(),
                "--samples".to_owned(),
                samples.to_string(),
            ];
            let render_args = parse_render_args(&render_arguments)?;
            let generation_started = Instant::now();
            let mut loaded = load_scene_config(&render_args)?;
            let generation_ms = generation_started.elapsed().as_secs_f64() * 1000.0;
            let generated_source = loaded
                .runtime_metal_source
                .take()
                .ok_or_else(|| anyhow!("scene did not produce runtime Metal source"))?;
            let generated_source_bytes = generated_source.len();
            let mut source = if matches!(
                kernel_set.as_str(),
                "render" | "offline-render" | "offline-render-o0"
            ) {
                let retained_kernel = if samples <= 16 {
                    "accumulate_all_kernel"
                } else {
                    "accumulate_chunk_kernel"
                };
                mandelbulber::compiler::retain_metal_kernels(
                    std::str::from_utf8(&generated_source)?,
                    &[retained_kernel, "present_kernel"],
                )?
                .into_bytes()
            } else {
                generated_source
            };
            source.extend_from_slice(
                format!(
                    "\n// FPT_MANDEL_BENCHMARK_NONCE: {benchmark_nonce}:{}\n",
                    corpus_index + 1
                )
                .as_bytes(),
            );
            let source_bytes = source.len();
            fs::create_dir_all(&entry_dir)?;
            let output = entry_dir.join(&loaded.output_name);
            let offline_metallib =
                if matches!(kernel_set.as_str(), "offline-render" | "offline-render-o0") {
                    Some(compile_mandel_metallib(
                        &source,
                        &output_dir.join("metallibs"),
                        &format!("scene-{:04}", corpus_index + 1),
                        if kernel_set == "offline-render" {
                            MandelMetalOptimization::Default
                        } else {
                            MandelMetalOptimization::O0
                        },
                    )?)
                } else {
                    None
                };
            let mut execution_config = loaded.config;
            if offline_metallib.is_some() {
                execution_config.sdf_runtime_source_bytecode = 0;
            }
            let pipeline_archive = offline_metallib.as_ref().map(|_| {
                output_dir
                    .join("archives")
                    .join(format!("scene-{:04}.metallibarchive", corpus_index + 1))
            });
            if let Some(parent) = pipeline_archive.as_ref().and_then(|path| path.parent()) {
                fs::create_dir_all(parent)?;
            }
            let (execution_metallib, execution_source) = offline_metallib
                .as_ref()
                .map_or((&base_metallib, source.as_slice()), |(path, _)| {
                    (path, &[][..])
                });
            let cold_execution_started = Instant::now();
            let mut cold = execute_metal_render_internal(
                &execution_config,
                execution_metallib,
                &stitch_metallib,
                pipeline_archive.as_deref(),
                &output,
                execution_source,
                None,
            )?;
            let cold_execution_wall_ms = cold_execution_started.elapsed().as_secs_f64() * 1000.0;
            let offline_compile_ms = offline_metallib
                .as_ref()
                .map_or(0.0, |(_, compile_ms)| *compile_ms);
            if let Some((_, compile_ms)) = offline_metallib.as_ref() {
                cold.build_ms = *compile_ms;
            }
            let mut warm_build_ms = Vec::with_capacity(warm_runs);
            let mut warm_render_ms = Vec::with_capacity(warm_runs);
            let mut warm_execution_wall_ms = Vec::with_capacity(warm_runs);
            for _ in 0..warm_runs {
                let warm_execution_started = Instant::now();
                let warm = execute_metal_render_internal(
                    &execution_config,
                    execution_metallib,
                    &stitch_metallib,
                    pipeline_archive.as_deref(),
                    &output,
                    execution_source,
                    None,
                )?;
                warm_execution_wall_ms
                    .push(warm_execution_started.elapsed().as_secs_f64() * 1000.0);
                warm_build_ms.push(warm.build_ms);
                warm_render_ms.push(warm.elapsed_ms);
            }
            let image_health = mandel_benchmark_image_health(&output)?;
            Ok((
                generated_source_bytes,
                source_bytes,
                generation_ms,
                cold,
                warm_build_ms,
                warm_render_ms,
                cold_execution_wall_ms,
                warm_execution_wall_ms,
                offline_compile_ms,
                output,
                image_health,
            ))
        })();
        let entry = match run {
            Ok((
                generated_source_bytes,
                source_bytes,
                generation_ms,
                cold,
                warm_build_ms,
                warm_render_ms,
                cold_execution_wall_ms,
                warm_execution_wall_ms,
                offline_compile_ms,
                output,
                image_health,
            )) => MandelBenchmarkEntry {
                corpus_index: corpus_index + 1,
                path: relative_path,
                hybrid,
                boolean,
                delta_de,
                formula_ids,
                status: "ok".to_owned(),
                source_bytes: Some(source_bytes),
                generated_source_bytes: Some(generated_source_bytes),
                generation_ms: Some(generation_ms),
                cold_build_ms: Some(cold.build_ms),
                warm_build_ms,
                cold_render_ms: Some(cold.elapsed_ms),
                warm_render_ms,
                cold_execution_wall_ms: Some(cold_execution_wall_ms),
                warm_execution_wall_ms,
                first_render_total_ms: Some(
                    generation_ms + offline_compile_ms + cold_execution_wall_ms,
                ),
                output: Some(output),
                image_health: Some(image_health),
                error: None,
            },
            Err(error) => {
                eprintln!("Mandel benchmark scene failed: {error:#}");
                MandelBenchmarkEntry {
                    corpus_index: corpus_index + 1,
                    path: relative_path,
                    hybrid,
                    boolean,
                    delta_de,
                    formula_ids,
                    status: "failed".to_owned(),
                    source_bytes: None,
                    generated_source_bytes: None,
                    generation_ms: None,
                    cold_build_ms: None,
                    warm_build_ms: Vec::new(),
                    cold_render_ms: None,
                    warm_render_ms: Vec::new(),
                    cold_execution_wall_ms: None,
                    warm_execution_wall_ms: Vec::new(),
                    first_render_total_ms: None,
                    output: None,
                    image_health: None,
                    error: Some(format!("{error:#}")),
                }
            }
        };
        entries.push(entry);
    }
    let summary = mandel_benchmark_summary(&entries);
    let report = MandelBenchmarkReport {
        schema_version: 1,
        renderer_revision: UPSTREAM_SHA.to_owned(),
        mandelbulber_root: root,
        metal_device: metal_device_name(),
        kernel_set,
        corpus_scenes: paths.len(),
        selected_scenes: entries.len(),
        offset,
        stride,
        width,
        height,
        samples,
        warm_runs,
        elapsed_ms: benchmark_started.elapsed().as_secs_f64() * 1000.0,
        summary,
        entries,
    };
    if let Some(parent) = report_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &report_path,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    eprintln!(
        "Mandel benchmark: {}/{} passed; cold build median {:.2} ms, render median {:.2} ms; wrote {}",
        report.summary.successful,
        report.selected_scenes,
        report.summary.cold_build_median_ms,
        report.summary.cold_render_median_ms,
        report_path.display()
    );
    ensure!(
        report.summary.failed == 0,
        "benchmark contains failed scenes"
    );
    Ok(())
}

fn diagnostic_batch(args: &[String]) -> Result<()> {
    let jobs_path = args
        .first()
        .ok_or_else(|| anyhow!("diagnostic-batch expects a JSON job manifest"))?;
    let mut report_path = PathBuf::from("reports/diagnostic-batch.json");
    let mut workers = 1usize;
    let mut offset = 0usize;
    let mut limit = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--report" => {
                index += 1;
                report_path = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--report requires a path"))?
                    .into();
            }
            "--workers" => {
                index += 1;
                workers = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--workers requires a value"))?
                    .parse()?;
                ensure!((1..=32).contains(&workers), "--workers must be 1..32");
            }
            "--offset" => {
                index += 1;
                offset = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--offset requires a value"))?
                    .parse()?;
            }
            "--limit" => {
                index += 1;
                limit = Some(
                    args.get(index)
                        .ok_or_else(|| anyhow!("--limit requires a value"))?
                        .parse::<usize>()?,
                );
            }
            value => bail!("unknown diagnostic-batch argument: {value}"),
        }
        index += 1;
    }
    let jobs: Vec<Vec<String>> = serde_json::from_slice(&fs::read(jobs_path)?)
        .context("diagnostic-batch manifest must be an array of argument arrays")?;
    ensure!(offset <= jobs.len(), "--offset exceeds manifest job count");
    let end = limit
        .map(|count| offset.saturating_add(count).min(jobs.len()))
        .unwrap_or(jobs.len());
    let selected_jobs = end - offset;
    let batch_started = Instant::now();
    let next_job = AtomicUsize::new(0);
    let entries = Mutex::new(Vec::with_capacity(selected_jobs));
    std::thread::scope(|scope| {
        for _ in 0..workers.min(selected_jobs.max(1)) {
            scope.spawn(|| {
                loop {
                    let local_index = next_job.fetch_add(1, Ordering::Relaxed);
                    if local_index >= selected_jobs {
                        break;
                    }
                    let job_index = offset + local_index;
                    let Some(job) = jobs.get(job_index) else {
                        break;
                    };
                    eprintln!("diagnostic-batch job {}/{}", job_index + 1, jobs.len());
                    let job_started = Instant::now();
                    let result = parse_render_args(job)
                        .with_context(|| format!("invalid diagnostic-batch job {}", job_index + 1))
                        .and_then(|parsed| diagnostic(&parsed));
                    let elapsed_ms = job_started.elapsed().as_secs_f64() * 1000.0;
                    let entry = match result {
                        Ok(()) => json!({
                            "index": job_index + 1,
                            "scene": job.first(),
                            "status": "ok",
                            "elapsed_ms": elapsed_ms,
                            "error": null,
                        }),
                        Err(error) => {
                            eprintln!("diagnostic-batch job {} failed: {error:#}", job_index + 1);
                            json!({
                                "index": job_index + 1,
                                "scene": job.first(),
                                "status": "failed",
                                "elapsed_ms": elapsed_ms,
                                "error": format!("{error:#}"),
                            })
                        }
                    };
                    entries
                        .lock()
                        .expect("diagnostic entries mutex")
                        .push(entry);
                }
            });
        }
    });
    let mut entries = entries.into_inner().expect("diagnostic entries mutex");
    entries.sort_by_key(|entry| entry.get("index").and_then(Value::as_u64));
    let passed = entries
        .iter()
        .filter(|entry| entry.get("status").and_then(Value::as_str) == Some("ok"))
        .count();
    let failed = selected_jobs - passed;
    let report = json!({
        "schema_version": 1,
        "manifest_jobs": jobs.len(),
        "offset": offset,
        "end": end,
        "jobs": selected_jobs,
        "passed": passed,
        "failed": failed,
        "workers": workers,
        "elapsed_ms": batch_started.elapsed().as_secs_f64() * 1000.0,
        "entries": entries,
    });
    if let Some(parent) = report_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &report_path,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    eprintln!(
        "diagnostic-batch: {passed}/{} passed; wrote {}",
        selected_jobs,
        report_path.display()
    );
    ensure!(failed == 0, "{failed} diagnostic-batch jobs failed");
    Ok(())
}

fn parity_hash(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^ (value >> 16)
}

fn percentile(values: &mut [f64], fraction: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let index = ((values.len() - 1) as f64 * fraction).round() as usize;
    values[index]
}

fn mandel_parity(args: &[String]) -> Result<()> {
    let scene_path = PathBuf::from(
        args.first()
            .ok_or_else(|| anyhow!("mandel-parity requires a .fract scene"))?,
    );
    let mut report_path = PathBuf::from("reports/mandel/field-parity.json");
    let mut sample_count = 65_536usize;
    let mut source_root = None::<PathBuf>;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--report" => {
                index += 1;
                report_path = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--report requires a path"))?
                    .into();
            }
            "--samples" => {
                index += 1;
                sample_count = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--samples requires a value"))?
                    .parse()?;
            }
            "--mandelbulber-root" => {
                index += 1;
                source_root = Some(
                    args.get(index)
                        .ok_or_else(|| anyhow!("--mandelbulber-root requires a directory"))?
                        .into(),
                );
            }
            value => bail!("unknown mandel-parity argument: {value}"),
        }
        index += 1;
    }
    if !(16..=1_048_576).contains(&sample_count) {
        bail!("mandel-parity samples must be 16..1048576");
    }

    let mut scene = MandelbulberScene::load(&scene_path)?;
    ensure!(
        scene.has_cpu_reference(),
        "formula ID {} has no independent CPU parity reference yet",
        scene.formula_id
    );
    let runtime_formulas = if let Some(source_root) = &source_root {
        let mut formulas = Vec::new();
        for (index, slot) in scene.formula_slots.iter().enumerate() {
            if !slot.active() {
                continue;
            }
            let formula =
                mandelbulber::compiler::parse_formula(source_root, &slot.formula_id.to_string())?;
            ensure!(
                if scene.hybrid_enabled {
                    mandelbulber::compiler::supports_runtime_kernel(&formula)
                } else {
                    mandelbulber::compiler::supports_runtime_emitter(&formula)
                },
                "formula slot {} ({}) is not supported by the generated runtime",
                index + 1,
                formula.source.symbol
            );
            formulas.push((index, formula));
        }
        let sources = formulas
            .iter()
            .map(|(index, formula)| (*index, &formula.source))
            .collect::<Vec<_>>();
        scene.configure_formula_slots(&sources)?;
        formulas
    } else {
        ensure!(
            !scene.hybrid_enabled,
            "hybrid parity requires --mandelbulber-root"
        );
        Vec::new()
    };
    let mut config = default_config();
    scene.apply_to_config(&mut config);
    let generated_source = if scene.hybrid_enabled {
        let sequence = scene.hybrid_sequence()?;
        let slots = runtime_formulas
            .iter()
            .map(
                |(index, formula)| mandelbulber::compiler::RuntimeFormulaSlot {
                    index: *index,
                    formula,
                    formula_values: &scene.formula_slots[*index].parameters,
                    iterations: scene.formula_slots[*index].iterations,
                    weight: scene.formula_slots[*index].weight,
                    add_c_constant: scene.formula_slots[*index].add_c_constant,
                    check_for_bailout: scene.formula_slots[*index].check_for_bailout,
                    bailout: scene.formula_slots[*index].bailout,
                },
            )
            .collect::<Vec<_>>();
        Some(mandelbulber::compiler::specialize_fpt_shader_hybrid(
            std::str::from_utf8(METAL_SOURCE_BYTES)?,
            &slots,
            &sequence,
            &config.set_values,
            scene.linear_de_offset,
            scene.force_delta_de,
            scene.delta_de_function,
            false,
            mandelbulber::compiler::SceneFormulaOptimizationPolicy::default(),
        )?)
    } else if let Some((_, formula)) = runtime_formulas.first() {
        Some(mandelbulber::compiler::specialize_fpt_shader(
            std::str::from_utf8(METAL_SOURCE_BYTES)?,
            formula,
            &config.set_values,
            &scene.formula_parameters,
            scene.force_delta_de,
            scene.delta_de_function,
        )?)
    } else {
        None
    };
    let world_scale = f64::from(config.set_values[mandelbulber::PARAM_WORLD_SCALE]);
    let mut points = vec![[0.0f32; 4]; sample_count];
    let mut cpu_points = vec![[0.0f64; 3]; sample_count];
    let fixed = [
        [0.0, 0.0, 0.0],
        scene.camera,
        scene.target,
        [
            (scene.camera[0] + scene.target[0]) * 0.5,
            (scene.camera[1] + scene.target[1]) * 0.5,
            (scene.camera[2] + scene.target[2]) * 0.5,
        ],
        [-1.0, -1.0, -1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, -1.0],
        [1.0, -1.0, 1.0],
    ];
    for ((point, cpu_point), source) in points.iter_mut().zip(&mut cpu_points).zip(fixed) {
        point[..3].copy_from_slice(&[
            (source[0] * world_scale) as f32,
            (source[2] * world_scale) as f32,
            (source[1] * world_scale) as f32,
        ]);
        *cpu_point = [
            f64::from(point[0]) / world_scale,
            f64::from(point[2]) / world_scale,
            f64::from(point[1]) / world_scale,
        ];
    }
    for (sample_index, (point, cpu_point)) in points
        .iter_mut()
        .zip(&mut cpu_points)
        .enumerate()
        .skip(fixed.len())
    {
        for axis in 0..3 {
            let hash = parity_hash(
                (sample_index as u32)
                    .wrapping_mul(3)
                    .wrapping_add(axis as u32)
                    .wrapping_add([0x9e37_79b9, 0x243f_6a88, 0xb7e1_5162][axis]),
            );
            cpu_point[axis] = hash as f64 * (4.0 / u32::MAX as f64) - 2.0;
        }
        point[..3].copy_from_slice(&[
            (cpu_point[0] * world_scale) as f32,
            (cpu_point[2] * world_scale) as f32,
            (cpu_point[1] * world_scale) as f32,
        ]);
        *cpu_point = [
            f64::from(point[0]) / world_scale,
            f64::from(point[2]) / world_scale,
            f64::from(point[1]) / world_scale,
        ];
    }

    let metallib = default_metallib_path()?;
    let metallib_c = c_path(&metallib)?;
    let mut gpu_samples = vec![FptMandelbulberFieldSample::default(); sample_count];
    let mut error = [0_i8; 4096];
    let gpu_started = Instant::now();
    let status = unsafe {
        fpt_mandelbulber_sample_field(
            metallib_c.as_ptr(),
            generated_source
                .as_ref()
                .map_or(std::ptr::null(), |source| source.as_ptr().cast()),
            generated_source.as_ref().map_or(0, String::len),
            &config,
            points.as_ptr().cast(),
            points.len(),
            gpu_samples.as_mut_ptr(),
            error.as_mut_ptr(),
            error.len(),
        )
    };
    let gpu_elapsed_ms = gpu_started.elapsed().as_secs_f64() * 1000.0;
    if status != 0 {
        bail!("{}", bridge_error(&error));
    }
    let mut distance_errors = Vec::with_capacity(sample_count);
    let mut radius_errors = Vec::with_capacity(sample_count);
    let mut derivative_errors = Vec::with_capacity(sample_count);
    let mut same_iteration_radius_relative_errors = Vec::with_capacity(sample_count);
    let mut same_iteration_derivative_relative_errors = Vec::with_capacity(sample_count);
    let mut distance_failures = 0usize;
    let mut precision_limited_samples = 0usize;
    let mut iteration_mismatches = 0usize;
    let mut non_finite_samples = 0usize;
    let mut outliers = Vec::<(f64, Value)>::new();
    let cpu_started = Instant::now();
    for ((point, cpu_point), gpu) in points.iter().zip(&cpu_points).zip(&gpu_samples) {
        let cpu = scene.distance(*cpu_point);
        if !gpu.distance.is_finite() || !gpu.radius.is_finite() || !gpu.derivative.is_finite() {
            non_finite_samples += 1;
            continue;
        }
        let gpu_distance = f64::from(gpu.distance) / world_scale;
        let distance_error = (cpu.distance - gpu_distance).abs();
        let radius_error = (cpu.radius - gpu.radius as f64).abs();
        let derivative_error = (cpu.derivative - gpu.derivative as f64).abs();
        let tolerance = 5.0e-5 * (1.0 + cpu.distance.abs());
        let precision_limited =
            scene.formula_id == 3 && cpu.derivative.abs() < f64::from(f32::MIN_POSITIVE);
        precision_limited_samples += usize::from(precision_limited);
        if distance_error > tolerance && !precision_limited {
            distance_failures += 1;
        }
        let iteration_mismatch = cpu.iterations != gpu.iterations as u32;
        if iteration_mismatch {
            iteration_mismatches += 1;
        } else {
            same_iteration_radius_relative_errors.push(radius_error / (1.0 + cpu.radius.abs()));
            same_iteration_derivative_relative_errors
                .push(derivative_error / (1.0 + cpu.derivative.abs()));
        }
        if distance_error > tolerance || iteration_mismatch {
            outliers.push((
                distance_error,
                json!({
                    "point": [point[0], point[1], point[2]],
                    "cpu": {
                        "distance": cpu.distance,
                        "radius": cpu.radius,
                        "derivative": cpu.derivative,
                        "iterations": cpu.iterations,
                    },
                    "gpu": {
                        "distance": gpu_distance,
                        "radius": gpu.radius,
                        "derivative": gpu.derivative,
                        "iterations": gpu.iterations as u32,
                    },
                    "absolute_distance_error": distance_error,
                    "distance_tolerance": tolerance,
                }),
            ));
        }
        distance_errors.push(distance_error);
        radius_errors.push(radius_error);
        derivative_errors.push(derivative_error);
    }
    let cpu_elapsed_ms = cpu_started.elapsed().as_secs_f64() * 1000.0;

    let mean_distance_error =
        distance_errors.iter().sum::<f64>() / distance_errors.len().max(1) as f64;
    let max_distance_error = distance_errors.iter().copied().fold(0.0, f64::max);
    let p99_distance_error = percentile(&mut distance_errors, 0.99);
    let max_radius_error = radius_errors.iter().copied().fold(0.0, f64::max);
    let max_derivative_error = derivative_errors.iter().copied().fold(0.0, f64::max);
    let max_same_iteration_radius_relative_error = same_iteration_radius_relative_errors
        .iter()
        .copied()
        .fold(0.0, f64::max);
    let max_same_iteration_derivative_relative_error = same_iteration_derivative_relative_errors
        .iter()
        .copied()
        .fold(0.0, f64::max);
    let distance_failure_fraction = distance_failures as f64 / sample_count as f64;
    let iteration_mismatch_fraction = iteration_mismatches as f64 / sample_count as f64;
    let allowed_outlier_fraction = 2.0e-4;
    let geometry_qualified = non_finite_samples == 0
        && distance_failure_fraction <= allowed_outlier_fraction
        && p99_distance_error <= 5.0e-5;
    let orbit_qualified =
        geometry_qualified && iteration_mismatch_fraction <= allowed_outlier_fraction;
    outliers.sort_by(|left, right| right.0.total_cmp(&left.0));
    let outlier_examples = outliers
        .into_iter()
        .take(16)
        .map(|(_, value)| value)
        .collect::<Vec<_>>();
    let formula_backend = if scene.hybrid_enabled {
        "hybrid-sequence"
    } else {
        match scene.formula_id {
            3 => "mandelbulb-power2",
            10 => "kaleidoscopic-ifs",
            _ => "unknown",
        }
    };
    let implementation = if generated_source.is_some() {
        "generated"
    } else {
        "handwritten"
    };
    let report = json!({
        "scene": scene_path,
        "backend": format!("mandelbulber-{implementation}-{formula_backend}-metal-f32"),
        "reference": "cpu-f64",
        "metal_device": metal_device_name(),
        "sample_count": sample_count,
        "sample_bounds_mandelbulber": [[-2.0, -2.0, -2.0], [2.0, 2.0, 2.0]],
        "metal_fpt_world_scale": world_scale,
        "gpu_batch_elapsed_ms": gpu_elapsed_ms,
        "cpu_reference_and_comparison_elapsed_ms": cpu_elapsed_ms,
        "distance_tolerance": "5e-5 * (1 + abs(cpu_distance))",
        "mean_absolute_distance_error": mean_distance_error,
        "p99_absolute_distance_error": p99_distance_error,
        "max_absolute_distance_error": max_distance_error,
        "max_absolute_radius_error": max_radius_error,
        "max_absolute_derivative_error": max_derivative_error,
        "max_same_iteration_radius_relative_error": max_same_iteration_radius_relative_error,
        "max_same_iteration_derivative_relative_error": max_same_iteration_derivative_relative_error,
        "distance_failures": distance_failures,
        "distance_failure_fraction": distance_failure_fraction,
        "iteration_mismatches": iteration_mismatches,
        "iteration_mismatch_fraction": iteration_mismatch_fraction,
        "non_finite_samples": non_finite_samples,
        "precision_limited_samples": precision_limited_samples,
        "precision_limited_note": "Power 2 samples whose f64 derivative is below f32::MIN_POSITIVE are reported but excluded from geometry qualification because Mandelbulber's f32 path takes its DE<=0 fallback after underflow.",
        "allowed_outlier_fraction": allowed_outlier_fraction,
        "outlier_examples": outlier_examples,
        "geometry_qualified": geometry_qualified,
        "orbit_qualified": orbit_qualified,
        "qualified": geometry_qualified,
    });
    if let Some(parent) = report_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &report_path,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    eprintln!(
        "Mandelbulber field parity: {sample_count} samples, GPU {gpu_elapsed_ms:.2} ms, CPU+comparison {cpu_elapsed_ms:.2} ms, mean {mean_distance_error:.3e}, p99 {p99_distance_error:.3e}, max {max_distance_error:.3e}, failures {distance_failures}, iteration mismatches {iteration_mismatches}; wrote {}",
        report_path.display()
    );
    Ok(())
}

fn mandel_catalog(args: &[String]) -> Result<()> {
    let source_root = PathBuf::from(
        args.first()
            .ok_or_else(|| anyhow!("mandel-catalog requires a Mandelbulber source directory"))?,
    );
    let mut output_dir = PathBuf::from("reports/mandel/catalog");
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                index += 1;
                output_dir = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--out requires a directory"))?
                    .into();
            }
            value => bail!("unknown mandel-catalog argument: {value}"),
        }
        index += 1;
    }
    let summary = mandelbulber::catalog::generate_catalog(&source_root, &output_dir)?;
    eprintln!(
        "Mandelbulber catalog: {} formulas ({} kernels), {} defaults, {} examples ({} hybrid), {} distinct scene formulas; wrote {}",
        summary.formulas,
        summary.formula_kernels,
        summary.parameter_defaults,
        summary.example_scenes,
        summary.hybrid_scenes,
        summary.distinct_scene_formula_ids,
        output_dir.display()
    );
    if !summary.unresolved_scene_formula_ids.is_empty() {
        eprintln!(
            "unresolved formula IDs referenced by scenes: {:?}",
            summary.unresolved_scene_formula_ids
        );
    }
    Ok(())
}

fn mandel_coverage(args: &[String]) -> Result<()> {
    let source_root = PathBuf::from(
        args.first()
            .ok_or_else(|| anyhow!("mandel-coverage requires a Mandelbulber source directory"))?,
    );
    let mut catalog_dir = PathBuf::from("reports/mandel/catalog");
    let mut report_path = PathBuf::from("reports/mandel/coverage.json");
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--catalog-out" => {
                index += 1;
                catalog_dir = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--catalog-out requires a directory"))?
                    .into();
            }
            "--report" => {
                index += 1;
                report_path = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--report requires a JSON path"))?
                    .into();
            }
            value => bail!("unknown mandel-coverage argument: {value}"),
        }
        index += 1;
    }
    let report =
        mandelbulber::coverage::generate_coverage(&source_root, &catalog_dir, &report_path)?;
    eprintln!(
        "Mandelbulber coverage: {}/{} scenes compatible ({} non-hybrid, {} hybrid), {} scenes blocked, {} one formula away; wrote {}",
        report.summary.runtime_compatible_scenes,
        report.summary.total_scenes,
        report.summary.compatible_non_hybrid_scenes,
        report.summary.compatible_hybrid_scenes,
        report.summary.blocked_scenes,
        report.summary.scenes_one_formula_away,
        report_path.display()
    );
    Ok(())
}

fn mandel_compile(args: &[String]) -> Result<()> {
    let source_root = PathBuf::from(
        args.first()
            .ok_or_else(|| anyhow!("mandel-compile requires a Mandelbulber source directory"))?,
    );
    let identifier = args
        .get(1)
        .ok_or_else(|| anyhow!("mandel-compile requires a formula ID or symbol"))?;
    let mut output = None::<PathBuf>;
    let mut report_path = None::<PathBuf>;
    let mut check_metal = true;
    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                index += 1;
                output = Some(
                    args.get(index)
                        .ok_or_else(|| anyhow!("--out requires a .metal path"))?
                        .into(),
                );
            }
            "--report" => {
                index += 1;
                report_path = Some(
                    args.get(index)
                        .ok_or_else(|| anyhow!("--report requires a JSON path"))?
                        .into(),
                );
            }
            "--no-check" => check_metal = false,
            value => bail!("unknown mandel-compile argument: {value}"),
        }
        index += 1;
    }
    let output = output.ok_or_else(|| anyhow!("mandel-compile requires --out"))?;
    let report =
        mandelbulber::compiler::compile_formula(&source_root, identifier, &output, check_metal)?;
    if let Some(report_path) = report_path {
        if let Some(parent) = report_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        fs::write(
            &report_path,
            format!("{}\n", serde_json::to_string_pretty(&report)?),
        )?;
    }
    eprintln!(
        "Mandelbulber formula {} (ID {}) -> {}{}",
        report.formula.symbol,
        report.formula.id,
        output.display(),
        if report.metal_checked {
            " (Metal compile passed)"
        } else {
            ""
        }
    );
    Ok(())
}

fn mandel_scene_compile(args: &[String]) -> Result<()> {
    let scene_path = PathBuf::from(
        args.first()
            .ok_or_else(|| anyhow!("mandel-scene-compile requires a .fract scene"))?,
    );
    let mut source_root = None::<PathBuf>;
    let mut output = None::<PathBuf>;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--mandelbulber-root" => {
                index += 1;
                source_root = Some(
                    args.get(index)
                        .ok_or_else(|| anyhow!("--mandelbulber-root requires a directory"))?
                        .into(),
                );
            }
            "--out" => {
                index += 1;
                output = Some(
                    args.get(index)
                        .ok_or_else(|| anyhow!("--out requires a .metal path"))?
                        .into(),
                );
            }
            value => bail!("unknown mandel-scene-compile argument: {value}"),
        }
        index += 1;
    }
    let source_root =
        source_root.ok_or_else(|| anyhow!("mandel-scene-compile requires --mandelbulber-root"))?;
    let output = output.ok_or_else(|| anyhow!("mandel-scene-compile requires --out"))?;
    let mut scene = MandelbulberScene::load(&scene_path)?;
    let mut config = default_config();
    scene.apply_to_config(&mut config);
    let source = mandelbulber::compiler::specialize_scene_with_kernel_specialization(
        std::str::from_utf8(METAL_SOURCE_BYTES)?,
        &mut scene,
        &source_root,
        &config.set_values,
        mandelbulber::compiler::scene_uses_kernel_specialization(&scene_path)?,
        mandelbulber::compiler::scene_uses_direct_hybrid_loop(&scene_path)?,
        mandelbulber::compiler::scene_formula_optimization_policy(&scene_path)?,
    )?;
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output, source)?;
    eprintln!(
        "Mandelbulber scene {} -> {}",
        scene_path.display(),
        output.display()
    );
    Ok(())
}

fn mandel_audit(args: &[String]) -> Result<()> {
    let source_root = PathBuf::from(
        args.first()
            .ok_or_else(|| anyhow!("mandel-audit requires a Mandelbulber source directory"))?,
    );
    let mut report_path = PathBuf::from("reports/mandel/compiler-audit.json");
    let mut metal_check = false;
    let mut metal_output_dir = None::<PathBuf>;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--report" => {
                index += 1;
                report_path = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--report requires a path"))?
                    .into();
            }
            "--metal-check" => metal_check = true,
            "--metal-out" => {
                index += 1;
                metal_output_dir = Some(
                    args.get(index)
                        .ok_or_else(|| anyhow!("--metal-out requires a directory"))?
                        .into(),
                );
                metal_check = true;
            }
            value => bail!("unknown mandel-audit argument: {value}"),
        }
        index += 1;
    }
    let default_metal_output = report_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("compiler-audit-metal");
    let metal_output =
        metal_check.then(|| metal_output_dir.as_deref().unwrap_or(&default_metal_output));
    let report = mandelbulber::compiler::audit_formulas(&source_root, metal_output)?;
    if let Some(parent) = report_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &report_path,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    eprintln!(
        "Mandelbulber frontend audit: {}/{} formulas parsed, {} failed, {} have generated runtime emitters; Metal {}/{} passed; wrote {}",
        report.parsed,
        report.formulas,
        report.failed,
        report.runtime_emitters,
        report.metal_passed,
        report.metal_checked,
        report_path.display()
    );
    Ok(())
}

fn mandel_scene_audit(args: &[String]) -> Result<()> {
    let source_root =
        PathBuf::from(args.first().ok_or_else(|| {
            anyhow!("mandel-scene-audit requires a Mandelbulber source directory")
        })?);
    let mut report_path = PathBuf::from("reports/mandel/scene-audit.json");
    let mut metal_check = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--report" => {
                index += 1;
                report_path = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--report requires a path"))?
                    .into();
            }
            "--metal-check" => metal_check = true,
            value => bail!("unknown mandel-scene-audit argument: {value}"),
        }
        index += 1;
    }
    let report = mandelbulber::compiler::audit_scenes(
        &source_root,
        std::str::from_utf8(METAL_SOURCE_BYTES)?,
        metal_check,
    )?;
    if let Some(parent) = report_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &report_path,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    eprintln!(
        "Mandelbulber scene audit: {}/{} generated, {} generation failures; Metal {}/{} passed; wrote {}",
        report.generated,
        report.scenes,
        report.generation_failed,
        report.metal_passed,
        report.metal_checked,
        report_path.display()
    );
    Ok(())
}

fn mandel_formula_policy_audit(args: &[String]) -> Result<()> {
    let source_root = PathBuf::from(args.first().ok_or_else(|| {
        anyhow!("mandel-formula-policy-audit requires a Mandelbulber source directory")
    })?);
    let mut report_path = PathBuf::from("reports/mandel/formula-policy-audit.json");
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--report" => {
                index += 1;
                report_path = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--report requires a path"))?
                    .into();
            }
            value => bail!("unknown mandel-formula-policy-audit argument: {value}"),
        }
        index += 1;
    }
    let report = mandelbulber::compiler::audit_formula_optimization_policy(
        &source_root,
        std::str::from_utf8(METAL_SOURCE_BYTES)?,
    )?;
    if let Some(parent) = report_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &report_path,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    eprintln!(
        "Mandel formula policy audit: {}/{} generated, {} selected, {} changed, {} unexpected source changes, {} unexpected generation failures, {} missed; wrote {}",
        report.generated,
        report.scenes,
        report.selected_scenes,
        report.changed_sources,
        report.unexpected_changes,
        report.unexpected_generation_failures,
        report.missed_selections,
        report_path.display()
    );
    ensure!(
        report.generated > 0
            && report.unexpected_generation_failures == 0
            && report.unexpected_changes == 0
            && report.missed_selections == 0,
        "formula policy audit failed"
    );
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
        "render-batch" => render_batch(tail),
        "diagnostic-batch" => diagnostic_batch(tail),
        "diagnostic" => diagnostic(&parse_render_args(tail)?),
        "preview" => preview(&parse_render_args(tail)?),
        "voxel-export" => voxel_export_command(tail),
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
        "mandel-catalog" => mandel_catalog(tail),
        "mandel-coverage" => mandel_coverage(tail),
        "mandel-audit" => mandel_audit(tail),
        "mandel-scene-audit" => mandel_scene_audit(tail),
        "mandel-formula-policy-audit" => mandel_formula_policy_audit(tail),
        "mandel-benchmark" => mandel_benchmark(tail),
        "mandel-compile" => mandel_compile(tail),
        "mandel-scene-compile" => mandel_scene_compile(tail),
        "mandel-parity" => mandel_parity(tail),
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

    fn run_async_jit_validation(
        metallib: &CString,
        stitch_metallib: &CString,
        archive: &CString,
        config: &FptRenderConfig,
    ) -> FptAsyncJitStats {
        let mut stats = FptAsyncJitStats::default();
        let mut error = [0_i8; 1024];
        let status = unsafe {
            fpt_test_async_stitch_context(
                metallib.as_ptr(),
                stitch_metallib.as_ptr(),
                archive.as_ptr(),
                config,
                &mut stats,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if status != 0 {
            panic!("{}", bridge_error(&error));
        }
        stats
    }

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

    #[test]
    fn mandel_render_cache_retains_only_required_entry_points() {
        let mut config = FptRenderConfig {
            samples: 1,
            focus_distance: 10.0,
            ..Default::default()
        };
        assert_eq!(
            mandel_render_kernel_names(&config),
            ["accumulate_all_kernel", "present_kernel"]
        );

        config.samples = 32;
        config.focus_distance = 0.0;
        config.sdf_profile = 1;
        assert_eq!(
            mandel_render_kernel_names(&config),
            [
                "accumulate_chunk_kernel",
                "present_kernel",
                "estimate_focus_distance_kernel",
                "sdf_profile_kernel",
            ]
        );
    }

    #[test]
    fn benchmark_statistics_accept_an_all_failure_cohort() {
        assert_eq!(median(Vec::new()), 0.0);
        assert_eq!(percentile(&mut [], 0.9), 0.0);
    }

    #[test]
    fn precompiled_metallib_matches_the_sdf_family() {
        let mut config = FptRenderConfig {
            renderer_backend: RENDERER_SDF,
            sdf_id: SDF_CAGE_FRACTAL,
            ..Default::default()
        };
        assert_eq!(precompiled_metallib(&config), PrecompiledMetallib::Cage);

        config.sdf_id = SDF_TOWER_FRACTAL;
        assert_eq!(precompiled_metallib(&config), PrecompiledMetallib::Tower);

        config.sdf_id = SDF_BALL_FRACTAL;
        assert_eq!(precompiled_metallib(&config), PrecompiledMetallib::Builtin);

        config.sdf_id = SDF_PROGRAM;
        assert_eq!(precompiled_metallib(&config), PrecompiledMetallib::Full);

        config.sdf_id = SDF_CAGE_FRACTAL;
        config.renderer_backend = RENDERER_VOXEL;
        assert_eq!(precompiled_metallib(&config), PrecompiledMetallib::Full);
    }

    #[test]
    fn backend_selection_requires_margin_and_parity() {
        assert!(candidate_probe_qualifies(120.0, 90.0, 1.0e-7, 1.0e-6));
        assert!(!candidate_probe_qualifies(120.0, 110.0, 1.0e-7, 1.0e-6));
        assert!(!candidate_probe_qualifies(120.0, 90.0, 2.0e-6, 1.0e-6));
        assert!(!candidate_probe_qualifies(120.0, 90.0, 1.0e-7, 3.0e-5));
    }

    #[test]
    fn backend_probe_order_balances_each_round_position() {
        assert_eq!(backend_probe_order(0), [0, 1, 2]);
        assert_eq!(backend_probe_order(1), [2, 0, 1]);
        assert_eq!(backend_probe_order(2), [1, 2, 0]);
        for slot in 0..3 {
            let mut seen = [false; 3];
            for run in 0..3 {
                seen[backend_probe_order(run)[slot]] = true;
            }
            assert_eq!(seen, [true, true, true]);
        }
    }

    #[test]
    fn backend_probe_only_expands_near_a_gate() {
        assert!(backend_gate_is_near(1.18, 1.176470588));
        assert!(backend_gate_is_near(1.05, 1.10));
        assert!(!backend_gate_is_near(1.40, 1.176470588));
        assert!(!backend_gate_is_near(0.90, 1.10));
    }

    #[test]
    fn generated_surface_requires_a_win_over_distance_only() {
        let measurement = |predicted_amortized_ms, qualified| SdfBackendCandidateMeasurement {
            probe_ms: Some(predicted_amortized_ms),
            cold_build_ms: Some(0.0),
            warm_build_ms: Some(0.0),
            predicted_render_ms: Some(predicted_amortized_ms),
            predicted_amortized_ms: Some(predicted_amortized_ms),
            speedup: None,
            parity_mean_absolute_error: Some(0.0),
            parity_max_absolute_error: Some(0.0),
            parity_outlier_fraction: Some(0.0),
            qualified,
            rejection_reason: None,
        };
        let mut candidates = BTreeMap::new();
        candidates.insert("generated-distance".to_owned(), measurement(50.0, true));
        candidates.insert("generated-surface".to_owned(), measurement(47.0, true));
        assert_eq!(
            choose_measured_backend(100.0, &candidates),
            "generated-distance"
        );
        candidates.insert("generated-surface".to_owned(), measurement(45.0, true));
        assert_eq!(
            choose_measured_backend(100.0, &candidates),
            "generated-surface"
        );
        candidates.insert("generated-distance".to_owned(), measurement(50.0, false));
        assert_eq!(
            choose_measured_backend(100.0, &candidates),
            "generated-surface"
        );
    }

    #[test]
    fn backend_keys_separate_pipeline_topology_from_workload_state() {
        let mut config = FptRenderConfig {
            width: 960,
            height: 540,
            samples: 112,
            sdf_program_count: 1,
            ..Default::default()
        };
        config.sdf_program[0] = FptSdfInstruction {
            opcode: SDF_OP_SPHERE,
            flags: 0,
            data: [0.75, 0.0, 0.0, 0.0],
            ..Default::default()
        };
        let topology_key = backend_topology_key(&config, "Test GPU");
        let workload_key = backend_workload_key(&config, "Test GPU");
        config.program_material[0] = 0.9;
        assert_eq!(topology_key, backend_topology_key(&config, "Test GPU"));
        assert_ne!(workload_key, backend_workload_key(&config, "Test GPU"));
        let material_workload_key = backend_workload_key(&config, "Test GPU");
        config.sdf_program[0].data[0] = 0.8;
        assert_eq!(topology_key, backend_topology_key(&config, "Test GPU"));
        assert_ne!(
            material_workload_key,
            backend_workload_key(&config, "Test GPU")
        );
        config.sdf_program[0].data[0] = 0.75;
        config.samples = 64;
        assert_ne!(workload_key, backend_workload_key(&config, "Test GPU"));
        config.samples = 112;
        config.sdf_russian_roulette = 1;
        assert_ne!(workload_key, backend_workload_key(&config, "Test GPU"));
        config.sdf_program[0].opcode = SDF_OP_BOX;
        assert_ne!(topology_key, backend_topology_key(&config, "Test GPU"));
    }

    #[test]
    fn auto_backend_prepares_typed_soa_direct_candidate() {
        let scene = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("scenes/benchmarks/Exact_Translation_Union.json");
        let args = parse_render_args(&[
            scene.to_string_lossy().into_owned(),
            "--sdf-function-stitching".into(),
            "auto".into(),
        ])
        .expect("parse auto backend scene");
        let mut loaded = load_scene_config(&args).expect("load auto backend scene");
        apply_optimization_args(&mut loaded.config, &args);
        assert_eq!(
            loaded.config.sdf_function_stitching,
            SdfFunctionStitching::Auto as u32
        );
        assert_eq!(direct_evaluator_name(&loaded.config), "typed-soa");
        assert!(topology_specialization_supported(&loaded.config));
        let generated_distance = generated_backend_config(&loaded.config, false);
        assert_eq!(generated_distance.sdf_topology_specialization, 1);
        assert_eq!(generated_distance.sdf_stitched_surface, 0);
        assert_eq!(
            direct_evaluator_name(&generated_distance),
            "generated-optimized-program"
        );
        let generated_surface = generated_backend_config(&loaded.config, true);
        assert_eq!(generated_surface.sdf_topology_specialization, 1);
        assert_eq!(generated_surface.sdf_stitched_surface, 1);
        assert_eq!(generated_surface.sdf_function_stitching, 0);
    }

    #[test]
    fn cached_backend_names_restore_all_production_candidates() {
        let config = FptRenderConfig::default();
        let direct = apply_cached_backend(&config, "direct").expect("direct candidate");
        assert_eq!(direct.sdf_topology_specialization, 0);
        let distance =
            apply_cached_backend(&config, "generated-distance").expect("distance candidate");
        assert_eq!(distance.sdf_topology_specialization, 1);
        assert_eq!(distance.sdf_stitched_surface, 0);
        let surface =
            apply_cached_backend(&config, "generated-surface").expect("surface candidate");
        assert_eq!(surface.sdf_topology_specialization, 1);
        assert_eq!(surface.sdf_stitched_surface, 1);
        assert!(apply_cached_backend(&config, "unknown").is_none());
    }

    #[test]
    fn persistent_async_jit_renders_fallback_then_reuses_archive() {
        let _metal_test_guard = metal_test_guard();
        use std::time::{SystemTime, UNIX_EPOCH};

        let scene =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("scenes/benchmarks/Exact_Sphere.json");
        let args = parse_render_args(&[
            scene.to_string_lossy().into_owned(),
            "--preview".into(),
            "--width".into(),
            "96".into(),
            "--height".into(),
            "54".into(),
            "--samples".into(),
            "1".into(),
            "--sdf-function-stitching".into(),
            "inline".into(),
        ])
        .expect("parse async JIT validation scene");
        let mut loaded = load_scene_config(&args).expect("load async JIT validation scene");
        apply_optimization_args(&mut loaded.config, &args);
        assert_eq!(loaded.config.sdf_id, SDF_PROGRAM);
        assert_ne!(loaded.config.sdf_program_count, 0);

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let cache_dir = std::env::temp_dir().join(format!(
            "fpt-metal/async-jit-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&cache_dir).expect("create async JIT test cache");
        let archive = cache_dir.join("pipeline.metallibarchive");
        let metallib = c_path(&default_metallib_path().expect("materialize fallback metallib"))
            .expect("encode fallback metallib path");
        let stitch_metallib =
            c_path(&default_stitch_metallib_path().expect("materialize stitch-host metallib"))
                .expect("encode stitch-host metallib path");
        let archive = c_path(&archive).expect("encode archive path");

        let cold = run_async_jit_validation(&metallib, &stitch_metallib, &archive, &loaded.config);
        eprintln!("async JIT cold: {cold:?}");
        assert_eq!(
            cold.cache_status, 1,
            "cold topology should populate archive"
        );
        assert_eq!(
            cold.fallback_completed_before_jit, 1,
            "fallback frame should complete while the cold JIT is active"
        );
        assert!(cold.jit_build_ms > 0.0);
        assert!(cold.max_absolute_error <= 1.0e-4);

        let warm = run_async_jit_validation(&metallib, &stitch_metallib, &archive, &loaded.config);
        eprintln!("async JIT warm: {warm:?}");
        assert_eq!(warm.cache_status, 2, "warm topology should load archive");
        assert!(warm.jit_build_ms > 0.0);
        assert!(warm.max_absolute_error <= 1.0e-4);
    }

    #[test]
    fn expanded_stitch_vocabulary_matches_preview_fallback() {
        let _metal_test_guard = metal_test_guard();
        use std::time::{SystemTime, UNIX_EPOCH};

        let scene =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("scenes/benchmarks/Exact_Transforms.json");
        let args = parse_render_args(&[
            scene.to_string_lossy().into_owned(),
            "--preview".into(),
            "--width".into(),
            "96".into(),
            "--height".into(),
            "54".into(),
            "--samples".into(),
            "1".into(),
            "--sdf-function-stitching".into(),
            "inline".into(),
        ])
        .expect("parse expanded stitch vocabulary scene");
        let mut loaded = load_scene_config(&args).expect("load stitch vocabulary scene");
        apply_optimization_args(&mut loaded.config, &args);

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let cache_dir = std::env::temp_dir().join(format!(
            "fpt-metal/stitch-vocabulary-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&cache_dir).expect("create stitch vocabulary test cache");
        let metallib = c_path(&default_metallib_path().expect("materialize fallback metallib"))
            .expect("encode fallback metallib path");
        let stitch_metallib =
            c_path(&default_stitch_metallib_path().expect("materialize stitch-host metallib"))
                .expect("encode stitch-host metallib path");
        let archive = c_path(&cache_dir.join("pipeline.metallibarchive"))
            .expect("encode vocabulary archive path");

        let stats = run_async_jit_validation(&metallib, &stitch_metallib, &archive, &loaded.config);
        eprintln!("expanded stitch vocabulary: {stats:?}");
        assert_eq!(stats.cache_status, 1);
        assert_eq!(stats.fallback_completed_before_jit, 1);
        assert!(stats.max_absolute_error <= 1.0e-4);
    }

    #[test]
    fn typed_soa_matches_interpreted_field_and_gradient() {
        let _metal_test_guard = metal_test_guard();
        let scene = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("scenes/benchmarks/Exact_Translation_Union.json");
        let args = parse_render_args(&[
            scene.to_string_lossy().into_owned(),
            "--sdf-typed-soa".into(),
        ])
        .expect("parse typed-SoA validation scene");
        let loaded = load_scene_config(&args).expect("load typed-SoA validation scene");
        let metallib = c_path(&default_metallib_path().expect("materialize metallib"))
            .expect("encode metallib path");
        let mut stats = FptStitchValidationStats::default();
        let mut error = [0_i8; 1024];
        let status = unsafe {
            fpt_test_typed_soa(
                metallib.as_ptr(),
                &loaded.config,
                &mut stats,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if status != 0 {
            panic!("{}", bridge_error(&error));
        }
        eprintln!(
            "typed SoA validation: {} samples, max distance {}, max gradient {}",
            stats.sample_count, stats.max_distance_error, stats.max_gradient_error
        );
        assert_eq!(stats.sample_count, 1 << 20);
        assert_eq!(stats.distance_failures, 0);
        assert_eq!(stats.gradient_failures, 0);
    }

    #[test]
    fn research_stitched_modes_match_interpreted_field_and_gradient() {
        let _metal_test_guard = metal_test_guard();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = ProbeDirectory(std::env::temp_dir().join(format!(
            "fpt-metal/lean-stitch-validation-{}-{nonce}",
            std::process::id()
        )));
        fs::create_dir_all(&directory.0).expect("create lean validation directory");
        let metallib = default_metallib_path().expect("materialize metallib");
        let stitch_metallib = default_stitch_metallib_path().expect("materialize stitch host");
        for (label, scene_name, extra, expected_state, expected_split, expected_fusion) in [
            (
                "lean",
                "Exact_Transforms.json",
                vec!["--sdf-stitch-distance-only"],
                SDF_STITCH_STATE_LEAN,
                0,
                0,
            ),
            (
                "split",
                "Exact_Transforms.json",
                vec!["--sdf-stitch-split-graph"],
                SDF_STITCH_STATE_LEAN,
                1,
                0,
            ),
            (
                "fusion",
                "Exact_Translation_Union.json",
                vec!["--sdf-stitch-fusion", "double-pairs"],
                SDF_STITCH_STATE_FULL,
                0,
                SdfStitchFusion::DoublePairs as u32,
            ),
        ] {
            let scene = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("scenes/benchmarks")
                .join(scene_name);
            let mut arguments = vec![
                scene.to_string_lossy().into_owned(),
                "--width".into(),
                "96".into(),
                "--height".into(),
                "54".into(),
                "--samples".into(),
                "1".into(),
                "--sdf-function-stitching".into(),
                "inline".into(),
            ];
            arguments.extend(extra.into_iter().map(str::to_owned));
            arguments.push("--sdf-stitch-validation".into());
            let args = parse_render_args(&arguments).expect("parse stitched validation scene");
            let mut loaded = load_scene_config(&args).expect("load stitched validation scene");
            apply_optimization_args(&mut loaded.config, &args);
            assert_eq!(loaded.config.sdf_stitch_distance_only, expected_state);
            assert_eq!(loaded.config.sdf_stitch_split_graph, expected_split);
            assert_eq!(loaded.config.sdf_stitch_fusion, expected_fusion);
            assert_eq!(
                loaded.config.sdf_stitched_surface,
                u32::from(expected_state == SDF_STITCH_STATE_FULL)
            );

            let archive = directory.0.join(format!("{label}.metallibarchive"));
            let output = directory.0.join(format!("{label}-validation.png"));
            let stats = execute_metal_render(
                &loaded.config,
                &metallib,
                &stitch_metallib,
                Some(&archive),
                &output,
            )
            .expect("render and validate stitched field");
            eprintln!(
                "{label} stitch validation: {} samples, max distance {}, max gradient {}",
                stats.stitch_validation.sample_count,
                stats.stitch_validation.max_distance_error,
                stats.stitch_validation.max_gradient_error
            );
            assert_eq!(stats.stitch_validation.sample_count, 1 << 20);
            assert_eq!(stats.stitch_validation.distance_failures, 0);
            assert_eq!(stats.stitch_validation.gradient_failures, 0);
            assert!(stats.stitch_pipeline.instruction_count > 0);
        }
    }

    #[test]
    fn topology_generated_surface_matches_interpreted_field_and_gradient() {
        let _metal_test_guard = metal_test_guard();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = ProbeDirectory(std::env::temp_dir().join(format!(
            "fpt-metal/topology-surface-validation-{}-{nonce}",
            std::process::id()
        )));
        fs::create_dir_all(&directory.0).expect("create topology validation directory");
        let scene =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("scenes/benchmarks/Exact_Transforms.json");
        let arguments = vec![
            scene.to_string_lossy().into_owned(),
            "--width".into(),
            "96".into(),
            "--height".into(),
            "54".into(),
            "--samples".into(),
            "1".into(),
            "--sdf-topology-specialization".into(),
            "--sdf-program-validation".into(),
        ];
        let args = parse_render_args(&arguments).expect("parse topology validation scene");
        let mut loaded = load_scene_config(&args).expect("load topology validation scene");
        apply_optimization_args(&mut loaded.config, &args);
        assert_eq!(loaded.config.sdf_topology_specialization, 1);
        assert_eq!(loaded.config.sdf_stitched_surface, 1);

        let metallib = default_metallib_path().expect("materialize metallib");
        let stitch_metallib = default_stitch_metallib_path().expect("materialize stitch host");
        let output = directory.0.join("topology-surface-validation.png");
        let stats =
            execute_metal_render(&loaded.config, &metallib, &stitch_metallib, None, &output)
                .expect("render and validate topology-generated field");
        eprintln!(
            "topology surface validation: {} samples, max distance {}, max gradient {}",
            stats.stitch_validation.sample_count,
            stats.stitch_validation.max_distance_error,
            stats.stitch_validation.max_gradient_error
        );
        assert_eq!(stats.stitch_validation.sample_count, 1 << 20);
        assert_eq!(stats.stitch_validation.distance_failures, 0);
        assert_eq!(stats.stitch_validation.gradient_failures, 0);
    }

    #[test]
    fn canonical_generated_surface_matches_interpreted_field_and_gradient() {
        let _metal_test_guard = metal_test_guard();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = ProbeDirectory(std::env::temp_dir().join(format!(
            "fpt-metal/canonical-surface-validation-{}-{nonce}",
            std::process::id()
        )));
        fs::create_dir_all(&directory.0).expect("create canonical validation directory");
        let scene = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("scenes/benchmarks/Exact_Translation_Union.json");
        let metallib = default_metallib_path().expect("materialize metallib");
        let stitch_metallib = default_stitch_metallib_path().expect("materialize stitch host");
        for (label, flag, expected_mode) in [
            ("wide", "--sdf-canonical-topology-specialization", 2),
            (
                "compact",
                "--sdf-compact-canonical-topology-specialization",
                3,
            ),
            (
                "shared-transform",
                "--sdf-shared-transform-topology-specialization",
                4,
            ),
            (
                "affine-index",
                "--sdf-affine-index-topology-specialization",
                5,
            ),
        ] {
            let arguments = vec![
                scene.to_string_lossy().into_owned(),
                "--width".into(),
                "96".into(),
                "--height".into(),
                "54".into(),
                "--samples".into(),
                "1".into(),
                flag.into(),
                "--sdf-program-validation".into(),
            ];
            let args = parse_render_args(&arguments).expect("parse canonical validation scene");
            let mut loaded = load_scene_config(&args).expect("load canonical validation scene");
            apply_optimization_args(&mut loaded.config, &args);
            assert_eq!(loaded.config.sdf_topology_specialization, expected_mode);
            assert!(loaded.config.sdf_canonical_count > 0);
            let output = directory
                .0
                .join(format!("canonical-{label}-surface-validation.png"));
            let stats =
                execute_metal_render(&loaded.config, &metallib, &stitch_metallib, None, &output)
                    .expect("render and validate canonical-generated field");
            assert_eq!(stats.stitch_validation.sample_count, 1 << 20);
            assert_eq!(stats.stitch_validation.distance_failures, 0);
            assert_eq!(stats.stitch_validation.gradient_failures, 0);
        }
    }
}
