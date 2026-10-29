//! Mandelbulber's lights (`light.cpp`, `InitLightParams`) and background (`shader_background.cpp`), in its own
//! coordinates, with the files before 2.25 converted as `cSettings::Compatibility` does (the main light became
//! light 1 and the auxiliary lights 2 to 5).

use crate::params::set_rotation;
use crate::scene::FractFile;

/// What a light is; directions are unit vectors in Mandelbulber's coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum LightKind {
    /// Parallel light arriving from `towards` (Mandelbulber's `lightDirection`, pointing at the light).
    Directional { towards: [f64; 3] },
    /// Falls off as 1 / distance^`decay`.
    Point { position: [f64; 3], decay: i32 },
    /// A conical light at `position` pointing away from `towards`, full within `cone` (radians) of its axis and
    /// fading over `soft` beyond.
    Spot {
        position: [f64; 3],
        towards: [f64; 3],
        cone: f64,
        soft: f64,
        decay: i32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Light {
    pub id: u32,
    pub kind: LightKind,
    /// Linear RGB (`toRGBFloat`).
    pub colour: [f64; 3],
    /// `intensity` × `all_lights_intensity`.
    pub intensity: f64,
}

/// `InitLightParams`' per-light defaults: intensity, position, colour.
fn defaults(id: u32) -> (f64, [f64; 3], [f64; 3]) {
    let c = |r: f64, g: f64, b: f64| [r / 65536.0, g / 65536.0, b / 65536.0];
    match id {
        2 => (0.325, [3.0, -3.0, 3.0], c(45761.0, 53633.0, 59498.0)),
        3 => (0.25, [-3.0, -3.0, 0.0], c(62875.0, 55818.0, 50083.0)),
        4 => (0.75, [-3.0, 3.0, -1.0], c(64884.0, 64928.0, 48848.0)),
        5 => (0.5, [0.0, -1.0, -3.0], c(52704.0, 62492.0, 45654.0)),
        _ => (1.0, [0.0; 3], c(65535.0, 65535.0, 65535.0)),
    }
}

fn rgb16(value: &str) -> Option<[f64; 3]> {
    let parts: Vec<f64> = value
        .split_whitespace()
        .map(|p| u32::from_str_radix(p, 16).map(f64::from))
        .collect::<Result<_, _>>()
        .ok()?;
    (parts.len() == 3).then(|| [parts[0] / 65536.0, parts[1] / 65536.0, parts[2] / 65536.0])
}

fn normalize(v: [f64; 3]) -> [f64; 3] {
    let l = v.iter().map(|c| c * c).sum::<f64>().sqrt();
    if l > 0.0 { v.map(|c| c / l) } else { v }
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// `CVector3::RotateAroundVectorByAngle` (Rodrigues).
fn rotate_around(v: [f64; 3], axis: [f64; 3], angle: f64) -> [f64; 3] {
    let (s, c) = angle.sin_cos();
    let k = cross(axis, v);
    let d = axis[0] * v[0] + axis[1] * v[1] + axis[2] * v[2];
    [0, 1, 2].map(|i| v[i] * c + k[i] * s + axis[i] * d * (1.0 - c))
}

fn apply(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

/// The enabled lights Mandelbulber renders with: directional, point and conical (`spot`); projection and beam
/// lights are left out.
#[must_use]
pub fn lights(file: &FractFile) -> Vec<Light> {
    let old = file.version() < 2.25;
    let main = file.sections.get("main_parameters");
    let mut ids: Vec<u32> = main
        .map(|m| {
            m.keys()
                .filter_map(|k| {
                    if let Some(rest) = k.strip_prefix("light") {
                        return rest.split_once('_')?.0.parse().ok();
                    }
                    // aux_light_<field>_<i> is light i + 1.
                    let index: u32 = k
                        .strip_prefix("aux_light_")?
                        .rsplit('_')
                        .next()?
                        .parse()
                        .ok()?;
                    Some(index + 1)
                })
                .collect()
        })
        .unwrap_or_default();
    ids.push(1);
    ids.sort_unstable();
    ids.dedup();
    let camera = file.vector("camera", [3.0, -6.0, 2.0]);
    let target = file.vector("target", [0.0, 0.0, 0.0]);
    let forward = normalize([0, 1, 2].map(|i| target[i] - camera[i]));
    let right = normalize(cross(
        forward,
        file.vector("camera_top", [-0.127_775_3, 0.255_550_6, 0.958_314]),
    ));
    let top = cross(right, forward);
    let all_intensity = file.number("all_lights_intensity", 1.0);
    ids.into_iter()
        .filter_map(|id| {
            // A setting under its 2.25 name, or (older files) its earlier one.
            let get = |field: &str| -> Option<String> {
                let modern = file
                    .get("main_parameters", &format!("light{id}_{field}"))
                    .map(str::to_owned);
                if modern.is_some() || !old {
                    return modern;
                }
                let legacy = if id == 1 {
                    match field {
                        "intensity" => "main_light_intensity".to_owned(),
                        "color" => "main_light_colour".into(),
                        "enabled" => "main_light_enable".into(),
                        "relative_position" => "main_light_position_relative".into(),
                        _ => return None,
                    }
                } else {
                    let name = if field == "color" { "colour" } else { field };
                    format!("aux_light_{name}_{}", id - 1)
                };
                file.get("main_parameters", &legacy).map(str::to_owned)
            };
            let number = |field: &str, fallback: f64| {
                get(field)
                    .and_then(|v| v.replace(',', ".").parse().ok())
                    .unwrap_or(fallback)
            };
            let boolean = |field: &str, fallback: bool| {
                get(field).map_or(fallback, |v| v == "true" || v == "1")
            };
            let vector = |field: &str, fallback: [f64; 3]| {
                get(field)
                    .map(|v| {
                        v.split_whitespace()
                            .filter_map(|p| p.replace(',', ".").parse().ok())
                            .collect::<Vec<f64>>()
                    })
                    .and_then(|v| <[f64; 3]>::try_from(v.as_slice()).ok())
                    .unwrap_or(fallback)
            };
            if !boolean("enabled", id == 1) {
                return None;
            }
            let (default_intensity, default_position, default_colour) = defaults(id);
            let mut intensity = number("intensity", default_intensity);
            // Before 2.25 the auxiliary lights were four times brighter, unless left at their default.
            if old && id > 1 && (intensity - default_intensity).abs() > 1e-12 {
                intensity /= 4.0;
            }
            let colour = get("color")
                .and_then(|v| rgb16(&v))
                .unwrap_or(default_colour);
            let kind = match get("type").as_deref() {
                Some("directional" | "0") => 0,
                Some("point" | "1") => 1,
                Some("spot" | "2") => 2,
                Some(_) => return None,
                None if id == 1 => 0,
                None => 1,
            };
            let decay = match get("decayFunction").as_deref() {
                Some("1/r" | "0") => 1,
                Some("1/r3" | "2") => 3,
                _ => 2,
            };
            let mut rotation = if id == 1 && old {
                [number("alpha", -45.0), number("beta", 45.0), 0.0]
            } else {
                vector(
                    "rotation",
                    if id == 1 {
                        [-45.0, 45.0, 0.0]
                    } else {
                        [0.0; 3]
                    },
                )
            };
            if id == 1 && old {
                rotation = [
                    file.number("main_light_alpha", rotation[0]),
                    file.number("main_light_beta", rotation[1]),
                    0.0,
                ];
            }
            // light.cpp divides by 180.8, and mirrors one axis by type.
            let sign = if kind == 0 {
                [1.0, -1.0, 1.0]
            } else {
                [-1.0, 1.0, 1.0]
            };
            let radians = [0, 1, 2].map(|i| rotation[i] / 180.8 * std::f64::consts::PI * sign[i]);
            let relative = boolean("relative_position", id == 1);
            let offset = vector("position", default_position);
            let (position, towards) = if relative {
                let position = [0, 1, 2].map(|i| {
                    camera[i] + forward[i] * offset[2] + top[i] * offset[1] + right[i] * offset[0]
                });
                let mut d = forward.map(|c| -c);
                d = rotate_around(d, forward, radians[2]);
                d = rotate_around(d, right, radians[1]);
                d = rotate_around(d, top, radians[0]);
                (position, d)
            } else {
                (offset, apply(set_rotation(radians), [0.0, -1.0, 0.0]))
            };
            let kind = match kind {
                0 => LightKind::Directional {
                    towards: normalize(towards),
                },
                1 => LightKind::Point { position, decay },
                _ => LightKind::Spot {
                    position,
                    towards: normalize(towards),
                    cone: number("cone_angle", 10.0).to_radians(),
                    soft: number("cone_soft_angle", 1.0).to_radians(),
                    decay,
                },
            };
            Some(Light {
                id,
                kind,
                colour,
                intensity: intensity * all_intensity,
            })
        })
        .collect()
}

/// The background gradient (`BackgroundShader`).
#[derive(Clone, Debug, PartialEq)]
pub struct Background {
    /// Top, middle and bottom.
    pub colours: [[f64; 3]; 3],
    /// Three-colour gradient, or the top colour alone.
    pub three: bool,
    pub brightness: f64,
    pub gamma: f64,
}

impl Background {
    /// The colour in direction `up` (the direction's component along Mandelbulber's z).
    #[must_use]
    pub fn colour(&self, up: f64) -> [f64; 3] {
        let [top, middle, bottom] = self.colours;
        let mix = |a: [f64; 3], b: [f64; 3], t: f64| {
            [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t.clamp(0.0, 1.0))
        };
        let t = (up + 1.0) / 2.0;
        let c = if !self.three {
            top
        } else if t < 0.5 {
            mix(bottom, middle, 2.0 * t)
        } else {
            mix(middle, top, 2.0 * t - 1.0)
        };
        c.map(|v| {
            (v * self.brightness)
                .max(0.0)
                .powf(1.0 / self.gamma.max(1e-6))
        })
    }
}

#[must_use]
pub fn background(file: &FractFile) -> Background {
    let colour = |key: &str, fallback: [f64; 3]| {
        file.get("main_parameters", key)
            .and_then(rgb16)
            .unwrap_or(fallback)
    };
    Background {
        colours: [
            colour(
                "background_color_1",
                [0.0, 38306.0 / 65536.0, 65535.0 / 65536.0],
            ),
            colour("background_color_2", [65535.0 / 65536.0; 3]),
            colour(
                "background_color_3",
                [0.0, 10000.0 / 65536.0, 500.0 / 65536.0],
            ),
        ],
        three: file.boolean("background_3_colors_enable", true),
        brightness: file.number("background_brightness", 1.0),
        gamma: file.number("background_gamma", 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_main_light_is_directional_and_legacy_aux_lights_map() {
        let file = FractFile::parse(
            "# version 2.13\naux_light_enabled_1 true;\naux_light_intensity_1 2;\naux_light_position_1 1 2 3;\n",
        );
        let all = lights(&file);
        assert_eq!(all.len(), 2);
        assert!(matches!(all[0].kind, LightKind::Directional { .. }));
        assert_eq!(all[1].id, 2);
        assert!(matches!(
            all[1].kind,
            LightKind::Point {
                position: [1.0, 2.0, 3.0],
                decay: 2
            }
        ));
        assert!((all[1].intensity - 0.5).abs() < 1e-12);
    }
}
