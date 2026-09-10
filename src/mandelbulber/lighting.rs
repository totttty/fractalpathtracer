//! Scene-specialized authored light tables. Positions use FPT world coordinates.
use super::{
    MandelbulberScene, parse_bool, parse_number, parse_rgb16, parse_vec3, version_is_before,
};
use anyhow::{Result, ensure};
use serde::Serialize;
use std::{collections::BTreeSet, fmt::Write};

const MARKER: &str = "// FPT_MANDELBULBER_GENERATED_INSERTION_POINT";

#[derive(Clone, Debug, Serialize)]
pub struct DirectionalLight {
    pub id: u32,
    pub camera_direction: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
    pub cast_shadows: bool,
    pub penetrating: bool,
    pub cone_radians: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct PointLight {
    pub id: u32,
    pub position: [f32; 3],
    pub camera_relative: bool,
    pub color: [f32; 3],
    pub intensity: f32,
    pub decay_power: u32,
    pub cast_shadows: bool,
    pub penetrating: bool,
    pub cone_radians: f32,
    pub size: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct FakeLights {
    pub trap: [f32; 3],
    pub first: u32,
    pub last: u32,
    pub relative: bool,
    pub intensity: f32,
    pub colors: Vec<[f32; 3]>,
    pub indirect: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct AuxiliaryLighting {
    pub directional: Vec<DirectionalLight>,
    pub point: Vec<PointLight>,
    pub fake: Option<FakeLights>,
    pub unsupported: Vec<String>,
}

impl AuxiliaryLighting {
    pub fn prepare_random(
        &mut self,
        scene: &MandelbulberScene,
        source: &str,
        cfg: &crate::ffi::FptRenderConfig,
    ) -> Result<()> {
        self.point
            .extend(super::lighting_random::generate(scene, source, cfg)?);
        Ok(())
    }
    pub fn load(scene: &MandelbulberScene) -> Result<Self> {
        let mut result = Self::default();
        if version_is_before(&scene.source_version, 2, 25)? {
            if scene.auxiliary_light_enabled {
                result
                    .unsupported
                    .push("legacy auxiliary point lights".into());
            }
            return Ok(result);
        }
        let parameters = &scene.main_parameters;
        if parameters
            .get("fake_lights_enabled")
            .map(|v| parse_bool(v))
            .transpose()?
            .unwrap_or(false)
        {
            let shape = parameters
                .get("fake_lights_orbit_trap_shape")
                .map(String::as_str)
                .unwrap_or("0");
            if !scene.hybrid_enabled
                && !scene.boolean_enabled
                && !scene.force_delta_de
                && matches!(shape, "0" | "point")
            {
                let num = |key, default| -> Result<f64> {
                    Ok(parameters
                        .get(key)
                        .map(|v| parse_number(v))
                        .transpose()?
                        .unwrap_or(default))
                };
                let first = num("fake_lights_min_iter", 1.0)?;
                let last = num("fake_lights_max_iter", 2.0)?;
                let intensity = num("fake_lights_intensity", 1.0)?;
                let trap = parameters
                    .get("fake_lights_orbit_trap")
                    .map(|v| parse_vec3(v))
                    .transpose()?
                    .unwrap_or([2.0, 0.0, 0.0]);
                ensure!(
                    first.is_finite()
                        && last.is_finite()
                        && (0.0..=4095.0).contains(&first)
                        && (first..=4095.0).contains(&last)
                        && first.fract() == 0.0
                        && last.fract() == 0.0
                        && intensity.is_finite()
                        && (0.0..=f32::MAX as f64).contains(&intensity)
                        && trap
                            .iter()
                            .all(|v| v.is_finite() && v.abs() <= f32::MAX as f64),
                    "invalid fake light parameters"
                );
                let flag = |key| -> Result<bool> {
                    Ok(parameters
                        .get(key)
                        .map(|v| parse_bool(v))
                        .transpose()?
                        .unwrap_or(false))
                };
                let count = if flag("fake_lights_color_3_enabled")? {
                    3
                } else if flag("fake_lights_color_2_enabled")? {
                    2
                } else {
                    1
                };
                let mut colors = Vec::new();
                for key in [
                    "fake_lights_color",
                    "fake_lights_color_2",
                    "fake_lights_color_3",
                ]
                .iter()
                .take(count)
                {
                    colors.push(
                        parameters
                            .get(*key)
                            .map(|v| parse_rgb16(v))
                            .transpose()?
                            .unwrap_or([1.0; 3]),
                    );
                }
                result.fake = Some(FakeLights {
                    trap: trap.map(|v| v as f32),
                    first: first as u32,
                    last: last as u32,
                    relative: flag("fake_lights_relative_center")?,
                    intensity: intensity as f32,
                    colors,
                    indirect: flag("MC_global_illumination")?,
                });
            } else {
                result
                    .unsupported
                    .push("fake lights: non-point, hybrid, boolean or delta orbit".into());
            }
        }
        let ids: BTreeSet<u32> = parameters
            .keys()
            .filter_map(|key| {
                key.strip_prefix("light")?
                    .strip_suffix("_enabled")?
                    .parse()
                    .ok()
            })
            .filter(|id| *id > 1)
            .collect();
        let global = parameters
            .get("all_lights_intensity")
            .map(|v| parse_number(v))
            .transpose()?
            .unwrap_or(1.0);
        ensure!(
            global.is_finite() && global >= 0.0,
            "invalid all_lights_intensity"
        );
        for id in ids {
            let get = |name: &str| parameters.get(&format!("light{id}_{name}"));
            let boolean = |name, default| -> Result<bool> {
                Ok(get(name)
                    .map(|v| parse_bool(v))
                    .transpose()?
                    .unwrap_or(default))
            };
            if !boolean("enabled", false)? {
                continue;
            }
            let kind = get("type").map(String::as_str).unwrap_or("point");
            let point = matches!(kind, "point" | "1");
            if !point && !matches!(kind, "directional" | "0") {
                result.unsupported.push(format!("light{id}: {kind}"));
                continue;
            }
            if !point
                && (!boolean("relative_position", false)? || boolean("use_target_point", false)?)
            {
                result.unsupported.push(format!(
                    "light{id}: world-space or target-point directional placement"
                ));
                continue;
            }
            let rotation = get("rotation")
                .map(|v| parse_vec3(v))
                .transpose()?
                .unwrap_or([0.0; 3]);
            let intensity = get("intensity")
                .map(|v| parse_number(v))
                .transpose()?
                .unwrap_or(match id {
                    2 => 0.325,
                    3 => 0.25,
                    4 => 0.75,
                    5 => 0.5,
                    _ => 1.0,
                })
                * global;
            let cone = get("soft_shadow_cone")
                .map(|v| parse_number(v))
                .transpose()?
                .unwrap_or(1.0);
            ensure!(
                rotation.iter().all(|v| v.is_finite())
                    && intensity.is_finite()
                    && intensity >= 0.0
                    && intensity <= f32::MAX as f64
                    && cone.is_finite()
                    && (0.0..=180.0).contains(&cone),
                "invalid directional light{id}"
            );
            // Derive camera-local direction using the same native-space angle
            // convention as light1. The shader applies the live camera rotation.
            let mut local = scene.clone();
            local.camera = [0.0; 3];
            local.target = [0.0, 1.0, 0.0];
            local.camera_top = [0.0, 0.0, 1.0];
            local.main_light_rotation = rotation;
            let color = get("color")
                .map(|v| parse_rgb16(v))
                .transpose()?
                .unwrap_or(match id {
                    2 => [45761.0 / 65535.0, 53633.0 / 65535.0, 59498.0 / 65535.0],
                    3 => [62875.0 / 65535.0, 55818.0 / 65535.0, 50083.0 / 65535.0],
                    4 => [64884.0 / 65535.0, 64928.0 / 65535.0, 48848.0 / 65535.0],
                    5 => [52704.0 / 65535.0, 62492.0 / 65535.0, 45654.0 / 65535.0],
                    _ => [1.0; 3],
                });
            if point {
                let position = get("position")
                    .map(|v| parse_vec3(v))
                    .transpose()?
                    .unwrap_or(match id {
                        2 => [3.0, -3.0, 3.0],
                        3 => [-3.0, -3.0, 0.0],
                        4 => [-3.0, 3.0, -1.0],
                        5 => [0.0, -1.0, -3.0],
                        _ => [0.0; 3],
                    });
                let relative = boolean("relative_position", false)?;
                let size = get("size")
                    .map(|v| parse_number(v))
                    .transpose()?
                    .unwrap_or(0.5)
                    * parameters
                        .get("all_lights_size")
                        .map(|v| parse_number(v))
                        .transpose()?
                        .unwrap_or(1.0);
                ensure!(
                    position
                        .iter()
                        .all(|v| v.is_finite() && v.abs() <= f32::MAX as f64)
                        && size.is_finite()
                        && size >= 0.0
                        && size <= f32::MAX as f64,
                    "invalid point light{id}"
                );
                let decay_power = match get("decayFunction").map(String::as_str).unwrap_or("1/r2") {
                    "0" | "1/r" => 1,
                    "1" | "1/r2" => 2,
                    "2" | "1/r3" => 3,
                    _ => anyhow::bail!("unsupported decay function for light{id}"),
                };
                result.point.push(PointLight {
                    id,
                    position: if relative {
                        position
                    } else {
                        super::map_mandel_point(position)
                    }
                    .map(|v| v as f32),
                    camera_relative: relative,
                    color,
                    intensity: intensity as f32,
                    decay_power,
                    cast_shadows: boolean("cast_shadows", true)?,
                    penetrating: boolean("penetrating", true)?,
                    cone_radians: cone.to_radians() as f32,
                    size: size as f32,
                });
                continue;
            }
            result.directional.push(DirectionalLight {
                id,
                camera_direction: local.main_light_direction().map(|v| v as f32),
                color,
                intensity: intensity as f32,
                cast_shadows: boolean("cast_shadows", true)?,
                penetrating: boolean("penetrating", true)?,
                cone_radians: cone.to_radians() as f32,
            });
        }
        ensure!(
            result.directional.len() + result.point.len() <= 64,
            "at most 64 explicit auxiliary lights are supported"
        );
        Ok(result)
    }

    pub fn specialize(&self, source: &str) -> Result<String> {
        if self.directional.is_empty() && self.point.is_empty() && self.fake.is_none() {
            return Ok(source.to_owned());
        }
        ensure!(
            source.matches(MARKER).count() == 1,
            "missing/duplicate Mandel source insertion marker"
        );
        let mut table = String::new();
        if !self.directional.is_empty() {
            table.push_str(
            "#define FPT_MANDEL_AUX_DIRECTIONAL 1\nstruct MandelAuxDirectional { float3 direction; float3 color; float intensity; float cone; bool casts; bool penetrating; };\n",
        );
            writeln!(
                table,
                "constant uint mandelAuxDirectionalCount = {}u;",
                self.directional.len()
            )?;
            writeln!(
                table,
                "constant MandelAuxDirectional mandelAuxDirectionalLights[{}] = {{",
                self.directional.len()
            )?;
            for light in &self.directional {
                let d = light.camera_direction;
                let c = light.color;
                writeln!(
                    table,
                    "{{ float3({:.9e}f,{:.9e}f,{:.9e}f), float3({:.9e}f,{:.9e}f,{:.9e}f), {:.9e}f, {:.9e}f, {}, {} }},",
                    d[0],
                    d[1],
                    d[2],
                    c[0],
                    c[1],
                    c[2],
                    light.intensity,
                    light.cone_radians,
                    light.cast_shadows,
                    light.penetrating
                )?;
            }
            table.push_str("};\n");
        }
        if !self.point.is_empty() {
            table.push_str("#define FPT_MANDEL_AUX_POINT 1\nstruct MandelAuxPoint { float3 position; float3 color; float intensity; uint decay; bool relative; bool casts; bool penetrating; float cone; float size; };\n");
            writeln!(
                table,
                "constant uint mandelAuxPointCount = {}u;",
                self.point.len()
            )?;
            writeln!(
                table,
                "constant MandelAuxPoint mandelAuxPointLights[{}] = {{",
                self.point.len()
            )?;
            for light in &self.point {
                let p = light.position;
                let c = light.color;
                writeln!(
                    table,
                    "{{ float3({:.9e}f,{:.9e}f,{:.9e}f), float3({:.9e}f,{:.9e}f,{:.9e}f), {:.9e}f, {}u, {}, {}, {}, {:.9e}f, {:.9e}f }},",
                    p[0],
                    p[1],
                    p[2],
                    c[0],
                    c[1],
                    c[2],
                    light.intensity,
                    light.decay_power,
                    light.camera_relative,
                    light.cast_shadows,
                    light.penetrating,
                    light.cone_radians,
                    light.size
                )?;
            }
            table.push_str("};\n");
        }
        if let Some(fake) = &self.fake {
            ensure!(
                source.contains("#define FPT_MANDEL_HAS_FAKE_ORBIT 1"),
                "fake light orbit evaluator unavailable for this formula"
            );
            writeln!(
                table,
                "constant float3 mandelFakeTrap=float3({:.9e}f,{:.9e}f,{:.9e}f);",
                fake.trap[0], fake.trap[1], fake.trap[2]
            )?;
            writeln!(
                table,
                "constant uint mandelFakeFirst={}u, mandelFakeLast={}u, mandelFakeChannels={}u;",
                fake.first,
                fake.last,
                fake.colors.len()
            )?;
            writeln!(
                table,
                "constant bool mandelFakeRelative={};\nconstant float mandelFakeIntensity={:.9e}f;",
                fake.relative, fake.intensity
            )?;
            writeln!(table, "constant bool mandelFakeIndirect={};", fake.indirect)?;
            writeln!(
                table,
                "constant float3 mandelFakeColors[{}]={{",
                fake.colors.len()
            )?;
            for c in &fake.colors {
                writeln!(table, "float3({:.9e}f,{:.9e}f,{:.9e}f),", c[0], c[1], c[2])?;
            }
            table.push_str("};\n");
        }
        let source = source.replace(MARKER, &format!("{table}\n{MARKER}"));
        Ok(if self.fake.is_some() {
            format!("#define FPT_MANDEL_FAKE_LIGHTS 1\n{source}")
        } else {
            source
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scene(extra: &str) -> MandelbulberScene {
        let source = include_str!("../../scenes/mandelbulber/ifs-20.fract");
        MandelbulberScene::parse(
            &source.replace("[main_parameters]", &format!("[main_parameters]\n{extra}")),
        )
        .unwrap()
    }
    #[test]
    fn disabled_and_unsupported_lights_are_not_reinterpreted() {
        let scene = scene(
            "light2_enabled false;\nlight3_enabled true;\nlight3_type spot;\nlight4_enabled true;\nlight4_type directional;",
        );
        let lights = AuxiliaryLighting::load(&scene).unwrap();
        assert!(lights.directional.is_empty());
        assert_eq!(lights.unsupported.len(), 2);
        assert_eq!(lights.specialize("unchanged").unwrap(), "unchanged");
    }
    #[test]
    fn directional_table_is_ordered_and_preserves_color_flags_and_scale() {
        let scene = scene(
            "light9_enabled true;\nlight9_type directional;\nlight9_relative_position true;\nlight2_enabled true;\nlight2_type 0;\nlight2_relative_position true;\nlight2_rotation -45 45 0;\nlight2_color ffff 0000 8000;\nlight2_intensity 2;\nall_lights_intensity 3;\nlight2_cast_shadows false;\nlight2_penetrating false;",
        );
        let lights = AuxiliaryLighting::load(&scene).unwrap();
        assert_eq!(
            lights.directional.iter().map(|v| v.id).collect::<Vec<_>>(),
            [2, 9]
        );
        let a = &lights.directional[0];
        assert_eq!(a.intensity, 6.0);
        assert_eq!(a.color, [1.0, 0.0, 32768.0 / 65535.0]);
        assert!(!a.cast_shadows && !a.penetrating);
        assert!(a.camera_direction[0] < -0.49 && a.camera_direction[1] > 0.70);
        let source = lights.specialize(MARKER).unwrap();
        assert!(source.contains("constant uint mandelAuxDirectionalCount = 2u;"));
        assert_eq!(source, lights.specialize(MARKER).unwrap());
        assert!(lights.specialize("no marker").is_err());
    }

    #[test]
    fn point_sources_keep_native_position_attenuation_and_global_scale() {
        let lights = AuxiliaryLighting::load(&scene(
            "light2_enabled true;\nlight2_position 1 2 3;\nlight2_color ffff 0000 0000;\nlight2_intensity 2;\nall_lights_intensity 3;\nlight2_decayFunction 1/r3;\nlight2_cast_shadows false;",
        )).unwrap();
        assert_eq!(lights.point.len(), 1);
        let point = &lights.point[0];
        assert_eq!(point.position, [1.0, 3.0, 2.0]);
        assert_eq!(point.intensity, 6.0);
        assert_eq!(point.decay_power, 3);
        assert!(!point.cast_shadows);
        assert!(lights.unsupported.is_empty());
        assert!(
            lights
                .specialize(MARKER)
                .unwrap()
                .contains("FPT_MANDEL_AUX_POINT")
        );
    }

    #[test]
    fn fake_lights_keep_authored_channels_and_explicit_gi_policy() {
        let base = "fake_lights_enabled true;\nfake_lights_orbit_trap 0 0 4;\nfake_lights_min_iter 9;\nfake_lights_max_iter 200;\nfake_lights_color_2_enabled true;\nfake_lights_color_3_enabled true;";
        let lights = AuxiliaryLighting::load(&scene(base)).unwrap();
        let fake = lights.fake.as_ref().unwrap();
        assert_eq!(fake.trap, [0.0, 0.0, 4.0]);
        assert_eq!((fake.first, fake.last, fake.colors.len()), (9, 200, 3));
        assert!(!fake.indirect);
        assert!(lights.specialize(MARKER).is_err());
        let source = lights
            .specialize(&format!("#define FPT_MANDEL_HAS_FAKE_ORBIT 1\n{MARKER}"))
            .unwrap();
        assert!(source.starts_with("#define FPT_MANDEL_FAKE_LIGHTS 1\n"));
        assert!(source.contains("mandelFakeIndirect=false"));
        let gi = AuxiliaryLighting::load(&scene(&format!("{base}\nMC_global_illumination true;")))
            .unwrap();
        assert!(gi.fake.unwrap().indirect);
        assert!(
            AuxiliaryLighting::load(&scene(
                "fake_lights_enabled true;\nfake_lights_min_iter -1;"
            ))
            .is_err()
        );
    }
}
