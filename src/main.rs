#![recursion_limit = "512"]

mod ffi;
mod scene;
mod tools;

use anyhow::{Context, Result, anyhow, bail};
use ffi::*;
use scene::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::fs;
use std::path::{Path, PathBuf};
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
  fpt-metal render <scene.json> --out <dir> [--renderer sdf|voxel|bound-grid|regional] [--sdf-backend auto] [--regional-program-resolution 16|32] [--bound-grid-resolution 32|64] [--bound-grid-directional] [--bound-grid-fp16] [--bound-grid-profile] [--bound-grid-profile-stride N] [--bound-grid-cage-bounds] [--no-sdf-geometry-split] [--no-sdf-canonical-ir] [--sdf-topology-specialization] [--sdf-tiny-linked-helper] [--sdf-canonical-topology-specialization] [--sdf-compact-canonical-topology-specialization] [--sdf-shared-transform-topology-specialization] [--sdf-affine-index-topology-specialization] [--no-sdf-generated-surface] [--sdf-runtime-source-bytecode] [--sdf-function-stitching normal|inline|auto] [--sdf-stitch-distance-only] [--sdf-stitch-split-graph] [--sdf-stitch-fusion off|one-pair|pairs|double-pairs] [--sdf-program-validation] [--sdf-flat-union] [--sdf-typed-soa] [--voxel-resolution N] [--voxel-normal face|smooth|exact] [--voxel-material stored|exact] [--voxel-offset legacy|precision] [--voxel-storage dense|sparse-bricks] [--voxel-surface-band N] [--voxel-coverage legacy|lipschitz|interval] [--voxel-build staging|direct] [--voxel-brick-rejection] [--voxel-leaf-refinement none|secant-bisection|restricted-trace|fixed-de] [--fpt-root <dir>] [--preview] [--glass-mode analytic|pathtrace] [--sdf-accumulation auto|per-sample|batch|chunked] [--sdf-normal-mode auto|central|tetra|program-gradient] [--sdf-program-optimization off|basic] [--sdf-chunk-samples N] [--width N] [--height N] [--samples N]\n\
  --sdf-backend auto reuses cached decisions; --sdf-backend probe measures on a cache miss\n\
  Research-only: function-stitching variants, canonical/shared/affine generated forms, dual/tiny libraries, bound-grid, and regional backends\n\
  fpt-metal render-batch <jobs.json>\n\
  fpt-metal diagnostic <scene.json> --out <dir> --mode <mode> [--fpt-root <dir>] [--width N] [--height N]\n\
  fpt-metal preview <scene.json> [--renderer sdf|voxel] [--sdf-backend auto] [--sdf-function-stitching normal|inline] [--no-sdf-stitched-surface] [--voxel-resolution N] [--voxel-normal face|smooth|exact] [--voxel-material stored|exact] [--voxel-offset legacy|precision] [--voxel-storage dense|sparse-bricks] [--voxel-leaf-refinement none|secant-bisection|restricted-trace|fixed-de] [--fpt-root <dir>] [--pathtrace] [--sdf-profile] [--width N] [--height N] [--samples N]\n\
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

fn materialize_metallib(bytes: &[u8], label: &str, digest: &str) -> Result<PathBuf> {
    let directory = std::env::temp_dir().join("fpt-metal");
    let path = directory.join(format!("{label}-{}.metallib", &digest[..16]));
    if !path.exists() {
        fs::create_dir_all(&directory)?;
        fs::write(&path, bytes)?;
    }
    Ok(path)
}

fn default_metallib_path() -> Result<PathBuf> {
    materialize_metallib(METALLIB_BYTES, "Shaders", METALLIB_SHA)
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
    hash_f32_slice(&mut digest, &[config.sdf_rr_start, config.sdf_rr_min_prob]);
    hash_f32_slice(&mut digest, &config.camera_position);
    hash_f32_slice(&mut digest, &config.camera_yaw_pitch);
    hash_f32_slice(
        &mut digest,
        &[config.camera_fov, config.camera_dof, config.focus_distance],
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
            METAL_SOURCE_BYTES.as_ptr().cast(),
            METAL_SOURCE_BYTES.len(),
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

struct RenderMetadataInput<'a> {
    output: &'a Path,
    scene: &'a Path,
    config: &'a FptRenderConfig,
    stats: &'a MetalRenderStats,
    stitch_cache_key: Option<&'a str>,
    backend_selection: Option<&'a SdfBackendSelection>,
}

fn write_render_metadata(input: RenderMetadataInput<'_>) -> Result<()> {
    let RenderMetadataInput {
        output,
        scene,
        config,
        stats,
        stitch_cache_key,
        backend_selection,
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
        "voxel_storage": if config.voxel_storage == VOXEL_STORAGE_SPARSE_BRICKS { "sparse-bricks" } else { "dense" },
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
        "voxel_memory_bytes": if config.renderer_backend == RENDERER_VOXEL { Some(voxel_memory_bytes) } else { None },
        "voxel_active_bricks": if config.renderer_backend == RENDERER_VOXEL { Some(voxel_active_bricks) } else { None },
        "voxel_active_cells": if config.renderer_backend == RENDERER_VOXEL { Some(voxel_active_cells) } else { None },
        "voxel_rejected_bricks": if config.renderer_backend == RENDERER_VOXEL { Some(voxel_rejected_bricks) } else { None },
        "bound_grid_resolution": config.bound_grid_resolution,
        "bound_grid_mode": if config.bound_grid_directional != 0 { "directional" } else { "range" },
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
    let metallib = metallib_path(args, &loaded.config)?;
    let stitch_metallib = default_stitch_metallib_path()?;
    let backend_selection = if args.sdf_function_stitching == SdfFunctionStitching::Auto {
        let selection_start = Instant::now();
        let (selected, mut selection) = select_sdf_backend(
            &loaded.config,
            &metallib,
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
    let stitch_archive = stitch_archive_path(&loaded.config)?;
    let stitch_cache_key = stitch_archive
        .as_ref()
        .and_then(|path| path.file_stem())
        .and_then(|stem| stem.to_str())
        .map(str::to_owned);
    let stats = execute_metal_render(
        &loaded.config,
        &metallib,
        &stitch_metallib,
        stitch_archive.as_deref(),
        &output,
    )?;
    write_render_metadata(RenderMetadataInput {
        output: &output,
        scene: &args.scene_path,
        config: &loaded.config,
        stats: &stats,
        stitch_cache_key: stitch_cache_key.as_deref(),
        backend_selection: backend_selection.as_ref(),
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
    let metallib_c = c_path(&metallib_path(args, &loaded.config)?)?;
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
    let status = unsafe {
        fpt_metal_preview(
            metallib_c.as_ptr(),
            stitch_metallib_c.as_ptr(),
            stitch_archive_ptrs.as_ptr(),
            METAL_SOURCE_BYTES.as_ptr().cast(),
            METAL_SOURCE_BYTES.len(),
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
