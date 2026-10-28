//! One-formula orbits and distance estimates in f64, mirroring Mandelbulber's
//! `Compute` (compute_fractal.cpp) and `CalculateDistanceSimple`
//! (calculate_distance.cpp) for non-hybrid scenes.

use std::collections::HashMap;

use crate::ast::Function;
use crate::eval::{Aux, EvalError, Parameters, Value, call_formula};
use crate::scene::{CAddition, DeFunction, DeType, FormulaMeta, FractFile};

/// A single-formula fractal ready to evaluate.
#[derive(Clone, Debug)]
pub struct Fractal {
    pub function: Function,
    pub parameters: Parameters,
    pub meta: FormulaMeta,
    pub max_iterations: usize,
    pub min_iterations: usize,
    pub bailout: f64,
    pub add_c: bool,
    pub constant_multiplier: [f64; 3],
    pub julia: Option<[f64; 3]>,
    pub initial_w: f64,
    pub delta: bool,
    pub de_function: DeFunction,
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
}

impl Fractal {
    /// Scene settings for slot 1 applied to a parsed formula.
    pub fn from_scene(
        file: &FractFile,
        function: Function,
        meta: FormulaMeta,
        parameters: Parameters,
    ) -> Fractal {
        let add_c = match meta.c_addition {
            CAddition::AlreadyHas => false,
            CAddition::EnabledByDefault => !file.boolean("dont_add_c_constant_1", false),
            CAddition::DisabledByDefault => file.boolean("dont_add_c_constant_1", false),
        };
        let bailout = if file.boolean("use_default_bailout", true) {
            meta.default_bailout
        } else {
            file.number("bailout", 100.0)
        };
        // delta_DE_method: 0 preferred, 1 force Delta-DE, 2 force analytic.
        let delta = match file.number("delta_DE_method", 0.0) as i64 {
            1 => true,
            2 => false,
            _ => meta.de_type == DeType::Delta,
        };
        // delta_DE_function: 0 preferred, 1 linear, 2 logarithmic, 3 pseudo-Kleinian, ...
        let de_function = match file.number("delta_DE_function", 0.0) as i64 {
            1 => DeFunction::Linear,
            2 => DeFunction::Logarithmic,
            3 => DeFunction::PseudoKleinian,
            4 => DeFunction::JosKleinian,
            _ => meta.de_function,
        };
        Fractal {
            function,
            parameters,
            max_iterations: file.number("N", 250.0) as usize,
            min_iterations: file.number("minN", 1.0) as usize,
            bailout,
            add_c,
            constant_multiplier: file.vector("fractal_constant_factor", [1.0; 3]),
            julia: file
                .boolean("julia_mode", false)
                .then(|| file.vector("julia_c", [0.0; 3])),
            initial_w: file.number("initial_waxis", 0.0),
            delta,
            de_function,
            meta,
        }
    }

    fn initial_aux(&self, z: [f64; 4]) -> Aux {
        let scale = self
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

    /// The orbit of `point`, for `forced` iterations (no bailout) or until
    /// escape, as Mandelbulber's `Compute`.
    pub fn orbit(&self, point: [f64; 3], forced: Option<usize>) -> Result<OrbitResult, EvalError> {
        let mut z = [point[0], point[1], point[2], self.initial_w];
        let mut aux = self.initial_aux(z);
        let limit = forced.unwrap_or(self.max_iterations);
        let mut iterations = 0;
        let mut escaped = false;
        let mut r = (z.iter().map(|x| x * x).sum::<f64>()).sqrt();
        for i in 0..limit {
            iterations = i + 1;
            let last_z = z;
            aux.fields.insert("i".into(), Value::Int(i as i64));
            let Value::Vector { v, .. } =
                call_formula(&self.function, Value::vec4(z), &self.parameters, &mut aux)?
            else {
                return Err(EvalError {
                    message: "formula returned a non-vector".into(),
                });
            };
            z = v;
            if self.add_c {
                let m = self.constant_multiplier;
                let add = match (self.julia, self.meta.internal_name.as_str()) {
                    (Some(j), "abox_mod1" | "amazing_surf") => {
                        [j[1] * m[1], j[0] * m[0], j[2] * m[2], 0.0]
                    }
                    (Some(j), _) => [j[0] * m[0], j[1] * m[1], j[2] * m[2], 0.0],
                    (None, name) => {
                        let Some(Value::Vector { v: c, .. }) = aux.fields.get("const_c") else {
                            return Err(EvalError {
                                message: "aux const_c missing".into(),
                            });
                        };
                        if matches!(name, "abox_mod1" | "amazing_surf") {
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
            r = (z.iter().map(|x| x * x).sum::<f64>()).sqrt();
            aux.fields.insert("r".into(), Value::Scalar(r));
            if z.iter().any(|x| x.is_nan()) {
                z = last_z;
                r = (z.iter().map(|x| x * x).sum::<f64>()).sqrt();
                break;
            }
            if r > self.bailout {
                escaped = true;
                break;
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
        })
    }

    /// Distance estimate at `point` for a ray step with `detail_size` (the
    /// march threshold), as `CalculateDistanceSimple` without interior or
    /// iteration-threshold modes.
    pub fn distance(&self, point: [f64; 3], detail_size: f64) -> Result<f64, EvalError> {
        let base = self.orbit(point, None)?;
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
                    _ => r,
                }
            } else {
                r
            }
        } else if base.de > 0.0 {
            let (r, de) = (base.r, base.de);
            match self.meta.analytic_function {
                DeFunction::Logarithmic => {
                    if r > 1.0 {
                        0.5 * r * r.ln() / de
                    } else {
                        0.0
                    }
                }
                DeFunction::Linear => r / de,
                DeFunction::Ifs => (r - 2.0) / de,
                DeFunction::PseudoKleinian => {
                    let rxy = (base.z[0] * base.z[0] + base.z[1] * base.z[1]).sqrt();
                    (rxy - base.pseudo_kleinian_de).max((rxy * base.z[2]).abs() / r) / de
                }
                DeFunction::MaxAxis => base.z[..3].iter().fold(0.0_f64, |m, x| m.max(x.abs())) / de,
                DeFunction::Custom => base.dist,
                DeFunction::None => -1.0,
                _ => r,
            }
        } else {
            base.r
        };
        if base.iterations < self.min_iterations && distance < detail_size {
            distance = detail_size;
        }
        Ok(distance.clamp(0.0, 10.0))
    }
}

fn length3(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}
