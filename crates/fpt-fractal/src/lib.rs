//! Mandelbulber fractal formulas as a portable intermediate representation.
//!
//! Mandelbulber ships its iteration formulas as OpenCL C (`formula/opencl`).
//! This crate parses that C subset into a syntax tree, so the same formula can
//! be evaluated on the CPU in f64 (a reference matching Mandelbulber's own
//! double-precision path) and emitted as GPU shader code.
//!
//! The crate reads no files: callers pass formula source text. Mandelbulber's
//! formulas are GPL v3 and are read from the user's own installation.

#![forbid(unsafe_code)]

pub mod ast;
pub mod boolean;
pub mod colouring;
pub mod constants;
pub mod displacement;
pub mod eval;
mod lex;
pub mod lights;
pub mod orbit;
pub mod params;
mod parse;
pub mod primitives;
pub mod scene;
pub mod wgsl;

pub use parse::{ParseError, parse_formula_file};
