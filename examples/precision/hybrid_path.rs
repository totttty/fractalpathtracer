//! Fail-closed, source-pinned integration experiment for scene 578.
use anyhow::{Context, Result, ensure};
use fpt_metal::mandelbulber::MandelbulberScene;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

pub const SCENE_SHA: &str = "ed6528d2335f47f90415a3d3874c0f93c78fd073f9cebbd56bbd502a7ab38a13";
const FORMULAS: [(&str, &str, &str); 2] = [
    (
        "fractal_menger_sponge.cpp",
        "cFractalMengerSponge",
        "2600bae0c2aa9ee9cf0fdcde30286fde7899b321b0cb6894db661427f489f358",
    ),
    (
        "fractal_transf_abs_add_tglad_fold4d.cpp",
        "cFractalTransfAbsAddTgladFold4d",
        "c6e34e279c598956b337dde92d5c9ce2636c124d5f89ae542d87566a6f009999",
    ),
];

fn scalar(v: f64) -> String {
    let hi = v as f32;
    let lo = (v - f64::from(hi)) as f32;
    format!("R({hi:.9e}f,{lo:.9e}f)")
}

fn vector(values: &[f64]) -> String {
    format!(
        "V({})",
        values
            .iter()
            .map(|v| scalar(*v))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn once(source: &str, old: &str, new: &str) -> Result<String> {
    ensure!(
        source.matches(old).count() == 1,
        "hybrid precision marker changed: {old}"
    );
    Ok(source.replacen(old, new, 1))
}

fn function<'a>(source: &'a str, marker: &str) -> Result<&'a str> {
    let start = source.find(marker).context("missing precision function")?;
    let body = start + source[start..].find('{').context("missing function body")?;
    let mut depth = 0;
    for (i, c) in source[body..].char_indices() {
        if c == '{' {
            depth += 1;
        }
        if c == '}' {
            depth -= 1;
            if depth == 0 {
                return Ok(&source[start..body + i + 1]);
            }
        }
    }
    anyhow::bail!("unclosed precision function")
}

fn import_formulas(root: &Path, limits: &[f64]) -> Result<String> {
    let mut replacements = BTreeMap::from([
        ("transformCommon.scale3".to_owned(), scalar(3.0)),
        (
            "transformCommon.additionConstant0000".to_owned(),
            vector(limits),
        ),
        (
            "foldColor.auxColorEnabledFalse".to_owned(),
            "false".to_owned(),
        ),
        ("foldColor.startIterationsA".to_owned(), "0".to_owned()),
        ("foldColor.stopIterationsA".to_owned(), "250".to_owned()),
        (
            "transformCommon.functionEnabledCxFalse".to_owned(),
            "false".to_owned(),
        ),
    ]);
    for (axis, iteration) in "xyzw".chars().zip("ABCD".chars()) {
        for (key, value) in [
            (format!("transformCommon.functionEnabledA{axis}"), "true"),
            (format!("transformCommon.startIterations{iteration}"), "0"),
            (format!("transformCommon.stopIterations{iteration}"), "250"),
            (format!("foldColor.difs0000.{axis}"), "R(0)"),
            (format!("mandelbox.color.factor4D.{axis}"), "R(0)"),
        ] {
            replacements.insert(key, value.to_owned());
        }
    }
    let mut result = String::new();
    for (slot, (file, class, digest)) in FORMULAS.iter().enumerate() {
        let source = fs::read_to_string(root.join("formula/definition").join(file))?;
        ensure!(
            format!("{:x}", Sha256::digest(source.as_bytes())) == *digest,
            "unsupported native formula revision: {file}"
        );
        let original = function(&source, &format!("void {class}::FormulaCode("))?;
        let body = &original[original.find('{').context("missing native body")?..];
        let body = substitute_parameters(body, &replacements)?;
        let body = body.replace("CVector4 ", "V ").replace("double ", "R ");
        // Preserve upstream notices; the generated GPL source stays in local reports.
        result.push_str(source.split("#include").next().context("missing notice")?);
        result.push_str(&format!(
            "\nvoid formula{slot}(thread V &z,thread Aux &aux) {body}\n"
        ));
    }
    Ok(result)
}

fn substitute_parameters(source: &str, values: &BTreeMap<String, String>) -> Result<String> {
    let mut rest = source;
    let mut result = String::new();
    while let Some(start) = rest.find("fractal->") {
        result.push_str(&rest[..start]);
        rest = &rest[start + 9..];
        let end = rest
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '.')
            .unwrap_or(rest.len());
        let key = &rest[..end];
        result.push_str(
            values
                .get(key)
                .with_context(|| format!("unmapped parameter: {key}"))?,
        );
        rest = &rest[end..];
    }
    result.push_str(rest);
    Ok(result)
}

pub fn helper(scene_path: &Path, root: &Path) -> Result<String> {
    ensure!(
        format!("{:x}", Sha256::digest(fs::read(scene_path)?)) == SCENE_SHA,
        "two-term hybrid experiment supports only the pinned scene 578"
    );
    let scene = MandelbulberScene::load(scene_path)?;
    let limits = scene.formula_slots[1]
        .parameters
        .get("transf_addition_constant_0000")
        .context("missing Tglad limits")?
        .split_whitespace()
        .map(|s| s.replace(',', ".").parse::<f64>())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(
        limits.len() == 4 && limits.iter().all(|v| v.is_finite()),
        "invalid Tglad limits"
    );
    let mut template = include_str!("HybridPath.metal").to_owned();
    for (key, value) in [
        ("@FORMULAS@", import_formulas(root, &limits)?),
        ("@ORIGIN@", vector(&scene.camera)),
        ("@MIN_THRESHOLD@", scalar(1e-12)),
        ("@VIEW_MAX@", scalar(13.68766681086107)),
        ("@REFINE_RATIO@", scalar(0.999)),
        ("@ORBIT_EPS@", scalar(1e-15)),
    ] {
        template = template.replace(key, &value);
    }
    ensure!(
        ![
            "@FORMULAS@",
            "@ORIGIN@",
            "@MIN_THRESHOLD@",
            "@VIEW_MAX@",
            "@REFINE_RATIO@",
            "@ORBIT_EPS@"
        ]
        .iter()
        .any(|key| template.contains(key)),
        "unexpanded precision template"
    );
    Ok(format!(
        "{}\nnamespace fpt_hybrid {{\n{template}\n}}\n",
        include_str!("TwoTerm.metal")
    ))
}

pub fn specialize(source: &str, helper: &str) -> Result<String> {
    for feature in [
        "#define FPT_MANDEL_AUX_POINT",
        "#define FPT_MANDEL_AUX_DIRECTIONAL",
        "#define FPT_MANDEL_FAKE_LIGHTS",
        "#define FPT_MANDEL_INTERIOR",
        "#define FPT_MANDEL_GENERATED_AMBIENT",
    ] {
        ensure!(
            !source.contains(feature),
            "unsupported precise path: {feature}"
        );
    }
    for marker in [
        "constant int kMandelColorAlgorithm = 0;",
        "constant bool kMandelColorExtra = false;",
        "constant bool kMandelColorPreV215 = false;",
    ] {
        ensure!(
            source.contains(marker),
            "unsupported precise color orbit: {marker}"
        );
    }
    let palette = function(source, "static float mandelbulberGeneratedPalettePosition(")?;
    let palette = once(palette, "float3 p,", "fpt_hybrid::V p,")?;
    let palette = once(
        &palette,
        "mandelbulberGeneratedPalettePosition(",
        "precisePalette(",
    )?;
    let palette = once(
        &palette,
        "mandelbulberGeneratedColorIndex(\n        mandelbulberGlobalPoint(p, cfg), cfg)",
        "fpt_hybrid::value(fpt_hybrid::colorIndex(p))",
    )?;
    let material = function(source, "static Material mandelbulberGeneratedMaterial(")?;
    let material = once(material, "float3 p,", "fpt_hybrid::V p,")?;
    let material = once(
        &material,
        "mandelbulberGeneratedMaterial(",
        "preciseMaterial(",
    )?;
    let material = once(
        &material,
        "mandelbulberGeneratedPalettePosition(p, cfg)",
        "precisePalette(p, cfg)",
    )?;
    let original = function(source, "static float3 renderPath(")?;
    let mut body = original.to_owned();
    for (old, new) in [
        (
            "float3 rp = cam_pos;",
            "float3 rp = cam_pos;\n    auto precise_position=fpt_hybrid::camera();",
        ),
        (
            "MandelbulberMarchResult hit = marchMandelbulberSampled(dr, rp, local_ni, cfg, mandelPathStepSeed(xy, sample_idx, uint(i), 1u));\n            rp = hit.position;",
            "auto hit=fpt_hybrid::trace(precise_position,fpt_hybrid::nativeDirection(dr),local_ni,cfg,mandelPathStepSeed(xy,sample_idx,uint(i),1u));\n            if(hit.invalid) return float3(8,0,8);\n            precise_position=hit.position;\n            rp=fpt_hybrid::worldPosition(precise_position);",
        ),
        (
            "float travel_sq = dot(rp - cam_pos, rp - cam_pos);",
            "float travel=fpt_hybrid::value((precise_position-fpt_hybrid::camera()).Length())*1024.0f;\n        float travel_sq=travel*travel;",
        ),
        (
            "Material material = userSdf(rp, cfg).material;\n        float3 n = normalAt(rp, cfg);",
            "Material material=preciseMaterial(precise_position,cfg);\n        float3 n=fpt_hybrid::normal(precise_position,fpt_hybrid::thresholdAt(precise_position,cfg));\n        if(!all(isfinite(n)) || !all(isfinite(material.rgb))) return float3(8,0,8);",
        ),
        (
            "sunContributionWithSurface(rp, xy, frame, material, n, cfg)",
            "fpt_hybrid::sun(precise_position,n,material,cfg)",
        ),
        (
            "pixellight += authored_path ? pixelcolor * material.rgb * direct : direct;",
            "if(!all(isfinite(direct))) return float3(8,0,8);\n            pixellight += authored_path ? pixelcolor * material.rgb * direct : direct;",
        ),
        (
            "float bounce_offset = cfg.sdf_id == SDF_MANDELBULBER && authored_path\n            ? mandelbulberMarchThreshold(rp, cfg) : 0.001f;",
            "auto bounce_offset=fpt_hybrid::thresholdAt(precise_position,cfg);",
        ),
        (
            "rp += n * bounce_offset * sign(dot(dr, n));",
            "precise_position=precise_position+fpt_hybrid::nativeDirection(n)*(bounce_offset*fpt_hybrid::R(sign(dot(dr,n))));\n        rp=fpt_hybrid::worldPosition(precise_position);",
        ),
    ] {
        body = once(&body, old, new)?;
    }
    // A large finite sentinel survives multi-sample averaging. It is rejected
    // from linear output rather than silently displayed as a valid background.
    body = body
        .replace("float3(8,0,8)", "float3(1.0e20f,0,1.0e20f)")
        .replace("float3(8.0f, 0.0f, 8.0f)", "float3(1.0e20f,0,1.0e20f)");
    ensure!(
        !body.contains("userSdf(rp")
            && !body.contains("normalAt(rp")
            && !body.contains("mandelbulberMarchThreshold(rp")
            && !body.contains("marchMandelbulberSampled(dr, rp"),
        "rounded position leaked into precise geometry"
    );
    once(
        source,
        original,
        &format!("{helper}\n{palette}\n{material}\n{body}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn splits_before_rounding() {
        assert!(scalar(0.225).contains("e-9f"));
        let hi = 0.225f64 as f32;
        let lo = (0.225f64 - f64::from(hi)) as f32;
        assert!((f64::from(hi) + f64::from(lo) - 0.225).abs() < 1e-15);
    }
    #[test]
    fn patches_fail_closed() {
        assert!(once("a a", "a", "b").is_err());
        assert!(once("x", "a", "b").is_err());
        assert_eq!(
            function("f(){if(x){y();}} tail", "f()").unwrap(),
            "f(){if(x){y();}}"
        );
        assert!(function("f(){", "f()").is_err());
        assert!(specialize("unknown source", "").is_err());
        let values = BTreeMap::from([("x".to_owned(), "R(3)".to_owned())]);
        assert_eq!(
            substitute_parameters("a=fractal->x;", &values).unwrap(),
            "a=R(3);"
        );
        assert!(substitute_parameters("fractal->xx", &values).is_err());
    }
    #[test]
    fn color_and_sampled_transport_contract() {
        let template = include_str!("HybridPath.metal");
        assert!(template.contains("mandelStepMultiplier(seed)"));
        assert!(template.contains("aux.color*R(100)"));
        assert!(template.contains("*R(5000)"));
        assert!(!template.contains("ambientVisibility"));
        assert!(!template.contains("mapSdf("));
        assert!(!template.contains("userSdf("));
    }
}
