use crate::ffi::*;
use anyhow::{Result, anyhow, bail, ensure};
use serde_json::Value;
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
pub enum RendererBackend {
    Sdf = RENDERER_SDF as isize,
    Voxel = RENDERER_VOXEL as isize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoxelNormalMode {
    Face = VOXEL_NORMAL_FACE as isize,
    Smooth = VOXEL_NORMAL_SMOOTH as isize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoxelStorageMode {
    Dense = VOXEL_STORAGE_DENSE as isize,
    SparseBricks = VOXEL_STORAGE_SPARSE_BRICKS as isize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticMode {
    Depth = DIAGNOSTIC_DEPTH as isize,
    Normal = DIAGNOSTIC_NORMAL as isize,
    Material = DIAGNOSTIC_MATERIAL as isize,
    PathDirect = DIAGNOSTIC_PATH_DIRECT as isize,
    PathEnvironment = DIAGNOSTIC_PATH_ENVIRONMENT as isize,
    PathThroughput = DIAGNOSTIC_PATH_THROUGHPUT as isize,
    PathFinal = DIAGNOSTIC_PATH_FINAL as isize,
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
    pub renderer_backend: RendererBackend,
    pub voxel_resolution: Option<u32>,
    pub voxel_normal_mode: VoxelNormalMode,
    pub voxel_storage_mode: Option<VoxelStorageMode>,
    pub voxel_surface_band: Option<f32>,
    pub sdf_rr_start: f32,
    pub sdf_rr_min_prob: f32,
    pub sdf_bounce_index: u32,
    pub diagnostic_mode: DiagnosticMode,
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
            renderer_backend: RendererBackend::Sdf,
            voxel_resolution: None,
            voxel_normal_mode: VoxelNormalMode::Face,
            voxel_storage_mode: None,
            voxel_surface_band: None,
            sdf_rr_start: 3.0,
            sdf_rr_min_prob: 0.2,
            sdf_bounce_index: 0,
            diagnostic_mode: DiagnosticMode::Depth,
            width: None,
            height: None,
            samples: None,
        }
    }
}

pub struct LoadedScene {
    pub config: FptRenderConfig,
    pub output_name: String,
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
                    value => bail!("invalid renderer: {value}"),
                }
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
                    value => bail!("invalid voxel normal mode: {value}"),
                }
            }
            "--voxel-storage" => {
                out.voxel_storage_mode = Some(match next_value(args, &mut i, "--voxel-storage")? {
                    "dense" => VoxelStorageMode::Dense,
                    "sparse-bricks" => VoxelStorageMode::SparseBricks,
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
            "--out" => out.out_dir = next_value(args, &mut i, "--out")?.into(),
            "--fpt-root" => out.fpt_root = next_value(args, &mut i, "--fpt-root")?.into(),
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
                    "path-direct" => DiagnosticMode::PathDirect,
                    "path-environment" => DiagnosticMode::PathEnvironment,
                    "path-throughput" => DiagnosticMode::PathThroughput,
                    "path-final" => DiagnosticMode::PathFinal,
                    "sdf-primary-steps" => DiagnosticMode::SdfPrimarySteps,
                    "sdf-shadow-steps" => DiagnosticMode::SdfShadowSteps,
                    "sdf-normal-evals" => DiagnosticMode::SdfNormalEvals,
                    "sdf-bounces" => DiagnosticMode::SdfBounces,
                    "sdf-bounce-contribution" => DiagnosticMode::SdfBounceContribution,
                    value => bail!("invalid diagnostic mode: {value}"),
                }
            }
            value => bail!("unknown argument: {value}"),
        }
        i += 1;
    }
    Ok(out)
}

pub fn apply_optimization_args(config: &mut FptRenderConfig, args: &RenderArgs) {
    config.sdf_bounce_cap = args.sdf_bounce_cap.unwrap_or(0);
    config.sdf_russian_roulette = u32::from(args.sdf_russian_roulette);
    config.sdf_normal_mode = args.sdf_normal_mode as u32;
    config.sdf_rr_start = args.sdf_rr_start;
    config.sdf_rr_min_prob = args.sdf_rr_min_prob;
    config.sdf_chunk_samples = args.sdf_chunk_samples;
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
}

pub fn default_config() -> FptRenderConfig {
    let mut cfg = FptRenderConfig {
        width: 768,
        height: 768,
        samples: 64,
        sdf_id: SDF_CORNELL_BOX,
        sdf_rr_start: 3.0,
        sdf_rr_min_prob: 0.2,
        sdf_chunk_samples: 8,
        camera_position: [0.1, 0.1, -5.0],
        camera_yaw_pitch: [0.0, 0.0],
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
                value => bail!("invalid voxel storage mode: {value}"),
            };
        }
        config.voxel_fill_interior = u32::from(
            voxel
                .get("fill_interior")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        );
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
        ];
        let parsed = parse_render_args(&args).unwrap();
        assert_eq!(parsed.renderer_backend, RendererBackend::Voxel);
        assert_eq!(parsed.voxel_resolution, Some(192));
        assert_eq!(parsed.voxel_normal_mode, VoxelNormalMode::Smooth);
        assert_eq!(parsed.voxel_storage_mode, Some(VoxelStorageMode::Dense));
        assert_eq!(parsed.voxel_surface_band, Some(1.25));
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
    fn fpt_vector_parses() {
        assert_eq!(
            parse_fpt_vec3(" [0.25, -1.5, 3.0] "),
            Some([0.25, -1.5, 3.0])
        );
        assert_eq!(parse_fpt_vec3("not a vector"), None);
    }
}
