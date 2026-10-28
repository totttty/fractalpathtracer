//! Prints the slot-1 parameter values a scene resolves to, one per line.
//!
//! Usage: params_dump MANDELBULBER_ROOT SCENE.fract [FILTER]

#[path = "common/loader.rs"]
mod loader;

use std::{env, path::Path};

fn main() {
    let args: Vec<String> = env::args().collect();
    let [_, root, scene, rest @ ..] = args.as_slice() else {
        panic!("usage: params_dump MANDELBULBER_ROOT SCENE.fract [FILTER]");
    };
    let fractal = loader::load_fractal(Path::new(root), scene);
    let mut values: Vec<_> = fractal.primary().parameters.values.iter().collect();
    values.sort_by(|a, b| a.0.cmp(b.0));
    for (path, value) in values {
        if rest
            .first()
            .is_none_or(|filter| path.contains(filter.as_str()))
        {
            println!("{path} = {value:?}");
        }
    }
}
