use crate::ffi::*;
use crate::mandelbulber::MandelbulberScene;
use anyhow::{Result, anyhow, bail, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_FPT_ROOT: &str = "../FPT";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlassMode {
    Pathtrace = 0,
    Analytic = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SdfAccumulationMode {
    Auto = SDF_ACCUMULATION_AUTO as isize,
    PerSample = SDF_ACCUMULATION_PER_SAMPLE as isize,
    Batch = SDF_ACCUMULATION_BATCH as isize,
    Chunked = SDF_ACCUMULATION_CHUNKED as isize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SdfNormalMode {
    Auto = 0,
    Tetra = 1,
    ProgramGradient = 2,
    Central = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SdfProgramOptimization {
    Off = 0,
    Basic = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SdfFunctionStitching {
    Off = 0,
    Normal = 1,
    AlwaysInline = 2,
    Auto = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SdfStitchFusion {
    Off = 0,
    OnePair = 1,
    Pairs = 2,
    DoublePairs = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendererBackend {
    Sdf = RENDERER_SDF as isize,
    Voxel = RENDERER_VOXEL as isize,
    BoundGrid = RENDERER_BOUND_GRID as isize,
    Regional = RENDERER_REGIONAL as isize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoxelNormalMode {
    Face = VOXEL_NORMAL_FACE as isize,
    Smooth = VOXEL_NORMAL_SMOOTH as isize,
    Exact = VOXEL_NORMAL_EXACT as isize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoxelMaterialMode {
    Stored = VOXEL_MATERIAL_STORED as isize,
    Exact = VOXEL_MATERIAL_EXACT as isize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoxelOffsetMode {
    Legacy = VOXEL_OFFSET_LEGACY as isize,
    Precision = VOXEL_OFFSET_PRECISION as isize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoxelStorageMode {
    Dense = VOXEL_STORAGE_DENSE as isize,
    SparseBricks = VOXEL_STORAGE_SPARSE_BRICKS as isize,
    TemplateBricks = VOXEL_STORAGE_TEMPLATE_BRICKS as isize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoxelCoverageMode {
    Legacy = VOXEL_COVERAGE_LEGACY as isize,
    Lipschitz = VOXEL_COVERAGE_LIPSCHITZ as isize,
    Interval = VOXEL_COVERAGE_INTERVAL as isize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoxelBuildMode {
    Staging = VOXEL_BUILD_STAGING as isize,
    Direct = VOXEL_BUILD_DIRECT as isize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoxelLeafRefinement {
    None = VOXEL_LEAF_REFINEMENT_NONE as isize,
    SecantBisection = VOXEL_LEAF_REFINEMENT_SECANT_BISECTION as isize,
    RestrictedTrace = VOXEL_LEAF_REFINEMENT_RESTRICTED_TRACE as isize,
    FixedDe = VOXEL_LEAF_REFINEMENT_FIXED_DE as isize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticMode {
    Depth = DIAGNOSTIC_DEPTH as isize,
    Normal = DIAGNOSTIC_NORMAL as isize,
    Material = DIAGNOSTIC_MATERIAL as isize,
    HitMask = DIAGNOSTIC_HIT_MASK as isize,
    PathDirect = DIAGNOSTIC_PATH_DIRECT as isize,
    PathEnvironment = DIAGNOSTIC_PATH_ENVIRONMENT as isize,
    PathThroughput = DIAGNOSTIC_PATH_THROUGHPUT as isize,
    PathFinal = DIAGNOSTIC_PATH_FINAL as isize,
    DiffuseNormal = DIAGNOSTIC_DIFFUSE_NORMAL as isize,
    MandelColorIndex = DIAGNOSTIC_MANDEL_COLOR_INDEX as isize,
    MandelPalettePosition = DIAGNOSTIC_MANDEL_PALETTE_POSITION as isize,
    SdfPrimarySteps = DIAGNOSTIC_SDF_PRIMARY_STEPS as isize,
    SdfShadowSteps = DIAGNOSTIC_SDF_SHADOW_STEPS as isize,
    SdfNormalEvals = DIAGNOSTIC_SDF_NORMAL_EVALS as isize,
    SdfBounces = DIAGNOSTIC_SDF_BOUNCES as isize,
    SdfBounceContribution = DIAGNOSTIC_SDF_BOUNCE_CONTRIBUTION as isize,
}

#[derive(Clone, Debug)]
pub struct RenderArgs {
    pub scene_path: PathBuf,
    pub out_dir: PathBuf,
    pub fpt_root: PathBuf,
    pub mandelbulber_root: Option<PathBuf>,
    pub metallib: Option<PathBuf>,
    pub preview: bool,
    pub live_pathtrace: bool,
    pub sdf_profile: bool,
    pub glass_mode: GlassMode,
    pub sdf_accumulation_mode: SdfAccumulationMode,
    pub sdf_chunk_samples: u32,
    pub sdf_bounce_cap: Option<u32>,
    pub sdf_russian_roulette: bool,
    pub sdf_normal_mode: SdfNormalMode,
    pub sdf_program_optimization: SdfProgramOptimization,
    pub sdf_geometry_split: bool,
    pub sdf_canonical_ir: bool,
    pub sdf_topology_specialization: bool,
    pub sdf_canonical_topology_specialization: bool,
    pub sdf_compact_canonical_topology_specialization: bool,
    pub sdf_shared_transform_topology_specialization: bool,
    pub sdf_affine_index_topology_specialization: bool,
    pub sdf_runtime_source_bytecode: bool,
    pub sdf_dual_generated_library: bool,
    pub sdf_tiny_linked_helper: bool,
    pub sdf_function_stitching: SdfFunctionStitching,
    pub sdf_backend_probe: bool,
    pub sdf_stitched_surface: bool,
    pub sdf_stitch_validation: bool,
    pub sdf_stitch_distance_only: bool,
    pub sdf_stitch_split_graph: bool,
    pub sdf_stitch_fusion: SdfStitchFusion,
    pub sdf_flat_union: bool,
    pub sdf_typed_soa: bool,
    pub renderer_backend: RendererBackend,
    pub voxel_resolution: Option<u32>,
    pub voxel_normal_mode: VoxelNormalMode,
    pub voxel_storage_mode: Option<VoxelStorageMode>,
    pub voxel_surface_band: Option<f32>,
    pub voxel_coverage_mode: Option<VoxelCoverageMode>,
    pub voxel_build_mode: Option<VoxelBuildMode>,
    pub voxel_brick_rejection: bool,
    pub voxel_leaf_refinement: Option<VoxelLeafRefinement>,
    pub voxel_material_mode: Option<VoxelMaterialMode>,
    pub voxel_offset_mode: Option<VoxelOffsetMode>,
    pub bound_grid_resolution: Option<u32>,
    pub bound_grid_profile: bool,
    pub bound_grid_profile_stride: u32,
    pub bound_grid_cage_bounds: bool,
    pub bound_grid_directional: bool,
    pub bound_grid_fp16: bool,
    pub regional_program_resolution: u32,
    pub sdf_rr_start: f32,
    pub sdf_rr_min_prob: f32,
    pub mandel_iteration_scale: Option<f32>,
    pub mandel_screen_lod_rate: Option<f32>,
    pub mandel_optimization_auto: bool,
    pub mandel_selection_cache: Option<PathBuf>,
    pub sdf_bounce_index: u32,
    pub diagnostic_mode: DiagnosticMode,
    pub diagnostic_max_distance: Option<f32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub samples: Option<u32>,
}

impl RenderArgs {
    fn new(scene_path: impl Into<PathBuf>) -> Self {
        Self {
            scene_path: scene_path.into(),
            out_dir: "renders".into(),
            fpt_root: DEFAULT_FPT_ROOT.into(),
            mandelbulber_root: None,
            metallib: None,
            preview: false,
            live_pathtrace: false,
            sdf_profile: false,
            glass_mode: GlassMode::Analytic,
            sdf_accumulation_mode: SdfAccumulationMode::Auto,
            sdf_chunk_samples: 8,
            sdf_bounce_cap: None,
            sdf_russian_roulette: false,
            sdf_normal_mode: SdfNormalMode::Auto,
            sdf_program_optimization: SdfProgramOptimization::Basic,
            sdf_geometry_split: true,
            sdf_canonical_ir: true,
            sdf_topology_specialization: false,
            sdf_canonical_topology_specialization: false,
            sdf_compact_canonical_topology_specialization: false,
            sdf_shared_transform_topology_specialization: false,
            sdf_affine_index_topology_specialization: false,
            sdf_runtime_source_bytecode: false,
            sdf_dual_generated_library: false,
            sdf_tiny_linked_helper: false,
            sdf_function_stitching: SdfFunctionStitching::Off,
            sdf_backend_probe: false,
            sdf_stitched_surface: true,
            sdf_stitch_validation: false,
            sdf_stitch_distance_only: false,
            sdf_stitch_split_graph: false,
            sdf_stitch_fusion: SdfStitchFusion::Off,
            sdf_flat_union: false,
            sdf_typed_soa: false,
            renderer_backend: RendererBackend::Sdf,
            voxel_resolution: None,
            voxel_normal_mode: VoxelNormalMode::Face,
            voxel_storage_mode: None,
            voxel_surface_band: None,
            voxel_coverage_mode: None,
            voxel_build_mode: None,
            voxel_brick_rejection: false,
            voxel_leaf_refinement: None,
            voxel_material_mode: None,
            voxel_offset_mode: None,
            bound_grid_resolution: None,
            bound_grid_profile: false,
            bound_grid_profile_stride: 4,
            bound_grid_cage_bounds: false,
            bound_grid_directional: false,
            bound_grid_fp16: false,
            regional_program_resolution: 16,
            sdf_rr_start: 3.0,
            sdf_rr_min_prob: 0.2,
            mandel_iteration_scale: None,
            mandel_screen_lod_rate: None,
            mandel_optimization_auto: false,
            mandel_selection_cache: None,
            sdf_bounce_index: 0,
            diagnostic_mode: DiagnosticMode::Depth,
            diagnostic_max_distance: None,
            width: None,
            height: None,
            samples: None,
        }
    }
}

pub struct LoadedScene {
    pub config: FptRenderConfig,
    pub output_name: String,
    pub runtime_metal_source: Option<Vec<u8>>,
}

fn next_value<'a>(args: &'a [String], index: &mut usize, flag: &str) -> Result<&'a str> {
    *index += 1;
    args.get(*index)
        .map(String::as_str)
        .ok_or_else(|| anyhow!("{flag} requires a value"))
}

pub fn parse_render_args(args: &[String]) -> Result<RenderArgs> {
    let scene = args.first().ok_or_else(|| anyhow!("missing scene JSON"))?;
    let mut out = RenderArgs::new(scene);
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--preview" => out.preview = true,
            "--pathtrace" => out.live_pathtrace = true,
            "--sdf-profile" => out.sdf_profile = true,
            "--renderer" => {
                out.renderer_backend = match next_value(args, &mut i, "--renderer")? {
                    "sdf" => RendererBackend::Sdf,
                    "voxel" => RendererBackend::Voxel,
                    "bound-grid" => RendererBackend::BoundGrid,
                    "regional" => RendererBackend::Regional,
                    value => bail!("invalid renderer: {value}"),
                }
            }
            "--bound-grid-resolution" => {
                let resolution = next_value(args, &mut i, "--bound-grid-resolution")?.parse()?;
                ensure!(
                    matches!(resolution, 32 | 64 | 128 | 256),
                    "bound-grid resolution must be 32, 64, 128, or 256"
                );
                out.bound_grid_resolution = Some(resolution);
            }
            "--bound-grid-profile" => out.bound_grid_profile = true,
            "--bound-grid-cage-bounds" => out.bound_grid_cage_bounds = true,
            "--bound-grid-directional" => out.bound_grid_directional = true,
            "--bound-grid-fp16" => out.bound_grid_fp16 = true,
            "--regional-program-resolution" => {
                let resolution =
                    next_value(args, &mut i, "--regional-program-resolution")?.parse()?;
                ensure!(
                    matches!(resolution, 16 | 32),
                    "regional program resolution must be 16 or 32"
                );
                out.regional_program_resolution = resolution;
            }
            "--bound-grid-profile-stride" => {
                let stride = next_value(args, &mut i, "--bound-grid-profile-stride")?.parse()?;
                ensure!(
                    (1..=16).contains(&stride),
                    "bound-grid profile stride must be 1..16"
                );
                out.bound_grid_profile_stride = stride;
            }
            "--voxel-resolution" => {
                let resolution = next_value(args, &mut i, "--voxel-resolution")?.parse()?;
                ensure!(
                    (32..=512).contains(&resolution),
                    "voxel resolution must be 32..512"
                );
                out.voxel_resolution = Some(resolution);
            }
            "--voxel-normal" => {
                out.voxel_normal_mode = match next_value(args, &mut i, "--voxel-normal")? {
                    "face" => VoxelNormalMode::Face,
                    "smooth" => VoxelNormalMode::Smooth,
                    "exact" => VoxelNormalMode::Exact,
                    value => bail!("invalid voxel normal mode: {value}"),
                }
            }
            "--voxel-storage" => {
                out.voxel_storage_mode = Some(match next_value(args, &mut i, "--voxel-storage")? {
                    "dense" => VoxelStorageMode::Dense,
                    "sparse-bricks" => VoxelStorageMode::SparseBricks,
                    "template-bricks" => VoxelStorageMode::TemplateBricks,
                    value => bail!("invalid voxel storage mode: {value}"),
                })
            }
            "--voxel-surface-band" => {
                let band = next_value(args, &mut i, "--voxel-surface-band")?.parse()?;
                ensure!(
                    (0.25..=4.0).contains(&band),
                    "voxel surface band must be 0.25..4"
                );
                out.voxel_surface_band = Some(band);
            }
            "--voxel-coverage" => {
                out.voxel_coverage_mode =
                    Some(match next_value(args, &mut i, "--voxel-coverage")? {
                        "legacy" => VoxelCoverageMode::Legacy,
                        "lipschitz" => VoxelCoverageMode::Lipschitz,
                        "interval" => VoxelCoverageMode::Interval,
                        value => bail!("invalid voxel coverage mode: {value}"),
                    });
            }
            "--voxel-build" => {
                out.voxel_build_mode = Some(match next_value(args, &mut i, "--voxel-build")? {
                    "staging" => VoxelBuildMode::Staging,
                    "direct" => VoxelBuildMode::Direct,
                    value => bail!("invalid voxel build mode: {value}"),
                });
            }
            "--voxel-brick-rejection" => out.voxel_brick_rejection = true,
            "--voxel-leaf-refinement" => {
                out.voxel_leaf_refinement =
                    Some(match next_value(args, &mut i, "--voxel-leaf-refinement")? {
                        "none" => VoxelLeafRefinement::None,
                        "secant-bisection" => VoxelLeafRefinement::SecantBisection,
                        "restricted-trace" => VoxelLeafRefinement::RestrictedTrace,
                        "fixed-de" => VoxelLeafRefinement::FixedDe,
                        value => bail!("invalid voxel leaf refinement: {value}"),
                    });
            }
            "--voxel-material" => {
                out.voxel_material_mode =
                    Some(match next_value(args, &mut i, "--voxel-material")? {
                        "stored" => VoxelMaterialMode::Stored,
                        "exact" => VoxelMaterialMode::Exact,
                        value => bail!("invalid voxel material mode: {value}"),
                    });
            }
            "--voxel-offset" => {
                out.voxel_offset_mode = Some(match next_value(args, &mut i, "--voxel-offset")? {
                    "legacy" => VoxelOffsetMode::Legacy,
                    "precision" => VoxelOffsetMode::Precision,
                    value => bail!("invalid voxel offset mode: {value}"),
                });
            }
            "--out" => out.out_dir = next_value(args, &mut i, "--out")?.into(),
            "--fpt-root" => out.fpt_root = next_value(args, &mut i, "--fpt-root")?.into(),
            "--mandelbulber-root" => {
                out.mandelbulber_root =
                    Some(next_value(args, &mut i, "--mandelbulber-root")?.into())
            }
            "--metallib" => out.metallib = Some(next_value(args, &mut i, "--metallib")?.into()),
            "--width" => out.width = Some(next_value(args, &mut i, "--width")?.parse()?),
            "--height" => out.height = Some(next_value(args, &mut i, "--height")?.parse()?),
            "--samples" => {
                let samples = next_value(args, &mut i, "--samples")?.parse()?;
                ensure!((1..=512).contains(&samples), "samples must be 1..512");
                out.samples = Some(samples);
            }
            "--sdf-chunk-samples" => {
                out.sdf_chunk_samples = next_value(args, &mut i, "--sdf-chunk-samples")?.parse()?;
                ensure!(
                    (1..=64).contains(&out.sdf_chunk_samples),
                    "chunk samples must be 1..64"
                );
            }
            "--sdf-bounce-cap" => {
                out.sdf_bounce_cap = Some(next_value(args, &mut i, "--sdf-bounce-cap")?.parse()?)
            }
            "--sdf-russian-roulette" => out.sdf_russian_roulette = true,
            "--sdf-rr-start" => {
                out.sdf_rr_start = next_value(args, &mut i, "--sdf-rr-start")?.parse()?
            }
            "--sdf-rr-min-prob" => {
                out.sdf_rr_min_prob = next_value(args, &mut i, "--sdf-rr-min-prob")?.parse()?
            }
            "--mandel-iteration-scale" => {
                let scale = next_value(args, &mut i, "--mandel-iteration-scale")?.parse()?;
                ensure!(
                    (0.125..=1.0).contains(&scale),
                    "Mandel iteration scale must be 0.125..1"
                );
                out.mandel_iteration_scale = Some(scale);
            }
            "--mandel-screen-lod-rate" => {
                let rate = next_value(args, &mut i, "--mandel-screen-lod-rate")?.parse()?;
                ensure!(
                    (0.0..=8.0).contains(&rate),
                    "Mandel screen LOD rate must be 0..8"
                );
                out.mandel_screen_lod_rate = Some(rate);
            }
            "--mandel-optimization" => {
                out.mandel_optimization_auto =
                    match next_value(args, &mut i, "--mandel-optimization")? {
                        "exact" => false,
                        "auto" => true,
                        value => bail!("unknown Mandel optimization mode: {value}"),
                    };
            }
            "--mandel-selection-cache" => {
                out.mandel_selection_cache = Some(PathBuf::from(next_value(
                    args,
                    &mut i,
                    "--mandel-selection-cache",
                )?));
            }
            "--sdf-bounce-index" => {
                out.sdf_bounce_index = next_value(args, &mut i, "--sdf-bounce-index")?.parse()?
            }
            "--sdf-accumulation" => {
                out.sdf_accumulation_mode = match next_value(args, &mut i, "--sdf-accumulation")? {
                    "auto" => SdfAccumulationMode::Auto,
                    "per-sample" => SdfAccumulationMode::PerSample,
                    "batch" => SdfAccumulationMode::Batch,
                    "chunked" => SdfAccumulationMode::Chunked,
                    value => bail!("invalid SDF accumulation mode: {value}"),
                }
            }
            "--sdf-normal-mode" => {
                out.sdf_normal_mode = match next_value(args, &mut i, "--sdf-normal-mode")? {
                    "auto" => SdfNormalMode::Auto,
                    "central" => SdfNormalMode::Central,
                    "tetra" => SdfNormalMode::Tetra,
                    "program-gradient" => SdfNormalMode::ProgramGradient,
                    value => bail!("invalid SDF normal mode: {value}"),
                }
            }
            "--sdf-program-optimization" => {
                out.sdf_program_optimization =
                    match next_value(args, &mut i, "--sdf-program-optimization")? {
                        "off" => SdfProgramOptimization::Off,
                        "basic" => SdfProgramOptimization::Basic,
                        value => bail!("invalid SDF program optimization: {value}"),
                    }
            }
            "--sdf-geometry-split" => out.sdf_geometry_split = true,
            "--no-sdf-geometry-split" => out.sdf_geometry_split = false,
            "--no-sdf-canonical-ir" => out.sdf_canonical_ir = false,
            "--sdf-topology-specialization" => out.sdf_topology_specialization = true,
            "--sdf-canonical-topology-specialization" => {
                out.sdf_topology_specialization = true;
                out.sdf_canonical_topology_specialization = true;
            }
            "--sdf-compact-canonical-topology-specialization" => {
                out.sdf_topology_specialization = true;
                out.sdf_compact_canonical_topology_specialization = true;
            }
            "--sdf-shared-transform-topology-specialization" => {
                out.sdf_topology_specialization = true;
                out.sdf_shared_transform_topology_specialization = true;
            }
            "--sdf-affine-index-topology-specialization" => {
                out.sdf_topology_specialization = true;
                out.sdf_affine_index_topology_specialization = true;
            }
            "--sdf-runtime-source-bytecode" => out.sdf_runtime_source_bytecode = true,
            "--sdf-dual-generated-library" => out.sdf_dual_generated_library = true,
            "--sdf-tiny-linked-helper" => out.sdf_tiny_linked_helper = true,
            "--sdf-backend" => match next_value(args, &mut i, "--sdf-backend")? {
                "auto" => {
                    out.sdf_function_stitching = SdfFunctionStitching::Auto;
                    out.sdf_backend_probe = false;
                }
                "probe" => {
                    out.sdf_function_stitching = SdfFunctionStitching::Auto;
                    out.sdf_backend_probe = true;
                }
                value => bail!("invalid automatic SDF backend mode: {value}"),
            },
            "--sdf-function-stitching" => {
                out.sdf_function_stitching =
                    match next_value(args, &mut i, "--sdf-function-stitching")? {
                        "normal" => SdfFunctionStitching::Normal,
                        "inline" => SdfFunctionStitching::AlwaysInline,
                        "auto" => {
                            out.sdf_backend_probe = true;
                            SdfFunctionStitching::Auto
                        }
                        value => bail!("invalid SDF function-stitching mode: {value}"),
                    }
            }
            "--no-sdf-stitched-surface" | "--no-sdf-generated-surface" => {
                out.sdf_stitched_surface = false
            }
            "--sdf-stitch-validation" | "--sdf-program-validation" => {
                out.sdf_stitch_validation = true
            }
            "--sdf-stitch-distance-only" => out.sdf_stitch_distance_only = true,
            "--sdf-stitch-split-graph" => out.sdf_stitch_split_graph = true,
            "--sdf-stitch-fusion" => {
                out.sdf_stitch_fusion = match next_value(args, &mut i, "--sdf-stitch-fusion")? {
                    "off" => SdfStitchFusion::Off,
                    "one-pair" => SdfStitchFusion::OnePair,
                    "pairs" => SdfStitchFusion::Pairs,
                    "double-pairs" => SdfStitchFusion::DoublePairs,
                    value => bail!("invalid SDF stitch-fusion mode: {value}"),
                }
            }
            "--sdf-flat-union" => out.sdf_flat_union = true,
            "--sdf-typed-soa" => out.sdf_typed_soa = true,
            "--glass-mode" => {
                out.glass_mode = match next_value(args, &mut i, "--glass-mode")? {
                    "analytic" => GlassMode::Analytic,
                    "pathtrace" => GlassMode::Pathtrace,
                    value => bail!("invalid glass mode: {value}"),
                }
            }
            "--mode" | "--diagnostic-mode" => {
                out.diagnostic_mode = match next_value(args, &mut i, "--mode")? {
                    "depth" => DiagnosticMode::Depth,
                    "normal" => DiagnosticMode::Normal,
                    "material" => DiagnosticMode::Material,
                    "hit-mask" => DiagnosticMode::HitMask,
                    "path-direct" => DiagnosticMode::PathDirect,
                    "path-environment" => DiagnosticMode::PathEnvironment,
                    "path-throughput" => DiagnosticMode::PathThroughput,
                    "path-final" => DiagnosticMode::PathFinal,
                    "diffuse-normal" => DiagnosticMode::DiffuseNormal,
                    "mandel-color-index" => DiagnosticMode::MandelColorIndex,
                    "mandel-palette-position" => DiagnosticMode::MandelPalettePosition,
                    "sdf-primary-steps" => DiagnosticMode::SdfPrimarySteps,
                    "sdf-shadow-steps" => DiagnosticMode::SdfShadowSteps,
                    "sdf-normal-evals" => DiagnosticMode::SdfNormalEvals,
                    "sdf-bounces" => DiagnosticMode::SdfBounces,
                    "sdf-bounce-contribution" => DiagnosticMode::SdfBounceContribution,
                    value => bail!("invalid diagnostic mode: {value}"),
                }
            }
            "--max-distance" => {
                let value = next_value(args, &mut i, "--max-distance")?.parse::<f32>()?;
                ensure!(
                    value.is_finite() && value > 0.0,
                    "--max-distance must be finite and positive"
                );
                out.diagnostic_max_distance = Some(value);
            }
            value => bail!("unknown argument: {value}"),
        }
        i += 1;
    }
    Ok(out)
}

fn cached_mandel_selection(config: &FptRenderConfig, args: &RenderArgs) -> Option<(String, f32)> {
    if !args.mandel_optimization_auto || config.sdf_id != SDF_MANDELBULBER {
        return None;
    }
    let cache_path = args
        .mandel_selection_cache
        .clone()
        .or_else(|| std::env::var_os("FPT_MANDEL_SELECTION_CACHE").map(PathBuf::from))?;
    let cache: Value = serde_json::from_slice(&fs::read(cache_path).ok()?).ok()?;
    if cache.get("schema_version")?.as_u64()? != 1 {
        return None;
    }
    let minimum_ssim = cache.get("minimum_ssim")?.as_f64()?.max(0.98);
    let minimum_speedup = cache.get("minimum_speedup")?.as_f64()?.max(1.05);
    let scene_sha256 = format!("{:x}", Sha256::digest(fs::read(&args.scene_path).ok()?));
    let key = format!(
        "{scene_sha256}:{}x{}:{}",
        config.width, config.height, config.samples
    );
    let entry = cache.get("entries")?.get(&key)?;
    if entry.get("scene_sha256")?.as_str()? != scene_sha256
        || entry.get("width")?.as_u64()? != u64::from(config.width)
        || entry.get("height")?.as_u64()? != u64::from(config.height)
        || entry.get("samples")?.as_u64()? != u64::from(config.samples)
        || entry.get("ssim")?.as_f64()? < minimum_ssim
        || entry.get("speedup")?.as_f64()? < minimum_speedup
    {
        return None;
    }
    let selection = entry.get("selection")?;
    Some((
        selection.get("kind")?.as_str()?.to_owned(),
        selection.get("value")?.as_f64()? as f32,
    ))
}

pub fn apply_optimization_args(config: &mut FptRenderConfig, args: &RenderArgs) {
    config.sdf_bounce_cap = args.sdf_bounce_cap.unwrap_or(0);
    config.sdf_russian_roulette = u32::from(args.sdf_russian_roulette);
    config.sdf_normal_mode = args.sdf_normal_mode as u32;
    config.sdf_rr_start = args.sdf_rr_start;
    config.sdf_rr_min_prob = args.sdf_rr_min_prob;
    let mut iteration_scale = args.mandel_iteration_scale;
    let mut screen_lod_rate = args.mandel_screen_lod_rate;
    if iteration_scale.is_none()
        && screen_lod_rate.is_none()
        && let Some((kind, value)) = cached_mandel_selection(config, args)
    {
        match kind.as_str() {
            "iteration_scale" if (0.125..=1.0).contains(&value) => iteration_scale = Some(value),
            "screen_lod" if (0.0..=8.0).contains(&value) => screen_lod_rate = Some(value),
            "exact" => {}
            _ => {}
        }
    }
    config.mandel_iteration_scale = iteration_scale.unwrap_or(1.0);
    config.vset_values[crate::mandelbulber::VPARAM_SCREEN_LOD_RATE] =
        screen_lod_rate.unwrap_or(0.0);
    if config.sdf_id == SDF_MANDELBULBER {
        if let Some(scale) = iteration_scale {
            let iterations = config.set_values[crate::mandelbulber::PARAM_MAX_ITERATIONS];
            config.set_values[crate::mandelbulber::PARAM_MAX_ITERATIONS] =
                (iterations * scale).round().max(1.0);
        }
    }
    config.sdf_chunk_samples = args.sdf_chunk_samples;
    config.sdf_topology_specialization = if args.sdf_affine_index_topology_specialization {
        5
    } else if args.sdf_shared_transform_topology_specialization {
        4
    } else if args.sdf_compact_canonical_topology_specialization {
        3
    } else if args.sdf_canonical_topology_specialization {
        2
    } else {
        u32::from(args.sdf_topology_specialization)
    };
    config.sdf_runtime_source_bytecode = if args.sdf_tiny_linked_helper {
        3
    } else if args.sdf_dual_generated_library {
        2
    } else if args.mandelbulber_root.is_some() {
        1
    } else {
        u32::from(args.sdf_runtime_source_bytecode)
    };
    config.sdf_function_stitching = args.sdf_function_stitching as u32;
    config.sdf_stitched_surface = u32::from(
        args.sdf_stitched_surface
            && !args.sdf_shared_transform_topology_specialization
            && !args.sdf_affine_index_topology_specialization
            && !args.sdf_stitch_distance_only
            && !args.sdf_stitch_split_graph,
    );
    config.sdf_stitch_validation = u32::from(args.sdf_stitch_validation);
    config.sdf_stitch_distance_only =
        if args.sdf_stitch_distance_only || args.sdf_stitch_split_graph {
            SDF_STITCH_STATE_LEAN
        } else {
            SDF_STITCH_STATE_FULL
        };
    config.sdf_stitch_split_graph = u32::from(args.sdf_stitch_split_graph);
    config.sdf_stitch_fusion = args.sdf_stitch_fusion as u32;
    config.renderer_backend = args.renderer_backend as u32;
    config.voxel_normal_mode = args.voxel_normal_mode as u32;
    if let Some(storage) = args.voxel_storage_mode {
        config.voxel_storage = storage as u32;
    }
    if let Some(resolution) = args.voxel_resolution {
        config.voxel_resolution = resolution;
    }
    if let Some(surface_band) = args.voxel_surface_band {
        config.voxel_surface_band = surface_band;
    }
    if let Some(coverage) = args.voxel_coverage_mode {
        config.voxel_coverage_mode = coverage as u32;
    }
    if let Some(build) = args.voxel_build_mode {
        config.voxel_build_mode = build as u32;
    }
    config.voxel_brick_rejection = u32::from(args.voxel_brick_rejection);
    if let Some(refinement) = args.voxel_leaf_refinement {
        config.voxel_leaf_refinement = refinement as u32;
    }
    if let Some(material) = args.voxel_material_mode {
        config.voxel_material_mode = material as u32;
    }
    if let Some(offset) = args.voxel_offset_mode {
        config.voxel_offset_mode = offset as u32;
    }
    if let Some(resolution) = args.bound_grid_resolution {
        config.bound_grid_resolution = resolution;
    }
    config.bound_grid_profile = u32::from(args.bound_grid_profile);
    config.bound_grid_profile_stride = args.bound_grid_profile_stride;
    config.bound_grid_cage_bounds = u32::from(args.bound_grid_cage_bounds);
    config.bound_grid_directional = u32::from(args.bound_grid_directional);
    config.bound_grid_fp16 = u32::from(args.bound_grid_fp16);
    config.regional_program_resolution = args.regional_program_resolution;
}

pub fn default_config() -> FptRenderConfig {
    let mut cfg = FptRenderConfig {
        width: 768,
        height: 768,
        samples: 64,
        sdf_id: SDF_CORNELL_BOX,
        sdf_rr_start: 3.0,
        sdf_rr_min_prob: 0.2,
        mandel_iteration_scale: 1.0,
        sdf_chunk_samples: 8,
        camera_position: [0.1, 0.1, -5.0],
        camera_yaw_pitch: [0.0, 0.0],
        camera_roll: 0.0,
        camera_fov: 90.0,
        camera_dof: 0.01,
        render: [5.0, 312.0, 0.0005, 0.0005, 1000.0, 0.25, 0.0, 0.0],
        world: [0.0, 1.0, 120.0, 30.0, 1.0, 1.0, 0.0],
        world_one_color: [1.0; 3],
        sun: [0.0, 120.0, 30.0, 1.0, 0.0],
        sun_color: [1.0; 3],
        background_gradient: [1.0; 6],
        post: [0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0],
        program_material: [1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 1.5, 0.0],
        fractal_style: [0.0, 1.0, 0.5, 1.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0],
        voxel_resolution: 128,
        voxel_storage: VOXEL_STORAGE_SPARSE_BRICKS,
        voxel_bounds_min: [-4.0, -4.0, -4.0],
        voxel_surface_band: 1.0,
        voxel_bounds_max: [4.0, 4.0, 4.0],
        bound_grid_resolution: 32,
        bound_grid_profile_stride: 4,
        ..Default::default()
    };
    cfg.focus_distance = 0.0;
    cfg
}

fn number_f32(value: Option<&Value>, fallback: f32) -> f32 {
    value
        .and_then(Value::as_f64)
        .map(|value| value as f32)
        .unwrap_or(fallback)
}

fn number_u32(value: Option<&Value>, fallback: u32) -> u32 {
    value
        .and_then(Value::as_f64)
        .map(|value| value as u32)
        .unwrap_or(fallback)
}

fn copy_float_array(target: &mut [f32], value: &Value) {
    if let Some(values) = value.as_array() {
        for (target, value) in target.iter_mut().zip(values) {
            *target = number_f32(Some(value), *target);
        }
    }
}

fn apply_float_object(target: &mut [f32], value: &Value, keys: &[&str]) {
    if let Some(object) = value.as_object() {
        for (index, key) in keys.iter().enumerate() {
            target[index] = number_f32(object.get(*key), target[index]);
        }
    }
}

fn apply_camera(config: &mut FptRenderConfig, value: &Value) {
    let Some(object) = value.as_object() else {
        return;
    };
    if let Some(value) = object.get("position") {
        copy_float_array(&mut config.camera_position, value);
    }
    if let Some(value) = object.get("yaw_pitch") {
        copy_float_array(&mut config.camera_yaw_pitch, value);
    }
    config.camera_roll = number_f32(object.get("roll"), config.camera_roll);
    config.camera_fov = number_f32(object.get("fov"), config.camera_fov);
    config.camera_dof = number_f32(object.get("dof"), config.camera_dof);
    config.focus_distance = number_f32(object.get("focus_distance"), config.focus_distance);
}

fn stem(path: &Path) -> Result<&str> {
    path.file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("invalid preset path: {}", path.display()))
}

fn sdf_id_from_preset(path: &Path) -> Result<u32> {
    Ok(match stem(path)? {
        "Cornell_Box" => SDF_CORNELL_BOX,
        "Glass_Ball" => SDF_GLASS_BALL,
        "Ball_Fractal" => SDF_BALL_FRACTAL,
        "Cage_Fractal" => SDF_CAGE_FRACTAL,
        "IFS_Fractal" => SDF_IFS_FRACTAL,
        "Mandelbox_Fractal" => SDF_MANDELBOX_FRACTAL,
        "Menger_Sponge" => SDF_MENGER_SPONGE,
        "Tower_Fractal" => SDF_TOWER_FRACTAL,
        "Tree_Fractal" => SDF_TREE_FRACTAL,
        "README_Cornell" => SDF_README_CORNELL,
        "README_Glass" => SDF_README_GLASS,
        _ if path.extension().and_then(|value| value.to_str()) == Some("fpt") => SDF_PROGRAM,
        _ => bail!("unsupported preset: {}", path.display()),
    })
}

fn resolve_preset_path(scene: &Path, fpt_root: &Path, preset: &str) -> PathBuf {
    let preset = Path::new(preset);
    if preset.is_absolute() {
        return preset.into();
    }
    if let Some(parent) = scene.parent() {
        let local = parent.join(preset);
        if local.exists() {
            return local;
        }
    }
    fpt_root.join(preset)
}

fn resolve_scene_path(scene: &Path, value: &str) -> PathBuf {
    let value = Path::new(value);
    if value.is_absolute() {
        value.into()
    } else {
        scene.parent().unwrap_or(Path::new(".")).join(value)
    }
}

fn set_hdri_path(config: &mut FptRenderConfig, path: &Path) {
    config.hdri_path.fill(0);
    let bytes = path.to_string_lossy();
    let count = bytes.len().min(config.hdri_path.len() - 1);
    config.hdri_path[..count].copy_from_slice(&bytes.as_bytes()[..count]);
    config.hdri_enabled = u32::from(count > 0);
}

fn program_opcode(name: &str) -> Result<u32> {
    Ok(match name {
        "abs" => SDF_OP_ABS,
        "translate" => SDF_OP_TRANSLATE,
        "scale" => SDF_OP_SCALE,
        "rotate_x" => SDF_OP_ROTATE_X,
        "rotate_y" => SDF_OP_ROTATE_Y,
        "rotate_z" => SDF_OP_ROTATE_Z,
        "repeat" => SDF_OP_REPEAT,
        "sort_desc" => SDF_OP_SORT_DESC,
        "sphere" => SDF_OP_SPHERE,
        "box" => SDF_OP_BOX,
        "plane" => SDF_OP_PLANE,
        "orbit_add" => SDF_OP_ORBIT_ADD,
        _ => bail!("unsupported SDF operation: {name}"),
    })
}

fn combine_mode(value: Option<&Value>) -> u32 {
    match value.and_then(Value::as_str) {
        Some("intersection") => 1,
        Some("subtract") => 2,
        _ => 0,
    }
}

fn apply_gradient(config: &mut FptRenderConfig, values: &[Value]) -> Result<()> {
    ensure!(
        values.len() <= SDF_GRADIENT_MAX_STOPS,
        "too many gradient stops"
    );
    config.gradient_count = values.len() as u32;
    let divisor = values.len().saturating_sub(1).max(1) as f32;
    for (index, value) in values.iter().enumerate() {
        let object = value
            .as_object()
            .ok_or_else(|| anyhow!("gradient stop must be an object"))?;
        config.gradient_stops[index][0] =
            number_f32(object.get("position"), index as f32 / divisor);
        if let Some(color) = object.get("color") {
            copy_float_array(&mut config.gradient_stops[index][1..4], color);
        }
    }
    Ok(())
}

fn apply_fractal_style(config: &mut FptRenderConfig, value: &Value) -> Result<()> {
    let style = value
        .as_object()
        .ok_or_else(|| anyhow!("style must be an object"))?;
    if let Some(mode) = style.get("mode").and_then(Value::as_str) {
        config.fractal_style_mode = match mode {
            "hsv" => 1,
            "solid" => 2,
            "gradient" => 3,
            _ => bail!("invalid style mode: {mode}"),
        };
    }
    for (index, key) in [
        "hue_offset",
        "hue_scale",
        "saturation",
        "value",
        "roughness",
        "specular",
        "emission",
        "solid_mix",
    ]
    .iter()
    .enumerate()
    {
        config.fractal_style[index] = number_f32(style.get(*key), config.fractal_style[index]);
    }
    if let Some(color) = style.get("solid_color") {
        copy_float_array(&mut config.fractal_style[8..11], color);
    }
    if let Some(gradient) = style.get("gradient").and_then(Value::as_array) {
        apply_gradient(config, gradient)?;
    }
    Ok(())
}

fn apply_sdf_program(config: &mut FptRenderConfig, value: &Value) -> Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("sdf_program must be an object"))?;
    let operations = object
        .get("operations")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("sdf_program.operations is required"))?;
    ensure!(
        !operations.is_empty() && operations.len() <= SDF_PROGRAM_MAX_OPS,
        "invalid SDF operation count"
    );
    config.sdf_id = SDF_PROGRAM;
    config.sdf_program_count = operations.len() as u32;
    for (index, value) in operations.iter().enumerate() {
        let operation = value
            .as_object()
            .ok_or_else(|| anyhow!("SDF operation must be an object"))?;
        let mut instruction = FptSdfInstruction {
            opcode: program_opcode(
                operation
                    .get("op")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("SDF operation is missing op"))?,
            )?,
            flags: combine_mode(operation.get("combine")),
            ..Default::default()
        };
        if let Some(value) = operation.get("data").or_else(|| operation.get("value")) {
            copy_float_array(&mut instruction.data, value);
        }
        instruction.data[0] = number_f32(operation.get("radius"), instruction.data[0]);
        instruction.data[0] = number_f32(operation.get("angle"), instruction.data[0]);
        instruction.data[0] = number_f32(operation.get("factor"), instruction.data[0]);
        instruction.data[3] = number_f32(operation.get("orbit_weight"), instruction.data[3]);
        config.sdf_program[index] = instruction;
    }
    if let Some(material) = object.get("material").and_then(Value::as_object) {
        if let Some(color) = material.get("color") {
            copy_float_array(&mut config.program_material[..3], color);
        }
        for (index, key) in ["roughness", "specular", "translucency", "ior", "emission"]
            .iter()
            .enumerate()
        {
            config.program_material[index + 3] =
                number_f32(material.get(*key), config.program_material[index + 3]);
        }
        config.material_mode = match material.get("mode").and_then(Value::as_str) {
            Some("hsv_orbit") => 1,
            Some("gradient") => 2,
            _ => config.material_mode,
        };
    }
    if let Some(gradient) = object.get("gradient").and_then(Value::as_array) {
        apply_gradient(config, gradient)?;
    }
    Ok(())
}

fn same_instruction_metadata(a: &FptSdfInstruction, b: &FptSdfInstruction) -> bool {
    a.flags == b.flags && a.material_index == b.material_index && a._pad0 == b._pad0
}

fn same_instruction_data(a: &FptSdfInstruction, b: &FptSdfInstruction) -> bool {
    a.data
        .iter()
        .zip(b.data)
        .all(|(left, right)| left.to_bits() == right.to_bits())
}

fn is_noop_instruction(instruction: &FptSdfInstruction) -> bool {
    match instruction.opcode {
        SDF_OP_TRANSLATE => instruction.data[..3].iter().all(|value| *value == 0.0),
        SDF_OP_SCALE => instruction.data[0] == 1.0 || instruction.data[0].abs() <= 1.0e-6,
        SDF_OP_ROTATE_X | SDF_OP_ROTATE_Y | SDF_OP_ROTATE_Z => instruction.data[0] == 0.0,
        SDF_OP_ORBIT_ADD => {
            instruction.data[3] == 0.0 || instruction.data[..3].iter().all(|value| *value == 0.0)
        }
        _ => false,
    }
}

fn fold_adjacent_transform(
    previous: &mut FptSdfInstruction,
    instruction: &FptSdfInstruction,
) -> bool {
    if previous.opcode != instruction.opcode || !same_instruction_metadata(previous, instruction) {
        return false;
    }
    match instruction.opcode {
        SDF_OP_TRANSLATE => {
            if previous.data[3].to_bits() != instruction.data[3].to_bits() {
                return false;
            }
            let folded = [
                previous.data[0] + instruction.data[0],
                previous.data[1] + instruction.data[1],
                previous.data[2] + instruction.data[2],
            ];
            if folded.iter().all(|value| value.is_finite()) {
                previous.data[..3].copy_from_slice(&folded);
                true
            } else {
                false
            }
        }
        SDF_OP_SCALE => {
            if !previous.data[1..]
                .iter()
                .zip(instruction.data[1..].iter())
                .all(|(left, right)| left.to_bits() == right.to_bits())
            {
                return false;
            }
            let folded = previous.data[0] * instruction.data[0];
            if folded.is_finite() && folded.abs() > 1.0e-6 {
                previous.data[0] = folded;
                true
            } else {
                false
            }
        }
        SDF_OP_ROTATE_X | SDF_OP_ROTATE_Y | SDF_OP_ROTATE_Z => {
            if !previous.data[1..]
                .iter()
                .zip(instruction.data[1..].iter())
                .all(|(left, right)| left.to_bits() == right.to_bits())
            {
                return false;
            }
            let folded = previous.data[0] + instruction.data[0];
            if folded.is_finite() {
                previous.data[0] = folded;
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

fn is_redundant_adjacent_primitive(
    previous: &FptSdfInstruction,
    instruction: &FptSdfInstruction,
) -> bool {
    if previous.opcode != instruction.opcode
        || !matches!(
            instruction.opcode,
            SDF_OP_SPHERE | SDF_OP_BOX | SDF_OP_PLANE
        )
        || !same_instruction_metadata(previous, instruction)
        || !same_instruction_data(previous, instruction)
        || !matches!(instruction.flags, 0 | 1)
    {
        return false;
    }
    instruction.opcode == SDF_OP_PLANE || instruction.data[3] == 0.0
}

fn optimize_sdf_program(config: &mut FptRenderConfig) {
    let mut optimized: Vec<FptSdfInstruction> =
        Vec::with_capacity(config.sdf_program_count as usize);
    for instruction in config
        .sdf_program
        .iter()
        .take(config.sdf_program_count as usize)
        .copied()
    {
        if is_noop_instruction(&instruction) {
            continue;
        }
        if let Some(previous) = optimized.last_mut() {
            if (instruction.opcode == SDF_OP_ABS || instruction.opcode == SDF_OP_SORT_DESC)
                && previous.opcode == instruction.opcode
            {
                continue;
            }
            if is_redundant_adjacent_primitive(previous, &instruction) {
                continue;
            }
            if fold_adjacent_transform(previous, &instruction) {
                if is_noop_instruction(previous) {
                    optimized.pop();
                }
                continue;
            }
        }
        optimized.push(instruction);
    }
    config.sdf_program.fill(FptSdfInstruction::default());
    config.sdf_program[..optimized.len()].copy_from_slice(&optimized);
    config.sdf_program_count = optimized.len() as u32;
}

fn split_sdf_geometry_program(config: &mut FptRenderConfig, enabled: bool) {
    config
        .sdf_shading_program
        .fill(FptSdfInstruction::default());
    let full_count = (config.sdf_program_count as usize).min(SDF_PROGRAM_MAX_OPS);
    config.sdf_shading_program[..full_count].copy_from_slice(&config.sdf_program[..full_count]);
    config.sdf_shading_program_count = full_count as u32;
    config.sdf_geometry_split = u32::from(enabled);
    if !enabled {
        return;
    }

    let geometry: Vec<FptSdfInstruction> = config.sdf_program[..full_count]
        .iter()
        .copied()
        .filter(|instruction| {
            !matches!(
                instruction.opcode,
                SDF_OP_ORBIT_ADD | SDF_OP_UNION | SDF_OP_INTERSECTION | SDF_OP_SUBTRACT
            )
        })
        .collect();
    config.sdf_program.fill(FptSdfInstruction::default());
    config.sdf_program[..geometry.len()].copy_from_slice(&geometry);
    config.sdf_program_count = geometry.len() as u32;
}

fn rotate_affine_rows(rows: &mut [[f32; 4]; 3], first: usize, second: usize, angle: f32) {
    let (sine, cosine) = angle.sin_cos();
    let old_first = rows[first];
    let old_second = rows[second];
    for column in 0..4 {
        rows[first][column] = cosine * old_first[column] - sine * old_second[column];
        rows[second][column] = sine * old_first[column] + cosine * old_second[column];
    }
}

fn same_canonical_primitive(left: &FptPrimitiveInstance, right: &FptPrimitiveInstance) -> bool {
    left.opcode == right.opcode
        && left.distance_scale.to_bits() == right.distance_scale.to_bits()
        && left
            .transform
            .iter()
            .zip(right.transform)
            .all(|(a, b)| a.to_bits() == b.to_bits())
        && left
            .data
            .iter()
            .zip(right.data)
            .all(|(a, b)| a.to_bits() == b.to_bits())
}

fn compile_canonical_geometry_program(config: &mut FptRenderConfig) -> bool {
    config.sdf_canonical_count = 0;
    config.sdf_canonical_source_count = 0;
    config.sdf_canonical_transform_count = 0;
    config
        .sdf_canonical_primitives
        .fill(FptPrimitiveInstance::default());
    config
        .sdf_canonical_transforms
        .fill(FptAffineTransform::default());
    config
        .sdf_indexed_primitives
        .fill(FptIndexedPrimitive::default());
    if config.sdf_id != SDF_PROGRAM || config.sdf_program_count == 0 {
        return false;
    }

    let mut rows = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
    ];
    let mut distance_scale = 1.0_f32;
    let mut primitives: Vec<FptPrimitiveInstance> = Vec::new();
    for (source_instruction, instruction) in config
        .sdf_program
        .iter()
        .take(config.sdf_program_count as usize)
        .enumerate()
    {
        if !instruction.data.iter().all(|value| value.is_finite()) {
            return false;
        }
        match instruction.opcode {
            SDF_OP_TRANSLATE => {
                for (axis, row) in rows.iter_mut().enumerate() {
                    row[3] -= instruction.data[axis];
                }
            }
            SDF_OP_SCALE => {
                let scale = if instruction.data[0].abs() > 1.0e-6 {
                    instruction.data[0]
                } else {
                    1.0
                };
                for row in &mut rows {
                    for value in row {
                        *value *= scale;
                    }
                }
                distance_scale *= scale.abs();
            }
            SDF_OP_ROTATE_X => rotate_affine_rows(&mut rows, 1, 2, instruction.data[0]),
            SDF_OP_ROTATE_Y => rotate_affine_rows(&mut rows, 0, 2, instruction.data[0]),
            SDF_OP_ROTATE_Z => rotate_affine_rows(&mut rows, 0, 1, instruction.data[0]),
            SDF_OP_SPHERE | SDF_OP_BOX | SDF_OP_PLANE => {
                if instruction.flags > 2 || primitives.len() >= SDF_FLAT_UNION_MAX_PRIMITIVES {
                    return false;
                }
                config.sdf_canonical_source_count += 1;
                let mut data = instruction.data;
                if instruction.opcode == SDF_OP_PLANE {
                    let length = (data[0] * data[0] + data[1] * data[1] + data[2] * data[2]).sqrt();
                    if length <= 1.0e-8 {
                        return false;
                    }
                    data[0] /= length;
                    data[1] /= length;
                    data[2] /= length;
                }
                let primitive = FptPrimitiveInstance {
                    transform: [
                        rows[0][0], rows[0][1], rows[0][2], rows[0][3], rows[1][0], rows[1][1],
                        rows[1][2], rows[1][3], rows[2][0], rows[2][1], rows[2][2], rows[2][3],
                    ],
                    data,
                    opcode: instruction.opcode,
                    distance_scale,
                    source_instruction: source_instruction as u32,
                    _pad0: if primitives.is_empty() {
                        0
                    } else {
                        instruction.flags
                    },
                };
                let associative_duplicate = if instruction.flags == 0 {
                    primitives.iter().skip(1).all(|value| value._pad0 == 0)
                        && primitives
                            .iter()
                            .any(|value| same_canonical_primitive(value, &primitive))
                } else if instruction.flags == 1 {
                    primitives.iter().skip(1).all(|value| value._pad0 == 1)
                        && primitives
                            .iter()
                            .any(|value| same_canonical_primitive(value, &primitive))
                } else {
                    false
                };
                if !associative_duplicate {
                    primitives.push(primitive);
                }
            }
            SDF_OP_UNION | SDF_OP_INTERSECTION | SDF_OP_SUBTRACT | SDF_OP_ORBIT_ADD => {}
            _ => return false,
        }
        if !distance_scale.is_finite() || !rows.iter().flatten().all(|value| value.is_finite()) {
            return false;
        }
    }
    if primitives.is_empty() {
        return false;
    }
    config.sdf_canonical_primitives[..primitives.len()].copy_from_slice(&primitives);
    config.sdf_canonical_count = primitives.len() as u32;
    let mut transforms: Vec<FptAffineTransform> = Vec::new();
    let mut indexed_primitives: Vec<FptIndexedPrimitive> = Vec::with_capacity(primitives.len());
    for primitive in &primitives {
        let transform_index = transforms
            .iter()
            .position(|candidate| {
                primitive.distance_scale.to_bits() == candidate.distance_scale.to_bits()
                    && primitive
                        .transform
                        .iter()
                        .zip(candidate.transform)
                        .all(|(left, right)| left.to_bits() == right.to_bits())
            })
            .unwrap_or_else(|| {
                transforms.push(FptAffineTransform {
                    transform: primitive.transform,
                    distance_scale: primitive.distance_scale,
                    _pad0: [0; 3],
                });
                transforms.len() - 1
            });
        indexed_primitives.push(FptIndexedPrimitive {
            data: primitive.data,
            opcode: primitive.opcode,
            transform_index: transform_index as u32,
            source_instruction: primitive.source_instruction,
            combine_mode: primitive._pad0,
        });
    }
    config.sdf_canonical_transform_count = transforms.len() as u32;
    config.sdf_canonical_transforms[..transforms.len()].copy_from_slice(&transforms);
    config.sdf_indexed_primitives[..indexed_primitives.len()].copy_from_slice(&indexed_primitives);
    true
}

fn lower_flat_union_program(config: &mut FptRenderConfig) -> bool {
    config.sdf_flat_union_count = 0;
    config
        .sdf_flat_union_instances
        .fill(FptPrimitiveInstance::default());
    if config.sdf_id != SDF_PROGRAM || config.sdf_program_count == 0 {
        return false;
    }

    let mut rows = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
    ];
    let mut distance_scale = 1.0_f32;
    let mut instances = Vec::new();
    for (source_instruction, instruction) in config
        .sdf_program
        .iter()
        .take(config.sdf_program_count as usize)
        .enumerate()
    {
        if !instruction.data.iter().all(|value| value.is_finite()) {
            return false;
        }
        match instruction.opcode {
            SDF_OP_TRANSLATE => {
                for (axis, row) in rows.iter_mut().enumerate() {
                    row[3] -= instruction.data[axis];
                }
            }
            SDF_OP_SCALE => {
                let scale = if instruction.data[0].abs() > 1.0e-6 {
                    instruction.data[0]
                } else {
                    1.0
                };
                for row in &mut rows {
                    for value in row {
                        *value *= scale;
                    }
                }
                distance_scale *= scale.abs();
            }
            SDF_OP_ROTATE_X => rotate_affine_rows(&mut rows, 1, 2, instruction.data[0]),
            SDF_OP_ROTATE_Y => rotate_affine_rows(&mut rows, 0, 2, instruction.data[0]),
            SDF_OP_ROTATE_Z => rotate_affine_rows(&mut rows, 0, 1, instruction.data[0]),
            SDF_OP_SPHERE | SDF_OP_BOX | SDF_OP_PLANE => {
                if instruction.flags != 0 || instances.len() >= SDF_FLAT_UNION_MAX_PRIMITIVES {
                    return false;
                }
                let mut data = instruction.data;
                if instruction.opcode == SDF_OP_PLANE {
                    let length = (data[0] * data[0] + data[1] * data[1] + data[2] * data[2]).sqrt();
                    if !length.is_finite() || length <= 1.0e-8 {
                        return false;
                    }
                    data[0] /= length;
                    data[1] /= length;
                    data[2] /= length;
                }
                instances.push(FptPrimitiveInstance {
                    transform: [
                        rows[0][0], rows[0][1], rows[0][2], rows[0][3], rows[1][0], rows[1][1],
                        rows[1][2], rows[1][3], rows[2][0], rows[2][1], rows[2][2], rows[2][3],
                    ],
                    data,
                    opcode: instruction.opcode,
                    distance_scale,
                    source_instruction: source_instruction as u32,
                    _pad0: 0,
                });
            }
            SDF_OP_ORBIT_ADD | SDF_OP_UNION => {}
            _ => return false,
        }
        if !distance_scale.is_finite() || !rows.iter().flatten().all(|value| value.is_finite()) {
            return false;
        }
    }
    if instances.is_empty() {
        return false;
    }
    config.sdf_flat_union_instances[..instances.len()].copy_from_slice(&instances);
    config.sdf_flat_union_count = instances.len() as u32;
    true
}

fn lower_typed_soa_program(config: &mut FptRenderConfig) -> bool {
    config.sdf_typed_soa = FptTypedSoAProgram::default();
    if config.sdf_id != SDF_PROGRAM || config.sdf_program_count == 0 {
        return false;
    }

    let mut translation = [0.0_f32; 3];
    let mut primitive_count = 0_usize;
    for (source_instruction, instruction) in config
        .sdf_program
        .iter()
        .take(config.sdf_program_count as usize)
        .enumerate()
    {
        if !instruction.data.iter().all(|value| value.is_finite()) {
            return false;
        }
        match instruction.opcode {
            SDF_OP_TRANSLATE => {
                for (current, delta) in translation.iter_mut().zip(instruction.data) {
                    *current += delta;
                }
            }
            SDF_OP_SPHERE => {
                if instruction.flags != 0 || primitive_count >= SDF_FLAT_UNION_MAX_PRIMITIVES {
                    return false;
                }
                let index = config.sdf_typed_soa.sphere_count as usize;
                config.sdf_typed_soa.sphere_x[index] = translation[0];
                config.sdf_typed_soa.sphere_y[index] = translation[1];
                config.sdf_typed_soa.sphere_z[index] = translation[2];
                config.sdf_typed_soa.sphere_radius[index] = instruction.data[0];
                config.sdf_typed_soa.sphere_source[index] = source_instruction as u32;
                config.sdf_typed_soa.sphere_count += 1;
                primitive_count += 1;
            }
            SDF_OP_BOX => {
                if instruction.flags != 0 || primitive_count >= SDF_FLAT_UNION_MAX_PRIMITIVES {
                    return false;
                }
                let index = config.sdf_typed_soa.box_count as usize;
                config.sdf_typed_soa.box_x[index] = translation[0];
                config.sdf_typed_soa.box_y[index] = translation[1];
                config.sdf_typed_soa.box_z[index] = translation[2];
                config.sdf_typed_soa.box_half_x[index] = instruction.data[0].abs();
                config.sdf_typed_soa.box_half_y[index] = instruction.data[1].abs();
                config.sdf_typed_soa.box_half_z[index] = instruction.data[2].abs();
                config.sdf_typed_soa.box_source[index] = source_instruction as u32;
                config.sdf_typed_soa.box_count += 1;
                primitive_count += 1;
            }
            SDF_OP_PLANE => {
                if instruction.flags != 0 || primitive_count >= SDF_FLAT_UNION_MAX_PRIMITIVES {
                    return false;
                }
                let length = (instruction.data[0] * instruction.data[0]
                    + instruction.data[1] * instruction.data[1]
                    + instruction.data[2] * instruction.data[2])
                    .sqrt();
                if !length.is_finite() || length <= 1.0e-8 {
                    return false;
                }
                let index = config.sdf_typed_soa.plane_count as usize;
                config.sdf_typed_soa.plane_x[index] = instruction.data[0];
                config.sdf_typed_soa.plane_y[index] = instruction.data[1];
                config.sdf_typed_soa.plane_z[index] = instruction.data[2];
                config.sdf_typed_soa.plane_center_x[index] = translation[0];
                config.sdf_typed_soa.plane_center_y[index] = translation[1];
                config.sdf_typed_soa.plane_center_z[index] = translation[2];
                config.sdf_typed_soa.plane_offset[index] = instruction.data[3];
                config.sdf_typed_soa.plane_source[index] = source_instruction as u32;
                config.sdf_typed_soa.plane_count += 1;
                primitive_count += 1;
            }
            SDF_OP_ORBIT_ADD | SDF_OP_UNION => {}
            _ => return false,
        }
        if !translation.iter().all(|value| value.is_finite()) {
            return false;
        }
    }
    primitive_count > 0
}

fn append_program_op(config: &mut FptRenderConfig, opcode: u32, data: [f32; 4]) -> Result<()> {
    ensure!(
        (config.sdf_program_count as usize) < SDF_PROGRAM_MAX_OPS,
        "SDF program is too large"
    );
    let index = config.sdf_program_count as usize;
    config.sdf_program[index] = FptSdfInstruction {
        opcode,
        data,
        ..Default::default()
    };
    config.sdf_program_count += 1;
    Ok(())
}

fn parse_fpt_vec3(line: &str) -> Option<[f32; 3]> {
    let mut values = line
        .trim_matches(|value: char| value.is_whitespace() || value == '[' || value == ']')
        .split(',');
    Some([
        values.next()?.trim().parse().ok()?,
        values.next()?.trim().parse().ok()?,
        values.next()?.trim().parse().ok()?,
    ])
}

fn compile_gradient_example(config: &mut FptRenderConfig) -> Result<()> {
    config.sdf_id = SDF_PROGRAM;
    config.material_mode = 2;
    let angle1 = -9.83 + config.set_values[2] / 2.0;
    let angle2 = -1.16 + config.set_values[3] / 2.0;
    let shift = [
        -3.508 + config.set_values[4] * 7.0,
        -3.593 + config.set_values[5] * 7.0,
        3.295 + config.set_values[6] * 7.0,
    ];
    for _ in 0..10 {
        append_program_op(config, SDF_OP_ABS, [0.0; 4])?;
        append_program_op(config, SDF_OP_ROTATE_Z, [-angle1, 0.0, 0.0, 0.0])?;
        append_program_op(config, SDF_OP_SORT_DESC, [0.0; 4])?;
        append_program_op(config, SDF_OP_ROTATE_X, [-angle2, 0.0, 0.0, 0.0])?;
        append_program_op(config, SDF_OP_SCALE, [1.9, 0.0, 0.0, 0.0])?;
        append_program_op(
            config,
            SDF_OP_TRANSLATE,
            [-shift[0], -shift[1], -shift[2], 0.0],
        )?;
    }
    append_program_op(config, SDF_OP_BOX, [6.0, 6.0, 6.0, 1.0])
}

fn load_fpt_settings(path: &Path, config: &mut FptRenderConfig) -> Result<()> {
    let data = fs::read_to_string(path)?;
    let Some(marker) = data.find("SDF Settings:") else {
        return Ok(());
    };
    let mut mode = 0;
    let mut set_index = 0;
    let mut vector_index = 0;
    for raw in data[marker..].lines() {
        let line = raw.trim();
        if line == "{" {
            mode += 1;
            continue;
        }
        if line == "}" {
            continue;
        }
        if mode == 1 && set_index < 40 {
            if let Ok(value) = line.parse::<f32>() {
                config.set_values[set_index] = value;
                set_index += 1;
            }
        } else if mode == 2
            && vector_index < 40
            && let Some(value) = parse_fpt_vec3(line)
        {
            config.vset_values[vector_index * 3..vector_index * 3 + 3].copy_from_slice(&value);
            vector_index += 1;
        }
    }
    if config.sdf_id == SDF_PROGRAM && data.contains("Gradient(Orbit_Trap)") {
        compile_gradient_example(config)?;
    }
    Ok(())
}

pub fn load_scene_config(args: &RenderArgs) -> Result<LoadedScene> {
    if args.scene_path.extension().and_then(|value| value.to_str()) == Some("fract") {
        return load_mandelbulber_scene_config(args);
    }
    let data = fs::read_to_string(&args.scene_path)?;
    let root: Value = serde_json::from_str(&data)?;
    let object = root
        .as_object()
        .ok_or_else(|| anyhow!("scene root must be an object"))?;
    let preset = object
        .get("preset")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("scene is missing preset"))?;
    let preset_path = resolve_preset_path(&args.scene_path, &args.fpt_root, preset);
    let mut config = default_config();
    config.sdf_id = sdf_id_from_preset(&preset_path)?;
    config.preview = u32::from(args.preview);
    config.sdf_profile = u32::from(args.sdf_profile);
    config.glass_mode = args.glass_mode as u32;
    config.width = args
        .width
        .unwrap_or_else(|| number_u32(object.get("width"), 768));
    config.height = args
        .height
        .unwrap_or_else(|| number_u32(object.get("height"), 768));
    config.samples = args
        .samples
        .unwrap_or_else(|| number_u32(object.get("samples"), 64));
    ensure!(
        (1..=512).contains(&config.samples),
        "scene samples must be 1..512"
    );
    if let Some(value) = object.get("camera") {
        apply_camera(&mut config, value);
    }
    if let Some(value) = object.get("render") {
        apply_float_object(
            &mut config.render,
            value,
            &[
                "bounces",
                "marching_steps",
                "normal_epsilon",
                "min_distance",
                "max_distance",
                "adaptive_marching",
                "error_protection",
                "random_mode",
            ],
        );
    }
    if let Some(value) = object.get("world") {
        apply_float_object(
            &mut config.world,
            value,
            &[
                "mode",
                "light_size",
                "rotation",
                "elevation",
                "power",
                "contrast",
                "gradient_background",
            ],
        );
    }
    if let Some(world) = object.get("world").and_then(Value::as_object) {
        if let Some(value) = world.get("one_color") {
            copy_float_array(&mut config.world_one_color, value);
        }
        if let Some(values) = world.get("background_gradient").and_then(Value::as_array)
            && values.len() >= 2
        {
            copy_float_array(&mut config.background_gradient[..3], &values[0]);
            copy_float_array(&mut config.background_gradient[3..], &values[1]);
        }
    }
    if let Some(value) = object.get("sun") {
        apply_float_object(
            &mut config.sun,
            value,
            &["enabled", "rotation", "elevation", "power", "softness"],
        );
    }
    if let Some(color) = object
        .get("sun")
        .and_then(Value::as_object)
        .and_then(|value| value.get("color"))
    {
        copy_float_array(&mut config.sun_color, color);
    }
    if let Some(value) = object.get("post") {
        apply_float_object(
            &mut config.post,
            value,
            &[
                "tone_map",
                "exposure",
                "brightness",
                "saturation",
                "contrast",
                "chromatic_aberration",
                "highlights",
            ],
        );
    }
    if let Some(value) = object.get("style") {
        apply_fractal_style(&mut config, value)?;
    }
    if let Some(voxel) = object.get("voxel").and_then(Value::as_object) {
        if let Some(value) = voxel.get("resolution") {
            config.voxel_resolution = number_u32(Some(value), config.voxel_resolution);
        }
        ensure!(
            (32..=512).contains(&config.voxel_resolution),
            "voxel resolution must be 32..512"
        );
        if let Some(value) = voxel.get("bounds_min") {
            copy_float_array(&mut config.voxel_bounds_min, value);
        }
        if let Some(value) = voxel.get("bounds_max") {
            copy_float_array(&mut config.voxel_bounds_max, value);
        }
        config.voxel_surface_band =
            number_f32(voxel.get("surface_band"), config.voxel_surface_band);
        if let Some(storage) = voxel.get("storage").and_then(Value::as_str) {
            config.voxel_storage = match storage {
                "dense" => VOXEL_STORAGE_DENSE,
                "sparse-bricks" => VOXEL_STORAGE_SPARSE_BRICKS,
                "template-bricks" => VOXEL_STORAGE_TEMPLATE_BRICKS,
                value => bail!("invalid voxel storage mode: {value}"),
            };
        }
        config.voxel_fill_interior = u32::from(
            voxel
                .get("fill_interior")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        );
        if let Some(coverage) = voxel.get("coverage").and_then(Value::as_str) {
            config.voxel_coverage_mode = match coverage {
                "legacy" => VOXEL_COVERAGE_LEGACY,
                "lipschitz" => VOXEL_COVERAGE_LIPSCHITZ,
                "interval" => VOXEL_COVERAGE_INTERVAL,
                value => bail!("invalid voxel coverage mode: {value}"),
            };
        }
        if let Some(build) = voxel.get("build").and_then(Value::as_str) {
            config.voxel_build_mode = match build {
                "staging" => VOXEL_BUILD_STAGING,
                "direct" => VOXEL_BUILD_DIRECT,
                value => bail!("invalid voxel build mode: {value}"),
            };
        }
        config.voxel_brick_rejection = u32::from(
            voxel
                .get("brick_rejection")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        );
        if let Some(refinement) = voxel.get("leaf_refinement").and_then(Value::as_str) {
            config.voxel_leaf_refinement = match refinement {
                "none" => VOXEL_LEAF_REFINEMENT_NONE,
                "secant-bisection" => VOXEL_LEAF_REFINEMENT_SECANT_BISECTION,
                "restricted-trace" => VOXEL_LEAF_REFINEMENT_RESTRICTED_TRACE,
                "fixed-de" => VOXEL_LEAF_REFINEMENT_FIXED_DE,
                value => bail!("invalid voxel leaf refinement: {value}"),
            };
        }
        if let Some(material) = voxel.get("material").and_then(Value::as_str) {
            config.voxel_material_mode = match material {
                "stored" => VOXEL_MATERIAL_STORED,
                "exact" => VOXEL_MATERIAL_EXACT,
                value => bail!("invalid voxel material mode: {value}"),
            };
        }
        if let Some(offset) = voxel.get("offset").and_then(Value::as_str) {
            config.voxel_offset_mode = match offset {
                "legacy" => VOXEL_OFFSET_LEGACY,
                "precision" => VOXEL_OFFSET_PRECISION,
                value => bail!("invalid voxel offset mode: {value}"),
            };
        }
    }
    ensure!(
        config
            .voxel_bounds_min
            .iter()
            .zip(config.voxel_bounds_max)
            .all(|(min, max)| *min < max),
        "voxel bounds_max must be greater than bounds_min"
    );
    load_fpt_settings(&preset_path, &mut config)?;
    if let Some(value) = object.get("sdf_program") {
        apply_sdf_program(&mut config, value)?;
    }
    config.sdf_program_source_count = config.sdf_program_count;
    config.sdf_program_optimization = args.sdf_program_optimization as u32;
    if config.sdf_id == SDF_PROGRAM
        && args.sdf_program_optimization == SdfProgramOptimization::Basic
    {
        optimize_sdf_program(&mut config);
    }
    if config.sdf_id == SDF_PROGRAM {
        split_sdf_geometry_program(&mut config, args.sdf_geometry_split);
        if args.sdf_canonical_ir {
            compile_canonical_geometry_program(&mut config);
        }
    }
    ensure!(
        !args.sdf_topology_specialization || args.renderer_backend == RendererBackend::Sdf,
        "topology specialization currently requires the direct SDF renderer"
    );
    ensure!(
        !args.sdf_runtime_source_bytecode || args.renderer_backend == RendererBackend::Sdf,
        "runtime-source bytecode currently requires the direct SDF renderer"
    );
    ensure!(
        !args.sdf_dual_generated_library || args.renderer_backend == RendererBackend::Sdf,
        "dual generated libraries currently require the direct SDF renderer"
    );
    ensure!(
        !args.sdf_tiny_linked_helper || args.renderer_backend == RendererBackend::Sdf,
        "tiny linked helpers currently require the direct SDF renderer"
    );
    ensure!(
        !args.sdf_dual_generated_library
            || args.sdf_topology_specialization
            || args.sdf_function_stitching == SdfFunctionStitching::Auto,
        "dual generated libraries require topology specialization or automatic backend probing"
    );
    ensure!(
        !args.sdf_tiny_linked_helper || args.sdf_topology_specialization,
        "tiny linked helpers require topology specialization"
    );
    ensure!(
        !args.sdf_tiny_linked_helper || !args.sdf_dual_generated_library,
        "tiny linked helpers and dual generated libraries are separate experiments"
    );
    ensure!(
        !args.sdf_typed_soa || args.renderer_backend == RendererBackend::Sdf,
        "typed-SoA lowering currently requires the direct SDF renderer"
    );
    ensure!(
        !args.sdf_topology_specialization || !args.sdf_runtime_source_bytecode,
        "topology specialization and runtime-source bytecode are separate compiler controls"
    );
    ensure!(
        args.sdf_function_stitching == SdfFunctionStitching::Off
            || args.renderer_backend == RendererBackend::Sdf,
        "function stitching currently requires the direct SDF renderer"
    );
    ensure!(
        args.sdf_function_stitching == SdfFunctionStitching::Off
            || (!args.sdf_topology_specialization && !args.sdf_runtime_source_bytecode),
        "function stitching and runtime-source compilation are separate compiler controls"
    );
    ensure!(
        args.sdf_function_stitching == SdfFunctionStitching::Off || !args.sdf_flat_union,
        "function stitching and flat-union lowering are separate evaluator modes"
    );
    ensure!(
        args.sdf_function_stitching == SdfFunctionStitching::Off || !args.sdf_typed_soa,
        "function stitching and typed-SoA lowering are separate evaluator modes"
    );
    ensure!(
        !args.sdf_stitch_validation
            || args.sdf_function_stitching != SdfFunctionStitching::Off
            || args.sdf_topology_specialization,
        "program validation requires function stitching or topology-specialized MSL"
    );
    ensure!(
        !args.sdf_stitch_distance_only
            || matches!(
                args.sdf_function_stitching,
                SdfFunctionStitching::Normal | SdfFunctionStitching::AlwaysInline
            ),
        "lean distance-only stitching requires an explicit normal or inline stitching mode"
    );
    ensure!(
        !args.sdf_stitch_split_graph
            || matches!(
                args.sdf_function_stitching,
                SdfFunctionStitching::Normal | SdfFunctionStitching::AlwaysInline
            ),
        "split-graph stitching requires an explicit normal or inline stitching mode"
    );
    ensure!(
        !args.sdf_stitch_split_graph || !args.sdf_stitch_distance_only,
        "split-graph stitching already selects the lean distance state"
    );
    ensure!(
        args.sdf_stitch_fusion == SdfStitchFusion::Off
            || matches!(
                args.sdf_function_stitching,
                SdfFunctionStitching::Normal | SdfFunctionStitching::AlwaysInline
            ),
        "stitch fusion requires an explicit normal or inline stitching mode"
    );
    ensure!(
        args.sdf_stitch_fusion == SdfStitchFusion::Off
            || (!args.sdf_stitch_distance_only && !args.sdf_stitch_split_graph),
        "stitch fusion currently requires the full stitched surface state"
    );
    ensure!(
        !args.sdf_topology_specialization || !args.sdf_flat_union,
        "topology specialization and flat-union lowering are separate evaluator modes"
    );
    ensure!(
        !args.sdf_topology_specialization || !args.sdf_typed_soa,
        "topology specialization and typed-SoA lowering are separate evaluator modes"
    );
    ensure!(
        !args.sdf_flat_union || !args.sdf_typed_soa,
        "flat-union and typed-SoA lowering are separate evaluator modes"
    );
    ensure!(
        args.sdf_function_stitching != SdfFunctionStitching::Auto
            || (!args.sdf_flat_union && !args.sdf_typed_soa),
        "automatic SDF backend selection owns the direct evaluator choice"
    );
    if args.sdf_flat_union {
        lower_flat_union_program(&mut config);
    }
    if args.sdf_typed_soa {
        ensure!(
            lower_typed_soa_program(&mut config),
            "typed-SoA lowering requires a translation-only hard union of spheres, boxes, or planes"
        );
    } else if args.sdf_function_stitching == SdfFunctionStitching::Auto {
        // Auto selection compares both generated-MSL variants against the
        // strongest direct evaluator. Unsupported programs retain bytecode.
        lower_typed_soa_program(&mut config);
    }
    ensure!(
        config.sdf_id != SDF_PROGRAM || config.sdf_program_count > 0,
        "preset '{}' needs a typed sdf_program",
        preset_path.display()
    );
    if let Some(hdri) = object
        .get("world")
        .and_then(Value::as_object)
        .and_then(|value| value.get("hdri"))
        .and_then(Value::as_str)
    {
        set_hdri_path(&mut config, &resolve_scene_path(&args.scene_path, hdri));
    }
    let output_name = object
        .get("output")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{}.png", stem(&preset_path).unwrap_or("render")));
    Ok(LoadedScene {
        config,
        output_name,
        runtime_metal_source: None,
    })
}

fn load_mandelbulber_scene_config(args: &RenderArgs) -> Result<LoadedScene> {
    ensure!(
        args.renderer_backend == RendererBackend::Sdf,
        "Mandelbulber scenes currently support only the direct sdf renderer"
    );
    ensure!(
        !args.sdf_topology_specialization
            && !args.sdf_runtime_source_bytecode
            && args.sdf_function_stitching == SdfFunctionStitching::Off
            && !args.sdf_flat_union
            && !args.sdf_typed_soa,
        "typed-program compiler options do not apply to Mandelbulber iterative-DE scenes"
    );

    let mut scene = MandelbulberScene::load(&args.scene_path)?;
    // Mandelbulber's dynamic distance threshold is resolution-dependent.
    // Apply CLI dimensions before deriving the render configuration so a
    // reduced-resolution render traces the same surface as upstream.
    scene.width = args.width.unwrap_or(scene.width);
    scene.height = args.height.unwrap_or(scene.height);
    ensure!(
        (!scene.hybrid_enabled && scene.formula_id == 10) || args.mandelbulber_root.is_some(),
        "formula ID {} requires --mandelbulber-root so its Metal source can be generated",
        scene.formula_id
    );
    let mut config = default_config();
    scene.apply_to_config(&mut config);

    let runtime_metal_source = if let Some(source_root) = &args.mandelbulber_root {
        let source = crate::mandelbulber::compiler::specialize_scene_with_kernel_specialization(
            include_str!("../shaders/Shaders.metal"),
            &mut scene,
            source_root,
            &config.set_values,
            crate::mandelbulber::compiler::scene_uses_kernel_specialization(&args.scene_path)?,
            crate::mandelbulber::compiler::scene_uses_direct_hybrid_loop(&args.scene_path)?,
            crate::mandelbulber::compiler::scene_formula_optimization_policy(&args.scene_path)?,
        )?;
        config.sdf_runtime_source_bytecode = 1;
        Some(source.into_bytes())
    } else {
        None
    };
    scene.apply_to_config(&mut config);
    config.preview = u32::from(args.preview);
    config.sdf_profile = u32::from(args.sdf_profile);
    config.width = scene.width;
    config.height = scene.height;
    config.samples = args.samples.unwrap_or(64);
    ensure!(
        (1..=512).contains(&config.samples),
        "scene samples must be 1..512"
    );
    let name = stem(&args.scene_path).unwrap_or("mandelbulber");
    Ok(LoadedScene {
        config,
        output_name: format!("{name}.png"),
        runtime_metal_source,
    })
}

pub fn preview_scene_list(
    args: &RenderArgs,
    current: &FptRenderConfig,
) -> Result<(Vec<FptRenderConfig>, u32)> {
    let bundled = [
        PathBuf::from("scenes/readme/01-Render005.json"),
        PathBuf::from("scenes/readme/08-Render0ad03.json"),
        PathBuf::from("scenes/readme/09-Glass.json"),
    ];
    let relatives = [
        "Beauty/Cornell_Box.json",
        "Beauty/Glass_Ball.json",
        "Beauty/Fractals/Ball_Fractal.json",
        "Beauty/Fractals/Cage_Fractal.json",
        "Beauty/Fractals/IFS_Fractal.json",
        "Beauty/Fractals/Mandelbox_Fractal.json",
        "Beauty/Fractals/Menger_Sponge.json",
        "Beauty/Fractals/Tower_Fractal.json",
        "Beauty/Fractals/Tree_Fractal.json",
    ];
    let mut scene_paths: Vec<PathBuf> = bundled.into_iter().filter(|path| path.exists()).collect();
    scene_paths.extend(relatives.into_iter().map(|path| args.fpt_root.join(path)));
    let mut configs = Vec::with_capacity(scene_paths.len());
    for scene_path in scene_paths {
        let mut scene_args = args.clone();
        scene_args.scene_path = scene_path;
        if !scene_args.scene_path.exists() {
            continue;
        }
        let mut loaded = load_scene_config(&scene_args)?;
        loaded.config.preview = u32::from(!args.live_pathtrace);
        loaded.config.samples = args.samples.unwrap_or(512);
        apply_optimization_args(&mut loaded.config, args);
        if args.width.is_none() {
            loaded.config.width = loaded.config.width.min(1280);
        }
        if args.height.is_none() {
            loaded.config.height = loaded.config.height.min(800);
        }
        configs.push(loaded.config);
    }
    Ok(prioritize_preview_config(current, configs))
}

fn prioritize_preview_config(
    current: &FptRenderConfig,
    mut candidates: Vec<FptRenderConfig>,
) -> (Vec<FptRenderConfig>, u32) {
    if current.renderer_backend == RENDERER_VOXEL {
        candidates.retain(|candidate| {
            candidate.sdf_id != current.sdf_id
                || candidate.camera_position != current.camera_position
                || candidate.camera_yaw_pitch != current.camera_yaw_pitch
                || candidate.camera_roll != current.camera_roll
                || candidate.fractal_style != current.fractal_style
                || candidate.post != current.post
        });
    } else {
        candidates.retain(|candidate| candidate.sdf_id != current.sdf_id);
    }
    candidates.insert(0, *current);
    (candidates, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_mapping() {
        assert_eq!(
            sdf_id_from_preset(Path::new("/tmp/IFS_Fractal.fpt")).unwrap(),
            SDF_IFS_FRACTAL
        );
        assert_eq!(
            sdf_id_from_preset(Path::new("Fractals/Menger_Sponge.fpt")).unwrap(),
            SDF_MENGER_SPONGE
        );
    }

    #[test]
    fn typed_program_parses() {
        let value: Value = serde_json::from_str(r#"{"operations":[{"op":"repeat","value":[2,2,2]},{"op":"sphere","radius":0.7,"orbit_weight":1}],"material":{"mode":"gradient"},"gradient":[{"position":0,"color":[1,0,0]},{"position":1,"color":[0,0,1]}]}"#).unwrap();
        let mut config = default_config();
        apply_sdf_program(&mut config, &value).unwrap();
        assert_eq!(config.sdf_program_count, 2);
        assert_eq!(config.gradient_count, 2);
    }

    #[test]
    fn typed_program_gradient_mode_parses() {
        let args = vec![
            "scene.json".to_owned(),
            "--sdf-normal-mode".to_owned(),
            "program-gradient".to_owned(),
        ];
        assert_eq!(
            parse_render_args(&args).unwrap().sdf_normal_mode,
            SdfNormalMode::ProgramGradient
        );
    }

    #[test]
    fn approximation_controls_parse_and_default_off() {
        let defaults = parse_render_args(&["scene.json".to_owned()]).unwrap();
        assert_eq!(defaults.mandel_iteration_scale, None);
        assert_eq!(defaults.mandel_screen_lod_rate, None);
        assert!(!defaults.mandel_optimization_auto);
        assert_eq!(defaults.mandel_selection_cache, None);

        let args = vec![
            "scene.json".to_owned(),
            "--mandel-iteration-scale".to_owned(),
            "0.75".to_owned(),
            "--mandel-screen-lod-rate".to_owned(),
            "1.5".to_owned(),
            "--mandel-optimization".to_owned(),
            "auto".to_owned(),
            "--mandel-selection-cache".to_owned(),
            "selection.json".to_owned(),
        ];
        let parsed = parse_render_args(&args).unwrap();
        assert_eq!(parsed.mandel_iteration_scale, Some(0.75));
        assert_eq!(parsed.mandel_screen_lod_rate, Some(1.5));
        assert!(parsed.mandel_optimization_auto);
        assert_eq!(
            parsed.mandel_selection_cache,
            Some(PathBuf::from("selection.json"))
        );
    }

    #[test]
    fn mandel_selection_cache_is_strict_and_manual_controls_win() {
        let directory = std::env::temp_dir().join(format!(
            "fpt-mandel-selection-cache-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        let scene_path = directory.join("fixture.fract");
        fs::write(&scene_path, "formula fixture").unwrap();
        let digest = format!("{:x}", Sha256::digest(fs::read(&scene_path).unwrap()));
        let cache_path = directory.join("selection.json");
        let key = format!("{digest}:640x360:1");
        fs::write(
            &cache_path,
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 1,
                "minimum_ssim": 0.98,
                "minimum_speedup": 1.05,
                "entries": {
                    (key): {
                        "scene_sha256": digest,
                        "width": 640,
                        "height": 360,
                        "samples": 1,
                        "ssim": 0.995,
                        "speedup": 1.2,
                        "selection": {"kind": "screen_lod", "value": 1.0}
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();
        let base = vec![
            scene_path.display().to_string(),
            "--mandel-optimization".to_owned(),
            "auto".to_owned(),
            "--mandel-selection-cache".to_owned(),
            cache_path.display().to_string(),
        ];
        let args = parse_render_args(&base).unwrap();
        let mut config = default_config();
        config.sdf_id = SDF_MANDELBULBER;
        config.width = 640;
        config.height = 360;
        config.samples = 1;
        apply_optimization_args(&mut config, &args);
        assert_eq!(
            config.vset_values[crate::mandelbulber::VPARAM_SCREEN_LOD_RATE],
            1.0
        );

        let mut manual = base;
        manual.extend(["--mandel-screen-lod-rate".to_owned(), "0.5".to_owned()]);
        let args = parse_render_args(&manual).unwrap();
        apply_optimization_args(&mut config, &args);
        assert_eq!(
            config.vset_values[crate::mandelbulber::VPARAM_SCREEN_LOD_RATE],
            0.5
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn typed_program_optimization_argument_parses() {
        let defaults = parse_render_args(&["scene.json".to_owned()]).unwrap();
        assert_eq!(
            defaults.sdf_program_optimization,
            SdfProgramOptimization::Basic
        );
        assert!(defaults.sdf_geometry_split);
        assert!(!defaults.sdf_topology_specialization);
        assert!(!defaults.sdf_runtime_source_bytecode);
        assert!(!defaults.sdf_dual_generated_library);
        assert!(!defaults.sdf_tiny_linked_helper);
        assert_eq!(defaults.sdf_function_stitching, SdfFunctionStitching::Off);
        assert!(!defaults.sdf_flat_union);
        assert!(!defaults.sdf_typed_soa);
        assert_eq!(defaults.renderer_backend, RendererBackend::Sdf);
        let args = vec![
            "scene.json".to_owned(),
            "--sdf-program-optimization".to_owned(),
            "basic".to_owned(),
            "--sdf-geometry-split".to_owned(),
            "--sdf-topology-specialization".to_owned(),
            "--sdf-runtime-source-bytecode".to_owned(),
            "--sdf-function-stitching".to_owned(),
            "inline".to_owned(),
            "--sdf-flat-union".to_owned(),
            "--sdf-typed-soa".to_owned(),
        ];
        let parsed = parse_render_args(&args).unwrap();
        assert_eq!(
            parsed.sdf_program_optimization,
            SdfProgramOptimization::Basic
        );
        assert!(parsed.sdf_geometry_split);
        assert!(parsed.sdf_topology_specialization);
        assert!(parsed.sdf_runtime_source_bytecode);
        assert_eq!(
            parsed.sdf_function_stitching,
            SdfFunctionStitching::AlwaysInline
        );
        assert!(parsed.sdf_flat_union);
        assert!(parsed.sdf_typed_soa);

        let canonical = parse_render_args(&[
            "scene.json".to_owned(),
            "--sdf-canonical-topology-specialization".to_owned(),
        ])
        .unwrap();
        assert!(canonical.sdf_topology_specialization);
        assert!(canonical.sdf_canonical_topology_specialization);

        let compact = parse_render_args(&[
            "scene.json".to_owned(),
            "--sdf-compact-canonical-topology-specialization".to_owned(),
        ])
        .unwrap();
        assert!(compact.sdf_topology_specialization);
        assert!(compact.sdf_compact_canonical_topology_specialization);

        let shared = parse_render_args(&[
            "scene.json".to_owned(),
            "--sdf-shared-transform-topology-specialization".to_owned(),
        ])
        .unwrap();
        assert!(shared.sdf_topology_specialization);
        assert!(shared.sdf_shared_transform_topology_specialization);

        let indexed = parse_render_args(&[
            "scene.json".to_owned(),
            "--sdf-affine-index-topology-specialization".to_owned(),
        ])
        .unwrap();
        assert!(indexed.sdf_topology_specialization);
        assert!(indexed.sdf_affine_index_topology_specialization);

        let automatic = parse_render_args(&[
            "scene.json".to_owned(),
            "--sdf-function-stitching".to_owned(),
            "auto".to_owned(),
        ])
        .unwrap();
        assert_eq!(automatic.sdf_function_stitching, SdfFunctionStitching::Auto);
        assert!(automatic.sdf_backend_probe);
        let automatic_backend = parse_render_args(&[
            "scene.json".to_owned(),
            "--sdf-backend".to_owned(),
            "auto".to_owned(),
        ])
        .unwrap();
        assert_eq!(
            automatic_backend.sdf_function_stitching,
            SdfFunctionStitching::Auto
        );
        assert!(!automatic_backend.sdf_backend_probe);
        let probing_backend = parse_render_args(&[
            "scene.json".to_owned(),
            "--sdf-backend".to_owned(),
            "probe".to_owned(),
        ])
        .unwrap();
        assert_eq!(
            probing_backend.sdf_function_stitching,
            SdfFunctionStitching::Auto
        );
        assert!(probing_backend.sdf_backend_probe);

        let dual_library = parse_render_args(&[
            "scene.json".to_owned(),
            "--sdf-topology-specialization".to_owned(),
            "--sdf-dual-generated-library".to_owned(),
        ])
        .unwrap();
        assert!(dual_library.sdf_dual_generated_library);

        let tiny_helper = parse_render_args(&[
            "scene.json".to_owned(),
            "--sdf-topology-specialization".to_owned(),
            "--sdf-tiny-linked-helper".to_owned(),
        ])
        .unwrap();
        assert!(tiny_helper.sdf_tiny_linked_helper);

        let lean = parse_render_args(&[
            "scene.json".to_owned(),
            "--sdf-function-stitching".to_owned(),
            "inline".to_owned(),
            "--sdf-stitch-distance-only".to_owned(),
        ])
        .unwrap();
        assert!(lean.sdf_stitch_distance_only);

        let split = parse_render_args(&[
            "scene.json".to_owned(),
            "--sdf-function-stitching".to_owned(),
            "normal".to_owned(),
            "--sdf-stitch-split-graph".to_owned(),
        ])
        .unwrap();
        assert!(split.sdf_stitch_split_graph);
        assert!(!split.sdf_stitch_distance_only);

        let unsplit = parse_render_args(&[
            "scene.json".to_owned(),
            "--no-sdf-geometry-split".to_owned(),
        ])
        .unwrap();
        assert!(!unsplit.sdf_geometry_split);

        let invalid = vec![
            "scene.json".to_owned(),
            "--sdf-program-optimization".to_owned(),
            "aggressive".to_owned(),
        ];
        assert!(parse_render_args(&invalid).is_err());
    }

    #[test]
    fn basic_program_optimizer_folds_local_identities() {
        let mut config = default_config();
        config.sdf_id = SDF_PROGRAM;
        append_program_op(&mut config, SDF_OP_TRANSLATE, [1.0, 2.0, 3.0, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_TRANSLATE, [4.0, 5.0, 6.0, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_ABS, [0.0; 4]).unwrap();
        append_program_op(&mut config, SDF_OP_ABS, [0.0; 4]).unwrap();
        append_program_op(&mut config, SDF_OP_ROTATE_X, [0.0; 4]).unwrap();
        append_program_op(&mut config, SDF_OP_SPHERE, [0.75, 0.0, 0.0, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_SPHERE, [0.75, 0.0, 0.0, 0.0]).unwrap();

        optimize_sdf_program(&mut config);

        assert_eq!(config.sdf_program_count, 3);
        assert_eq!(config.sdf_program[0].opcode, SDF_OP_TRANSLATE);
        assert_eq!(config.sdf_program[0].data, [5.0, 7.0, 9.0, 0.0]);
        assert_eq!(config.sdf_program[1].opcode, SDF_OP_ABS);
        assert_eq!(config.sdf_program[2].opcode, SDF_OP_SPHERE);
    }

    #[test]
    fn basic_program_optimizer_preserves_orbit_and_subtraction_operations() {
        let mut config = default_config();
        config.sdf_id = SDF_PROGRAM;
        append_program_op(&mut config, SDF_OP_SPHERE, [0.75, 0.0, 0.0, 1.0]).unwrap();
        append_program_op(&mut config, SDF_OP_SPHERE, [0.75, 0.0, 0.0, 1.0]).unwrap();
        append_program_op(&mut config, SDF_OP_BOX, [1.0, 1.0, 1.0, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_BOX, [1.0, 1.0, 1.0, 0.0]).unwrap();
        config.sdf_program[3].flags = 2;

        optimize_sdf_program(&mut config);

        assert_eq!(config.sdf_program_count, 4);
        assert_eq!(config.sdf_program[0].data[3], 1.0);
        assert_eq!(config.sdf_program[1].data[3], 1.0);
        assert_eq!(config.sdf_program[3].flags, 2);
    }

    #[test]
    fn basic_program_optimizer_does_not_fold_across_geometry() {
        let mut config = default_config();
        config.sdf_id = SDF_PROGRAM;
        append_program_op(&mut config, SDF_OP_TRANSLATE, [1.0, 0.0, 0.0, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_SPHERE, [0.75, 0.0, 0.0, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_TRANSLATE, [1.0, 0.0, 0.0, 0.0]).unwrap();

        optimize_sdf_program(&mut config);

        assert_eq!(config.sdf_program_count, 3);
        assert_eq!(config.sdf_program[0].opcode, SDF_OP_TRANSLATE);
        assert_eq!(config.sdf_program[1].opcode, SDF_OP_SPHERE);
        assert_eq!(config.sdf_program[2].opcode, SDF_OP_TRANSLATE);
    }

    #[test]
    fn geometry_split_preserves_full_shading_program() {
        let mut config = default_config();
        config.sdf_id = SDF_PROGRAM;
        append_program_op(&mut config, SDF_OP_TRANSLATE, [1.0, 2.0, 3.0, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_ORBIT_ADD, [0.5, 0.25, 0.75, 0.8]).unwrap();
        append_program_op(&mut config, SDF_OP_SPHERE, [0.75, 0.0, 0.0, 0.6]).unwrap();
        append_program_op(&mut config, SDF_OP_UNION, [0.0; 4]).unwrap();

        split_sdf_geometry_program(&mut config, true);

        assert_eq!(config.sdf_geometry_split, 1);
        assert_eq!(config.sdf_shading_program_count, 4);
        assert_eq!(config.sdf_shading_program[1].opcode, SDF_OP_ORBIT_ADD);
        assert_eq!(config.sdf_shading_program[2].data[3], 0.6);
        assert_eq!(config.sdf_program_count, 2);
        assert_eq!(config.sdf_program[0].opcode, SDF_OP_TRANSLATE);
        assert_eq!(config.sdf_program[1].opcode, SDF_OP_SPHERE);
        assert_eq!(config.sdf_program[1].data[3], 0.6);
    }

    #[test]
    fn flat_union_lowering_composes_affine_transforms() {
        let mut config = default_config();
        config.sdf_id = SDF_PROGRAM;
        append_program_op(&mut config, SDF_OP_TRANSLATE, [1.0, 2.0, 3.0, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_SCALE, [2.0, 0.0, 0.0, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_SPHERE, [0.75, 0.0, 0.0, 0.0]).unwrap();

        assert!(lower_flat_union_program(&mut config));
        assert_eq!(config.sdf_flat_union_count, 1);
        let instance = config.sdf_flat_union_instances[0];
        assert_eq!(instance.opcode, SDF_OP_SPHERE);
        assert_eq!(instance.distance_scale, 2.0);
        assert_eq!(instance.source_instruction, 2);
        assert_eq!(
            instance.transform,
            [
                2.0, 0.0, 0.0, -2.0, 0.0, 2.0, 0.0, -4.0, 0.0, 0.0, 2.0, -6.0,
            ]
        );
    }

    #[test]
    fn flat_union_lowering_rejects_nonlinear_and_non_union_programs() {
        let mut repeated = default_config();
        repeated.sdf_id = SDF_PROGRAM;
        append_program_op(&mut repeated, SDF_OP_REPEAT, [2.0, 2.0, 2.0, 0.0]).unwrap();
        append_program_op(&mut repeated, SDF_OP_SPHERE, [0.5, 0.0, 0.0, 0.0]).unwrap();
        assert!(!lower_flat_union_program(&mut repeated));

        let mut intersected = default_config();
        intersected.sdf_id = SDF_PROGRAM;
        append_program_op(&mut intersected, SDF_OP_SPHERE, [0.8, 0.0, 0.0, 0.0]).unwrap();
        append_program_op(&mut intersected, SDF_OP_BOX, [0.5, 0.5, 0.5, 0.0]).unwrap();
        intersected.sdf_program[1].flags = 1;
        assert!(!lower_flat_union_program(&mut intersected));
    }

    #[test]
    fn canonical_geometry_compiler_composes_affine_transforms() {
        let mut config = default_config();
        config.sdf_id = SDF_PROGRAM;
        append_program_op(&mut config, SDF_OP_TRANSLATE, [1.0, 2.0, 3.0, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_ROTATE_Z, [0.5, 0.0, 0.0, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_SCALE, [2.0, 0.0, 0.0, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_BOX, [0.75, 0.5, 0.25, 0.0]).unwrap();

        assert!(compile_canonical_geometry_program(&mut config));
        assert_eq!(config.sdf_canonical_source_count, 1);
        assert_eq!(config.sdf_canonical_count, 1);
        assert_eq!(config.sdf_canonical_transform_count, 1);
        let primitive = config.sdf_canonical_primitives[0];
        assert_eq!(primitive.opcode, SDF_OP_BOX);
        assert_eq!(primitive.distance_scale, 2.0);
        assert_eq!(primitive.source_instruction, 3);
        assert!(primitive.transform.iter().all(|value| value.is_finite()));
        assert_ne!(
            primitive.transform,
            [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]
        );
        let indexed = config.sdf_indexed_primitives[0];
        assert_eq!(indexed.opcode, SDF_OP_BOX);
        assert_eq!(indexed.transform_index, 0);
        assert_eq!(indexed.source_instruction, 3);
        assert_eq!(indexed.combine_mode, 0);
        assert_eq!(indexed.data, [0.75, 0.5, 0.25, 0.0]);
        assert_eq!(
            config.sdf_canonical_transforms[0].transform,
            primitive.transform
        );
        assert_eq!(config.sdf_canonical_transforms[0].distance_scale, 2.0);
    }

    #[test]
    fn canonical_geometry_compiler_eliminates_nonadjacent_union_duplicates() {
        let mut config = default_config();
        config.sdf_id = SDF_PROGRAM;
        append_program_op(&mut config, SDF_OP_SPHERE, [0.75, 0.0, 0.0, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_TRANSLATE, [1.0, 0.0, 0.0, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_SPHERE, [0.75, 0.0, 0.0, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_TRANSLATE, [-1.0, 0.0, 0.0, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_SPHERE, [0.75, 0.0, 0.0, 0.0]).unwrap();

        assert!(compile_canonical_geometry_program(&mut config));
        assert_eq!(config.sdf_canonical_source_count, 3);
        assert_eq!(config.sdf_canonical_count, 2);
        assert_eq!(config.sdf_canonical_transform_count, 2);
        assert_eq!(config.sdf_canonical_primitives[0].source_instruction, 0);
        assert_eq!(config.sdf_canonical_primitives[1].source_instruction, 2);
        assert_eq!(config.sdf_indexed_primitives[0].transform_index, 0);
        assert_eq!(config.sdf_indexed_primitives[1].transform_index, 1);
    }

    #[test]
    fn canonical_geometry_compiler_rejects_nonlinear_programs() {
        let mut config = default_config();
        config.sdf_id = SDF_PROGRAM;
        append_program_op(&mut config, SDF_OP_REPEAT, [2.0, 2.0, 2.0, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_SPHERE, [0.5, 0.0, 0.0, 0.0]).unwrap();

        assert!(!compile_canonical_geometry_program(&mut config));
        assert_eq!(config.sdf_canonical_count, 0);
    }

    #[test]
    fn canonical_geometry_compiler_normalizes_plane_normal_not_offset() {
        let mut config = default_config();
        config.sdf_id = SDF_PROGRAM;
        append_program_op(&mut config, SDF_OP_PLANE, [0.0, 2.0, 0.0, 2.8]).unwrap();

        assert!(compile_canonical_geometry_program(&mut config));
        assert_eq!(
            config.sdf_canonical_primitives[0].data,
            [0.0, 1.0, 0.0, 2.8]
        );
    }

    #[test]
    fn typed_soa_lowering_separates_types_and_bakes_translation() {
        let mut config = default_config();
        config.sdf_id = SDF_PROGRAM;
        append_program_op(&mut config, SDF_OP_TRANSLATE, [1.0, 2.0, 3.0, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_SPHERE, [0.75, 0.0, 0.0, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_TRANSLATE, [1.0, -1.0, 0.5, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_BOX, [0.5, 0.4, 0.3, 0.0]).unwrap();
        append_program_op(&mut config, SDF_OP_PLANE, [0.0, 2.0, 0.0, 4.0]).unwrap();

        assert!(lower_typed_soa_program(&mut config));
        assert_eq!(config.sdf_typed_soa.sphere_count, 1);
        assert_eq!(config.sdf_typed_soa.box_count, 1);
        assert_eq!(config.sdf_typed_soa.plane_count, 1);
        assert_eq!(config.sdf_typed_soa.sphere_x[0], 1.0);
        assert_eq!(config.sdf_typed_soa.sphere_y[0], 2.0);
        assert_eq!(config.sdf_typed_soa.sphere_z[0], 3.0);
        assert_eq!(config.sdf_typed_soa.sphere_radius[0], 0.75);
        assert_eq!(config.sdf_typed_soa.sphere_source[0], 1);
        assert_eq!(config.sdf_typed_soa.box_x[0], 2.0);
        assert_eq!(config.sdf_typed_soa.box_y[0], 1.0);
        assert_eq!(config.sdf_typed_soa.box_z[0], 3.5);
        assert_eq!(config.sdf_typed_soa.box_source[0], 3);
        assert_eq!(config.sdf_typed_soa.plane_y[0], 2.0);
        assert_eq!(config.sdf_typed_soa.plane_center_x[0], 2.0);
        assert_eq!(config.sdf_typed_soa.plane_center_y[0], 1.0);
        assert_eq!(config.sdf_typed_soa.plane_center_z[0], 3.5);
        assert_eq!(config.sdf_typed_soa.plane_offset[0], 4.0);
        assert_eq!(config.sdf_typed_soa.plane_source[0], 4);
    }

    #[test]
    fn typed_soa_lowering_rejects_non_translation_and_non_union_programs() {
        let mut scaled = default_config();
        scaled.sdf_id = SDF_PROGRAM;
        append_program_op(&mut scaled, SDF_OP_SCALE, [2.0, 0.0, 0.0, 0.0]).unwrap();
        append_program_op(&mut scaled, SDF_OP_SPHERE, [0.5, 0.0, 0.0, 0.0]).unwrap();
        assert!(!lower_typed_soa_program(&mut scaled));

        let mut intersected = default_config();
        intersected.sdf_id = SDF_PROGRAM;
        append_program_op(&mut intersected, SDF_OP_SPHERE, [0.8, 0.0, 0.0, 0.0]).unwrap();
        append_program_op(&mut intersected, SDF_OP_BOX, [0.5, 0.5, 0.5, 0.0]).unwrap();
        intersected.sdf_program[1].flags = 1;
        assert!(!lower_typed_soa_program(&mut intersected));
    }

    #[test]
    fn bound_grid_backend_arguments_parse() {
        let args = vec![
            "scene.json".to_owned(),
            "--renderer".to_owned(),
            "bound-grid".to_owned(),
            "--bound-grid-resolution".to_owned(),
            "64".to_owned(),
            "--bound-grid-profile".to_owned(),
            "--bound-grid-cage-bounds".to_owned(),
            "--bound-grid-directional".to_owned(),
            "--bound-grid-fp16".to_owned(),
        ];
        let parsed = parse_render_args(&args).unwrap();
        assert_eq!(parsed.renderer_backend, RendererBackend::BoundGrid);
        assert_eq!(parsed.bound_grid_resolution, Some(64));
        assert!(parsed.bound_grid_profile);
        assert!(parsed.bound_grid_cage_bounds);
        assert!(parsed.bound_grid_directional);
        assert!(parsed.bound_grid_fp16);

        for resolution in [128, 256] {
            let higher_resolution = vec![
                "scene.fract".to_owned(),
                "--bound-grid-resolution".to_owned(),
                resolution.to_string(),
            ];
            assert_eq!(
                parse_render_args(&higher_resolution)
                    .unwrap()
                    .bound_grid_resolution,
                Some(resolution)
            );
        }

        let invalid = vec![
            "scene.json".to_owned(),
            "--bound-grid-resolution".to_owned(),
            "48".to_owned(),
        ];
        assert!(parse_render_args(&invalid).is_err());
    }

    #[test]
    fn regional_backend_arguments_parse() {
        let args = vec![
            "scene.json".to_owned(),
            "--renderer".to_owned(),
            "regional".to_owned(),
            "--regional-program-resolution".to_owned(),
            "32".to_owned(),
        ];
        let parsed = parse_render_args(&args).unwrap();
        assert_eq!(parsed.renderer_backend, RendererBackend::Regional);
        assert_eq!(parsed.regional_program_resolution, 32);

        let invalid = vec![
            "scene.json".to_owned(),
            "--regional-program-resolution".to_owned(),
            "24".to_owned(),
        ];
        assert!(parse_render_args(&invalid).is_err());
    }

    #[test]
    fn diagnostic_max_distance_requires_a_positive_finite_value() {
        let args = vec![
            "scene.json".to_owned(),
            "--max-distance".to_owned(),
            "1000".to_owned(),
        ];
        assert_eq!(
            parse_render_args(&args).unwrap().diagnostic_max_distance,
            Some(1000.0)
        );
        for value in ["0", "-1", "NaN", "inf"] {
            let invalid = vec![
                "scene.json".to_owned(),
                "--max-distance".to_owned(),
                value.to_owned(),
            ];
            assert!(parse_render_args(&invalid).is_err());
        }
    }

    #[test]
    fn diffuse_normal_diagnostic_mode_parses() {
        let args = vec![
            "scene.json".to_owned(),
            "--mode".to_owned(),
            "diffuse-normal".to_owned(),
        ];
        assert_eq!(
            parse_render_args(&args).unwrap().diagnostic_mode,
            DiagnosticMode::DiffuseNormal
        );
    }

    #[test]
    fn mandelbulber_colour_diagnostic_modes_parse() {
        for (name, expected) in [
            ("mandel-color-index", DiagnosticMode::MandelColorIndex),
            (
                "mandel-palette-position",
                DiagnosticMode::MandelPalettePosition,
            ),
        ] {
            let args = vec![
                "scene.fract".to_owned(),
                "--mode".to_owned(),
                name.to_owned(),
            ];
            assert_eq!(parse_render_args(&args).unwrap().diagnostic_mode, expected);
        }
    }

    #[test]
    fn sample_count_accepts_one_and_rejects_zero() {
        let one = vec![
            "scene.json".to_owned(),
            "--samples".to_owned(),
            "1".to_owned(),
        ];
        assert_eq!(parse_render_args(&one).unwrap().samples, Some(1));

        let zero = vec![
            "scene.json".to_owned(),
            "--samples".to_owned(),
            "0".to_owned(),
        ];
        assert!(parse_render_args(&zero).is_err());
    }

    #[test]
    fn voxel_backend_arguments_parse() {
        let args = vec![
            "scene.json".to_owned(),
            "--renderer".to_owned(),
            "voxel".to_owned(),
            "--voxel-resolution".to_owned(),
            "192".to_owned(),
            "--voxel-normal".to_owned(),
            "smooth".to_owned(),
            "--voxel-storage".to_owned(),
            "dense".to_owned(),
            "--voxel-surface-band".to_owned(),
            "1.25".to_owned(),
            "--voxel-coverage".to_owned(),
            "interval".to_owned(),
            "--voxel-build".to_owned(),
            "direct".to_owned(),
            "--voxel-brick-rejection".to_owned(),
            "--voxel-leaf-refinement".to_owned(),
            "secant-bisection".to_owned(),
            "--voxel-material".to_owned(),
            "exact".to_owned(),
            "--voxel-offset".to_owned(),
            "precision".to_owned(),
        ];
        let parsed = parse_render_args(&args).unwrap();
        assert_eq!(parsed.renderer_backend, RendererBackend::Voxel);
        assert_eq!(parsed.voxel_resolution, Some(192));
        assert_eq!(parsed.voxel_normal_mode, VoxelNormalMode::Smooth);
        assert_eq!(parsed.voxel_storage_mode, Some(VoxelStorageMode::Dense));
        assert_eq!(parsed.voxel_surface_band, Some(1.25));
        assert_eq!(
            parsed.voxel_coverage_mode,
            Some(VoxelCoverageMode::Interval)
        );
        assert_eq!(parsed.voxel_build_mode, Some(VoxelBuildMode::Direct));
        assert!(parsed.voxel_brick_rejection);
        assert_eq!(
            parsed.voxel_leaf_refinement,
            Some(VoxelLeafRefinement::SecantBisection)
        );
        assert_eq!(parsed.voxel_material_mode, Some(VoxelMaterialMode::Exact));
        assert_eq!(parsed.voxel_offset_mode, Some(VoxelOffsetMode::Precision));
    }

    #[test]
    fn voxel_resolution_accepts_512_and_rejects_larger_fields() {
        let maximum = vec![
            "scene.json".to_owned(),
            "--voxel-resolution".to_owned(),
            "512".to_owned(),
        ];
        assert_eq!(
            parse_render_args(&maximum).unwrap().voxel_resolution,
            Some(512)
        );

        let too_large = vec![
            "scene.json".to_owned(),
            "--voxel-resolution".to_owned(),
            "513".to_owned(),
        ];
        assert!(parse_render_args(&too_large).is_err());
    }

    #[test]
    fn template_voxel_storage_mode_parses() {
        for (name, expected) in [("template-bricks", VoxelStorageMode::TemplateBricks)] {
            let args = vec![
                "scene.json".to_owned(),
                "--voxel-storage".to_owned(),
                name.to_owned(),
            ];
            assert_eq!(
                parse_render_args(&args).unwrap().voxel_storage_mode,
                Some(expected)
            );
        }
    }

    #[test]
    fn preview_keeps_requested_configuration() {
        let mut current = default_config();
        current.sdf_id = SDF_CAGE_FRACTAL;
        current.camera_position = [9.0, 8.0, 7.0];
        let mut duplicate = default_config();
        duplicate.sdf_id = SDF_CAGE_FRACTAL;
        let mut other = default_config();
        other.sdf_id = SDF_TOWER_FRACTAL;

        let (configs, selected) = prioritize_preview_config(&current, vec![duplicate, other]);
        assert_eq!(selected, 0);
        assert_eq!(configs.len(), 2);
        assert_eq!(configs[0].camera_position, [9.0, 8.0, 7.0]);
        assert_eq!(configs[1].sdf_id, SDF_TOWER_FRACTAL);
    }

    #[test]
    fn mandelbulber_cli_resolution_controls_dynamic_threshold() {
        let scene_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("scenes/mandelbulber/ifs-20.fract");
        let mut native_args = RenderArgs::new(&scene_path);
        native_args.width = Some(120);
        native_args.height = Some(1080);
        let native = load_mandelbulber_scene_config(&native_args).unwrap();

        let mut reduced_args = RenderArgs::new(&scene_path);
        reduced_args.width = Some(120);
        reduced_args.height = Some(68);
        let reduced = load_mandelbulber_scene_config(&reduced_args).unwrap();

        assert_eq!((reduced.config.width, reduced.config.height), (120, 68));
        let threshold_slot = crate::mandelbulber::VPARAM_THRESHOLD_SCALE;
        let ratio =
            reduced.config.vset_values[threshold_slot] / native.config.vset_values[threshold_slot];
        assert!((ratio - 1080.0 / 68.0).abs() < 1.0e-5);
    }

    #[test]
    fn fpt_vector_parses() {
        assert_eq!(
            parse_fpt_vec3(" [0.25, -1.5, 3.0] "),
            Some([0.25, -1.5, 3.0])
        );
        assert_eq!(parse_fpt_vec3("not a vector"), None);
    }
}
