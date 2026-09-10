//! Diagnostic sampling control. Production renderer and sampling defaults are unchanged.
use anyhow::{Context, Result, ensure};
use fpt_metal::{ffi::*, mandelbulber::compiler::retain_metal_kernels, scene};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{ffi::CStr, fs, path::Path};

const KERNEL: &str = r#"
kernel void mandelbulber_field_sample_kernel(
    device const float4 *points [[buffer(0)]], device float4 *samples [[buffer(1)]],
    constant FptRenderConfig &cfg [[buffer(2)]], uint gid [[thread_position_in_grid]]) {
    uint2 pixel = uint2(gid % cfg.width, cfg.height - 1u - gid / cfg.width);
    float2 xy = sdfScreenUv(pixel, cfg);
    float3 color = float3(0);
    for (uint sample = 0; sample < cfg.samples; ++sample)
        color = mix(color, renderPath(xy, sample, cfg), 1.0f / float(sample + 1));
    samples[gid] = float4(postProcess(color, cfg), 1);
}
"#;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 7,
        "usage: mandel_sampling_probe scene.fract mandel-root width height samples jittered|native NEW-output-dir"
    );
    ensure!(
        matches!(args[5].as_str(), "jittered" | "native"),
        "unknown sampling mode"
    );
    let out = Path::new(&args[6]);
    ensure!(!out.exists(), "refusing to overwrite probe output");
    let render_args = scene::parse_render_args(&[
        args[0].clone(),
        "--mandelbulber-root".into(),
        args[1].clone(),
        "--width".into(),
        args[2].clone(),
        "--height".into(),
        args[3].clone(),
        "--samples".into(),
        args[4].clone(),
        "--mandel-appearance".into(),
        "authored-path".into(),
        "--sdf-bounce-cap".into(),
        "1".into(),
    ])?;
    let loaded = scene::load_scene_config(&render_args)?;
    let mut cfg = loaded.config;
    scene::apply_camera_args(&mut cfg, &render_args);
    scene::apply_optimization_args(&mut cfg, &render_args);
    ensure!(
        cfg.sdf_bounce_cap == 1,
        "one-vertex control was not applied"
    );
    ensure!(cfg.camera_dof == 0.0, "disable DOF in the control scene");
    ensure!(
        cfg.width <= 400 && cfg.height <= 400 && cfg.samples <= 64,
        "probe limited to 400x400 and 64 samples"
    );
    let shader = String::from_utf8(
        loaded
            .runtime_metal_source
            .context("missing Mandel shader")?,
    )?;
    let mut shader = retain_metal_kernels(&shader, &[])?;
    let prefix =
        "static float3 renderPath(float2 xy, uint sample_idx, constant FptRenderConfig &cfg) {";
    let start = shader
        .find(prefix)
        .context("renderPath signature changed")?;
    let marker = "float aa_strength = 0.3f / max(float(cfg.width), float(cfg.height));";
    let offset = shader[start..]
        .find(marker)
        .context("camera jitter definition changed")?
        + start;
    ensure!(
        offset - start < 400,
        "jitter definition moved out of camera setup"
    );
    if args[5] == "native" {
        shader.replace_range(offset..offset + marker.len(), "float aa_strength = 0.0f;");
    }
    shader.push_str(KERNEL);
    // The field bridge requires an input buffer; camera coordinates are derived
    // by sdfScreenUv on the GPU to preserve production rounding and RNG seeds.
    let points = vec![[0.0_f32; 4]; cfg.width as usize * cfg.height as usize];
    let mut output = vec![FptMandelbulberFieldSample::default(); points.len()];
    let mut error = [0i8; 4096];
    let status = unsafe {
        fpt_mandelbulber_sample_field(
            c"unused.metallib".as_ptr(),
            shader.as_ptr().cast(),
            shader.len(),
            &cfg,
            points.as_ptr().cast(),
            points.len(),
            output.as_mut_ptr(),
            error.as_mut_ptr(),
            error.len(),
        )
    };
    ensure!(
        status == 0,
        "{}",
        unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy()
    );
    let mut pixels = Vec::with_capacity(points.len() * 3);
    for value in &output {
        for channel in [value.distance, value.radius, value.derivative] {
            ensure!(channel.is_finite(), "non-finite probe output");
            pixels.push((channel.clamp(0.0, 1.0) * 255.0) as u8);
        }
    }
    fs::create_dir_all(out)?;
    image::RgbImage::from_raw(cfg.width, cfg.height, pixels)
        .context("image dimensions")?
        .save(out.join("probe.png"))?;
    fs::write(
        out.join("summary.json"),
        serde_json::to_vec_pretty(&json!({
            "scene": args[0], "scene_sha256": format!("{:x}", Sha256::digest(fs::read(&args[0])?)),
            "shader_sha256": format!("{:x}", Sha256::digest(shader.as_bytes())),
            "size": [cfg.width,cfg.height], "samples": cfg.samples, "sampling": args[5],
            "scope": "One production renderPath vertex through a diagnostic kernel; no production sampling change. Native mode sets only camera jitter strength to zero. Control must disable other stochastic effects.",
        }))?,
    )?;
    println!("{}", out.display());
    Ok(())
}
