use crate::ffi::{
    FptPrimitiveInstance, FptRenderConfig, SDF_FLAT_UNION_MAX_PRIMITIVES, SDF_MANDELBULBER,
    SDF_OP_PLANE, SDF_OP_SPHERE,
};
use anyhow::{Context, Result, anyhow, bail, ensure};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub mod catalog;
pub mod compiler;
pub mod coverage;
mod formula_optimizer;
pub mod orbit;

const DEFAULT_FOV_DEGREES: f64 = 53.13;
const DEFAULT_MAX_ITERATIONS: u32 = 250;
const DEFAULT_BAILOUT: f64 = 100.0;
const IFS_VECTOR_COUNT: usize = 9;
const DEFAULT_SURFACE_GRADIENT: &str = "0 fd6029 1000 698403 2000 fff59b 3000 f5bd22 4000 0b5e87 \
     5000 c68876 6000 a51c64 7000 3b9fee 8000 d4ffd4 9000 aba53c";

// A power-of-two similarity scale changes only the fp32 exponent, preserving
// the source-coordinate mantissa while adapting Mandelbulber distances to the
// numerical range used by Metal-FPT's marcher.
const WORLD_SCALE: f64 = 1024.0;

pub const PARAM_WORLD_SCALE: usize = 0;
pub const PARAM_MAX_ITERATIONS: usize = 1;
pub const PARAM_BAILOUT: usize = 2;
pub const PARAM_INITIAL_W: usize = 3;
pub const PARAM_ADD_C_CONSTANT: usize = 4;
pub const PARAM_JULIA_MODE: usize = 5;
pub const PARAM_JULIA_C: usize = 6;
pub const PARAM_CONSTANT_MULTIPLIER: usize = 9;
pub const PARAM_IFS_SCALE: usize = 3;
pub const PARAM_ABS_MASK: usize = 4;
pub const PARAM_ENABLED_MASK: usize = 5;
pub const PARAM_OFFSET_X: usize = 6;
pub const PARAM_OFFSET_Y: usize = 7;
pub const PARAM_OFFSET_Z: usize = 8;
pub const PARAM_ROTATION_X: usize = 9;
pub const PARAM_ROTATION_Y: usize = 10;
pub const PARAM_ROTATION_Z: usize = 11;
pub const PARAM_DIRECTION_BASE: usize = 12;
pub const PARAM_FORMULA_ID: usize = 39;
pub const VPARAM_GLOBAL_BOX_FOLD: usize = 101;
pub const VPARAM_GLOBAL_BOX_LIMIT: usize = 102;
pub const VPARAM_GLOBAL_BOX_VALUE: usize = 103;
pub const VPARAM_GLOBAL_SPHERICAL_FOLD: usize = 104;
pub const VPARAM_GLOBAL_SPHERICAL_OUTER: usize = 105;
pub const VPARAM_GLOBAL_SPHERICAL_INNER: usize = 106;
pub const VPARAM_ADVANCED_QUALITY: usize = 108;
pub const VPARAM_ABS_MIN_STEP: usize = 109;
pub const VPARAM_ABS_MAX_STEP: usize = 110;
pub const VPARAM_REL_MIN_STEP: usize = 111;
pub const VPARAM_REL_MAX_STEP: usize = 112;
pub const VPARAM_DE_FACTOR: usize = 113;
pub const VPARAM_SMOOTHNESS: usize = 114;
pub const VPARAM_DYNAMIC_THRESHOLD: usize = 115;
pub const VPARAM_THRESHOLD_SCALE: usize = 116;
pub const VPARAM_CONSTANT_THRESHOLD: usize = 117;
pub const VPARAM_MIN_THRESHOLD: usize = 118;
pub const VPARAM_MAX_THRESHOLD: usize = 119;
pub const VPARAM_FRACTAL_POSITION: usize = 120;
pub const VPARAM_FRACTAL_ROTATION: usize = 123;
pub const VPARAM_FRACTAL_REPEAT: usize = 126;
pub const VPARAM_DETAIL_LEVEL: usize = 129;
/// Optional screen-space iteration reduction, measured in formula iterations
/// removed per octave of pixel footprint above the scene's minimum detail
/// threshold. Zero preserves the exact configured iteration count.
pub const VPARAM_SCREEN_LOD_RATE: usize = 130;
/// Optional conservative world-space radius produced by the procedural-bound
/// compiler. Zero means that no formula-family proof is available.
/// Preview-only scheduler state. Negative values request a reduced orbit
/// iteration fraction; positive integer values select an exact refinement
/// tile. Offline renders leave this at zero.
pub const VPARAM_INTERACTIVE_REFINEMENT: usize = 131;
/// Relative Delta-DE probe used by offline mesh extraction when the scene does
/// not provide an advanced-quality override.
pub const MESH_DELTA_RELATIVE_DEFAULT: f32 = 0.05;
/// Preview-only spatial mode. Negative values select a replicated moving
/// preview stride; positive values encode an exact interlace stride/pass.
pub const VPARAM_INTERACTIVE_SPATIAL: usize = 132;
pub const VPARAM_CAMERA_PROJECTION: usize = 107;
/// `0`: constant threshold, `1`: distance-scaled threshold, `2`: Mandelbulber
/// iteration-threshold mode. Values above zero intentionally retain the old
/// dynamic-threshold predicate used by the Metal marcher.
pub const VPARAM_ITERATION_THRESHOLD_MODE: f32 = 2.0;
pub const FORMULA_SLOT_COUNT: usize = 9;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MandelbulberGradientStop {
    pub position: f32,
    pub color: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct MandelbulberMaterial {
    pub surface_color: [f32; 3],
    pub use_colors_from_palette: bool,
    pub surface_gradient_enabled: bool,
    pub coloring_speed: f64,
    pub palette_offset: f64,
    pub surface_gradient: Vec<MandelbulberGradientStop>,
    pub shading: f64,
    pub specular: f64,
    pub specular_width: f64,
    pub specular_plastic_enabled: bool,
    pub surface_roughness: f64,
    pub reflectance: f64,
    pub parameters: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MandelbulberFormulaSlot {
    pub formula_id: u32,
    pub enabled: bool,
    pub iterations: u32,
    pub weight: f64,
    pub start_iteration: u32,
    pub stop_iteration: u32,
    pub dont_add_c_constant: bool,
    pub add_c_constant: bool,
    pub check_for_bailout: bool,
    pub bailout: f64,
    pub parameters: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MandelbulberPrimitivePlane {
    pub position: [f64; 3],
    pub rotation: [f64; 3],
    pub material_id: u32,
    pub calculation_order: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MandelbulberPrimitiveSphere {
    pub position: [f64; 3],
    pub radius: f64,
    pub wall_thickness: f64,
    pub empty: bool,
    pub material_id: u32,
    pub calculation_order: u32,
}

impl MandelbulberFormulaSlot {
    pub fn active(&self) -> bool {
        self.enabled && self.formula_id != 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MandelbulberScene {
    pub source_version: String,
    pub formula_id: u32,
    pub formula_slots: [MandelbulberFormulaSlot; FORMULA_SLOT_COUNT],
    pub hybrid_enabled: bool,
    pub boolean_enabled: bool,
    pub boolean_operators: [u32; FORMULA_SLOT_COUNT - 1],
    pub formula_positions: [[f64; 3]; FORMULA_SLOT_COUNT],
    pub formula_rotations: [[f64; 3]; FORMULA_SLOT_COUNT],
    pub formula_repeats: [[f64; 3]; FORMULA_SLOT_COUNT],
    pub formula_scales: [f64; FORMULA_SLOT_COUNT],
    pub primitive_planes: Vec<MandelbulberPrimitivePlane>,
    pub primitive_spheres: Vec<MandelbulberPrimitiveSphere>,
    pub force_delta_de: bool,
    pub force_analytic_de: bool,
    /// Mandelbulber's `delta_DE_function`: zero selects the formula-preferred
    /// finalizer, while 1..=6 force linear, logarithmic, pseudo-Kleinian,
    /// Jos-Kleinian, custom, or max-axis distance respectively.
    pub delta_de_function: u32,
    pub repeat_from: usize,
    pub width: u32,
    pub height: u32,
    pub camera: [f64; 3],
    pub target: [f64; 3],
    pub camera_top: [f64; 3],
    pub camera_projection: u32,
    pub legacy_coordinate_system: bool,
    pub fov_degrees: f64,
    pub background_three_colors: bool,
    pub background_colors: [[f32; 3]; 3],
    pub background_brightness: f64,
    pub background_gamma: f64,
    pub image_brightness: f64,
    pub image_contrast: f64,
    pub image_gamma: f64,
    pub image_saturation: f64,
    pub main_light_enabled: bool,
    pub main_light_rotation: [f64; 3],
    pub main_light_intensity: f64,
    pub main_light_color: [f32; 3],
    pub main_light_soft_shadow_degrees: f64,
    pub main_light_cast_shadows: bool,
    pub main_light_penetrating: bool,
    pub ambient_occlusion_enabled: bool,
    pub ambient_occlusion_mode: u32,
    pub ambient_occlusion: f64,
    pub ambient_occlusion_quality: u32,
    pub ambient_occlusion_fast_tune: f64,
    pub auxiliary_light_enabled: bool,
    pub auxiliary_light_position: [f64; 3],
    pub auxiliary_light_intensity: f64,
    pub auxiliary_light_color: [f32; 3],
    pub auxiliary_light_cast_shadows: bool,
    pub auxiliary_light_penetrating: bool,
    pub max_iterations: u32,
    pub bailout: f64,
    pub use_default_bailout: bool,
    pub global_bailout: f64,
    pub linear_de_offset: f64,
    pub initial_waxis: f64,
    pub global_box_folding: bool,
    pub global_box_folding_limit: f64,
    pub global_box_folding_value: f64,
    pub global_spherical_folding: bool,
    pub global_spherical_folding_outer: f64,
    pub global_spherical_folding_inner: f64,
    pub julia_mode: bool,
    pub julia_c: [f64; 3],
    pub constant_multiplier: [f64; 3],
    pub dont_add_c_constant: bool,
    pub add_c_constant: bool,
    pub formula_parameters: BTreeMap<String, String>,
    pub bulb_power: f64,
    pub bulb_alpha_angle: f64,
    pub bulb_beta_angle: f64,
    pub bulb_gamma_angle: f64,
    pub analytic_de_scale: f64,
    pub analytic_de_offset1: f64,
    pub mandelbox_scale: f64,
    pub mandelbox_folding_limit: f64,
    pub mandelbox_folding_value: f64,
    pub mandelbox_fixed_radius: f64,
    pub mandelbox_min_radius: f64,
    pub mandelbox_offset: [f64; 3],
    pub mandelbox_color: [f64; 3],
    pub mandelbox_color_sp1: f64,
    pub mandelbox_color_sp2: f64,
    pub mandelbox_rotations_enabled: bool,
    pub mandelbox_rotations: [[[[f64; 3]; 3]; 3]; 2],
    pub mandelbox_inverse_rotations: [[[[f64; 3]; 3]; 3]; 2],
    pub mandelbox_main_rotation_enabled: bool,
    pub mandelbox_main_rotation: [f64; 3],
    pub detail_level: f64,
    pub constant_de_threshold: bool,
    pub de_threshold: f64,
    pub iteration_threshold_mode: bool,
    pub detail_size_min: f64,
    pub detail_size_max: f64,
    pub smoothness: f64,
    pub slow_shading: bool,
    pub fractal_position: [f64; 3],
    pub fractal_rotation: [f64; 3],
    pub fractal_repeat: [f64; 3],
    pub de_factor: f64,
    pub advanced_quality: bool,
    pub delta_de_relative_delta: f64,
    pub abs_min_marching_step: f64,
    pub abs_max_marching_step: f64,
    pub rel_min_marching_step: f64,
    pub rel_max_marching_step: f64,
    pub max_raymarching_steps: u32,
    pub view_distance_max: f64,
    pub ifs_scale: f64,
    pub ifs_rotation: [f64; 3],
    pub ifs_offset: [f64; 3],
    pub ifs_abs: [bool; 3],
    pub ifs_enabled: [bool; IFS_VECTOR_COUNT],
    pub ifs_directions: [[f64; 3]; IFS_VECTOR_COUNT],
    pub material: MandelbulberMaterial,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub struct DistanceSample {
    pub distance: f64,
    pub radius: f64,
    pub derivative: f64,
    pub iterations: u32,
    pub escaped: bool,
}

#[derive(Debug)]
struct FractDocument {
    version: String,
    sections: BTreeMap<String, BTreeMap<String, String>>,
}

impl FractDocument {
    fn parse(source: &str) -> Result<Self> {
        let mut version = None;
        let mut current_section = None::<String>;
        let mut sections = BTreeMap::<String, BTreeMap<String, String>>::new();

        for (line_index, raw_line) in source.lines().enumerate() {
            let line_number = line_index + 1;
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(comment) = line.strip_prefix('#') {
                if let Some(value) = comment.trim().strip_prefix("version ") {
                    version = Some(value.trim().to_owned());
                }
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                let name = line[1..line.len() - 1].trim();
                ensure!(!name.is_empty(), "empty section at line {line_number}");
                current_section = Some(name.to_owned());
                sections.entry(name.to_owned()).or_default();
                continue;
            }
            let section = current_section
                .as_ref()
                .ok_or_else(|| anyhow!("parameter outside a section at line {line_number}"))?;
            if section != "main_parameters"
                && !section
                    .strip_prefix("fractal_")
                    .is_some_and(|index| index.parse::<usize>().is_ok_and(|index| index <= 9))
            {
                continue;
            }
            let statement = line
                .strip_suffix(';')
                .ok_or_else(|| anyhow!("missing ';' at line {line_number}"))?;
            let split = statement
                .find(char::is_whitespace)
                .ok_or_else(|| anyhow!("missing parameter value at line {line_number}"))?;
            let key = statement[..split].trim();
            let value = statement[split..].trim();
            ensure!(
                !key.is_empty(),
                "empty parameter name at line {line_number}"
            );
            if value.is_empty() {
                continue;
            }
            let prior = sections
                .get_mut(section)
                .expect("current section was inserted")
                .insert(key.to_owned(), value.to_owned());
            ensure!(
                prior.is_none(),
                "duplicate parameter {section}.{key} at line {line_number}"
            );
        }

        let version = version.ok_or_else(|| anyhow!("missing Mandelbulber version comment"))?;
        Ok(Self { version, sections })
    }

    fn value(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .get(section)
            .and_then(|values| values.get(key))
            .map(String::as_str)
    }

    fn number(&self, section: &str, key: &str, fallback: f64) -> Result<f64> {
        self.value(section, key)
            .map(parse_number)
            .transpose()
            .with_context(|| format!("invalid {section}.{key}"))
            .map(|value| value.unwrap_or(fallback))
    }

    fn integer(&self, section: &str, key: &str, fallback: u32) -> Result<u32> {
        let value = self.number(section, key, fallback as f64)?;
        ensure!(
            value.is_finite() && value >= 0.0,
            "{section}.{key} must be non-negative"
        );
        ensure!(value.fract() == 0.0, "{section}.{key} must be an integer");
        ensure!(value <= u32::MAX as f64, "{section}.{key} is too large");
        Ok(value as u32)
    }

    fn boolean(&self, section: &str, key: &str, fallback: bool) -> Result<bool> {
        match self.value(section, key) {
            None => Ok(fallback),
            Some("true") => Ok(true),
            Some("false") => Ok(false),
            Some(value) => bail!("invalid {section}.{key} boolean: {value}"),
        }
    }

    fn vec3(&self, section: &str, key: &str, fallback: [f64; 3]) -> Result<[f64; 3]> {
        self.value(section, key)
            .map(parse_vec3)
            .transpose()
            .with_context(|| format!("invalid {section}.{key}"))
            .map(|value| value.unwrap_or(fallback))
    }
}

fn parse_number(value: &str) -> Result<f64> {
    value
        .replace(',', ".")
        .parse::<f64>()
        .with_context(|| format!("invalid decimal number '{value}'"))
}

fn version_is_before(version: &str, major: u32, minor: u32) -> Result<bool> {
    let mut components = version.split('.');
    let source_major = components
        .next()
        .ok_or_else(|| anyhow!("invalid Mandelbulber version '{version}'"))?
        .parse::<u32>()
        .with_context(|| format!("invalid Mandelbulber version '{version}'"))?;
    let source_minor = components
        .next()
        .ok_or_else(|| anyhow!("invalid Mandelbulber version '{version}'"))?
        .parse::<u32>()
        .with_context(|| format!("invalid Mandelbulber version '{version}'"))?;
    Ok((source_major, source_minor) < (major, minor))
}

fn migrated_fov_degrees(version: &str, projection: u32, stored_fov: f64) -> Result<f64> {
    if !version_is_before(version, 2, 21)? {
        return Ok(stored_fov);
    }

    // Mandelbulber 2.20 and earlier stored a projection-specific internal FOV
    // rather than degrees. This mirrors cSettings::Compatibility exactly.
    let legacy_fov = if stored_fov == DEFAULT_FOV_DEGREES {
        1.0
    } else {
        stored_fov
    };
    Ok(match projection {
        0 => (legacy_fov * 0.5).atan().to_degrees() * 2.0,
        1 | 3 => legacy_fov * 180.0,
        2 => legacy_fov * 360.0,
        _ => bail!("unsupported camera projection {projection}"),
    })
}

fn parse_vec3(value: &str) -> Result<[f64; 3]> {
    let values = value
        .split_whitespace()
        .map(parse_number)
        .collect::<Result<Vec<_>>>()?;
    ensure!(values.len() == 3, "expected three vector components");
    Ok([values[0], values[1], values[2]])
}

fn parse_rgb16(value: &str) -> Result<[f32; 3]> {
    let values = value.split_whitespace().collect::<Vec<_>>();
    ensure!(
        values.len() == 3,
        "expected three hexadecimal color components"
    );
    let mut color = [0.0; 3];
    for (target, source) in color.iter_mut().zip(values) {
        *target = u16::from_str_radix(source, 16)
            .with_context(|| format!("invalid 16-bit color component '{source}'"))?
            as f32
            / u16::MAX as f32;
    }
    Ok(color)
}

fn parse_rgb8(value: &str) -> Result<[f32; 3]> {
    ensure!(value.len() == 6, "expected a six-digit hexadecimal color");
    let packed = u32::from_str_radix(value, 16)
        .with_context(|| format!("invalid 8-bit RGB color '{value}'"))?;
    Ok([
        ((packed >> 16) & 0xff) as f32 / 256.0,
        ((packed >> 8) & 0xff) as f32 / 256.0,
        (packed & 0xff) as f32 / 256.0,
    ])
}

fn parse_gradient(value: &str) -> Result<Vec<MandelbulberGradientStop>> {
    let values = value.split_whitespace().collect::<Vec<_>>();
    ensure!(
        values.len() >= 2 && values.len() % 2 == 0,
        "gradient must contain position/color pairs"
    );
    ensure!(
        values.len() / 2 <= 4096,
        "gradient contains too many color stops"
    );
    let mut stops = Vec::with_capacity(values.len() / 2 + 1);
    for pair in values.chunks_exact(2) {
        let position = parse_number(pair[0])? as f32 / 10_000.0;
        ensure!(position.is_finite(), "gradient position must be finite");
        stops.push(MandelbulberGradientStop {
            position,
            color: parse_rgb8(pair[1])?,
        });
    }
    // Mandelbulber closes every imported gradient by inserting a copy of its
    // first colour at position 1.0, even when the serialized string contains
    // several stops. This is what makes palette wrapping continuous.
    stops.push(MandelbulberGradientStop {
        position: 1.0,
        color: stops[0].color,
    });
    stops.sort_by(|left, right| left.position.total_cmp(&right.position));
    Ok(stops)
}

fn parse_legacy_palette(value: &str) -> Result<Vec<MandelbulberGradientStop>> {
    let colors = value.split_whitespace().collect::<Vec<_>>();
    ensure!(
        colors.len() >= 2,
        "palette must contain at least two colors"
    );
    ensure!(colors.len() <= 4096, "palette contains too many colors");

    let count = colors.len() as f32;
    let mut stops = colors
        .into_iter()
        .enumerate()
        .map(|(index, color)| {
            // Mandelbulber's pre-2.19 settings migration truncates each
            // evenly spaced stop to four decimal digits before loading it as
            // a surface gradient.
            let position = ((index as f32 / count) * 10_000.0).trunc() / 10_000.0;
            Ok(MandelbulberGradientStop {
                position,
                color: parse_rgb8(color)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    stops.push(MandelbulberGradientStop {
        position: 1.0,
        color: stops[0].color,
    });
    Ok(stops)
}

impl MandelbulberScene {
    /// Rebase a periodic Jos-Kleinian field before converting its camera to
    /// fp32. Old example scenes can be hundreds of units from the origin while
    /// focusing only thousandths of a unit away. Their first Jos iteration
    /// wraps the point into a periodic cell, so translating the camera and
    /// target by whole cell periods is exact and avoids losing every initial
    /// march step to fp32 cancellation.
    fn periodic_camera_rebase(&self) -> Option<([f64; 3], [f64; 3])> {
        if self.boolean_enabled
            || self.fractal_position != [0.0; 3]
            || self.fractal_rotation != [0.0; 3]
            || self.fractal_repeat != [0.0; 3]
        {
            return None;
        }
        let first_slot = if self.hybrid_enabled {
            usize::from(*self.hybrid_sequence().ok()?.first()?)
        } else {
            0
        };
        let slot = self.formula_slots.get(first_slot)?;
        if slot.formula_id != 122
            || parameter_bool(
                &slot.parameters,
                "transf_sphere_inversion_enabled_false",
                false,
            )
            .ok()?
            || parameter_u32(&slot.parameters, "transf_start_iterations_C", 0).ok()? != 0
            || parameter_u32(&slot.parameters, "transf_stop_iterations_C", 250).ok()? == 0
        {
            return None;
        }
        let box_size = parameter_vec3(&slot.parameters, "transf_offset_111", [1.0; 3]).ok()?;
        let folding_value = parameter_number(&slot.parameters, "transf_folding_value", 2.0).ok()?;
        let periods = [
            2.0 * box_size[0],
            folding_value * box_size[1],
            2.0 * box_size[2],
        ];
        if periods
            .iter()
            .any(|period| !period.is_finite() || *period <= 0.0)
        {
            return None;
        }
        let shift: [f64; 3] =
            std::array::from_fn(|axis| (self.camera[axis] / periods[axis]).round() * periods[axis]);
        Some((
            std::array::from_fn(|axis| self.camera[axis] - shift[axis]),
            std::array::from_fn(|axis| self.target[axis] - shift[axis]),
        ))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let source = fs::read_to_string(path)
            .with_context(|| format!("read Mandelbulber scene {}", path.display()))?;
        Self::parse(&source)
            .with_context(|| format!("import Mandelbulber scene {}", path.display()))
    }

    pub fn parse(source: &str) -> Result<Self> {
        let document = FractDocument::parse(source)?;
        ensure!(
            document.version.starts_with("2."),
            "only Mandelbulber 2.x settings are currently supported, got {}",
            document.version
        );
        let hybrid_enabled = document.boolean("main_parameters", "hybrid_fractal_enable", false)?;
        let boolean_enabled = document.boolean("main_parameters", "boolean_operators", false)?;
        let repeat_from = document.integer("main_parameters", "repeat_from", 1)?;
        ensure!((1..=9).contains(&repeat_from), "repeat_from must be 1..9");
        let mut formula_slots = Vec::with_capacity(FORMULA_SLOT_COUNT);
        for index in 1..=FORMULA_SLOT_COUNT {
            let formula_id = document.integer(
                "main_parameters",
                &format!("formula_{index}"),
                if index == 1 { 2 } else { 0 },
            )?;
            let enabled =
                document.boolean("main_parameters", &format!("fractal_enable_{index}"), true)?;
            let iterations =
                document.integer("main_parameters", &format!("formula_iterations_{index}"), 1)?;
            ensure!(
                (1..=65_536).contains(&iterations),
                "formula_iterations_{index} must be 1..65536"
            );
            let weight =
                document.number("main_parameters", &format!("formula_weight_{index}"), 1.0)?;
            ensure!(
                (0.0..=1.0).contains(&weight),
                "formula_weight_{index} must be 0..1"
            );
            let start_iteration = document.integer(
                "main_parameters",
                &format!("formula_start_iteration_{index}"),
                0,
            )?;
            let stop_iteration = document.integer(
                "main_parameters",
                &format!("formula_stop_iteration_{index}"),
                250,
            )?;
            ensure!(
                (0..=65_536).contains(&start_iteration) && (0..=65_536).contains(&stop_iteration),
                "formula iteration ranges must be 0..65536"
            );
            formula_slots.push(MandelbulberFormulaSlot {
                formula_id: formula_id as u32,
                enabled,
                iterations: iterations as u32,
                weight,
                start_iteration: start_iteration as u32,
                stop_iteration: stop_iteration as u32,
                dont_add_c_constant: document.boolean(
                    "main_parameters",
                    &format!("dont_add_c_constant_{index}"),
                    false,
                )?,
                add_c_constant: false,
                check_for_bailout: document.boolean(
                    "main_parameters",
                    &format!("check_for_bailout_{index}"),
                    true,
                )?,
                bailout: DEFAULT_BAILOUT,
                parameters: document
                    .sections
                    .get(&format!("fractal_{index}"))
                    .cloned()
                    .unwrap_or_default(),
            });
        }
        let formula_slots: [MandelbulberFormulaSlot; FORMULA_SLOT_COUNT] = formula_slots
            .try_into()
            .expect("nine formula slots were constructed");
        let mut boolean_operators = [0; FORMULA_SLOT_COUNT - 1];
        let mut formula_positions = [[0.0; 3]; FORMULA_SLOT_COUNT];
        let mut formula_rotations = [[0.0; 3]; FORMULA_SLOT_COUNT];
        let mut formula_repeats = [[0.0; 3]; FORMULA_SLOT_COUNT];
        let mut formula_scales = [1.0; FORMULA_SLOT_COUNT];
        for index in 0..FORMULA_SLOT_COUNT {
            let scene_index = index + 1;
            formula_positions[index] = document.vec3(
                "main_parameters",
                &format!("formula_position_{scene_index}"),
                [0.0; 3],
            )?;
            formula_rotations[index] = document.vec3(
                "main_parameters",
                &format!("formula_rotation_{scene_index}"),
                [0.0; 3],
            )?;
            formula_repeats[index] = document.vec3(
                "main_parameters",
                &format!("formula_repeat_{scene_index}"),
                [0.0; 3],
            )?;
            formula_scales[index] = document.number(
                "main_parameters",
                &format!("formula_scale_{scene_index}"),
                1.0,
            )?;
            ensure!(
                formula_scales[index].is_finite() && formula_scales[index] != 0.0,
                "formula_scale_{scene_index} must be finite and non-zero"
            );
            if index > 0 {
                boolean_operators[index - 1] =
                    document.integer("main_parameters", &format!("boolean_operator_{index}"), 1)?;
            }
        }
        ensure!(
            formula_slots[0].active(),
            "formula slot 1 must select an enabled formula"
        );
        let has_secondary_formula = formula_slots[1..]
            .iter()
            .any(MandelbulberFormulaSlot::active);
        ensure!(
            hybrid_enabled || boolean_enabled || !has_secondary_formula,
            "secondary formula slots require hybrid_fractal_enable or boolean_operators"
        );
        let formula_id = formula_slots[0].formula_id;
        let mut primitive_planes = Vec::new();
        for index in 1..=SDF_FLAT_UNION_MAX_PRIMITIVES {
            let prefix = format!("primitive_plane_{index}");
            if !document.boolean("main_parameters", &format!("{prefix}_enabled"), false)? {
                continue;
            }
            ensure!(
                document.integer("main_parameters", &format!("{prefix}_boolean_operator"), 1,)?
                    == 1,
                "{prefix} currently requires Mandelbulber's OR boolean operator"
            );
            ensure!(
                !document.boolean("main_parameters", &format!("{prefix}_empty"), false)?,
                "{prefix} empty-shell mode is not yet supported"
            );
            ensure!(
                document.number("main_parameters", &format!("{prefix}_wall_thickness"), 0.0,)?
                    == 0.0,
                "{prefix} wall thickness is not yet supported"
            );
            ensure!(
                !document.boolean(
                    "main_parameters",
                    &format!("{prefix}_smooth_de_combine_enable"),
                    false,
                )?,
                "{prefix} smooth distance combination is not yet supported"
            );
            primitive_planes.push(MandelbulberPrimitivePlane {
                position: document.vec3(
                    "main_parameters",
                    &format!("{prefix}_position"),
                    [0.0; 3],
                )?,
                rotation: document.vec3(
                    "main_parameters",
                    &format!("{prefix}_rotation"),
                    [0.0; 3],
                )?,
                material_id: document.integer(
                    "main_parameters",
                    &format!("{prefix}_material_id"),
                    1,
                )?,
                calculation_order: document.integer(
                    "main_parameters",
                    &format!("{prefix}_calculation_order"),
                    1,
                )?,
            });
        }
        primitive_planes.sort_by_key(|plane| plane.calculation_order);
        let mut primitive_spheres = Vec::new();
        for index in 1..=SDF_FLAT_UNION_MAX_PRIMITIVES {
            let prefix = format!("primitive_sphere_{index}");
            if !document.boolean("main_parameters", &format!("{prefix}_enabled"), false)? {
                continue;
            }
            ensure!(
                document.integer("main_parameters", &format!("{prefix}_boolean_operator"), 1)? == 1,
                "{prefix} currently requires Mandelbulber's OR boolean operator"
            );
            ensure!(
                !document.boolean(
                    "main_parameters",
                    &format!("{prefix}_smooth_de_combine_enable"),
                    false,
                )?,
                "{prefix} smooth distance combination is not yet supported"
            );
            ensure!(
                document.vec3("main_parameters", &format!("{prefix}_repeat"), [0.0; 3],)?
                    == [0.0; 3],
                "{prefix} repetition is not yet supported"
            );
            ensure!(
                !document.boolean("main_parameters", &format!("{prefix}_limits_enable"), false,)?,
                "{prefix} limits are not yet supported"
            );
            let radius = document.number("main_parameters", &format!("{prefix}_radius"), 1.0)?;
            ensure!(
                radius.is_finite() && radius > 0.0,
                "{prefix} radius must be finite and positive"
            );
            let wall_thickness =
                document.number("main_parameters", &format!("{prefix}_wall_thickness"), 0.0)?;
            ensure!(
                wall_thickness.is_finite() && wall_thickness >= 0.0,
                "{prefix} wall thickness must be finite and non-negative"
            );
            primitive_spheres.push(MandelbulberPrimitiveSphere {
                position: document.vec3(
                    "main_parameters",
                    &format!("{prefix}_position"),
                    [0.0; 3],
                )?,
                radius,
                wall_thickness,
                empty: document.boolean("main_parameters", &format!("{prefix}_empty"), false)?,
                material_id: document.integer(
                    "main_parameters",
                    &format!("{prefix}_material_id"),
                    1,
                )?,
                calculation_order: document.integer(
                    "main_parameters",
                    &format!("{prefix}_calculation_order"),
                    1,
                )?,
            });
        }
        primitive_spheres.sort_by_key(|sphere| sphere.calculation_order);
        ensure!(
            primitive_planes.len() + primitive_spheres.len() <= SDF_FLAT_UNION_MAX_PRIMITIVES,
            "enabled Mandelbulber primitives exceed the supported limit"
        );
        let (force_delta_de, force_analytic_de) = if document
            .value("main_parameters", "delta_DE_method")
            .is_some()
        {
            let method = document.integer("main_parameters", "delta_DE_method", 0)?;
            ensure!(method <= 2, "delta_DE_method must be 0..2");
            (method == 1, method == 2)
        } else if let Some(legacy) = document.value("main_parameters", "analityc_DE_mode") {
            match legacy {
                "true" => (false, true),
                "false" => (true, false),
                value => bail!("invalid main_parameters.analityc_DE_mode boolean: {value}"),
            }
        } else {
            (false, false)
        };
        let delta_de_function = document.integer("main_parameters", "delta_DE_function", 0)?;
        ensure!(delta_de_function <= 6, "delta_DE_function must be 0..6");
        let use_default_bailout =
            document.boolean("main_parameters", "use_default_bailout", true)?;
        let global_bailout = document.number("main_parameters", "bailout", DEFAULT_BAILOUT)?;
        ensure!(
            global_bailout.is_finite() && global_bailout >= 1.0,
            "global bailout must be finite and at least one"
        );

        let max_iterations = document.integer("main_parameters", "N", DEFAULT_MAX_ITERATIONS)?;
        ensure!((1..=4096).contains(&max_iterations), "N must be 1..4096");
        let width = document.integer("main_parameters", "image_width", 1920)?;
        let height = document.integer("main_parameters", "image_height", 1080)?;
        ensure!(
            (1..=16384).contains(&width) && (1..=16384).contains(&height),
            "image dimensions must be 1..16384"
        );
        let camera = document.vec3("main_parameters", "camera", [0.0, 0.0, -5.0])?;
        let target = document.vec3("main_parameters", "target", [0.0, 0.0, 0.0])?;
        ensure!(camera != target, "camera and target must differ");
        let camera_top = document.vec3("main_parameters", "camera_top", [0.0, 0.0, 1.0])?;
        let camera_projection = match document
            .value("main_parameters", "perspective_type")
            .unwrap_or("three_point")
        {
            "three_point" | "0" => 0,
            "fish_eye" | "1" => 1,
            "equirectangular" | "2" => 2,
            "fish_eye_cut" | "3" => 3,
            value => bail!("unsupported perspective_type {value}"),
        };
        let legacy_coordinate_system =
            document.boolean("main_parameters", "legacy_coordinate_system", false)?;
        let stored_fov = document.number("main_parameters", "fov", DEFAULT_FOV_DEGREES)?;
        let fov_degrees = migrated_fov_degrees(&document.version, camera_projection, stored_fov)?;
        let maximum_fov = if camera_projection == 0 { 180.0 } else { 720.0 };
        ensure!(
            fov_degrees > 0.0 && fov_degrees <= maximum_fov,
            "field of view must be positive and at most {maximum_fov} degrees for this projection"
        );
        let detail_level = document.number("main_parameters", "detail_level", 1.0)?;
        ensure!(detail_level > 0.0, "detail_level must be positive");
        let constant_de_threshold =
            document.boolean("main_parameters", "constant_DE_threshold", false)?;
        let de_threshold = document.number("main_parameters", "DE_thresh", 0.01)?;
        let iteration_threshold_mode =
            document.boolean("main_parameters", "iteration_threshold_mode", false)?;
        let detail_size_min = document.number("main_parameters", "detail_size_min", 1.0e-12)?;
        let detail_size_max = document.number("main_parameters", "detail_size_max", 1.0)?;
        let smoothness = document.number("main_parameters", "smoothness", 1.0)?;
        let slow_shading = document.boolean("main_parameters", "slow_shading", false)?;
        let fractal_position = document.vec3("main_parameters", "fractal_position", [0.0; 3])?;
        let fractal_rotation = document.vec3("main_parameters", "fractal_rotation", [0.0; 3])?;
        let fractal_repeat = document.vec3("main_parameters", "repeat", [0.0; 3])?;
        let de_factor = document.number("main_parameters", "DE_factor", 1.0)?;
        let advanced_quality = document.boolean("main_parameters", "advanced_quality", false)?;
        let delta_de_relative_delta =
            document.number("main_parameters", "deltade_relative_delta", 0.01)?;
        let abs_min_marching_step =
            document.number("main_parameters", "abs_min_marching_step", 1.0e-15)?;
        let abs_max_marching_step =
            document.number("main_parameters", "abs_max_marching_step", 3.0)?;
        let rel_min_marching_step =
            document.number("main_parameters", "rel_min_marching_step", 1.0e-2)?;
        let rel_max_marching_step =
            document.number("main_parameters", "rel_max_marching_step", 1.0e4)?;
        let max_raymarching_steps =
            document.integer("main_parameters", "max_raymarching_steps", 10_000)?;
        ensure!(
            de_threshold > 0.0
                && detail_size_min > 0.0
                && detail_size_max >= detail_size_min
                && smoothness > 0.0
                && de_factor > 0.0
                && delta_de_relative_delta > 0.0
                && abs_min_marching_step > 0.0
                && abs_max_marching_step >= abs_min_marching_step
                && rel_min_marching_step > 0.0
                && rel_max_marching_step >= rel_min_marching_step,
            "distance-threshold parameters must be positive and ordered"
        );
        ensure!(
            (1..=100_000_000).contains(&max_raymarching_steps),
            "max_raymarching_steps must be 1..100000000"
        );
        let view_distance_max = document.number("main_parameters", "view_distance_max", 50.0)?;
        ensure!(
            view_distance_max > 0.0,
            "view_distance_max must be positive"
        );

        let ifs_scale = document.number("fractal_1", "IFS_scale", 2.0)?;
        let ifs_rotation = document.vec3("fractal_1", "IFS_rotation", [0.0; 3])?;
        let _rotation_enabled = document.boolean("fractal_1", "IFS_rotation_enabled", false)?;
        let ifs_offset = document.vec3("fractal_1", "IFS_offset", [1.0, 0.0, 0.0])?;
        let _ifs_edge_enabled = document.boolean("fractal_1", "IFS_edge_enabled", false)?;
        let _ifs_menger_sponge_mode =
            document.boolean("fractal_1", "IFS_menger_sponge_mode", false)?;
        let ifs_abs = [
            document.boolean("fractal_1", "IFS_abs_x", false)?,
            document.boolean("fractal_1", "IFS_abs_y", false)?,
            document.boolean("fractal_1", "IFS_abs_z", false)?,
        ];
        let mut ifs_enabled = [false; IFS_VECTOR_COUNT];
        let mut ifs_directions = [[1.0, 0.0, 0.0]; IFS_VECTOR_COUNT];
        for index in 0..IFS_VECTOR_COUNT {
            let enabled_key = format!("IFS_enabled_{index}");
            let direction_key = format!("IFS_direction_{index}");
            let rotations_key = format!("IFS_rotations_{index}");
            let distance_key = format!("IFS_distance_{index}");
            let intensity_key = format!("IFS_intensity_{index}");
            ifs_enabled[index] = document.boolean("fractal_1", &enabled_key, false)?;
            ifs_directions[index] =
                normalize(document.vec3("fractal_1", &direction_key, [1.0, 0.0, 0.0])?)?;
            let _rotation = document.vec3("fractal_1", &rotations_key, [0.0; 3])?;
            let _distance = document.number("fractal_1", &distance_key, 0.0)?;
            let _intensity = document.number("fractal_1", &intensity_key, 1.0)?;
        }

        for slot in formula_slots
            .iter()
            .filter(|slot| slot.active() && slot.formula_id == 10)
        {
            kaleidoscopic_ifs_from_parameters(&slot.parameters)?;
        }
        for slot in formula_slots
            .iter()
            .filter(|slot| slot.active() && matches!(slot.formula_id, 8 | 9))
        {
            mandelbox_kernel_from_parameters(slot.formula_id, &slot.parameters)?;
        }
        for slot in formula_slots
            .iter()
            .filter(|slot| slot.active() && slot.formula_id == 122)
        {
            jos_kleinian_from_parameters(&slot.parameters)?;
        }
        for slot in formula_slots
            .iter()
            .filter(|slot| slot.active() && slot.formula_id == 103)
        {
            pseudo_kleinian_from_parameters(&slot.parameters)?;
        }
        for slot in formula_slots
            .iter()
            .filter(|slot| slot.active() && slot.formula_id == 610)
        {
            difs_msltoe_donut_from_parameters(&slot.parameters)?;
        }
        for slot in formula_slots
            .iter()
            .filter(|slot| slot.active() && slot.formula_id == 607)
        {
            difs_menger_from_parameters(&slot.parameters)?;
        }
        for slot in formula_slots
            .iter()
            .filter(|slot| slot.active() && slot.formula_id == 1113)
        {
            transf_de_linear_cube_from_parameters(&slot.parameters)?;
        }
        for slot in formula_slots
            .iter()
            .filter(|slot| slot.active() && slot.formula_id == 1613)
        {
            transf_difs_grid_from_parameters(&slot.parameters)?;
        }
        for slot in formula_slots
            .iter()
            .filter(|slot| slot.active() && slot.formula_id == 1600)
        {
            transf_difs_box_from_parameters(&slot.parameters)?;
        }
        for slot in formula_slots
            .iter()
            .filter(|slot| slot.active() && slot.formula_id == 1602)
        {
            transf_difs_ellipsoid_from_parameters(&slot.parameters)?;
        }
        for slot in formula_slots
            .iter()
            .filter(|slot| slot.active() && slot.formula_id == 1604)
        {
            transf_difs_sphere_from_parameters(&slot.parameters)?;
        }
        for slot in formula_slots
            .iter()
            .filter(|slot| slot.active() && slot.formula_id == 1639)
        {
            transf_difs_chessboard_from_parameters(&slot.parameters)?;
        }
        for slot in formula_slots
            .iter()
            .filter(|slot| slot.active() && slot.formula_id == 1603)
        {
            transf_difs_hextgrid2_from_parameters(&slot.parameters)?;
        }
        for slot in formula_slots
            .iter()
            .filter(|slot| slot.active() && slot.formula_id == 1646)
        {
            transf_difs_torus_v4_from_parameters(&slot.parameters)?;
        }

        let mandelbox_min_radius =
            document.number("fractal_1", "mandelbox_folding_min_radius", 0.5)?;
        let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let mut mandelbox_rotations = [[identity; 3]; 2];
        let mut mandelbox_inverse_rotations = [[identity; 3]; 2];
        for (fold, name) in ["neg", "pos"].into_iter().enumerate() {
            for axis in 0..3 {
                let key = format!("mandelbox_rotation_{name}_{}", axis + 1);
                let angles = document
                    .vec3("fractal_1", &key, [0.0; 3])?
                    .map(f64::to_radians);
                mandelbox_rotations[fold][axis] = rotation2_matrix(angles);
                mandelbox_inverse_rotations[fold][axis] =
                    transpose_matrix(mandelbox_rotations[fold][axis]);
            }
        }

        let material_parameters = document
            .sections
            .get("main_parameters")
            .into_iter()
            .flat_map(|values| values.iter())
            .filter(|(key, _)| key.starts_with("mat1_"))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<BTreeMap<_, _>>();
        let surface_color = document
            .value("main_parameters", "mat1_surface_color")
            .map(parse_rgb16)
            .transpose()?
            .unwrap_or([50000.0 / 65535.0; 3]);
        let surface_gradient = if let Some(value) =
            document.value("main_parameters", "mat1_surface_color_gradient")
        {
            parse_gradient(value).context("invalid main_parameters.mat1_surface_color_gradient")?
        } else if version_is_before(&document.version, 2, 19)?
            && let Some(value) = document.value("main_parameters", "mat1_surface_color_palette")
        {
            parse_legacy_palette(value)
                .context("invalid main_parameters.mat1_surface_color_palette")?
        } else {
            parse_gradient(DEFAULT_SURFACE_GRADIENT).expect("valid built-in surface gradient")
        };
        let legacy_gradient_size = (surface_gradient.len() - 1) as f64;
        let mut coloring_speed = document.number("main_parameters", "mat1_coloring_speed", 1.0)?;
        let mut palette_offset =
            document.number("main_parameters", "mat1_coloring_palette_offset", 0.0)?;
        if version_is_before(&document.version, 2, 19)? {
            // settings.cpp migrates pre-2.19 palette coordinates after the
            // legacy palette has been converted to a positioned gradient.
            palette_offset /= legacy_gradient_size;
            coloring_speed *= 10.0 / legacy_gradient_size;
        }
        let legacy_material_defaults = version_is_before(&document.version, 2, 14)?;
        let material = MandelbulberMaterial {
            surface_color,
            use_colors_from_palette: document.boolean(
                "main_parameters",
                "mat1_use_colors_from_palette",
                true,
            )?,
            surface_gradient_enabled: document.boolean(
                "main_parameters",
                "mat1_surface_gradient_enable",
                true,
            )?,
            coloring_speed,
            palette_offset,
            surface_gradient,
            shading: document.number("main_parameters", "mat1_shading", 1.0)?,
            specular: document.number(
                "main_parameters",
                "mat1_specular",
                if legacy_material_defaults { 1.0 } else { 5.0 },
            )?,
            specular_width: document.number("main_parameters", "mat1_specular_width", 0.05)?,
            specular_plastic_enabled: document.boolean(
                "main_parameters",
                "mat1_specular_plastic_enable",
                true,
            )?,
            surface_roughness: document.number(
                "main_parameters",
                "mat1_surface_roughness",
                0.01,
            )?,
            reflectance: document.number("main_parameters", "mat1_reflectance", 0.0)?,
            parameters: material_parameters,
        };
        let background_colors = [
            document
                .value("main_parameters", "background_color_1")
                .map(parse_rgb16)
                .transpose()?
                .unwrap_or([0.0, 38_306.0 / 65_535.0, 1.0]),
            document
                .value("main_parameters", "background_color_2")
                .map(parse_rgb16)
                .transpose()?
                .unwrap_or([1.0; 3]),
            document
                .value("main_parameters", "background_color_3")
                .map(parse_rgb16)
                .transpose()?
                .unwrap_or([0.0, 10_000.0 / 65_535.0, 500.0 / 65_535.0]),
        ];
        let modern_lights = !version_is_before(&document.version, 2, 25)?;
        let main_light_rotation = if modern_lights {
            document.vec3("main_parameters", "light1_rotation", [-45.0, 45.0, 0.0])?
        } else {
            [
                document.number("main_parameters", "main_light_alpha", -45.0)?,
                document.number("main_parameters", "main_light_beta", 45.0)?,
                0.0,
            ]
        };
        let main_light_color = document
            .value(
                "main_parameters",
                if modern_lights {
                    "light1_color"
                } else {
                    "main_light_colour"
                },
            )
            .map(parse_rgb16)
            .transpose()?
            .unwrap_or([1.0; 3]);
        let auxiliary_light_position = if modern_lights {
            document.vec3("main_parameters", "light2_position", [3.0, -3.0, 3.0])?
        } else {
            document.vec3("main_parameters", "aux_light_position_1", [3.0, -3.0, 3.0])?
        };
        let auxiliary_light_color = document
            .value(
                "main_parameters",
                if modern_lights {
                    "light2_color"
                } else {
                    "aux_light_colour_1"
                },
            )
            .map(parse_rgb16)
            .transpose()?
            .unwrap_or([
                45_761.0 / 65_535.0,
                53_633.0 / 65_535.0,
                59_498.0 / 65_535.0,
            ]);
        let legacy_auxiliary_defined = !modern_lights
            && [
                "aux_light_position_1",
                "aux_light_intensity_1",
                "aux_light_colour_1",
                "aux_light_enabled_1",
            ]
            .iter()
            .any(|key| document.value("main_parameters", key).is_some());

        Ok(Self {
            source_version: document.version.clone(),
            formula_id,
            formula_slots: formula_slots.clone(),
            hybrid_enabled,
            boolean_enabled,
            boolean_operators,
            formula_positions,
            formula_rotations,
            formula_repeats,
            formula_scales,
            primitive_planes,
            primitive_spheres,
            force_delta_de,
            force_analytic_de,
            delta_de_function,
            repeat_from: repeat_from as usize,
            width,
            height,
            camera,
            target,
            camera_top,
            camera_projection,
            legacy_coordinate_system,
            fov_degrees,
            background_three_colors: document.boolean(
                "main_parameters",
                "background_3_colors_enable",
                true,
            )?,
            background_colors,
            background_brightness: document.number(
                "main_parameters",
                "background_brightness",
                1.0,
            )?,
            background_gamma: document.number("main_parameters", "background_gamma", 1.0)?,
            image_brightness: document.number("main_parameters", "brightness", 1.0)?,
            image_contrast: document.number("main_parameters", "contrast", 1.0)?,
            image_gamma: document.number("main_parameters", "gamma", 1.0)?,
            image_saturation: document.number("main_parameters", "saturation", 1.0)?,
            main_light_enabled: document.boolean(
                "main_parameters",
                if modern_lights {
                    "light1_enabled"
                } else {
                    "main_light_enable"
                },
                true,
            )?,
            main_light_rotation,
            main_light_intensity: document.number(
                "main_parameters",
                if modern_lights {
                    "light1_intensity"
                } else {
                    "main_light_intensity"
                },
                1.0,
            )?,
            main_light_color,
            main_light_soft_shadow_degrees: document.number(
                "main_parameters",
                if modern_lights {
                    "light1_soft_shadow_cone"
                } else {
                    "shadows_cone_angle"
                },
                1.0,
            )?,
            main_light_cast_shadows: document.boolean(
                "main_parameters",
                if modern_lights {
                    "light1_cast_shadows"
                } else {
                    "shadows_enabled"
                },
                true,
            )?,
            main_light_penetrating: document.boolean(
                "main_parameters",
                if modern_lights {
                    "light1_penetrating"
                } else {
                    "penetrating_lights"
                },
                true,
            )?,
            ambient_occlusion_enabled: document.boolean(
                "main_parameters",
                "ambient_occlusion_enabled",
                false,
            )?,
            ambient_occlusion_mode: document.integer(
                "main_parameters",
                "ambient_occlusion_mode",
                2,
            )?,
            ambient_occlusion: document.number("main_parameters", "ambient_occlusion", 1.0)?,
            ambient_occlusion_quality: document.integer(
                "main_parameters",
                "ambient_occlusion_quality",
                4,
            )?,
            ambient_occlusion_fast_tune: document.number(
                "main_parameters",
                "ambient_occlusion_fast_tune",
                1.0,
            )?,
            auxiliary_light_enabled: if modern_lights {
                document.boolean("main_parameters", "light2_enabled", false)?
            } else if legacy_auxiliary_defined {
                document.boolean("main_parameters", "aux_light_enabled_1", true)?
            } else {
                false
            },
            auxiliary_light_position,
            // Mandelbulber's pre-2.25 migration divides legacy auxiliary
            // light intensities by four before constructing light #2.
            auxiliary_light_intensity: document.number(
                "main_parameters",
                if modern_lights {
                    "light2_intensity"
                } else {
                    "aux_light_intensity_1"
                },
                if modern_lights { 0.325 } else { 1.3 },
            )? / if modern_lights { 1.0 } else { 4.0 },
            auxiliary_light_color,
            auxiliary_light_cast_shadows: document.boolean(
                "main_parameters",
                "light2_cast_shadows",
                true,
            )?,
            auxiliary_light_penetrating: document.boolean(
                "main_parameters",
                "light2_penetrating",
                true,
            )?,
            max_iterations,
            bailout: if formula_id == 3 {
                10.0
            } else {
                DEFAULT_BAILOUT
            },
            use_default_bailout,
            global_bailout,
            linear_de_offset: document.number("main_parameters", "linear_DE_offset", 0.0)?,
            initial_waxis: document.number("main_parameters", "initial_waxis", 0.0)?,
            global_box_folding: document.boolean("main_parameters", "box_folding", false)?,
            global_box_folding_limit: document.number(
                "main_parameters",
                "box_folding_limit",
                1.0,
            )?,
            global_box_folding_value: document.number(
                "main_parameters",
                "box_folding_value",
                2.0,
            )?,
            global_spherical_folding: document.boolean(
                "main_parameters",
                "spherical_folding",
                false,
            )?,
            global_spherical_folding_outer: document.number(
                "main_parameters",
                "spherical_folding_outer",
                1.0,
            )?,
            global_spherical_folding_inner: document.number(
                "main_parameters",
                "spherical_folding_inner",
                0.5,
            )?,
            julia_mode: document.boolean("main_parameters", "julia_mode", false)?,
            julia_c: document.vec3("main_parameters", "julia_c", [0.0; 3])?,
            constant_multiplier: document.vec3(
                "main_parameters",
                "fractal_constant_factor",
                [1.0; 3],
            )?,
            dont_add_c_constant: formula_slots[0].dont_add_c_constant,
            add_c_constant: formula_id != 10,
            formula_parameters: formula_slots[0].parameters.clone(),
            bulb_power: document.number("fractal_1", "power", 9.0)?,
            bulb_alpha_angle: document
                .number("fractal_1", "alpha_angle_offset", 0.0)?
                .to_radians(),
            bulb_beta_angle: document
                .number("fractal_1", "beta_angle_offset", 0.0)?
                .to_radians(),
            bulb_gamma_angle: document
                .number("fractal_1", "gamma_angle_offset", 0.0)?
                .to_radians(),
            analytic_de_scale: document.number("fractal_1", "analyticDE_scale_1", 1.0)?,
            analytic_de_offset1: document.number("fractal_1", "analyticDE_offset_1", 1.0)?,
            mandelbox_scale: document.number("fractal_1", "mandelbox_scale", 2.0)?,
            mandelbox_folding_limit: document.number(
                "fractal_1",
                "mandelbox_folding_limit",
                1.0,
            )?,
            mandelbox_folding_value: document.number(
                "fractal_1",
                "mandelbox_folding_value",
                2.0,
            )?,
            mandelbox_fixed_radius: document.number(
                "fractal_1",
                "mandelbox_folding_fixed_radius",
                1.0,
            )?,
            mandelbox_min_radius,
            mandelbox_offset: document.vec3("fractal_1", "mandelbox_offset", [0.0; 3])?,
            mandelbox_color: document.vec3("fractal_1", "mandelbox_color", [0.03, 0.05, 0.07])?,
            mandelbox_color_sp1: document.number("fractal_1", "mandelbox_color_Sp1", 0.2)?,
            mandelbox_color_sp2: document.number("fractal_1", "mandelbox_color_Sp2", 0.2)?,
            mandelbox_rotations_enabled: document.boolean(
                "fractal_1",
                "mandelbox_rotations_enabled",
                false,
            )?,
            mandelbox_rotations,
            mandelbox_inverse_rotations,
            mandelbox_main_rotation_enabled: document.boolean(
                "fractal_1",
                "mandelbox_main_rotation_enabled",
                false,
            )?,
            mandelbox_main_rotation: document
                .vec3("fractal_1", "mandelbox_rotation_main", [0.0; 3])?
                .map(f64::to_radians),
            detail_level,
            constant_de_threshold,
            de_threshold,
            iteration_threshold_mode,
            detail_size_min,
            detail_size_max,
            smoothness,
            slow_shading,
            fractal_position,
            fractal_rotation,
            fractal_repeat,
            de_factor,
            advanced_quality,
            delta_de_relative_delta,
            abs_min_marching_step,
            abs_max_marching_step,
            rel_min_marching_step,
            rel_max_marching_step,
            max_raymarching_steps,
            view_distance_max,
            ifs_scale,
            ifs_rotation,
            ifs_offset,
            ifs_abs,
            ifs_enabled,
            ifs_directions,
            material,
        })
    }

    pub fn configure_formula_slots(
        &mut self,
        sources: &[(usize, &catalog::ResolvedFormulaSource)],
    ) -> Result<()> {
        let mut configured = [false; FORMULA_SLOT_COUNT];
        for &(index, source) in sources {
            ensure!(
                index < FORMULA_SLOT_COUNT,
                "formula slot index must be 0..8"
            );
            ensure!(
                self.formula_slots[index].active(),
                "cannot configure inactive formula slot {}",
                index + 1
            );
            Self::configure_slot(
                &mut self.formula_slots[index],
                source,
                self.use_default_bailout,
                self.global_bailout,
            )?;
            configured[index] = true;
        }
        for (index, slot) in self.formula_slots.iter().enumerate() {
            ensure!(
                !slot.active() || configured[index],
                "formula slot {} was not resolved",
                index + 1
            );
        }
        self.bailout = if self.hybrid_enabled && self.use_default_bailout {
            self.formula_slots
                .iter()
                .filter(|slot| slot.active())
                .map(|slot| slot.bailout)
                .fold(0.0, f64::max)
        } else {
            self.formula_slots[0].bailout
        };
        if self.hybrid_enabled {
            for slot in &mut self.formula_slots {
                if slot.active() {
                    slot.bailout = self.bailout;
                }
            }
        }
        self.add_c_constant = self.formula_slots[0].add_c_constant;
        Ok(())
    }

    fn configure_slot(
        slot: &mut MandelbulberFormulaSlot,
        source: &catalog::ResolvedFormulaSource,
        use_default_bailout: bool,
        global_bailout: f64,
    ) -> Result<()> {
        ensure!(
            slot.formula_id == source.id as u32,
            "scene formula ID {} does not match generated formula {} ({})",
            slot.formula_id,
            source.symbol,
            source.id
        );
        slot.bailout = if use_default_bailout {
            source.default_bailout
        } else {
            global_bailout
        };
        slot.add_c_constant = match source.pixel_addition.as_str() {
            "cpixelAlreadyHas" => false,
            "cpixelEnabledByDefault" => !slot.dont_add_c_constant,
            "cpixelDisabledByDefault" => slot.dont_add_c_constant,
            other => bail!("unsupported Mandelbulber C-addition mode {other}"),
        };
        Ok(())
    }

    pub fn hybrid_sequence(&self) -> Result<Vec<u8>> {
        self.hybrid_sequence_with_limit(self.max_iterations)
    }

    pub fn hybrid_coloring_sequence(&self) -> Result<Vec<u8>> {
        self.hybrid_sequence_with_limit(self.max_iterations)
    }

    fn hybrid_sequence_with_limit(&self, iteration_limit: u32) -> Result<Vec<u8>> {
        if !self.hybrid_enabled {
            return Ok(vec![0; iteration_limit as usize]);
        }
        let mut sequence = Vec::with_capacity(iteration_limit as usize);
        let mut formula_index = 0usize;
        let mut counter = 0u32;
        for iteration in 0..iteration_limit {
            counter += 1;
            let mut searches = 0;
            while searches < FORMULA_SLOT_COUNT {
                let slot = &self.formula_slots[formula_index];
                if slot.active()
                    && iteration >= slot.start_iteration
                    && iteration <= slot.stop_iteration
                {
                    break;
                }
                formula_index += 1;
                if formula_index >= FORMULA_SLOT_COUNT {
                    formula_index = self.repeat_from - 1;
                }
                searches += 1;
            }
            let slot = &self.formula_slots[formula_index];
            if !slot.active() || iteration < slot.start_iteration || iteration > slot.stop_iteration
            {
                if sequence.is_empty()
                    && let Some(first_active) = self
                        .formula_slots
                        .iter()
                        .position(MandelbulberFormulaSlot::active)
                {
                    sequence.push(first_active as u8);
                }
                break;
            }
            sequence.push(formula_index as u8);
            if counter >= slot.iterations {
                counter = 0;
                formula_index += 1;
                if formula_index >= FORMULA_SLOT_COUNT {
                    formula_index = self.repeat_from - 1;
                }
            }
        }
        Ok(sequence)
    }

    pub fn mesh_delta_relative_delta(&self) -> f32 {
        if self.advanced_quality {
            self.delta_de_relative_delta as f32
        } else {
            MESH_DELTA_RELATIVE_DEFAULT
        }
    }

    pub fn apply_to_config(&self, config: &mut FptRenderConfig) {
        config.sdf_id = SDF_MANDELBULBER;
        config.width = self.width;
        config.height = self.height;

        config.sdf_flat_union_count =
            (self.primitive_planes.len() + self.primitive_spheres.len()) as u32;
        config
            .sdf_flat_union_instances
            .fill(FptPrimitiveInstance::default());
        for (target, plane) in config
            .sdf_flat_union_instances
            .iter_mut()
            .zip(&self.primitive_planes)
        {
            let rotation = rotation2_matrix(plane.rotation.map(f64::to_radians));
            let normal = map_mandel_vector(rotation[2]);
            let position = map_mandel_point(plane.position).map(|value| value * WORLD_SCALE);
            *target = FptPrimitiveInstance {
                transform: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
                data: [
                    normal[0] as f32,
                    normal[1] as f32,
                    normal[2] as f32,
                    -dot(normal, position) as f32,
                ],
                opcode: SDF_OP_PLANE,
                distance_scale: 1.0,
                source_instruction: plane.material_id,
                _pad0: 0,
            };
        }
        for (target, sphere) in config
            .sdf_flat_union_instances
            .iter_mut()
            .skip(self.primitive_planes.len())
            .zip(&self.primitive_spheres)
        {
            let position = map_mandel_point(sphere.position).map(|value| value * WORLD_SCALE);
            *target = FptPrimitiveInstance {
                transform: [
                    1.0,
                    0.0,
                    0.0,
                    -position[0] as f32,
                    0.0,
                    1.0,
                    0.0,
                    -position[1] as f32,
                    0.0,
                    0.0,
                    1.0,
                    -position[2] as f32,
                ],
                data: [
                    (sphere.radius * WORLD_SCALE) as f32,
                    (sphere.wall_thickness * WORLD_SCALE) as f32,
                    0.0,
                    0.0,
                ],
                opcode: SDF_OP_SPHERE,
                distance_scale: 1.0,
                source_instruction: sphere.material_id,
                _pad0: u32::from(sphere.empty),
            };
        }

        let (source_camera, source_target) = self
            .periodic_camera_rebase()
            .unwrap_or((self.camera, self.target));
        let camera = map_mandel_point(source_camera).map(|value| value * WORLD_SCALE);
        let target = map_mandel_point(source_target).map(|value| value * WORLD_SCALE);
        let direction = subtract(target, camera);
        config.camera_position = camera.map(|value| value as f32);
        let horizontal = direction[0].hypot(direction[2]);
        config.camera_yaw_pitch = [
            direction[0].atan2(direction[2]) as f32,
            direction[1].atan2(horizontal) as f32,
        ];
        config.camera_roll = camera_roll(
            direction,
            map_mandel_vector(self.camera_top),
            config.camera_yaw_pitch,
        ) as f32;
        config.camera_image_y_sign = if self.legacy_coordinate_system {
            -1.0
        } else {
            1.0
        };
        // Mandelbulber spans [-0.5, 0.5] on the image plane and multiplies by
        // 2*tan(fov/2). Metal-FPT uses the same span with 1/tan(fov/2) as its
        // focal length, so its configured angle must absorb that factor of 2.
        config.camera_fov = if self.camera_projection == 0 {
            (2.0 * (2.0 * (self.fov_degrees.to_radians() * 0.5).tan()).atan()).to_degrees() as f32
        } else {
            self.fov_degrees as f32
        };
        config.camera_dof = 0.0;
        config.focus_distance = (length(direction) * WORLD_SCALE) as f32;

        let internal_fov = if self.camera_projection == 0 {
            2.0 * (self.fov_degrees.to_radians() * 0.5).tan()
        } else if self.camera_projection == 2 {
            self.fov_degrees.to_radians() * 0.5
        } else {
            self.fov_degrees.to_radians()
        };
        let threshold_scale = internal_fov
            / f64::from(self.height)
            / if self.iteration_threshold_mode {
                1.0
            } else {
                self.detail_level
            };
        let distance_threshold = if self.constant_de_threshold && !self.iteration_threshold_mode {
            self.de_threshold * WORLD_SCALE
        } else {
            (length(direction) * threshold_scale).clamp(
                self.detail_size_min * WORLD_SCALE,
                self.detail_size_max * WORLD_SCALE,
            )
        };
        config.render[1] = self.max_raymarching_steps as f32;
        config.render[2] = distance_threshold as f32;
        config.render[3] = distance_threshold as f32;
        config.render[4] = (self.view_distance_max * WORLD_SCALE) as f32;
        config.render[5] = 0.0;
        config.render[7] = if self.slow_shading {
            (internal_fov / f64::from(self.height)) as f32
        } else {
            0.0
        };
        config.world = [3.0, 1.0, 125.0, 30.0, 0.45, 1.0, 1.0];
        config.world_one_color = self.background_colors[1];
        config.background_gradient[..3].copy_from_slice(&self.background_colors[0]);
        config.background_gradient[3..].copy_from_slice(&self.background_colors[2]);
        // Background modes 2 and 3 reproduce Mandelbulber's three-color and
        // one-color backgrounds respectively. The existing FPT gradient mode
        // remains mode 1 for ordinary JSON scenes.
        config.world[1] = self.background_brightness as f32;
        config.world[5] = self.background_gamma as f32;
        config.world[6] = if self.background_three_colors {
            2.0
        } else {
            3.0
        };
        let light_direction = self.main_light_direction();
        let light_horizontal = light_direction[0].hypot(light_direction[2]);
        config.sun = [
            u32::from(self.main_light_enabled) as f32,
            light_direction[0].atan2(light_direction[2]).to_degrees() as f32,
            light_direction[1].atan2(light_horizontal).to_degrees() as f32,
            self.main_light_intensity as f32,
            self.main_light_soft_shadow_degrees.to_radians() as f32,
        ];
        config.sun_color = self.main_light_color;
        // Negative tone-map values select Mandelbulber's image-adjustment
        // order in the shader. Its default gamma is identity rather than the
        // sRGB transfer used by native FPT scenes.
        config.post = [
            -(self.image_gamma.max(1.0e-6) as f32),
            self.image_brightness as f32,
            0.0,
            self.image_saturation as f32,
            self.image_contrast as f32,
            0.0,
            0.0,
        ];

        config.set_values[PARAM_WORLD_SCALE] = WORLD_SCALE as f32;
        config.set_values[PARAM_MAX_ITERATIONS] = self.max_iterations as f32;
        config.set_values[PARAM_BAILOUT] = self.bailout as f32;
        config.vset_values[VPARAM_GLOBAL_BOX_FOLD] = u32::from(self.global_box_folding) as f32;
        config.vset_values[VPARAM_GLOBAL_BOX_LIMIT] = self.global_box_folding_limit as f32;
        config.vset_values[VPARAM_GLOBAL_BOX_VALUE] = self.global_box_folding_value as f32;
        config.vset_values[VPARAM_GLOBAL_SPHERICAL_FOLD] =
            u32::from(self.global_spherical_folding) as f32;
        config.vset_values[VPARAM_GLOBAL_SPHERICAL_OUTER] =
            self.global_spherical_folding_outer as f32;
        config.vset_values[VPARAM_GLOBAL_SPHERICAL_INNER] =
            self.global_spherical_folding_inner as f32;
        config.vset_values[VPARAM_ADVANCED_QUALITY] = u32::from(self.advanced_quality) as f32;
        config.vset_values[VPARAM_ABS_MIN_STEP] = (self.abs_min_marching_step * WORLD_SCALE) as f32;
        config.vset_values[VPARAM_ABS_MAX_STEP] = (self.abs_max_marching_step * WORLD_SCALE) as f32;
        config.vset_values[VPARAM_REL_MIN_STEP] = self.rel_min_marching_step as f32;
        config.vset_values[VPARAM_REL_MAX_STEP] = self.rel_max_marching_step as f32;
        config.vset_values[VPARAM_DE_FACTOR] = self.de_factor as f32;
        config.vset_values[VPARAM_SMOOTHNESS] = self.smoothness as f32;
        config.vset_values[VPARAM_DYNAMIC_THRESHOLD] = if self.iteration_threshold_mode {
            VPARAM_ITERATION_THRESHOLD_MODE
        } else {
            u32::from(!self.constant_de_threshold) as f32
        };
        config.vset_values[VPARAM_THRESHOLD_SCALE] = threshold_scale as f32;
        config.vset_values[VPARAM_CONSTANT_THRESHOLD] = (self.de_threshold * WORLD_SCALE) as f32;
        config.vset_values[VPARAM_MIN_THRESHOLD] = (self.detail_size_min * WORLD_SCALE) as f32;
        config.vset_values[VPARAM_MAX_THRESHOLD] = (self.detail_size_max * WORLD_SCALE) as f32;
        config.vset_values[VPARAM_CAMERA_PROJECTION] = self.camera_projection as f32;
        config.vset_values[VPARAM_FRACTAL_POSITION..VPARAM_FRACTAL_POSITION + 3]
            .copy_from_slice(&self.fractal_position.map(|value| value as f32));
        config.vset_values[VPARAM_FRACTAL_ROTATION..VPARAM_FRACTAL_ROTATION + 3]
            .copy_from_slice(&self.fractal_rotation.map(|value| value.to_radians() as f32));
        config.vset_values[VPARAM_FRACTAL_REPEAT..VPARAM_FRACTAL_REPEAT + 3]
            .copy_from_slice(&self.fractal_repeat.map(|value| value as f32));
        config.vset_values[VPARAM_DETAIL_LEVEL] = self.detail_level as f32;
        config.vset_values[VPARAM_INTERACTIVE_REFINEMENT] =
            std::env::var("FPT_MANDEL_PREVIEW_REFINEMENT_MASK")
                .ok()
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|mask| (1..=0xffff).contains(mask))
                .map(|mask| mask as f32)
                .or_else(|| {
                    std::env::var("FPT_MANDEL_PREVIEW_ITERATION_SCALE")
                        .ok()
                        .and_then(|value| value.parse::<f32>().ok())
                        .filter(|scale| scale.is_finite() && (0.125..=1.0).contains(scale))
                        .map(|scale| -scale)
                })
                .unwrap_or(0.0);
        config.vset_values[VPARAM_INTERACTIVE_SPATIAL] =
            std::env::var("FPT_MANDEL_PREVIEW_SPATIAL_STRIDE")
                .ok()
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|stride| matches!(stride, 2 | 4 | 8 | 16))
                .map_or(0.0, |stride| -(stride as f32));
        if self.formula_id == 10 && !self.hybrid_enabled {
            config.set_values[PARAM_IFS_SCALE] = self.ifs_scale as f32;
            config.set_values[PARAM_ABS_MASK] = bool_mask(&self.ifs_abs) as f32;
            config.set_values[PARAM_ENABLED_MASK] = bool_mask(&self.ifs_enabled) as f32;
            config.set_values[PARAM_OFFSET_X] = self.ifs_offset[0] as f32;
            config.set_values[PARAM_OFFSET_Y] = self.ifs_offset[1] as f32;
            config.set_values[PARAM_OFFSET_Z] = self.ifs_offset[2] as f32;
            config.set_values[PARAM_ROTATION_X] = self.ifs_rotation[0].to_radians() as f32;
            config.set_values[PARAM_ROTATION_Y] = self.ifs_rotation[1].to_radians() as f32;
            config.set_values[PARAM_ROTATION_Z] = self.ifs_rotation[2].to_radians() as f32;
            for (index, direction) in self.ifs_directions.iter().enumerate() {
                for (component, value) in direction.iter().enumerate() {
                    config.set_values[PARAM_DIRECTION_BASE + index * 3 + component] = *value as f32;
                }
            }
        } else {
            config.set_values[PARAM_INITIAL_W] = self.initial_waxis as f32;
            config.set_values[PARAM_ADD_C_CONSTANT] = u32::from(self.add_c_constant) as f32;
            config.set_values[PARAM_JULIA_MODE] = u32::from(self.julia_mode) as f32;
            config.set_values[PARAM_JULIA_C..PARAM_JULIA_C + 3]
                .copy_from_slice(&self.julia_c.map(|value| value as f32));
            config.set_values[PARAM_CONSTANT_MULTIPLIER..PARAM_CONSTANT_MULTIPLIER + 3]
                .copy_from_slice(&self.constant_multiplier.map(|value| value as f32));
        }
        config.set_values[PARAM_FORMULA_ID] = self.formula_id as f32;

        let analytic_formula = self.formula_id != 10;
        config.fractal_style_mode = if analytic_formula { 1 } else { 2 };
        config.fractal_style[1] = if analytic_formula { 5.0 } else { 0.0 };
        config.fractal_style[2] = if analytic_formula { 0.72 } else { 0.0 };
        config.fractal_style[3] = 1.0;
        config.fractal_style[4] = 0.72;
        config.fractal_style[5] = 0.08;
        config.fractal_style[6] = if analytic_formula { 0.04 } else { 0.0 };
        config.fractal_style[8..11].copy_from_slice(&self.material.surface_color);

        // Mandel compatibility renders intentionally use a deterministic,
        // unshadowed Lambert surface so geometry comparisons are independent
        // of either renderer's AO, specular, reflection, and post-effect paths.
        config.mandel_appearance_mode = 1;
        config.world[1] = 1.0;
        config.world[5] = 1.0;
        config.world[6] = 3.0;
        config.background_gradient[..3].fill(0.0);
        config.post = [-1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0];
    }

    fn main_light_direction(&self) -> [f64; 3] {
        let forward = normalize(map_mandel_vector(subtract(self.target, self.camera)))
            .expect("camera direction is non-zero");
        let top = normalize(map_mandel_vector(self.camera_top)).expect("camera top is non-zero");
        let right = normalize(cross(forward, top)).expect("camera basis is non-degenerate");
        // Mandelbulber retains 180.8 in this legacy conversion path. Match it
        // exactly because all pre-2.25 main-light angles pass through it.
        let rotation = self
            .main_light_rotation
            .map(|degrees| degrees / 180.8 * std::f64::consts::PI);
        let mut direction = scale(forward, -1.0);
        direction = rotate_around_axis(direction, forward, rotation[2]);
        direction = rotate_around_axis(direction, right, -rotation[1]);
        direction = rotate_around_axis(direction, top, rotation[0]);
        normalize(direction).expect("rotated light direction is non-zero")
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn has_cpu_reference(&self) -> bool {
        if self.hybrid_enabled {
            self.formula_slots
                .iter()
                .filter(|slot| slot.active())
                .all(|slot| {
                    matches!(
                        slot.formula_id,
                        3..=5
                            | 8..=10
                            | 103
                            | 122
                            | 607
                            | 610
                            | 1113
                            | 1600
                            | 1602
                            | 1603
                            | 1604
                            | 1613
                            | 1639
                            | 1646
                    )
                })
        } else {
            matches!(
                self.formula_id,
                2..=5
                    | 8..=10
                    | 48
                    | 103
                    | 122
                    | 607
                    | 610
                    | 1113
                    | 1600
                    | 1602
                    | 1603
                    | 1604
                    | 1613
                    | 1639
                    | 1646
            )
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn distance(&self, point: [f64; 3]) -> DistanceSample {
        assert!(
            self.has_cpu_reference(),
            "formula {} has no independent CPU reference",
            self.formula_id
        );
        let orbit_point = [point[0], point[1], point[2], self.initial_waxis];
        let sample = if self.hybrid_enabled {
            self.hybrid_orbit_program().evaluate(orbit_point)
        } else {
            self.orbit_program().evaluate(orbit_point)
        };
        DistanceSample {
            distance: sample.distance,
            radius: sample.radius,
            derivative: sample.derivative,
            iterations: sample.iterations,
            escaped: sample.escaped,
        }
    }

    fn orbit_program(&self) -> orbit::OrbitProgram {
        if matches!(self.formula_id, 2..=5) {
            let kernel = match self.formula_id {
                2 => orbit::FormulaKernel::Mandelbulb {
                    power: self.bulb_power,
                    alpha_angle: self.bulb_alpha_angle,
                    beta_angle: self.bulb_beta_angle,
                },
                3 => orbit::FormulaKernel::MandelbulbPower2,
                4 => orbit::FormulaKernel::Hypercomplex,
                5 => orbit::FormulaKernel::Quaternion,
                _ => unreachable!(),
            };
            return orbit::OrbitProgram {
                max_iterations: self.max_iterations,
                bailout: self.bailout,
                add_constant: self.add_c_constant.then_some(orbit::ConstantAddition {
                    julia: self.julia_mode.then_some([
                        self.julia_c[0],
                        self.julia_c[1],
                        self.julia_c[2],
                        0.0,
                    ]),
                    multiplier: [
                        self.constant_multiplier[0],
                        self.constant_multiplier[1],
                        self.constant_multiplier[2],
                        1.0,
                    ],
                    swap_xy: false,
                }),
                finalizer: orbit::AnalyticFinalizer::Logarithmic,
                finalizer_parameters: orbit::FinalizerParameters::default(),
                kernel,
            };
        }
        if self.formula_id == 48 {
            return orbit::OrbitProgram {
                max_iterations: self.max_iterations,
                bailout: self.bailout,
                add_constant: self.add_c_constant.then_some(orbit::ConstantAddition {
                    julia: self.julia_mode.then_some([
                        self.julia_c[0],
                        self.julia_c[1],
                        self.julia_c[2],
                        0.0,
                    ]),
                    multiplier: [
                        self.constant_multiplier[0],
                        self.constant_multiplier[1],
                        self.constant_multiplier[2],
                        1.0,
                    ],
                    swap_xy: false,
                }),
                finalizer: orbit::AnalyticFinalizer::Logarithmic,
                finalizer_parameters: orbit::FinalizerParameters::default(),
                kernel: orbit::FormulaKernel::QuickDudley {
                    derivative_scale: self.analytic_de_scale,
                    derivative_offset: self.analytic_de_offset1,
                },
            };
        }
        if self.formula_id == 122 {
            let (kernel, finalizer_parameters) =
                jos_kleinian_from_parameters(&self.formula_parameters)
                    .expect("validated Jos Kleinian parameters");
            return orbit::OrbitProgram {
                max_iterations: self.max_iterations,
                bailout: self.bailout,
                add_constant: None,
                finalizer: orbit::AnalyticFinalizer::JosKleinian,
                finalizer_parameters,
                kernel,
            };
        }
        if self.formula_id == 103 {
            return orbit::OrbitProgram {
                max_iterations: self.max_iterations,
                bailout: self.bailout,
                add_constant: None,
                finalizer: orbit::AnalyticFinalizer::PseudoKleinian,
                finalizer_parameters: orbit::FinalizerParameters::default(),
                kernel: pseudo_kleinian_from_parameters(&self.formula_parameters)
                    .expect("validated Pseudo-Kleinian parameters"),
            };
        }
        if self.formula_id == 610 {
            return orbit::OrbitProgram {
                max_iterations: self.max_iterations,
                bailout: self.bailout,
                add_constant: None,
                finalizer: orbit::AnalyticFinalizer::CustomDistance,
                finalizer_parameters: orbit::FinalizerParameters::default(),
                kernel: difs_msltoe_donut_from_parameters(&self.formula_parameters)
                    .expect("validated dIFS Msltoe Donut parameters"),
            };
        }
        if self.formula_id == 607 {
            return orbit::OrbitProgram {
                max_iterations: self.max_iterations,
                bailout: self.bailout,
                add_constant: None,
                finalizer: orbit::AnalyticFinalizer::CustomDistance,
                finalizer_parameters: orbit::FinalizerParameters::default(),
                kernel: difs_menger_from_parameters(&self.formula_parameters)
                    .expect("validated dIFS Menger parameters"),
            };
        }
        if self.formula_id == 1113 {
            return orbit::OrbitProgram {
                max_iterations: self.max_iterations,
                bailout: self.bailout,
                add_constant: None,
                finalizer: orbit::AnalyticFinalizer::CustomDistance,
                finalizer_parameters: orbit::FinalizerParameters::default(),
                kernel: transf_de_linear_cube_from_parameters(&self.formula_parameters)
                    .expect("validated Linear Cube transform parameters"),
            };
        }
        if self.formula_id == 1613 {
            return orbit::OrbitProgram {
                max_iterations: self.max_iterations,
                bailout: self.bailout,
                add_constant: None,
                finalizer: orbit::AnalyticFinalizer::CustomDistance,
                finalizer_parameters: orbit::FinalizerParameters::default(),
                kernel: transf_difs_grid_from_parameters(&self.formula_parameters)
                    .expect("validated dIFS Grid transform parameters"),
            };
        }
        if matches!(self.formula_id, 1600 | 1602 | 1604 | 1639) {
            let kernel = match self.formula_id {
                1600 => transf_difs_box_from_parameters(&self.formula_parameters)
                    .expect("validated dIFS Box transform parameters"),
                1602 => transf_difs_ellipsoid_from_parameters(&self.formula_parameters)
                    .expect("validated dIFS Ellipsoid transform parameters"),
                1604 => transf_difs_sphere_from_parameters(&self.formula_parameters)
                    .expect("validated dIFS Sphere transform parameters"),
                1639 => transf_difs_chessboard_from_parameters(&self.formula_parameters)
                    .expect("validated dIFS Chessboard transform parameters"),
                _ => unreachable!(),
            };
            return orbit::OrbitProgram {
                max_iterations: self.max_iterations,
                bailout: self.bailout,
                add_constant: None,
                finalizer: orbit::AnalyticFinalizer::CustomDistance,
                finalizer_parameters: orbit::FinalizerParameters::default(),
                kernel,
            };
        }
        if matches!(self.formula_id, 1603 | 1646) {
            let kernel = match self.formula_id {
                1603 => transf_difs_hextgrid2_from_parameters(&self.formula_parameters)
                    .expect("validated dIFS Hextgrid2 transform parameters"),
                1646 => transf_difs_torus_v4_from_parameters(&self.formula_parameters)
                    .expect("validated dIFS Torus V4 transform parameters"),
                _ => unreachable!(),
            };
            return orbit::OrbitProgram {
                max_iterations: self.max_iterations,
                bailout: self.bailout,
                add_constant: None,
                finalizer: orbit::AnalyticFinalizer::CustomDistance,
                finalizer_parameters: orbit::FinalizerParameters::default(),
                kernel,
            };
        }
        if matches!(self.formula_id, 8 | 9) {
            let fixed_radius_squared = self.mandelbox_fixed_radius.powi(2);
            let minimum_radius_squared = self.mandelbox_min_radius.powi(2);
            let add_constant = self.add_c_constant.then_some(orbit::ConstantAddition {
                julia: self.julia_mode.then_some([
                    self.julia_c[0],
                    self.julia_c[1],
                    self.julia_c[2],
                    0.0,
                ]),
                multiplier: [
                    self.constant_multiplier[0],
                    self.constant_multiplier[1],
                    self.constant_multiplier[2],
                    1.0,
                ],
                swap_xy: false,
            });
            if self.formula_id == 8 {
                return orbit::OrbitProgram {
                    max_iterations: self.max_iterations,
                    bailout: self.bailout,
                    add_constant,
                    finalizer: orbit::AnalyticFinalizer::Linear,
                    finalizer_parameters: orbit::FinalizerParameters::default(),
                    kernel: orbit::FormulaKernel::Mandelbox(Box::new(orbit::Mandelbox {
                        scale: self.mandelbox_scale,
                        folding_limit: self.mandelbox_folding_limit,
                        folding_value: self.mandelbox_folding_value,
                        fixed_radius_squared,
                        minimum_radius_squared,
                        minimum_radius_factor: fixed_radius_squared / minimum_radius_squared,
                        offset: [
                            self.mandelbox_offset[0],
                            self.mandelbox_offset[1],
                            self.mandelbox_offset[2],
                            0.0,
                        ],
                        color_factor: self.mandelbox_color,
                        color_sphere_min: self.mandelbox_color_sp1,
                        color_sphere_fixed: self.mandelbox_color_sp2,
                        rotations_enabled: self.mandelbox_rotations_enabled,
                        main_rotation_enabled: self.mandelbox_main_rotation_enabled,
                        main_rotation: rotation2_matrix(self.mandelbox_main_rotation),
                        rotations: self.mandelbox_rotations,
                        inverse_rotations: self.mandelbox_inverse_rotations,
                    })),
                };
            }
            return orbit::OrbitProgram {
                max_iterations: self.max_iterations,
                bailout: self.bailout,
                add_constant,
                finalizer: orbit::AnalyticFinalizer::Linear,
                finalizer_parameters: orbit::FinalizerParameters::default(),
                kernel: orbit::FormulaKernel::MandelboxFast {
                    scale: self.mandelbox_scale,
                    fixed_radius_squared,
                    minimum_radius_squared,
                    minimum_radius_factor: fixed_radius_squared / minimum_radius_squared,
                    main_rotation_enabled: self.mandelbox_main_rotation_enabled,
                    main_rotation: rotation2_matrix(self.mandelbox_main_rotation),
                },
            };
        }
        assert_eq!(self.formula_id, 10, "unsupported CPU orbit formula");
        orbit::OrbitProgram {
            max_iterations: self.max_iterations,
            bailout: self.bailout,
            add_constant: None,
            finalizer: orbit::AnalyticFinalizer::Ifs,
            finalizer_parameters: orbit::FinalizerParameters::default(),
            kernel: orbit::FormulaKernel::KaleidoscopicIfs(Box::new(
                kaleidoscopic_ifs_from_parameters(&self.formula_parameters)
                    .expect("validated single-formula IFS parameters"),
            )),
        }
    }

    fn hybrid_orbit_program(&self) -> orbit::HybridOrbitProgram {
        assert!(
            self.hybrid_enabled,
            "hybrid orbit requested for a single formula"
        );
        let sequence = self
            .hybrid_sequence()
            .expect("validated hybrid scene has complete iteration coverage");
        let mut slots = std::array::from_fn(|_| None);
        for (index, scene_slot) in self.formula_slots.iter().enumerate() {
            if !scene_slot.active() {
                continue;
            }
            let kernel = match scene_slot.formula_id {
                3 => orbit::FormulaKernel::MandelbulbPower2,
                4 => orbit::FormulaKernel::Hypercomplex,
                5 => orbit::FormulaKernel::Quaternion,
                8 | 9 => {
                    mandelbox_kernel_from_parameters(scene_slot.formula_id, &scene_slot.parameters)
                        .expect("validated hybrid Mandelbox parameters")
                }
                10 => orbit::FormulaKernel::KaleidoscopicIfs(Box::new(
                    kaleidoscopic_ifs_from_parameters(&scene_slot.parameters)
                        .expect("validated hybrid IFS parameters"),
                )),
                122 => {
                    jos_kleinian_from_parameters(&scene_slot.parameters)
                        .expect("validated hybrid Jos Kleinian parameters")
                        .0
                }
                103 => pseudo_kleinian_from_parameters(&scene_slot.parameters)
                    .expect("validated hybrid Pseudo-Kleinian parameters"),
                610 => difs_msltoe_donut_from_parameters(&scene_slot.parameters)
                    .expect("validated hybrid dIFS Msltoe Donut parameters"),
                607 => difs_menger_from_parameters(&scene_slot.parameters)
                    .expect("validated hybrid dIFS Menger parameters"),
                1113 => transf_de_linear_cube_from_parameters(&scene_slot.parameters)
                    .expect("validated hybrid Linear Cube transform parameters"),
                1613 => transf_difs_grid_from_parameters(&scene_slot.parameters)
                    .expect("validated hybrid dIFS Grid transform parameters"),
                1600 => transf_difs_box_from_parameters(&scene_slot.parameters)
                    .expect("validated hybrid dIFS Box transform parameters"),
                1602 => transf_difs_ellipsoid_from_parameters(&scene_slot.parameters)
                    .expect("validated hybrid dIFS Ellipsoid transform parameters"),
                1604 => transf_difs_sphere_from_parameters(&scene_slot.parameters)
                    .expect("validated hybrid dIFS Sphere transform parameters"),
                1639 => transf_difs_chessboard_from_parameters(&scene_slot.parameters)
                    .expect("validated hybrid dIFS Chessboard transform parameters"),
                1603 => transf_difs_hextgrid2_from_parameters(&scene_slot.parameters)
                    .expect("validated hybrid dIFS Hextgrid2 transform parameters"),
                1646 => transf_difs_torus_v4_from_parameters(&scene_slot.parameters)
                    .expect("validated hybrid dIFS Torus V4 transform parameters"),
                formula_id => panic!("formula {formula_id} has no hybrid CPU kernel"),
            };
            slots[index] = Some(orbit::HybridOrbitSlot {
                kernel,
                add_constant: scene_slot
                    .add_c_constant
                    .then_some(orbit::ConstantAddition {
                        julia: self.julia_mode.then_some([
                            self.julia_c[0],
                            self.julia_c[1],
                            self.julia_c[2],
                            0.0,
                        ]),
                        multiplier: [
                            self.constant_multiplier[0],
                            self.constant_multiplier[1],
                            self.constant_multiplier[2],
                            1.0,
                        ],
                        swap_xy: false,
                    }),
                weight: scene_slot.weight,
                check_for_bailout: scene_slot.check_for_bailout,
                bailout: scene_slot.bailout,
            });
        }
        let mut counts = [0_u64; 6];
        for slot in self.formula_slots.iter().filter(|slot| slot.active()) {
            let index = match slot.formula_id {
                3..=5 => 1,
                8..=10 => 0,
                103 => 2,
                122 => 3,
                607 | 610 | 1113 | 1600 | 1602 | 1603 | 1604 | 1613 | 1639 | 1646 => 4,
                _ => unreachable!("CPU-supported hybrid formula has a DE family"),
            };
            counts[index] += u64::from(slot.iterations);
        }
        let selected = if counts[4] > 0 {
            4
        } else {
            counts
                .iter()
                .enumerate()
                .max_by_key(|(index, count)| (**count, std::cmp::Reverse(*index)))
                .map(|(index, _)| index)
                .unwrap_or(0)
        };
        let (finalizer, finalizer_parameters) = match selected {
            0 => (
                orbit::AnalyticFinalizer::Linear,
                orbit::FinalizerParameters {
                    offset1: self.linear_de_offset,
                    ..orbit::FinalizerParameters::default()
                },
            ),
            1 => (
                orbit::AnalyticFinalizer::Logarithmic,
                orbit::FinalizerParameters::default(),
            ),
            2 => (
                orbit::AnalyticFinalizer::PseudoKleinian,
                orbit::FinalizerParameters::default(),
            ),
            3 => {
                let primary = &self.formula_slots[0];
                let parameters = jos_kleinian_from_parameters(&primary.parameters)
                    .expect("Jos finalizer uses validated primary-slot parameters")
                    .1;
                (orbit::AnalyticFinalizer::JosKleinian, parameters)
            }
            4 => (
                orbit::AnalyticFinalizer::CustomDistance,
                orbit::FinalizerParameters::default(),
            ),
            _ => unreachable!("unsupported CPU hybrid finalizer"),
        };
        orbit::HybridOrbitProgram {
            sequence,
            slots,
            finalizer,
            finalizer_parameters,
        }
    }
}

fn difs_menger_from_parameters(values: &BTreeMap<String, String>) -> Result<orbit::FormulaKernel> {
    let vector4 = |name: &str, fallback: [f64; 3], w: f64| -> Result<[f64; 4]> {
        let value = parameter_vec3(values, name, fallback)?;
        Ok([value[0], value[1], value[2], w])
    };
    let range = |start: &str, stop: &str| -> Result<[u32; 2]> {
        Ok([
            parameter_u32(values, start, 0)?,
            parameter_u32(values, stop, 250)?,
        ])
    };
    let integer = |name: &str, fallback: u32| -> Result<i32> {
        i32::try_from(parameter_u32(values, name, fallback)?)
            .with_context(|| format!("{name} exceeds signed integer range"))
    };
    let initial_scale = parameter_number(values, "transf_scale_05", 0.5)?;
    let fold_offset = parameter_number(values, "transf_offset_3", 3.0)?;
    ensure!(
        initial_scale != 0.0,
        "transf_scale_05 must be non-zero for dIFS Menger"
    );
    ensure!(
        fold_offset != 0.0,
        "transf_offset_3 must be non-zero for dIFS Menger"
    );
    let rotation = parameter_vec3(values, "transf_rotation", [0.0; 3])?.map(f64::to_radians);
    Ok(orbit::FormulaKernel::DifsMenger(Box::new(
        orbit::DifsMenger {
            box_fold: vector4("transf_addition_constantA_111", [1.0; 3], 0.0)?,
            absolute_enabled: [
                parameter_bool(values, "transf_function_enabledAx", true)?,
                parameter_bool(values, "transf_function_enabledAy", true)?,
                parameter_bool(values, "transf_function_enabledAz_false", false)?,
            ],
            absolute_ranges: [
                range("transf_start_iterations_X", "transf_stop_iterations_X")?,
                range("transf_start_iterations_Y", "transf_stop_iterations_Y")?,
                range("transf_start_iterations_Z", "transf_stop_iterations_Z")?,
            ],
            folds_enabled: parameter_bool(values, "transf_function_enabled_false", false)?,
            xy_fold_enabled: parameter_bool(values, "transf_function_enabledA_false", false)?,
            xy_fold_range: range("transf_start_iterations_A", "transf_stop_iterations_A")?,
            xyz_fold_enabled: parameter_bool(values, "transf_function_enabledB_false", false)?,
            xyz_fold_range: range("transf_start_iterations_B", "transf_stop_iterations_B")?,
            polyfold_enabled: parameter_bool(values, "transf_function_enabledP_false", false)?,
            polyfold_range: range("transf_start_iterations_P", "transf_stop_iterations_P")?,
            polyfold_sides: integer("transf_int_6", 6)?,
            diagonal_one_enabled: parameter_bool(values, "transf_function_enabledCx_false", false)?,
            diagonal_one_range: range("transf_start_iterations_Cx", "transf_stop_iterations_Cx")?,
            x_offset_enabled: parameter_bool(values, "transf_function_enabledC_false", false)?,
            x_offset_range: range("transf_start_iterations_C", "transf_stop_iterations_C")?,
            x_offset: parameter_number(values, "transf_offsetC_0", 0.0)?,
            y_offset_enabled: parameter_bool(values, "transf_function_enabledD_false", false)?,
            y_offset_range: range("transf_start_iterations_D", "transf_stop_iterations_D")?,
            y_offset: parameter_number(values, "transf_offsetD_0", 0.0)?,
            diagonal_two_enabled: parameter_bool(values, "transf_function_enabledCy_false", false)?,
            diagonal_two_range: range("transf_start_iterations_Cy", "transf_stop_iterations_Cy")?,
            reverse_x_range: range("transf_start_iterations_E", "transf_stop_iterations_E")?,
            reverse_x_offset: parameter_number(values, "transf_offsetE_2", 2.0)?,
            reverse_y_range: range("transf_start_iterations_F", "transf_stop_iterations_F")?,
            reverse_y_offset: parameter_number(values, "transf_offsetF_2", 2.0)?,
            scale_range: range("transf_start_iterations_S", "transf_stop_iterations_S")?,
            scale: parameter_number(values, "transf_scale_2", 2.0)?,
            scale_vary_enabled: parameter_bool(values, "transf_function_enabledK_false", false)?,
            scale_vary_range: range("transf_start_iterations_K", "transf_stop_iterations_K")?,
            scale_vary: parameter_number(values, "transf_scale_vary_0", 0.0)?,
            scale_target: parameter_number(values, "transf_scaleC_1", 1.0)?,
            offset: vector4("transf_offset_001", [0.0, 0.0, 1.0], 0.0)?,
            rotation_enabled: parameter_bool(values, "transf_function_enabledR_false", false)?,
            rotation_range: range("transf_start_iterations_R", "transf_stop_iterations_R")?,
            rotation: rotation2_matrix(rotation),
            menger_range: range("transf_start_iterations", "transf_stop_iterations")?,
            menger_initial_scale: initial_scale,
            menger_iterations: integer("transf_int8_X", 8)?,
            menger_offset: vector4("transf_offsetA_000", [0.0; 3], 0.0)?,
            menger_fold_offset: fold_offset,
            menger_unconditional_fold: parameter_bool(
                values,
                "transf_function_enabledJ_false",
                false,
            )?,
            menger_scale: parameter_number(values, "transf_scale_3", 3.0)?,
            color_enabled: parameter_bool(values, "fold_color_aux_color_enabled", true)?,
            color_detail_enabled: parameter_bool(
                values,
                "fold_color_aux_color_enabled_false",
                false,
            )?,
            color_replace: parameter_bool(values, "fold_color_aux_color_enabledA", true)?,
            color_range: range(
                "fold_color_start_iterations_A",
                "fold_color_stop_iterations_A",
            )?,
            color_iteration_scale: parameter_number(values, "fold_color_difs1", 1.0)?,
            color_base: parameter_number(values, "fold_color_difs0", 0.0)?,
            color_factors: parameter_vec4(values, "fold_color_difs_0000", [0.0; 4])?,
        },
    )))
}

fn difs_msltoe_donut_from_parameters(
    values: &BTreeMap<String, String>,
) -> Result<orbit::FormulaKernel> {
    let number = parameter_number(values, "donut_number", 9.0)?;
    ensure!(number != 0.0, "donut_number must be non-zero");
    Ok(orbit::FormulaKernel::DifsMsltoeDonut(
        orbit::DifsMsltoeDonut {
            factor: parameter_number(values, "donut_factor", 3.0)?,
            number,
            ring_radius: parameter_number(values, "donut_ring_radius", 1.0)?,
            ring_thickness: parameter_number(values, "donut_ring_thickness", 0.1)?,
            color_enabled: parameter_bool(values, "fold_color_aux_color_enabled", true)?,
            color_add: parameter_number(values, "fold_color_difs1", 1.0)?,
        },
    ))
}

fn transf_de_linear_cube_from_parameters(
    values: &BTreeMap<String, String>,
) -> Result<orbit::FormulaKernel> {
    Ok(orbit::FormulaKernel::TransfDeLinearCube(
        orbit::TransfDeLinearCube {
            mix_enabled: parameter_bool(values, "transf_function_enabledA_false", false)?,
            euclidean_enabled: parameter_bool(values, "transf_function_enabled_false", false)?,
            mix: parameter_number(values, "transf_scaleA_0", 0.0)?,
            scale: parameter_number(values, "transf_scale_1", 1.0)?,
            offset: parameter_number(values, "transf_offset_0", 0.0)?,
            color_enabled: parameter_bool(values, "fold_color_aux_color_enabled_false", false)?,
            color_scale: parameter_number(values, "fold_color_difs1", 1.0)?,
        },
    ))
}

fn transf_difs_grid_from_parameters(
    values: &BTreeMap<String, String>,
) -> Result<orbit::FormulaKernel> {
    let size = parameter_number(values, "transf_scale_1", 1.0)?;
    let z_scale = parameter_number(values, "transf_scaleF_1", 1.0)?;
    ensure!(size != 0.0, "transf_scale_1 must be non-zero for dIFS Grid");
    ensure!(
        z_scale != 0.0,
        "transf_scaleF_1 must be non-zero for dIFS Grid"
    );
    let rotation = parameter_vec3(values, "transf_rotation", [0.0; 3])?.map(f64::to_radians);
    Ok(orbit::FormulaKernel::TransfDifsGrid(Box::new(
        orbit::TransfDifsGrid {
            size,
            z_scale,
            rotation_enabled: parameter_bool(values, "transf_rotation_enabled", false)?,
            rotation: rotation2_matrix(rotation),
            square_cross_section: parameter_bool(values, "transf_function_enabledJ_false", false)?,
            radius: parameter_number(values, "transf_offset_0005", 0.005)?,
            color_enabled: parameter_bool(values, "fold_color_aux_color_enabled_false", false)?,
            color_add_enabled: parameter_bool(
                values,
                "fold_color_aux_color_enabledB_false",
                false,
            )?,
            color_base: parameter_vec4(values, "fold_color_difs_0000", [0.0; 4])?,
            color_iteration_scale: parameter_number(values, "fold_color_difs0", 0.0)?,
            color_start: parameter_u32(values, "fold_color_start_iterations_A", 0)?,
            color_stop: parameter_u32(values, "fold_color_stop_iterations_A", 250)?,
        },
    )))
}

fn transf_difs_box_from_parameters(
    values: &BTreeMap<String, String>,
) -> Result<orbit::FormulaKernel> {
    let half_size = parameter_vec3(values, "transf_addition_constant_111", [1.0; 3])?;
    Ok(orbit::FormulaKernel::TransfDifsBox(Box::new(
        orbit::TransfDifsBox {
            half_size: [half_size[0], half_size[1], half_size[2], 0.0],
            distance_offset: parameter_number(values, "transf_offsetB_0", 0.0)?,
            color_enabled: parameter_bool(values, "fold_color_aux_color_enabled_false", false)?,
            color_add_enabled: parameter_bool(
                values,
                "fold_color_aux_color_enabledB_false",
                false,
            )?,
            color_axis_enabled: parameter_bool(
                values,
                "fold_color_aux_color_enabledA_false",
                false,
            )?,
            color_base: parameter_vec4(values, "fold_color_difs_0000", [0.0; 4])?,
            color_iteration_scale: parameter_number(values, "fold_color_difs0", 0.0)?,
            color_octant_offset: parameter_number(values, "transf_offset_0", 0.0)?,
            color_start: parameter_u32(values, "fold_color_start_iterations_A", 0)?,
            color_stop: parameter_u32(values, "fold_color_stop_iterations_A", 250)?,
        },
    )))
}

fn transf_difs_ellipsoid_from_parameters(
    values: &BTreeMap<String, String>,
) -> Result<orbit::FormulaKernel> {
    let radii = parameter_vec3(values, "transf_addition_constant_111", [1.0; 3])?;
    ensure!(
        radii.into_iter().all(|radius| radius != 0.0),
        "transf_addition_constant_111 radii must be non-zero for dIFS Ellipsoid"
    );
    Ok(orbit::FormulaKernel::TransfDifsEllipsoid(Box::new(
        orbit::TransfDifsEllipsoid {
            radii,
            color_enabled: parameter_bool(values, "fold_color_aux_color_enabled_false", false)?,
            color_add_enabled: parameter_bool(
                values,
                "fold_color_aux_color_enabledB_false",
                false,
            )?,
            color_base: parameter_vec4(values, "fold_color_difs_0000", [0.0; 4])?,
            color_iteration_scale: parameter_number(values, "fold_color_difs0", 0.0)?,
            color_start: parameter_u32(values, "fold_color_start_iterations_A", 0)?,
            color_stop: parameter_u32(values, "fold_color_stop_iterations_A", 250)?,
        },
    )))
}

fn transf_difs_sphere_from_parameters(
    values: &BTreeMap<String, String>,
) -> Result<orbit::FormulaKernel> {
    Ok(orbit::FormulaKernel::TransfDifsSphere(Box::new(
        orbit::TransfDifsSphere {
            radius: parameter_number(values, "transf_offsetR_1", 1.0)?,
            analytic_offset: parameter_number(values, "analyticDE_offset_0", 0.0)?,
            four_dimensional: parameter_bool(values, "transf_function_enabled4d_false", false)?,
            color_enabled: parameter_bool(values, "fold_color_aux_color_enabled_false", false)?,
            color_add_enabled: parameter_bool(
                values,
                "fold_color_aux_color_enabledB_false",
                false,
            )?,
            color_base: parameter_vec4(values, "fold_color_difs_0000", [0.0; 4])?,
            color_iteration_scale: parameter_number(values, "fold_color_difs0", 0.0)?,
            color_start: parameter_u32(values, "fold_color_start_iterations_A", 0)?,
            color_stop: parameter_u32(values, "fold_color_stop_iterations_A", 250)?,
        },
    )))
}

fn transf_difs_chessboard_from_parameters(
    values: &BTreeMap<String, String>,
) -> Result<orbit::FormulaKernel> {
    let vector4 = |name: &str, fallback: [f64; 3], w: f64| -> Result<[f64; 4]> {
        let value = parameter_vec3(values, name, fallback)?;
        Ok([value[0], value[1], value[2], w])
    };
    Ok(orbit::FormulaKernel::TransfDifsChessboard(Box::new(
        orbit::TransfDifsChessboard {
            color_disabled: parameter_bool(values, "fold_color_aux_color_enabled_false", false)?,
            color_add_enabled: parameter_bool(
                values,
                "fold_color_aux_color_enabledA_false",
                false,
            )?,
            color_three_dimensional: parameter_bool(
                values,
                "transf_function_enabledC_false",
                false,
            )?,
            color_offset: vector4("transf_offset_000", [0.0; 3], 0.0)?,
            color_repeats: vector4("transf_scale3D_444", [4.0; 3], 1.0)?,
            box_half_size: vector4("transf_offset_110", [1.0, 1.0, 0.0], 0.0)?,
            plane_enabled: parameter_bool(values, "transf_function_enabled_false", false)?,
            mutate_orbit: parameter_bool(values, "transf_function_enabledZc_false", false)?,
            mutate_start: parameter_u32(values, "transf_start_iterations_Zc", 0)?,
            mutate_stop: parameter_u32(values, "transf_stop_iterations_Zc", 250)?,
            replace_distance: parameter_bool(values, "transf_function_enabledD_false", false)?,
        },
    )))
}

fn transf_difs_hextgrid2_from_parameters(
    values: &BTreeMap<String, String>,
) -> Result<orbit::FormulaKernel> {
    let size = parameter_number(values, "transf_scale_1", 1.0)?;
    let z_scale = parameter_number(values, "transf_scaleF_1", 1.0)?;
    ensure!(
        size != 0.0,
        "transf_scale_1 must be non-zero for dIFS Hextgrid2"
    );
    ensure!(
        z_scale != 0.0,
        "transf_scaleF_1 must be non-zero for dIFS Hextgrid2"
    );
    let offset = parameter_vec3(values, "transf_offset_000", [0.0; 3])?;
    let rotation = parameter_vec3(values, "transf_rotation", [0.0; 3])?.map(f64::to_radians);
    Ok(orbit::FormulaKernel::TransfDifsHextgrid2(Box::new(
        orbit::TransfDifsHextgrid2 {
            pre_transform_enabled: parameter_bool(values, "transf_function_enabledG_false", false)?,
            pre_scale: parameter_number(values, "transf_scaleA_1", 1.0)?,
            pre_offset: [offset[0], offset[1], offset[2], 0.0],
            negate_abs: [
                parameter_bool(values, "transf_function_enabledx_false", false)?,
                parameter_bool(values, "transf_function_enabledy_false", false)?,
                parameter_bool(values, "transf_function_enabledz_false", false)?,
            ],
            size,
            z_scale,
            rotation_enabled: parameter_bool(values, "transf_rotation_enabled", false)?,
            rotation: rotation2_matrix(rotation),
            square_cross_section: parameter_bool(values, "transf_function_enabledJ_false", false)?,
            radius: parameter_number(values, "transf_offset_0005", 0.005)?,
            analytic_offset: parameter_number(values, "analyticDE_offset_0", 0.0)?,
            color_enabled: parameter_bool(values, "fold_color_aux_color_enabled_false", false)?,
            color_add_enabled: parameter_bool(
                values,
                "fold_color_aux_color_enabledB_false",
                false,
            )?,
            color_base: parameter_vec4(values, "fold_color_difs_0000", [0.0; 4])?,
            color_iteration_scale: parameter_number(values, "fold_color_difs0", 0.0)?,
            color_start: parameter_u32(values, "fold_color_start_iterations_A", 0)?,
            color_stop: parameter_u32(values, "fold_color_stop_iterations_A", 250)?,
        },
    )))
}

fn transf_difs_torus_v4_from_parameters(
    values: &BTreeMap<String, String>,
) -> Result<orbit::FormulaKernel> {
    let offset = parameter_vec3(values, "transf_offset_000", [0.0; 3])?;
    let rotation = parameter_vec3(values, "transf_rotation", [0.0; 3])?.map(f64::to_radians);
    let angle = parameter_number(values, "transf_angle_deg_A", 0.0)?.to_radians();
    Ok(orbit::FormulaKernel::TransfDifsTorusV4(Box::new(
        orbit::TransfDifsTorusV4 {
            transform_enabled: parameter_bool(values, "transf_function_enabledT_false", false)?,
            transform_start: parameter_u32(values, "transf_start_iterations_T", 0)?,
            transform_stop: parameter_u32(values, "transf_stop_iterations_T", 250)?,
            scale: parameter_number(values, "transf_scale_1", 1.0)?,
            fold_start: parameter_u32(values, "transf_start_iterations_M", 0)?,
            fold_stop: parameter_u32(values, "transf_stop_iterations_M", 250)?,
            absolute_enabled: parameter_bool(values, "transf_function_enabledG_false", false)?,
            absolute_axes: [
                parameter_bool(values, "transf_function_enabledAx_false", false)?,
                parameter_bool(values, "transf_function_enabledAy_false", false)?,
                parameter_bool(values, "transf_function_enabledAz_false", false)?,
            ],
            offset: [offset[0], offset[1], offset[2], 0.0],
            rotation_enabled: parameter_bool(values, "transf_function_enabledR_false", false)?,
            rotation_start: parameter_u32(values, "transf_start_iterations_R", 0)?,
            rotation_stop: parameter_u32(values, "transf_stop_iterations_R", 250)?,
            rotation: rotation2_matrix(rotation),
            angle,
            angle_sine: angle.sin(),
            angle_cosine: angle.cos(),
            major_radius: parameter_number(values, "transf_radius_1", 1.0)?,
            hollow_enabled: parameter_bool(values, "transf_function_enabledC_false", false)?,
            square_cross_section: parameter_bool(values, "transf_function_enabledJ_false", false)?,
            shell_radius: parameter_number(values, "transf_offset_0005", 0.005)?,
            surface_offset: parameter_number(values, "transf_offset_02", 0.2)?,
            analytic_offset: parameter_number(values, "analyticDE_offset_0", 0.0)?,
            color_enabled: parameter_bool(values, "fold_color_aux_color_enabled_false", false)?,
            color_add_enabled: parameter_bool(
                values,
                "fold_color_aux_color_enabledB_false",
                false,
            )?,
            color_base: parameter_vec4(values, "fold_color_difs_0000", [0.0; 4])?,
            color_iteration_scale: parameter_number(values, "fold_color_difs0", 0.0)?,
            color_start: parameter_u32(values, "fold_color_start_iterations_A", 0)?,
            color_stop: parameter_u32(values, "fold_color_stop_iterations_A", 250)?,
        },
    )))
}

fn pseudo_kleinian_from_parameters(
    values: &BTreeMap<String, String>,
) -> Result<orbit::FormulaKernel> {
    let vector4 = |name: &str, fallback: [f64; 3], w: f64| -> Result<[f64; 4]> {
        let value = parameter_vec3(values, name, fallback)?;
        Ok([value[0], value[1], value[2], w])
    };
    let rotation = parameter_vec3(values, "transf_rotation", [0.0; 3])?.map(f64::to_radians);
    Ok(orbit::FormulaKernel::PseudoKleinian(Box::new(
        orbit::PseudoKleinian {
            analytic_offset: parameter_number(values, "analyticDE_offset_0", 0.0)?,
            analytic_scale: parameter_number(values, "analyticDE_scale_1", 1.0)?,
            analytic_tweak: parameter_number(values, "analyticDE_tweak_005", 0.05)?,
            color_grid_enabled: parameter_bool(
                values,
                "fold_color_aux_color_enabledA_false",
                false,
            )?,
            color_add_enabled: parameter_bool(values, "fold_color_aux_color_enabled_false", false)?,
            color_factors: parameter_vec4(values, "fold_color_difs_0000", [0.0; 4])?,
            color_mix: parameter_number(values, "fold_color_difs1", 1.0)?,
            color_start: parameter_u32(values, "fold_color_start_iterations_A", 0)?,
            color_stop: parameter_u32(values, "fold_color_stop_iterations_A", 250)?,
            box_color: parameter_vec3(values, "mandelbox_color", [0.03, 0.05, 0.07])?,
            folding_limit: parameter_number(values, "mandelbox_folding_limit", 1.0)?,
            folding_value: parameter_number(values, "mandelbox_folding_value", 2.0)?,
            addition: vector4("transf_addition_constant", [0.0; 3], 0.0)?,
            box_size: vector4("transf_addition_constant_0777", [0.7; 3], 0.0)?,
            fold_size: vector4("transf_addition_constant_111", [1.0; 3], 0.0)?,
            inversion_addition: vector4("transf_addition_constantA_000", [0.0; 3], 0.0)?,
            grid_phase: vector4("transf_addition_constantP_000", [0.0; 3], 0.0)?,
            offset_zero: vector4("transf_offset_000", [0.0; 3], 0.0)?,
            prism_offset: vector4("transf_constant_multiplier_000", [0.0; 3], 1.0)?,
            grid_scale: vector4("transf_constant_multiplier_111", [1.0; 3], 1.0)?,
            grid_multiplier: vector4("transf_constant_multiplierC_111", [1.0; 3], 1.0)?,
            rotation: rotation2_matrix(rotation),
            max_radius_squared: parameter_number(values, "transf_maxR2_1", 1.0)?,
            minimum_radius: parameter_number(values, "transf_minimum_radius_05", 0.5)?,
            prism_scale: parameter_number(values, "transf_scale", 1.0)?,
            z_fold_scale: parameter_number(values, "transf_scale_1", 1.0)?,
            grid_z_enabled: parameter_bool(values, "transf_function_enabledA_false", false)?,
            squared_grid_enabled: parameter_bool(values, "transf_function_enabledAx_false", false)?,
            box_fold_enabled: parameter_bool(values, "transf_function_enabledBx_false", false)?,
            fold_z_enabled: parameter_bool(values, "transf_function_enabledBy", true)?,
            tglad_fold_enabled: parameter_bool(values, "transf_function_enabledBy_false", false)?,
            negate_z: parameter_bool(values, "transf_function_enabledN_false", false)?,
            prism_enabled: parameter_bool(values, "transf_function_enabledP_false", false)?,
            rotation_enabled: parameter_bool(values, "transf_function_enabledR_false", false)?,
            negate_w: parameter_bool(values, "transf_function_enabledw_false", false)?,
            sphere_inversion_enabled: parameter_bool(
                values,
                "transf_sphere_inversion_enabled_false",
                false,
            )?,
            start_a: parameter_u32(values, "transf_start_iterations_A", 0)?,
            start_c: parameter_u32(values, "transf_start_iterations_C", 0)?,
            start_e: parameter_u32(values, "transf_start_iterations_E", 0)?,
            start_p: parameter_u32(values, "transf_start_iterations_P", 0)?,
            start_r: parameter_u32(values, "transf_start_iterations_R", 0)?,
            start_t: parameter_u32(values, "transf_start_iterations_T", 0)?,
            start_x: parameter_u32(values, "transf_start_iterations_X", 0)?,
            stop_one: parameter_u32(values, "transf_stop_iterations_1", 1)?,
            stop_a: parameter_u32(values, "transf_stop_iterations_A", 250)?,
            stop_c: parameter_u32(values, "transf_stop_iterations_C", 250)?,
            stop_e: parameter_u32(values, "transf_stop_iterations_E", 250)?,
            stop_p: parameter_u32(values, "transf_stop_iterations_P1", 1)?,
            stop_r: parameter_u32(values, "transf_stop_iterations_R", 250)?,
            stop_t: parameter_u32(values, "transf_stop_iterations_T", 250)?,
        },
    )))
}

fn jos_kleinian_from_parameters(
    values: &BTreeMap<String, String>,
) -> Result<(orbit::FormulaKernel, orbit::FinalizerParameters)> {
    let vector4 = |name: &str, fallback: [f64; 3], w: f64| -> Result<[f64; 4]> {
        let value = parameter_vec3(values, name, fallback)?;
        Ok([value[0], value[1], value[2], w])
    };
    let color_factors = parameter_vec4(values, "fold_color_difs_0000", [0.0; 4])?;
    let folding_value = parameter_number(values, "transf_folding_value", 2.0)?;
    let tweak005 = parameter_number(values, "analyticDE_tweak_005", 0.05)?;
    let offset1 = parameter_number(values, "analyticDE_offset_1", 1.0)?;
    let spheres_enabled = parameter_bool(values, "transf_spheres_enabled", true)?;
    let kernel = orbit::FormulaKernel::JosKleinian(Box::new(orbit::JosKleinian {
        analytic_scale: parameter_number(values, "analyticDE_scale_1", 1.0)?,
        color_enabled: parameter_bool(values, "fold_color_aux_color_enabled", true)?,
        color_grid_enabled: parameter_bool(values, "fold_color_aux_color_enabledA_false", false)?,
        color_add_enabled: parameter_bool(values, "fold_color_aux_color_enabled_false", false)?,
        color_factors,
        color_mix: parameter_number(values, "fold_color_difs1", 1.0)?,
        color_start: parameter_u32(values, "fold_color_start_iterations_A", 0)?,
        color_stop: parameter_u32(values, "fold_color_stop_iterations_A", 250)?,
        addition: vector4("transf_addition_constant", [0.0; 3], 0.0)?,
        addition_p: vector4("transf_addition_constantP_000", [0.0; 3], 0.0)?,
        constant_c: vector4("transf_constant_multiplierC_111", [1.0; 3], 1.0)?,
        offset_zero: vector4("transf_offset_000", [0.0; 3], 0.0)?,
        box_size: vector4("transf_offset_111", [1.0; 3], 0.0)?,
        scale_three: vector4("transf_scale3D_222", [2.0; 3], 1.0)?,
        folding_value,
        max_radius_squared: parameter_number(values, "transf_maxR2_1", 1.0)?,
        offset: parameter_number(values, "transf_offset", 0.0)?,
        grid_y_enabled: parameter_bool(values, "transf_function_enabledA_false", false)?,
        sphere_inversion_enabled: parameter_bool(
            values,
            "transf_sphere_inversion_enabled_false",
            false,
        )?,
        start_c: parameter_u32(values, "transf_start_iterations_C", 0)?,
        start_t: parameter_u32(values, "transf_start_iterations_T", 0)?,
        stop_c: parameter_u32(values, "transf_stop_iterations_C", 250)?,
        stop_t: parameter_u32(values, "transf_stop_iterations_T", 250)?,
    }));
    Ok((
        kernel,
        orbit::FinalizerParameters {
            spheres_enabled,
            folding_value,
            tweak005,
            offset1,
        },
    ))
}

fn mandelbox_kernel_from_parameters(
    formula_id: u32,
    values: &BTreeMap<String, String>,
) -> Result<orbit::FormulaKernel> {
    let scale = parameter_number(values, "mandelbox_scale", 2.0)?;
    let fixed_radius_squared =
        parameter_number(values, "mandelbox_folding_fixed_radius", 1.0)?.powi(2);
    let minimum_radius_squared =
        parameter_number(values, "mandelbox_folding_min_radius", 0.5)?.powi(2);
    let minimum_radius_factor = fixed_radius_squared / minimum_radius_squared;
    let main_rotation_enabled = parameter_bool(values, "mandelbox_main_rotation_enabled", false)?;
    let main_rotation = rotation2_matrix(
        parameter_vec3(values, "mandelbox_rotation_main", [0.0; 3])?.map(f64::to_radians),
    );
    if formula_id == 9 {
        return Ok(orbit::FormulaKernel::MandelboxFast {
            scale,
            fixed_radius_squared,
            minimum_radius_squared,
            minimum_radius_factor,
            main_rotation_enabled,
            main_rotation,
        });
    }
    ensure!(
        formula_id == 8,
        "unsupported Mandelbox formula {formula_id}"
    );
    let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let mut rotations = [[identity; 3]; 2];
    let mut inverse_rotations = [[identity; 3]; 2];
    for (fold, name) in ["neg", "pos"].into_iter().enumerate() {
        for axis in 0..3 {
            rotations[fold][axis] = rotation2_matrix(
                parameter_vec3(
                    values,
                    &format!("mandelbox_rotation_{name}_{}", axis + 1),
                    [0.0; 3],
                )?
                .map(f64::to_radians),
            );
            inverse_rotations[fold][axis] = transpose_matrix(rotations[fold][axis]);
        }
    }
    let offset = parameter_vec3(values, "mandelbox_offset", [0.0; 3])?;
    Ok(orbit::FormulaKernel::Mandelbox(Box::new(
        orbit::Mandelbox {
            scale,
            folding_limit: parameter_number(values, "mandelbox_folding_limit", 1.0)?,
            folding_value: parameter_number(values, "mandelbox_folding_value", 2.0)?,
            fixed_radius_squared,
            minimum_radius_squared,
            minimum_radius_factor,
            offset: [offset[0], offset[1], offset[2], 0.0],
            color_factor: parameter_vec3(values, "mandelbox_color", [0.03, 0.05, 0.07])?,
            color_sphere_min: parameter_number(values, "mandelbox_color_Sp1", 0.2)?,
            color_sphere_fixed: parameter_number(values, "mandelbox_color_Sp2", 0.2)?,
            rotations_enabled: parameter_bool(values, "mandelbox_rotations_enabled", false)?,
            main_rotation_enabled,
            main_rotation,
            rotations,
            inverse_rotations,
        },
    )))
}

fn kaleidoscopic_ifs_from_parameters(
    values: &BTreeMap<String, String>,
) -> Result<orbit::KaleidoscopicIfs> {
    let absolute = [
        parameter_bool(values, "IFS_abs_x", false)?,
        parameter_bool(values, "IFS_abs_y", false)?,
        parameter_bool(values, "IFS_abs_z", false)?,
    ];
    let mut enabled = [false; IFS_VECTOR_COUNT];
    let mut directions = [[1.0, 0.0, 0.0]; IFS_VECTOR_COUNT];
    let mut rotations = [[[0.0; 3]; 3]; IFS_VECTOR_COUNT];
    let mut distances = [0.0; IFS_VECTOR_COUNT];
    let mut intensities = [1.0; IFS_VECTOR_COUNT];
    for index in 0..IFS_VECTOR_COUNT {
        enabled[index] = parameter_bool(values, &format!("IFS_enabled_{index}"), false)?;
        directions[index] = normalize(parameter_vec3(
            values,
            &format!("IFS_direction_{index}"),
            [1.0, 0.0, 0.0],
        )?)?;
        rotations[index] = rotation3_matrix(
            parameter_vec3(values, &format!("IFS_rotations_{index}"), [0.0; 3])?
                .map(f64::to_radians),
        );
        distances[index] = parameter_number(values, &format!("IFS_distance_{index}"), 0.0)?;
        intensities[index] = parameter_number(values, &format!("IFS_intensity_{index}"), 1.0)?;
    }
    let scale = parameter_number(values, "IFS_scale", 2.0)?;
    ensure!(scale != 0.0, "IFS_scale must be non-zero");
    Ok(orbit::KaleidoscopicIfs {
        absolute,
        enabled,
        directions,
        rotations,
        distances,
        intensities,
        main_rotation: rotation3_matrix(
            parameter_vec3(values, "IFS_rotation", [0.0; 3])?.map(f64::to_radians),
        ),
        rotation_enabled: parameter_bool(values, "IFS_rotation_enabled", false)?,
        offset: parameter_vec3(values, "IFS_offset", [1.0, 0.0, 0.0])?,
        scale,
        edge: parameter_vec3(values, "IFS_edge", [0.0; 3])?,
        edge_enabled: parameter_bool(values, "IFS_edge_enabled", false)?,
        menger_sponge_mode: parameter_bool(values, "IFS_menger_sponge_mode", false)?,
    })
}

fn parameter_number(values: &BTreeMap<String, String>, name: &str, fallback: f64) -> Result<f64> {
    let Some(value) = values.get(name) else {
        return Ok(fallback);
    };
    let parsed = value
        .replace(',', ".")
        .parse::<f64>()
        .with_context(|| format!("invalid {name} value {value}"))?;
    ensure!(parsed.is_finite(), "{name} must be finite");
    Ok(parsed)
}

fn parameter_bool(values: &BTreeMap<String, String>, name: &str, fallback: bool) -> Result<bool> {
    match values.get(name).map(String::as_str) {
        None => Ok(fallback),
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(value) => bail!("invalid {name} boolean {value}"),
    }
}

fn parameter_u32(values: &BTreeMap<String, String>, name: &str, fallback: u32) -> Result<u32> {
    let Some(value) = values.get(name) else {
        return Ok(fallback);
    };
    value
        .parse::<u32>()
        .with_context(|| format!("invalid {name} integer {value}"))
}

fn parameter_vec4(
    values: &BTreeMap<String, String>,
    name: &str,
    fallback: [f64; 4],
) -> Result<[f64; 4]> {
    let Some(value) = values.get(name) else {
        return Ok(fallback);
    };
    let components = value
        .replace(',', ".")
        .split_whitespace()
        .map(str::parse::<f64>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("invalid {name} vector {value}"))?;
    ensure!(
        components.len() == 4 && components.iter().all(|component| component.is_finite()),
        "{name} must contain four finite components"
    );
    Ok([components[0], components[1], components[2], components[3]])
}

fn parameter_vec3(
    values: &BTreeMap<String, String>,
    name: &str,
    fallback: [f64; 3],
) -> Result<[f64; 3]> {
    let Some(value) = values.get(name) else {
        return Ok(fallback);
    };
    let components = value
        .replace(',', ".")
        .split_whitespace()
        .map(str::parse::<f64>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("invalid {name} vector {value}"))?;
    ensure!(
        components.len() == 3 && components.iter().all(|component| component.is_finite()),
        "{name} must contain three finite components"
    );
    Ok([components[0], components[1], components[2]])
}

fn bool_mask(values: &[bool]) -> u32 {
    values.iter().enumerate().fold(0, |mask, (index, enabled)| {
        mask | (u32::from(*enabled) << index)
    })
}

fn map_mandel_point(point: [f64; 3]) -> [f64; 3] {
    [point[0], point[2], point[1]]
}

fn map_mandel_vector(vector: [f64; 3]) -> [f64; 3] {
    [vector[0], vector[2], vector[1]]
}

fn camera_roll(direction: [f64; 3], desired_up: [f64; 3], yaw_pitch: [f32; 2]) -> f64 {
    let forward = normalize(direction).expect("camera direction is non-zero");
    let projected_up = subtract(desired_up, scale(forward, dot(desired_up, forward)));
    let projected_up = normalize(projected_up).unwrap_or([0.0, 1.0, 0.0]);
    let (yaw, pitch) = (f64::from(yaw_pitch[0]), f64::from(yaw_pitch[1]));
    let right = [yaw.cos(), 0.0, -yaw.sin()];
    let unrolled_up = [
        -yaw.sin() * pitch.sin(),
        pitch.cos(),
        -yaw.cos() * pitch.sin(),
    ];
    (-dot(projected_up, right)).atan2(dot(projected_up, unrolled_up))
}

fn normalize(value: [f64; 3]) -> Result<[f64; 3]> {
    let magnitude = length(value);
    ensure!(
        magnitude > 0.0 && magnitude.is_finite(),
        "vector must be finite and non-zero"
    );
    Ok(scale(value, magnitude.recip()))
}

fn length(value: [f64; 3]) -> f64 {
    dot(value, value).sqrt()
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn rotate_around_axis(value: [f64; 3], axis: [f64; 3], angle: f64) -> [f64; 3] {
    let (sine, cosine) = angle.sin_cos();
    let cross = cross(axis, value);
    let along = scale(axis, dot(axis, value) * (1.0 - cosine));
    [
        value[0] * cosine + cross[0] * sine + along[0],
        value[1] * cosine + cross[1] * sine + along[1],
        value[2] * cosine + cross[2] * sine + along[2],
    ]
}

fn subtract(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn scale(value: [f64; 3], factor: f64) -> [f64; 3] {
    [value[0] * factor, value[1] * factor, value[2] * factor]
}

fn rotation3_matrix(rotation: [f64; 3]) -> [[f64; 3]; 3] {
    let (sz, cz) = rotation[0].sin_cos();
    let (sy, cy) = rotation[1].sin_cos();
    let (sx, cx) = rotation[2].sin_cos();
    [
        [cz * cy, cz * sy * sx - sz * cx, cz * sy * cx + sz * sx],
        [sz * cy, sz * sy * sx + cz * cx, sz * sy * cx - cz * sx],
        [-sy, cy * sx, cy * cx],
    ]
}

fn rotation2_matrix(rotation: [f64; 3]) -> [[f64; 3]; 3] {
    rotation3_matrix([rotation[2], rotation[1], rotation[0]])
}

fn transpose_matrix(matrix: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    [
        [matrix[0][0], matrix[1][0], matrix[2][0]],
        [matrix[0][1], matrix[1][1], matrix[2][1]],
        [matrix[0][2], matrix[1][2], matrix[2][2]],
    ]
}

#[cfg(test)]
fn matrix_vector(matrix: [[f64; 3]; 3], vector: [f64; 3]) -> [f64; 3] {
    [
        dot(matrix[0], vector),
        dot(matrix[1], vector),
        dot(matrix[2], vector),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const IFS_SCENE: &str = r#"
# Mandelbulber settings file
# version 2.33
# only modified parameters
[main_parameters]
camera 0,811466000478122 -0,408023831291236 -1,37676147184911;
camera_top 0,665331264161861 0,700434635203032 -0,258313047941398;
detail_level 2;
formula_1 10;
fov 61,93;
image_height 1080;
image_width 1920;
target 0,810487393068948 -0,405416779822347 -1,3722128396787;
[fractal_1]
IFS_abs_z true;
IFS_direction_2 -0,56 0,61 0,56;
IFS_direction_3 -0,54 0,65 0,54;
IFS_direction_4 0,71 0 -0,71;
IFS_enabled_2 true;
IFS_enabled_3 true;
IFS_enabled_4 true;
IFS_offset 1 1 1;
IFS_rotation -2 8 6;
IFS_rotation_enabled true;
IFS_scale 1,4;
"#;

    #[test]
    fn parses_decimal_commas_and_ifs_parameters() {
        let scene = MandelbulberScene::parse(IFS_SCENE).expect("parse IFS scene");
        assert_eq!(scene.width, 1920);
        assert_eq!(scene.formula_id, 10);
        assert_eq!(scene.height, 1080);
        assert_eq!(scene.max_iterations, 250);
        assert_eq!(scene.ifs_scale, 1.4);
        assert_eq!(scene.ifs_offset, [1.0; 3]);
        assert_eq!(scene.ifs_abs, [false, false, true]);
        assert_eq!(
            scene.ifs_enabled,
            [false, false, true, true, true, false, false, false, false]
        );
    }

    #[test]
    fn parser_preserves_mandelbulber_palette_controls_and_closure() {
        let source = IFS_SCENE.replace(
            "detail_level 2;",
            "detail_level 2;\nmat1_coloring_palette_offset 0,125;\nmat1_coloring_speed 2,5;\nmat1_fractal_coloring_extra_color_enabled_false true;\nmat1_surface_color_gradient 0 ff0000 5000 00ff00;",
        );
        let scene = MandelbulberScene::parse(&source).expect("palette scene");
        assert_eq!(scene.material.palette_offset, 0.125);
        assert_eq!(scene.material.coloring_speed, 2.5);
        assert_eq!(scene.material.surface_gradient.len(), 3);
        assert_eq!(scene.material.surface_gradient[0].position, 0.0);
        assert_eq!(scene.material.surface_gradient[1].position, 0.5);
        assert_eq!(scene.material.surface_gradient[2].position, 1.0);
        assert_eq!(
            scene.material.surface_gradient[0].color,
            scene.material.surface_gradient[2].color
        );
        assert_eq!(
            scene
                .material
                .parameters
                .get("mat1_fractal_coloring_extra_color_enabled_false")
                .map(String::as_str),
            Some("true")
        );
    }

    #[test]
    fn parser_migrates_pre_219_surface_color_palette() {
        let source = IFS_SCENE
            .replace("# version 2.33", "# version 2.13")
            .replace(
                "detail_level 2;",
                "detail_level 2;\nmat1_coloring_palette_offset 9;\nmat1_coloring_speed 3;\nmat1_surface_color_palette ff0000 00ff00 0000ff;",
            );
        let scene = MandelbulberScene::parse(&source).expect("legacy palette scene");
        let stops = &scene.material.surface_gradient;
        assert_eq!(stops.len(), 4);
        assert_eq!(stops[0].position, 0.0);
        assert_eq!(stops[1].position, 0.3333);
        assert_eq!(stops[2].position, 0.6666);
        assert_eq!(stops[3].position, 1.0);
        assert_eq!(stops[0].color, [255.0 / 256.0, 0.0, 0.0]);
        assert_eq!(stops[1].color, [0.0, 255.0 / 256.0, 0.0]);
        assert_eq!(stops[2].color, [0.0, 0.0, 255.0 / 256.0]);
        assert_eq!(stops[3].color, stops[0].color);
        assert_eq!(scene.material.palette_offset, 3.0);
        assert_eq!(scene.material.coloring_speed, 10.0);
    }

    #[test]
    fn parser_migrates_pre_219_default_gradient_palette_coordinates() {
        let source = IFS_SCENE
            .replace("# version 2.33", "# version 2.14")
            .replace(
                "detail_level 2;",
                "detail_level 2;\nmat1_coloring_palette_offset 174,56;\nmat1_coloring_speed 0,1;",
            );
        let scene = MandelbulberScene::parse(&source).expect("legacy default-gradient scene");
        assert_eq!(scene.material.surface_gradient.len(), 11);
        assert!((scene.material.palette_offset - 17.456).abs() < 1.0e-12);
        assert!((scene.material.coloring_speed - 0.1).abs() < 1.0e-12);
    }

    #[test]
    fn config_uses_neutral_appearance_for_geometry_comparison() {
        let source = IFS_SCENE.replace(
            "detail_level 2;",
            "detail_level 2;\nbackground_3_colors_enable false;\nbackground_color_1 2800 6200 aa00;\nbackground_brightness 0,8;\nbackground_gamma 1,25;\nbrightness 0,9;\ncontrast 1,1;\ngamma 0,7;\nsaturation 0,75;",
        );
        let scene = MandelbulberScene::parse(&source).expect("appearance scene");
        let mut config = FptRenderConfig::default();
        scene.apply_to_config(&mut config);

        assert_eq!(config.mandel_appearance_mode, 1);
        assert_eq!(config.world[6], 3.0);
        assert_eq!(config.world[1], 1.0);
        assert_eq!(config.world[5], 1.0);
        assert_eq!(config.background_gradient[..3], [0.0, 0.0, 0.0]);
        assert_eq!(config.post, [-1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0]);
    }

    #[test]
    fn config_maps_legacy_mandelbulber_directional_light() {
        let source = IFS_SCENE
            .replace("# version 2.33", "# version 2.13")
            .replace(
                "detail_level 2;",
                "detail_level 2;\nmain_light_alpha -35;\nmain_light_beta -25;\nmain_light_colour ff00 8000 4000;\nmain_light_intensity 0,7;\nshadows_cone_angle 2;",
            );
        let scene = MandelbulberScene::parse(&source).expect("legacy light scene");
        let direction = scene.main_light_direction();
        let mut config = FptRenderConfig::default();
        scene.apply_to_config(&mut config);

        assert_eq!(config.sun[0], 1.0);
        assert!((config.sun[3] - 0.7).abs() < 1.0e-6);
        assert!((config.sun[4] - 2.0_f32.to_radians()).abs() < 1.0e-6);
        assert_eq!(
            config.sun_color,
            [
                0xff00 as f32 / 65_535.0,
                0x8000 as f32 / 65_535.0,
                0x4000 as f32 / 65_535.0
            ]
        );

        let yaw = f64::from(config.sun[1]).to_radians();
        let pitch = f64::from(config.sun[2]).to_radians();
        let reconstructed = [
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            yaw.cos() * pitch.cos(),
        ];
        assert!(dot(direction, reconstructed) > 1.0 - 1.0e-12);
    }

    #[test]
    fn parser_uses_pre_214_material_defaults() {
        let source = IFS_SCENE.replace("# version 2.33", "# version 2.13");
        let scene = MandelbulberScene::parse(&source).expect("legacy material scene");
        assert_eq!(scene.material.shading, 1.0);
        assert_eq!(scene.material.specular, 1.0);
    }

    #[test]
    fn parser_migrates_pre_221_projection_fov_to_degrees() {
        let legacy_perspective = IFS_SCENE
            .replace("# version 2.33", "# version 2.20")
            .replace("fov 61,93;", "fov 0,5;");
        let scene = MandelbulberScene::parse(&legacy_perspective).expect("legacy perspective");
        assert!((scene.fov_degrees - 28.072_486_935_853).abs() < 1.0e-12);

        let legacy_fisheye = legacy_perspective.replace(
            "detail_level 2;",
            "detail_level 2;\nperspective_type fish_eye;",
        );
        let scene = MandelbulberScene::parse(&legacy_fisheye).expect("legacy fisheye");
        assert!((scene.fov_degrees - 90.0).abs() < 1.0e-12);

        let legacy_equirectangular = legacy_perspective.replace(
            "detail_level 2;",
            "detail_level 2;\nperspective_type equirectangular;",
        );
        let scene =
            MandelbulberScene::parse(&legacy_equirectangular).expect("legacy equirectangular");
        assert!((scene.fov_degrees - 180.0).abs() < 1.0e-12);
    }

    #[test]
    fn parser_defers_formula_compatibility_to_the_generated_runtime() {
        let source = IFS_SCENE.replace("formula_1 10;", "formula_1 7;");
        let scene = MandelbulberScene::parse(&source).expect("parse formula 7 metadata");
        assert_eq!(scene.formula_id, 7);
        assert!(!scene.has_cpu_reference());
    }

    #[test]
    fn parser_prefers_current_delta_de_method_over_legacy_analytic_flag() {
        let forced_delta = IFS_SCENE.replace(
            "[main_parameters]",
            "[main_parameters]\ndelta_DE_method 1;\nanalityc_DE_mode true;",
        );
        assert!(
            MandelbulberScene::parse(&forced_delta)
                .unwrap()
                .force_delta_de
        );
        assert!(
            !MandelbulberScene::parse(&forced_delta)
                .unwrap()
                .force_analytic_de
        );

        let forced_analytic = IFS_SCENE.replace(
            "[main_parameters]",
            "[main_parameters]\ndelta_DE_method 2;\nanalityc_DE_mode false;",
        );
        assert!(
            !MandelbulberScene::parse(&forced_analytic)
                .unwrap()
                .force_delta_de
        );
        assert!(
            MandelbulberScene::parse(&forced_analytic)
                .unwrap()
                .force_analytic_de
        );

        let legacy_delta = IFS_SCENE.replace(
            "[main_parameters]",
            "[main_parameters]\nanalityc_DE_mode false;",
        );
        assert!(
            MandelbulberScene::parse(&legacy_delta)
                .unwrap()
                .force_delta_de
        );

        let preferred = MandelbulberScene::parse(IFS_SCENE).unwrap();
        assert!(!preferred.force_delta_de);
        assert!(!preferred.force_analytic_de);

        let logarithmic = IFS_SCENE.replace(
            "[main_parameters]",
            "[main_parameters]\ndelta_DE_function 2;",
        );
        assert_eq!(
            MandelbulberScene::parse(&logarithmic)
                .unwrap()
                .delta_de_function,
            2
        );
    }

    #[test]
    fn hybrid_parser_builds_mandelbulbers_slot_sequence() {
        let source = r#"
# Mandelbulber settings file
# version 2.33
[main_parameters]
N 6;
camera 3 -6 2;
formula_1 4;
formula_2 5;
formula_iterations_1 2;
formula_iterations_2 1;
hybrid_fractal_enable true;
target 0 0 0;
[fractal_1]
[fractal_2]
"#;
        let scene = MandelbulberScene::parse(source).expect("hybrid scene");
        assert!(scene.hybrid_enabled);
        assert_eq!(scene.formula_slots[1].formula_id, 5);
        assert_eq!(
            scene.hybrid_sequence().expect("sequence"),
            [0, 0, 1, 0, 0, 1]
        );
    }

    #[test]
    fn power2_uses_shared_orbit_runtime() {
        let source = IFS_SCENE.replace("formula_1 10;", "formula_1 3;");
        let scene = MandelbulberScene::parse(&source).expect("parse Power 2 scene");
        assert_eq!(scene.formula_id, 3);
        assert_eq!(scene.bailout, 10.0);
        let sample = scene.distance([-0.7, 0.1, 0.2]);
        assert!(sample.distance.is_finite());
        assert!(sample.derivative.is_finite());
        let mut config = FptRenderConfig::default();
        scene.apply_to_config(&mut config);
        assert!(config.fractal_style[6] > 0.0);
        assert_eq!(config.fractal_style_mode, 1);
    }

    #[test]
    fn cpu_reference_produces_finite_samples() {
        let scene = MandelbulberScene::parse(IFS_SCENE).expect("parse IFS scene");
        for point in [[0.0; 3], [0.2, -0.3, 0.4], [4.0, 3.0, -2.0]] {
            let sample = scene.distance(point);
            assert!(sample.distance.is_finite());
            assert!(sample.radius.is_finite());
            assert!(sample.derivative.is_finite() && sample.derivative > 0.0);
            assert!((1..=scene.max_iterations).contains(&sample.iterations));
        }
    }

    #[test]
    fn bundled_reference_scene_imports() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("scenes/mandelbulber/ifs-20.fract");
        let scene = MandelbulberScene::load(&path).expect("load bundled IFS scene");
        assert_eq!(scene.source_version, "2.33");
        assert_eq!(scene.ifs_scale, 1.4);
    }

    #[test]
    fn config_preserves_target_and_camera_roll() {
        let scene = MandelbulberScene::parse(IFS_SCENE).expect("parse IFS scene");
        let mut config = FptRenderConfig::default();
        scene.apply_to_config(&mut config);
        assert_eq!(config.sdf_id, SDF_MANDELBULBER);
        assert!(config.focus_distance > 5.0);
        assert!(config.camera_roll.abs() > 1.0);
        assert_eq!(config.camera_image_y_sign, 1.0);
        assert_eq!(config.set_values[PARAM_IFS_SCALE], 1.4);
        assert_eq!(config.fractal_style_mode, 2);
        assert_eq!(config.render[1], 10_000.0);
    }

    #[test]
    fn config_preserves_legacy_image_coordinates() {
        let source = IFS_SCENE.replace(
            "[main_parameters]",
            "[main_parameters]\nlegacy_coordinate_system true;",
        );
        let scene = MandelbulberScene::parse(&source).expect("parse legacy camera scene");
        assert!(scene.legacy_coordinate_system);
        let mut config = FptRenderConfig::default();
        scene.apply_to_config(&mut config);
        assert_eq!(config.camera_image_y_sign, -1.0);
    }

    #[test]
    fn global_folding_controls_do_not_overlap_legacy_ifs_directions() {
        let source = IFS_SCENE
            .replace(
                "[main_parameters]",
                "[main_parameters]\nbox_folding true;\nbox_folding_limit 3;\nbox_folding_value 6;\nspherical_folding true;\nspherical_folding_outer 2;\nspherical_folding_inner 0,5;",
            )
            .replace("[fractal_1]", "[fractal_1]\nIFS_direction_0 1 2 3;");
        let scene = MandelbulberScene::parse(&source).expect("folded IFS scene");
        let mut config = FptRenderConfig::default();
        scene.apply_to_config(&mut config);

        assert_eq!(config.vset_values[VPARAM_GLOBAL_BOX_FOLD], 1.0);
        assert_eq!(config.vset_values[VPARAM_GLOBAL_BOX_LIMIT], 3.0);
        assert_eq!(config.vset_values[VPARAM_GLOBAL_BOX_VALUE], 6.0);
        assert_eq!(config.vset_values[VPARAM_GLOBAL_SPHERICAL_FOLD], 1.0);
        assert_eq!(config.vset_values[VPARAM_GLOBAL_SPHERICAL_OUTER], 2.0);
        assert_eq!(config.vset_values[VPARAM_GLOBAL_SPHERICAL_INNER], 0.5);
        assert_ne!(config.set_values[PARAM_DIRECTION_BASE], 1.0);
    }

    #[test]
    fn world_scale_preserves_large_source_coordinate_mantissas() {
        let source = IFS_SCENE
            .replace(
                "camera 0,811466000478122 -0,408023831291236 -1,37676147184911;",
                "camera 301.6904743885267 -438.2784468930019 -193.1132163166291;",
            )
            .replace(
                "target 0,810487393068948 -0,405416779822347 -1,3722128396787;",
                "target 301.6903192372978 -438.2803842413788 -193.112744501663;",
            );
        let scene = MandelbulberScene::parse(&source).expect("large-coordinate scene");
        let mut config = FptRenderConfig::default();
        scene.apply_to_config(&mut config);
        let recovered = config
            .camera_position
            .map(|coordinate| coordinate / WORLD_SCALE as f32);
        assert_eq!(recovered[0], scene.camera[0] as f32);
        assert_eq!(recovered[1], scene.camera[2] as f32);
        assert_eq!(recovered[2], scene.camera[1] as f32);
    }

    #[test]
    fn periodic_jos_camera_is_rebased_without_changing_view_direction() {
        let source = IFS_SCENE
            .replace("formula_1 10;", "formula_1 122;")
            .replace(
                "camera 0,811466000478122 -0,408023831291236 -1,37676147184911;",
                "camera 301.6904743885267 -438.2784468930019 -193.1132163166291;",
            )
            .replace(
                "target 0,810487393068948 -0,405416779822347 -1,3722128396787;",
                "target 301.6903192372978 -438.2803842413788 -193.112744501663;",
            )
            .replace(
                "[fractal_1]",
                "[fractal_1]\ntransf_folding_value 1,93;\ntransf_offset_111 1 1 0,6;",
            );
        let scene = MandelbulberScene::parse(&source).expect("large-coordinate Jos scene");
        let (camera, target) = scene
            .periodic_camera_rebase()
            .expect("periodic Jos field is rebasable");
        assert!(camera[0].abs() <= 1.0);
        assert!(camera[1].abs() <= 0.965);
        assert!(camera[2].abs() <= 0.6);
        for axis in 0..3 {
            let original_direction = scene.target[axis] - scene.camera[axis];
            let rebased_direction = target[axis] - camera[axis];
            assert!((rebased_direction - original_direction).abs() < 1.0e-12);
        }
    }

    #[test]
    fn config_preserves_mandelbulber_threshold_policy() {
        let source = IFS_SCENE.replace(
            "detail_level 2;",
            "detail_level 2;\nconstant_DE_threshold true;\nDE_thresh 0,00015;\nDE_factor 0,326;\nsmoothness 0,1;\nadvanced_quality true;\ndeltade_relative_delta 0,004;",
        );
        let scene = MandelbulberScene::parse(&source).expect("parse threshold policy");
        let mut config = FptRenderConfig::default();
        scene.apply_to_config(&mut config);
        assert_eq!(config.vset_values[VPARAM_DYNAMIC_THRESHOLD], 0.0);
        assert!((config.vset_values[VPARAM_CONSTANT_THRESHOLD] - 0.1536).abs() < 1.0e-6);
        assert!((config.vset_values[VPARAM_DE_FACTOR] - 0.326).abs() < 1.0e-6);
        assert!((config.vset_values[VPARAM_SMOOTHNESS] - 0.1).abs() < 1.0e-6);
        assert_eq!(config.vset_values[VPARAM_ADVANCED_QUALITY], 1.0);
        assert!((scene.delta_de_relative_delta - 0.004).abs() < 1.0e-12);
        assert!((scene.mesh_delta_relative_delta() - 0.004).abs() < 1.0e-7);

        let default_scene = MandelbulberScene::parse(IFS_SCENE).expect("default mesh policy");
        assert_eq!(
            default_scene.mesh_delta_relative_delta(),
            MESH_DELTA_RELATIVE_DEFAULT
        );
    }

    #[test]
    fn config_distinguishes_iteration_threshold_mode() {
        let source = IFS_SCENE.replace(
            "detail_level 2;",
            "detail_level 2;\niteration_threshold_mode true;",
        );
        let scene = MandelbulberScene::parse(&source).expect("parse iteration threshold mode");
        let mut config = FptRenderConfig::default();
        scene.apply_to_config(&mut config);
        assert_eq!(
            config.vset_values[VPARAM_DYNAMIC_THRESHOLD],
            VPARAM_ITERATION_THRESHOLD_MODE
        );
    }

    #[test]
    fn config_preserves_equirectangular_projection() {
        let source = IFS_SCENE
            .replace(
                "detail_level 2;",
                "detail_level 2;\nperspective_type equirectangular;",
            )
            .replace("fov 61,93;", "fov 360;");
        let scene = MandelbulberScene::parse(&source).expect("parse equirectangular scene");
        let mut config = FptRenderConfig::default();
        scene.apply_to_config(&mut config);
        assert_eq!(scene.camera_projection, 2);
        assert_eq!(config.vset_values[VPARAM_CAMERA_PROJECTION], 2.0);
        assert_eq!(config.camera_fov, 360.0);
    }

    #[test]
    fn config_preserves_fish_eye_projection() {
        let source = IFS_SCENE
            .replace(
                "detail_level 2;",
                "detail_level 2;\nperspective_type fish_eye;\nslow_shading true;\nfractal_position 1 2 3;\nfractal_rotation 0 -90 45;\nrepeat 4 5 6;",
            )
            .replace("fov 61,93;", "fov 210;");
        let scene = MandelbulberScene::parse(&source).expect("parse fish-eye scene");
        let mut config = FptRenderConfig::default();
        scene.apply_to_config(&mut config);
        assert_eq!(scene.camera_projection, 1);
        assert!(scene.slow_shading);
        assert_eq!(config.vset_values[VPARAM_CAMERA_PROJECTION], 1.0);
        assert_eq!(config.camera_fov, 210.0);
        assert!(config.render[7] > 0.0);
        assert_eq!(
            &config.vset_values[VPARAM_FRACTAL_POSITION..VPARAM_FRACTAL_POSITION + 3],
            &[1.0, 2.0, 3.0]
        );
        assert!(
            (config.vset_values[VPARAM_FRACTAL_ROTATION + 1] + std::f32::consts::FRAC_PI_2).abs()
                < 1.0e-6
        );
        assert_eq!(
            &config.vset_values[VPARAM_FRACTAL_REPEAT..VPARAM_FRACTAL_REPEAT + 3],
            &[4.0, 5.0, 6.0]
        );
    }

    #[test]
    fn parser_uses_mandelbulber_boolean_operator_values() {
        let source = IFS_SCENE.replace(
            "formula_1 10;",
            "boolean_operators true;\nformula_1 10;\nformula_2 7;",
        );
        let scene = MandelbulberScene::parse(&source).expect("parse boolean scene");
        assert_eq!(scene.boolean_operators[0], 1, "OR is Mandelbulber value 1");

        let source = source.replace("formula_2 7;", "formula_2 7;\nboolean_operator_1 0;");
        let scene = MandelbulberScene::parse(&source).expect("parse explicit AND");
        assert_eq!(scene.boolean_operators[0], 0, "AND is Mandelbulber value 0");
    }

    #[test]
    fn config_maps_mandelbulber_primitive_planes_into_world_space() {
        let source = IFS_SCENE.replace(
            "detail_level 2;",
            "detail_level 2;\nprimitive_plane_1_enabled true;\nprimitive_plane_1_material_id 7;\nprimitive_plane_1_position 1 2 3;\nprimitive_plane_1_rotation 0 0 0;",
        );
        let scene = MandelbulberScene::parse(&source).expect("primitive plane scene");
        assert_eq!(scene.primitive_planes.len(), 1);
        assert_eq!(scene.primitive_planes[0].material_id, 7);

        let mut config = FptRenderConfig::default();
        scene.apply_to_config(&mut config);
        assert_eq!(config.sdf_flat_union_count, 1);
        let plane = config.sdf_flat_union_instances[0];
        assert_eq!(plane.opcode, SDF_OP_PLANE);
        assert_eq!(plane.source_instruction, 7);
        assert_eq!(plane.data[..3], [0.0, 1.0, 0.0]);
        assert_eq!(plane.data[3], -3.0 * WORLD_SCALE as f32);
        assert_eq!(plane.distance_scale, 1.0);
    }

    #[test]
    fn config_maps_mandelbulber_empty_sphere_shells_into_world_space() {
        let source = IFS_SCENE.replace(
            "detail_level 2;",
            "detail_level 2;\nprimitive_sphere_1_enabled true;\nprimitive_sphere_1_empty true;\nprimitive_sphere_1_material_id 9;\nprimitive_sphere_1_position 1 2 3;\nprimitive_sphere_1_radius 0.5;\nprimitive_sphere_1_wall_thickness 0.125;",
        );
        let scene = MandelbulberScene::parse(&source).expect("primitive sphere scene");
        assert_eq!(scene.primitive_spheres.len(), 1);

        let mut config = FptRenderConfig::default();
        scene.apply_to_config(&mut config);
        assert_eq!(config.sdf_flat_union_count, 1);
        let sphere = config.sdf_flat_union_instances[0];
        assert_eq!(sphere.opcode, SDF_OP_SPHERE);
        assert_eq!(sphere.source_instruction, 9);
        assert_eq!(sphere._pad0, 1);
        assert_eq!(sphere.data[0], 0.5 * WORLD_SCALE as f32);
        assert_eq!(sphere.data[1], 0.125 * WORLD_SCALE as f32);
        assert_eq!(sphere.transform[3], -WORLD_SCALE as f32);
        assert_eq!(sphere.transform[7], -3.0 * WORLD_SCALE as f32);
        assert_eq!(sphere.transform[11], -2.0 * WORLD_SCALE as f32);
    }

    #[test]
    fn rotation3_matches_identity_and_z_rotation() {
        assert_eq!(
            rotation3_matrix([0.0; 3]),
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        );
        let rotated = matrix_vector(
            rotation3_matrix([90.0_f64.to_radians(), 0.0, 0.0]),
            [1.0, 0.0, 0.0],
        );
        assert!(rotated[0].abs() < 1.0e-12);
        assert!((rotated[1] - 1.0).abs() < 1.0e-12);
    }
}
