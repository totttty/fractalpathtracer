//! A small process adapter for non-Rust hosts. All production work is performed
//! by the linked library; the host does not shell out to the legacy FPT CLI.
use anyhow::{Context, Result, bail};
use fpt_metal::runtime::{BundleRequest, ExportRequest, RenderRequest, Runtime};
use std::{env, fs};

fn main() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 3 {
        bail!("usage: scene_worker render|export|bundle request.json receipt.json");
    }
    if std::path::Path::new(&args[2]).exists() {
        bail!("receipt already exists");
    }
    let bytes = fs::read(&args[1]).context("read typed request")?;
    let runtime = Runtime;
    let result = match args[0].as_str() {
        "render" => {
            let request: RenderRequest = serde_json::from_slice(&bytes)?;
            serde_json::to_value(runtime.render(runtime.prepare(&request)?)?)?
        }
        "export" => serde_json::to_value(
            runtime.export(&serde_json::from_slice::<ExportRequest>(&bytes)?)?,
        )?,
        "bundle" => {
            serde_json::json!({"manifest":runtime.build_bundle(&serde_json::from_slice::<BundleRequest>(&bytes)?)?})
        }
        _ => bail!("unknown operation"),
    };
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])?;
    serde_json::to_writer_pretty(&mut file, &result)?;
    Ok(())
}
