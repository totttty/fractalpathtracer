//! Scene-specialized OR boxes, including zero-thickness boundary shells.
use super::{
    MandelbulberScene, parameter_bool, parameter_number, parameter_u32, parameter_vec3,
    rotation2_matrix,
};
use anyhow::{Context, Result, ensure};
use std::fmt::Write;

fn number(value: f64) -> Result<String> {
    ensure!(
        (value as f32).is_finite(),
        "box parameter exceeds finite Metal range"
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

pub(super) fn specialize(base: &str, scene: &MandelbulberScene) -> Result<String> {
    let values = &scene.main_parameters;
    let mut boxes = Vec::new();
    for key in values.keys() {
        if let Some(prefix) = key.strip_suffix("_enabled") {
            if prefix.starts_with("primitive_box_") && parameter_bool(values, key, false)? {
                boxes.push((
                    parameter_u32(values, &format!("{prefix}_calculation_order"), 1)?,
                    prefix,
                ));
            }
        }
    }
    if boxes.is_empty() {
        return Ok(base.to_owned());
    }
    boxes.sort();
    for field in ["all_primitives_position", "all_primitives_rotation"] {
        ensure!(
            parameter_vec3(values, field, [0.0; 3])? == [0.0; 3],
            "box: {field} requires grouped primitive transform support"
        );
    }
    let mut output = String::from(
        "#define FPT_MANDEL_BOX 1\nstatic float mandelbulberPrimitiveUnionDistance(float3 p, constant FptRenderConfig &cfg);\n",
    );
    let mut distances = String::new();
    let mut materials = String::new();
    for (index, (_, prefix)) in boxes.iter().enumerate() {
        let key = |field: &str| format!("{prefix}_{field}");
        let id = parameter_u32(values, &key("material_id"), 1)?;
        ensure!(
            parameter_u32(values, &key("boolean_operator"), 1)? == 1,
            "{prefix}: only OR is supported"
        );
        ensure!(
            !parameter_bool(values, &key("smooth_de_combine_enable"), false)?,
            "{prefix}: smooth combination is not supported"
        );
        ensure!(
            parameter_vec3(values, &key("repeat"), [0.0; 3])? == [0.0; 3],
            "{prefix}: repetition is not supported"
        );
        let size = parameter_vec3(values, &key("size"), [1.0; 3])?;
        ensure!(
            size.iter().all(|v| (*v as f32) > 0.0),
            "{prefix}: size must be positive"
        );
        let wall = parameter_number(values, &key("wall_thickness"), 0.0)?;
        let rounding = parameter_number(values, &key("rounding"), 0.0)?;
        ensure!(
            wall >= 0.0 && rounding >= 0.0,
            "{prefix}: negative thickness or rounding"
        );
        let rotation = rotation2_matrix(
            parameter_vec3(values, &key("rotation"), [0.0; 3])?.map(f64::to_radians),
        );
        let columns: [[f64; 3]; 3] =
            std::array::from_fn(|i| std::array::from_fn(|j| rotation[j][i]));
        writeln!(
            output,
            "static float fptBox{index}(float3 p) {{\n    float3 local = float3x3({}, {}, {}) * (p - {});\n    float3 q = abs(local) - {} * 0.5f;",
            vector(columns[0])?,
            vector(columns[1])?,
            vector(columns[2])?,
            vector(parameter_vec3(values, &key("position"), [0.0; 3])?)?,
            vector(size)?
        )?;
        if parameter_bool(values, &key("empty"), false)? {
            output.push_str("    float d = abs(max(q.x, max(q.y, q.z)));\n");
        } else {
            writeln!(
                output,
                "    float d = length(max(q, float3(0.0f))) - {};",
                number(rounding)?
            )?;
        }
        writeln!(output, "    d = max(d - {}, 0.0f);", number(wall)?)?;
        if parameter_bool(values, &key("limits_enable"), false)? {
            let lo = parameter_vec3(values, &key("limits_min"), [-0.7; 3])?;
            let hi = parameter_vec3(values, &key("limits_max"), [0.7; 3])?;
            ensure!((0..3).all(|i| lo[i] <= hi[i]), "{prefix}: invalid limits");
            writeln!(
                output,
                "    float3 limit = max(local - {}, {} - local);\n    d = max(d, max(limit.x, max(limit.y, limit.z)));",
                vector(hi)?,
                vector(lo)?
            )?;
        }
        writeln!(
            output,
            "#ifdef FPT_MANDEL_PERLIN\n    d = fptPerlinDisplace(d, p, {id}u);\n#endif\n    return d;\n}}"
        )?;
        writeln!(
            distances,
            "    distance = min(distance, fptBox{index}(source) * world_scale);"
        )?;
        let material = scene
            .materials
            .get(&id)
            .with_context(|| format!("{prefix}: material {id} is missing"))?;
        writeln!(
            materials,
            "    {{ float candidate = fptBox{index}(source) * world_scale; if (candidate < distance) {{ distance = candidate;\n        material.rgb = {}; material.roughness = {}; material.specular = {}; material.translucency = {}; material.ior = {}; material.emission = 0.0f;{} }} }}",
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
            match scene.primitive_emission_override(material) {
                Some(emission) => format!(
                    " material.emission_rgb = {};",
                    vector(emission.map(f64::from))?
                ),
                None => String::new(),
            }
        )?;
    }
    writeln!(
        output,
        "static float mandelbulberBoxDistance(float3 p, float distance, constant FptRenderConfig &cfg) {{\n    float world_scale = max(setv(cfg,0),1.0f); float3 source = p.xzy / world_scale;\n{distances}    return distance;\n}}"
    )?;
    writeln!(
        output,
        "static Material mandelbulberBoxMaterial(float3 p, constant FptRenderConfig &cfg, Material material) {{\n    float world_scale = max(setv(cfg,0),1.0f); float3 source = p.xzy / world_scale;\n    float distance = mandelbulberGeneratedFieldSample(mandelbulberGlobalPoint(p,cfg),cfg,1,0).x;\n    if (cfg.sdf_flat_union_count > 0u) distance = min(distance,mandelbulberPrimitiveUnionDistance(p,cfg));\n{materials}    return material;\n}}"
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
    fn enabled_empty_box_is_not_silently_omitted() {
        let scene = MandelbulberScene::parse(
            "# Mandelbulber settings file\n# version 2.33\n[main_parameters]\nformula_1 1;\nmat1_is_defined true;\nprimitive_box_2_enabled true;\nprimitive_box_2_empty true;\nprimitive_box_2_size 10 10 0,15;\n",
        ).unwrap();
        let result = specialize("#define FPT_MANDEL_GENERATED_MATERIAL 1", &scene).unwrap();
        assert!(result.contains("#define FPT_MANDEL_BOX 1"));
        assert!(result.contains("fptBox0"));
        assert!(result.contains("abs(max(q.x, max(q.y, q.z)))"));
    }

    #[test]
    fn absent_boxes_preserve_source_and_unsupported_combinations_fail() {
        let source = "# Mandelbulber settings file\n# version 2.33\n[main_parameters]\nformula_1 1;\nmat1_is_defined true;\n";
        let scene = MandelbulberScene::parse(source).unwrap();
        assert_eq!(specialize("unchanged", &scene).unwrap(), "unchanged");
        for parameter in [
            "boolean_operator 2",
            "repeat 1 0 0",
            "size 1 0 1",
            "smooth_de_combine_enable true",
        ] {
            let scene = MandelbulberScene::parse(&format!(
                "{source}primitive_box_2_enabled true;\nprimitive_box_2_{parameter};\n"
            ))
            .unwrap();
            assert!(
                specialize("#define FPT_MANDEL_GENERATED_MATERIAL 1", &scene).is_err(),
                "{parameter}"
            );
        }
    }
}
