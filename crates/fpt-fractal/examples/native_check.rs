//! Compares the f64 interpreter with Mandelbulber's own distances.
//!
//! Usage: native_check MANDELBULBER_ROOT SCENE.fract POINTS
//! POINTS has one sample per line: `x y z detail_size native_distance`
//! (native coordinates), e.g. from native_probe march output.

#[path = "common/loader.rs"]
mod loader;

use std::{env, path::Path};

fn main() {
    let args: Vec<String> = env::args().collect();
    let [_, root, scene, points] = args.as_slice() else {
        panic!("usage: native_check MANDELBULBER_ROOT SCENE.fract POINTS");
    };
    let fractal = loader::load_fractal(Path::new(root), scene);
    println!(
        "formula {}, delta {}, bailout {}, N {}, add_c {}",
        fractal.meta.internal_name,
        fractal.delta,
        fractal.bailout,
        fractal.max_iterations,
        fractal.add_c
    );
    let samples = loader::read_points(points);
    let pairs = samples
        .iter()
        .map(|s| (fractal.distance([s[0], s[1], s[2]], s[3]).unwrap(), s[4]));
    match loader::relative_errors(pairs) {
        Some((median, p99, max, n)) => {
            println!(
                "{n} samples: relative error median {median:.2e}, p99 {p99:.2e}, max {max:.2e}"
            )
        }
        None => println!("no samples"),
    }
}
