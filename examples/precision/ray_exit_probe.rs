//! Instrument a clone of the current primary loop, then cross-check its hit flag.
use anyhow::{Context, Result, ensure};
use fpt_metal::ffi::*;
use serde_json::json;
use std::{ffi::CStr, fs, path::Path};

fn once(text: &str, from: &str, to: &str) -> Result<String> {
    ensure!(
        text.matches(from).count() == 1,
        "ray probe source shape changed: {from}"
    );
    Ok(text.replacen(from, to, 1))
}

pub fn run(
    source: &str,
    cfg: &FptRenderConfig,
    input: &Path,
    out: &Path,
    analytic: bool,
) -> Result<()> {
    let points: Vec<[f32; 4]> = serde_json::from_slice(&fs::read(input)?)?;
    ensure!(
        !points.is_empty() && points.len() <= 4096,
        "invalid ray count"
    );
    ensure!(
        points.iter().all(|p| p.iter().all(|v| v.is_finite())
            && p[0] >= 0.0
            && p[0] < cfg.width as f32
            && p[1] >= 0.0
            && p[1] < cfg.height as f32
            && p[0].fract() == 0.0
            && p[1].fract() == 0.0),
        "invalid pixel coordinates"
    );
    let start = source
        .find("static MandelbulberMarchResult marchMandelbulber(")
        .context("missing primary function")?;
    let end = start
        + source[start..]
            .find("    if (!found) return MandelbulberMarchResult{position, false};")
            .context("missing primary exit")?;
    let mut probe = source[start..end].to_owned();
    probe = once(
        &probe,
        "static MandelbulberMarchResult marchMandelbulber(",
        "static float4 derivativeExitProbe(",
    )?;
    probe = once(
        &probe,
        "    bool found = false;",
        "    bool found = false;\n    int reason = 5, steps = 0;",
    )?;
    probe = once(
        &probe,
        "        threshold = mandelbulberMarchThreshold(position, cfg);",
        "        steps = iteration + 1;\n        threshold = mandelbulberMarchThreshold(position, cfg);",
    )?;
    probe = once(
        &probe,
        "if (!isfinite(distance)) break;",
        "if (!isfinite(distance)) { reason = 2; break; }",
    )?;
    probe = once(
        &probe,
        "            found = true;",
        "            found = true; reason = 1;",
    )?;
    probe = once(
        &probe,
        "if (all(next_position == position)) break;",
        "if (all(next_position == position)) { reason = 3; break; }",
    )?;
    probe = once(
        &probe,
        "if (length(position - start) >= cfg.render[4]) break;",
        "if (length(position - start) >= cfg.render[4]) { reason = 4; break; }",
    )?;
    probe.push_str("    return float4(reason, steps, isfinite(distance) ? distance / max(threshold, 1e-30f) : -1.0f, float(found));\n}\n");
    let shader = format!(
        "{source}\n{probe}\n{}",
        r#"
kernel void mandelbulber_field_sample_kernel(
    device const float4 *points [[buffer(0)]], device float4 *samples [[buffer(1)]],
    constant FptRenderConfig &cfg [[buffer(2)]], uint gid [[thread_position_in_grid]]) {
    uint2 pixel = uint2(points[gid].xy);
    if (cfg.camera_image_y_sign >= 0.0f) pixel.y = cfg.height - 1u - pixel.y;
    float3 direction = mandelbulberCameraRay(sdfScreenUv(pixel, cfg), cfg);
    int limit = min(int(max(cfg.render[1], 32.0f)), 10000);
    float4 result = derivativeExitProbe(direction, cameraPos(cfg), limit, cfg);
    bool actual = marchMandelbulber(direction, cameraPos(cfg), limit, cfg).found;
    // Negative reason marks instrumentation disagreement with the original loop.
    if (actual != (result.w > 0.5f)) result.x = -result.x;
    result.w = float(actual);
    samples[gid] = result;
}
"#
    );
    let mut values = vec![FptMandelbulberFieldSample::default(); points.len()];
    let mut error = [0i8; 4096];
    let status = unsafe {
        fpt_mandelbulber_sample_field(
            c"unused.metallib".as_ptr(),
            shader.as_ptr().cast(),
            shader.len(),
            cfg,
            points.as_ptr().cast(),
            points.len(),
            values.as_mut_ptr(),
            error.as_mut_ptr(),
            error.len(),
        )
    };
    ensure!(
        status == 0,
        "{}",
        unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy()
    );
    let rows: Vec<_> = values.iter().zip(points).map(|(s,p)| json!({"pixel": [p[0],p[1]], "reason":s.distance, "steps":s.radius, "distance_over_threshold":s.derivative,"found":s.iterations > 0.5})).collect();
    fs::write(
        out.join("ray-exits.json"),
        serde_json::to_vec_pretty(&json!({"analytic85":analytic,
        "reasons":{"1":"hit","2":"nonfinite","3":"float32 position stall","4":"view range","5":"iteration budget"},
        "rows":rows}))?,
    )?;
    ensure!(
        values.iter().all(|s| s.distance > 0.0),
        "instrumented/actual hit flags disagree"
    );
    Ok(())
}
