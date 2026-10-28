//! Named constants from Mandelbulber's `opencl/defines_cl.h`.
//!
//! A `_F` suffix names the same value as a float literal; both read the
//! double value here, as Mandelbulber's CPU path does.

use std::collections::HashMap;
use std::f64::consts::*;

pub fn float_constant(name: &str) -> Option<f64> {
    Some(match name.strip_suffix("_F").unwrap_or(name) {
        "M_PI" => PI,
        "M_PI_2" => FRAC_PI_2,
        "M_PI_4" => FRAC_PI_4,
        "M_PI_8" => FRAC_PI_8,
        "M_1_PI" => FRAC_1_PI,
        "M_2_PI" => FRAC_2_PI,
        "M_PI_180" => PI / 180.0,
        "M_PI_2x" => TAU,
        "M_PI_2x_INV" => 1.0 / TAU,
        "M_SQRT2" => SQRT_2,
        "M_SQRT1_2" | "SQRT_1_2" => FRAC_1_SQRT_2,
        "M_E" => E,
        "SQRT_1_3" => 0.577_350_269_189_625_764_509_148_780_501_96,
        "SQRT_2_3" => 0.816_496_580_927_726_032_732_428_024_901_96,
        "SQRT_3_2" => 1.224_744_871_391_589_049_098_642_037_352_95,
        "SQRT_3_4" => 0.866_025_403_784_438_646_763_723_170_752_94,
        "SQRT_3_4d2" => 0.433_012_701_892_219_323_381_861_585_376_47,
        "SQRT_3" => 1.732_050_807_568_877_293_527_446_341_505_87,
        "FRAC_1_3" => 1.0 / 3.0,
        _ => return None,
    })
}

pub fn int_constant(name: &str) -> Option<i64> {
    Some(match name {
        "NUMBER_OF_FRACTALS" => 9,
        "IFS_VECTOR_COUNT" | "IFS_VECTOR_COUNTCl" => 9,
        "HYBRID_COUNT" => 5,
        "MANDELBOX_FOLDS" => 2,
        _ => return None,
    })
}

/// Enumerator values from `typedef enum { a, b = 4, c } name;` blocks, as in
/// `opencl/fractal_cl.h`.
pub fn enum_constants(header: &str) -> HashMap<String, i64> {
    let mut constants = HashMap::new();
    let text: String = header
        .lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    let mut rest = text.as_str();
    while let Some(start) = rest.find("enum") {
        rest = &rest[start + 4..];
        let (Some(open), Some(close)) = (rest.find('{'), rest.find('}')) else {
            break;
        };
        if open > close
            || !rest[..open]
                .trim()
                .chars()
                .all(|c| c == '_' || c.is_ascii_alphanumeric())
        {
            continue;
        }
        let mut next = 0;
        for item in rest[open + 1..close].split(',') {
            let (name, value) = match item.split_once('=') {
                Some((name, value)) => (name.trim(), value.trim().parse().ok()),
                None => (item.trim(), None),
            };
            if name.is_empty() {
                continue;
            }
            let value = value.unwrap_or(next);
            constants.insert(name.to_owned(), value);
            next = value + 1;
        }
        rest = &rest[close..];
    }
    constants
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerators_count_on_from_explicit_values() {
        let constants = enum_constants(
            "typedef enum\n{\n\tmulti_xyz, // first\n\tmulti_xzy,\n} enumMulti;\ntypedef enum { a = 4, b } other;",
        );
        assert_eq!(constants["multi_xyz"], 0);
        assert_eq!(constants["multi_xzy"], 1);
        assert_eq!(constants["b"], 5);
    }
}
