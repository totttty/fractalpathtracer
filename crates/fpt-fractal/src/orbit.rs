//! Orbits and distance estimates in f64, mirroring Mandelbulber's `Compute`
//! (compute_fractal.cpp), `cNineFractals` (nine_fractals.cpp) and
//! `CalculateDistanceSimple` (calculate_distance.cpp) for single-formula and
//! hybrid scenes.

use std::collections::HashMap;

use crate::ast::Function;
use crate::eval::{Aux, EvalError, Parameters, Value, call_formula};
use crate::scene::{CAddition, DeFunction, DeType, FormulaMeta, FractFile};

/// Mandelbulber's formula slot count.
pub const SLOTS: usize = 9;

/// A parsed formula for one slot (`slot` is 1-based), as the caller loads it.
#[derive(Clone, Debug)]
pub struct FormulaSource {
    pub slot: usize,
    pub function: Function,
    pub meta: FormulaMeta,
    pub parameters: Parameters,
}

/// One formula of a fractal with its per-slot settings.
#[derive(Clone, Debug)]
pub struct Formula {
    pub slot: usize,
    pub function: Function,
    pub parameters: Parameters,
    pub meta: FormulaMeta,
    pub add_c: bool,
    /// Hybrid blend weight; 1 replaces z with the formula's output.
    pub weight: f64,
    pub check_bailout: bool,
    /// The pseudo-Kleinian / Jos-Kleinian "orbit stalled" exit.
    pub additional_bailout: bool,
}

/// A fractal ready to evaluate: one formula, or a hybrid sequence of them.
#[derive(Clone, Debug)]
pub struct Fractal {
    pub formulas: Vec<Formula>,
    /// The formula (index into `formulas`) each iteration runs; `None` is an
    /// empty slot, which ends the orbit as empty space.
    pub sequence: Vec<Option<usize>>,
    pub hybrid: bool,
    pub max_iterations: usize,
    pub min_iterations: usize,
    pub bailout: f64,
    pub constant_multiplier: [f64; 3],
    pub julia: Option<[f64; 3]>,
    pub initial_w: f64,
    pub delta: bool,
    /// The Delta-DE finaliser, and for hybrids the analytic one too.
    pub de_function: DeFunction,
    /// `linear_DE_offset`, used by hybrids' linear analytic DE.
    pub linear_de_offset: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrbitResult {
    pub z: [f64; 4],
    pub r: f64,
    pub de: f64,
    pub pseudo_kleinian_de: f64,
    pub dist: f64,
    pub iterations: usize,
    pub max_iterations_reached: bool,
    /// The sequence reached an empty slot: Mandelbulber returns distance 10.
    pub empty_slot: bool,
    /// The formula of the last iteration.
    pub last_formula: usize,
}

fn delta_function(file: &FractFile, preferred: DeFunction) -> DeFunction {
    // delta_DE_function: 0 preferred, 1 linear, 2 logarithmic, 3 pseudo-Kleinian, ...
    match file.number("delta_DE_function", 0.0) as i64 {
        1 => DeFunction::Linear,
        2 => DeFunction::Logarithmic,
        3 => DeFunction::PseudoKleinian,
        4 => DeFunction::JosKleinian,
        5 => DeFunction::Custom,
        6 => DeFunction::MaxAxis,
        _ => preferred,
    }
}

fn slot_number(file: &FractFile, key: &str, slot: usize, fallback: f64) -> f64 {
    file.number(&format!("{key}_{slot}"), fallback)
}

impl Fractal {
    /// A single-formula scene: slot 1 only.
    pub fn from_scene(
        file: &FractFile,
        function: Function,
        meta: FormulaMeta,
        parameters: Parameters,
    ) -> Fractal {
        let source = FormulaSource {
            slot: 1,
            function,
            meta,
            parameters,
        };
        let mut fractal = Fractal::from_slots(file, vec![source]).expect("one formula");
        fractal.hybrid = false;
        fractal
    }

    /// The scene's fractal from its active slots' formulas. A hybrid when
    /// `hybrid_fractal_enable` is set; otherwise only slot 1 is used.
    pub fn from_slots(file: &FractFile, sources: Vec<FormulaSource>) -> Result<Fractal, String> {
        let hybrid = file.boolean("hybrid_fractal_enable", false);
        let mut sources = sources;
        sources.sort_by_key(|s| s.slot);
        if !hybrid {
            sources.retain(|s| s.slot == 1);
        }
        if sources.is_empty() {
            return Err("no formula in slot 1".into());
        }
        let max_iterations = file.number("N", 250.0) as usize;
        let use_default_bailout = file.boolean("use_default_bailout", true);
        // Hybrids share the largest default bailout, empty slots' 1.0 included.
        let bailout = match (use_default_bailout, hybrid) {
            (false, _) => file.number("bailout", 100.0),
            (true, false) => sources[0].meta.default_bailout,
            (true, true) => sources
                .iter()
                .map(|s| s.meta.default_bailout)
                .fold(1.0, f64::max),
        };
        // delta_DE_method: 0 preferred, 1 force Delta-DE, 2 force analytic.
        let method = file.number("delta_DE_method", 0.0) as i64;
        let any_delta = sources.iter().any(|s| s.meta.de_type == DeType::Delta);
        let delta = match method {
            1 => true,
            2 => false,
            _ => any_delta,
        };
        let de_function = if !hybrid {
            delta_function(file, sources[0].meta.de_function)
        } else {
            // The DE function used by the most iterations; dIFS wins outright.
            let mut counts: HashMap<usize, f64> = HashMap::new();
            let order = [
                DeFunction::Linear,
                DeFunction::Logarithmic,
                DeFunction::PseudoKleinian,
                DeFunction::JosKleinian,
                DeFunction::Custom,
                DeFunction::MaxAxis,
            ];
            for source in &sources {
                if let Some(k) = order.iter().position(|f| *f == source.meta.de_function) {
                    *counts.entry(k).or_default() +=
                        slot_number(file, "formula_iterations", source.slot, 1.0);
                }
            }
            let mut best = (DeFunction::Linear, -1.0);
            for (k, function) in order.iter().enumerate() {
                let count = counts.get(&k).copied().unwrap_or(0.0);
                if count > best.1 {
                    best = (*function, count);
                }
            }
            let preferred = if counts.contains_key(&4) {
                DeFunction::Custom
            } else {
                best.0
            };
            delta_function(file, preferred)
        };
        let mut formulas = Vec::new();
        for source in sources {
            let slot = source.slot;
            let dont_add_c = file.boolean(&format!("dont_add_c_constant_{slot}"), false);
            let add_c = match source.meta.c_addition {
                CAddition::AlreadyHas => false,
                CAddition::EnabledByDefault => !dont_add_c,
                CAddition::DisabledByDefault => dont_add_c,
            };
            let kleinian = matches!(
                source.meta.de_function,
                DeFunction::PseudoKleinian | DeFunction::JosKleinian
            );
            formulas.push(Formula {
                slot,
                function: source.function,
                parameters: source.parameters,
                add_c,
                weight: if hybrid {
                    slot_number(file, "formula_weight", slot, 1.0)
                } else {
                    1.0
                },
                check_bailout: !hybrid || file.boolean(&format!("check_for_bailout_{slot}"), true),
                additional_bailout: kleinian,
                meta: source.meta,
            });
        }
        let sequence = if hybrid {
            hybrid_sequence(file, &formulas, max_iterations)
        } else {
            vec![Some(0); max_iterations]
        };
        Ok(Fractal {
            formulas,
            sequence,
            hybrid,
            max_iterations,
            min_iterations: file.number("minN", 1.0) as usize,
            bailout,
            constant_multiplier: file.vector("fractal_constant_factor", [1.0; 3]),
            julia: file
                .boolean("julia_mode", false)
                .then(|| file.vector("julia_c", [0.0; 3])),
            initial_w: file.number("initial_waxis", 0.0),
            delta,
            de_function,
            linear_de_offset: file.number("linear_DE_offset", 0.0),
        })
    }

    /// The first formula: the only one of a single-formula fractal.
    pub fn primary(&self) -> &Formula {
        &self.formulas[0]
    }

    fn initial_aux(&self, z: [f64; 4]) -> Aux {
        let scale = self
            .primary()
            .parameters
            .values
            .get("mandelbox.scale")
            .cloned()
            .unwrap_or(Value::Scalar(2.0));
        let length = (z[0] * z[0] + z[1] * z[1] + z[2] * z[2] + z[3] * z[3]).sqrt();
        let fields: HashMap<String, Value> = [
            ("i", Value::Int(0)),
            ("c", Value::vec4(z)),
            ("const_c", Value::vec4(z)),
            ("old_z", Value::vec4(z)),
            ("pos_neg", Value::Scalar(1.0)),
            ("r", Value::Scalar(length)),
            ("DE", Value::Scalar(1.0)),
            ("DE0", Value::Scalar(0.0)),
            ("dist", Value::Scalar(1000.0)),
            ("pseudoKleinianDE", Value::Scalar(1.0)),
            ("actualScale", scale),
            ("actualScaleA", Value::Scalar(0.0)),
            ("color", Value::Scalar(1.0)),
            ("colorHybrid", Value::Scalar(0.0)),
            ("temp1000", Value::Scalar(1000.0)),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        Aux { fields }
    }

    /// The orbit of `point`, as Mandelbulber's `Compute`: until escape, or
    /// for exactly `forced` iterations with no bailout (Delta-DE's probes,
    /// `calcModeDeltaDE2`).
    pub fn orbit(&self, point: [f64; 3], forced: Option<usize>) -> Result<OrbitResult, EvalError> {
        let mut z = [point[0], point[1], point[2], self.initial_w];
        let mut aux = self.initial_aux(z);
        let limit = forced.unwrap_or(self.max_iterations);
        let mut iterations = 0;
        let mut escaped = false;
        let mut last_formula = 0;
        let mut r = norm(z);
        let mut last_z = z;
        for i in 0..limit {
            iterations = i + 1;
            let last_last_z = last_z;
            last_z = z;
            let Some(index) = self.sequence.get(i).copied().unwrap_or(Some(0)) else {
                return Ok(OrbitResult {
                    z,
                    r,
                    de: 1.0,
                    pseudo_kleinian_de: 1.0,
                    dist: 1000.0,
                    iterations: 1,
                    max_iterations_reached: false,
                    empty_slot: true,
                    last_formula,
                });
            };
            last_formula = index;
            let formula = &self.formulas[index];
            let scalar = |aux: &Aux, name: &str| {
                aux.fields
                    .get(name)
                    .and_then(|v| v.as_scalar().ok())
                    .unwrap_or(0.0)
            };
            let (previous_de, previous_color) = (scalar(&aux, "DE"), scalar(&aux, "color"));
            aux.fields.insert("i".into(), Value::Int(i as i64));
            if !self.hybrid || formula.weight > 0.0 {
                let Value::Vector { v, .. } = call_formula(
                    &formula.function,
                    Value::vec4(z),
                    &formula.parameters,
                    &mut aux,
                )?
                else {
                    return Err(EvalError {
                        message: "formula returned a non-vector".into(),
                    });
                };
                z = v;
            }
            if formula.add_c {
                let m = self.constant_multiplier;
                let swapped = matches!(
                    formula.meta.internal_name.as_str(),
                    "abox_mod1" | "amazing_surf"
                );
                let add = match (self.julia, swapped) {
                    (Some(j), true) => [j[1] * m[1], j[0] * m[0], j[2] * m[2], 0.0],
                    (Some(j), false) => [j[0] * m[0], j[1] * m[1], j[2] * m[2], 0.0],
                    (None, _) => {
                        let Some(Value::Vector { v: c, .. }) = aux.fields.get("const_c") else {
                            return Err(EvalError {
                                message: "aux const_c missing".into(),
                            });
                        };
                        if swapped {
                            [c[1] * m[0], c[0] * m[1], c[2] * m[2], 0.0]
                        } else {
                            // CVector4 * CVector3 multiplies xyz and keeps w.
                            [c[0] * m[0], c[1] * m[1], c[2] * m[2], c[3]]
                        }
                    }
                };
                for k in 0..4 {
                    z[k] += add[k];
                }
            }
            if self.hybrid && formula.weight < 1.0 {
                let k = formula.weight;
                z = smooth_vector(last_z, z, k);
                let de = scalar(&aux, "DE") * k + previous_de * (1.0 - k);
                let color = scalar(&aux, "color") * k + previous_color * (1.0 - k);
                aux.fields.insert("DE".into(), Value::Scalar(de));
                aux.fields.insert("color".into(), Value::Scalar(color));
            }
            r = norm(z);
            aux.fields.insert("r".into(), Value::Scalar(r));
            if z.iter().any(|x| x.is_nan()) {
                z = last_z;
                r = norm(z);
                break;
            }
            if forced.is_none() && formula.check_bailout {
                if r > self.bailout {
                    escaped = true;
                    break;
                }
                if formula.additional_bailout {
                    escaped = true;
                    let stalled = |other: [f64; 4]| {
                        norm([
                            z[0] - other[0],
                            z[1] - other[1],
                            z[2] - other[2],
                            z[3] - other[3],
                        ]) / r
                            < 0.1 / self.bailout
                    };
                    if stalled(last_z) || stalled(last_last_z) {
                        break;
                    }
                }
            }
        }
        let field = |name: &str, fallback: f64| {
            aux.fields
                .get(name)
                .and_then(|v| v.as_scalar().ok())
                .unwrap_or(fallback)
        };
        Ok(OrbitResult {
            z,
            r,
            de: field("DE", 1.0),
            pseudo_kleinian_de: field("pseudoKleinianDE", 1.0),
            dist: field("dist", 1000.0),
            iterations,
            max_iterations_reached: !escaped && forced.is_none(),
            empty_slot: false,
            last_formula,
        })
    }

    /// Distance estimate at `point` for a ray step with `detail_size` (the
    /// march threshold), as `CalculateDistanceSimple` without interior or
    /// iteration-threshold modes.
    pub fn distance(&self, point: [f64; 3], detail_size: f64) -> Result<f64, EvalError> {
        let base = self.orbit(point, None)?;
        if base.empty_slot {
            return Ok(10.0);
        }
        let mut distance = if self.delta {
            let delta = (length3(point) * 1e-14).max(1e-5 * detail_size);
            let r = length3([base.z[0], base.z[1], base.z[2]]);
            let mut gradient = [0.0; 3];
            for axis in 0..3 {
                let mut offset = point;
                offset[axis] += delta;
                let z = self.orbit(offset, Some(base.iterations))?.z;
                gradient[axis] = (length3([z[0], z[1], z[2]]) - r).abs() / delta;
            }
            let dr = length3(gradient);
            if dr > 0.0 {
                match self.de_function {
                    DeFunction::Linear => 0.5 * r / dr,
                    DeFunction::Logarithmic => 0.5 * r * r.ln() / dr,
                    DeFunction::PseudoKleinian => {
                        let rxy = (base.z[0] * base.z[0] + base.z[1] * base.z[1]).sqrt();
                        (rxy - 0.92784).max((rxy * base.z[2]).abs() / r) / dr
                    }
                    DeFunction::JosKleinian => {
                        let rxy = (base.z[0] * base.z[0] + base.z[2] * base.z[2]).sqrt();
                        (rxy * base.z[1]).abs() / r / dr
                    }
                    DeFunction::MaxAxis => {
                        let max_z = base.z[..3].iter().fold(0.0_f64, |m, x| m.max(x.abs()));
                        0.5 * max_z / gradient.iter().fold(0.0_f64, |m, g| m.max(*g))
                    }
                    _ => r,
                }
            } else {
                r
            }
        } else if base.de > 0.0 {
            self.analytic_distance(&base)
        } else {
            base.r
        };
        if base.iterations < self.min_iterations && distance < detail_size {
            distance = detail_size;
        }
        Ok(distance.clamp(0.0, 10.0))
    }

    fn analytic_distance(&self, base: &OrbitResult) -> f64 {
        let (r, de, z) = (base.r, base.de, base.z);
        let rxy = (z[0] * z[0] + z[1] * z[1]).sqrt();
        let max_axis = z[..3].iter().fold(0.0_f64, |m, x| m.max(x.abs()));
        let log = if r > 1.0 { 0.5 * r * r.ln() / de } else { 0.0 };
        if self.hybrid {
            return match self.de_function {
                DeFunction::Linear => (r - self.linear_de_offset) / de,
                DeFunction::Logarithmic => log,
                DeFunction::PseudoKleinian => {
                    (rxy - base.pseudo_kleinian_de).max((rxy * z[2]).abs() / r) / de
                }
                DeFunction::JosKleinian => self.jos_kleinian(0, z, de),
                DeFunction::Custom => base.dist,
                DeFunction::MaxAxis => max_axis / de,
                _ => 0.0,
            };
        }
        match self.formulas[base.last_formula].meta.analytic_function {
            DeFunction::Logarithmic => log,
            DeFunction::Linear => r / de,
            DeFunction::Ifs => (r - 2.0) / de,
            DeFunction::PseudoKleinian => {
                (rxy - base.pseudo_kleinian_de).max((rxy * z[2]).abs() / r) / de
            }
            DeFunction::JosKleinian => self.jos_kleinian(base.last_formula, z, de),
            DeFunction::MaxAxis => max_axis / de,
            DeFunction::Custom => base.dist,
            DeFunction::None => -1.0,
        }
    }

    /// `min(z.y, tweak005) / max(DE, offset1)`, z.y folded when spheres are on.
    fn jos_kleinian(&self, formula: usize, z: [f64; 4], de: f64) -> f64 {
        let values = &self.formulas[formula].parameters.values;
        let scalar = |path: &str| {
            values
                .get(path)
                .and_then(|v| v.as_scalar().ok())
                .unwrap_or(0.0)
        };
        let mut y = z[1];
        if matches!(
            values.get("transformCommon.spheresEnabled"),
            Some(Value::Bool(true))
        ) {
            y = y.min(scalar("transformCommon.foldingValue") - y);
        }
        y.min(scalar("analyticDE.tweak005")) / de.max(scalar("analyticDE.offset1"))
    }
}

/// `cNineFractals::CreateSequence` for the first `length` iterations.
fn hybrid_sequence(file: &FractFile, formulas: &[Formula], length: usize) -> Vec<Option<usize>> {
    let repeat_from = (file.number("repeat_from", 1.0) as usize).clamp(1, SLOTS);
    let by_slot = |slot: usize| formulas.iter().position(|f| f.slot == slot + 1);
    let start = |slot: usize| slot_number(file, "formula_start_iteration", slot + 1, 0.0) as usize;
    let stop = |slot: usize| slot_number(file, "formula_stop_iteration", slot + 1, 250.0) as usize;
    let count = |slot: usize| slot_number(file, "formula_iterations", slot + 1, 1.0) as usize;
    let mut sequence = Vec::with_capacity(length);
    let mut slot = 0;
    let mut counter = 0;
    for i in 0..length {
        counter += 1;
        let mut searches = 0;
        while (by_slot(slot).is_none() || i < start(slot) || i > stop(slot)) && searches < SLOTS {
            slot += 1;
            if slot >= SLOTS {
                slot = repeat_from - 1;
            }
            searches += 1;
        }
        sequence.push(by_slot(slot));
        if counter >= count(slot) {
            counter = 0;
            slot += 1;
            if slot >= SLOTS {
                slot = repeat_from - 1;
            }
        }
    }
    sequence
}

/// Mandelbulber's `SmoothCVector`: blends direction and length separately.
fn smooth_vector(a: [f64; 4], b: [f64; 4], k: f64) -> [f64; 4] {
    if k <= 0.0 {
        return a;
    }
    if k >= 1.0 {
        return b;
    }
    let length = norm(a) * (1.0 - k) + norm(b) * k;
    let blend = [0, 1, 2, 3].map(|i| a[i] * (1.0 - k) + b[i] * k);
    let blend_length = norm(blend);
    if blend_length > 0.0 {
        blend.map(|x| x / blend_length * length)
    } else {
        a
    }
}

fn norm(v: [f64; 4]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

fn length3(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hybrid_sequence_repeats_counts_and_skips_out_of_range_slots() {
        let file = FractFile::parse(
            "[main_parameters]\nhybrid_fractal_enable true;\nformula_iterations_1 2;\nformula_start_iteration_3 4;\nrepeat_from 1;\n",
        );
        let formula = |slot| {
            Formula {
            slot,
            function: crate::parse_formula_file("REAL4 TIteration(REAL4 z, __constant sFractalCl *fractal, sExtendedAuxCl *aux) { return z; }")
                .unwrap()
                .functions
                .remove(0),
            parameters: Parameters::default(),
            meta: FormulaMeta {
                internal_name: "t".into(),
                de_type: DeType::Analytic,
                de_function: DeFunction::Linear,
                analytic_function: DeFunction::Linear,
                c_addition: CAddition::DisabledByDefault,
                default_bailout: 10.0,
            },
            add_c: false,
            weight: 1.0,
            check_bailout: true,
            additional_bailout: false,
        }
        };
        let formulas = [formula(1), formula(3)];
        // Slot 3 starts at iteration 4; until then the search wraps to slot 1.
        let sequence = hybrid_sequence(&file, &formulas, 8);
        assert_eq!(sequence, [0, 0, 0, 0, 1, 0, 0, 1].map(Some).to_vec());
    }

    #[test]
    fn smooth_vector_interpolates_length() {
        let v = smooth_vector([1.0, 0.0, 0.0, 0.0], [0.0, 3.0, 0.0, 0.0], 0.5);
        assert!((norm(v) - 2.0).abs() < 1e-12);
    }
}
