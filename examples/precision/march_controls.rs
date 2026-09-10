//! Source-scoped primary-march experiments. Never used by production rendering.
use anyhow::{Context, Result, ensure};

pub fn deterministic_path(source: &str) -> Result<String> {
    let sampled = "marchMandelbulberSampled(dr, rp, local_ni, cfg, mandelPathStepSeed(xy, sample_idx, uint(i), 1u))";
    if !source.contains("static MandelbulberMarchResult marchMandelbulberSampled(") {
        return Ok(source.to_string());
    }
    if !source.contains(sampled)
        && source
            .matches("marchMandelbulber(dr, rp, local_ni, cfg)")
            .count()
            == 1
    {
        return Ok(source.to_string());
    }
    ensure!(
        source.matches(sampled).count() == 1,
        "production sampled call changed"
    );
    Ok(source.replace(sampled, "marchMandelbulber(dr, rp, local_ni, cfg)"))
}

pub fn specialize(source: &str, seed: Option<u32>, dynamic_threshold: bool) -> Result<String> {
    ensure!(
        seed.is_none_or(|v| v <= 2_147_483_646),
        "invalid step-jitter seed"
    );
    let start = source
        .find("static MandelbulberMarchResult marchMandelbulber(")
        .context("missing production primary marcher")?;
    let end = start
        + source[start..]
            .find("\nstatic float3 march(")
            .context("missing primary marcher end")?;
    let mut marcher = source[start..end].to_string();
    let mut prefix = String::new();
    if let Some(seed) = seed {
        let step_start = source
            .find("static float sdfMarchStep(")
            .context("missing step helper")?;
        let step_end = step_start
            + source[step_start..]
                .find("\nstatic bool sdfMarchConverged(")
                .context("missing step helper end")?;
        prefix = source[step_start..step_end].replace("sdfMarchStep(", "probeJitterStep(");
        let signature = "constant FptRenderConfig &cfg) {";
        ensure!(
            prefix.matches(signature).count() == 1,
            "step signature changed"
        );
        prefix = prefix.replace(
            signature,
            "constant FptRenderConfig &cfg, thread uint &seed) {",
        );
        let factor = "cfg.vset_values[113];";
        ensure!(
            prefix.matches(factor).count() == 2,
            "step factor branches changed"
        );
        prefix = prefix.replace(factor, "cfg.vset_values[113] * probeStepMultiplier(seed);");
        // Park-Miller recurrence, evaluated in integer arithmetic before the
        // floating step. Seed zero is the explicit no-jitter control.
        prefix.insert_str(
            0,
            r#"
static float probeStepMultiplier(thread uint &seed) {
    seed = uint((ulong(seed) * 16807ul) % 2147483647ul);
    return 1.0f - float(seed % 1001u) / 10000.0f;
}

"#,
        );
        marcher = marcher.replace(
            "    float3 start = position;",
            &format!("    uint probe_seed = {seed}u;\n    float3 start = position;"),
        );
        let step_call = "step = sdfMarchStep(distance, threshold, cfg);";
        ensure!(
            marcher.matches(step_call).count() == 1,
            "primary step call changed"
        );
        marcher = marcher.replace(
            step_call,
            "step = probeJitterStep(distance, threshold, cfg, probe_seed);",
        );
    }
    if dynamic_threshold {
        let refinement_start = marcher
            .find("    for (int refinement = 0;")
            .context("missing refinement loop")?;
        let (primary, refinement) = marcher.split_at(refinement_start);
        let evaluation = "        distance = mapSdf(position, cfg);";
        ensure!(
            refinement.matches(evaluation).count() == 1,
            "refinement evaluation changed"
        );
        marcher = primary.to_string() + &refinement.replace(evaluation,
            "        threshold = mandelbulberMarchThreshold(position, cfg);\n        distance = mapSdf(position, cfg);");
    }
    Ok(source[..start].to_string() + &prefix + &marcher + &source[end..])
}

/// Seed a separate march stream from pixel coordinates, sample, bounce and a
/// fixed event salt. Other path random dimensions and non-path queries stay put.
pub fn dimensioned(source: &str, experiment_seed: u32, dynamic_threshold: bool) -> Result<String> {
    let deterministic = deterministic_path(source)?;
    let source = deterministic.as_str();
    let seeded = specialize(source, Some(0), dynamic_threshold)?;
    let start = seeded
        .find("static float probeStepMultiplier(")
        .context("missing seeded helper")?;
    let end = start
        + seeded[start..]
            .find("\nstatic float3 march(")
            .context("missing seeded end")?;
    let mut helper = seeded[start..end].replace("marchMandelbulber(", "probeDimensionedMarch(");
    let signature = "constant FptRenderConfig &cfg) {";
    ensure!(
        helper.matches(signature).count() == 1,
        "seeded marcher signature changed"
    );
    helper = helper.replace(
        signature,
        "constant FptRenderConfig &cfg, uint initial_seed) {",
    );
    helper = helper.replace("uint probe_seed = 0u;", "uint probe_seed = initial_seed;");
    helper.push_str(
        r#"
static uint probeMixBits(uint x) {
    x ^= x >> 16u;
    x *= 0x7feb352du;
    x ^= x >> 15u;
    x *= 0x846ca68bu;
    return x ^ (x >> 16u);
}
static uint probePathStepSeed(float2 xy, uint sample, uint bounce, uint experiment) {
    uint h = probeMixBits(as_type<uint>(xy.x) ^ 0x4d415243u);
    h = probeMixBits(h ^ as_type<uint>(xy.y));
    h = probeMixBits(h ^ sample);
    h = probeMixBits(h ^ bounce);
    return 1u + probeMixBits(h ^ experiment) % 2147483646u;
}
"#,
    );
    let base = specialize(source, None, dynamic_threshold)?;
    let start = base
        .find("static float3 renderPath(float2 xy, uint sample_idx,")
        .context("missing ordinary path integrator")?;
    let end = start
        + base[start + 1..]
            .find("\nstatic ")
            .context("missing path integrator end")?
        + 1;
    let call = "marchMandelbulber(dr, rp, local_ni, cfg)";
    let body = &base[start..end];
    ensure!(body.matches(call).count() == 1, "path march call changed");
    let body = body.replace(call, &format!(
        "probeDimensionedMarch(dr, rp, local_ni, cfg, probePathStepSeed(xy, sample_idx, uint(i), {experiment_seed}u))"));
    Ok(base[..start].to_string() + &helper + &body + &base[end..])
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = include_str!("../../shaders/Shaders.metal");

    #[test]
    fn no_controls_preserve_source() {
        assert_eq!(specialize(SOURCE, None, false).unwrap(), SOURCE);
    }

    #[test]
    fn jitter_only_changes_primary_call_and_adds_private_helper() {
        let result = specialize(SOURCE, Some(1), false).unwrap();
        assert_eq!(result.matches("uint probe_seed = 1u;").count(), 1);
        assert_eq!(
            result
                .matches("step = probeJitterStep(distance, threshold, cfg, probe_seed);")
                .count(),
            1
        );
        let original = SOURCE
            .split("static float sdfMarchStep(")
            .nth(1)
            .unwrap()
            .split("static bool sdfMarchConverged(")
            .next()
            .unwrap();
        assert!(result.contains(original));
        assert_eq!(
            SOURCE
                .matches("threshold = mandelbulberMarchThreshold(position, cfg);")
                .count(),
            result
                .matches("threshold = mandelbulberMarchThreshold(position, cfg);")
                .count()
        );
    }

    #[test]
    fn dynamic_threshold_adds_only_one_refresh() {
        let result = specialize(SOURCE, None, true).unwrap();
        assert_eq!(
            SOURCE
                .matches("threshold = mandelbulberMarchThreshold(position, cfg);")
                .count()
                + 1,
            result
                .matches("threshold = mandelbulberMarchThreshold(position, cfg);")
                .count()
        );
        assert!(!result.contains("probeStepMultiplier"));
    }

    #[test]
    fn invalid_seed_and_changed_source_rejected() {
        assert!(specialize(SOURCE, Some(u32::MAX), false).is_err());
        assert!(specialize("", Some(1), true).is_err());
    }

    #[test]
    fn dimensioned_sampling_preserves_other_callers() {
        let source = deterministic_path(SOURCE).unwrap();
        let result = dimensioned(&source, 42, false).unwrap();
        assert_eq!(
            result
                .matches("probeDimensionedMarch(dr, rp, local_ni, cfg,")
                .count(),
            1
        );
        let path = source
            .find("static float3 renderPath(float2 xy, uint sample_idx,")
            .unwrap();
        assert!(result.starts_with(&source[..path]));
        assert!(result.contains("probePathStepSeed(xy, sample_idx, uint(i), 42u)"));
        assert!(!result.contains("uint probe_seed = 0u;"));
        assert_eq!(
            result
                .matches("randomPoint(aa_strength, xy, frame)")
                .count(),
            SOURCE
                .matches("randomPoint(aa_strength, xy, frame)")
                .count()
        );
    }
}
