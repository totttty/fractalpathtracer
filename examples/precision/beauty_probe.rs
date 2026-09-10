//! Offline, isolated use of the existing path-tracing bridge for derivative A/B.
use anyhow::{Result, ensure};
use fpt_metal::ffi::*;
use std::{
    ffi::{CStr, CString},
    fs,
    path::Path,
    process::Command,
};

pub fn render(source: &str, cfg: &FptRenderConfig, out: &Path) -> Result<f64> {
    ensure!(
        cfg.preview == 0 && cfg.sdf_accumulation_mode == SDF_ACCUMULATION_CHUNKED,
        "beauty probe requires offline chunked accumulation"
    );
    ensure!(
        cfg.sdf_function_stitching == 0 && cfg.sdf_profile == 0,
        "beauty probe does not support stitching or profiling"
    );
    let metal = out.join("probe.metal");
    let air = out.join("probe.air");
    let lib = out.join("probe.metallib");
    fs::write(&metal, source)?;
    for (label, mut command) in [
        ("compile", {
            let mut c = Command::new("xcrun");
            c.args([
                "-sdk",
                "macosx",
                "metal",
                "-std=macos-metal2.4",
                "-ffast-math",
                "-c",
            ])
            .arg(&metal)
            .arg("-o")
            .arg(&air);
            c
        }),
        ("link", {
            let mut c = Command::new("xcrun");
            c.args(["-sdk", "macosx", "metallib"])
                .arg(&air)
                .arg("-o")
                .arg(&lib);
            c
        }),
    ] {
        fs::write(
            out.join(format!("{label}-command.txt")),
            format!("{command:?}\n"),
        )?;
        let result = command.output()?;
        fs::write(out.join(format!("{label}-stderr.log")), &result.stderr)?;
        ensure!(
            result.status.success(),
            "{label} failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let library = CString::new(lib.to_str().unwrap())?;
    let image = CString::new(out.join("render.png").to_str().unwrap())?;
    let mut error = [0i8; 4096];
    let mut elapsed = 0.0;
    let mut linear = vec![0.0f32; cfg.width as usize * cfg.height as usize * 4];
    let status = unsafe {
        fpt_metal_render(
            library.as_ptr(),
            c"".as_ptr(),
            c"".as_ptr(),
            image.as_ptr(),
            std::ptr::null(),
            0,
            cfg,
            std::ptr::null_mut(),
            &mut elapsed,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            linear.as_mut_ptr(),
            linear.len(),
            error.as_mut_ptr(),
            error.len(),
        )
    };
    ensure!(
        status == 0,
        "{}",
        unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy()
    );
    ensure!(
        linear.iter().all(|v| v.is_finite()),
        "non-finite linear radiance"
    );
    fs::write(
        out.join("linear.f32"),
        linear
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
    )?;
    Ok(elapsed)
}
