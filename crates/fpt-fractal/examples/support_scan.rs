//! Which scenes the library can evaluate (f64) and turn into WGSL.
//!
//! Usage: support_scan MANDELBULBER_ROOT EXAMPLES_DIR SCENES.tsv
//! SCENES.tsv: `id width height relative_path` per line. Prints
//! `id status detail` per scene: `gpu`, `cpu` (WGSL failed), `hybrid`,
//! `boolean` or `error`.

#[path = "common/loader.rs"]
mod loader;

use std::{env, panic, path::Path};

use fpt_fractal::scene::FractFile;

fn classify(root: &Path, scene: &Path) -> (&'static str, String) {
    let file = FractFile::parse(&loader::read(scene));
    if file.boolean("boolean_operators", false) {
        return ("boolean", String::new());
    }
    let loaded = panic::catch_unwind(|| loader::load_fractal(root, scene));
    let fractal = match loaded {
        Ok(fractal) => fractal,
        Err(e) => return ("error", panic_text(e)),
    };
    let name = fractal
        .formulas
        .iter()
        .map(|f| f.meta.internal_name.as_str())
        .collect::<Vec<_>>()
        .join("+");
    let mut detail = name.clone();
    if fractal.delta {
        detail.push_str(" delta");
    }
    if let Err(e) = fractal.distance([0.1, 0.2, 0.3], 1e-4) {
        return ("error", format!("{detail}: eval {e}"));
    }
    match fpt_fractal::wgsl::emit(&fractal) {
        Ok(shader) => match validate(&shader.source) {
            Ok(()) => ("gpu", detail),
            Err(e) => ("cpu", format!("{detail}: invalid WGSL {e}")),
        },
        Err(e) => ("cpu", format!("{detail}: {e}")),
    }
}

fn validate(source: &str) -> Result<(), String> {
    let source = format!(
        "@group(0) @binding(0) {}\n{source}",
        fpt_fractal::wgsl::PARAMS_DECLARATION
    );
    let module = naga::front::wgsl::parse_str(&source).map_err(|e| e.message().to_owned())?;
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .map(|_| ())
    .map_err(|e| format!("{:?}", e.into_inner()).chars().take(200).collect())
}

fn panic_text(e: Box<dyn std::any::Any + Send>) -> String {
    e.downcast_ref::<String>()
        .cloned()
        .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_default()
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let [_, root, examples, list] = args.as_slice() else {
        panic!("usage: support_scan MANDELBULBER_ROOT EXAMPLES_DIR SCENES.tsv");
    };
    panic::set_hook(Box::new(|_| {}));
    for line in loader::read(list).lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        let [id, _, _, relative] = fields.as_slice() else {
            continue;
        };
        let (status, detail) = classify(Path::new(root), &Path::new(examples).join(relative));
        println!("{id}\t{status}\t{}", detail.replace(['\n', '\t'], " "));
    }
}
