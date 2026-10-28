//! Compares the f64 interpreter with Mandelbulber's own distances.
//!
//! Usage: native_check MANDELBULBER_ROOT SCENE.fract POINTS
//! POINTS has one sample per line: `x y z detail_size native_distance`
//! (native coordinates), e.g. from native_probe march output.

use std::{env, fs, path::Path};

use fpt_fractal::orbit::Fractal;
use fpt_fractal::params::ParameterSchema;
use fpt_fractal::scene::{FormulaMeta, FractFile};

fn read(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The definition file whose `internalID = fractal::<enum>` has `id`.
fn formula_files(root: &Path, id: i64) -> (String, String) {
    let enums = read(root.join("formula/definition/all_fractal_list_enums.hpp"));
    let enum_name = enums
        .lines()
        .find_map(|line| {
            let (name, value) = line.trim().trim_end_matches(',').split_once('=')?;
            (value.trim().parse::<i64>().ok()? == id).then(|| name.trim().to_owned())
        })
        .unwrap_or_else(|| panic!("no formula with id {id}"));
    let definitions = root.join("formula/definition");
    for entry in fs::read_dir(&definitions).unwrap().flatten() {
        let text = read(entry.path());
        if text.contains(&format!("internalID = fractal::{enum_name};")) {
            let meta = FormulaMeta::parse(&text).unwrap();
            let cl = read(root.join(format!("formula/opencl/{}.cl", meta.internal_name)));
            return (text, cl);
        }
    }
    panic!("no definition for fractal::{enum_name}");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let [_, root, scene, points] = args.as_slice() else {
        panic!("usage: native_check MANDELBULBER_ROOT SCENE.fract POINTS");
    };
    let root = Path::new(root);
    let file = FractFile::parse(&read(scene));
    let id = file.number("formula_1", 2.0) as i64;
    let (definition, cl) = formula_files(root, id);
    let meta = FormulaMeta::parse(&definition).unwrap();
    let formula = fpt_fractal::parse_formula_file(&cl).unwrap();
    let function = formula
        .functions
        .into_iter()
        .find(|f| f.name.ends_with("Iteration"))
        .expect("iteration function");
    let schema = ParameterSchema::parse(
        &read(root.join("src/initparameters.cpp")),
        &read(root.join("src/fractal.cpp")),
    )
    .unwrap();
    let parameters = schema.parameters(&file.fractal_settings(1)).unwrap();
    let fractal = Fractal::from_scene(&file, function, meta.clone(), parameters);
    println!(
        "formula {} ({}), delta {}, bailout {}, N {}, add_c {}",
        meta.internal_name,
        id,
        fractal.delta,
        fractal.bailout,
        fractal.max_iterations,
        fractal.add_c
    );
    let mut errors = Vec::new();
    for line in read(points).lines() {
        let v: Vec<f64> = line
            .split_whitespace()
            .filter_map(|w| w.parse().ok())
            .collect();
        if v.len() < 5 {
            continue;
        }
        let ours = fractal.distance([v[0], v[1], v[2]], v[3]).unwrap();
        if v[4] > 0.0 {
            errors.push((ours / v[4] - 1.0).abs());
        }
    }
    errors.sort_by(f64::total_cmp);
    if errors.is_empty() {
        println!("no samples");
        return;
    }
    let median = errors[errors.len() / 2];
    let p99 = errors[(errors.len() * 99 / 100).min(errors.len() - 1)];
    println!(
        "{} samples: relative error median {median:.2e}, p99 {p99:.2e}, max {:.2e}",
        errors.len(),
        errors[errors.len() - 1]
    );
}
