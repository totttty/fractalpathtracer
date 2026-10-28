//! Prints the orbit and distance at one point of a scene.
//!
//! Usage: orbit_dump MANDELBULBER_ROOT SCENE.fract X Y Z DETAIL_SIZE

#[path = "common/loader.rs"]
mod loader;

use std::{env, path::Path};

fn main() {
    let args: Vec<String> = env::args().collect();
    let [_, root, scene, x, y, z, detail] = args.as_slice() else {
        panic!("usage: orbit_dump MANDELBULBER_ROOT SCENE.fract X Y Z DETAIL_SIZE");
    };
    let number = |s: &String| s.parse::<f64>().expect("number");
    let point = [number(x), number(y), number(z)];
    let shape = loader::load_shape(Path::new(root), scene);
    for fractal in shape.fractals() {
        let names: Vec<_> = fractal
            .formulas
            .iter()
            .map(|f| f.meta.internal_name.as_str())
            .collect();
        println!(
            "{names:?}: delta {}, DE function {:?}, analytic {:?}, bailout {}, N {}",
            fractal.delta,
            fractal.de_function,
            fractal.primary().meta.analytic_function,
            fractal.bailout,
            fractal.max_iterations
        );
        println!("  orbit {:?}", fractal.orbit(point, None));
        println!("  distance {:?}", fractal.distance(point, number(detail)));
    }
    println!("shape distance {:?}", shape.distance(point, number(detail)));
}
