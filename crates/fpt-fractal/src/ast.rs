//! Syntax tree for the OpenCL C subset used by Mandelbulber formulas.

/// A parsed formula file: its function definitions in source order.
#[derive(Clone, Debug, PartialEq)]
pub struct FormulaFile {
    pub functions: Vec<Function>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    pub return_type: Type,
    pub name: String,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub ty: Type,
    pub name: String,
}

/// A declared type. `name` is the base type as written (`REAL4`, `int`,
/// `sFractalCl`, ...); qualifiers and pointer levels are kept alongside.
#[derive(Clone, Debug, PartialEq)]
pub struct Type {
    pub name: String,
    pub constant: bool,
    pub pointer: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    /// `type a = x, b;` (each declarator may add its own pointer star).
    Decl {
        ty: Type,
        vars: Vec<Declarator>,
    },
    Expr(Expr),
    If {
        cond: Expr,
        then: Box<Stmt>,
        otherwise: Option<Box<Stmt>>,
    },
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Expr>,
        step: Option<Expr>,
        body: Box<Stmt>,
    },
    While {
        cond: Expr,
        body: Box<Stmt>,
    },
    DoWhile {
        body: Box<Stmt>,
        cond: Expr,
    },
    Switch {
        value: Expr,
        cases: Vec<Case>,
    },
    Break,
    Continue,
    Return(Option<Expr>),
    Block(Vec<Stmt>),
    Empty,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Declarator {
    pub name: String,
    pub pointer: bool,
    pub array: Option<Expr>,
    pub init: Option<Expr>,
}

/// One `case`/`default` group: `None` in `labels` is `default`.
#[derive(Clone, Debug, PartialEq)]
pub struct Case {
    pub labels: Vec<Option<Expr>>,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    /// A numeric literal; `float` is true for `1.0`, `1.f`, `1e3`.
    Number {
        value: f64,
        float: bool,
    },
    Ident(String),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// `target op= value`; `op` is `None` for plain `=`.
    Assign {
        op: Option<BinaryOp>,
        target: Box<Expr>,
        value: Box<Expr>,
    },
    Ternary {
        cond: Box<Expr>,
        then: Box<Expr>,
        otherwise: Box<Expr>,
    },
    Call {
        name: String,
        args: Vec<Expr>,
    },
    /// `base.field` or, with `arrow`, `base->field`.
    Member {
        base: Box<Expr>,
        field: String,
        arrow: bool,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Cast {
        ty: Type,
        expr: Box<Expr>,
    },
    /// `(REAL4){a, b, c, d}`.
    VectorLiteral {
        ty: Type,
        elements: Vec<Expr>,
    },
    /// `{a, b, c}` initialising an array declaration.
    InitList(Vec<Expr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Plus,
    Not,
    BitNot,
    Deref,
    AddressOf,
    PreIncrement,
    PreDecrement,
    PostIncrement,
    PostDecrement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
    And,
    Or,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}
