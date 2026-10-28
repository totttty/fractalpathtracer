//! WGSL generation for single-formula fractals.
//!
//! The generated module defines `fractal_distance(p, detail_size)` and reads
//! numeric parameters from `fractal_params: array<vec4<f32>>`, which the host
//! declares and binds (`FractalShader::parameters` fills it). Integer and
//! boolean parameters change which code runs, so they are baked into the
//! source as constants; scenes that differ only in numbers share a shader.

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::ast::*;
use crate::eval::{Parameters, Value};
use crate::orbit::Fractal;
use crate::scene::DeFunction;

/// Generated WGSL plus the values for its parameter buffer.
#[derive(Clone, Debug, PartialEq)]
pub struct FractalShader {
    pub source: String,
    pub parameters: Vec<[f32; 4]>,
}

/// Declaration a host can prepend; group and binding are the host's choice.
pub const PARAMS_DECLARATION: &str = "var<storage, read> fractal_params: array<vec4<f32>>;";

#[derive(Clone, Debug, PartialEq)]
pub struct EmitError(pub String);

impl std::fmt::Display for EmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for EmitError {}

type Result<T> = std::result::Result<T, EmitError>;

fn err<T>(message: impl Into<String>) -> Result<T> {
    Err(EmitError(message.into()))
}

#[derive(Clone, Debug, PartialEq)]
enum Ty {
    F32,
    I32,
    Bool,
    Vec(u8),
    Mat3,
    Array(Box<Ty>, usize),
    /// A `REAL *` into the components of the vector `.0` (WGSL code): held
    /// as the i32 component index.
    Ptr(String),
}

impl Ty {
    fn wgsl(&self) -> String {
        match self {
            Ty::F32 => "f32".into(),
            Ty::I32 => "i32".into(),
            Ty::Bool => "bool".into(),
            Ty::Vec(n) => format!("vec{n}<f32>"),
            Ty::Mat3 => "mat3x3<f32>".into(),
            Ty::Array(element, n) => format!("array<{}, {n}>", element.wgsl()),
            Ty::Ptr(_) => "i32".into(),
        }
    }

    fn is_scalar(&self) -> bool {
        matches!(self, Ty::F32 | Ty::I32 | Ty::Bool)
    }
}

fn declared_type(name: &str) -> Result<Ty> {
    Ok(match name {
        "REAL" | "float" | "double" => Ty::F32,
        "int" | "uint" | "unsigned int" | "long" | "short" | "char" => Ty::I32,
        "bool" => Ty::Bool,
        "matrix33" => Ty::Mat3,
        enumeration if enumeration.starts_with("enum") => Ty::I32,
        other => match other
            .trim_start_matches("REAL")
            .trim_start_matches("float")
            .trim_start_matches("double")
        {
            "2" => Ty::Vec(2),
            "3" => Ty::Vec(3),
            "4" => Ty::Vec(4),
            _ => return err(format!("unsupported type {other}")),
        },
    })
}

const AUX_FIELDS: [(&str, Ty); 15] = [
    ("i", Ty::I32),
    ("c", Ty::Vec(4)),
    ("const_c", Ty::Vec(4)),
    ("old_z", Ty::Vec(4)),
    ("pos_neg", Ty::F32),
    ("r", Ty::F32),
    ("DE", Ty::F32),
    ("DE0", Ty::F32),
    ("dist", Ty::F32),
    ("pseudoKleinianDE", Ty::F32),
    ("actualScale", Ty::F32),
    ("actualScaleA", Ty::F32),
    ("color", Ty::F32),
    ("colorHybrid", Ty::F32),
    ("temp1000", Ty::F32),
];

/// Where a `fractal->path` value lives in the generated code.
#[derive(Clone, Debug)]
enum Slot {
    Baked(String, Ty),
    Buffer(usize, Ty),
}

/// A parameter read: a fixed path, or an array family (`rot[][]`) with
/// runtime indices.
enum ParameterRef {
    Path(String),
    Indexed {
        template: String,
        indices: Vec<String>,
    },
}

/// Every element of an array parameter family, laid out row-major from
/// `first`, `stride` slots apart, or for baked booleans and integers in the
/// module-scope array `table`.
#[derive(Clone, Debug)]
struct Family {
    first: usize,
    dims: Vec<usize>,
    stride: usize,
    ty: Ty,
    table: Option<String>,
}

struct Emitter<'a> {
    fractal: &'a Fractal,
    parameters: Vec<[f32; 4]>,
    slots: HashMap<String, Slot>,
    families: HashMap<String, Family>,
    /// The formula being emitted (index into `Fractal::formulas`).
    current: usize,
    scopes: Vec<HashMap<String, Ty>>,
    /// Module-scope declarations, placed after the prelude.
    globals: String,
    out: String,
    indent: usize,
    temporaries: usize,
}

/// Header slots at the start of the parameter buffer.
const HEADER_SLOTS: usize = 2;

pub fn emit(fractal: &Fractal) -> Result<FractalShader> {
    let mut emitter = Emitter {
        fractal,
        parameters: vec![[0.0; 4]; HEADER_SLOTS],
        slots: HashMap::new(),
        families: HashMap::new(),
        current: 0,
        scopes: vec![HashMap::new()],
        globals: String::new(),
        out: String::new(),
        indent: 0,
        temporaries: 0,
    };
    let julia = fractal.julia.unwrap_or([0.0; 3]);
    emitter.parameters[0] = [
        fractal.max_iterations as f32,
        fractal.bailout as f32,
        fractal.initial_w as f32,
        fractal.min_iterations as f32,
    ];
    let m = fractal.constant_multiplier;
    emitter.parameters[1] = [m[0] as f32, m[1] as f32, m[2] as f32, 0.0];
    let julia_slot = emitter.push_slot([julia[0] as f32, julia[1] as f32, julia[2] as f32, 0.0]);
    emitter.prelude();
    let globals_at = emitter.out.len();
    emitter.formulas()?;
    emitter.distance(julia_slot)?;
    let globals = std::mem::take(&mut emitter.globals);
    emitter.out.insert_str(globals_at, &globals);
    Ok(FractalShader {
        source: emitter.out,
        parameters: emitter.parameters,
    })
}

impl Emitter<'_> {
    fn params(&self) -> &Parameters {
        &self.fractal.formulas[self.current].parameters
    }

    fn push_slot(&mut self, value: [f32; 4]) -> usize {
        self.parameters.push(value);
        self.parameters.len() - 1
    }

    fn line(&mut self, text: impl AsRef<str>) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
        self.out.push_str(text.as_ref());
        self.out.push('\n');
    }

    fn prelude(&mut self) {
        self.out
            .push_str("// Generated by fpt-fractal from Mandelbulber's formula sources.\n");
        self.out.push_str("struct FractalAux {\n");
        for (name, ty) in &AUX_FIELDS {
            let _ = writeln!(self.out, "    {name}: {},", ty.wgsl());
        }
        self.out.push_str("}\n\n");
    }

    fn slot(&mut self, path: &str) -> Result<Slot> {
        let key = format!("{}:{path}", self.current);
        if let Some(slot) = self.slots.get(&key) {
            return Ok(slot.clone());
        }
        let Some(value) = self.params().values.get(path).cloned() else {
            return err(format!("unknown parameter fractal->{path}"));
        };
        let slot = match value {
            Value::Int(i) => Slot::Baked(i.to_string(), Ty::I32),
            Value::Bool(b) => Slot::Baked(b.to_string(), Ty::Bool),
            Value::Scalar(x) => Slot::Buffer(self.push_slot([x as f32, 0.0, 0.0, 0.0]), Ty::F32),
            Value::Vector { len, v } => {
                Slot::Buffer(self.push_slot(v.map(|x| x as f32)), Ty::Vec(len))
            }
            Value::Matrix(rows) => {
                let first =
                    self.push_slot([rows[0][0] as f32, rows[0][1] as f32, rows[0][2] as f32, 0.0]);
                self.push_slot([rows[1][0] as f32, rows[1][1] as f32, rows[1][2] as f32, 0.0]);
                self.push_slot([rows[2][0] as f32, rows[2][1] as f32, rows[2][2] as f32, 0.0]);
                Slot::Buffer(first, Ty::Mat3)
            }
            Value::Array(_) | Value::Pointer { .. } => {
                return err(format!("parameter fractal->{path} is not a value"));
            }
        };
        self.slots.insert(key, slot.clone());
        Ok(slot)
    }

    /// Lays out every `template` element (`mandelbox.rot[][]`) contiguously,
    /// so runtime indices can address them.
    fn family(&mut self, template: &str) -> Result<Family> {
        let key = format!("{}:{template}", self.current);
        if let Some(family) = self.families.get(&key) {
            return Ok(family.clone());
        }
        let mut elements = Vec::new();
        for (key, value) in &self.fractal.formulas[self.current].parameters.values {
            let (shape, indices) = index_template(key);
            if shape == template {
                elements.push((indices, value.clone()));
            }
        }
        let Some((first_indices, _)) = elements.first() else {
            return err(format!("no parameter array fractal->{template}"));
        };
        let mut dims = vec![0; first_indices.len()];
        for (indices, _) in &elements {
            for (dim, index) in dims.iter_mut().zip(indices) {
                *dim = (*dim).max(index + 1);
            }
        }
        if elements.len() != dims.iter().product::<usize>() {
            return err(format!("parameter array fractal->{template} has gaps"));
        }
        elements.sort_by(|a, b| a.0.cmp(&b.0));
        if let Some((_, Value::Bool(_) | Value::Int(_))) = elements.first() {
            let mut items = Vec::new();
            let mut ty = Ty::Bool;
            for (_, value) in &elements {
                match value {
                    Value::Bool(b) => items.push(b.to_string()),
                    Value::Int(i) => {
                        ty = Ty::I32;
                        items.push(i.to_string());
                    }
                    other => {
                        return err(format!(
                            "mixed element types in fractal->{template}: {other:?}"
                        ));
                    }
                }
            }
            if ty == Ty::I32 && elements.iter().any(|(_, v)| matches!(v, Value::Bool(_))) {
                return err(format!("mixed element types in fractal->{template}"));
            }
            let table = format!("fractal_table{}", self.families.len());
            let array = Ty::Array(Box::new(ty.clone()), items.len()).wgsl();
            let _ = writeln!(
                self.globals,
                "// fractal->{template}\nvar<private> {table}: {array} = {array}({});\n",
                items.join(", ")
            );
            let family = Family {
                first: 0,
                dims,
                stride: 1,
                ty,
                table: Some(table),
            };
            self.families.insert(key, family.clone());
            return Ok(family);
        }
        let first = self.parameters.len();
        let mut ty = None;
        for (_, value) in &elements {
            let element_ty = match value {
                Value::Scalar(x) => {
                    self.push_slot([*x as f32, 0.0, 0.0, 0.0]);
                    Ty::F32
                }
                Value::Vector { len, v } => {
                    self.push_slot(v.map(|x| x as f32));
                    Ty::Vec(*len)
                }
                Value::Matrix(rows) => {
                    for row in rows {
                        self.push_slot([row[0] as f32, row[1] as f32, row[2] as f32, 0.0]);
                    }
                    Ty::Mat3
                }
                other => {
                    return err(format!(
                        "runtime index into fractal->{template} of {other:?}"
                    ));
                }
            };
            if ty.get_or_insert(element_ty.clone()) != &element_ty {
                return err(format!("mixed element types in fractal->{template}"));
            }
        }
        let ty = ty.expect("elements");
        let family = Family {
            first,
            dims,
            stride: if ty == Ty::Mat3 { 3 } else { 1 },
            ty,
            table: None,
        };
        self.families.insert(key, family.clone());
        Ok(family)
    }

    fn slot_code(slot: &Slot) -> (String, Ty) {
        match slot {
            Slot::Baked(code, ty) => (code.clone(), ty.clone()),
            Slot::Buffer(k, ty) => (Self::buffer_code(&k.to_string(), ty), ty.clone()),
        }
    }

    /// Reads a `ty` value starting at buffer slot `k` (WGSL code).
    fn buffer_code(k: &str, ty: &Ty) -> String {
        match ty {
            Ty::F32 => format!("fractal_params[{k}].x"),
            Ty::Vec(2) => format!("fractal_params[{k}].xy"),
            Ty::Vec(3) => format!("fractal_params[{k}].xyz"),
            Ty::Mat3 => format!(
                "mat3x3<f32>(fractal_params[{k}].xyz, fractal_params[{k} + 1].xyz, fractal_params[{k} + 2].xyz)"
            ),
            _ => format!("fractal_params[{k}]"),
        }
    }

    fn formulas(&mut self) -> Result<()> {
        for k in 0..self.fractal.formulas.len() {
            self.current = k;
            self.formula(k)?;
        }
        Ok(())
    }

    fn formula(&mut self, k: usize) -> Result<()> {
        let fractal = self.fractal;
        let function = &fractal.formulas[k].function;
        let z = function
            .params
            .first()
            .map(|p| p.name.clone())
            .ok_or(EmitError("formula has no z".into()))?;
        self.line(format!("// {}", fractal.formulas[k].meta.internal_name));
        self.line(format!(
            "fn fractal_formula_{k}(z_in: vec4<f32>, aux: ptr<function, FractalAux>) -> vec4<f32> {{"
        ));
        self.indent += 1;
        self.line(format!("var {z} = z_in;"));
        self.scopes.push(HashMap::from([(z.clone(), Ty::Vec(4))]));
        let body = function.body.clone();
        for statement in &body {
            self.statement(statement)?;
        }
        if !matches!(body.last(), Some(Stmt::Return(_))) {
            self.line(format!("return {z};"));
        }
        self.scopes.pop();
        self.indent -= 1;
        self.line("}");
        self.out.push('\n');
        Ok(())
    }

    fn lookup(&self, name: &str) -> Option<Ty> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

    fn declare(&mut self, name: &str, ty: Ty) {
        self.scopes
            .last_mut()
            .expect("scope")
            .insert(name.to_owned(), ty);
    }

    fn block(&mut self, statements: &[Stmt]) -> Result<()> {
        self.scopes.push(HashMap::new());
        for statement in statements {
            self.statement(statement)?;
        }
        self.scopes.pop();
        Ok(())
    }

    fn body(&mut self, statement: &Stmt) -> Result<()> {
        match statement {
            Stmt::Block(statements) => self.block(statements),
            other => self.block(std::slice::from_ref(other)),
        }
    }

    fn statement(&mut self, statement: &Stmt) -> Result<()> {
        match statement {
            Stmt::Empty => {}
            Stmt::Expr(expr) => self.expression_statement(expr)?,
            Stmt::Decl { ty, vars } => {
                for var in vars {
                    if ty.pointer || var.pointer {
                        let Some(init) = &var.init else {
                            return err(format!("uninitialised pointer {}", var.name));
                        };
                        let (code, pointer) = self.expr(init)?;
                        if !matches!(pointer, Ty::Ptr(_)) {
                            return err(format!("pointer {} set to a {pointer:?}", var.name));
                        }
                        self.line(format!("var {}: i32 = {code};", var.name));
                        self.declare(&var.name, pointer);
                        continue;
                    }
                    let base = declared_type(&ty.name)?;
                    match (&var.array, &var.init) {
                        (Some(_), Some(Expr::InitList(items))) => {
                            let elements = items
                                .iter()
                                .map(|item| self.coerced(item, &base))
                                .collect::<Result<Vec<_>>>()?
                                .join(", ");
                            let array = Ty::Array(Box::new(base.clone()), items.len());
                            self.line(format!("var {} = {}({elements});", var.name, array.wgsl()));
                            self.declare(&var.name, array);
                        }
                        (Some(size), None) => {
                            let Expr::Number { value, .. } = size else {
                                return err("non-constant array size");
                            };
                            let array = Ty::Array(Box::new(base.clone()), *value as usize);
                            self.line(format!("var {}: {};", var.name, array.wgsl()));
                            self.declare(&var.name, array);
                        }
                        (None, Some(init)) => {
                            let code = self.coerced(init, &base)?;
                            self.line(format!("var {}: {} = {code};", var.name, base.wgsl()));
                            self.declare(&var.name, base.clone());
                        }
                        (None, None) => {
                            self.line(format!("var {}: {};", var.name, base.wgsl()));
                            self.declare(&var.name, base.clone());
                        }
                        _ => return err("unsupported array declaration"),
                    }
                }
            }
            Stmt::Block(statements) => {
                self.line("{");
                self.indent += 1;
                self.block(statements)?;
                self.indent -= 1;
                self.line("}");
            }
            Stmt::If {
                cond,
                then,
                otherwise,
            } => {
                // Branches on baked parameters fold away at generation time.
                if let Some(constant) = self.constant_condition(cond)? {
                    if constant {
                        return self.inline(then);
                    }
                    return match otherwise {
                        Some(otherwise) => self.inline(otherwise),
                        None => Ok(()),
                    };
                }
                let cond = self.coerced(cond, &Ty::Bool)?;
                self.line(format!("if ({cond}) {{"));
                self.indent += 1;
                self.body(then)?;
                self.indent -= 1;
                match otherwise {
                    Some(otherwise) => {
                        self.line("} else {");
                        self.indent += 1;
                        self.body(otherwise)?;
                        self.indent -= 1;
                        self.line("}");
                    }
                    None => self.line("}"),
                }
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
            } => {
                self.line("{");
                self.indent += 1;
                self.scopes.push(HashMap::new());
                if let Some(init) = init {
                    self.statement(init)?;
                }
                self.line("loop {");
                self.indent += 1;
                if let Some(cond) = cond {
                    let cond = self.coerced(cond, &Ty::Bool)?;
                    self.line(format!("if (!({cond})) {{ break; }}"));
                }
                self.body(body)?;
                if let Some(step) = step {
                    self.line("continuing {");
                    self.indent += 1;
                    self.expression_statement(step)?;
                    self.indent -= 1;
                    self.line("}");
                }
                self.indent -= 1;
                self.line("}");
                self.scopes.pop();
                self.indent -= 1;
                self.line("}");
            }
            Stmt::While { cond, body } => {
                let cond = self.coerced(cond, &Ty::Bool)?;
                self.line("loop {");
                self.indent += 1;
                self.line(format!("if (!({cond})) {{ break; }}"));
                self.body(body)?;
                self.indent -= 1;
                self.line("}");
            }
            Stmt::DoWhile { body, cond } => {
                self.line("loop {");
                self.indent += 1;
                self.body(body)?;
                let cond = self.coerced(cond, &Ty::Bool)?;
                self.line("continuing {");
                self.indent += 1;
                self.line(format!("break if !({cond});"));
                self.indent -= 1;
                self.line("}");
                self.indent -= 1;
                self.line("}");
            }
            Stmt::Switch { value, cases } => {
                let value = self.coerced(value, &Ty::I32)?;
                self.line(format!("switch ({value}) {{"));
                self.indent += 1;
                // Empty cases fall into the next case: their labels join it.
                let mut pending: Vec<String> = Vec::new();
                for (index, case) in cases.iter().enumerate() {
                    for label in &case.labels {
                        pending.push(match label {
                            None => "default".to_owned(),
                            Some(expr) => self.coerced(expr, &Ty::I32)?,
                        });
                    }
                    if case.body.is_empty() && index + 1 < cases.len() {
                        continue;
                    }
                    let ends = matches!(
                        case.body.last(),
                        Some(Stmt::Break | Stmt::Return(_) | Stmt::Continue)
                    );
                    if !ends && index + 1 < cases.len() {
                        return err("switch fall-through is not supported");
                    }
                    self.line(format!("case {}: {{", pending.join(", ")));
                    pending.clear();
                    self.indent += 1;
                    let body: Vec<Stmt> = match case.body.split_last() {
                        Some((Stmt::Break, rest)) => rest.to_vec(),
                        _ => case.body.clone(),
                    };
                    self.block(&body)?;
                    self.indent -= 1;
                    self.line("}");
                }
                if !cases.iter().any(|case| case.labels.contains(&None)) {
                    self.line("default: {}");
                }
                self.indent -= 1;
                self.line("}");
            }
            Stmt::Break => self.line("break;"),
            Stmt::Continue => self.line("continue;"),
            Stmt::Return(Some(expr)) => {
                let code = self.coerced(expr, &Ty::Vec(4))?;
                self.line(format!("return {code};"));
            }
            Stmt::Return(None) => return err("bare return in a formula"),
        }
        Ok(())
    }

    /// Emits a branch body without braces (for folded constant conditions).
    fn inline(&mut self, statement: &Stmt) -> Result<()> {
        self.line("{");
        self.indent += 1;
        self.body(statement)?;
        self.indent -= 1;
        self.line("}");
        Ok(())
    }

    /// Evaluates conditions that only read baked parameters and literals.
    fn constant_condition(&mut self, cond: &Expr) -> Result<Option<bool>> {
        fn eval(emitter: &mut Emitter<'_>, expr: &Expr) -> Result<Option<i64>> {
            Ok(match expr {
                Expr::Number {
                    value,
                    float: false,
                } => Some(*value as i64),
                Expr::Ident(name) if name == "true" => Some(1),
                Expr::Ident(name) if name == "false" => Some(0),
                Expr::Ident(name) => emitter.params().constants.get(name).copied(),
                Expr::Member { .. } => match emitter.parameter_path(expr)? {
                    Some(ParameterRef::Path(path)) => match emitter.params().values.get(&path) {
                        Some(Value::Bool(b)) => Some(i64::from(*b)),
                        Some(Value::Int(i)) => Some(*i),
                        _ => None,
                    },
                    _ => None,
                },
                Expr::Unary {
                    op: UnaryOp::Not,
                    expr,
                } => eval(emitter, expr)?.map(|v| i64::from(v == 0)),
                Expr::Binary { op, lhs, rhs } => match (eval(emitter, lhs)?, eval(emitter, rhs)?) {
                    (Some(a), Some(b)) => Some(match op {
                        BinaryOp::And => i64::from(a != 0 && b != 0),
                        BinaryOp::Or => i64::from(a != 0 || b != 0),
                        BinaryOp::Eq => i64::from(a == b),
                        BinaryOp::Ne => i64::from(a != b),
                        BinaryOp::Lt => i64::from(a < b),
                        BinaryOp::Le => i64::from(a <= b),
                        BinaryOp::Gt => i64::from(a > b),
                        BinaryOp::Ge => i64::from(a >= b),
                        _ => return Ok(None),
                    }),
                    _ => None,
                },
                _ => None,
            })
        }
        Ok(eval(self, cond)?.map(|v| v != 0))
    }

    fn expression_statement(&mut self, expr: &Expr) -> Result<()> {
        match expr {
            Expr::Assign { op, target, value } => self.assignment(*op, target, value),
            Expr::Unary {
                op:
                    op @ (UnaryOp::PostIncrement
                    | UnaryOp::PreIncrement
                    | UnaryOp::PostDecrement
                    | UnaryOp::PreDecrement),
                expr,
            } => {
                let (target, _) = self.place(expr)?;
                let operator = if matches!(op, UnaryOp::PostIncrement | UnaryOp::PreIncrement) {
                    "++"
                } else {
                    "--"
                };
                self.line(format!("{target}{operator};"));
                Ok(())
            }
            Expr::Call { name, .. } if name == "Q_UNUSED" => Ok(()),
            Expr::Call { name, args } if name == "__comma" => {
                self.expression_statement(&args[0])?;
                self.expression_statement(&args[1])
            }
            other => {
                let (code, _) = self.expr(other)?;
                self.line(format!("_ = {code};"));
                Ok(())
            }
        }
    }

    /// A writable reference and its type.
    fn place(&mut self, expr: &Expr) -> Result<(String, Ty)> {
        match expr {
            Expr::Ident(name) => match self.lookup(name) {
                Some(ty) => Ok((name.clone(), ty)),
                None => err(format!("assignment to unknown {name}")),
            },
            Expr::Member {
                base,
                field,
                arrow: true,
            } if matches!(&**base, Expr::Ident(b) if b == "aux") => {
                let ty = AUX_FIELDS
                    .iter()
                    .find(|(name, _)| *name == field)
                    .map(|(_, ty)| ty.clone());
                match ty {
                    Some(ty) => Ok((format!("(*aux).{field}"), ty)),
                    None => err(format!("unknown aux->{field}")),
                }
            }
            Expr::Member {
                base,
                field,
                arrow: false,
            } => {
                let (base, ty) = self.place(base)?;
                match swizzle_type(&ty, field) {
                    Some(ty) => Ok((format!("{base}.{field}"), ty)),
                    None => err(format!("member .{field} of {ty:?}")),
                }
            }
            Expr::Unary {
                op: UnaryOp::Deref,
                expr: inner,
            } => match self.expr(inner)? {
                (pointer, Ty::Ptr(target)) => Ok((format!("{target}[{pointer}]"), Ty::F32)),
                (_, other) => err(format!("dereference of {other:?}")),
            },
            Expr::Index { base, index } => {
                let (base, ty) = self.place(base)?;
                let index = self.coerced(index, &Ty::I32)?;
                match ty {
                    Ty::Array(element, _) => Ok((format!("{base}[{index}]"), *element)),
                    Ty::Vec(_) => Ok((format!("{base}[{index}]"), Ty::F32)),
                    Ty::Ptr(target) => Ok((format!("{target}[{base} + {index}]"), Ty::F32)),
                    other => err(format!("index into {other:?}")),
                }
            }
            other => err(format!("cannot assign to {other:?}")),
        }
    }

    fn assignment(&mut self, op: Option<BinaryOp>, target: &Expr, value: &Expr) -> Result<()> {
        let (place, ty) = self.place(target)?;
        let value = match op {
            Some(op) => {
                let combined = Expr::Binary {
                    op,
                    lhs: Box::new(target.clone()),
                    rhs: Box::new(value.clone()),
                };
                self.coerced(&combined, &ty)?
            }
            None => self.coerced(value, &ty)?,
        };
        // WGSL cannot assign through a multi-component swizzle.
        if let Some((base, field)) = place.rsplit_once('.')
            && field.len() > 1
            && swizzle_indices(field).is_some()
        {
            self.temporaries += 1;
            let temporary = format!("swizzled{}", self.temporaries);
            self.line(format!("let {temporary} = {value};"));
            for (slot, component) in field.chars().enumerate() {
                let source = ['x', 'y', 'z', 'w'][slot];
                self.line(format!("{base}.{component} = {temporary}.{source};"));
            }
            return Ok(());
        }
        self.line(format!("{place} = {value};"));
        Ok(())
    }

    /// `fractal->a.b[i]` as a parameter reference, when `expr` reads one.
    fn parameter_path(&mut self, expr: &Expr) -> Result<Option<ParameterRef>> {
        // Field names, and index expressions as `Err`.
        let mut segments: Vec<std::result::Result<String, &Expr>> = Vec::new();
        let mut node = expr;
        loop {
            match node {
                Expr::Member { base, field, .. } => {
                    segments.push(Ok(field.clone()));
                    node = base;
                }
                Expr::Index { base, index } => {
                    segments.push(Err(&**index));
                    node = base;
                }
                Expr::Ident(name) if name == "fractal" => break,
                _ => return Ok(None),
            }
        }
        segments.reverse();
        let constant = |index: &Expr| match index {
            Expr::Number {
                value,
                float: false,
            } => Some(*value as i64),
            _ => None,
        };
        let dynamic = segments
            .iter()
            .any(|s| matches!(s, Err(index) if constant(index).is_none()));
        let mut path = String::new();
        let mut indices = Vec::new();
        for segment in &segments {
            match segment {
                Ok(field) => {
                    if !path.is_empty() {
                        path.push('.');
                    }
                    path.push_str(field);
                }
                Err(index) if dynamic => {
                    path.push_str("[]");
                    indices.push(self.coerced(index, &Ty::I32)?);
                }
                Err(index) => {
                    let _ = write!(path, "[{}]", constant(index).expect("constant"));
                }
            }
        }
        Ok(Some(if dynamic {
            ParameterRef::Indexed {
                template: path,
                indices,
            }
        } else {
            ParameterRef::Path(path)
        }))
    }

    fn has_family(&self, template: &str) -> bool {
        self.params()
            .values
            .keys()
            .any(|key| index_template(key).0 == template)
    }

    /// The family of `template` and the flat element offset (WGSL code,
    /// in slots) for `indices`.
    fn family_offset(&mut self, template: &str, indices: &[String]) -> Result<(Family, String)> {
        let family = self.family(template)?;
        if family.dims.len() != indices.len() {
            return err(format!(
                "fractal->{template} needs {} indices",
                family.dims.len()
            ));
        }
        let mut flat = String::new();
        for (position, index) in indices.iter().enumerate() {
            let step: usize = family.dims[position + 1..].iter().product();
            if !flat.is_empty() {
                flat.push_str(" + ");
            }
            let _ = write!(flat, "({index}) * {}", step * family.stride);
        }
        Ok((family, flat))
    }

    /// The first buffer slot (WGSL code) and type of a parameter read
    /// through runtime indices.
    fn indexed_parameter(&mut self, template: &str, indices: &[String]) -> Result<(String, Ty)> {
        let (family, flat) = self.family_offset(template, indices)?;
        if family.table.is_some() {
            return err(format!("fractal->{template} is baked, not in the buffer"));
        }
        Ok((format!("({} + {flat})", family.first), family.ty))
    }

    /// Reads a parameter element through runtime indices.
    fn indexed_read(&mut self, template: &str, indices: &[String]) -> Result<(String, Ty)> {
        let (family, flat) = self.family_offset(template, indices)?;
        Ok(match &family.table {
            Some(table) => (format!("{table}[{flat}]"), family.ty),
            None => {
                let k = format!("({} + {flat})", family.first);
                (Self::buffer_code(&k, &family.ty), family.ty)
            }
        })
    }

    fn parameter_ref(&mut self, reference: ParameterRef) -> Result<(String, Ty)> {
        match reference {
            ParameterRef::Path(path) => self.parameter(&path),
            ParameterRef::Indexed { template, indices } => {
                // A trailing swizzle (`direction[i].x`) reads part of an element.
                if let Some((base, field)) = template.rsplit_once('.')
                    && swizzle_indices(field).is_some()
                    && !self.has_family(&template)
                {
                    let (code, ty) = self.indexed_read(base, &indices)?;
                    return match swizzle_type(&ty, field) {
                        Some(result) => Ok((format!("{code}.{field}"), result)),
                        None => err(format!("swizzle .{field} of fractal->{base}")),
                    };
                }
                self.indexed_read(&template, &indices)
            }
        }
    }

    /// `&expr`: a component pointer into a local vector, a vector parameter
    /// or the target of another pointer.
    fn address_of(&mut self, expr: &Expr) -> Result<(String, Ty)> {
        if let Some(reference) = self.parameter_path(expr)? {
            let (k, ty) = match reference {
                ParameterRef::Path(path) => match self.slot(&path)? {
                    Slot::Buffer(k, ty) => (k.to_string(), ty),
                    Slot::Baked(..) => return err(format!("address of baked fractal->{path}")),
                },
                ParameterRef::Indexed { template, indices } => {
                    self.indexed_parameter(&template, &indices)?
                }
            };
            return match ty {
                Ty::F32 | Ty::Vec(_) => Ok(("0".into(), Ty::Ptr(format!("fractal_params[{k}]")))),
                other => err(format!("pointer into {other:?} parameter")),
            };
        }
        match expr {
            Expr::Ident(name) => match self.lookup(name) {
                Some(Ty::Vec(_)) => Ok(("0".into(), Ty::Ptr(name.clone()))),
                other => err(format!("address of {name}: {other:?}")),
            },
            Expr::Index { base, index } => {
                let (pointer, ty) = self.expr(base)?;
                let Ty::Ptr(_) = ty else {
                    return err(format!("address of an element of {ty:?}"));
                };
                let offset = self.coerced(index, &Ty::I32)?;
                Ok((format!("({pointer} + {offset})"), ty))
            }
            other => err(format!("address of {other:?}")),
        }
    }

    fn coerced(&mut self, expr: &Expr, target: &Ty) -> Result<String> {
        if let (Expr::Number { value, .. }, Ty::F32 | Ty::Vec(_)) = (expr, target) {
            let literal = float_literal(*value);
            return Ok(match target {
                Ty::Vec(n) => format!("vec{n}<f32>({literal})"),
                _ => literal,
            });
        }
        let (code, ty) = self.expr(expr)?;
        convert(code, &ty, target)
    }

    fn expr(&mut self, expr: &Expr) -> Result<(String, Ty)> {
        Ok(match expr {
            Expr::Number { value, float } => {
                if *float {
                    (float_literal(*value), Ty::F32)
                } else {
                    (format!("{}", *value as i64), Ty::I32)
                }
            }
            Expr::Ident(name) => {
                if let Some(ty) = self.lookup(name) {
                    (name.clone(), ty)
                } else if let Some(value) = self.params().constants.get(name) {
                    (value.to_string(), Ty::I32)
                } else if let Some(value) = crate::constants::int_constant(name) {
                    (value.to_string(), Ty::I32)
                } else if let Some(value) = crate::constants::float_constant(name) {
                    (float_literal(value), Ty::F32)
                } else if name == "true" || name == "false" {
                    (name.clone(), Ty::Bool)
                } else {
                    return err(format!("unknown identifier {name}"));
                }
            }
            Expr::Member { base, field, arrow } => {
                if let Some(reference) = self.parameter_path(expr)? {
                    return self.parameter_ref(reference);
                }
                if *arrow {
                    return self.place(expr);
                }
                let (base, ty) = self.expr(base)?;
                match swizzle_type(&ty, field) {
                    Some(result) => (format!("{base}.{field}"), result),
                    None => return err(format!("member .{field} of {ty:?}")),
                }
            }
            Expr::Index { base, .. } => {
                if let Some(reference) = self.parameter_path(expr)? {
                    return self.parameter_ref(reference);
                }
                let _ = base;
                self.place(expr)?
            }
            Expr::Unary { op, expr: inner } => match op {
                UnaryOp::Neg => {
                    let (code, ty) = self.expr(inner)?;
                    (format!("(-{code})"), ty)
                }
                UnaryOp::Plus => self.expr(inner)?,
                UnaryOp::Not => (format!("(!{})", self.coerced(inner, &Ty::Bool)?), Ty::Bool),
                UnaryOp::BitNot => (format!("(~{})", self.coerced(inner, &Ty::I32)?), Ty::I32),
                UnaryOp::AddressOf => self.address_of(inner)?,
                UnaryOp::Deref => self.place(expr)?,
                _ => return err(format!("{op:?} inside an expression is not supported")),
            },
            Expr::Binary { op, lhs, rhs } => self.binary(*op, lhs, rhs)?,
            Expr::Assign { .. } => return err("assignment inside an expression is not supported"),
            Expr::Ternary {
                cond,
                then,
                otherwise,
            } => {
                let (_, then_ty) = self.expr(then)?;
                let (_, else_ty) = self.expr(otherwise)?;
                let ty = common_type(&then_ty, &else_ty)?;
                let cond = self.coerced(cond, &Ty::Bool)?;
                let then = self.coerced(then, &ty)?;
                let otherwise = self.coerced(otherwise, &ty)?;
                (format!("select({otherwise}, {then}, {cond})"), ty)
            }
            Expr::Call { name, args } => self.call(name, args)?,
            Expr::Cast { ty, expr: inner } => {
                if ty.pointer {
                    // `(REAL *)&v` only reinterprets a vector as its components.
                    return self.expr(inner);
                }
                let target = declared_type(&ty.name)?;
                let (code, from) = self.expr(inner)?;
                (convert(code, &from, &target)?, target)
            }
            Expr::VectorLiteral { ty, elements } => {
                let target = declared_type(&ty.name)?;
                let Ty::Vec(n) = target else {
                    return err("vector literal of a non-vector type");
                };
                let mut parts = Vec::new();
                for element in elements {
                    let (code, ty) = self.expr(element)?;
                    parts.push(match ty {
                        Ty::Vec(_) => code,
                        other => convert(code, &other, &Ty::F32)?,
                    });
                }
                (format!("vec{n}<f32>({})", parts.join(", ")), target)
            }
            Expr::InitList(_) => return err("initialiser list outside a declaration"),
        })
    }

    fn parameter(&mut self, path: &str) -> Result<(String, Ty)> {
        // A trailing swizzle (`color.factor.x`) reads part of a vector slot.
        if !self.params().values.contains_key(path)
            && let Some((base, field)) = path.rsplit_once('.')
            && swizzle_indices(field).is_some()
            && self.params().values.contains_key(base)
        {
            let (code, ty) = Self::slot_code(&self.slot(base)?);
            return match swizzle_type(&ty, field) {
                Some(result) => Ok((format!("{code}.{field}"), result)),
                None => err(format!("swizzle .{field} of fractal->{base}")),
            };
        }
        Ok(Self::slot_code(&self.slot(path)?))
    }

    fn binary(&mut self, op: BinaryOp, lhs: &Expr, rhs: &Expr) -> Result<(String, Ty)> {
        use BinaryOp::*;
        let symbol = match op {
            Add => "+",
            Sub => "-",
            Mul => "*",
            Div => "/",
            Rem => "%",
            Shl => "<<",
            Shr => ">>",
            BitAnd => "&",
            BitOr => "|",
            BitXor => "^",
            And => "&&",
            Or => "||",
            Eq => "==",
            Ne => "!=",
            Lt => "<",
            Le => "<=",
            Gt => ">",
            Ge => ">=",
        };
        if matches!(op, And | Or) {
            let (a, b) = (self.coerced(lhs, &Ty::Bool)?, self.coerced(rhs, &Ty::Bool)?);
            return Ok((format!("({a} {symbol} {b})"), Ty::Bool));
        }
        if matches!(op, Shl | Shr | BitAnd | BitOr | BitXor) {
            let (a, b) = (self.coerced(lhs, &Ty::I32)?, self.coerced(rhs, &Ty::I32)?);
            let b = if matches!(op, Shl | Shr) {
                format!("u32({b})")
            } else {
                b
            };
            return Ok((format!("({a} {symbol} {b})"), Ty::I32));
        }
        let (_, lt) = self.expr(lhs)?;
        let (_, rt) = self.expr(rhs)?;
        let operand = common_type(&lt, &rt)?;
        let comparison = matches!(op, Eq | Ne | Lt | Le | Gt | Ge);
        // Vector-scalar arithmetic is native in WGSL; keep the scalar side.
        let (a, b) = match (&lt, &rt, comparison) {
            (Ty::Vec(_), r, false) if r.is_scalar() => {
                (self.coerced(lhs, &lt)?, self.coerced(rhs, &Ty::F32)?)
            }
            (l, Ty::Vec(_), false) if l.is_scalar() => {
                (self.coerced(lhs, &Ty::F32)?, self.coerced(rhs, &rt)?)
            }
            _ => (self.coerced(lhs, &operand)?, self.coerced(rhs, &operand)?),
        };
        Ok((
            format!("({a} {symbol} {b})"),
            if comparison { Ty::Bool } else { operand },
        ))
    }

    /// Arguments `..count` converted to f32, or to the widest vector among them.
    fn float_args(&mut self, name: &str, args: &[Expr], count: usize) -> Result<(Vec<String>, Ty)> {
        if args.len() < count {
            return err(format!("{name}: expected {count} arguments"));
        }
        let mut ty = Ty::F32;
        for arg in &args[..count] {
            if let (_, Ty::Vec(n)) = self.expr(arg)? {
                ty = Ty::Vec(n);
            }
        }
        let codes = args[..count]
            .iter()
            .map(|arg| self.coerced(arg, &ty))
            .collect::<Result<Vec<_>>>()?;
        Ok((codes, ty))
    }

    fn call(&mut self, name: &str, args: &[Expr]) -> Result<(String, Ty)> {
        let name = name.strip_prefix("native_").unwrap_or(name);
        let float_args = |emitter: &mut Self, count: usize| emitter.float_args(name, args, count);
        let unary = |wgsl: &str, emitter: &mut Self| -> Result<(String, Ty)> {
            let (codes, ty) = emitter.float_args(name, args, 1)?;
            Ok((format!("{wgsl}({})", codes[0]), ty))
        };
        Ok(match name {
            "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh" | "tanh" | "exp"
            | "exp2" | "log" | "log2" | "sqrt" | "floor" | "ceil" | "round" | "trunc" | "sign"
            | "abs" => unary(name, self)?,
            "fabs" => unary("abs", self)?,
            "rsqrt" => unary("inverseSqrt", self)?,
            "log10" => {
                let (codes, ty) = float_args(self, 1)?;
                (format!("(log({}) * 0.4342944819)", codes[0]), ty)
            }
            "recip" => {
                let (codes, ty) = float_args(self, 1)?;
                (format!("(1.0 / {})", codes[0]), ty)
            }
            "atan2" | "pow" | "min" | "max" | "powr" | "fmin" | "fmax" => {
                let wgsl = match name {
                    "powr" => "pow",
                    "fmin" => "min",
                    "fmax" => "max",
                    other => other,
                };
                let (codes, ty) = float_args(self, 2)?;
                (format!("{wgsl}({}, {})", codes[0], codes[1]), ty)
            }
            "fmod" => {
                let (codes, ty) = float_args(self, 2)?;
                (
                    format!("({0} - {1} * trunc({0} / {1}))", codes[0], codes[1]),
                    ty,
                )
            }
            "copysign" => {
                let (codes, ty) = float_args(self, 2)?;
                (
                    format!(
                        "select(abs({0}), -abs({0}), {1} < {2})",
                        codes[0],
                        codes[1],
                        zero_of(&ty)
                    ),
                    ty,
                )
            }
            "clamp" | "mad" => {
                let wgsl = if name == "mad" { "fma" } else { "clamp" };
                let (codes, ty) = float_args(self, 3)?;
                (
                    format!("{wgsl}({}, {}, {})", codes[0], codes[1], codes[2]),
                    ty,
                )
            }
            "mix" => {
                let (codes, ty) = float_args(self, 2)?;
                let t_ty = match self.expr(&args[2])?.1 {
                    Ty::Vec(n) => Ty::Vec(n),
                    _ => Ty::F32,
                };
                let t = self.coerced(&args[2], &t_ty)?;
                (format!("mix({}, {}, {t})", codes[0], codes[1]), ty)
            }
            "dot" | "distance" => {
                let (codes, _) = float_args(self, 2)?;
                (format!("{name}({}, {})", codes[0], codes[1]), Ty::F32)
            }
            "length" => {
                let (codes, ty) = float_args(self, 1)?;
                let code = if ty.is_scalar() {
                    format!("abs({})", codes[0])
                } else {
                    format!("length({})", codes[0])
                };
                (code, Ty::F32)
            }
            "normalize" => unary("normalize", self)?,
            "cross" => {
                let (codes, ty) = float_args(self, 2)?;
                match ty {
                    Ty::Vec(4) => (
                        format!("vec4<f32>(cross({}.xyz, {}.xyz), 0.0)", codes[0], codes[1]),
                        ty,
                    ),
                    _ => (format!("cross({}, {})", codes[0], codes[1]), ty),
                }
            }
            "Matrix33MulFloat4" | "Matrix33MulFloat3" => {
                let matrix = self.coerced(&args[0], &Ty::Mat3)?;
                let (vector, ty) = self.expr(&args[1])?;
                // Rows-times-vector, as CRotationMatrix::RotateVector: the
                // matrix holds rows as columns, so v * M gives row dot v.
                match ty {
                    Ty::Vec(4) => (
                        format!("vec4<f32>(({vector}).xyz * {matrix}, ({vector}).w)"),
                        ty,
                    ),
                    _ => (format!("(({vector}) * {matrix})"), ty),
                }
            }
            other => return err(format!("unsupported function {other}")),
        })
    }

    fn distance(&mut self, julia_slot: usize) -> Result<()> {
        let fractal = self.fractal;
        self.current = 0;
        let actual_scale = match self.params().values.get("mandelbox.scale") {
            Some(_) => Self::slot_code(&self.slot("mandelbox.scale")?).0,
            None => "2.0".into(),
        };
        let sequence_length = fractal.sequence.len().max(1);
        if fractal.hybrid {
            let entries: Vec<String> = fractal
                .sequence
                .iter()
                .map(|entry| entry.map_or(-1, |k| k as i64).to_string())
                .collect();
            let _ = writeln!(
                self.globals,
                "// Formula run by each iteration; -1 is an empty slot.\nvar<private> fractal_sequence: array<i32, {sequence_length}> = array<i32, {sequence_length}>({});\n",
                entries.join(", ")
            );
        }
        let weights: Vec<usize> = fractal
            .formulas
            .iter()
            .map(|f| self.push_slot([f.weight as f32, 0.0, 0.0, 0.0]))
            .collect();
        let linear_offset = self.push_slot([fractal.linear_de_offset as f32, 0.0, 0.0, 0.0]);
        self.line("struct FractalOrbit {");
        self.line("    z: vec4<f32>,");
        self.line("    aux: FractalAux,");
        self.line("    iterations: i32,");
        self.line("    formula: i32,");
        self.line("    empty: bool,");
        self.line("}");
        self.line("");
        self.line("// `forced` >= 0 runs exactly that many iterations with no bailout,");
        self.line("// as Delta-DE's probes do.");
        self.line("fn fractal_orbit(p: vec3<f32>, forced: i32) -> FractalOrbit {");
        self.indent += 1;
        self.line("let header = fractal_params[0];");
        self.line("let max_iterations = select(i32(header.x), forced, forced >= 0);");
        self.line("let bailout = header.y;");
        self.line("var z = vec4<f32>(p, header.z);");
        self.line("var aux: FractalAux;");
        for (field, value) in [
            ("i", "0".to_owned()),
            ("c", "z".into()),
            ("const_c", "z".into()),
            ("old_z", "z".into()),
            ("pos_neg", "1.0".into()),
            ("r", "length(z)".into()),
            ("DE", "1.0".into()),
            ("DE0", "0.0".into()),
            ("dist", "1000.0".into()),
            ("pseudoKleinianDE", "1.0".into()),
            ("actualScale", actual_scale),
            ("actualScaleA", "0.0".into()),
            ("color", "1.0".into()),
            ("colorHybrid", "0.0".into()),
            ("temp1000", "1000.0".into()),
        ] {
            self.line(format!("aux.{field} = {value};"));
        }
        self.line("var iterations = 0;");
        self.line("var formula = 0;");
        self.line("var last_z = z;");
        self.line("var done = false;");
        self.line("for (var i = 0; i < max_iterations; i++) {");
        self.indent += 1;
        self.line("iterations = i + 1;");
        self.line("let last_last_z = last_z;");
        self.line("last_z = z;");
        self.line("aux.i = i;");
        if fractal.hybrid {
            self.line(format!(
                "formula = fractal_sequence[min(i, {})];",
                sequence_length - 1
            ));
            self.line("if (formula < 0) { return FractalOrbit(z, aux, 1, 0, true); }");
            self.line("let previous_de = aux.DE;");
            self.line("let previous_color = aux.color;");
            self.line("switch formula {");
        }
        let m = "fractal_params[1].xyz";
        for (k, formula) in fractal.formulas.iter().enumerate() {
            if fractal.hybrid {
                let label = if k + 1 == fractal.formulas.len() {
                    "default".to_owned()
                } else {
                    format!("case {k}")
                };
                self.line(format!("{label}: {{"));
                self.indent += 1;
                let weight = format!("fractal_params[{}].x", weights[k]);
                self.line(format!(
                    "if ({weight} > 0.0) {{ z = fractal_formula_{k}(z, &aux); }}"
                ));
            } else {
                self.line(format!("z = fractal_formula_{k}(z, &aux);"));
            }
            if formula.add_c {
                let swapped = matches!(
                    formula.meta.internal_name.as_str(),
                    "abox_mod1" | "amazing_surf"
                );
                let add = match (fractal.julia.is_some(), swapped) {
                    (true, false) => {
                        format!("vec4<f32>(fractal_params[{julia_slot}].xyz * {m}, 0.0)")
                    }
                    (true, true) => {
                        format!("vec4<f32>(fractal_params[{julia_slot}].yxz * {m}.yxz, 0.0)")
                    }
                    (false, false) => format!("vec4<f32>(aux.const_c.xyz * {m}, aux.const_c.w)"),
                    (false, true) => format!("vec4<f32>(aux.const_c.yxz * {m}, 0.0)"),
                };
                self.line(format!("z += {add};"));
            }
            if fractal.hybrid {
                let weight = format!("fractal_params[{}].x", weights[k]);
                self.line(format!("if ({weight} < 1.0) {{"));
                self.line(format!(
                    "    z = fractal_smooth_vector(last_z, z, {weight});"
                ));
                self.line(format!(
                    "    aux.DE = aux.DE * {weight} + previous_de * (1.0 - {weight});"
                ));
                self.line(format!(
                    "    aux.color = aux.color * {weight} + previous_color * (1.0 - {weight});"
                ));
                self.line("}");
            }
            // `break` inside a WGSL switch leaves only the switch.
            let exit = if fractal.hybrid {
                "done = true;"
            } else {
                "break;"
            };
            self.line("aux.r = length(z);");
            self.line("if (any(z != z)) {");
            self.line("    z = last_z;");
            self.line("    aux.r = length(z);");
            self.line(format!("    {exit}"));
            self.line("}");
            if formula.check_bailout {
                self.line("if (!done && forced < 0) {");
                self.line(format!("    if (aux.r > bailout) {{ {exit} }}"));
                if formula.additional_bailout {
                    // The pseudo-Kleinian "orbit stalled" exit.
                    self.line(format!(
                        "    if (length(z - last_z) / aux.r < 0.1 / bailout) {{ {exit} }}"
                    ));
                    self.line(format!(
                        "    if (length(z - last_last_z) / aux.r < 0.1 / bailout) {{ {exit} }}"
                    ));
                }
                self.line("}");
            }
            if fractal.hybrid {
                self.indent -= 1;
                self.line("}");
            }
        }
        if fractal.hybrid {
            self.line("}");
            self.line("if (done) { break; }");
        }
        self.indent -= 1;
        self.line("}");
        self.line("return FractalOrbit(z, aux, iterations, formula, false);");
        self.indent -= 1;
        self.line("}");
        self.line("");
        if fractal.hybrid {
            self.line("// Mandelbulber's SmoothCVector: blends direction and length apart.");
            self.line(
                "fn fractal_smooth_vector(a: vec4<f32>, b: vec4<f32>, k: f32) -> vec4<f32> {",
            );
            self.line("    if (k <= 0.0) { return a; }");
            self.line("    if (k >= 1.0) { return b; }");
            self.line("    let blend = a * (1.0 - k) + b * k;");
            self.line("    let blend_length = length(blend);");
            self.line("    if (blend_length <= 0.0) { return a; }");
            self.line("    return blend / blend_length * (length(a) * (1.0 - k) + length(b) * k);");
            self.line("}");
            self.line("");
        }
        self.line("fn fractal_distance(p: vec3<f32>, detail_size: f32) -> f32 {");
        self.indent += 1;
        self.line("let orbit = fractal_orbit(p, -1);");
        self.line("if (orbit.empty) { return 10.0; }");
        self.line("let z = orbit.z;");
        self.line("let aux = orbit.aux;");
        if fractal.delta {
            self.delta_distance()?;
        } else {
            self.analytic_distance(linear_offset)?;
        }
        self.line(
            "if (orbit.iterations < i32(fractal_params[0].w) && distance < detail_size) { distance = detail_size; }",
        );
        self.line("return clamp(distance, 0.0, 10.0);");
        self.indent -= 1;
        self.line("}");
        Ok(())
    }

    fn analytic_distance(&mut self, linear_offset: usize) -> Result<()> {
        self.current = 0;
        let jos_kleinian = || -> String {
            "min(jos_y, fractal_jos_tweak) / max(aux.DE, fractal_jos_offset)".into()
        };
        let function = if self.fractal.hybrid {
            match self.fractal.de_function {
                DeFunction::Linear => DeFunction::Linear,
                DeFunction::Logarithmic => DeFunction::Logarithmic,
                DeFunction::PseudoKleinian => DeFunction::PseudoKleinian,
                DeFunction::JosKleinian => DeFunction::JosKleinian,
                DeFunction::Custom => DeFunction::Custom,
                DeFunction::MaxAxis => DeFunction::MaxAxis,
                _ => DeFunction::None,
            }
        } else {
            self.fractal.primary().meta.analytic_function
        };
        if function == DeFunction::JosKleinian {
            let (tweak, _) = self.parameter("analyticDE.tweak005")?;
            let (offset, _) = self.parameter("analyticDE.offset1")?;
            let (spheres, _) = self.parameter("transformCommon.spheresEnabled")?;
            let (folding, _) = self.parameter("transformCommon.foldingValue")?;
            self.line(format!("let fractal_jos_tweak = {tweak};"));
            self.line(format!("let fractal_jos_offset = {offset};"));
            self.line(format!(
                "let jos_y = select(z.y, min(z.y, {folding} - z.y), {spheres});"
            ));
        }
        let analytic = match (function, self.fractal.hybrid) {
            (DeFunction::Logarithmic, _) => {
                "select(0.0, 0.5 * aux.r * log(aux.r) / aux.DE, aux.r > 1.0)".to_owned()
            }
            (DeFunction::Linear, true) => {
                format!("(aux.r - fractal_params[{linear_offset}].x) / aux.DE")
            }
            (DeFunction::Linear, false) => "aux.r / aux.DE".into(),
            (DeFunction::Ifs, _) => "(aux.r - 2.0) / aux.DE".into(),
            (DeFunction::PseudoKleinian, _) => {
                "max(length(z.xy) - aux.pseudoKleinianDE, abs(length(z.xy) * z.z) / aux.r) / aux.DE"
                    .into()
            }
            (DeFunction::MaxAxis, _) => "max(abs(z.x), max(abs(z.y), abs(z.z))) / aux.DE".into(),
            (DeFunction::Custom, _) => "aux.dist".into(),
            (DeFunction::JosKleinian, _) => jos_kleinian(),
            (DeFunction::None, true) => "0.0".into(),
            (DeFunction::None, false) => "-1.0".into(),
        };
        self.line(format!(
            "var distance = select(aux.r, {analytic}, aux.DE > 0.0);"
        ));
        Ok(())
    }

    /// Mandelbulber's Delta-DE (`CalculateDistance`): the radial gradient of
    /// the xyz orbit radius from three probes at the same iteration count.
    /// Native steps by 1e-5 x detail size in f64; f32 cannot resolve that
    /// far from the origin, so the step never drops below 1e-5 of |p|.
    fn delta_distance(&mut self) -> Result<()> {
        let finaliser = match self.fractal.de_function {
            DeFunction::Linear => "0.5 * r / dr",
            DeFunction::Logarithmic => "0.5 * r * log(r) / dr",
            DeFunction::PseudoKleinian => {
                "max(length(z.xy) - 0.92784, abs(length(z.xy) * z.z) / r) / dr"
            }
            DeFunction::JosKleinian => "abs(length(z.xz) * z.y) / r / dr",
            DeFunction::MaxAxis => {
                "0.5 * max(abs(z.x), max(abs(z.y), abs(z.z))) / max(gradient.x, max(gradient.y, gradient.z))"
            }
            _ => "r",
        };
        self.line("let delta = max(1e-5 * detail_size, 1e-5 * max(length(p), 1.0));");
        self.line("let r = length(z.xyz);");
        // Divide by the step f32 actually took: p + delta rounds.
        self.line("var gradient: vec3<f32>;");
        self.line("for (var axis = 0; axis < 3; axis++) {");
        self.line("    var ahead = p;");
        self.line("    var behind = p;");
        self.line("    ahead[axis] += delta;");
        self.line("    behind[axis] -= delta;");
        self.line("    let step = ahead[axis] - behind[axis];");
        self.line("    let r_ahead = length(fractal_orbit(ahead, orbit.iterations).z.xyz);");
        self.line("    let r_behind = length(fractal_orbit(behind, orbit.iterations).z.xyz);");
        // Across a fold kink the central slope can vanish; the smaller
        // one-sided slope keeps the step bounded there.
        self.line("    let one_sided = min(abs(r_ahead - r), abs(r - r_behind)) / (0.5 * step);");
        self.line("    gradient[axis] = max(abs(r_ahead - r_behind) / step, one_sided);");
        self.line("}");
        self.line("let dr = length(gradient);");
        self.line(format!("var distance = select(r, {finaliser}, dr > 0.0);"));
        Ok(())
    }
}

/// `a.b[2][0]` as (`a.b[][]`, [2, 0]).
fn index_template(key: &str) -> (String, Vec<usize>) {
    let mut template = String::new();
    let mut indices = Vec::new();
    let mut rest = key;
    while let Some(open) = rest.find('[') {
        template.push_str(&rest[..open]);
        let Some(close) = rest[open..].find(']') else {
            break;
        };
        match rest[open + 1..open + close].parse() {
            Ok(index) => {
                indices.push(index);
                template.push_str("[]");
            }
            Err(_) => template.push_str(&rest[open..=open + close]),
        }
        rest = &rest[open + close + 1..];
    }
    template.push_str(rest);
    (template, indices)
}

fn zero_of(ty: &Ty) -> String {
    match ty {
        Ty::Vec(n) => format!("vec{n}<f32>(0.0)"),
        _ => "0.0".into(),
    }
}

fn swizzle_indices(field: &str) -> Option<Vec<usize>> {
    if field.is_empty() || field.len() > 4 {
        return None;
    }
    field
        .chars()
        .map(|c| match c {
            'x' => Some(0),
            'y' => Some(1),
            'z' => Some(2),
            'w' => Some(3),
            _ => None,
        })
        .collect()
}

fn swizzle_type(ty: &Ty, field: &str) -> Option<Ty> {
    let Ty::Vec(n) = ty else { return None };
    let indices = swizzle_indices(field)?;
    if indices.iter().any(|&i| i >= usize::from(*n)) {
        return None;
    }
    Some(if indices.len() == 1 {
        Ty::F32
    } else {
        Ty::Vec(indices.len() as u8)
    })
}

fn common_type(a: &Ty, b: &Ty) -> Result<Ty> {
    Ok(match (a, b) {
        (Ty::Vec(n), Ty::Vec(m)) if n == m => Ty::Vec(*n),
        (Ty::Vec(n), other) | (other, Ty::Vec(n)) if other.is_scalar() => Ty::Vec(*n),
        (Ty::F32, _) | (_, Ty::F32) => Ty::F32,
        (Ty::I32, _) | (_, Ty::I32) => Ty::I32,
        (Ty::Bool, Ty::Bool) => Ty::Bool,
        (Ty::Ptr(x), Ty::Ptr(y)) if x == y => a.clone(),
        _ => return err(format!("no common type for {a:?} and {b:?}")),
    })
}

fn convert(code: String, from: &Ty, to: &Ty) -> Result<String> {
    Ok(match (from, to) {
        (a, b) if a == b => code,
        (Ty::I32, Ty::F32) => format!("f32({code})"),
        (Ty::F32, Ty::I32) => format!("i32({code})"),
        (Ty::Bool, Ty::F32) => format!("select(0.0, 1.0, {code})"),
        (Ty::Bool, Ty::I32) => format!("select(0, 1, {code})"),
        (Ty::F32, Ty::Bool) => format!("({code} != 0.0)"),
        (Ty::I32, Ty::Bool) => format!("({code} != 0)"),
        (scalar, Ty::Vec(n)) if scalar.is_scalar() => {
            let scalar = convert(code, scalar, &Ty::F32)?;
            format!("vec{n}<f32>({scalar})")
        }
        (Ty::Vec(n), Ty::Vec(m)) if n > m => format!("({code}).{}", &"xyzw"[..usize::from(*m)]),
        (Ty::Vec(n), Ty::Vec(m)) if n < m => {
            let padding = vec!["0.0"; usize::from(m - n)].join(", ");
            format!("vec{m}<f32>({code}, {padding})")
        }
        _ => return err(format!("cannot convert {from:?} to {to:?}")),
    })
}

fn float_literal(value: f64) -> String {
    let value = value as f32;
    if value.is_finite() && value == value.trunc() && value.abs() < 1e7 {
        format!("{value:.1}")
    } else {
        format!("{value:e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::Parameters;
    use crate::scene::{CAddition, DeType, FormulaMeta};

    const FORMULA: &str = r#"
REAL4 TestIteration(REAL4 z, __constant sFractalCl *fractal, sExtendedAuxCl *aux)
{
	REAL rp = pow(aux->r, fractal->bulb.power - 1.0f);
	aux->DE = rp * aux->DE * fractal->bulb.power + 1.0f;
	if (fractal->test.enabled)
	{
		z.xy = (REAL2){z.y, z.x};
		z = Matrix33MulFloat4(fractal->test.rotation, z);
	}
	else
	{
		z *= 777.0f;
	}
	for (int i = 0; i < 2; i++) z.z = fabs(z.z) * fractal->test.offset.x;
	return z * rp;
}"#;

    fn fractal(enabled: bool) -> Fractal {
        let function = crate::parse_formula_file(FORMULA)
            .unwrap()
            .functions
            .remove(0);
        let mut parameters = Parameters::default();
        parameters
            .values
            .insert("bulb.power".into(), Value::Scalar(8.0));
        parameters
            .values
            .insert("test.enabled".into(), Value::Bool(enabled));
        parameters
            .values
            .insert("test.offset".into(), Value::vec3([0.5, 0.0, 0.0]));
        parameters.values.insert(
            "test.rotation".into(),
            Value::Matrix([[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]),
        );
        let meta = FormulaMeta {
            internal_name: "test".into(),
            de_type: DeType::Analytic,
            de_function: DeFunction::Logarithmic,
            analytic_function: DeFunction::Logarithmic,
            c_addition: CAddition::EnabledByDefault,
            default_bailout: 10.0,
        };
        Fractal {
            formulas: vec![crate::orbit::Formula {
                slot: 1,
                function,
                parameters,
                meta,
                add_c: true,
                weight: 1.0,
                check_bailout: true,
                additional_bailout: false,
            }],
            sequence: vec![Some(0); 20],
            hybrid: false,
            max_iterations: 20,
            min_iterations: 1,
            bailout: 10.0,
            constant_multiplier: [1.0; 3],
            julia: None,
            initial_w: 0.0,
            delta: false,
            de_function: DeFunction::Logarithmic,
            linear_de_offset: 0.0,
        }
    }

    fn validate(shader: &FractalShader) {
        let source = format!(
            "@group(0) @binding(0) {PARAMS_DECLARATION}\n{}",
            shader.source
        );
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{}\n{source}", e.emit_to_string(&source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    }

    #[test]
    fn generated_module_validates_and_folds_baked_branches() {
        let enabled = emit(&fractal(true)).unwrap();
        validate(&enabled);
        assert!(enabled.source.contains("mat3x3<f32>"));
        assert!(!enabled.source.contains("777.0"));
        let disabled = emit(&fractal(false)).unwrap();
        validate(&disabled);
        assert!(disabled.source.contains("777.0"));
        assert!(!disabled.source.contains("mat3x3<f32>"));
        // Header (2) + julia + power + offset (+ 3 matrix rows when enabled)
        // + weight + linear DE offset.
        assert_eq!(disabled.parameters.len(), 7);
        assert_eq!(enabled.parameters.len(), 10);
    }
}
