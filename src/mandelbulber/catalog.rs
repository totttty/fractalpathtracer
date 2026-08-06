use anyhow::{Context, Result, anyhow, bail, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const FORMULA_CSV: &str = "deploy/formulaData.csv";
const FORMULA_ENUMS: &str = "formula/definition/all_fractal_list_enums.hpp";
const FORMULA_DEFINITIONS: &str = "formula/definition";
const FORMULA_OPENCL: &str = "formula/opencl";
const PARAMETER_SOURCE: &str = "src/initparameters.cpp";
const FRACTAL_PARAMETER_SOURCE: &str = "src/fractal.cpp";
const EXAMPLES: &str = "deploy/share/mandelbulber2/examples";

#[derive(Clone, Debug, Serialize)]
pub struct FormulaRecord {
    pub symbol: String,
    pub display_name: String,
    pub id: i32,
    pub internal_name: String,
    pub de_type: String,
    pub de_function_type: String,
    pub pixel_addition: String,
    pub default_bailout: f64,
    pub analytic_function: String,
    pub coloring_function: String,
    pub definition_path: String,
    pub opencl_path: Option<String>,
    pub iteration_function: Option<String>,
    pub parameter_reads: Vec<String>,
    pub auxiliary_fields: Vec<String>,
    pub source_features: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ParameterRecord {
    pub name: String,
    pub indexed: bool,
    pub index_expression: Option<String>,
    pub default_expression: String,
    pub enum_values: Vec<String>,
    pub source_line: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct RuntimeParameterRecord {
    pub member_expression: String,
    pub scene_key: String,
    pub value_type: String,
    pub indexed: bool,
    pub index_expression: Option<String>,
    pub source_line: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct SceneDependency {
    pub path: String,
    pub source_version: String,
    pub hybrid: bool,
    pub formula_ids: Vec<i32>,
    pub formula_symbols: Vec<String>,
    pub unresolved_formula_ids: Vec<i32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CatalogSummary {
    pub source_revision: String,
    pub source_hash_sha256: String,
    pub formulas: usize,
    pub formula_kernels: usize,
    pub analytic_de_formulas: usize,
    pub delta_de_formulas: usize,
    pub transforms: usize,
    pub parameter_defaults: usize,
    pub runtime_parameter_mappings: usize,
    pub example_scenes: usize,
    pub hybrid_scenes: usize,
    pub distinct_scene_formula_ids: usize,
    pub unresolved_scene_formula_ids: Vec<i32>,
}

#[derive(Debug, Serialize)]
struct FormulaRegistry<'a> {
    schema_version: u32,
    source_revision: &'a str,
    source_hash_sha256: &'a str,
    formulas: &'a [FormulaRecord],
}

#[derive(Debug, Serialize)]
struct ParameterSchema<'a> {
    schema_version: u32,
    source_revision: &'a str,
    parameters: &'a [ParameterRecord],
}

#[derive(Debug, Serialize)]
struct RuntimeParameterSchema<'a> {
    schema_version: u32,
    source_revision: &'a str,
    parameters: &'a [RuntimeParameterRecord],
}

#[derive(Debug, Serialize)]
struct SceneManifest<'a> {
    schema_version: u32,
    source_revision: &'a str,
    scenes: &'a [SceneDependency],
}

#[derive(Debug)]
struct DefinitionRecord {
    internal_name: String,
    path: PathBuf,
}

#[derive(Clone, Debug)]
struct FormulaMetadata {
    display_name: String,
    de_type: String,
    de_function_type: String,
    pixel_addition: String,
    default_bailout: f64,
    analytic_function: String,
    coloring_function: String,
}

fn metadata_from_csv(fields: &[String]) -> Result<FormulaMetadata> {
    ensure!(fields.len() == 8, "formulaData.csv row must have 8 columns");
    Ok(FormulaMetadata {
        display_name: fields[1].clone(),
        de_type: fields[2].clone(),
        de_function_type: fields[3].clone(),
        pixel_addition: fields[4].clone(),
        default_bailout: fields[5]
            .parse()
            .with_context(|| format!("invalid bailout {}", fields[5]))?,
        analytic_function: fields[6].clone(),
        coloring_function: fields[7].clone(),
    })
}

fn metadata_from_definition(path: &Path) -> Result<FormulaMetadata> {
    let source = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let value = |key: &str| {
        assignment_value(&source, key)
            .ok_or_else(|| anyhow!("{} has no {key} constructor assignment", path.display()))
    };
    let default_bailout = value("defaultBailout")?
        .parse::<f64>()
        .with_context(|| format!("invalid defaultBailout in {}", path.display()))?;
    Ok(FormulaMetadata {
        display_name: quoted_assignment(&source, "nameInComboBox")
            .ok_or_else(|| anyhow!("{} has no formula display name", path.display()))?,
        de_type: value("DEType")?,
        de_function_type: value("DEFunctionType")?,
        pixel_addition: value("cpixelAddition")?,
        default_bailout,
        analytic_function: value("DEAnalyticFunction")?,
        coloring_function: value("coloringFunction")?,
    })
}

fn build_formula_record(
    source_root: &Path,
    symbol: String,
    id: i32,
    definition: &DefinitionRecord,
    metadata: FormulaMetadata,
    source_hasher: &mut Sha256,
) -> Result<FormulaRecord> {
    let opencl = source_root
        .join(FORMULA_OPENCL)
        .join(format!("{}.cl", definition.internal_name));
    let (opencl_path, iteration_function, parameter_reads, auxiliary_fields, source_features) =
        if opencl.exists() {
            hash_file(source_hasher, &opencl)?;
            let source = fs::read_to_string(&opencl)
                .with_context(|| format!("read {}", opencl.display()))?;
            (
                Some(relative_path(source_root, &opencl)),
                find_iteration_function(&source),
                collect_member_paths(&source, "fractal->"),
                collect_member_paths(&source, "aux->"),
                classify_source_features(&source),
            )
        } else {
            (
                None,
                None,
                Vec::new(),
                Vec::new(),
                vec!["custom-source".into()],
            )
        };
    Ok(FormulaRecord {
        symbol,
        display_name: metadata.display_name,
        id,
        internal_name: definition.internal_name.clone(),
        de_type: metadata.de_type,
        de_function_type: metadata.de_function_type,
        pixel_addition: metadata.pixel_addition,
        default_bailout: metadata.default_bailout,
        analytic_function: metadata.analytic_function,
        coloring_function: metadata.coloring_function,
        definition_path: relative_path(source_root, &definition.path),
        opencl_path,
        iteration_function,
        parameter_reads,
        auxiliary_fields,
        source_features,
    })
}

#[derive(Clone, Debug)]
pub struct ResolvedFormulaSource {
    pub symbol: String,
    pub id: i32,
    pub internal_name: String,
    pub path: PathBuf,
    pub de_type: String,
    pub de_function_type: String,
    pub pixel_addition: String,
    pub default_bailout: f64,
    pub analytic_function: String,
    pub coloring_function: String,
}

pub fn resolve_formula_source(root: &Path, identifier: &str) -> Result<ResolvedFormulaSource> {
    validate_source_root(root)?;
    let ids = parse_formula_enums(&root.join(FORMULA_ENUMS))?;
    let definitions = parse_formula_definitions(&root.join(FORMULA_DEFINITIONS))?;
    let csv = fs::read_to_string(root.join(FORMULA_CSV))
        .with_context(|| format!("read {}", root.join(FORMULA_CSV).display()))?;
    let symbol = if let Ok(id) = identifier.parse::<i32>() {
        ids.iter()
            .find_map(|(symbol, value)| (*value == id).then(|| symbol.clone()))
            .ok_or_else(|| anyhow!("unknown Mandelbulber formula ID {id}"))?
    } else {
        identifier.to_owned()
    };
    let id = *ids
        .get(&symbol)
        .ok_or_else(|| anyhow!("unknown Mandelbulber formula symbol {symbol}"))?;
    let definition = definitions
        .get(&symbol)
        .ok_or_else(|| anyhow!("formula {symbol} has no definition source"))?;
    let csv_rows = parse_csv(&csv)?;
    let metadata = if let Some(fields) = csv_rows
        .iter()
        .find(|fields| fields.len() == 8 && fields[0].trim_start_matches('\u{feff}') == symbol)
    {
        metadata_from_csv(fields)?
    } else {
        metadata_from_definition(&definition.path)?
    };
    let path = root
        .join(FORMULA_OPENCL)
        .join(format!("{}.cl", definition.internal_name));
    ensure!(
        path.exists(),
        "formula {symbol} uses custom source and cannot be imported from the catalog"
    );
    Ok(ResolvedFormulaSource {
        symbol,
        id,
        internal_name: definition.internal_name.clone(),
        path,
        de_type: metadata.de_type,
        de_function_type: metadata.de_function_type,
        pixel_addition: metadata.pixel_addition,
        default_bailout: metadata.default_bailout,
        analytic_function: metadata.analytic_function,
        coloring_function: metadata.coloring_function,
    })
}

pub fn formula_symbols(root: &Path) -> Result<Vec<String>> {
    validate_source_root(root)?;
    let csv = fs::read_to_string(root.join(FORMULA_CSV))
        .with_context(|| format!("read {}", root.join(FORMULA_CSV).display()))?;
    let definitions = parse_formula_definitions(&root.join(FORMULA_DEFINITIONS))?;
    let ids = parse_formula_enums(&root.join(FORMULA_ENUMS))?;
    let mut symbols = parse_csv(&csv)?
        .into_iter()
        .filter(|fields| fields.len() == 8 && fields[0] != "index")
        .map(|fields| fields[0].trim_start_matches('\u{feff}').to_owned())
        .collect::<Vec<_>>();
    symbols.extend(
        definitions
            .into_keys()
            .filter(|symbol| ids.get(symbol).is_some_and(|id| *id > 0)),
    );
    symbols.sort();
    symbols.dedup();
    Ok(symbols)
}

pub fn parameter_defaults(root: &Path) -> Result<Vec<ParameterRecord>> {
    validate_source_root(root)?;
    parse_parameter_defaults(&root.join(PARAMETER_SOURCE))
}

pub fn runtime_parameter_mappings(root: &Path) -> Result<Vec<RuntimeParameterRecord>> {
    validate_source_root(root)?;
    parse_runtime_parameter_mappings(&root.join(FRACTAL_PARAMETER_SOURCE))
}

pub fn example_scene_paths(root: &Path) -> Result<Vec<PathBuf>> {
    validate_source_root(root)?;
    sorted_files(&root.join(EXAMPLES), Some("fract"))
}

pub fn generate_catalog(source_root: &Path, output_dir: &Path) -> Result<CatalogSummary> {
    validate_source_root(source_root)?;
    let enum_ids = parse_formula_enums(&source_root.join(FORMULA_ENUMS))?;
    let definitions = parse_formula_definitions(&source_root.join(FORMULA_DEFINITIONS))?;
    let csv = fs::read_to_string(source_root.join(FORMULA_CSV))
        .with_context(|| format!("read {}", source_root.join(FORMULA_CSV).display()))?;

    let mut formulas = Vec::new();
    let mut source_hasher = Sha256::new();
    hash_file(&mut source_hasher, &source_root.join(FORMULA_CSV))?;
    hash_file(&mut source_hasher, &source_root.join(FORMULA_ENUMS))?;
    hash_file(&mut source_hasher, &source_root.join(PARAMETER_SOURCE))?;
    hash_file(
        &mut source_hasher,
        &source_root.join(FRACTAL_PARAMETER_SOURCE),
    )?;

    let csv_rows = parse_csv(&csv)?;
    let mut catalog_symbols = BTreeSet::new();
    for fields in csv_rows {
        ensure!(fields.len() == 8, "formulaData.csv row must have 8 columns");
        if fields[0] == "index" {
            continue;
        }
        let symbol = fields[0].trim_start_matches('\u{feff}').to_owned();
        catalog_symbols.insert(symbol.clone());
        let id = *enum_ids
            .get(&symbol)
            .ok_or_else(|| anyhow!("formula {symbol} is missing from {FORMULA_ENUMS}"))?;
        let definition = definitions
            .get(&symbol)
            .ok_or_else(|| anyhow!("formula {symbol} has no definition source"))?;
        let metadata = metadata_from_csv(&fields)?;
        formulas.push(build_formula_record(
            source_root,
            symbol,
            id,
            definition,
            metadata,
            &mut source_hasher,
        )?);
    }

    // formulaData.csv is a generated conversion manifest rather than the
    // authoritative formula registry. Some fixed formulas used by bundled
    // examples (notably IDs 282 and 283) have complete constructors and
    // OpenCL kernels but are absent from the CSV. Recover their metadata from
    // the constructor so scene compatibility is not limited by that manifest.
    for (symbol, definition) in &definitions {
        if catalog_symbols.contains(symbol) {
            continue;
        }
        let Some(&id) = enum_ids.get(symbol) else {
            continue;
        };
        if id == 0 {
            continue;
        }
        let metadata = metadata_from_definition(&definition.path)?;
        hash_file(&mut source_hasher, &definition.path)?;
        formulas.push(build_formula_record(
            source_root,
            symbol.clone(),
            id,
            definition,
            metadata,
            &mut source_hasher,
        )?);
    }
    formulas.sort_by_key(|formula| formula.id);

    let parameters = parse_parameter_defaults(&source_root.join(PARAMETER_SOURCE))?;
    let runtime_parameters =
        parse_runtime_parameter_mappings(&source_root.join(FRACTAL_PARAMETER_SOURCE))?;
    let scenes = scan_example_scenes(source_root, &formulas)?;
    let source_hash = format!("{:x}", source_hasher.finalize());
    let source_revision = git_revision(source_root).unwrap_or_else(|| source_hash[..12].to_owned());
    let scene_formula_ids = scenes
        .iter()
        .flat_map(|scene| scene.formula_ids.iter().copied())
        .collect::<BTreeSet<_>>();
    let unresolved_scene_formula_ids = scenes
        .iter()
        .flat_map(|scene| scene.unresolved_formula_ids.iter().copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let summary = CatalogSummary {
        source_revision: source_revision.clone(),
        source_hash_sha256: source_hash.clone(),
        formulas: formulas.len(),
        formula_kernels: formulas
            .iter()
            .filter(|formula| formula.opencl_path.is_some())
            .count(),
        analytic_de_formulas: formulas
            .iter()
            .filter(|formula| formula.de_type == "analyticDEType")
            .count(),
        delta_de_formulas: formulas
            .iter()
            .filter(|formula| formula.de_type == "deltaDEType")
            .count(),
        transforms: formulas
            .iter()
            .filter(|formula| formula.internal_name.starts_with("transf_"))
            .count(),
        parameter_defaults: parameters.len(),
        runtime_parameter_mappings: runtime_parameters.len(),
        example_scenes: scenes.len(),
        hybrid_scenes: scenes.iter().filter(|scene| scene.hybrid).count(),
        distinct_scene_formula_ids: scene_formula_ids.len(),
        unresolved_scene_formula_ids,
    };

    fs::create_dir_all(output_dir).with_context(|| format!("create {}", output_dir.display()))?;
    write_json(
        &output_dir.join("formula-registry.json"),
        &FormulaRegistry {
            schema_version: 1,
            source_revision: &source_revision,
            source_hash_sha256: &source_hash,
            formulas: &formulas,
        },
    )?;
    write_json(
        &output_dir.join("parameter-schema.json"),
        &ParameterSchema {
            schema_version: 1,
            source_revision: &source_revision,
            parameters: &parameters,
        },
    )?;
    write_json(
        &output_dir.join("runtime-parameter-schema.json"),
        &RuntimeParameterSchema {
            schema_version: 1,
            source_revision: &source_revision,
            parameters: &runtime_parameters,
        },
    )?;
    write_json(
        &output_dir.join("scene-dependencies.json"),
        &SceneManifest {
            schema_version: 1,
            source_revision: &source_revision,
            scenes: &scenes,
        },
    )?;
    write_json(&output_dir.join("catalog-summary.json"), &summary)?;
    Ok(summary)
}

fn validate_source_root(root: &Path) -> Result<()> {
    ensure!(
        root.is_dir(),
        "Mandelbulber source root does not exist: {}",
        root.display()
    );
    for relative in [
        FORMULA_CSV,
        FORMULA_ENUMS,
        PARAMETER_SOURCE,
        FRACTAL_PARAMETER_SOURCE,
        EXAMPLES,
    ] {
        ensure!(
            root.join(relative).exists(),
            "missing Mandelbulber source path {relative}"
        );
    }
    Ok(())
}

fn parse_formula_enums(path: &Path) -> Result<BTreeMap<String, i32>> {
    let source = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut result = BTreeMap::new();
    for raw_line in source.lines() {
        let line = raw_line.split("//").next().unwrap_or("").trim();
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|character| character == '_' || character.is_ascii_alphanumeric())
        {
            continue;
        }
        let value = value.trim().trim_end_matches(',').trim();
        if let Ok(value) = value.parse::<i32>() {
            result.insert(name.to_owned(), value);
        }
    }
    ensure!(
        !result.is_empty(),
        "no formula IDs found in {}",
        path.display()
    );
    Ok(result)
}

fn parse_formula_definitions(directory: &Path) -> Result<BTreeMap<String, DefinitionRecord>> {
    let mut result = BTreeMap::new();
    for path in sorted_files(directory, Some("cpp"))? {
        if !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("fractal_"))
        {
            continue;
        }
        let source =
            fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let Some(internal_name) = quoted_assignment(&source, "internalName") else {
            continue;
        };
        let Some(symbol) = symbolic_assignment(&source, "internalID", "fractal::") else {
            continue;
        };
        let prior = result.insert(
            symbol.clone(),
            DefinitionRecord {
                internal_name,
                path,
            },
        );
        ensure!(prior.is_none(), "duplicate formula definition for {symbol}");
    }
    Ok(result)
}

fn quoted_assignment(source: &str, key: &str) -> Option<String> {
    let start = source.find(key)?;
    let remainder = &source[start + key.len()..];
    let quote = remainder.find('"')?;
    let value = &remainder[quote + 1..];
    Some(value[..value.find('"')?].to_owned())
}

fn assignment_value(source: &str, key: &str) -> Option<String> {
    let start = source.find(key)?;
    let remainder = &source[start + key.len()..];
    let equals = remainder.find('=')?;
    let value = &remainder[equals + 1..];
    Some(value[..value.find(';')?].trim().to_owned())
}

fn symbolic_assignment(source: &str, key: &str, prefix: &str) -> Option<String> {
    let start = source.find(key)?;
    let remainder = &source[start + key.len()..];
    let prefix = remainder.find(prefix)? + prefix.len();
    let value = &remainder[prefix..];
    let end = value
        .find(|character: char| !(character == '_' || character.is_ascii_alphanumeric()))
        .unwrap_or(value.len());
    Some(value[..end].to_owned())
}

fn parse_csv(source: &str) -> Result<Vec<Vec<String>>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = source.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => row.push(std::mem::take(&mut field)),
            '\n' if !quoted => {
                if field.ends_with('\r') {
                    field.pop();
                }
                row.push(std::mem::take(&mut field));
                if row.iter().any(|value| !value.is_empty()) {
                    rows.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
            }
            value => field.push(value),
        }
    }
    ensure!(!quoted, "unterminated quoted CSV field");
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}

fn find_iteration_function(source: &str) -> Option<String> {
    let marker = "Iteration(";
    let end = source.find(marker)? + "Iteration".len();
    let prefix = &source[..end];
    let start = prefix
        .rfind(|character: char| !(character == '_' || character.is_ascii_alphanumeric()))
        .map_or(0, |index| index + 1);
    Some(prefix[start..].to_owned())
}

fn collect_member_paths(source: &str, prefix: &str) -> Vec<String> {
    let mut result = BTreeSet::new();
    let mut remainder = source;
    while let Some(index) = remainder.find(prefix) {
        remainder = &remainder[index + prefix.len()..];
        let end = remainder
            .find(|character: char| {
                !(character == '_' || character == '.' || character.is_ascii_alphanumeric())
            })
            .unwrap_or(remainder.len());
        if end > 0 {
            result.insert(remainder[..end].to_owned());
        }
        remainder = &remainder[end..];
    }
    result.into_iter().collect()
}

fn classify_source_features(source: &str) -> Vec<String> {
    let checks = [
        ("for (", "loop"),
        ("while (", "while-loop"),
        ("switch (", "switch"),
        ("Matrix33", "matrix-3x3"),
        ("Matrix44", "matrix-4x4"),
        ("native_", "native-math"),
        (".w", "four-dimensional"),
        ("#ifdef", "conditional-compilation"),
        ("?", "ternary"),
    ];
    checks
        .into_iter()
        .filter(|(needle, _)| source.contains(needle))
        .map(|(_, feature)| feature.to_owned())
        .collect()
}

fn parse_parameter_defaults(path: &Path) -> Result<Vec<ParameterRecord>> {
    let source = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let enum_lists = parse_qstring_lists(&source);
    let marker = "->addParam(";
    let mut records = Vec::new();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find(marker) {
        let call_start = offset + relative + marker.len();
        let call_end = matching_parenthesis(&source, call_start)?;
        let arguments = split_arguments(&source[call_start..call_end]);
        if arguments.len() >= 2
            && let Some(name) = quoted_literal(arguments[0].trim())
        {
            let indexed = arguments.len() >= 3
                && !looks_like_default(arguments[1].trim())
                && looks_like_default(arguments[2].trim());
            let (index_expression, default_expression) = if indexed {
                (
                    Some(arguments[1].trim().to_owned()),
                    arguments[2].trim().to_owned(),
                )
            } else {
                (None, arguments[1].trim().to_owned())
            };
            let enum_values = arguments
                .last()
                .and_then(|argument| enum_lists.get(argument.trim()).cloned())
                .or_else(|| {
                    arguments.last().and_then(|argument| {
                        argument
                            .contains("QStringList")
                            .then(|| quoted_literals(argument))
                    })
                })
                .unwrap_or_default();
            records.push(ParameterRecord {
                name,
                indexed,
                index_expression,
                default_expression,
                enum_values,
                source_line: source[..call_start]
                    .bytes()
                    .filter(|byte| *byte == b'\n')
                    .count()
                    + 1,
            });
        }
        offset = call_end + 1;
    }
    records.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.source_line.cmp(&right.source_line))
    });
    ensure!(
        !records.is_empty(),
        "no parameter defaults found in {}",
        path.display()
    );
    Ok(records)
}

fn parse_qstring_lists(source: &str) -> BTreeMap<String, Vec<String>> {
    let marker = "QStringList ";
    let mut lists = BTreeMap::new();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find(marker) {
        let start = offset + relative + marker.len();
        let name_end = source[start..]
            .find(|character: char| !(character == '_' || character.is_ascii_alphanumeric()))
            .map(|relative| start + relative)
            .unwrap_or(source.len());
        let name = source[start..name_end].trim();
        let Some(statement_end) = source[name_end..]
            .find(';')
            .map(|relative| name_end + relative)
        else {
            break;
        };
        let statement = &source[name_end..statement_end];
        let values = quoted_literals(statement);
        if !name.is_empty() && !values.is_empty() {
            lists.insert(name.to_owned(), values);
        }
        offset = statement_end + 1;
    }
    lists
}

fn quoted_literals(source: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut remainder = source;
    while let Some(quote) = remainder.find('"') {
        remainder = &remainder[quote + 1..];
        let Some(end) = remainder.find('"') else {
            break;
        };
        values.push(remainder[..end].to_owned());
        remainder = &remainder[end + 1..];
    }
    values
}

fn parse_runtime_parameter_mappings(path: &Path) -> Result<Vec<RuntimeParameterRecord>> {
    let source = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let constructor = source
        .split("void sFractal::RecalculateFractalParams()")
        .next()
        .unwrap_or(&source);
    let marker = "container->Get<";
    let mut records = BTreeMap::new();
    let mut offset = 0;
    while let Some(relative) = constructor[offset..].find(marker) {
        let marker_start = offset + relative;
        let type_start = marker_start + marker.len();
        let type_end = constructor[type_start..]
            .find('>')
            .map(|relative| type_start + relative)
            .ok_or_else(|| anyhow!("unterminated container Get type in {}", path.display()))?;
        let value_type = constructor[type_start..type_end].trim().to_owned();
        let open = constructor[type_end..]
            .find('(')
            .map(|relative| type_end + relative)
            .ok_or_else(|| anyhow!("container Get has no arguments in {}", path.display()))?;
        let close = matching_parenthesis(constructor, open + 1)?;
        let arguments = split_arguments(&constructor[open + 1..close]);
        let Some(scene_key) = arguments
            .first()
            .and_then(|argument| quoted_literal(argument.trim()))
        else {
            offset = close + 1;
            continue;
        };
        let before_call = &constructor[..marker_start];
        let Some(equals) = before_call.rfind('=') else {
            offset = close + 1;
            continue;
        };
        let member_expression = before_call[..equals]
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .map(str::trim)
            .unwrap_or("")
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        if member_expression.is_empty()
            || !member_expression
                .chars()
                .next()
                .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        {
            offset = close + 1;
            continue;
        }
        let index_expression = arguments
            .get(1)
            .map(|argument| argument.trim().to_owned())
            .filter(|argument| !argument.is_empty());
        let record = RuntimeParameterRecord {
            member_expression: member_expression.clone(),
            scene_key: scene_key.clone(),
            value_type,
            indexed: index_expression.is_some(),
            index_expression: index_expression.clone(),
            source_line: constructor[..marker_start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1,
        };
        records.insert((member_expression, scene_key, index_expression), record);
        offset = close + 1;
    }
    ensure!(
        !records.is_empty(),
        "no runtime parameter mappings found in {}",
        path.display()
    );
    Ok(records.into_values().collect())
}

fn matching_parenthesis(source: &str, start: usize) -> Result<usize> {
    let mut depth = 1usize;
    let mut quoted = false;
    let mut escaped = false;
    for (relative, character) in source[start..].char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = false;
            }
            continue;
        }
        match character {
            '"' => quoted = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(start + relative);
                }
            }
            _ => {}
        }
    }
    bail!("unterminated addParam call")
}

fn split_arguments(source: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for (index, character) in source.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = false;
            }
            continue;
        }
        match character {
            '"' => quoted = true,
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
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
    let value = value.strip_prefix('"')?;
    Some(value.strip_suffix('"')?.to_owned())
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

fn scan_example_scenes(root: &Path, formulas: &[FormulaRecord]) -> Result<Vec<SceneDependency>> {
    let id_to_symbol = formulas
        .iter()
        .map(|formula| (formula.id, formula.symbol.as_str()))
        .collect::<BTreeMap<_, _>>();
    let example_root = root.join(EXAMPLES);
    let mut scenes = Vec::new();
    for path in sorted_files(&example_root, Some("fract"))? {
        let source =
            fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let (source_version, main_parameters) = parse_scene_metadata(&source);
        let mut formula_ids = Vec::new();
        for slot in 1..=9 {
            if main_parameters
                .get(&format!("fractal_enable_{slot}"))
                .is_some_and(|value| value == "false")
            {
                continue;
            }
            let key = format!("formula_{slot}");
            let id = if let Some(value) = main_parameters.get(&key) {
                value
                    .replace(',', ".")
                    .parse::<i32>()
                    .with_context(|| format!("invalid {} in {}", key, path.display()))?
            } else if slot == 1 {
                2
            } else {
                continue;
            };
            if id != 0 {
                formula_ids.push(id);
            }
        }
        formula_ids.sort_unstable();
        formula_ids.dedup();
        let mut formula_symbols = Vec::new();
        let mut unresolved_formula_ids = Vec::new();
        for id in &formula_ids {
            if let Some(symbol) = id_to_symbol.get(id) {
                formula_symbols.push((*symbol).to_owned());
            } else {
                unresolved_formula_ids.push(*id);
            }
        }
        scenes.push(SceneDependency {
            path: relative_path(root, &path),
            source_version,
            hybrid: main_parameters
                .get("hybrid_fractal_enable")
                .is_some_and(|value| value == "true"),
            formula_ids,
            formula_symbols,
            unresolved_formula_ids,
        });
    }
    Ok(scenes)
}

fn parse_scene_metadata(source: &str) -> (String, BTreeMap<String, String>) {
    let mut version = "unknown".to_owned();
    let mut in_main_parameters = false;
    let mut parameters = BTreeMap::new();
    for raw_line in source.lines() {
        let line = raw_line.trim();
        if let Some(value) = line.strip_prefix("# version ") {
            version = value.trim().to_owned();
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_main_parameters = line == "[main_parameters]";
            continue;
        }
        if !in_main_parameters || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(statement) = line.strip_suffix(';') else {
            continue;
        };
        let Some(split) = statement.find(char::is_whitespace) else {
            continue;
        };
        parameters.insert(
            statement[..split].trim().to_owned(),
            statement[split..].trim().to_owned(),
        );
    }
    (version, parameters)
}

fn sorted_files(root: &Path, extension: Option<&str>) -> Result<Vec<PathBuf>> {
    fn visit(path: &Path, extension: Option<&str>, files: &mut Vec<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(path).with_context(|| format!("read {}", path.display()))? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit(&path, extension, files)?;
            } else if extension.is_none_or(|expected| {
                path.extension().and_then(|value| value.to_str()) == Some(expected)
            }) {
                files.push(path);
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    visit(root, extension, &mut files)?;
    files.sort();
    Ok(files)
}

fn hash_file(hasher: &mut Sha256, path: &Path) -> Result<()> {
    hasher.update(relative_path(path.parent().unwrap_or(Path::new("")), path));
    hasher.update(fs::read(path).with_context(|| format!("read {}", path.display()))?);
    Ok(())
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn git_revision(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let data = serde_json::to_vec_pretty(value)?;
    fs::write(path, [data, b"\n".to_vec()].concat())
        .with_context(|| format!("write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_parser_handles_quoted_names() {
        let rows = parse_csv("index,name\nfoo,\"A, B\"\n").expect("parse CSV");
        assert_eq!(rows[1], ["foo", "A, B"]);
    }

    #[test]
    fn member_paths_are_sorted_and_deduplicated() {
        assert_eq!(
            collect_member_paths("aux->DE += fractal->IFS.scale; aux->DE *= 2;", "aux->"),
            ["DE"]
        );
        assert_eq!(
            collect_member_paths("fractal->IFS.scale + fractal->IFS.offset.x", "fractal->"),
            ["IFS.offset.x", "IFS.scale"]
        );
    }

    #[test]
    fn balanced_argument_split_preserves_vector_constructor() {
        let values = split_arguments("\"camera\", CVector3(1.0, 2.0, 3.0), morphNone");
        assert_eq!(
            values,
            ["\"camera\"", " CVector3(1.0, 2.0, 3.0)", " morphNone"]
        );
    }
}
