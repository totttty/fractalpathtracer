//! Which scenes the library can evaluate (f64) and turn into WGSL.
//!
//! Usage: support_scan MANDELBULBER_ROOT EXAMPLES_DIR SCENES.tsv
//! SCENES.tsv: `id width height relative_path` per line. Prints
//! `id status detail` per scene: `gpu`, `cpu` (WGSL failed), `hybrid`,
//! `boolean` or `error`. `SUPPORT_SCAN_DEEP=1` emits single fractals as deep
//! zooms; `SUPPORT_SCAN_COLOUR=1` adds the colouring orbit; `SUPPORT_SCAN_DUMP=dir` writes each module and its
//! parameters there.

#[path = "common/loader.rs"]
mod loader;

use std::{env, panic, path::Path};

use fpt_fractal::boolean::Shape;

fn classify(id: &str, root: &Path, scene: &Path) -> (&'static str, String) {
    let loaded = panic::catch_unwind(|| loader::load_shape(root, scene));
    let shape = match loaded {
        Ok(shape) => shape,
        Err(e) => return ("error", panic_text(e)),
    };
    let mut detail = shape
        .fractals()
        .iter()
        .map(|f| {
            f.formulas
                .iter()
                .map(|f| f.meta.internal_name.as_str())
                .collect::<Vec<_>>()
                .join("+")
        })
        .collect::<Vec<_>>()
        .join(" | ");
    if shape.fractals().iter().any(|f| f.delta) {
        detail.push_str(" delta");
    }
    if matches!(shape, Shape::Boolean(_)) {
        detail.push_str(" boolean");
    }
    if let Err(e) = shape.distance([0.1, 0.2, 0.3], 1e-4) {
        return ("error", format!("{detail}: eval {e}"));
    }
    // SUPPORT_SCAN_COLOUR=1 adds the colouring orbit of the scene's fractal material (each Boolean component's).
    let file = fpt_fractal::scene::FractFile::parse(&loader::read(scene));
    let colouring_of = |key: &str| {
        let id = file.number(key, 1.0).max(1.0) as i64;
        fpt_fractal::colouring::material(&file, id).colouring
    };
    let colouring = env::var_os("SUPPORT_SCAN_COLOUR").map(|_| colouring_of("formula_material_id"));
    let deep = env::var_os("SUPPORT_SCAN_DEEP").map(|_| [0.1, 0.2, 0.3]);
    let emitted = match (&shape, &colouring, deep) {
        (Shape::Single(fractal), Some(colouring), origin) => {
            fpt_fractal::wgsl::emit_coloured(fractal, origin, colouring)
        }
        (Shape::Boolean(boolean), Some(_), _) => {
            let colourings: Vec<_> = boolean
                .components
                .iter()
                .map(|c| colouring_of(&format!("formula_material_id_{}", c.slot)))
                .collect();
            fpt_fractal::wgsl::emit_boolean_coloured(boolean, &colourings)
        }
        (Shape::Single(fractal), None, Some(origin)) => {
            fpt_fractal::wgsl::emit_deep(fractal, origin)
        }
        _ => fpt_fractal::wgsl::emit_shape(&shape),
    };
    if let (Some(dir), Ok(shader)) = (env::var_os("SUPPORT_SCAN_DUMP"), &emitted) {
        let dir = Path::new(&dir);
        let _ = std::fs::write(dir.join(format!("{id}.wgsl")), &shader.source);
        let _ = std::fs::write(
            dir.join(format!("{id}.params")),
            format!("{:?}", shader.parameters),
        );
    }
    match emitted {
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
        let (status, detail) = classify(id, Path::new(root), &Path::new(examples).join(relative));
        println!("{id}\t{status}\t{}", detail.replace(['\n', '\t'], " "));
    }
}
