//! Boolean scenes (`boolean_operators`): each active slot is its own
//! single-formula fractal, placed by a per-slot transform and combined in
//! slot order, as Mandelbulber's `CalculateDistance`.

use crate::eval::EvalError;
use crate::orbit::{FormulaSource, Fractal};
use crate::params::rotation_matrix;
use crate::scene::FractFile;

/// `params::enumBooleanOperator`; the default is OR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operator {
    And,
    Or,
    Sub,
}

/// One slot of a Boolean scene.
#[derive(Clone, Debug)]
pub struct Component {
    /// The formula slot (1 to 9).
    pub slot: usize,
    pub fractal: Fractal,
    pub position: [f64; 3],
    /// `formula_rotation` as `SetRotation2` builds it (rows).
    pub rotation: [[f64; 3]; 3],
    pub repeat: [f64; 3],
    /// `1 / formula_scale`, as fractparams.cpp stores it.
    pub scale: f64,
    /// How this slot combines with the slots before it (unused for the first).
    pub operator: Operator,
    /// `smooth_de_combine_distance` when smooth union is enabled.
    pub smooth: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct BooleanFractal {
    pub components: Vec<Component>,
}

impl BooleanFractal {
    /// One component per active slot. Each reads its slot's own julia,
    /// constant factor, initial w and `formula_maxiter`, as cNineFractals
    /// does for Boolean scenes.
    pub fn from_slots(file: &FractFile, sources: Vec<FormulaSource>) -> Result<Self, String> {
        let mut sources = sources;
        sources.sort_by_key(|s| s.slot);
        let mut components = Vec::new();
        for source in sources {
            let slot = source.slot;
            let mut view = file.clone();
            let main = view.sections.entry("main_parameters".into()).or_default();
            main.insert("hybrid_fractal_enable".into(), "false".into());
            for (key, fallback) in [
                ("julia_mode", "false"),
                ("julia_c", "0 0 0"),
                ("fractal_constant_factor", "1 1 1"),
                ("initial_waxis", "0"),
            ] {
                let value = file.get("main_parameters", &format!("{key}_{slot}"));
                main.insert(key.into(), value.unwrap_or(fallback).to_owned());
            }
            let maxiter = file.number(&format!("formula_maxiter_{slot}"), 250.0);
            main.insert("N".into(), maxiter.to_string());
            let fractal = Fractal::from_slots(&view, vec![source])?;
            let operator = match file.number(&format!("boolean_operator_{}", slot - 1), 1.0) as i64
            {
                0 => Operator::And,
                2 => Operator::Sub,
                _ => Operator::Or,
            };
            components.push(Component {
                slot,
                fractal,
                position: file.vector(&format!("formula_position_{slot}"), [0.0; 3]),
                rotation: rotation_matrix(
                    2,
                    file.vector(&format!("formula_rotation_{slot}"), [0.0; 3]),
                ),
                repeat: file.vector(&format!("formula_repeat_{slot}"), [0.0; 3]),
                scale: 1.0 / file.number(&format!("formula_scale_{slot}"), 1.0),
                operator,
                smooth: file
                    .boolean(&format!("smooth_de_combine_enable_{slot}"), false)
                    .then(|| file.number(&format!("smooth_de_combine_distance_{slot}"), 0.1)),
            });
        }
        if components.is_empty() {
            return Err("Boolean scene with no formulas".into());
        }
        Ok(BooleanFractal { components })
    }

    /// The combined distance at `point`, ignoring displacement and noise.
    pub fn distance(&self, point: [f64; 3], detail_size: f64) -> Result<f64, EvalError> {
        let mut distance = 0.0;
        for (k, component) in self.components.iter().enumerate() {
            let local = component.local_point(point);
            let d = component.fractal.distance(local, detail_size)? / component.scale;
            distance = if k == 0 {
                d
            } else {
                combine(
                    component.operator,
                    component.smooth,
                    distance,
                    d,
                    detail_size,
                )
            };
        }
        Ok(distance)
    }
}

impl Component {
    /// `(p - position)`, rotated, repeated, then scaled.
    pub fn local_point(&self, point: [f64; 3]) -> [f64; 3] {
        let p = [0, 1, 2].map(|i| point[i] - self.position[i]);
        let m = self.rotation;
        let p = [0, 1, 2].map(|i| m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2]);
        repeat_mod(p, self.repeat).map(|x| x * self.scale)
    }
}

/// `CVector3::repeatMod`: wraps each axis with a positive period into
/// [-period/2, period/2).
pub(crate) fn repeat_mod(p: [f64; 3], period: [f64; 3]) -> [f64; 3] {
    if period.iter().all(|x| *x == 0.0) {
        return p;
    }
    let fmod = |x: f64, y: f64| if y > 0.0 { x % y } else { x };
    [0, 1, 2].map(|i| {
        fmod(
            fmod(p[i] - period[i] * 0.5, period[i]) + period[i],
            period[i],
        ) - period[i] * 0.5
    })
}

/// `opSmoothUnion`.
fn smooth_union(d1: f64, d2: f64, k: f64) -> f64 {
    let h = (0.5 + 0.5 * (d2 - d1) / k).clamp(0.0, 1.0);
    d2 * (1.0 - h) + d1 * h - k * h * (1.0 - h)
}

/// One step of the Boolean fold: `distance` so far, `d` of the next slot.
fn combine(
    operator: Operator,
    smooth: Option<f64>,
    distance: f64,
    d: f64,
    detail_size: f64,
) -> f64 {
    match operator {
        Operator::Or => match smooth {
            Some(k) => smooth_union(d, distance, k),
            None => d.min(distance),
        },
        Operator::And => d.max(distance),
        Operator::Sub => {
            // Ray-marching distances, not normal sampling.
            const LIMIT: f64 = 1.5;
            if distance < detail_size {
                if d < detail_size * LIMIT {
                    detail_size * LIMIT
                } else {
                    (detail_size * LIMIT - d).max(distance).max(0.0)
                }
            } else {
                distance
            }
        }
    }
}

/// WGSL helpers for [`crate::wgsl::emit_boolean`]; the same rules as above.
pub(crate) const WGSL_HELPERS: &str = r#"fn fractal_repeat_mod(p: vec3<f32>, period: vec3<f32>) -> vec3<f32> {
    if (all(period == vec3<f32>(0.0))) { return p; }
    let wrapped = select(p - period * 0.5, (p - period * 0.5) % period, period > vec3<f32>(0.0)) + period;
    return select(wrapped, wrapped % period, period > vec3<f32>(0.0)) - period * 0.5;
}

fn fractal_smooth_union(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (d2 - d1) / k, 0.0, 1.0);
    return mix(d2, d1, h) - k * h * (1.0 - h);
}

"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_wraps_into_centered_cells() {
        let p = repeat_mod([2.6, -2.6, 7.0], [5.0, 5.0, 0.0]);
        assert!((p[0] + 2.4).abs() < 1e-12, "{p:?}");
        assert!((p[1] - 2.4).abs() < 1e-12, "{p:?}");
        assert_eq!(p[2], 7.0);
        let q = repeat_mod([3.0, 0.0, 0.0], [5.0, 0.0, 0.0]);
        assert!((q[0] + 2.0).abs() < 1e-12, "{q:?}");
    }

    #[test]
    fn subtraction_carves_only_inside_the_first_shape() {
        assert_eq!(combine(Operator::Sub, None, 5.0, 0.0, 0.01), 5.0);
        assert_eq!(combine(Operator::Sub, None, 0.0, 0.0, 0.01), 0.015);
        assert_eq!(combine(Operator::Sub, None, 0.0, 1.0, 0.01), 0.0);
    }
}

/// A scene's fractal: one formula or hybrid, or a Boolean combination.
#[derive(Clone, Debug)]
pub enum Shape {
    Single(Fractal),
    Boolean(BooleanFractal),
}

impl Shape {
    /// The scene's shape from its active slots' formulas.
    pub fn from_slots(file: &FractFile, sources: Vec<FormulaSource>) -> Result<Shape, String> {
        if file.boolean("boolean_operators", false) {
            BooleanFractal::from_slots(file, sources).map(Shape::Boolean)
        } else {
            Fractal::from_slots(file, sources).map(Shape::Single)
        }
    }

    pub fn distance(&self, point: [f64; 3], detail_size: f64) -> Result<f64, EvalError> {
        match self {
            Shape::Single(fractal) => fractal.distance(point, detail_size),
            Shape::Boolean(scene) => scene.distance(point, detail_size),
        }
    }

    /// Every fractal in the shape.
    pub fn fractals(&self) -> Vec<&Fractal> {
        match self {
            Shape::Single(fractal) => vec![fractal],
            Shape::Boolean(scene) => scene.components.iter().map(|c| &c.fractal).collect(),
        }
    }
}
