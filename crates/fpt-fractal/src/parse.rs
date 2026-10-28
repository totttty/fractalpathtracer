//! Recursive-descent parser for Mandelbulber's OpenCL C formula files.

use std::fmt;

use crate::ast::*;
use crate::lex::{Token, TokenKind, tokenize};

#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parses every function definition in a formula source file.
pub fn parse_formula_file(source: &str) -> Result<FormulaFile> {
    let tokens = tokenize(source).map_err(|message| ParseError { line: 0, message })?;
    let mut parser = Parser {
        tokens,
        position: 0,
    };
    let mut functions = Vec::new();
    while !parser.at_end() {
        functions.push(parser.function()?);
    }
    Ok(FormulaFile { functions })
}

/// Base types the formula sources declare, cast to, or build literals of.
const TYPE_NAMES: [&str; 26] = [
    "REAL", "REAL2", "REAL3", "REAL4", "float", "float2", "float3", "float4", "double", "double2",
    "double3", "double4", "int", "int2", "int3", "int4", "uint", "unsigned", "bool", "char",
    "short", "long", "void", "matrix33", "matrix44", "size_t",
];

const QUALIFIERS: [&str; 9] = [
    "__constant",
    "constant",
    "const",
    "__private",
    "__global",
    "__local",
    "inline",
    "static",
    "private",
];

struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

type Result<T> = std::result::Result<T, ParseError>;

impl Parser {
    fn at_end(&self) -> bool {
        self.position >= self.tokens.len()
    }

    fn peek(&self) -> Option<&TokenKind> {
        self.tokens.get(self.position).map(|t| &t.kind)
    }

    fn peek_at(&self, offset: usize) -> Option<&TokenKind> {
        self.tokens.get(self.position + offset).map(|t| &t.kind)
    }

    fn line(&self) -> usize {
        self.tokens
            .get(self.position)
            .or_else(|| self.tokens.last())
            .map_or(0, |t| t.line)
    }

    fn error<T>(&self, message: impl Into<String>) -> Result<T> {
        Err(ParseError {
            line: self.line(),
            message: message.into(),
        })
    }

    fn is_punct(&self, punct: &str) -> bool {
        matches!(self.peek(), Some(TokenKind::Punct(p)) if *p == punct)
    }

    fn is_ident(&self, word: &str) -> bool {
        matches!(self.peek(), Some(TokenKind::Ident(w)) if w == word)
    }

    fn eat_punct(&mut self, punct: &str) -> bool {
        if self.is_punct(punct) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn expect_punct(&mut self, punct: &str) -> Result<()> {
        if self.eat_punct(punct) {
            Ok(())
        } else {
            self.error(format!("expected `{punct}`, found {:?}", self.peek()))
        }
    }

    fn ident(&mut self) -> Result<String> {
        match self.peek() {
            Some(TokenKind::Ident(name)) => {
                let name = name.clone();
                self.position += 1;
                Ok(name)
            }
            other => self.error(format!("expected identifier, found {other:?}")),
        }
    }

    fn word_at(&self, offset: usize) -> Option<&str> {
        match self.peek_at(offset) {
            Some(TokenKind::Ident(w)) => Some(w.as_str()),
            _ => None,
        }
    }

    /// Whether a type (with optional qualifiers) starts at `offset`.
    fn type_starts_at(&self, offset: usize) -> bool {
        match self.word_at(offset) {
            Some(word) if QUALIFIERS.contains(&word) || TYPE_NAMES.contains(&word) => true,
            Some(word) if word.starts_with('s') && word.ends_with("Cl") => true,
            _ => false,
        }
    }

    /// A declaration starts here: a known type, or `Name name` / `Name *name`.
    fn declaration_starts(&self) -> bool {
        if self.type_starts_at(0) {
            return true;
        }
        matches!(
            (self.peek(), self.peek_at(1), self.peek_at(2)),
            (Some(TokenKind::Ident(_)), Some(TokenKind::Ident(_)), _)
                | (
                    Some(TokenKind::Ident(_)),
                    Some(TokenKind::Punct("*")),
                    Some(TokenKind::Ident(_))
                )
        ) && !matches!(self.word_at(0), Some("return" | "case" | "else" | "goto"))
    }

    fn base_type(&mut self) -> Result<Type> {
        let mut constant = false;
        while let Some(word) = self.word_at(0) {
            if !QUALIFIERS.contains(&word) {
                break;
            }
            constant |= matches!(word, "__constant" | "constant" | "const");
            self.position += 1;
        }
        let mut name = self.ident()?;
        // `unsigned int`, `long long` and similar two-word types.
        if matches!(name.as_str(), "unsigned" | "long" | "short")
            && matches!(self.word_at(0), Some("int" | "long" | "char" | "short"))
        {
            name = format!("{name} {}", self.ident()?);
        }
        while let Some(word) = self.word_at(0) {
            if !QUALIFIERS.contains(&word) {
                break;
            }
            constant |= matches!(word, "__constant" | "constant" | "const");
            self.position += 1;
        }
        Ok(Type {
            name,
            constant,
            pointer: false,
        })
    }

    fn function(&mut self) -> Result<Function> {
        let mut return_type = self.base_type()?;
        return_type.pointer = self.eat_punct("*");
        let name = self.ident()?;
        self.expect_punct("(")?;
        let mut params = Vec::new();
        if !self.eat_punct(")") {
            loop {
                if self.is_ident("void") && matches!(self.peek_at(1), Some(TokenKind::Punct(")"))) {
                    self.position += 1;
                    self.expect_punct(")")?;
                    break;
                }
                let mut ty = self.base_type()?;
                ty.pointer = self.eat_punct("*");
                let name = self.ident()?;
                params.push(Param { ty, name });
                if self.eat_punct(")") {
                    break;
                }
                self.expect_punct(",")?;
            }
        }
        if self.eat_punct(";") {
            return Ok(Function {
                return_type,
                name,
                params,
                body: Vec::new(),
            });
        }
        self.expect_punct("{")?;
        let body = self.block_body()?;
        Ok(Function {
            return_type,
            name,
            params,
            body,
        })
    }

    fn block_body(&mut self) -> Result<Vec<Stmt>> {
        let mut statements = Vec::new();
        while !self.eat_punct("}") {
            if self.at_end() {
                return self.error("unterminated block");
            }
            statements.push(self.statement()?);
        }
        Ok(statements)
    }

    fn statement(&mut self) -> Result<Stmt> {
        if self.eat_punct(";") {
            return Ok(Stmt::Empty);
        }
        if self.eat_punct("{") {
            return Ok(Stmt::Block(self.block_body()?));
        }
        match self.word_at(0) {
            Some("if") => {
                self.position += 1;
                self.expect_punct("(")?;
                let cond = self.expression()?;
                self.expect_punct(")")?;
                let then = Box::new(self.statement()?);
                let otherwise = if self.is_ident("else") {
                    self.position += 1;
                    Some(Box::new(self.statement()?))
                } else {
                    None
                };
                return Ok(Stmt::If {
                    cond,
                    then,
                    otherwise,
                });
            }
            Some("for") => {
                self.position += 1;
                self.expect_punct("(")?;
                let init = if self.eat_punct(";") {
                    None
                } else if self.declaration_starts() {
                    Some(Box::new(self.declaration()?))
                } else {
                    let expr = self.expression()?;
                    self.expect_punct(";")?;
                    Some(Box::new(Stmt::Expr(expr)))
                };
                let cond = if self.is_punct(";") {
                    None
                } else {
                    Some(self.expression()?)
                };
                self.expect_punct(";")?;
                let step = if self.is_punct(")") {
                    None
                } else {
                    Some(self.expression()?)
                };
                self.expect_punct(")")?;
                let body = Box::new(self.statement()?);
                return Ok(Stmt::For {
                    init,
                    cond,
                    step,
                    body,
                });
            }
            Some("while") => {
                self.position += 1;
                self.expect_punct("(")?;
                let cond = self.expression()?;
                self.expect_punct(")")?;
                let body = Box::new(self.statement()?);
                return Ok(Stmt::While { cond, body });
            }
            Some("do") => {
                self.position += 1;
                let body = Box::new(self.statement()?);
                if !self.is_ident("while") {
                    return self.error("expected `while` after do body");
                }
                self.position += 1;
                self.expect_punct("(")?;
                let cond = self.expression()?;
                self.expect_punct(")")?;
                self.expect_punct(";")?;
                return Ok(Stmt::DoWhile { body, cond });
            }
            Some("switch") => {
                self.position += 1;
                self.expect_punct("(")?;
                let value = self.expression()?;
                self.expect_punct(")")?;
                self.expect_punct("{")?;
                return Ok(Stmt::Switch {
                    value,
                    cases: self.switch_cases()?,
                });
            }
            Some("break") => {
                self.position += 1;
                self.expect_punct(";")?;
                return Ok(Stmt::Break);
            }
            Some("continue") => {
                self.position += 1;
                self.expect_punct(";")?;
                return Ok(Stmt::Continue);
            }
            Some("return") => {
                self.position += 1;
                let value = if self.is_punct(";") {
                    None
                } else {
                    Some(self.expression()?)
                };
                self.expect_punct(";")?;
                return Ok(Stmt::Return(value));
            }
            _ => {}
        }
        if self.declaration_starts() {
            return self.declaration();
        }
        let expr = self.expression()?;
        self.expect_punct(";")?;
        Ok(Stmt::Expr(expr))
    }

    fn switch_cases(&mut self) -> Result<Vec<Case>> {
        let mut cases: Vec<Case> = Vec::new();
        loop {
            if self.eat_punct("}") {
                return Ok(cases);
            }
            let mut labels = Vec::new();
            loop {
                if self.is_ident("case") {
                    self.position += 1;
                    labels.push(Some(self.conditional()?));
                    self.expect_punct(":")?;
                } else if self.is_ident("default") {
                    self.position += 1;
                    labels.push(None);
                    self.expect_punct(":")?;
                } else {
                    break;
                }
            }
            if labels.is_empty() {
                return self.error("expected `case` or `default` in switch");
            }
            let mut body = Vec::new();
            while !self.is_ident("case") && !self.is_ident("default") && !self.is_punct("}") {
                body.push(self.statement()?);
            }
            cases.push(Case { labels, body });
        }
    }

    fn declaration(&mut self) -> Result<Stmt> {
        let ty = self.base_type()?;
        let mut vars = Vec::new();
        loop {
            let pointer = self.eat_punct("*");
            let name = self.ident()?;
            let array = if self.eat_punct("[") {
                let size = self.expression()?;
                self.expect_punct("]")?;
                Some(size)
            } else {
                None
            };
            let init = if !self.eat_punct("=") {
                None
            } else if self.eat_punct("{") {
                Some(Expr::InitList(self.elements_until_brace()?))
            } else {
                Some(self.assignment()?)
            };
            vars.push(Declarator {
                name,
                pointer,
                array,
                init,
            });
            if self.eat_punct(";") {
                return Ok(Stmt::Decl { ty, vars });
            }
            self.expect_punct(",")?;
        }
    }

    fn expression(&mut self) -> Result<Expr> {
        // The comma operator never appears outside `for` headers in these
        // sources; parse it as a sequence of assignments keeping the last.
        let mut expr = self.assignment()?;
        while self.is_punct(",") {
            self.position += 1;
            let next = self.assignment()?;
            expr = Expr::Call {
                name: "__comma".into(),
                args: vec![expr, next],
            };
        }
        Ok(expr)
    }

    fn assignment(&mut self) -> Result<Expr> {
        let target = self.conditional()?;
        let op = match self.peek() {
            Some(TokenKind::Punct("=")) => None,
            Some(TokenKind::Punct(p)) => match *p {
                "+=" => Some(BinaryOp::Add),
                "-=" => Some(BinaryOp::Sub),
                "*=" => Some(BinaryOp::Mul),
                "/=" => Some(BinaryOp::Div),
                "%=" => Some(BinaryOp::Rem),
                "&=" => Some(BinaryOp::BitAnd),
                "|=" => Some(BinaryOp::BitOr),
                "^=" => Some(BinaryOp::BitXor),
                "<<=" => Some(BinaryOp::Shl),
                ">>=" => Some(BinaryOp::Shr),
                _ => return Ok(target),
            },
            _ => return Ok(target),
        };
        self.position += 1;
        let value = self.assignment()?;
        Ok(Expr::Assign {
            op,
            target: Box::new(target),
            value: Box::new(value),
        })
    }

    fn conditional(&mut self) -> Result<Expr> {
        let cond = self.binary(0)?;
        if !self.eat_punct("?") {
            return Ok(cond);
        }
        let then = self.assignment()?;
        self.expect_punct(":")?;
        let otherwise = self.conditional()?;
        Ok(Expr::Ternary {
            cond: Box::new(cond),
            then: Box::new(then),
            otherwise: Box::new(otherwise),
        })
    }

    fn binary(&mut self, min_precedence: u8) -> Result<Expr> {
        let mut lhs = self.unary()?;
        loop {
            let Some((op, precedence)) = self.binary_operator() else {
                return Ok(lhs);
            };
            if precedence < min_precedence {
                return Ok(lhs);
            }
            self.position += 1;
            let rhs = self.binary(precedence + 1)?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
    }

    fn binary_operator(&self) -> Option<(BinaryOp, u8)> {
        let Some(TokenKind::Punct(p)) = self.peek() else {
            return None;
        };
        Some(match *p {
            "||" => (BinaryOp::Or, 1),
            "&&" => (BinaryOp::And, 2),
            "|" => (BinaryOp::BitOr, 3),
            "^" => (BinaryOp::BitXor, 4),
            "&" => (BinaryOp::BitAnd, 5),
            "==" => (BinaryOp::Eq, 6),
            "!=" => (BinaryOp::Ne, 6),
            "<" => (BinaryOp::Lt, 7),
            "<=" => (BinaryOp::Le, 7),
            ">" => (BinaryOp::Gt, 7),
            ">=" => (BinaryOp::Ge, 7),
            "<<" => (BinaryOp::Shl, 8),
            ">>" => (BinaryOp::Shr, 8),
            "+" => (BinaryOp::Add, 9),
            "-" => (BinaryOp::Sub, 9),
            "*" => (BinaryOp::Mul, 10),
            "/" => (BinaryOp::Div, 10),
            "%" => (BinaryOp::Rem, 10),
            _ => return None,
        })
    }

    fn unary(&mut self) -> Result<Expr> {
        let op = match self.peek() {
            Some(TokenKind::Punct("-")) => Some(UnaryOp::Neg),
            Some(TokenKind::Punct("+")) => Some(UnaryOp::Plus),
            Some(TokenKind::Punct("!")) => Some(UnaryOp::Not),
            Some(TokenKind::Punct("~")) => Some(UnaryOp::BitNot),
            Some(TokenKind::Punct("*")) => Some(UnaryOp::Deref),
            Some(TokenKind::Punct("&")) => Some(UnaryOp::AddressOf),
            Some(TokenKind::Punct("++")) => Some(UnaryOp::PreIncrement),
            Some(TokenKind::Punct("--")) => Some(UnaryOp::PreDecrement),
            _ => None,
        };
        if let Some(op) = op {
            self.position += 1;
            let expr = self.unary()?;
            return Ok(Expr::Unary {
                op,
                expr: Box::new(expr),
            });
        }
        if self.is_punct("(") && self.type_starts_at(1) {
            self.position += 1;
            let mut ty = self.base_type()?;
            ty.pointer = self.eat_punct("*");
            self.expect_punct(")")?;
            if self.eat_punct("{") {
                let elements = self.elements_until_brace()?;
                return self.postfix(Expr::VectorLiteral { ty, elements });
            }
            let expr = self.unary()?;
            return Ok(Expr::Cast {
                ty,
                expr: Box::new(expr),
            });
        }
        let primary = self.primary()?;
        self.postfix(primary)
    }

    /// Comma-separated elements after an opening `{`, through the closing `}`.
    fn elements_until_brace(&mut self) -> Result<Vec<Expr>> {
        let mut elements = Vec::new();
        if self.eat_punct("}") {
            return Ok(elements);
        }
        loop {
            elements.push(self.assignment()?);
            if self.eat_punct("}") {
                return Ok(elements);
            }
            self.expect_punct(",")?;
        }
    }

    fn postfix(&mut self, mut expr: Expr) -> Result<Expr> {
        loop {
            if self.eat_punct("[") {
                let index = self.expression()?;
                self.expect_punct("]")?;
                expr = Expr::Index {
                    base: Box::new(expr),
                    index: Box::new(index),
                };
            } else if self.is_punct(".") || self.is_punct("->") {
                let arrow = self.is_punct("->");
                self.position += 1;
                let field = self.ident()?;
                expr = Expr::Member {
                    base: Box::new(expr),
                    field,
                    arrow,
                };
            } else if self.eat_punct("++") {
                expr = Expr::Unary {
                    op: UnaryOp::PostIncrement,
                    expr: Box::new(expr),
                };
            } else if self.eat_punct("--") {
                expr = Expr::Unary {
                    op: UnaryOp::PostDecrement,
                    expr: Box::new(expr),
                };
            } else {
                return Ok(expr);
            }
        }
    }

    fn primary(&mut self) -> Result<Expr> {
        match self.peek().cloned() {
            Some(TokenKind::Number { value, float }) => {
                self.position += 1;
                Ok(Expr::Number { value, float })
            }
            Some(TokenKind::Ident(name)) => {
                self.position += 1;
                if self.eat_punct("(") {
                    let mut args = Vec::new();
                    if !self.eat_punct(")") {
                        loop {
                            args.push(self.assignment()?);
                            if self.eat_punct(")") {
                                break;
                            }
                            self.expect_punct(",")?;
                        }
                    }
                    Ok(Expr::Call { name, args })
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            Some(TokenKind::Punct("(")) => {
                self.position += 1;
                let expr = self.expression()?;
                self.expect_punct(")")?;
                Ok(expr)
            }
            other => self.error(format!("unexpected {other:?} in expression")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANDELBULB: &str = r#"
REAL4 MandelbulbIteration(REAL4 z, __constant sFractalCl *fractal, sExtendedAuxCl *aux)
{
	REAL th0 = asin(z.z / aux->r) + fractal->bulb.betaAngleOffset;
	REAL rp = pow(aux->r, fractal->bulb.power - 1.0f);
	aux->DE = (rp * aux->DE) * fractal->bulb.power + 1.0f;
	rp *= aux->r;
	z.x = native_cos(th0) * rp;
	return z;
}
"#;

    #[test]
    fn mandelbulb_parses_into_one_function() {
        let file = parse_formula_file(MANDELBULB).unwrap();
        assert_eq!(file.functions.len(), 1);
        let function = &file.functions[0];
        assert_eq!(function.name, "MandelbulbIteration");
        assert_eq!(function.params.len(), 3);
        assert!(function.params[1].ty.constant && function.params[1].ty.pointer);
        assert_eq!(function.body.len(), 6);
        let Stmt::Expr(Expr::Assign {
            op: Some(BinaryOp::Mul),
            ..
        }) = &function.body[3]
        else {
            panic!("expected compound assignment, got {:?}", function.body[3]);
        };
    }

    #[test]
    fn precedence_and_casts_and_vector_literals() {
        let source = "REAL4 F(REAL4 z) { REAL a = 1.0f + 2.0f * (REAL)z.x; z = (REAL4){a, a, -a, 0.0f}; return a > 0.0f ? z : -z; }";
        let function = &parse_formula_file(source).unwrap().functions[0];
        let Stmt::Decl { vars, .. } = &function.body[0] else {
            panic!()
        };
        let Some(Expr::Binary {
            op: BinaryOp::Add,
            rhs,
            ..
        }) = &vars[0].init
        else {
            panic!()
        };
        assert!(matches!(
            **rhs,
            Expr::Binary {
                op: BinaryOp::Mul,
                ..
            }
        ));
        let Stmt::Expr(Expr::Assign { value, .. }) = &function.body[1] else {
            panic!()
        };
        assert!(matches!(&**value, Expr::VectorLiteral { elements, .. } if elements.len() == 4));
        assert!(matches!(
            function.body[2],
            Stmt::Return(Some(Expr::Ternary { .. }))
        ));
    }

    #[test]
    fn switch_groups_stacked_labels() {
        let source =
            "void F(int o) { switch (o) { case A: default: x = 1; break; case B: x = 2; break; } }";
        let function = &parse_formula_file(source).unwrap().functions[0];
        let Stmt::Switch { cases, .. } = &function.body[0] else {
            panic!()
        };
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].labels.len(), 2);
        assert_eq!(cases[0].labels[1], None);
    }

    #[test]
    fn array_declarations_take_initialiser_lists() {
        let source = "void F() { bool on[3] = {a, b, c}; }";
        let function = &parse_formula_file(source).unwrap().functions[0];
        let Stmt::Decl { vars, .. } = &function.body[0] else {
            panic!()
        };
        assert!(vars[0].array.is_some());
        assert!(matches!(&vars[0].init, Some(Expr::InitList(items)) if items.len() == 3));
    }
}
