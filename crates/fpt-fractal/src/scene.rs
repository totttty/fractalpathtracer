//! `.fract` settings files and per-formula metadata.

use std::collections::HashMap;

/// A Mandelbulber settings file: `[section]` headers and `key value;` lines.
#[derive(Clone, Debug, Default)]
pub struct FractFile {
    pub version: String,
    pub sections: HashMap<String, HashMap<String, String>>,
}

impl FractFile {
    pub fn parse(text: &str) -> FractFile {
        let mut file = FractFile::default();
        let mut section = String::from("main_parameters");
        for line in text.lines() {
            let line = line.trim();
            if let Some(version) = line.strip_prefix("# version") {
                file.version = version.trim().to_owned();
            } else if line.starts_with('#') || line.is_empty() {
                continue;
            } else if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.to_owned();
            } else if let Some((key, value)) = line.split_once(char::is_whitespace) {
                let value = value.trim().trim_end_matches(';').trim();
                file.sections
                    .entry(section.clone())
                    .or_default()
                    .insert(key.to_owned(), value.to_owned());
            }
        }
        file
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections.get(section)?.get(key).map(String::as_str)
    }

    pub fn number(&self, key: &str, fallback: f64) -> f64 {
        self.get("main_parameters", key)
            .and_then(|v| v.replace(',', ".").parse().ok())
            .unwrap_or(fallback)
    }

    pub fn boolean(&self, key: &str, fallback: bool) -> bool {
        self.get("main_parameters", key)
            .map_or(fallback, |v| v == "true" || v == "1")
    }

    pub fn vector(&self, key: &str, fallback: [f64; 3]) -> [f64; 3] {
        let Some(value) = self.get("main_parameters", key) else {
            return fallback;
        };
        let parts: Vec<f64> = value
            .split_whitespace()
            .filter_map(|p| p.replace(',', ".").parse().ok())
            .collect();
        if parts.len() == 3 {
            [parts[0], parts[1], parts[2]]
        } else {
            fallback
        }
    }

    /// Settings of fractal slot `index` (1-based), as `fractal.cpp` reads them.
    pub fn fractal_settings(&self, index: usize) -> HashMap<String, String> {
        self.sections
            .get(&format!("fractal_{index}"))
            .cloned()
            .unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeType {
    Analytic,
    Delta,
}

/// `DEFunctionType` / `DEAnalyticFunction` in a formula definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeFunction {
    Linear,
    Logarithmic,
    Ifs,
    PseudoKleinian,
    JosKleinian,
    Custom,
    MaxAxis,
    /// `withoutDEFunction` / `analyticFunctionNone`: distance -1.
    None,
    /// `analyticFunctionUndefined`: the orbit radius.
    Undefined,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CAddition {
    EnabledByDefault,
    DisabledByDefault,
    AlreadyHas,
}

/// Metadata from `formula/definition/fractal_<name>.cpp`.
#[derive(Clone, Debug, PartialEq)]
pub struct FormulaMeta {
    pub internal_name: String,
    pub de_type: DeType,
    pub de_function: DeFunction,
    pub analytic_function: DeFunction,
    pub c_addition: CAddition,
    pub default_bailout: f64,
}

impl FormulaMeta {
    pub fn parse(definition: &str) -> Result<FormulaMeta, String> {
        let field = |name: &str| -> Option<String> {
            definition.lines().map(str::trim).find_map(|line| {
                let rest = line.strip_prefix(name)?.trim_start().strip_prefix('=')?;
                Some(
                    rest.trim()
                        .trim_end_matches(';')
                        .trim()
                        .trim_matches('"')
                        .to_owned(),
                )
            })
        };
        let function = |value: Option<String>| match value
            .as_deref()
            .map(|v| v.rsplit("::").next().unwrap_or(v))
        {
            Some("linearDEFunction" | "analyticFunctionLinear") => DeFunction::Linear,
            Some("logarithmicDEFunction" | "analyticFunctionLogarithmic") => {
                DeFunction::Logarithmic
            }
            Some("analyticFunctionIFS") => DeFunction::Ifs,
            Some("analyticFunctionUndefined" | "undefinedDEFunction") => DeFunction::Undefined,
            Some(
                "peudoKleinianDEFunction"
                | "pseudoKleinianDEFunction"
                | "analyticFunctionPeudoKleinian"
                | "analyticFunctionPseudoKleinian",
            ) => DeFunction::PseudoKleinian,
            Some(
                "joKleinianDEFunction"
                | "josKleinianDEFunction"
                | "analyticFunctionJoKleinian"
                | "analyticFunctionJosKleinian",
            ) => DeFunction::JosKleinian,
            Some(
                "cutomDEFunction"
                | "customDEFunction"
                | "analyticFunctionCutomDE"
                | "analyticFunctionCustomDE",
            ) => DeFunction::Custom,
            Some("maxAxisDEFunction" | "analyticFunctionMaxAxis") => DeFunction::MaxAxis,
            _ => DeFunction::None,
        };
        Ok(FormulaMeta {
            internal_name: field("internalName").ok_or("definition has no internalName")?,
            de_type: match field("DEType").as_deref() {
                Some("deltaDEType") => DeType::Delta,
                _ => DeType::Analytic,
            },
            de_function: function(field("DEFunctionType")),
            analytic_function: function(field("DEAnalyticFunction")),
            c_addition: match field("cpixelAddition").as_deref() {
                Some("cpixelEnabledByDefault") => CAddition::EnabledByDefault,
                Some("cpixelAlreadyHa" | "cpixelAlreadyHas") => CAddition::AlreadyHas,
                _ => CAddition::DisabledByDefault,
            },
            default_bailout: field("defaultBailout")
                .and_then(|v| v.trim_end_matches('f').parse().ok())
                .unwrap_or(10.0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fract_sections_keep_comma_decimals() {
        let file = FractFile::parse(
            "# version 2.33\n[main_parameters]\nN 47;\ncamera 0,5 -1 2;\n[fractal_1]\npower 8;\n",
        );
        assert_eq!(file.version, "2.33");
        assert_eq!(file.number("N", 250.0), 47.0);
        assert_eq!(file.vector("camera", [0.0; 3]), [0.5, -1.0, 2.0]);
        assert_eq!(file.fractal_settings(1)["power"], "8");
    }

    #[test]
    fn mandelbulb_definition_metadata() {
        let meta = FormulaMeta::parse(
            "\tinternalName = \"mandelbulb\";\n\tDEType = analyticDEType;\n\tDEFunctionType = logarithmicDEFunction;\n\tcpixelAddition = cpixelEnabledByDefault;\n\tdefaultBailout = 10.0;\n\tDEAnalyticFunction = analyticFunctionLogarithmic;\n",
        )
        .unwrap();
        assert_eq!(meta.internal_name, "mandelbulb");
        assert_eq!(meta.de_type, DeType::Analytic);
        assert_eq!(meta.analytic_function, DeFunction::Logarithmic);
        assert_eq!(meta.c_addition, CAddition::EnabledByDefault);
        assert_eq!(meta.default_bailout, 10.0);
    }
}
