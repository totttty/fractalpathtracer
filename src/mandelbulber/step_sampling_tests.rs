//! Lock the separate sampled marcher to the ordinary traversal/refinement.
const SOURCE: &str = include_str!("../../shaders/Shaders.metal");

fn body(name: &str) -> &str {
    let start = SOURCE.find(name).expect("shader function missing");
    let start = start + SOURCE[start..].find('{').unwrap();
    let mut depth = 0;
    for (offset, ch) in SOURCE[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &SOURCE[start + 1..start + offset];
                }
            }
            _ => {}
        }
    }
    panic!("unterminated shader function");
}

fn normalized(value: &str) -> String {
    value.split_whitespace().collect()
}

#[test]
fn sampled_traversal_and_refinement_match_ordinary_body() {
    let sampled = body("static MandelbulberMarchResult marchMandelbulberSampled(")
        .replace("uint step_seed = initial_seed;", "")
        .replace(
            "mandelSampledStep(distance, threshold, cfg, step_seed)",
            "sdfMarchStep(distance, threshold, cfg)",
        );
    assert_eq!(
        normalized(&sampled),
        normalized(body("static MandelbulberMarchResult marchMandelbulber("))
    );
}

#[test]
fn jitter_precedes_existing_step_clamps() {
    let sampled =
        body("static float mandelSampledStep(").replace(" * mandelStepMultiplier(seed)", "");
    assert_eq!(
        normalized(&sampled),
        normalized(body("static float sdfMarchStep("))
    );
}

#[test]
fn sampling_is_path_scoped_and_profile_uses_same_dimensions() {
    assert_eq!(
        SOURCE
            .matches("marchMandelbulberSampled(dr, rp, local_ni, cfg,")
            .count(),
        1
    );
    assert_eq!(
        SOURCE
            .matches("mandelPathStepSeed(xy, sample_idx, uint(i), 1u)")
            .count(),
        2
    );
    assert!(
        body("static float3 marchMandelbulberProfiled(")
            .contains("mandelSampledStep(distance, threshold, cfg, step_seed)")
    );
    assert!(body("static uint sunProfiledWithNormal(").contains("marchProfiled("));
    assert!(!body("static uint sunProfiledWithNormal(").contains("mandelPathStepSeed"));
}
