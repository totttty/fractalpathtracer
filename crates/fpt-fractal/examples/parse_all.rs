//! Parses every `*.cl` formula in a directory and reports coverage.
//! Usage: cargo run --example parse_all -- /path/to/mandelbulber2/formula/opencl

use std::{env, fs};

fn main() {
    let directory = env::args()
        .nth(1)
        .expect("usage: parse_all FORMULA_DIRECTORY");
    let mut paths: Vec<_> = fs::read_dir(&directory)
        .expect("read formula directory")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|e| e == "cl"))
        .collect();
    paths.sort();
    let mut failures = Vec::new();
    for path in &paths {
        let source = fs::read_to_string(path).expect("read formula");
        if let Err(error) = fpt_fractal::parse_formula_file(&source) {
            failures.push(format!(
                "{}: {error}",
                path.file_name().unwrap().to_string_lossy()
            ));
        }
    }
    println!(
        "parsed {}/{} formula files",
        paths.len() - failures.len(),
        paths.len()
    );
    for failure in &failures {
        println!("  {failure}");
    }
}
