//! How far f32 arithmetic moves each scene's distance estimate: f64 against `with_f32_rounding` (inputs rounded too)
//! at the scene's sample points.
//!
//! Usage: precision_scan MANDELBULBER_ROOT EXAMPLES_DIR SCENES.tsv POINTS_DIR
//! SCENES.tsv: `id width height relative_path`; POINTS_DIR holds `pts-<id>-np.txt` (`x y z detail distance`).
//! Prints `id points median p90 p99 share>1e-3 share>1e-2` of the relative difference.

#[path = "common/loader.rs"]
mod loader;

use std::{env, panic, path::Path};

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "f32 rounding and point counts"
)]
fn main() {
    let args: Vec<String> = env::args().collect();
    let [_, root, examples, list, points] = args.as_slice() else {
        panic!("usage: precision_scan MANDELBULBER_ROOT EXAMPLES_DIR SCENES.tsv POINTS_DIR");
    };
    panic::set_hook(Box::new(|_| {}));
    for line in loader::read(list).lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        let [id, _, _, relative] = fields.as_slice() else {
            continue;
        };
        let path = Path::new(points).join(format!("pts-{id}-np.txt"));
        if !path.exists() {
            continue;
        }
        let scene = Path::new(examples).join(relative);
        let Ok(shape) = panic::catch_unwind(|| loader::load_shape(Path::new(root), &scene)) else {
            continue;
        };
        let mut errors: Vec<f64> = loader::read_points(&path)
            .iter()
            .filter_map(|s| {
                let exact = shape.distance([s[0], s[1], s[2]], s[3]).ok()?;
                let p = [s[0], s[1], s[2]].map(|x| f64::from(x as f32));
                let rounded = fpt_fractal::eval::with_f32_rounding(|| {
                    shape.distance(p, f64::from(s[3] as f32))
                })
                .ok()?;
                let error = (rounded - exact).abs() / exact.abs().max(1e-30);
                // PRECISION_SCAN_POINTS=id prints that scene's points: distance, detail size, error.
                if env::var("PRECISION_SCAN_POINTS").as_deref() == Ok(*id) {
                    eprintln!("{exact:.6e}\t{:.3e}\t{error:.3e}", s[3]);
                }
                Some(error)
            })
            .collect();
        if errors.is_empty() {
            continue;
        }
        errors.sort_by(f64::total_cmp);
        let at = |q: usize| errors[(errors.len() - 1) * q / 100];
        let share = |t: f64| errors.iter().filter(|e| **e > t).count() as f64 / errors.len() as f64;
        println!(
            "{id}\t{}\t{:.2e}\t{:.2e}\t{:.2e}\t{:.4}\t{:.4}",
            errors.len(),
            at(50),
            at(90),
            at(99),
            share(1e-3),
            share(1e-2)
        );
    }
}
