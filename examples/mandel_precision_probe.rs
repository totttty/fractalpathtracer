//! Numerical diagnostics only; this example does not change production rendering.
use anyhow::{Result, anyhow, ensure};
use fpt_metal::{MandelbulberScene, ffi::*, scene};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{ffi::CStr, fs, path::PathBuf};

const PROBE: &str = r#"
kernel void mandelbulber_field_sample_kernel(
    device const float4 *points [[buffer(0)]],
    device float4 *samples [[buffer(1)]],
    constant FptRenderConfig &cfg [[buffer(2)]],
    uint gid [[thread_position_in_grid]]) {
    float4 input = points[gid];
    if (input.w == 0.0f) {
        samples[gid] = mandelbulberFieldSample(input.xyz, cfg, 1);
        return;
    }
    float3 origin = cameraPos(cfg);
    float3 direction = mandelbulberCameraRay(input.xy, cfg);
    if (input.w == 1.0f) {
        samples[gid] = float4(direction, 0.0f);
    } else if (input.w == 2.0f) {
        float threshold = mandelbulberMarchThreshold(origin, cfg);
        float distance = mapSdf(origin, cfg);
        float step = sdfMarchStep(distance, threshold, cfg);
        bool stalled = all(origin + direction * step == origin);
        samples[gid] = float4(distance, threshold, step, float(stalled));
    } else if (input.w == 4.0f) {
        float threshold = mandelbulberMarchThreshold(origin, cfg);
        float step = sdfMarchStep(mapSdf(origin, cfg), threshold, cfg);
        // Read back the rounded position itself. Fast math can cancel an
        // in-shader subtraction of origin and hide the rounding being tested.
        samples[gid] = float4(origin + direction * step, 0.0f);
    } else {
        MandelbulberMarchResult hit = marchMandelbulber(direction, origin, int(cfg.render[1]), cfg);
        samples[gid] = float4(hit.position, float(hit.found));
    }
}
"#;

fn gpu_point(point: [f64; 3], scale: f64) -> [f32; 4] {
    [
        (point[0] * scale) as f32,
        (point[2] * scale) as f32,
        (point[1] * scale) as f32,
        0.0,
    ]
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 3,
        "usage: mandel_precision_probe scene.fract mandelbulber-root report.json"
    );
    let output = PathBuf::from(&args[2]);
    ensure!(
        !output.exists(),
        "refusing to overwrite a completed probe report"
    );
    let source = MandelbulberScene::load(&PathBuf::from(&args[0]))?;
    let render_args = scene::parse_render_args(&[
        args[0].clone(),
        "--mandelbulber-root".into(),
        args[1].clone(),
        "--width".into(),
        "160".into(),
        "--height".into(),
        "120".into(),
        "--samples".into(),
        "1".into(),
    ])?;
    let loaded = scene::load_scene_config(&render_args)?;
    let mut shader = String::from_utf8(
        loaded
            .runtime_metal_source
            .ok_or_else(|| anyhow!("scene has no generated shader"))?,
    )?;
    let generated_hash = format!("{:x}", Sha256::digest(shader.as_bytes()));
    let marker = "kernel void mandelbulber_field_sample_kernel(";
    ensure!(
        shader.matches(marker).count() == 1,
        "field kernel signature changed"
    );
    shader = shader.replace(marker, "kernel void original_field_sample_kernel(");
    shader.push_str(PROBE);
    let cfg = loaded.config;
    let scale = f64::from(cfg.set_values[0]);
    let delta: [f64; 3] = std::array::from_fn(|i| source.target[i] - source.camera[i]);
    let separation = delta.iter().map(|x| x * x).sum::<f64>().sqrt();
    let direction = delta.map(|x| x / separation);
    let mut points = Vec::<[f32; 4]>::new();
    let mut labels = Vec::new();
    for t in [-2e-6, -1e-7, 0.0, 1e-9, 1e-8, separation, 1e-7, 2e-6] {
        let point: [f64; 3] = std::array::from_fn(|i| source.camera[i] + direction[i] * t);
        let gpu = gpu_point(point, scale);
        points.push(gpu);
        labels.push(json!({"kind":"field", "ray_t_fractal_units":t, "source_point_f64":point, "gpu_input":gpu}));
    }
    for y in [-0.45, -0.225, 0.0, 0.225, 0.45] {
        for x in [-0.6, -0.3, 0.0, 0.3, 0.6] {
            for (mode, kind) in [
                (1.0, "ray"),
                (2.0, "first_step"),
                (3.0, "march"),
                (4.0, "next_position"),
            ] {
                points.push([x, y, 0.0, mode]);
                labels.push(json!({"kind":kind,"image_xy":[x,y]}));
            }
        }
    }
    let mut samples = vec![FptMandelbulberFieldSample::default(); points.len()];
    let mut error = [0_i8; 4096];
    // Runtime source is supplied, so the bridge does not load this fallback path.
    let status = unsafe {
        fpt_mandelbulber_sample_field(
            c"unused.metallib".as_ptr(),
            shader.as_ptr().cast(),
            shader.len(),
            &cfg,
            points.as_ptr().cast(),
            points.len(),
            samples.as_mut_ptr(),
            error.as_mut_ptr(),
            error.len(),
        )
    };
    ensure!(
        status == 0,
        "{}",
        unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy()
    );
    for (label, sample) in labels.iter_mut().zip(samples) {
        let output = [
            sample.distance,
            sample.radius,
            sample.derivative,
            sample.iterations,
        ];
        label["output"] = json!(output);
        label["finite"] = json!(output.iter().all(|x| x.is_finite()));
    }
    let report = json!({
        "scene": args[0],
        "scene_sha256":format!("{:x}", Sha256::digest(fs::read(&args[0])?)),
        "generated_shader_sha256":generated_hash,
        "diagnostic_shader_sha256":format!("{:x}", Sha256::digest(shader.as_bytes())),
        "probe_executable_sha256":format!("{:x}", Sha256::digest(fs::read(std::env::current_exe()?)?)),
        "fpt_environment":std::env::vars().filter(|(k,_)| k.starts_with("FPT_")).collect::<std::collections::BTreeMap<_,_>>(),
        "render_size":[160,120], "world_scale":scale, "camera_source":source.camera,
        "camera_gpu":cfg.camera_position, "camera_yaw_pitch":cfg.camera_yaw_pitch,
        "camera_target_separation":separation,
        "output_layouts": {
            "field":["distance_world_units","orbit_radius","derivative","signed_iterations"],
            "ray":["direction_x","direction_y","direction_z","unused"],
            "first_step":["distance_world_units","threshold_world_units","step_world_units","stalled"],
            "next_position":["x_world_units","y_world_units","z_world_units","unused"],
            "march":["hit_x_world_units","hit_y_world_units","hit_z_world_units","found"]
        }, "probes":labels
    });
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    ensure!(
        !output.exists(),
        "refusing to overwrite a completed probe report"
    );
    fs::write(output, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_probe_exposes_rounding_without_rounding_its_source() {
        let source = [-1.87049551608698, -19.6377709180615, 1.08136927634342];
        let near = std::array::from_fn(|i| source[i] + 1.0e-9);
        assert_ne!(source, near);
        assert_eq!(gpu_point(source, 1024.0), gpu_point(near, 1024.0));
        let far = std::array::from_fn(|i| source[i] + 2.0e-6);
        assert_ne!(gpu_point(source, 1024.0), gpu_point(far, 1024.0));
    }
}
