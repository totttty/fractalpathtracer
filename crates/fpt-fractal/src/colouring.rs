//! Mandelbulber's materials (`material.cpp`) and how they colour the fractal (`fractal_coloring.cpp`,
//! `shader_surface_color.cpp`): the surface colour, the palette gradient its colour index picks from, and the
//! settings of the colouring orbit `wgsl::emit_coloured` writes.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::scene::FractFile;

/// `fractal_coloring_*`: how the colouring orbit's trap and index are formed (`sFractalColoring`).
#[derive(Clone, Debug, PartialEq)]
pub struct Colouring {
    /// 0 standard, 1 z·point, 2 sphere, 3 cross, 4 line; −1 none.
    pub algorithm: i64,
    pub sphere_radius: f64,
    pub line_direction: [f64; 4],
    pub color_4d: bool,
    pub pre_v215: bool,
    /// The hybrid index's three scales, when `extra_color_options_false` is on.
    pub hybrid_scales: Option<[f64; 3]>,
    /// "Colour by numbers" (`extra_color_enabled_false`).
    pub extra: Option<ExtraColour>,
}

/// `CalculateColorIndex`'s colour-by-numbers terms; `None` fields are off.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtraColour {
    pub initial: f64,
    /// Initial condition: (radius weight when on, xyz weights when on, whether xyz is taken as is (not |xyz|)).
    pub initial_radius: Option<f64>,
    pub initial_xyz: Option<([f64; 3], bool)>,
    pub orbit_trap: Option<f64>,
    /// (aux.color weight, colorHybrid weight).
    pub aux_color: Option<(f64, f64)>,
    /// (weight, divided by 1e13, squared).
    pub radius: Option<(f64, bool, bool)>,
    pub radius_over_de: Option<(f64, bool, bool)>,
    pub xyz: Option<XyzBias>,
    /// Iteration group: (start, add scale when on, multiply scale when on).
    pub iterations: Option<(f64, Option<f64>, Option<f64>)>,
    /// Global palette: add (start, spread, max), parabola (start, scale), cosine (start, period, add), round scale.
    pub add: Option<[f64; 3]>,
    pub parabola: Option<[f64; 2]>,
    pub cosine: Option<[f64; 3]>,
    pub round: Option<f64>,
    pub cos_start: f64,
    pub min: f64,
    pub max: f64,
}

/// The xyz bias term: (weights, iteration scale, taken as is (not |z|), divided by 1e13, squared x/y/z).
pub type XyzBias = ([f64; 3], f64, bool, bool, [bool; 3]);

/// One `mat{N}_` material, Mandelbulber's defaults and compatibility rules applied.
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub id: i64,
    /// Linear RGB, as Mandelbulber uses it (no gamma).
    pub surface_colour: [f64; 3],
    /// Whether the colour comes from the gradient at the colour index (`use_colors_from_palette` and
    /// `surface_gradient_enable`).
    pub palette: bool,
    /// (position 0..1, RGB) stops, sorted, closed with the first colour at 1.
    pub gradient: Vec<(f64, [f64; 3])>,
    pub speed: f64,
    pub offset: f64,
    pub shading: f64,
    pub specular: f64,
    pub specular_width: f64,
    pub roughness: f64,
    pub reflectance: f64,
    pub metallic: bool,
    pub luminosity: f64,
    pub luminosity_colour: [f64; 3],
    pub transparency: f64,
    pub ior: f64,
    pub colouring: Colouring,
}

const DEFAULT_GRADIENT: &str = "0 fd6029 1000 698403 2000 fff59b 3000 f5bd22 4000 0b5e87 5000 c68876 6000 a51c64 7000 3b9fee 8000 d4ffd4 9000 aba53c";

/// Keys that files up to 2.071 stored without the `mat1_` prefix (`cSettings::Compatibility`).
const UNPREFIXED: [(&str, &str); 18] = [
    ("shading", "shading"),
    ("specular", "specular"),
    ("reflectance", "reflect"),
    ("transparency_of_surface", "transparency_of_surface"),
    ("transparency_of_interior", "transparency_of_interior"),
    (
        "transparency_index_of_refraction",
        "transparency_index_of_refraction",
    ),
    ("transparency_interior_color", "transparency_interior_color"),
    ("fresnel_reflectance", "fresnel_reflectance"),
    ("coloring_random_seed", "coloring_random_seed"),
    ("coloring_saturation", "coloring_saturation"),
    ("coloring_speed", "coloring_speed"),
    ("coloring_palette_size", "coloring_palette_size"),
    ("coloring_palette_offset", "coloring_palette_offset"),
    ("use_colors_from_palette", "fractal_color"),
    ("surface_color_palette", "surface_color_palette"),
    ("fractal_coloring_algorithm", "fractal_coloring_algorithm"),
    (
        "fractal_coloring_sphere_radius",
        "fractal_coloring_sphere_radius",
    ),
    (
        "fractal_coloring_line_direction",
        "fractal_coloring_line_direction",
    ),
];

/// Every material the scene defines (from its `mat{N}_` keys), the fractal's (`formula_material_id`) and 1.
#[must_use]
pub fn materials(file: &FractFile) -> BTreeMap<i64, Material> {
    let mut ids: Vec<i64> = file
        .sections
        .get("main_parameters")
        .map(|main| {
            main.keys()
                .filter_map(|key| key.strip_prefix("mat")?.split_once('_')?.0.parse().ok())
                .collect()
        })
        .unwrap_or_default();
    ids.push(1);
    #[allow(clippy::cast_possible_truncation, reason = "a material id")]
    ids.push(file.number("formula_material_id", 1.0).max(1.0) as i64);
    ids.into_iter().map(|id| (id, material(file, id))).collect()
}

/// A 16-bit-per-channel colour (`rrrr gggg bbbb` in hex) as Mandelbulber's floats (`toRGBFloat`: / 65536).
fn rgb16(value: &str) -> Option<[f64; 3]> {
    let parts: Vec<f64> = value
        .split_whitespace()
        .map(|p| u32::from_str_radix(p, 16).map(f64::from))
        .collect::<Result<_, _>>()
        .ok()?;
    (parts.len() == 3).then(|| [parts[0] / 65536.0, parts[1] / 65536.0, parts[2] / 65536.0])
}

/// A gradient's `rrggbb` stop (`InterpolateFloat`: / 256).
fn rgb8(value: &str) -> Option<[f64; 3]> {
    let v = u32::from_str_radix(value.get(..6)?, 16).ok()?;
    Some([
        f64::from(v >> 16) / 256.0,
        f64::from((v >> 8) & 255) / 256.0,
        f64::from(v & 255) / 256.0,
    ])
}

/// `cColorGradient::SetColorsFromString`: `position colour` pairs (positions in ten-thousandths), sorted, closed
/// with the first colour at 1.
fn gradient(value: &str) -> Vec<(f64, [f64; 3])> {
    let tokens: Vec<&str> = value.split_whitespace().collect();
    let mut stops: Vec<(f64, [f64; 3])> = tokens
        .as_chunks::<2>()
        .0
        .iter()
        .filter_map(|[position, colour]| {
            Some((position.parse::<f64>().ok()? / 10000.0, rgb8(colour)?))
        })
        .collect();
    if let Some(&(_, first)) = stops.first() {
        stops.push((1.0, first));
    }
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    stops
}

/// A pre-2.19 palette (`rrggbb` colours) as the gradient Compatibility makes of it, and its colour count.
fn legacy_palette(value: &str) -> (String, usize) {
    let colours: Vec<&str> = value.split(' ').collect();
    let mut count = colours.len();
    if colours.last().is_some_and(|c| c.len() < 6) {
        count -= 1;
    }
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "palette positions"
    )]
    let text = (0..count)
        .map(|i| {
            format!(
                "{} {}",
                (i as f64 * (1.0 / count as f64) * 10000.0) as i64,
                colours[i]
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    (text, count)
}

#[must_use]
pub fn material(file: &FractFile, id: i64) -> Material {
    let version = file.version();
    let get = |name: &str| -> Option<String> {
        if let Some(v) = file.get("main_parameters", &format!("mat{id}_{name}")) {
            return Some(v.to_owned());
        }
        // Files up to 2.071 kept material 1's settings unprefixed.
        let old = UNPREFIXED.iter().find(|(new, _)| *new == name)?.1;
        (id == 1 && version <= 2.071)
            .then(|| file.get("main_parameters", old).map(str::to_owned))
            .flatten()
    };
    let number = |name: &str, fallback: f64| {
        get(name)
            .and_then(|v| v.replace(',', ".").parse().ok())
            .unwrap_or(fallback)
    };
    let boolean =
        |name: &str, fallback: bool| get(name).map_or(fallback, |v| v == "true" || v == "1");
    let colour =
        |name: &str, fallback: [f64; 3]| get(name).and_then(|v| rgb16(&v)).unwrap_or(fallback);
    let vector4 = |name: &str, fallback: [f64; 4]| {
        get(name)
            .map(|v| {
                v.split_whitespace()
                    .filter_map(|p| p.replace(',', ".").parse().ok())
                    .collect::<Vec<f64>>()
            })
            .and_then(|v| <[f64; 4]>::try_from(v.as_slice()).ok())
            .unwrap_or(fallback)
    };
    // Before 2.19 palettes were colour lists; Compatibility made gradients of them and rescaled speed and offset
    // to the palette size, for every defined material.
    let (gradient_text, palette_size) = match (
        version < 2.19,
        get("surface_color_gradient"),
        get("surface_color_palette"),
    ) {
        (true, None, Some(palette)) => {
            let (text, count) = legacy_palette(&palette);
            (text, Some(count))
        }
        (old, text, _) => {
            let text = text.unwrap_or_else(|| DEFAULT_GRADIENT.to_owned());
            let count = text.split(' ').count() / 2;
            (text, old.then_some(count))
        }
    };
    let (mut speed, mut offset) = (
        number("coloring_speed", 1.0),
        number("coloring_palette_offset", 0.0),
    );
    #[allow(clippy::cast_precision_loss, reason = "a palette size")]
    if let Some(size) = palette_size.filter(|s| *s > 0) {
        offset /= size as f64;
        speed *= 10.0 / size as f64;
    }
    // PreCompatibilityMaterials: before 2.15 materials were plain, not metallic.
    let old_plain = version < 2.15;
    let flag = |name: &str| boolean(&format!("fractal_coloring_{name}"), false);
    let value = |name: &str, fallback: f64| number(&format!("fractal_coloring_{name}"), fallback);
    let vector3 = |name: &str, fallback: [f64; 3]| {
        let v = vector4(
            &format!("fractal_coloring_{name}"),
            [fallback[0], fallback[1], fallback[2], 0.0],
        );
        let three = get(&format!("fractal_coloring_{name}"))
            .map(|v| {
                v.split_whitespace()
                    .filter_map(|p| p.replace(',', ".").parse().ok())
                    .collect::<Vec<f64>>()
            })
            .and_then(|v| <[f64; 3]>::try_from(v.as_slice()).ok());
        three.unwrap_or([v[0], v[1], v[2]])
    };
    let extra = flag("extra_color_enabled_false").then(|| ExtraColour {
        initial: value("initial_color_value", 0.0),
        initial_radius: (flag("init_cond_enabled_false") && flag("ic_rad_enabled_false"))
            .then(|| value("ic_rad_weight", 1.0)),
        initial_xyz: (flag("init_cond_enabled_false") && flag("ic_xyz_enabled_false"))
            .then(|| (vector3("xyzC_111", [1.0; 3]), flag("ic_fabs_enabled_false"))),
        orbit_trap: boolean("fractal_coloring_orbit_trap_true", true)
            .then(|| value("orbit_trap_weight", 1.0)),
        aux_color: flag("aux_color_false").then(|| {
            (
                value("aux_color_weight", 1.0),
                value("aux_color_hybrid_weight", 0.0),
            )
        }),
        radius: flag("rad_enabled_false").then(|| {
            (
                value("rad_weight", 1.0),
                flag("rad_div_1e13_false"),
                flag("rad_squared_enabled_false"),
            )
        }),
        radius_over_de: flag("rad_div_de_enabled_false").then(|| {
            (
                value("rad_div_de_weight", 1.0),
                flag("rad_div_de_1e13_false"),
                flag("rad_div_de_squared_false"),
            )
        }),
        xyz: flag("xyz_bias_enabled_false").then(|| {
            (
                vector3("xyz_000", [1.0; 3]),
                value("xyz_iter_scale", 0.0),
                flag("xyz_fabs_enabled_false"),
                flag("xyz_div_1e13_false"),
                [
                    flag("xyz_x_sqrd_enabled_false"),
                    flag("xyz_y_sqrd_enabled_false"),
                    flag("xyz_z_sqrd_enabled_false"),
                ],
            )
        }),
        iterations: flag("iter_group_enabled_false").then(|| {
            (
                value("i_start_value", 0.0),
                boolean("fractal_coloring_iter_add_scale_enabled_true", true)
                    .then(|| value("iter_add_scale", 1.0)),
                flag("iter_scale_enabled_false").then(|| value("iter_scale", 0.0)),
            )
        }),
        add: (flag("global_palette_false") && flag("add_enabled_false")).then(|| {
            [
                value("add_start_value", 0.0),
                value("add_spread", 1.0),
                value("add_max", 1.0),
            ]
        }),
        parabola: (flag("global_palette_false") && flag("parab_enabled_false"))
            .then(|| [value("parab_start_value", 0.0), value("parab_scale", 1.0)]),
        cosine: (flag("global_palette_false") && flag("cos_enabled_false")).then(|| {
            [
                value("cos_start_value", 0.0),
                value("cos_period", 1.0),
                value("cos_add", 1.0),
            ]
        }),
        round: (flag("global_palette_false") && flag("round_enabled_false"))
            .then(|| value("round_scale", 1.0)),
        cos_start: value("cos_start_value", 0.0),
        min: value("min_color_value", 0.0),
        max: value("max_color_value", 1e6),
    });
    #[allow(clippy::cast_possible_truncation, reason = "an enum value")]
    let colouring = Colouring {
        algorithm: number("fractal_coloring_algorithm", 0.0) as i64,
        sphere_radius: value("sphere_radius", 1.0),
        line_direction: vector4("fractal_coloring_line_direction", [1.0, 0.0, 0.0, 0.0]),
        color_4d: flag("color_4D_enabled_false"),
        pre_v215: flag("color_preV215_false"),
        hybrid_scales: flag("extra_color_options_false").then(|| {
            [
                value("orbit_trap_scale1", 1.0),
                value("aux_color_scale1", 1.0),
                value("rad_div_de_scale1", 1.0),
            ]
        }),
        extra,
    };
    Material {
        id,
        surface_colour: colour("surface_color", [50000.0 / 65536.0; 3]),
        palette: boolean("use_colors_from_palette", true)
            && boolean("surface_gradient_enable", true),
        gradient: self::gradient(&gradient_text),
        speed,
        offset,
        shading: number("shading", 1.0),
        specular: number("specular", if old_plain { 1.0 } else { 5.0 }),
        specular_width: number("specular_width", if old_plain { 1.0 } else { 0.05 }),
        roughness: number("surface_roughness", 0.01),
        reflectance: number("reflectance", 0.0),
        metallic: boolean("metallic", !old_plain),
        luminosity: number("luminosity", 0.0),
        luminosity_colour: colour("luminosity_color", [1.0; 3]),
        transparency: number("transparency_of_surface", 0.0),
        ior: number("transparency_index_of_refraction", 1.5),
        colouring,
    }
}

#[allow(clippy::cast_possible_truncation, reason = "GPU parameters are f32")]
fn slot(v: [f64; 3], w: f64) -> [f32; 4] {
    [v[0] as f32, v[1] as f32, v[2] as f32, w as f32]
}

/// WGSL for `fractal_palette(index)`: the colour `material`'s gradient gives a colour index (`SurfaceColour`),
/// or its surface colour when it does not use the palette.
#[must_use]
pub fn emit_palette(material: &Material, params: &mut Vec<[f32; 4]>) -> String {
    let mut out = String::from("fn fractal_palette(index: f32) -> vec3<f32> {\n");
    let k = params.len();
    params.push(slot(material.surface_colour, material.speed));
    params.push(slot([material.offset, 0.0, 0.0], 0.0));
    if !material.palette || material.gradient.len() < 2 {
        let _ = writeln!(out, "    return fractal_params[{k}].xyz;\n}}");
        return out;
    }
    let first = params.len();
    for (position, colour) in &material.gradient {
        params.push(slot(*colour, *position));
    }
    let last = material.gradient.len() - 1;
    let _ = write!(
        out,
        "    let settings = fractal_params[{k}];
    // nrCol: |index| modulo 248 × 256; its position along the gradient.
    let number = abs(index) - floor(abs(index) / 63488.0) * 63488.0;
    let raw = number / 2560.0 * settings.w + fractal_params[{}].x;
    let position = raw - floor(raw);
    // PaletteIterator: the last stop at or before the position, then linear interpolation.
    var lower = 0u;
    for (var i = 1u; i < {last}u; i++) {{
        if (position >= fractal_params[{first}u + i].w) {{ lower = i; }}
    }}
    let a = fractal_params[{first}u + lower];
    let b = fractal_params[{first}u + lower + 1u];
    let span = b.w - a.w;
    let t = select(0.0, clamp((position - a.w) / span, 0.0, 1.0), span > 0.0);
    return mix(a.xyz, b.xyz, t);
}}
",
        k + 1
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_palettes_become_gradients_and_rescale() {
        let file = FractFile::parse(
            "# version 2.13\nmat1_surface_color_palette ff0000 00ff00 0000ff ffffff;\nmat1_coloring_speed 2;\n",
        );
        let m = material(&file, 1);
        assert_eq!(m.gradient.len(), 5);
        assert!((m.gradient[1].0 - 0.25).abs() < 1e-12);
        assert!((m.speed - 5.0).abs() < 1e-12);
        let mut params = Vec::new();
        let source = format!(
            "@group(0) @binding(0) var<storage, read> fractal_params: array<vec4<f32>>;\n{}",
            emit_palette(&m, &mut params)
        );
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    }
}
