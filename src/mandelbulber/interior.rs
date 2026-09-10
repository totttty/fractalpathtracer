//! Interior rendering is a pixel-threshold rule, not a voxel occupancy rule.
use super::{MandelbulberScene, parameter_bool};
use anyhow::{Result, ensure};

pub(super) fn specialize(
    base: &str,
    scene: &MandelbulberScene,
    uses_delta: bool,
) -> Result<String> {
    if !parameter_bool(&scene.main_parameters, "interior_mode", false)? {
        return Ok(base.to_owned());
    }
    ensure!(
        !scene.boolean_enabled,
        "interior mode with Boolean fractals requires per-object distance handling"
    );
    let marker = "#define FPT_MANDEL_GENERATED_MATERIAL 1";
    ensure!(base.contains(marker), "missing interior insertion point");
    let delta_without_limits =
        uses_delta && !parameter_bool(&scene.main_parameters, "limits_enabled", false)?;
    let flags = format!(
        "#define FPT_MANDEL_INTERIOR 1\n#define FPT_MANDEL_INTERIOR_DELTA_UNBOUNDED {}\n",
        u32::from(delta_without_limits)
    );
    Ok(base.replacen(marker, &format!("{flags}{marker}"), 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    const HEADER: &str = "# Mandelbulber settings file\n# version 2.33\n[main_parameters]\nformula_1 1;\nmat1_is_defined true;\n";
    const MARKER: &str = "#define FPT_MANDEL_GENERATED_MATERIAL 1";

    #[test]
    fn disabled_interior_preserves_source() {
        let scene = MandelbulberScene::parse(HEADER).unwrap();
        assert_eq!(specialize("unchanged", &scene, false).unwrap(), "unchanged");
    }

    #[test]
    fn unsupported_boolean_interior_is_not_silently_rendered() {
        let mut scene =
            MandelbulberScene::parse(&format!("{HEADER}interior_mode true;\n")).unwrap();
        scene.boolean_enabled = true;
        assert!(specialize(MARKER, &scene, false).is_err());
    }

    #[test]
    fn delta_maxiter_policy_depends_on_limits() {
        for (settings, uses_delta, unbounded) in [
            ("", false, false),
            ("", true, true),
            ("limits_enabled true;\n", true, false),
        ] {
            let scene =
                MandelbulberScene::parse(&format!("{HEADER}interior_mode true;\n{settings}"))
                    .unwrap();
            let source = specialize(MARKER, &scene, uses_delta).unwrap();
            assert!(source.contains("#define FPT_MANDEL_INTERIOR 1"));
            assert!(source.contains(&format!(
                "FPT_MANDEL_INTERIOR_DELTA_UNBOUNDED {}",
                u32::from(unbounded)
            )));
            assert!(specialize("no marker", &scene, uses_delta).is_err());
        }
    }
}
