//! f64 interpreter for parsed formula functions.
//!
//! Mandelbulber's CPU renderer iterates its formulas in double precision, so
//! interpreting the OpenCL source in f64 reproduces the native distance field
//! and serves as the reference for generated GPU code.

use std::collections::HashMap;
use std::fmt;

use crate::ast::*;

/// A runtime value: scalars, OpenCL vectors (`len` 2..=4), integers,
/// booleans, 3x3 matrices (rows) and local arrays.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Scalar(f64),
    Vector {
        len: u8,
        v: [f64; 4],
    },
    Int(i64),
    Bool(bool),
    Matrix([[f64; 3]; 3]),
    Array(Vec<Value>),
    /// Component `index` of a vector: a local variable, or a read-only
    /// parameter value. The only pointers the formula sources form.
    Pointer {
        target: PointerTarget,
        index: usize,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum PointerTarget {
    Local(String),
    Parameter(Box<Value>),
}

impl Value {
    pub fn vec4(v: [f64; 4]) -> Self {
        Value::Vector { len: 4, v }
    }

    pub fn vec3(v: [f64; 3]) -> Self {
        Value::Vector {
            len: 3,
            v: [v[0], v[1], v[2], 0.0],
        }
    }

    pub fn as_scalar(&self) -> Result<f64> {
        match self {
            Value::Scalar(x) => Ok(*x),
            Value::Int(i) => Ok(*i as f64),
            Value::Bool(b) => Ok(f64::from(u8::from(*b))),
            other => Err(EvalError::new(format!("expected a scalar, got {other:?}"))),
        }
    }

    fn as_int(&self) -> Result<i64> {
        match self {
            Value::Int(i) => Ok(*i),
            Value::Scalar(x) => Ok(*x as i64),
            Value::Bool(b) => Ok(i64::from(*b)),
            other => Err(EvalError::new(format!(
                "expected an integer, got {other:?}"
            ))),
        }
    }

    pub fn truthy(&self) -> Result<bool> {
        match self {
            Value::Bool(b) => Ok(*b),
            Value::Int(i) => Ok(*i != 0),
            Value::Scalar(x) => Ok(*x != 0.0),
            other => Err(EvalError::new(format!(
                "expected a condition, got {other:?}"
            ))),
        }
    }

    /// Components of a vector (or a scalar broadcast to `len`).
    fn components(&self, len: u8) -> Result<[f64; 4]> {
        match self {
            Value::Vector { v, .. } => Ok(*v),
            _ => {
                let x = self.as_scalar()?;
                let mut v = [0.0; 4];
                v[..usize::from(len)].fill(x);
                Ok(v)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EvalError {
    pub message: String,
}

impl EvalError {
    fn new(message: impl Into<String>) -> Self {
        EvalError {
            message: message.into(),
        }
    }
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for EvalError {}

type Result<T> = std::result::Result<T, EvalError>;

/// Formula parameters (`fractal->path`) and named integer constants (enum
/// values used in `switch` labels and comparisons).
#[derive(Clone, Debug, Default)]
pub struct Parameters {
    pub values: HashMap<String, Value>,
    pub constants: HashMap<String, i64>,
}

/// Mandelbulber's per-orbit auxiliary state (`sExtendedAuxCl`).
#[derive(Clone, Debug, Default)]
pub struct Aux {
    pub fields: HashMap<String, Value>,
}

enum Flow {
    Next,
    Break,
    Continue,
    Return(Value),
}

/// Runs `function(z, fractal, aux)` and returns the new `z`.
pub fn call_formula(
    function: &Function,
    z: Value,
    parameters: &Parameters,
    aux: &mut Aux,
) -> Result<Value> {
    let mut interpreter = Interpreter {
        parameters,
        aux,
        scopes: vec![HashMap::new()],
    };
    let z_name = function
        .params
        .first()
        .map(|p| p.name.clone())
        .ok_or_else(|| EvalError::new("formula has no z parameter"))?;
    interpreter.scopes[0].insert(z_name.clone(), z);
    for statement in &function.body {
        if let Flow::Return(value) = interpreter.statement(statement)? {
            return Ok(value);
        }
    }
    interpreter.lookup(&z_name)
}

struct Interpreter<'a> {
    parameters: &'a Parameters,
    aux: &'a mut Aux,
    scopes: Vec<HashMap<String, Value>>,
}

/// A writable location: a local variable or an aux field, optionally narrowed
/// by array index and vector swizzle.
struct Place {
    aux: bool,
    name: String,
    index: Option<usize>,
    swizzle: Option<Vec<usize>>,
}

fn swizzle_indices(field: &str) -> Option<Vec<usize>> {
    field
        .chars()
        .map(|c| match c {
            'x' | 'r' => Some(0),
            'y' | 'g' => Some(1),
            'z' | 'b' => Some(2),
            'w' | 'a' => Some(3),
            _ => None,
        })
        .collect()
}

fn swizzle(value: &Value, indices: &[usize]) -> Result<Value> {
    let Value::Vector { v, .. } = value else {
        return Err(EvalError::new(format!("swizzle of non-vector {value:?}")));
    };
    if indices.len() == 1 {
        return Ok(Value::Scalar(v[indices[0]]));
    }
    let mut out = [0.0; 4];
    for (slot, &index) in indices.iter().enumerate() {
        out[slot] = v[index];
    }
    Ok(Value::Vector {
        len: indices.len() as u8,
        v: out,
    })
}

fn vector_len(a: &Value, b: &Value) -> Option<u8> {
    match (a, b) {
        (Value::Vector { len, .. }, _) | (_, Value::Vector { len, .. }) => Some(*len),
        _ => None,
    }
}

fn arithmetic(op: BinaryOp, a: &Value, b: &Value) -> Result<Value> {
    use BinaryOp::*;
    if let Some(len) = vector_len(a, b) {
        let (x, y) = (a.components(len)?, b.components(len)?);
        if matches!(op, Eq | Ne | Lt | Le | Gt | Ge | And | Or) {
            return Err(EvalError::new(format!(
                "vector comparison {op:?} is not supported"
            )));
        }
        let mut out = [0.0; 4];
        for i in 0..usize::from(len) {
            out[i] = scalar_op(op, x[i], y[i])?;
        }
        return Ok(Value::Vector { len, v: out });
    }
    match (op, a, b) {
        (And, _, _) => Ok(Value::Bool(a.truthy()? && b.truthy()?)),
        (Or, _, _) => Ok(Value::Bool(a.truthy()? || b.truthy()?)),
        (Eq | Ne | Lt | Le | Gt | Ge, _, _) => {
            let (x, y) = (a.as_scalar()?, b.as_scalar()?);
            Ok(Value::Bool(match op {
                Eq => x == y,
                Ne => x != y,
                Lt => x < y,
                Le => x <= y,
                Gt => x > y,
                _ => x >= y,
            }))
        }
        (_, Value::Int(_) | Value::Bool(_), Value::Int(_) | Value::Bool(_)) => {
            let (x, y) = (a.as_int()?, b.as_int()?);
            Ok(Value::Int(match op {
                Add => x.wrapping_add(y),
                Sub => x.wrapping_sub(y),
                Mul => x.wrapping_mul(y),
                Div if y != 0 => x / y,
                Rem if y != 0 => x % y,
                Shl => x << y,
                Shr => x >> y,
                BitAnd => x & y,
                BitOr => x | y,
                BitXor => x ^ y,
                _ => return Err(EvalError::new("integer division by zero")),
            }))
        }
        _ => Ok(Value::Scalar(scalar_op(
            op,
            a.as_scalar()?,
            b.as_scalar()?,
        )?)),
    }
}

fn scalar_op(op: BinaryOp, x: f64, y: f64) -> Result<f64> {
    use BinaryOp::*;
    Ok(match op {
        Add => x + y,
        Sub => x - y,
        Mul => x * y,
        Div => x / y,
        Rem => x % y,
        other => return Err(EvalError::new(format!("operator {other:?} on floats"))),
    })
}

/// Applies `f` per component for vectors, or once for scalars.
fn map(value: &Value, f: impl Fn(f64) -> f64) -> Result<Value> {
    match value {
        Value::Vector { len, v } => {
            let mut out = *v;
            for x in out.iter_mut().take(usize::from(*len)) {
                *x = f(*x);
            }
            Ok(Value::Vector { len: *len, v: out })
        }
        _ => Ok(Value::Scalar(f(value.as_scalar()?))),
    }
}

fn map2(a: &Value, b: &Value, f: impl Fn(f64, f64) -> f64) -> Result<Value> {
    if let Some(len) = vector_len(a, b) {
        let (x, y) = (a.components(len)?, b.components(len)?);
        let mut out = [0.0; 4];
        for i in 0..usize::from(len) {
            out[i] = f(x[i], y[i]);
        }
        return Ok(Value::Vector { len, v: out });
    }
    Ok(Value::Scalar(f(a.as_scalar()?, b.as_scalar()?)))
}

fn dot(a: &Value, b: &Value) -> Result<f64> {
    let len = vector_len(a, b).ok_or_else(|| EvalError::new("dot of scalars"))?;
    let (x, y) = (a.components(len)?, b.components(len)?);
    Ok((0..usize::from(len)).map(|i| x[i] * y[i]).sum())
}

/// OpenCL `sign`: -1, 0 or 1 (and 0 for NaN).
fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

fn builtin(name: &str, args: &[Value]) -> Result<Value> {
    let arg = |i: usize| {
        args.get(i)
            .ok_or_else(|| EvalError::new(format!("{name}: missing argument {i}")))
    };
    let name = name.strip_prefix("native_").unwrap_or(name);
    Ok(match name {
        "sin" => map(arg(0)?, f64::sin)?,
        "cos" => map(arg(0)?, f64::cos)?,
        "tan" => map(arg(0)?, f64::tan)?,
        "asin" => map(arg(0)?, f64::asin)?,
        "acos" => map(arg(0)?, f64::acos)?,
        "atan" => map(arg(0)?, f64::atan)?,
        "sinh" => map(arg(0)?, f64::sinh)?,
        "cosh" => map(arg(0)?, f64::cosh)?,
        "tanh" => map(arg(0)?, f64::tanh)?,
        "exp" => map(arg(0)?, f64::exp)?,
        "exp2" => map(arg(0)?, f64::exp2)?,
        "log" => map(arg(0)?, f64::ln)?,
        "log2" => map(arg(0)?, f64::log2)?,
        "log10" => map(arg(0)?, f64::log10)?,
        "sqrt" => map(arg(0)?, f64::sqrt)?,
        "rsqrt" => map(arg(0)?, |x| 1.0 / x.sqrt())?,
        "recip" => map(arg(0)?, |x| 1.0 / x)?,
        "fabs" | "abs" => map(arg(0)?, f64::abs)?,
        "floor" => map(arg(0)?, f64::floor)?,
        "ceil" => map(arg(0)?, f64::ceil)?,
        "round" => map(arg(0)?, f64::round)?,
        "trunc" => map(arg(0)?, f64::trunc)?,
        "sign" => map(arg(0)?, sign)?,
        "atan2" => map2(arg(0)?, arg(1)?, f64::atan2)?,
        "pow" | "powr" => map2(arg(0)?, arg(1)?, f64::powf)?,
        "fmod" => map2(arg(0)?, arg(1)?, |x, y| x % y)?,
        // opencl_algebra.h: x - s wrapped into [0, a), shifted back by s.
        "wrap" => {
            let shifted = map2(arg(0)?, arg(2)?, |x, s| x - s)?;
            let wrapped = map2(&shifted, arg(1)?, |x, a| x - a * (x / a).floor())?;
            map2(&wrapped, arg(2)?, |x, s| x + s)?
        }
        "min" | "fmin" => map2(arg(0)?, arg(1)?, f64::min)?,
        "max" | "fmax" => map2(arg(0)?, arg(1)?, f64::max)?,
        "copysign" => map2(arg(0)?, arg(1)?, f64::copysign)?,
        "divide" => map2(arg(0)?, arg(1)?, |x, y| x / y)?,
        "clamp" => {
            let low = map2(arg(0)?, arg(1)?, f64::max)?;
            map2(&low, arg(2)?, f64::min)?
        }
        "mix" => {
            let t = arg(2)?;
            let delta = map2(arg(1)?, arg(0)?, |b, a| b - a)?;
            let scaled = map2(&delta, t, |d, t| d * t)?;
            map2(arg(0)?, &scaled, |a, s| a + s)?
        }
        "mad" => {
            let product = map2(arg(0)?, arg(1)?, |a, b| a * b)?;
            map2(&product, arg(2)?, |p, c| p + c)?
        }
        "dot" => Value::Scalar(dot(arg(0)?, arg(1)?)?),
        "length" => match arg(0)? {
            v @ Value::Vector { .. } => Value::Scalar(dot(v, v)?.sqrt()),
            other => Value::Scalar(other.as_scalar()?.abs()),
        },
        "distance" => {
            let delta = map2(arg(0)?, arg(1)?, |a, b| a - b)?;
            Value::Scalar(dot(&delta, &delta)?.sqrt())
        }
        "normalize" => {
            let v = arg(0)?;
            let length = dot(v, v)?.sqrt();
            map(v, |x| x / length)?
        }
        // opencl_algebra.h: a logistic step, 1 / (1 + exp(sharpness (b - a))).
        "SmoothConditionAGreaterB" => {
            let (a, b, k) = (
                arg(0)?.as_scalar()?,
                arg(1)?.as_scalar()?,
                arg(2)?.as_scalar()?,
            );
            Value::Scalar(1.0 / (1.0 + (k * (b - a)).exp()))
        }
        "SmoothConditionALessB" => {
            let (a, b, k) = (
                arg(0)?.as_scalar()?,
                arg(1)?.as_scalar()?,
                arg(2)?.as_scalar()?,
            );
            Value::Scalar(1.0 / (1.0 + (k * (a - b)).exp()))
        }
        // opencl_algebra.h: Rodrigues' rotation; the 4D form keeps w.
        "RotateAroundVectorByAngle" | "RotateAroundVectorByAngle4" => {
            let (o, k) = (arg(0)?.components(3)?, arg(1)?.components(3)?);
            let angle = arg(2)?.as_scalar()?;
            let (s, c) = angle.sin_cos();
            let along = k[0] * o[0] + k[1] * o[1] + k[2] * o[2];
            let cross = [
                k[1] * o[2] - k[2] * o[1],
                k[2] * o[0] - k[0] * o[2],
                k[0] * o[1] - k[1] * o[0],
            ];
            let r = [0, 1, 2].map(|i| o[i] * c + cross[i] * s + k[i] * along * (1.0 - c));
            if name.ends_with('4') {
                Value::vec4([r[0], r[1], r[2], o[3]])
            } else {
                Value::vec3(r)
            }
        }
        "cross" => {
            let (a, b) = (arg(0)?.components(3)?, arg(1)?.components(3)?);
            let len = if let Value::Vector { len, .. } = arg(0)? {
                *len
            } else {
                3
            };
            Value::Vector {
                len,
                v: [
                    a[1] * b[2] - a[2] * b[1],
                    a[2] * b[0] - a[0] * b[2],
                    a[0] * b[1] - a[1] * b[0],
                    0.0,
                ],
            }
        }
        "Matrix33MulFloat4" | "Matrix33MulFloat3" => {
            let Value::Matrix(m) = arg(0)? else {
                return Err(EvalError::new(format!(
                    "{name}: first argument is not a matrix"
                )));
            };
            let Value::Vector { len, v } = arg(1)? else {
                return Err(EvalError::new(format!(
                    "{name}: second argument is not a vector"
                )));
            };
            let row = |r: [f64; 3]| r[0] * v[0] + r[1] * v[1] + r[2] * v[2];
            Value::Vector {
                len: *len,
                v: [row(m[0]), row(m[1]), row(m[2]), v[3]],
            }
        }
        other => return Err(EvalError::new(format!("unsupported function {other}"))),
    })
}

impl Interpreter<'_> {
    fn lookup(&self, name: &str) -> Result<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(value) = scope.get(name) {
                return Ok(value.clone());
            }
        }
        if let Some(&constant) = self.parameters.constants.get(name) {
            return Ok(Value::Int(constant));
        }
        if let Some(value) = crate::constants::int_constant(name) {
            return Ok(Value::Int(value));
        }
        if let Some(value) = crate::constants::float_constant(name) {
            return Ok(Value::Scalar(value));
        }
        match name {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(EvalError::new(format!("unknown identifier {name}"))),
        }
    }

    fn declare(&mut self, name: &str, value: Value) {
        self.scopes
            .last_mut()
            .expect("scope")
            .insert(name.to_owned(), value);
    }

    fn store_local(&mut self, name: &str, value: Value) -> Result<()> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(slot) = scope.get_mut(name) {
                *slot = value;
                return Ok(());
            }
        }
        Err(EvalError::new(format!("assignment to undeclared {name}")))
    }

    fn block(&mut self, statements: &[Stmt]) -> Result<Flow> {
        self.scopes.push(HashMap::new());
        let mut flow = Flow::Next;
        for statement in statements {
            flow = self.statement(statement)?;
            if !matches!(flow, Flow::Next) {
                break;
            }
        }
        self.scopes.pop();
        Ok(flow)
    }

    fn statement(&mut self, statement: &Stmt) -> Result<Flow> {
        match statement {
            Stmt::Empty => Ok(Flow::Next),
            Stmt::Expr(Expr::Call { name, .. }) if name == "Q_UNUSED" => Ok(Flow::Next),
            // `(void)aux;` only silences an unused-parameter warning.
            Stmt::Expr(Expr::Cast { ty, .. }) if ty.name == "void" => Ok(Flow::Next),
            Stmt::Expr(expr) => {
                self.expr(expr)?;
                Ok(Flow::Next)
            }
            Stmt::Decl { ty, vars } => {
                for var in vars {
                    if var.pointer || ty.pointer {
                        let Some(init) = &var.init else {
                            return Err(EvalError::new(format!(
                                "uninitialised pointer {}",
                                var.name
                            )));
                        };
                        let value = self.expr(init)?;
                        if !matches!(value, Value::Pointer { .. }) {
                            return Err(EvalError::new(format!(
                                "pointer {} set to {value:?}",
                                var.name
                            )));
                        }
                        self.declare(&var.name, value);
                        continue;
                    }
                    let value = match (&var.init, &var.array) {
                        (Some(Expr::InitList(items)), _) => Value::Array(
                            items
                                .iter()
                                .map(|item| self.expr(item))
                                .collect::<Result<_>>()?,
                        ),
                        (Some(init), _) => convert(&ty.name, self.expr(init)?)?,
                        (None, Some(size)) => {
                            let size = self.expr(size)?.as_int()? as usize;
                            Value::Array(vec![zero(&ty.name); size])
                        }
                        (None, None) => zero(&ty.name),
                    };
                    self.declare(&var.name, value);
                }
                Ok(Flow::Next)
            }
            Stmt::Block(statements) => self.block(statements),
            Stmt::If {
                cond,
                then,
                otherwise,
            } => {
                if self.expr(cond)?.truthy()? {
                    self.scoped(then)
                } else if let Some(otherwise) = otherwise {
                    self.scoped(otherwise)
                } else {
                    Ok(Flow::Next)
                }
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
            } => {
                self.scopes.push(HashMap::new());
                if let Some(init) = init {
                    self.statement(init)?;
                }
                let flow = loop {
                    if let Some(cond) = cond {
                        if !self.expr(cond)?.truthy()? {
                            break Flow::Next;
                        }
                    }
                    match self.scoped(body)? {
                        Flow::Break => break Flow::Next,
                        flow @ Flow::Return(_) => break flow,
                        Flow::Next | Flow::Continue => {}
                    }
                    if let Some(step) = step {
                        self.expr(step)?;
                    }
                };
                self.scopes.pop();
                Ok(flow)
            }
            Stmt::While { cond, body } => {
                while self.expr(cond)?.truthy()? {
                    match self.scoped(body)? {
                        Flow::Break => break,
                        flow @ Flow::Return(_) => return Ok(flow),
                        Flow::Next | Flow::Continue => {}
                    }
                }
                Ok(Flow::Next)
            }
            Stmt::DoWhile { body, cond } => {
                loop {
                    match self.scoped(body)? {
                        Flow::Break => break,
                        flow @ Flow::Return(_) => return Ok(flow),
                        Flow::Next | Flow::Continue => {}
                    }
                    if !self.expr(cond)?.truthy()? {
                        break;
                    }
                }
                Ok(Flow::Next)
            }
            Stmt::Switch { value, cases } => {
                let value = self.expr(value)?.as_int()?;
                let mut start = None;
                for (index, case) in cases.iter().enumerate() {
                    for label in case.labels.iter().flatten() {
                        if self.expr(label)?.as_int()? == value {
                            start = Some(index);
                        }
                    }
                    if start.is_some() {
                        break;
                    }
                }
                let start =
                    start.or_else(|| cases.iter().position(|case| case.labels.contains(&None)));
                let Some(start) = start else {
                    return Ok(Flow::Next);
                };
                // C semantics: fall through until `break`.
                self.scopes.push(HashMap::new());
                let mut flow = Flow::Next;
                'cases: for case in &cases[start..] {
                    for statement in &case.body {
                        flow = self.statement(statement)?;
                        if !matches!(flow, Flow::Next) {
                            break 'cases;
                        }
                    }
                }
                self.scopes.pop();
                Ok(if matches!(flow, Flow::Break) {
                    Flow::Next
                } else {
                    flow
                })
            }
            Stmt::Break => Ok(Flow::Break),
            Stmt::Continue => Ok(Flow::Continue),
            Stmt::Return(value) => {
                let value = match value {
                    Some(expr) => self.expr(expr)?,
                    None => Value::Int(0),
                };
                Ok(Flow::Return(value))
            }
        }
    }

    fn scoped(&mut self, statement: &Stmt) -> Result<Flow> {
        match statement {
            Stmt::Block(statements) => self.block(statements),
            other => {
                self.scopes.push(HashMap::new());
                let flow = self.statement(other);
                self.scopes.pop();
                flow
            }
        }
    }

    /// `fractal->a.b[i].c` as the path `a.b[i].c`, with any trailing swizzle
    /// left for the caller when the path names a vector.
    fn parameter(&mut self, expr: &Expr) -> Result<Option<Value>> {
        let mut segments = Vec::new();
        let mut node = expr;
        loop {
            match node {
                Expr::Member { base, field, .. } => {
                    segments.push(field.clone());
                    node = base;
                }
                Expr::Index { base, index } => {
                    let index = self.expr(index)?.as_int()?;
                    segments.push(format!("[{index}]"));
                    node = base;
                }
                Expr::Ident(name) if name == "fractal" => break,
                _ => return Ok(None),
            }
        }
        segments.reverse();
        for keep in (1..=segments.len()).rev() {
            let path = join_path(&segments[..keep]);
            if let Some(value) = self.parameters.values.get(&path) {
                let mut value = value.clone();
                for rest in &segments[keep..] {
                    let indices = swizzle_indices(rest).ok_or_else(|| {
                        EvalError::new(format!(
                            "unknown parameter fractal->{}",
                            join_path(&segments)
                        ))
                    })?;
                    value = swizzle(&value, &indices)?;
                }
                return Ok(Some(value));
            }
        }
        Err(EvalError::new(format!(
            "unknown parameter fractal->{}",
            join_path(&segments)
        )))
    }

    /// `&local`, `&fractal->path` (component 0 of the vector) or `&p[k]` for
    /// a pointer `p`.
    fn address_of(&mut self, expr: &Expr) -> Result<Value> {
        match expr {
            Expr::Ident(name) => match self.lookup(name)? {
                Value::Pointer { .. } => Err(EvalError::new("pointer to pointer")),
                _ => Ok(Value::Pointer {
                    target: PointerTarget::Local(name.clone()),
                    index: 0,
                }),
            },
            Expr::Index { base, index } => {
                let offset = self.expr(index)?.as_int()? as usize;
                match self.expr(base)? {
                    Value::Pointer { target, index } => Ok(Value::Pointer {
                        target,
                        index: index + offset,
                    }),
                    other => Err(EvalError::new(format!("address of element of {other:?}"))),
                }
            }
            other => match self.parameter(other)? {
                Some(value) => Ok(Value::Pointer {
                    target: PointerTarget::Parameter(Box::new(value)),
                    index: 0,
                }),
                None => Err(EvalError::new(format!("address of {other:?}"))),
            },
        }
    }

    fn place(&mut self, expr: &Expr) -> Result<Place> {
        match expr {
            Expr::Ident(name) => Ok(Place {
                aux: false,
                name: name.clone(),
                index: None,
                swizzle: None,
            }),
            Expr::Member {
                base,
                field,
                arrow: true,
            } if matches!(&**base, Expr::Ident(b) if b == "aux") => Ok(Place {
                aux: true,
                name: field.clone(),
                index: None,
                swizzle: None,
            }),
            Expr::Member {
                base,
                field,
                arrow: false,
            } => {
                let mut place = self.place(base)?;
                if place.swizzle.is_some() {
                    return Err(EvalError::new("nested swizzle assignment"));
                }
                place.swizzle = Some(
                    swizzle_indices(field)
                        .ok_or_else(|| EvalError::new(format!("unknown member .{field}")))?,
                );
                Ok(place)
            }
            Expr::Index { base, index } => {
                let mut place = self.place(base)?;
                place.index = Some(self.expr(index)?.as_int()? as usize);
                Ok(place)
            }
            Expr::Unary {
                op: UnaryOp::Deref,
                expr: inner,
            } => match self.expr(inner)? {
                Value::Pointer {
                    target: PointerTarget::Local(name),
                    index,
                } => Ok(Place {
                    aux: false,
                    name,
                    index: None,
                    swizzle: Some(vec![index]),
                }),
                other => Err(EvalError::new(format!("cannot write through {other:?}"))),
            },
            other => Err(EvalError::new(format!("cannot assign to {other:?}"))),
        }
    }

    fn read_place(&self, place: &Place) -> Result<Value> {
        let mut value = if place.aux {
            self.aux
                .fields
                .get(&place.name)
                .cloned()
                .ok_or_else(|| EvalError::new(format!("unknown aux->{}", place.name)))?
        } else {
            self.lookup(&place.name)?
        };
        if let Some(index) = place.index {
            let Value::Array(items) = value else {
                return Err(EvalError::new("index into non-array"));
            };
            value = items
                .get(index)
                .cloned()
                .ok_or_else(|| EvalError::new("array index out of range"))?;
        }
        if let Some(indices) = &place.swizzle {
            value = swizzle(&value, indices)?;
        }
        Ok(value)
    }

    fn write_place(&mut self, place: &Place, value: Value) -> Result<()> {
        let mut whole = if place.aux {
            self.aux
                .fields
                .get(&place.name)
                .cloned()
                .unwrap_or(Value::Scalar(0.0))
        } else {
            self.lookup(&place.name)?
        };
        {
            let target: &mut Value = match (&mut whole, place.index) {
                (Value::Array(items), Some(index)) => items
                    .get_mut(index)
                    .ok_or_else(|| EvalError::new("array index out of range"))?,
                (other, None) => other,
                _ => return Err(EvalError::new("index into non-array")),
            };
            match &place.swizzle {
                None => {
                    *target = match target {
                        Value::Scalar(_) => Value::Scalar(value.as_scalar()?),
                        Value::Int(_) => Value::Int(value.as_int()?),
                        Value::Bool(_) => Value::Bool(value.truthy()?),
                        _ => value,
                    }
                }
                Some(indices) => {
                    let Value::Vector { v, .. } = target else {
                        return Err(EvalError::new("swizzle assignment to non-vector"));
                    };
                    let source = value.components(indices.len() as u8)?;
                    for (slot, &index) in indices.iter().enumerate() {
                        v[index] = source[slot];
                    }
                }
            }
        }
        if place.aux {
            self.aux.fields.insert(place.name.clone(), whole);
            Ok(())
        } else {
            self.store_local(&place.name, whole)
        }
    }

    fn expr(&mut self, expr: &Expr) -> Result<Value> {
        Ok(match expr {
            Expr::Number { value, float } => {
                if *float {
                    Value::Scalar(*value)
                } else {
                    Value::Int(*value as i64)
                }
            }
            Expr::Ident(name) => self.lookup(name)?,
            Expr::Member { base, field, arrow } => {
                if let Some(value) = self.parameter(expr)? {
                    return Ok(value);
                }
                if *arrow {
                    let place = self.place(expr)?;
                    return self.read_place(&place);
                }
                let value = self.expr(base)?;
                let indices = swizzle_indices(field)
                    .ok_or_else(|| EvalError::new(format!("unknown member .{field}")))?;
                swizzle(&value, &indices)?
            }
            Expr::Index { base, index } => {
                if let Some(value) = self.parameter(expr)? {
                    return Ok(value);
                }
                let base = self.expr(base)?;
                let index = self.expr(index)?.as_int()? as usize;
                match base {
                    Value::Array(items) => items
                        .get(index)
                        .cloned()
                        .ok_or_else(|| EvalError::new("index out of range"))?,
                    Value::Vector { v, .. } => Value::Scalar(v[index]),
                    other => return Err(EvalError::new(format!("index into {other:?}"))),
                }
            }
            Expr::Unary { op, expr: inner } => match op {
                UnaryOp::Neg => match self.expr(inner)? {
                    Value::Int(i) => Value::Int(-i),
                    other => map(&other, |x| -x)?,
                },
                UnaryOp::Plus => self.expr(inner)?,
                UnaryOp::Not => Value::Bool(!self.expr(inner)?.truthy()?),
                UnaryOp::BitNot => Value::Int(!self.expr(inner)?.as_int()?),
                UnaryOp::PreIncrement
                | UnaryOp::PreDecrement
                | UnaryOp::PostIncrement
                | UnaryOp::PostDecrement => {
                    let place = self.place(inner)?;
                    let old = self.read_place(&place)?;
                    let delta = if matches!(op, UnaryOp::PreIncrement | UnaryOp::PostIncrement) {
                        1
                    } else {
                        -1
                    };
                    let new = arithmetic(BinaryOp::Add, &old, &Value::Int(delta))?;
                    self.write_place(&place, new.clone())?;
                    if matches!(op, UnaryOp::PreIncrement | UnaryOp::PreDecrement) {
                        new
                    } else {
                        old
                    }
                }
                UnaryOp::AddressOf => self.address_of(inner)?,
                UnaryOp::Deref => match self.expr(inner)? {
                    Value::Pointer { target, index } => {
                        let vector = match target {
                            PointerTarget::Local(name) => self.lookup(&name)?,
                            PointerTarget::Parameter(value) => *value,
                        };
                        swizzle(&vector, &[index])?
                    }
                    other => return Err(EvalError::new(format!("dereference of {other:?}"))),
                },
            },
            Expr::Binary {
                op: BinaryOp::And,
                lhs,
                rhs,
            } => Value::Bool(self.expr(lhs)?.truthy()? && self.expr(rhs)?.truthy()?),
            Expr::Binary {
                op: BinaryOp::Or,
                lhs,
                rhs,
            } => Value::Bool(self.expr(lhs)?.truthy()? || self.expr(rhs)?.truthy()?),
            Expr::Binary { op, lhs, rhs } => {
                let (a, b) = (self.expr(lhs)?, self.expr(rhs)?);
                arithmetic(*op, &a, &b)?
            }
            Expr::Assign { op, target, value } => {
                let value = self.expr(value)?;
                let place = self.place(target)?;
                let value = match op {
                    Some(op) => arithmetic(*op, &self.read_place(&place)?, &value)?,
                    None => value,
                };
                self.write_place(&place, value.clone())?;
                value
            }
            Expr::Ternary {
                cond,
                then,
                otherwise,
            } => {
                if self.expr(cond)?.truthy()? {
                    self.expr(then)?
                } else {
                    self.expr(otherwise)?
                }
            }
            Expr::Call { name, args } => {
                if name == "__comma" {
                    self.expr(&args[0])?;
                    return self.expr(&args[1]);
                }
                let args = args
                    .iter()
                    .map(|arg| self.expr(arg))
                    .collect::<Result<Vec<_>>>()?;
                builtin(name, &args)?
            }
            Expr::Cast { ty, expr: inner } => {
                if ty.pointer {
                    // `(REAL *)&v` only reinterprets a vector as its components.
                    return self.expr(inner);
                }
                convert(&ty.name, self.expr(inner)?)?
            }
            Expr::VectorLiteral { ty, elements } => {
                let len = vector_width(&ty.name)
                    .ok_or_else(|| EvalError::new(format!("vector literal of {}", ty.name)))?;
                let mut v = [0.0; 4];
                let mut slot = 0;
                for element in elements {
                    match self.expr(element)? {
                        Value::Vector { len: n, v: parts } => {
                            for part in parts.iter().take(usize::from(n)) {
                                if slot < 4 {
                                    v[slot] = *part;
                                }
                                slot += 1;
                            }
                        }
                        scalar => {
                            if slot < 4 {
                                v[slot] = scalar.as_scalar()?;
                            }
                            slot += 1;
                        }
                    }
                }
                if elements.len() == 1 && slot == 1 {
                    v = [v[0]; 4];
                }
                Value::Vector { len, v }
            }
            Expr::InitList(_) => {
                return Err(EvalError::new("initialiser list outside a declaration"));
            }
        })
    }
}

fn join_path(segments: &[String]) -> String {
    let mut path = String::new();
    for segment in segments {
        if !path.is_empty() && !segment.starts_with('[') {
            path.push('.');
        }
        path.push_str(segment);
    }
    path
}

fn vector_width(type_name: &str) -> Option<u8> {
    match type_name
        .trim_start_matches("REAL")
        .trim_start_matches("float")
        .trim_start_matches("double")
    {
        "2" => Some(2),
        "3" => Some(3),
        "4" => Some(4),
        _ => None,
    }
}

fn zero(type_name: &str) -> Value {
    match type_name {
        "int" | "uint" | "unsigned int" | "long" | "short" | "char" => Value::Int(0),
        "bool" => Value::Bool(false),
        "matrix33" => Value::Matrix([[0.0; 3]; 3]),
        name => match vector_width(name) {
            Some(len) => Value::Vector { len, v: [0.0; 4] },
            None => Value::Scalar(0.0),
        },
    }
}

/// C conversion on declaration or cast to `type_name`.
fn convert(type_name: &str, value: Value) -> Result<Value> {
    Ok(match type_name {
        "int" | "uint" | "unsigned int" | "long" | "short" | "char" => Value::Int(value.as_int()?),
        "bool" => Value::Bool(value.truthy()?),
        "REAL" | "float" | "double" => Value::Scalar(value.as_scalar()?),
        name => match (vector_width(name), &value) {
            (Some(len), Value::Vector { v, .. }) => Value::Vector { len, v: *v },
            (Some(len), _) => Value::Vector {
                len,
                v: value.components(len)?,
            },
            _ => value,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_formula_file;

    fn parameters(pairs: &[(&str, Value)]) -> Parameters {
        Parameters {
            values: pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
            constants: HashMap::new(),
        }
    }

    #[test]
    fn mandelbulb_step_matches_hand_computation() {
        let source = r#"
REAL4 MandelbulbIteration(REAL4 z, __constant sFractalCl *fractal, sExtendedAuxCl *aux)
{
	REAL th0 = asin(z.z / aux->r) + fractal->bulb.betaAngleOffset;
	REAL ph0 = atan2(z.y, z.x) + fractal->bulb.alphaAngleOffset;
	REAL rp = pow(aux->r, fractal->bulb.power - 1.0f);
	REAL th = th0 * fractal->bulb.power;
	REAL ph = ph0 * fractal->bulb.power;
	REAL cth = native_cos(th);
	aux->DE = (rp * aux->DE) * fractal->bulb.power + 1.0f;
	rp *= aux->r;
	z.x = cth * native_cos(ph) * rp;
	z.y = cth * native_sin(ph) * rp;
	z.z = native_sin(th) * rp;
	return z;
}"#;
        let function = &parse_formula_file(source).unwrap().functions[0];
        let params = parameters(&[
            ("bulb.power", Value::Scalar(8.0)),
            ("bulb.alphaAngleOffset", Value::Scalar(0.0)),
            ("bulb.betaAngleOffset", Value::Scalar(0.0)),
        ]);
        let z: [f64; 3] = [0.3, -0.4, 0.5];
        let r = (z[0] * z[0] + z[1] * z[1] + z[2] * z[2]).sqrt();
        let mut aux = Aux::default();
        aux.fields.insert("r".into(), Value::Scalar(r));
        aux.fields.insert("DE".into(), Value::Scalar(1.0));
        let out = call_formula(
            function,
            Value::vec4([z[0], z[1], z[2], 0.0]),
            &params,
            &mut aux,
        )
        .unwrap();
        let (th, ph) = ((z[2] / r).asin() * 8.0, z[1].atan2(z[0]) * 8.0);
        let rp = r.powi(8);
        let expected = [
            th.cos() * ph.cos() * rp,
            th.cos() * ph.sin() * rp,
            th.sin() * rp,
        ];
        let Value::Vector { v, .. } = out else {
            panic!()
        };
        for i in 0..3 {
            assert!(
                (v[i] - expected[i]).abs() < 1e-12 * rp,
                "{i}: {} vs {}",
                v[i],
                expected[i]
            );
        }
        assert_eq!(aux.fields["DE"], Value::Scalar(r.powi(7) * 8.0 + 1.0));
    }

    #[test]
    fn switch_falls_through_and_arrays_index() {
        let source = "REAL4 F(REAL4 z) { bool on[2] = {false, true}; int n = 0; switch (n) { case 0: z.x = 1.0f; case 1: z.y = 2.0f; break; default: z.z = 3.0f; } if (on[1]) z.w = 4.0f; return z; }";
        let function = &parse_formula_file(source).unwrap().functions[0];
        let out = call_formula(
            function,
            Value::vec4([0.0; 4]),
            &Parameters::default(),
            &mut Aux::default(),
        )
        .unwrap();
        assert_eq!(out, Value::vec4([1.0, 2.0, 0.0, 4.0]));
    }
}
