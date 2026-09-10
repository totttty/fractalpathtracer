//! Diagnostic-only differentiated orbit; never linked into production shaders.
use anyhow::{Result, ensure};

const JACOBIAN: &str = r#"
// Derivative of the local inversion/scaling map. Orbit positions still use
// the runtime-translated authoritative formula, not this derivative helper.
static float3x3 diagnostic85Jacobian(float3 z) {
    constant auto &t = kMandelFormulaParameters.transformCommon;
    float rr = dot(z, z);
    float3x3 j;
    if (rr < t.minR05 * t.minR05) {
        if (t.functionEnabled) {
            float k = t.minR05 * 1.18f * t.scaleA1;
            float factor = k * (0.1f + 0.4f / rr);
            float radial = 0.8f * k / (rr * rr);
            j = float3x3(float3(factor, 0, 0) - radial * z * z.x,
                        float3(0, factor, 0) - radial * z * z.y,
                        float3(0, 0, factor) - radial * z * z.z);
        } else {
            j = float3x3(float3(t.constantMultiplier111.x, 0, 0),
                        float3(0, t.constantMultiplier111.y, 0),
                        float3(0, 0, t.constantMultiplier111.z));
        }
    } else {
        float3 b = t.constantMultiplier222.xyz;
        float3 q = b * z - float3(0, t.offset1, 0);
        float r = dot(q, q);
        float factor = t.scale3 / r;
        j = float3x3(factor * b.x * (float3(1, 0, 0) - 2.0f * q * q.x / r),
                    factor * b.y * (float3(0, 1, 0) - 2.0f * q * q.y / r),
                    factor * b.z * (float3(0, 0, 1) - 2.0f * q * q.z / r));
        // The derivative of x-round(x) is one away from its discontinuities.
        // No claim is made at those discontinuities or orbit branch boundaries.
    }
    return j;
}
"#;

fn replace_once(source: &str, marker: &str, replacement: &str) -> Result<String> {
    ensure!(
        source.matches(marker).count() == 1,
        "analytic85 source shape changed: {marker}"
    );
    Ok(source.replacen(marker, replacement, 1))
}

pub fn specialize(source: &str, box_fold: f32, sphere_fold: f32) -> Result<String> {
    ensure!(
        box_fold <= 0.5 && sphere_fold <= 0.5,
        "analytic85 does not support global folds"
    );
    let start = source
        .find("struct MandelDeltaOrbitResult {")
        .ok_or_else(|| anyhow::anyhow!("analytic85 requires standalone delta orbit"))?;
    let end_marker =
        "    return float4(distance, base.radius, base.derivative, iteration_state);\n}";
    let end = source[start..]
        .find(end_marker)
        .map(|v| v + start + end_marker.len())
        .ok_or_else(|| anyhow::anyhow!("missing generated field boundary"))?;
    let mut part = source[start..end].to_owned();
    ensure!(
        part.contains("float4 z = float4(scaled.x, scaled.z, scaled.y, 0.0f);"),
        "analytic85 requires zero initial W"
    );
    ensure!(
        part.contains("RiemannBulbMsltoeMod2Iteration"),
        "analytic85 requires formula 85"
    );
    let step = "        z = RiemannBulbMsltoeMod2Iteration(z, kMandelFormulaParameters, aux);\n        aux.r = length(z);";
    part = replace_once(
        &part,
        step,
        &format!("        jacobian = diagnostic85Jacobian(z.xyz) * jacobian;\n{step}"),
    )?;
    part = replace_once(
        &part,
        "    float derivative;",
        "    float derivative;\n    float3 radialDerivative;",
    )?;
    part = replace_once(
        &part,
        "    MandelOrbitState aux = {};",
        "    float3x3 jacobian = float3x3(1.0f);\n    MandelOrbitState aux = {};",
    )?;
    part = replace_once(
        &part,
        "return { z, aux.r, aux.DE, completed_iterations, escaped };",
        "return { z, aux.r, aux.DE, transpose(jacobian) * (z.xyz / max(aux.r, 1.0e-30f)), completed_iterations, escaped };",
    )?;
    let first = part
        .find("    float delta = max(1.0e-7f, 5.0e-7f * length(scaled));")
        .ok_or_else(|| anyhow::anyhow!("unexpected formula delta"))?;
    let last = part[first..]
        .find("    float radial_gradient = length(radial_derivative);")
        .map(|v| v + first)
        .ok_or_else(|| anyhow::anyhow!("missing radial derivative boundary"))?;
    part.replace_range(
        first..last,
        "    float3 radial_derivative = base.radialDerivative;\n",
    );
    // Compile-time guard is evaluated against the generated parameter record.
    let guard = "\nstatic_assert(kMandelFormulaParameters.transformCommon.functionEnabledxFalse == 0, \"analytic85 sine branch unsupported\");\n";
    Ok(format!(
        "{}{}{}{}{}",
        &source[..start],
        guard,
        JACOBIAN,
        part,
        &source[end..]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unhandled_folds_and_wrong_formula() {
        assert!(
            specialize("", 1.0, 0.0)
                .unwrap_err()
                .to_string()
                .contains("global folds")
        );
        assert!(specialize("", 0.0, 0.0).is_err());
    }

    #[test]
    fn rejects_ambiguous_replacement() {
        assert!(replace_once("aa", "a", "b").is_err());
        assert!(replace_once("aa", "z", "b").is_err());
        assert_eq!(replace_once("abc", "b", "d").unwrap(), "adc");
    }

    #[test]
    fn differentiation_is_scoped_before_material_orbits() {
        let orbit = r#"struct MandelDeltaOrbitResult {
    float derivative;
};
static MandelDeltaOrbitResult mandelDeltaOrbit() {
    float4 z = float4(scaled.x, scaled.z, scaled.y, 0.0f);
    MandelOrbitState aux = {};
        z = RiemannBulbMsltoeMod2Iteration(z, kMandelFormulaParameters, aux);
        aux.r = length(z);
    return { z, aux.r, aux.DE, completed_iterations, escaped };
}
static float4 mandelbulberGeneratedFieldSample() {
    float delta = max(1.0e-7f, 5.0e-7f * length(scaled));
    float3 radial_derivative = placeholder;
    float radial_gradient = length(radial_derivative);
    return float4(distance, base.radius, base.derivative, iteration_state);
}"#;
        let tail = "\n        z = RiemannBulbMsltoeMod2Iteration(z, kMandelFormulaParameters, aux);\n        aux.r = length(z);\n// FPT_MANDELBULBER_GENERATED_INSERTION_POINT";
        let source = format!("{orbit}{tail}");
        let result = specialize(&source, 0.0, 0.0).unwrap();
        assert!(result.ends_with(tail));
        assert_eq!(result.matches("jacobian = diagnostic85Jacobian").count(), 1);
        assert!(result.contains("float3 radial_derivative = base.radialDerivative;"));
        assert!(!result.contains("placeholder"));
        assert!(
            specialize(
                &source.replace("scaled.y, 0.0f", "scaled.y, 1.0f"),
                0.0,
                0.0
            )
            .is_err()
        );
        assert!(
            specialize(
                &source.replacen(
                    "        aux.r = length(z);",
                    "        z += aux.c;\n        aux.r = length(z);",
                    1
                ),
                0.0,
                0.0
            )
            .is_err()
        );
    }
}
