//! Scene-specialized water primitives. The evaluator is imported at runtime
//! from the external GPL source tree, just like fractal formula evaluators.
use super::compiler::{TokenKind, lex};
use super::{
    MandelbulberScene, parameter_bool, parameter_number, parameter_u32, parameter_vec3,
    rotation2_matrix,
};
use anyhow::{Context, Result, anyhow, ensure};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt::Write, fs, path::Path};

fn number(value: f64) -> Result<String> {
    ensure!(
        (value as f32).is_finite(),
        "water parameter exceeds finite Metal range"
    );
    Ok(format!("{:.9e}f", value as f32))
}

fn vector(value: [f64; 3]) -> Result<String> {
    Ok(format!(
        "float3({}, {}, {})",
        number(value[0])?,
        number(value[1])?,
        number(value[2])?
    ))
}

fn enabled(values: &BTreeMap<String, String>) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for key in values.keys() {
        if let Some(prefix) = key.strip_suffix("_enabled") {
            if prefix.starts_with("primitive_water_") && parameter_bool(values, key, false)? {
                result.push(prefix.to_owned());
            }
        }
    }
    result
        .sort_by_key(|p| parameter_u32(values, &format!("{p}_calculation_order"), 1).unwrap_or(1));
    Ok(result)
}

fn constant(values: &BTreeMap<String, String>, prefix: &str, index: usize) -> Result<String> {
    let key = |name: &str| format!("{prefix}_{name}");
    let n = |name: &str, fallback| parameter_number(values, &key(name), fallback);
    let b = |name: &str| parameter_bool(values, &key(name), false);
    ensure!(
        parameter_u32(values, &key("boolean_operator"), 1)? == 1,
        "{prefix}: only OR is supported"
    );
    ensure!(
        !b("smooth_de_combine_enable")?,
        "{prefix}: smooth combination is not supported"
    );
    // Object-coupled waves depend on the interleaved order of every primitive.
    ensure!(
        !b("wave_from_objects_enable")?,
        "{prefix}: object-coupled waves require ordered primitive support"
    );
    parameter_u32(values, &key("calculation_order"), 1)?;
    ensure!(
        (n("length", 0.1)? as f32) > 0.0,
        "{prefix}: length must be positive in Metal precision"
    );
    ensure!(
        n("relative_amplitude", 0.2)? >= 0.0 && n("wall_thickness", 0.0)? >= 0.0,
        "{prefix}: amplitude and wall thickness must be non-negative"
    );
    let iterations = parameter_u32(values, &key("iterations"), 5)?;
    ensure!(
        (1..=64).contains(&iterations),
        "{prefix}: iterations must be 1..64"
    );
    let rotation =
        rotation2_matrix(parameter_vec3(values, &key("rotation"), [0.0; 3])?.map(f64::to_radians));
    let columns: [[f64; 3]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| rotation[j][i]));
    let minimum = parameter_vec3(values, &key("limits_min"), [-0.7; 3])?;
    let maximum = parameter_vec3(values, &key("limits_max"), [0.7; 3])?;
    ensure!(
        (0..3).all(|i| minimum[i] <= maximum[i]),
        "{prefix}: invalid limits"
    );
    Ok(format!(
        "constant FptWaterPrimitive kFptWater{index} = {{ {{ {}, float3x3({}, {}, {}), {} }}, {{ {{ {}, {}, {}, {}, {}, {}, false, 0.5f, {}, {}, {}, {} }} }} }};\n",
        vector(parameter_vec3(values, &key("position"), [0.0; 3])?)?,
        vector(columns[0])?,
        vector(columns[1])?,
        vector(columns[2])?,
        number(n("wall_thickness", 0.0)?)?,
        number(n("length", 0.1)?)?,
        number(n("relative_amplitude", 0.2)?)?,
        number(n("anim_speed", 1.0)?)?,
        number(n("anim_progression_speed", 1.0)?)?,
        number(parameter_number(values, "frame_no", 0.0)?)?,
        iterations,
        b("empty")?,
        b("limits_enable")?,
        vector(minimum)?,
        vector(maximum)?
    ))
}

fn imported_function(source: &str) -> Result<String> {
    let tokens = lex(source)?;
    let name = tokens
        .iter()
        .position(|t| t.kind == TokenKind::Identifier && t.text == "PrimitiveWater")
        .ok_or_else(|| anyhow!("external primitives.cl has no PrimitiveWater function"))?;
    let opening = (name..tokens.len())
        .find(|&i| tokens[i].text == "{")
        .context("water body missing")?;
    let mut depth = 0;
    let mut closing = None;
    for (i, token) in tokens.iter().enumerate().skip(opening) {
        if token.text == "{" {
            depth += 1;
        }
        if token.text == "}" {
            depth -= 1;
            if depth == 0 {
                closing = Some(i);
                break;
            }
        }
    }
    let closing = closing.context("water body unbalanced")?;
    let body = &source[tokens[opening].offset..tokens[closing].offset + 1];
    // This overload preserves the external evaluator's row/column contract.
    let body = body.replace("Matrix33MulFloat3", "fptWaterRotate");
    Ok(format!(
        "static float3 fptWaterRotate(float3x3 m, float3 p) {{ return m * p; }}\nstatic float fptImportedWater(constant FptWaterPrimitive *primitive, float3 _point, float distanceFromAnother) {body}\n"
    ))
}

pub(super) fn specialize(base: &str, scene: &MandelbulberScene, root: &Path) -> Result<String> {
    let waters = enabled(&scene.main_parameters)?;
    if waters.is_empty() {
        return Ok(base.to_owned());
    }
    for field in ["all_primitives_position", "all_primitives_rotation"] {
        ensure!(
            parameter_vec3(&scene.main_parameters, field, [0.0; 3])? == [0.0; 3],
            "water: {field} requires grouped primitive transform support"
        );
    }
    let source_path = root.join("opencl/engines/primitives.cl");
    let source = fs::read_to_string(&source_path)
        .with_context(|| format!("reading {}", source_path.display()))?;
    let mut output = format!(
        "// Water evaluator imported from Mandelbulber GPL primitives.cl; SHA256 {:x}\n#define FPT_MANDEL_WATER 1\n",
        Sha256::digest(source.as_bytes())
    );
    output.push_str(
        r#"
struct FptWaterObject { float3 position; float3x3 rotationMatrix; float wallThickness; };
struct FptWaterData {
    float length, relativeAmplitude, animSpeed, animProgressionSpeed, animFrame;
    int iterations; bool waveFromObjectsEnable; float waveFromObjectsRelativeAmplitude;
    bool empty, limitsEnable; float3 limitsMin, limitsMax;
};
struct FptWaterPayload { FptWaterData water; };
struct FptWaterPrimitive { FptWaterObject object; FptWaterPayload data; };
static float mandelbulberPrimitiveUnionDistance(float3 p, constant FptRenderConfig &cfg);
"#,
    );
    output.push_str(&imported_function(&source)?);
    let mut distances = String::new();
    let mut materials = String::new();
    for (index, prefix) in waters.iter().enumerate() {
        output.push_str(&constant(&scene.main_parameters, prefix, index)?);
        let id = parameter_u32(&scene.main_parameters, &format!("{prefix}_material_id"), 1)?;
        writeln!(
            output,
            "static float fptWater{index}(float3 p, float distance) {{ float d = fptImportedWater(&kFptWater{index}, p, distance);\n#ifdef FPT_MANDEL_PERLIN\n    d = fptPerlinDisplace(d, p, {id}u);\n#endif\n    return d; }}"
        )?;
        writeln!(
            distances,
            "    distance = min(distance, fptWater{index}(source, distance / world_scale) * world_scale);"
        )?;
        let material = scene
            .materials
            .get(&id)
            .with_context(|| format!("{prefix}: material {id} is missing"))?;
        // Native SurfaceColour uses fixed colour for primitive objects even
        // when the material's fractal-palette switch is enabled.
        writeln!(
            materials,
            "    {{ float candidate = fptWater{index}(source, distance / world_scale) * world_scale; if (candidate < distance) {{ distance = candidate;"
        )?;
        writeln!(
            materials,
            "        material.rgb = {}; material.roughness = {}; material.specular = {}; material.translucency = {}; material.ior = {}; material.emission = {}; }} }}",
            vector(material.surface_color.map(f64::from))?,
            number(material.surface_roughness.max(0.0).sqrt().clamp(0.0, 1.0))?,
            number(
                (material.specular / 10.0)
                    .max(material.metallic)
                    .max(material.reflectance)
                    .clamp(0.0, 1.0)
            )?,
            number(material.transparency_of_surface.clamp(0.0, 1.0))?,
            number(material.index_of_refraction.max(1.0))?,
            number(material.luminosity.max(0.0))?
        )?;
    }
    writeln!(
        output,
        "static float mandelbulberWaterDistance(float3 p, float distance, constant FptRenderConfig &cfg) {{\n    float world_scale = max(setv(cfg,0),1.0f); float3 source = p.xzy / world_scale;\n{distances}    return distance;\n}}"
    )?;
    writeln!(
        output,
        "static Material mandelbulberWaterMaterial(float3 p, constant FptRenderConfig &cfg, Material material) {{\n    float world_scale = max(setv(cfg,0),1.0f); float3 source = p.xzy / world_scale;\n    float distance = mandelbulberGeneratedFieldSample(mandelbulberGlobalPoint(p,cfg),cfg,1,0).x;\n    if (cfg.sdf_flat_union_count > 0u) distance = min(distance,mandelbulberPrimitiveUnionDistance(p,cfg));\n#ifdef FPT_MANDEL_BOX\n    distance = mandelbulberBoxDistance(p, distance, cfg);\n#endif\n{materials}    return material;\n}}"
    )?;
    let marker = "#define FPT_MANDEL_GENERATED_MATERIAL 1";
    ensure!(
        base.contains(marker),
        "missing generated material insertion point"
    );
    Ok(base.replacen(marker, &format!("{output}\n{marker}"), 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scenes_without_water_do_not_read_upstream_or_change_shader() {
        let scene = MandelbulberScene::parse(
            "# Mandelbulber settings file\n# version 2.33\n[main_parameters]\nformula_1 1;\n",
        )
        .unwrap();
        assert_eq!(
            specialize("unchanged", &scene, Path::new("/nonexistent")).unwrap(),
            "unchanged"
        );
    }
    #[test]
    fn imports_only_balanced_water_body() {
        let function = imported_function("float Other() {return 0;} float PrimitiveWater(void) { if (true) {return 1;} return 2;} float Next() {return 3;}").unwrap();
        assert!(function.contains("return 2;"));
        assert!(!function.contains("return 3;"));
    }
    #[test]
    fn validates_active_water_and_decimal_comma_values() {
        let mut values = BTreeMap::from([
            ("primitive_water_2_enabled".into(), "true".into()),
            ("primitive_water_2_length".into(), "0,002".into()),
        ]);
        assert_eq!(enabled(&values).unwrap(), vec!["primitive_water_2"]);
        assert!(
            constant(&values, "primitive_water_2", 0)
                .unwrap()
                .contains("kFptWater0")
        );
        values.insert("primitive_water_2_length".into(), "0".into());
        assert!(constant(&values, "primitive_water_2", 0).is_err());
        values.insert("primitive_water_2_length".into(), "0.1".into());
        values.insert(
            "primitive_water_2_wave_from_objects_enable".into(),
            "true".into(),
        );
        assert!(constant(&values, "primitive_water_2", 0).is_err());
    }
}
