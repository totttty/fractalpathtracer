//! Opt-in source specialization for one precision diagnostic, never production.
use anyhow::{Context, Result, ensure};

fn once(source: &str, old: &str, new: &str) -> Result<String> {
    ensure!(
        source.matches(old).count() == 1,
        "precision patch marker changed: {old}"
    );
    Ok(source.replacen(old, new, 1))
}

fn function<'a>(source: &'a str, signature: &str) -> Result<&'a str> {
    let start = source
        .find(signature)
        .context("missing precision function marker")?;
    let body = start + source[start..].find('{').context("missing body")?;
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

pub fn specialize(source: &str, helper: &str, headlight: bool) -> Result<String> {
    for feature in [
        "#define FPT_MANDEL_AUX_POINT",
        "#define FPT_MANDEL_AUX_DIRECTIONAL",
        "#define FPT_MANDEL_FAKE_LIGHTS",
    ] {
        ensure!(
            !source.contains(feature),
            "unsupported precise light path: {feature}"
        );
    }
    ensure!(
        source.contains("constant int kMandelColorAlgorithm = 0;")
            && source.contains("constant bool kMandelColorExtra = false;"),
        "unsupported precision color orbit: {}",
        source
            .lines()
            .filter(|l| l.contains("kMandelColorPreV215 =")
                || l.contains("kMandelColorAlgorithm =")
                || l.contains("kMandelColorExtra ="))
            .collect::<Vec<_>>()
            .join("; ")
    );
    let palette = function(source, "static float mandelbulberGeneratedPalettePosition(")?;
    let palette = once(palette, "float3 p,", "fpt_ifs::V p,")?;
    let palette = once(
        &palette,
        "mandelbulberGeneratedPalettePosition(",
        "precisePalette(",
    )?;
    let palette = once(
        &palette,
        "mandelbulberGeneratedColorIndex(\n        mandelbulberGlobalPoint(p, cfg), cfg)",
        "fpt_ifs::value(fpt_ifs::colorIndex(p))",
    )?;
    let material = function(source, "static Material mandelbulberGeneratedMaterial(")?;
    let material = once(material, "float3 p,", "fpt_ifs::V p,")?;
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
    body = once(
        &body,
        "float3 rp = cam_pos;",
        "float3 rp = cam_pos;\n    fpt_ifs::V precise_position=fpt_ifs::camera();",
    )?;
    body = once(
        &body,
        "MandelbulberMarchResult hit = marchMandelbulber(dr, rp, local_ni, cfg);\n            rp = hit.position;",
        "auto hit=fpt_ifs::trace(precise_position,fpt_ifs::nativeDirection(dr),local_ni,cfg);\n            if(hit.exhausted) return float3(NAN);\n            precise_position=hit.position;\n            rp=fpt_ifs::worldPosition(precise_position);",
    )?;
    body = once(
        &body,
        "float travel_sq = dot(rp - cam_pos, rp - cam_pos);",
        "float travel=fpt_ifs::value((precise_position-fpt_ifs::camera()).Length())*1024.0f;\n        float travel_sq=travel*travel;",
    )?;
    body = once(
        &body,
        "Material material = userSdf(rp, cfg).material;\n        float3 n = normalAt(rp, cfg);",
        "Material material=preciseMaterial(precise_position,cfg);\n        float3 n=fpt_ifs::normal(precise_position,fpt_ifs::thresholdAt(precise_position,cfg));",
    )?;
    if headlight {
        body = once(
            &body,
            "Material material=preciseMaterial(precise_position,cfg);",
            "Material material=defaultMaterial();",
        )?;
        body = once(
            &body,
            "if (cfg.mandel_appearance_mode == 1u) {",
            "return float3(max(dot(n,-dr),0.0f));\n        if (cfg.mandel_appearance_mode == 1u) {",
        )?;
        body = once(
            &body,
            "if (missed || travel_sq > far_dist_sq) {",
            "if (missed || travel_sq > far_dist_sq) { return float3(0);",
        )?;
    }
    body = body.replace(
        "#if defined(FPT_MANDEL_GENERATED_AMBIENT)",
        "#if 0 // Precision control deliberately excludes AO.",
    );
    body = once(
        &body,
        "sunContributionWithSurface(rp, xy, frame, material, n, cfg)",
        "fpt_ifs::sun(precise_position,n,material,cfg)",
    )?;
    body = once(
        &body,
        "pixellight += authored_path ? pixelcolor * material.rgb * direct : direct;",
        "if(!all(isfinite(direct))) return float3(NAN);\n            pixellight += authored_path ? pixelcolor * material.rgb * direct : direct;",
    )?;
    body = once(
        &body,
        "rp += n * bounce_offset * sign(dot(dr, n));",
        "precise_position=precise_position+fpt_ifs::nativeDirection(n)*(fpt_ifs::thresholdAt(precise_position,cfg)*fpt_ifs::R(sign(dot(dr,n))));\n        rp=fpt_ifs::worldPosition(precise_position);",
    )?;
    // The old offset's computation is dead after substitution; remove it to
    // make accidental float32 threshold use evident in source review.
    body = once(
        &body,
        "float bounce_offset = cfg.sdf_id == SDF_MANDELBULBER && authored_path\n            ? mandelbulberMarchThreshold(rp, cfg) : 0.001f;",
        "",
    )?;
    ensure!(
        !body.contains("userSdf(rp")
            && !body.contains("normalAt(rp")
            && !body.contains("mandelbulberMarchThreshold(rp"),
        "rounded position leaked into geometry"
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
    fn markers_fail_closed() {
        assert!(once("a a", "a", "b").is_err());
        assert!(once("c", "a", "b").is_err());
        assert_eq!(once("x a", "a", "b").unwrap(), "x b");
    }
    #[test]
    fn balanced_function() {
        assert_eq!(
            function("prefix f(){ if(x){y();} } tail", "f()").unwrap(),
            "f(){ if(x){y();} }"
        );
        assert!(function("f(){", "f()").is_err());
    }
}
