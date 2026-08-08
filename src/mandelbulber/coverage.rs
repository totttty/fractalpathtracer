use super::{MandelbulberScene, catalog, compiler};
use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Serialize)]
pub struct FormulaCoverage {
    pub id: i32,
    pub symbol: Option<String>,
    pub internal_name: Option<String>,
    pub de_type: Option<String>,
    pub de_function_type: Option<String>,
    pub analytic_function: Option<String>,
    pub parameter_read_count: usize,
    pub parameter_roots: Vec<String>,
    pub auxiliary_fields: Vec<String>,
    pub scene_count: usize,
    pub runtime_kernel_supported: bool,
    pub runtime_supported: bool,
    pub blockers: Vec<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SceneCoverage {
    pub path: String,
    pub source_version: String,
    pub hybrid: bool,
    pub formula_ids: Vec<i32>,
    pub runtime_supported_formula_ids: Vec<i32>,
    pub blocked_formula_ids: Vec<i32>,
    pub unresolved_formula_ids: Vec<i32>,
    pub feature_flags: Vec<String>,
    pub runtime_compatible: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CoverageSummary {
    pub source_revision: String,
    pub catalog_formulas: usize,
    pub scene_used_formulas: usize,
    pub runtime_supported_scene_formulas: usize,
    pub runtime_kernel_supported_scene_formulas: usize,
    pub total_scenes: usize,
    pub runtime_compatible_scenes: usize,
    pub blocked_scenes: usize,
    pub compatible_non_hybrid_scenes: usize,
    pub compatible_hybrid_scenes: usize,
    pub scenes_one_formula_away: usize,
    pub blocker_counts: BTreeMap<String, usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CoverageReport {
    pub schema_version: u32,
    pub summary: CoverageSummary,
    pub formulas: Vec<FormulaCoverage>,
    pub scenes: Vec<SceneCoverage>,
}

pub fn generate_coverage(
    source_root: &Path,
    catalog_dir: &Path,
    report_path: &Path,
) -> Result<CoverageReport> {
    let catalog_summary = catalog::generate_catalog(source_root, catalog_dir)?;
    let registry: Value = read_json(&catalog_dir.join("formula-registry.json"))?;
    let manifest: Value = read_json(&catalog_dir.join("scene-dependencies.json"))?;
    let records = registry
        .get("formulas")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("formula registry has no formulas array"))?;
    let scene_values = manifest
        .get("scenes")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("scene manifest has no scenes array"))?;

    let records_by_id = records
        .iter()
        .filter_map(|record| Some((record.get("id")?.as_i64()? as i32, record)))
        .collect::<BTreeMap<_, _>>();
    let mut scene_counts = BTreeMap::<i32, usize>::new();
    let mut scene_formula_ids = BTreeSet::new();
    for scene in scene_values {
        for id in integer_array(scene.get("formula_ids")) {
            scene_formula_ids.insert(id);
            *scene_counts.entry(id).or_default() += 1;
        }
    }

    let mut formulas = Vec::with_capacity(scene_formula_ids.len());
    let mut runtime_supported = BTreeSet::new();
    let mut runtime_kernel_supported = BTreeSet::new();
    for id in scene_formula_ids.iter().copied() {
        let Some(record) = records_by_id.get(&id).copied() else {
            formulas.push(FormulaCoverage {
                id,
                symbol: None,
                internal_name: None,
                de_type: None,
                de_function_type: None,
                analytic_function: None,
                parameter_read_count: 0,
                parameter_roots: Vec::new(),
                auxiliary_fields: Vec::new(),
                scene_count: scene_counts.get(&id).copied().unwrap_or(0),
                runtime_kernel_supported: false,
                runtime_supported: false,
                blockers: vec!["missing_catalog_record".into()],
                error: None,
            });
            continue;
        };

        let symbol = string_field(record, "symbol");
        let internal_name = string_field(record, "internal_name");
        let de_type = string_field(record, "de_type");
        let de_function_type = string_field(record, "de_function_type");
        let analytic_function = string_field(record, "analytic_function");
        let parameter_reads = string_array(record.get("parameter_reads"));
        let parameter_roots = parameter_reads
            .iter()
            .filter_map(|path| path.split('.').next().map(str::to_owned))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let auxiliary_fields = string_array(record.get("auxiliary_fields"));
        let custom_source = record.get("opencl_path").is_none_or(Value::is_null);
        let mut blockers = Vec::new();
        let mut error = None;
        let (kernel_supported, supported) = if custom_source {
            blockers.push("custom_source".into());
            (false, false)
        } else {
            match compiler::parse_formula(source_root, &id.to_string()) {
                Ok(formula) => {
                    let kernel_supported = compiler::supports_runtime_kernel(&formula);
                    let supported = compiler::supports_runtime_emitter(&formula);
                    if !kernel_supported && formula.source.de_type == "deltaDEType" {
                        blockers.push("delta_de".into());
                    }
                    if kernel_supported && !supported {
                        blockers.push("unsupported_finalizer".into());
                    }
                    if !kernel_supported
                        && formula.source.de_type == "analyticDEType"
                        && blockers.is_empty()
                        && !formula.parameter_reads.is_empty()
                    {
                        blockers.push("parameter_binding".into());
                    }
                    if !supported && blockers.is_empty() {
                        blockers.push("runtime_emitter".into());
                    }
                    (kernel_supported, supported)
                }
                Err(parse_error) => {
                    blockers.push("formula_parse".into());
                    error = Some(format!("{parse_error:#}"));
                    (false, false)
                }
            }
        };
        if kernel_supported {
            runtime_kernel_supported.insert(id);
        }
        if supported {
            runtime_supported.insert(id);
        }
        formulas.push(FormulaCoverage {
            id,
            symbol,
            internal_name,
            de_type,
            de_function_type,
            analytic_function,
            parameter_read_count: parameter_reads.len(),
            parameter_roots,
            auxiliary_fields,
            scene_count: scene_counts.get(&id).copied().unwrap_or(0),
            runtime_kernel_supported: kernel_supported,
            runtime_supported: supported,
            blockers,
            error,
        });
    }

    let mut scenes = Vec::with_capacity(scene_values.len());
    for scene in scene_values {
        let path = string_field(scene, "path").unwrap_or_default();
        let formula_ids = integer_array(scene.get("formula_ids"));
        let unresolved_formula_ids = integer_array(scene.get("unresolved_formula_ids"));
        let hybrid = scene
            .get("hybrid")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let supported_formulas = if hybrid {
            &runtime_kernel_supported
        } else {
            &runtime_supported
        };
        let custom_supported = if formula_ids.contains(&10_000) {
            MandelbulberScene::load(&source_root.join(&path))
                .ok()
                .is_some_and(|scene| {
                    scene
                        .formula_slots
                        .iter()
                        .filter(|slot| slot.active() && slot.formula_id == 10_000)
                        .all(|slot| {
                            slot.parameters
                                .get("formula_code")
                                .and_then(|encoded| {
                                    compiler::parse_embedded_custom_formula(source_root, encoded)
                                        .ok()
                                })
                                .is_some_and(|formula| {
                                    if hybrid {
                                        compiler::supports_runtime_kernel(&formula)
                                    } else {
                                        compiler::supports_runtime_emitter(&formula)
                                    }
                                })
                        })
                })
        } else {
            false
        };
        let runtime_supported_formula_ids = formula_ids
            .iter()
            .copied()
            .filter(|id| supported_formulas.contains(id) || (*id == 10_000 && custom_supported))
            .collect::<Vec<_>>();
        let blocked_formula_ids = formula_ids
            .iter()
            .copied()
            .filter(|id| !supported_formulas.contains(id) && !(*id == 10_000 && custom_supported))
            .collect::<Vec<_>>();
        let feature_flags = detect_scene_features(&source_root.join(&path))?;
        scenes.push(SceneCoverage {
            path,
            source_version: string_field(scene, "source_version").unwrap_or_default(),
            hybrid,
            formula_ids,
            runtime_supported_formula_ids,
            runtime_compatible: blocked_formula_ids.is_empty() && unresolved_formula_ids.is_empty(),
            blocked_formula_ids,
            unresolved_formula_ids,
            feature_flags,
        });
    }

    let mut blocker_counts = BTreeMap::new();
    for formula in &formulas {
        for blocker in &formula.blockers {
            *blocker_counts.entry(blocker.clone()).or_default() += formula.scene_count;
        }
    }
    let runtime_compatible_scenes = scenes
        .iter()
        .filter(|scene| scene.runtime_compatible)
        .count();
    let report = CoverageReport {
        schema_version: 1,
        summary: CoverageSummary {
            source_revision: catalog_summary.source_revision,
            catalog_formulas: catalog_summary.formulas,
            scene_used_formulas: scene_formula_ids.len(),
            runtime_supported_scene_formulas: runtime_supported.len(),
            runtime_kernel_supported_scene_formulas: runtime_kernel_supported.len(),
            total_scenes: scenes.len(),
            runtime_compatible_scenes,
            blocked_scenes: scenes.len() - runtime_compatible_scenes,
            compatible_non_hybrid_scenes: scenes
                .iter()
                .filter(|scene| scene.runtime_compatible && !scene.hybrid)
                .count(),
            compatible_hybrid_scenes: scenes
                .iter()
                .filter(|scene| scene.runtime_compatible && scene.hybrid)
                .count(),
            scenes_one_formula_away: scenes
                .iter()
                .filter(|scene| {
                    !scene.runtime_compatible
                        && scene.blocked_formula_ids.len() + scene.unresolved_formula_ids.len() == 1
                })
                .count(),
            blocker_counts,
        },
        formulas,
        scenes,
    };
    if let Some(parent) = report_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create coverage directory {}", parent.display()))?;
    }
    fs::write(
        report_path,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )
    .with_context(|| format!("write {}", report_path.display()))?;
    Ok(report)
}

fn read_json(path: &Path) -> Result<Value> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("read {}", path.display()))?)
        .with_context(|| format!("parse {}", path.display()))
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_owned)
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn integer_array(value: Option<&Value>) -> Vec<i32> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .map(|id| id as i32)
        .collect()
}

fn detect_scene_features(path: &Path) -> Result<Vec<String>> {
    let source = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    const FLAGS: &[&str] = &[
        "nebula_mode",
        "limits_enabled",
        "volumetric_fog_enabled",
        "basic_fog_enabled",
        "iteration_fog_enable",
        "clouds_enable",
        "DOF_enabled",
        "stereo_enabled",
        "boolean_operators_enabled",
    ];
    let mut features = BTreeSet::new();
    for raw_line in source.lines() {
        let line = raw_line.trim();
        if let Some((key, value)) = line.trim_end_matches(';').split_once(' ')
            && value == "true"
            && (FLAGS.contains(&key)
                || (key.starts_with("primitive_") && key.ends_with("_enabled")))
        {
            features.insert(key.to_owned());
        }
    }
    Ok(features.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_arrays_ignore_non_numbers() {
        let value = serde_json::json!([7, "8", 9]);
        assert_eq!(integer_array(Some(&value)), vec![7, 9]);
    }

    #[test]
    fn feature_detection_is_limited_to_enabled_scene_flags() {
        let path = std::env::temp_dir().join(format!(
            "fpt-metal-mandel-features-{}.fract",
            std::process::id()
        ));
        fs::write(
            &path,
            "nebula_mode true;\nlimits_enabled false;\nprimitive_plane_1_enabled true;\n",
        )
        .unwrap();
        let features = detect_scene_features(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(features, vec!["nebula_mode", "primitive_plane_1_enabled"]);
    }
}
