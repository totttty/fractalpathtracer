//! Authored clipping bounds in untransformed Mandelbulber coordinates.
use super::{MandelbulberScene, parameter_bool, parameter_vec3};
use anyhow::{Result, ensure};

pub(super) fn specialize(base: &str, scene: &MandelbulberScene) -> Result<String> {
    if !parameter_bool(&scene.main_parameters, "limits_enabled", false)? {
        return Ok(base.to_owned());
    }
    let lo = parameter_vec3(&scene.main_parameters, "limit_min", [-10.0; 3])?;
    let hi = parameter_vec3(&scene.main_parameters, "limit_max", [10.0; 3])?;
    ensure!(
        (0..3).all(|i| {
            (lo[i] as f32).is_finite()
                && (hi[i] as f32).is_finite()
                && (lo[i] as f32) < (hi[i] as f32)
        }),
        "scene limits must have finite, increasing bounds in Metal precision"
    );
    let vector = |v: [f64; 3]| {
        format!(
            "float3({:.9e}f, {:.9e}f, {:.9e}f)",
            v[0] as f32, v[1] as f32, v[2] as f32
        )
    };
    let helper = format!(
        "#define FPT_MANDEL_SCENE_LIMITS 1\n\
         static float fptMandelLimitDistance(float3 p, constant FptRenderConfig &cfg) {{\n\
             float scale = max(setv(cfg, 0), 1.0f);\n\
             float3 source = p.xzy / scale;\n\
             float3 outside = max(source - {}, {} - source);\n\
             return max(outside.x, max(outside.y, outside.z)) * scale;\n\
         }}\n",
        vector(hi),
        vector(lo)
    );
    let marker = "#define FPT_MANDEL_GENERATED_MATERIAL 1";
    ensure!(
        base.contains(marker),
        "missing scene limits insertion point"
    );
    Ok(base.replacen(marker, &format!("{helper}{marker}"), 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    const HEADER: &str = "# Mandelbulber settings file\n# version 2.33\n[main_parameters]\nformula_1 1;\nmat1_is_defined true;\n";
    const MARKER: &str = "#define FPT_MANDEL_GENERATED_MATERIAL 1";

    #[test]
    fn disabled_limits_leave_source_unchanged() {
        let scene = MandelbulberScene::parse(HEADER).unwrap();
        assert_eq!(specialize("unchanged", &scene).unwrap(), "unchanged");
    }

    #[test]
    fn enabled_limits_use_source_axes_and_native_defaults() {
        let scene = MandelbulberScene::parse(&format!("{HEADER}limits_enabled true;\n")).unwrap();
        let source = specialize(MARKER, &scene).unwrap();
        assert!(source.contains("p.xzy / scale"));
        assert!(source.contains("1.000000000e1f"));
        assert!(source.contains("-1.000000000e1f"));
        assert!(!source.contains("mandelbulberGlobalPoint"));
        assert!(specialize("missing marker", &scene).is_err());
    }

    #[test]
    fn invalid_or_unrepresentable_bounds_are_rejected() {
        for bounds in [
            "limit_min 1 0 0;\nlimit_max 0 1 1;",
            "limit_min 0 0 0;\nlimit_max 0 1 1;",
            "limit_max 1e100 1 1;",
            "limit_min 1000000000 0 0;\nlimit_max 1000000001 1 1;",
        ] {
            let scene =
                MandelbulberScene::parse(&format!("{HEADER}limits_enabled true;\n{bounds}\n"))
                    .unwrap();
            assert!(specialize(MARKER, &scene).is_err(), "{bounds}");
        }
    }
}
