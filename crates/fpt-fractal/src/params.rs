//! Formula parameters (`fractal->...`) from Mandelbulber's own sources.
//!
//! `initparameters.cpp` declares every scene setting with its default
//! (`par->addParam("power", 9.0, ...)`), `fractal.cpp` copies settings into
//! the `sFractal` members formulas read (`bulb.power = container->Get<double>
//! ("power")`), and `RecalculateFractalParams` derives the rest (radians,
//! squared radii, rotation matrices). This module reproduces all three in f64.

use std::collections::HashMap;

use crate::eval::{Parameters, Value};

/// One `member = container->Get<type>("key"[, index])` copy in `fractal.cpp`.
#[derive(Clone, Debug, PartialEq)]
struct Mapping {
    member: String,
    key: String,
    value_type: String,
    index: Option<String>,
    /// `CVector4(container->Get<CVector3>(...), 0.0)` widens to a 4-vector.
    widen_to_vec4: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ParameterSchema {
    defaults: HashMap<String, String>,
    indexed_defaults: HashMap<String, String>,
    enums: HashMap<String, Vec<String>>,
    mappings: Vec<Mapping>,
}

impl ParameterSchema {
    /// Parses `initparameters.cpp` and `fractal.cpp` source text.
    pub fn parse(initparameters: &str, fractal: &str) -> Result<Self, String> {
        let mut schema = ParameterSchema::default();
        let lists = qstring_lists(initparameters);
        let marker = "->addParam(";
        let mut offset = 0;
        while let Some(relative) = initparameters[offset..].find(marker) {
            let start = offset + relative + marker.len();
            let end = matching_parenthesis(initparameters, start)?;
            let arguments = split_arguments(&initparameters[start..end]);
            if arguments.len() >= 2
                && let Some(name) = quoted_literal(arguments[0].trim())
            {
                let indexed = arguments.len() >= 3
                    && !looks_like_default(arguments[1].trim())
                    && looks_like_default(arguments[2].trim());
                let default = arguments[if indexed { 2 } else { 1 }].trim().to_owned();
                if let Some(last) = arguments.last().map(|a| a.trim()) {
                    // A named list, or one written inline: `QStringList({"a", "b"})`.
                    let values = match lists.get(last) {
                        Some(values) => values.clone(),
                        None if last.starts_with("QStringList(") => quoted_literals(last),
                        None => Vec::new(),
                    };
                    if !values.is_empty() {
                        schema.enums.insert(name.clone(), values);
                    }
                }
                if indexed {
                    schema.indexed_defaults.insert(name, default);
                } else {
                    schema.defaults.insert(name, default);
                }
            }
            offset = end + 1;
        }
        // Line comments would otherwise join the member before `=`.
        let constructor: String = fractal
            .split("void sFractal::RecalculateFractalParams()")
            .next()
            .unwrap_or(fractal)
            .lines()
            .map(|line| line.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        let constructor = constructor.as_str();
        let marker = "container->Get<";
        let mut offset = 0;
        while let Some(relative) = constructor[offset..].find(marker) {
            let call = offset + relative;
            let type_start = call + marker.len();
            let type_end = type_start
                + constructor[type_start..]
                    .find('>')
                    .ok_or("unterminated Get type")?;
            let open = type_end
                + constructor[type_end..]
                    .find('(')
                    .ok_or("Get without arguments")?;
            let close = matching_parenthesis(constructor, open + 1)?;
            let arguments = split_arguments(&constructor[open + 1..close]);
            offset = close + 1;
            let Some(key) = arguments.first().and_then(|a| quoted_literal(a.trim())) else {
                continue;
            };
            let before = &constructor[..call];
            let Some(equals) = before.rfind('=') else {
                continue;
            };
            let statement_start = before[..equals]
                .rfind([';', '{', '}', ')'])
                .map_or(0, |i| i + 1);
            let member: String = before[statement_start..equals]
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            if !member.starts_with(|c: char| c == '_' || c.is_ascii_alphabetic()) {
                continue;
            }
            let rhs = before[equals + 1..].trim_start();
            schema.mappings.push(Mapping {
                member,
                key,
                value_type: constructor[type_start..type_end].trim().to_owned(),
                index: arguments
                    .get(1)
                    .map(|a| a.trim().to_owned())
                    .filter(|a| !a.is_empty()),
                widen_to_vec4: rhs.starts_with("CVector4("),
            });
        }
        if schema.defaults.is_empty() || schema.mappings.is_empty() {
            return Err("no parameter declarations or mappings found".into());
        }
        Ok(schema)
    }

    /// Formula parameters for one fractal slot's settings (`key -> value` as
    /// written in the `.fract` file, comma or dot decimals), then derived
    /// values as `RecalculateFractalParams` computes them.
    pub fn parameters(&self, settings: &HashMap<String, String>) -> Result<Parameters, String> {
        let mut values = HashMap::new();
        for mapping in &self.mappings {
            match &mapping.index {
                None => {
                    let raw = settings
                        .get(&mapping.key)
                        .or_else(|| self.defaults.get(&mapping.key));
                    if let Some(raw) = raw {
                        values.insert(mapping.member.clone(), self.convert(mapping, raw)?);
                    }
                }
                Some(variable) => {
                    for n in 0..=9 {
                        let key = format!("{}_{n}", mapping.key);
                        let raw = settings.get(&key).or_else(|| {
                            self.indexed_defaults
                                .get(&mapping.key)
                                .filter(|_| self.index_exists(&mapping.key, n))
                        });
                        let Some(raw) = raw else { continue };
                        let member = substitute_index(&mapping.member, variable, n)?;
                        values.insert(member, self.convert(mapping, raw)?);
                    }
                }
            }
        }
        recalculate(&mut values);
        Ok(Parameters {
            values,
            constants: HashMap::new(),
        })
    }

    /// Indexed settings exist for 1..=N where Mandelbulber declares them;
    /// accept 1..=4 for IFS-style arrays and 1..=3 for mandelbox rotations.
    fn index_exists(&self, key: &str, n: usize) -> bool {
        match key {
            k if k.starts_with("mandelbox_rotation") => (1..=3).contains(&n),
            k if k.starts_with("IFS_") => n < 9,
            _ => (1..=4).contains(&n),
        }
    }

    fn convert(&self, mapping: &Mapping, raw: &str) -> Result<Value, String> {
        let numbers = numbers(raw);
        let first = || {
            numbers
                .first()
                .copied()
                .ok_or_else(|| format!("{}: no value in {raw:?}", mapping.key))
        };
        Ok(match mapping.value_type.as_str() {
            "double" | "float" => Value::Scalar(first()?),
            "int" => match self
                .enums
                .get(&mapping.key)
                .and_then(|names| names.iter().position(|n| n == raw.trim()))
            {
                Some(index) => Value::Int(index as i64),
                None => Value::Int(first()? as i64),
            },
            "bool" => Value::Bool(matches!(raw.trim(), "true" | "1")),
            "CVector3" => {
                let v = vector(&numbers, 3);
                if mapping.widen_to_vec4 {
                    Value::vec4([v[0], v[1], v[2], 0.0])
                } else {
                    Value::vec3([v[0], v[1], v[2]])
                }
            }
            "CVector4" => Value::vec4(vector(&numbers, 4)),
            "sRGB" | "sRGBFloat" => Value::vec3([
                numbers.first().copied().unwrap_or(0.0),
                numbers.get(1).copied().unwrap_or(0.0),
                numbers.get(2).copied().unwrap_or(0.0),
            ]),
            other => {
                return Err(format!(
                    "{}: unsupported parameter type {other}",
                    mapping.key
                ));
            }
        })
    }
}

/// Numbers in a setting or default: `2,5 -1 0`, `CVector3(1.0, 2.0, 3.0)`,
/// `true` and hex colour words are not numbers and are skipped.
fn numbers(raw: &str) -> Vec<f64> {
    let cleaned = raw.trim();
    let inner = cleaned
        .find('(')
        .and_then(|open| cleaned.rfind(')').map(|close| &cleaned[open + 1..close]))
        .unwrap_or(cleaned);
    let separator = if inner.contains(", ") || (inner.contains(',') && inner.contains('(')) {
        ','
    } else {
        ' '
    };
    inner
        .split(|c: char| c == separator || c.is_whitespace())
        .filter(|word| !word.is_empty())
        .filter_map(|word| {
            word.replace(',', ".")
                .trim_end_matches('f')
                .parse::<f64>()
                .ok()
        })
        .collect()
}

fn vector(numbers: &[f64], len: usize) -> [f64; 4] {
    let mut v = [0.0; 4];
    for i in 0..len.min(numbers.len()) {
        v[i] = numbers[i];
    }
    if numbers.len() == 1 {
        v[..len].fill(numbers[0]);
    }
    v
}

fn substitute_index(member: &str, variable: &str, n: usize) -> Result<String, String> {
    let mut output = String::new();
    let mut rest = member;
    while let Some(open) = rest.find('[') {
        let close = rest[open..].find(']').ok_or("unterminated index")? + open;
        output.push_str(&rest[..open]);
        let expression = &rest[open + 1..close];
        let value = if expression == variable {
            n as i64
        } else if let Some(offset) = expression.strip_prefix(variable) {
            n as i64
                + offset
                    .parse::<i64>()
                    .map_err(|e| format!("index {expression}: {e}"))?
        } else {
            expression
                .parse::<i64>()
                .map_err(|e| format!("index {expression}: {e}"))?
        };
        output.push_str(&format!("[{value}]"));
        rest = &rest[close + 1..];
    }
    output.push_str(rest);
    Ok(output)
}

fn scalar(values: &HashMap<String, Value>, path: &str) -> Option<f64> {
    values.get(path).and_then(|v| v.as_scalar().ok())
}

fn vec3_of(values: &HashMap<String, Value>, path: &str) -> Option<[f64; 3]> {
    match values.get(path)? {
        Value::Vector { v, .. } => Some([v[0], v[1], v[2]]),
        _ => None,
    }
}

fn multiply(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut out = [[0.0; 3]; 3];
    for (row, out_row) in out.iter_mut().enumerate() {
        for (column, cell) in out_row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[row][k] * b[k][column]).sum();
        }
    }
    out
}

const IDENTITY: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// `CRotationMatrix::RotateX/Y/Z`: post-multiplies, skipping zero angles.
fn rotate(matrix: [[f64; 3]; 3], axis: usize, angle: f64) -> [[f64; 3]; 3] {
    if angle == 0.0 {
        return matrix;
    }
    let (s, c) = angle.sin_cos();
    let rotation = match axis {
        0 => [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]],
        1 => [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]],
        _ => [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
    };
    multiply(matrix, rotation)
}

/// `SetRotation2`: Z(z), Y(y), X(x). `SetRotation3`: Z(x), Y(y), X(z).
/// `SetRotation4`: X(x), Y(y), Z(z). Angles in degrees.
fn rotation_matrix(order: u8, degrees: [f64; 3]) -> [[f64; 3]; 3] {
    let r = degrees.map(f64::to_radians);
    match order {
        2 => rotate(rotate(rotate(IDENTITY, 2, r[2]), 1, r[1]), 0, r[0]),
        3 => rotate(rotate(rotate(IDENTITY, 2, r[0]), 1, r[1]), 0, r[2]),
        _ => rotate(rotate(rotate(IDENTITY, 0, r[0]), 1, r[1]), 2, r[2]),
    }
}

fn transpose(m: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    [
        [m[0][0], m[1][0], m[2][0]],
        [m[0][1], m[1][1], m[2][1]],
        [m[0][2], m[1][2], m[2][2]],
    ]
}

/// `sFractal::RecalculateFractalParams` for the derived members formulas read.
fn set(values: &mut HashMap<String, Value>, path: &str, value: Value) {
    values.insert(path.to_owned(), value);
}

fn recalculate(values: &mut HashMap<String, Value>) {
    if let Some(r) = vec3_of(values, "IFS.rotation") {
        set(values, "IFS.mainRot", Value::Matrix(rotation_matrix(3, r)));
    }
    for i in 0..9 {
        if let Some(r) = vec3_of(values, &format!("IFS.rotations[{i}]")) {
            set(
                values,
                &format!("IFS.rot[{i}]"),
                Value::Matrix(rotation_matrix(3, r)),
            );
        }
        if let Some(Value::Vector { v, .. }) = values.get(&format!("IFS.direction[{i}]")).cloned() {
            let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            set(
                values,
                &format!("IFS.direction[{i}]"),
                Value::vec4([v[0] / length, v[1] / length, v[2] / length, v[3]]),
            );
        }
    }
    if let Some(r) = vec3_of(values, "mandelbox.rotationMain") {
        set(
            values,
            "mandelbox.mainRot",
            Value::Matrix(rotation_matrix(2, r)),
        );
    }
    for fold in 0..2 {
        for axis in 0..3 {
            if let Some(r) = vec3_of(values, &format!("mandelbox.rotation[{fold}][{axis}]")) {
                let m = rotation_matrix(2, r);
                set(
                    values,
                    &format!("mandelbox.rot[{fold}][{axis}]"),
                    Value::Matrix(m),
                );
                set(
                    values,
                    &format!("mandelbox.rotinv[{fold}][{axis}]"),
                    Value::Matrix(transpose(m)),
                );
            }
        }
    }
    if let (Some(fixed), Some(min)) = (
        scalar(values, "mandelbox.foldingSphericalFixed"),
        scalar(values, "mandelbox.foldingSphericalMin"),
    ) {
        set(values, "mandelbox.fR2", Value::Scalar(fixed * fixed));
        set(values, "mandelbox.mR2", Value::Scalar(min * min));
        set(
            values,
            "mandelbox.mboxFactor1",
            Value::Scalar((fixed * fixed) / (min * min)),
        );
    }
    for path in [
        "bulb.alphaAngleOffset",
        "bulb.betaAngleOffset",
        "transformCommon.alphaAngleOffset",
        "transformCommon.betaAngleOffset",
        "transformCommon.angleDegA",
        "transformCommon.angleDegB",
        "transformCommon.angleDegC",
    ] {
        if let Some(degrees) = scalar(values, path) {
            set(values, path, Value::Scalar(degrees.to_radians()));
        }
    }
    for (angle, cos, sin) in [
        (
            "transformCommon.angleDegA",
            "transformCommon.cosA",
            "transformCommon.sinA",
        ),
        (
            "transformCommon.angleDegB",
            "transformCommon.cosB",
            "transformCommon.sinB",
        ),
        (
            "transformCommon.angleDegC",
            "transformCommon.cosC",
            "transformCommon.sinC",
        ),
    ] {
        if let Some(radians) = scalar(values, angle) {
            set(values, cos, Value::Scalar(radians.cos()));
            set(values, sin, Value::Scalar(radians.sin()));
        }
    }
    for (source, target, order) in [
        (
            "transformCommon.rotation",
            "transformCommon.rotationMatrix",
            2,
        ),
        (
            "transformCommon.rotation2",
            "transformCommon.rotationMatrix2",
            2,
        ),
        (
            "transformCommon.rotationXYZ",
            "transformCommon.rotationMatrixXYZ",
            4,
        ),
        (
            "transformCommon.rotation2XYZ",
            "transformCommon.rotationMatrix2XYZ",
            4,
        ),
        (
            "transformCommon.rotationVary",
            "transformCommon.rotationMatrixVary",
            2,
        ),
    ] {
        if let Some(r) = vec3_of(values, source) {
            set(values, target, Value::Matrix(rotation_matrix(order, r)));
        }
    }
    if let Some(min_r05) = scalar(values, "transformCommon.minR05") {
        let sqt_r = min_r05.sqrt();
        set(values, "transformCommon.sqtR", Value::Scalar(sqt_r));
        set(
            values,
            "transformCommon.mboxFactor1",
            Value::Scalar(1.0 / sqt_r),
        );
    }
    for (source, target) in [
        ("transformCommon.invert0", "transformCommon.inv0"),
        ("transformCommon.invert1", "transformCommon.inv1"),
    ] {
        if let Some(x) = scalar(values, source) {
            set(values, target, Value::Scalar(1.0 / x));
        }
    }
    if let Some(max_r2) = scalar(values, "transformCommon.maxR2d1") {
        if let Some(min_r2) = scalar(values, "transformCommon.minR2p25") {
            set(
                values,
                "transformCommon.maxMinR2factor",
                Value::Scalar(max_r2 / min_r2),
            );
        }
        if let Some(min_r0) = scalar(values, "transformCommon.minR0") {
            set(
                values,
                "transformCommon.maxMinR0factor",
                Value::Scalar(max_r2 / min_r0),
            );
        }
    }
}

fn qstring_lists(source: &str) -> HashMap<String, Vec<String>> {
    let marker = "QStringList ";
    let mut lists = HashMap::new();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find(marker) {
        let start = offset + relative + marker.len();
        let name_end = source[start..]
            .find(|c: char| !(c == '_' || c.is_ascii_alphanumeric()))
            .map_or(source.len(), |relative| start + relative);
        let Some(end) = source[name_end..]
            .find(';')
            .map(|relative| name_end + relative)
        else {
            break;
        };
        let values = quoted_literals(&source[name_end..end]);
        if name_end > start && !values.is_empty() {
            lists.insert(source[start..name_end].to_owned(), values);
        }
        offset = end + 1;
    }
    lists
}

fn quoted_literals(source: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut rest = source;
    while let Some(quote) = rest.find('"') {
        rest = &rest[quote + 1..];
        let Some(end) = rest.find('"') else { break };
        values.push(rest[..end].to_owned());
        rest = &rest[end + 1..];
    }
    values
}

fn matching_parenthesis(source: &str, start: usize) -> Result<usize, String> {
    let mut depth = 1usize;
    let mut quoted = false;
    for (relative, c) in source[start..].char_indices() {
        match c {
            '"' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => {
                depth -= 1;
                if depth == 0 {
                    return Ok(start + relative);
                }
            }
            _ => {}
        }
    }
    Err("unterminated call".into())
}

fn split_arguments(source: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let (mut start, mut depth, mut quoted) = (0, 0usize, false);
    for (index, c) in source.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '(' | '{' | '[' if !quoted => depth += 1,
            ')' | '}' | ']' if !quoted => depth = depth.saturating_sub(1),
            ',' if !quoted && depth == 0 => {
                result.push(&source[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    result.push(&source[start..]);
    result
}

fn quoted_literal(value: &str) -> Option<String> {
    Some(value.strip_prefix('"')?.strip_suffix('"')?.to_owned())
}

fn looks_like_default(value: &str) -> bool {
    matches!(value, "true" | "false")
        || value.starts_with('"')
        || value.starts_with("CVector")
        || value.starts_with("sRGB")
        || value.starts_with("cColorPalette")
        || value.starts_with("enum")
        || value.parse::<f64>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const INIT: &str = r#"
void InitFractalParams(std::shared_ptr<cParameterContainer> par)
{
	QStringList qslOrderOfXyz({"xyz", "xzy"});
	par->addParam("power", 9.0, morphAkima, paramStandard);
	par->addParam("alpha_angle_offset", 0.0, morphAkima, paramStandard);
	par->addParam("mandelbox_folding_min_radius", 0.5, morphAkima, paramStandard);
	par->addParam("mandelbox_folding_fixed_radius", 1.0, morphAkima, paramStandard);
	par->addParam("mandelbox_offset", CVector3(0.0, 0.0, 0.0), morphAkima, paramStandard);
	par->addParam("mandelbulbMulti_order_of_xyz", 0, 0, 5, morphNone, paramStandard, qslOrderOfXyz);
	par->addParam("mandelbox_generalized_fold_type", 0, morphNone, paramStandard,
		QStringList({"fold_tet", "fold_cube"}));
	for (int i = 1; i <= 3; i++)
		par->addParam("mandelbox_rotation_neg", i, CVector3(0.0, 0.0, 0.0), morphAkimaAngle, paramStandard);
}
"#;
    const FRACTAL: &str = r#"
sFractal::sFractal(const cParameterContainer *container)
{
	// common parameters
	bulb.power = container->Get<double>("power"); // comment
	bulb.alphaAngleOffset = container->Get<double>("alpha_angle_offset");
	mandelbox.foldingSphericalMin = container->Get<double>("mandelbox_folding_min_radius");
	mandelbox.foldingSphericalFixed = container->Get<double>("mandelbox_folding_fixed_radius");
	mandelbox.offset = CVector4(container->Get<CVector3>("mandelbox_offset"), 0.0);
	for (int i = 1; i <= 3; i++)
		mandelbox.rotation[0][i - 1] = container->Get<CVector3>("mandelbox_rotation_neg", i);
	mandelbulbMulti.orderOfXYZ =
		enumMulti_OrderOfXYZ(container->Get<int>("mandelbulbMulti_order_of_xyz"));
	genFoldBox.type =
		enumGeneralizedFoldBoxType(container->Get<int>("mandelbox_generalized_fold_type"));
}
void sFractal::RecalculateFractalParams()
{
}
"#;

    #[test]
    fn settings_override_defaults_and_derive_members() {
        let schema = ParameterSchema::parse(INIT, FRACTAL).unwrap();
        let settings: HashMap<String, String> = [
            ("power".to_owned(), "8".to_owned()),
            ("alpha_angle_offset".to_owned(), "22,5".to_owned()),
            ("mandelbox_offset".to_owned(), "1 2 3".to_owned()),
            ("mandelbox_rotation_neg_2".to_owned(), "0 0 90".to_owned()),
            (
                "mandelbox_generalized_fold_type".to_owned(),
                "fold_cube".to_owned(),
            ),
        ]
        .into();
        let p = schema.parameters(&settings).unwrap().values;
        assert_eq!(p["bulb.power"], Value::Scalar(8.0));
        assert_eq!(
            p["bulb.alphaAngleOffset"],
            Value::Scalar(22.5_f64.to_radians())
        );
        assert_eq!(p["mandelbox.offset"], Value::vec4([1.0, 2.0, 3.0, 0.0]));
        assert_eq!(p["mandelbox.mR2"], Value::Scalar(0.25));
        assert_eq!(p["mandelbox.mboxFactor1"], Value::Scalar(4.0));
        assert_eq!(p["mandelbulbMulti.orderOfXYZ"], Value::Int(0));
        assert_eq!(p["genFoldBox.type"], Value::Int(1));
        let Value::Matrix(m) = p["mandelbox.rot[0][1]"] else {
            panic!()
        };
        assert!(
            (m[0][1] + 1.0).abs() < 1e-12 && (m[1][0] - 1.0).abs() < 1e-12,
            "{m:?}"
        );
        assert!(p.contains_key("mandelbox.rotinv[0][2]"));
    }
}
