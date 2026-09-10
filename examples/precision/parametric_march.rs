//! Diagnostic ray-position experiment, never enabled by the production CLI.
use anyhow::{Context, Result, ensure};

pub fn specialize(source: &str) -> Result<String> {
    let start = source
        .find("static MandelbulberMarchResult marchMandelbulber(")
        .context("missing primary marcher")?;
    let end = start
        + source[start..]
            .find("    if (!found) return MandelbulberMarchResult{position, false};")
            .context("missing primary exit")?;
    let original = &source[start..end];
    let mut body = original.to_owned();
    for (from, to) in [
        (
            "    float3 start = position;",
            "    float3 start = position;\n    float travel = 0.0f;",
        ),
        (
            "        float3 next_position = position + direction * step;",
            "        float next_travel = travel + step;\n        float3 next_position = fma(direction, float3(next_travel), start);",
        ),
        (
            "        if (all(next_position == position)) break;",
            "        if (next_travel == travel) break;\n        travel = next_travel;",
        ),
    ] {
        ensure!(
            body.matches(from).count() == 1,
            "unexpected marcher source: {from}"
        );
        body = body.replacen(from, to, 1);
    }
    Ok(format!("{}{}{}", &source[..start], body, &source[end..]))
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_primary_steps_change() {
        let source = include_str!("../../shaders/Shaders.metal");
        let result = super::specialize(source).unwrap();
        assert!(result.contains("fma(direction, float3(next_travel), start)"));
        let marker = "    if (!found) return MandelbulberMarchResult{position, false};";
        assert_eq!(
            source.split_once(marker).unwrap().1,
            result.split_once(marker).unwrap().1
        );
        assert!(super::specialize(&result).is_err());
    }
}
