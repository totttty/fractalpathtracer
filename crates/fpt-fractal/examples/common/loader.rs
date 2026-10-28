//! Loads a single-formula scene from a Mandelbulber checkout (examples only).

use std::{fs, path::Path};

use fpt_fractal::orbit::Fractal;
use fpt_fractal::params::ParameterSchema;
use fpt_fractal::scene::{FormulaMeta, FractFile};

pub fn read(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The definition file whose `internalID = fractal::<enum>` has `id`.
pub fn formula_files(root: &Path, id: i64) -> (String, String) {
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

/// The scene's slot-1 fractal, ready to evaluate.
pub fn load_fractal(root: &Path, scene: impl AsRef<Path>) -> Fractal {
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
    Fractal::from_scene(&file, function, meta, parameters)
}

/// `x y z detail_size native_distance` samples.
pub fn read_points(path: impl AsRef<Path>) -> Vec<[f64; 5]> {
    read(path)
        .lines()
        .filter_map(|line| {
            let v: Vec<f64> = line
                .split_whitespace()
                .filter_map(|w| w.parse().ok())
                .collect();
            (v.len() >= 5).then(|| [v[0], v[1], v[2], v[3], v[4]])
        })
        .collect()
}

/// Median, 99th percentile and maximum of |a/b - 1| over pairs with b > 0.
pub fn relative_errors(
    pairs: impl IntoIterator<Item = (f64, f64)>,
) -> Option<(f64, f64, f64, usize)> {
    let mut errors: Vec<f64> = pairs
        .into_iter()
        .filter(|(_, b)| *b > 0.0)
        .map(|(a, b)| (a / b - 1.0).abs())
        .collect();
    errors.sort_by(f64::total_cmp);
    let n = errors.len();
    (n > 0).then(|| {
        (
            errors[n / 2],
            errors[(n * 99 / 100).min(n - 1)],
            errors[n - 1],
            n,
        )
    })
}
