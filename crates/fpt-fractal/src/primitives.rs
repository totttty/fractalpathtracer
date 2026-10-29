//! Mandelbulber's primitive objects (`primitives.cpp`, `primitive.cpp`): planes, boxes, spheres, water and the
//! rest, combined with the fractal's distance by their Boolean operators. `emit` writes them as WGSL.

use std::fmt::Write as _;

use crate::params::rotation_matrix;
use crate::scene::FractFile;

/// One primitive's shape and its own parameters (Mandelbulber's defaults filled in).
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Plane,
    Box {
        size: [f64; 3],
        rounding: f64,
    },
    Sphere {
        radius: f64,
    },
    Water(Water),
    Cylinder {
        radius: f64,
        height: f64,
        caps: bool,
    },
    Cone {
        height: f64,
        caps: bool,
        wall_normal: [f64; 2],
    },
    Torus {
        radius: f64,
        radius_power: f64,
        tube: f64,
        tube_power: f64,
    },
    Rectangle {
        width: f64,
        height: f64,
    },
    Circle {
        radius: f64,
    },
    Prism {
        height: f64,
        triangle_height: f64,
        normals: [f64; 3],
    },
    Ellipsoid {
        size: [f64; 3],
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Water {
    pub relative_amplitude: f64,
    pub length: f64,
    pub phase: f64,
    pub progression: f64,
    pub iterations: i64,
    /// Waves from the other objects' distance: their relative amplitude.
    pub from_objects: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Primitive {
    pub name: String,
    pub kind: Kind,
    pub position: [f64; 3],
    /// `SetRotation2` rows: a point's local coordinates are rows · (point − position).
    pub rotation: [[f64; 3]; 3],
    pub repeat: [f64; 3],
    /// 0 AND, 1 OR, 2 SUB, 3 reversed SUB.
    pub operator: i64,
    pub smooth: Option<f64>,
    pub empty: bool,
    pub wall: f64,
    pub limits: Option<([f64; 3], [f64; 3])>,
}

/// The enabled primitives in Mandelbulber's evaluation order (parameter-name order, then `_calculation_order`),
/// without those that only shape fog or clouds.
#[must_use]
pub fn primitives(file: &FractFile) -> Vec<Primitive> {
    let Some(main) = file.sections.get("main_parameters") else {
        return Vec::new();
    };
    // GetListOfPrimitives: `primitive_<type>_<index>` in the parameter container's (sorted) order.
    let mut names: Vec<String> = main
        .keys()
        .filter_map(|key| {
            let mut parts = key.splitn(4, '_');
            let (Some("primitive"), Some(kind), Some(index), Some(_)) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            else {
                return None;
            };
            index.parse::<u32>().ok()?;
            Some(format!("primitive_{kind}_{index}"))
        })
        .collect();
    names.sort();
    names.dedup();
    let volumetric: Vec<&str> = [
        "basic_fog_primitive",
        "distance_fog_primitive",
        "iteration_fog_primitive",
        "clouds_primitive",
    ]
    .iter()
    .filter_map(|key| {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a list index"
        )]
        let index = file.number(key, 0.0) as usize;
        names.get(index.checked_sub(1)?).map(String::as_str)
    })
    .collect();
    let order = |name: &str| file.number(&format!("{name}_calculation_order"), 1.0);
    let mut sorted: Vec<&String> = names
        .iter()
        .filter(|n| !volumetric.contains(&n.as_str()))
        .collect();
    sorted.sort_by(|a, b| order(a).total_cmp(&order(b)));
    sorted
        .into_iter()
        .filter_map(|name| primitive(file, name))
        .collect()
}

fn primitive(file: &FractFile, name: &str) -> Option<Primitive> {
    let key = |field: &str| format!("{name}_{field}");
    if !file.boolean(&key("enabled"), false) {
        return None;
    }
    let number = |field: &str, fallback: f64| file.number(&key(field), fallback);
    let boolean = |field: &str, fallback: bool| file.boolean(&key(field), fallback);
    let vector = |field: &str, fallback: [f64; 3]| file.vector(&key(field), fallback);
    let kind = match name.split('_').nth(1)? {
        "plane" => Kind::Plane,
        "box" => Kind::Box {
            size: vector("size", [1.0; 3]),
            rounding: number("rounding", 1e-15),
        },
        "sphere" => Kind::Sphere {
            radius: number("radius", 1.0),
        },
        "water" => Kind::Water(Water {
            relative_amplitude: water_amplitude(file, name),
            length: number("length", 0.1),
            phase: -number("anim_speed", 1.0) * file.number("frame_no", 0.0) * 0.1,
            progression: number("anim_progression_speed", 1.0),
            #[allow(clippy::cast_possible_truncation, reason = "an iteration count")]
            iterations: number("iterations", 5.0) as i64,
            from_objects: boolean("wave_from_objects_enable", false)
                .then(|| number("wave_from_objects_relative_amplitude", 0.5)),
        }),
        "cylinder" => Kind::Cylinder {
            radius: number("radius", 1.0),
            height: number("height", 1.0),
            caps: boolean("caps", true),
        },
        "cone" => {
            let (radius, height) = (number("radius", 1.0), number("height", 1.0));
            let length = (1.0 + (radius / height).powi(2)).sqrt();
            Kind::Cone {
                height,
                caps: boolean("caps", true),
                wall_normal: [1.0 / length, radius / height / length],
            }
        }
        "torus" => Kind::Torus {
            radius: number("radius", 1.0),
            radius_power: 2f64.powf(number("radius_lpow", 1.0)),
            tube: number("tube_radius", 0.2),
            tube_power: 2f64.powf(number("tube_radius_lpow", 1.0)),
        },
        "rectangle" => Kind::Rectangle {
            width: number("width", 1.0),
            height: number("height", 1.0),
        },
        "circle" => Kind::Circle {
            radius: number("radius", 1.0),
        },
        "prism" => {
            let angle = number("prism_angle", 60.0).to_radians() / 2.0;
            Kind::Prism {
                height: number("height", 1.0) / 2.0,
                triangle_height: number("trangle_height", 1.0) * angle.sin(),
                normals: [angle.sin(), angle.cos(), angle.sin()],
            }
        }
        "ellipsoid" => Kind::Ellipsoid {
            size: vector("size", [1.0; 3]),
        },
        _ => return None,
    };
    // Planes, rectangles and circles have no repeat, limits or (for the last two) walls.
    let simple = matches!(
        kind,
        Kind::Plane | Kind::Rectangle { .. } | Kind::Circle { .. }
    );
    let walled = !matches!(kind, Kind::Rectangle { .. } | Kind::Circle { .. });
    let limited = !matches!(
        kind,
        Kind::Plane | Kind::Rectangle { .. } | Kind::Circle { .. } | Kind::Prism { .. }
    );
    Some(Primitive {
        name: name.to_owned(),
        position: vector("position", [0.0; 3]),
        rotation: rotation_matrix(2, vector("rotation", [0.0; 3])),
        repeat: if simple || matches!(kind, Kind::Water(_)) {
            [0.0; 3]
        } else {
            vector("repeat", [0.0; 3])
        },
        #[allow(clippy::cast_possible_truncation, reason = "an enum value")]
        operator: number("boolean_operator", 1.0) as i64,
        smooth: boolean("smooth_de_combine_enable", false)
            .then(|| number("smooth_de_combine_distance", 0.1)),
        empty: walled && boolean("empty", false),
        wall: if walled {
            number("wall_thickness", 0.0)
        } else {
            0.0
        },
        limits: (limited && boolean("limits_enable", false)).then(|| {
            (
                vector("limits_min", [-0.7; 3]),
                vector("limits_max", [0.7; 3]),
            )
        }),
        kind,
    })
}

/// The water's amplitude relative to its wave length: files before 2.13 call it `_amplitude`, and up to 2.12 it
/// was absolute (`cSettings::Compatibility`).
fn water_amplitude(file: &FractFile, name: &str) -> f64 {
    let old = file.version_number() < (2, 13);
    let stored = file
        .get("main_parameters", &format!("{name}_relative_amplitude"))
        .or_else(|| {
            old.then(|| file.get("main_parameters", &format!("{name}_amplitude")))
                .flatten()
        })
        .and_then(|v| v.replace(',', ".").parse().ok())
        .unwrap_or(0.2);
    if file.version_number() <= (2, 12) {
        stored / file.number(&format!("{name}_length"), 0.1)
    } else {
        stored
    }
}

#[allow(clippy::cast_possible_truncation, reason = "GPU parameters are f32")]
fn slot(v: [f64; 3], w: f64) -> [f32; 4] {
    [v[0] as f32, v[1] as f32, v[2] as f32, w as f32]
}

/// WGSL for `fractal_primitives(p, distance, detail_size)`: `TotalDistance` over the scene's primitives, with `p`
/// in the estimate's coordinates (Mandelbulber's, less `origin`). Their numbers go into `params`.
#[must_use]
pub fn emit(file: &FractFile, origin: [f64; 3], params: &mut Vec<[f32; 4]>) -> String {
    let all = primitives(file);
    let mut out = String::from(
        "fn fractal_primitives(p: vec3<f32>, distance_in: f32, detail_size: f32) -> f32 {\n",
    );
    out.push_str("    var distance = distance_in;\n");
    if !all.is_empty() {
        let global = params.len();
        let at = file.vector("all_primitives_position", [0.0; 3]);
        params.push(slot([0, 1, 2].map(|i| at[i] - origin[i]), 0.0));
        for row in rotation_matrix(2, file.vector("all_primitives_rotation", [0.0; 3])) {
            params.push(slot(row, 0.0));
        }
        let rows = |k: usize| {
            format!(
                "vec3(dot(fractal_params[{}].xyz, {{v}}), dot(fractal_params[{}].xyz, {{v}}), dot(fractal_params[{}].xyz, {{v}}))",
                k,
                k + 1,
                k + 2
            )
        };
        let _ = writeln!(out, "    let all = p - fractal_params[{global}].xyz;");
        let _ = writeln!(
            out,
            "    let q = {};",
            rows(global + 1).replace("{v}", "all")
        );
        for primitive in &all {
            let _ = writeln!(out, "    {{ // {}", primitive.name);
            let k = params.len();
            params.push(slot(primitive.position, primitive.wall));
            for row in primitive.rotation {
                params.push(slot(row, 0.0));
            }
            params.push(slot(primitive.repeat, primitive.smooth.unwrap_or(0.0)));
            let (low, high) = primitive.limits.unwrap_or(([0.0; 3], [0.0; 3]));
            params.push(slot(low, 0.0));
            params.push(slot(high, 0.0));
            let own = params.len();
            let _ = writeln!(out, "        let local = q - fractal_params[{k}].xyz;");
            let _ = writeln!(
                out,
                "        var point = {};",
                rows(k + 1).replace("{v}", "local")
            );
            let _ = writeln!(
                out,
                "        point = fractal_repeat_mod(point, fractal_params[{}].xyz);",
                k + 4
            );
            let d = shape(&primitive.kind, primitive.empty, own, params);
            out.push_str(&d);
            if primitive.empty {
                out.push_str("        d = abs(d);\n");
            }
            if !matches!(primitive.kind, Kind::Rectangle { .. } | Kind::Circle { .. }) {
                let _ = writeln!(out, "        d = max(d - fractal_params[{k}].w, 0.0);");
            }
            if primitive.limits.is_some() {
                let _ = writeln!(
                    out,
                    "        let outside = max(point - fractal_params[{}].xyz, fractal_params[{}].xyz - point);\n        d = max(d, max(outside.x, max(outside.y, outside.z)));",
                    k + 6,
                    k + 5
                );
            }
            let combine = match (primitive.operator, primitive.smooth) {
                (0, _) => "distance = max(distance, d);".to_owned(),
                (1, Some(_)) => format!("distance = fractal_smooth_union(distance, d, fractal_params[{}].w);", k + 4),
                (2, _) => "if (distance < detail_size) { if (d < 1.5 * detail_size) { distance = 1.5 * detail_size; } else { distance = max(max(1.5 * detail_size - d, distance), 0.0); } }".into(),
                (3, _) => "if (d < detail_size) { if (distance < 1.5 * detail_size) { distance = 1.5 * detail_size; } else { distance = max(max(1.5 * detail_size - distance, d), 0.0); } } else { distance = d; }".into(),
                _ => "distance = min(distance, d);".into(),
            };
            let _ = writeln!(out, "        {combine}\n    }}");
        }
    }
    out.push_str("    return distance;\n}\n");
    out
}

/// The shape's distance `d` from `point` (its local, repeated coordinates); its numbers from slot `own`.
fn shape(kind: &Kind, empty: bool, own: usize, params: &mut Vec<[f32; 4]>) -> String {
    let s = format!("fractal_params[{own}]");
    match kind {
        Kind::Plane => "        var d = point.z;\n".into(),
        Kind::Box { size, rounding } => {
            params.push(slot(*size, *rounding));
            if empty {
                // A hollow box: the largest axial distance (made absolute below).
                format!(
                    "        let axes = abs(point) - {s}.xyz * 0.5;\n        var d = max(axes.x, max(axes.y, axes.z));\n"
                )
            } else {
                format!(
                    "        var d = length(max(abs(point) - {s}.xyz * 0.5, vec3(0.0))) - {s}.w;\n"
                )
            }
        }
        Kind::Sphere { radius } => {
            params.push(slot([*radius, 0.0, 0.0], 0.0));
            format!("        var d = length(point) - {s}.x;\n")
        }
        Kind::Water(water) => {
            params.push(slot(
                [water.length, water.relative_amplitude, water.phase],
                water.progression,
            ));
            params.push(slot([water.from_objects.unwrap_or(0.0), 0.0, 0.0], 0.0));
            let from_objects = water.from_objects.is_some();
            let iterations = water.iterations;
            let mut code = format!(
                "        let water = {s};\n        let length_ = water.x;\n        let phase = water.z;\n        var w = point;\n"
            );
            if from_objects {
                let _ = writeln!(
                    code,
                    "        let wave_amplitude = fractal_params[{}].x;\n        w.x += length_ * 20.0 * exp(-distance / length_ / 5.0) * wave_amplitude;",
                    own + 1
                );
            }
            code.push_str("        let amplitude = length_ * water.y;\n        var d = w.z;\n");
            code.push_str("        if (d < amplitude * 10.0) {\n");
            if from_objects {
                code.push_str(
                    "            let object_wave = sin(distance / length_ * 5.0 - phase * 2.0) * exp(-distance / length_ / 5.0) * wave_amplitude;\n",
                );
            } else {
                code.push_str("            let object_wave = 0.0;\n");
            }
            let _ = write!(
                code,
                "            var wave_x = object_wave;
            var wave_y = -object_wave;
            var power = 1.0;
            var power2 = 0.05;
            w.x += phase * 0.05 * (water.w * 3.0 - 3.0);
            for (var i = 1; i <= {iterations}; i++) {{
                let p3 = power * power2;
                let shift = phase / (f32(i) / 2.697 + 1.0);
                let x_temp = sin(f32(i) + 0.4 * wave_x * p3 + sin(0.23 * w.y / length_ * p3 + shift * 0.134 * p3) + w.x / length_ * p3 + shift * p3) / power;
                let y_temp = cos(f32(i) + 0.4 * wave_y * p3 + sin(w.x / length_ * p3 + shift * 0.0179 * p3) + 0.23 * w.y / length_ * p3 + shift * 0.023 * p3) / power;
                wave_x += x_temp;
                wave_y += y_temp;
                power2 = power2 + (1.0 - power2 + 0.5 * sin(shift * 0.0323)) * 0.7;
                power *= 1.872;
            }}
            d += (wave_x + wave_y) * amplitude;
        }}
        point = w;
"
            );
            code
        }
        Kind::Cylinder {
            radius,
            height,
            caps,
        } => {
            params.push(slot([*radius, *height, 0.0], 0.0));
            let open = if *caps { "" } else { "        d = abs(d);\n" };
            format!(
                "        var d = length(point.xy) - {s}.x;\n{open}        d = max(abs(point.z) - {s}.y * 0.5, d);\n"
            )
        }
        Kind::Cone {
            height,
            caps,
            wall_normal,
        } => {
            params.push(slot([*height, wall_normal[0], wall_normal[1]], 0.0));
            let open = if *caps { "" } else { "        d = abs(d);\n" };
            format!(
                "        point.z -= {s}.x;\n        var d = dot({s}.yz, vec2(length(point.xy), point.z));\n{open}        d = max(-point.z - {s}.x, d);\n"
            )
        }
        Kind::Torus {
            radius,
            radius_power,
            tube,
            tube_power,
        } => {
            params.push(slot([*radius, *radius_power, *tube], *tube_power));
            // LengthPow: (x^p + y^p)^(1/p); the powers are 2^n, even for n >= 1.
            format!(
                "        let d1 = pow(pow(abs(point.x), {s}.y) + pow(abs(point.y), {s}.y), 1.0 / {s}.y) - {s}.x;\n        var d = pow(pow(abs(d1), {s}.w) + pow(abs(point.z), {s}.w), 1.0 / {s}.w) - {s}.z;\n"
            )
        }
        Kind::Rectangle { width, height } => {
            params.push(slot([*width, *height, 0.0], 0.0));
            format!(
                "        var d = length(vec3(max(abs(point.x) - {s}.x * 0.5, 0.0), max(abs(point.y) - {s}.y * 0.5, 0.0), abs(point.z)));\n"
            )
        }
        Kind::Circle { radius } => {
            params.push(slot([*radius, 0.0, 0.0], 0.0));
            format!("        var d = max(abs(point.z), length(point.xy) - {s}.x);\n")
        }
        Kind::Prism {
            height,
            triangle_height,
            normals,
        } => {
            params.push(slot(*normals, *height));
            params.push(slot([*triangle_height, 0.0, 0.0], 0.0));
            format!(
                "        let q = abs(point);\n        let th = fractal_params[{}].x;\n        var d = max(q.z - {s}.w, max(q.x * {s}.y + point.y * {s}.z, -point.y + th) - th);\n",
                own + 1
            )
        }
        Kind::Ellipsoid { size } => {
            params.push(slot(*size, 0.0));
            format!(
                "        let k0 = length(point / {s}.xyz);\n        let k1 = length(point / ({s}.xyz * {s}.xyz));\n        var d = k0 * (k0 - 1.0) / k1;\n"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_parse_in_order_and_validate() {
        let file = FractFile::parse(
            "primitive_plane_1_enabled true;\nprimitive_plane_1_position 0 0 -1;\n\
             primitive_water_2_enabled true;\nprimitive_water_2_calculation_order 0;\n\
             primitive_box_1_enabled false;\nprimitive_torus_1_enabled true;\nprimitive_torus_1_boolean_operator 2;\n",
        );
        let all = primitives(&file);
        let names: Vec<&str> = all.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "primitive_water_2",
                "primitive_plane_1",
                "primitive_torus_1"
            ]
        );
        let mut params = Vec::new();
        let source = format!(
            "@group(0) @binding(0) var<storage, read> fractal_params: array<vec4<f32>>;\n{}{}",
            crate::boolean::WGSL_HELPERS,
            emit(&file, [0.0; 3], &mut params)
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
