use anyhow::{Result, anyhow, bail, ensure};
use image::{GenericImage, ImageBuffer, Rgb, RgbImage, imageops::FilterType};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

fn existing_images(args: &[String]) -> Vec<PathBuf> {
    args.iter()
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .collect()
}

fn fitted(path: &Path, width: u32, height: u32, background: Rgb<u8>) -> Result<RgbImage> {
    let image = image::open(path)?.to_rgb8();
    let scale = (width as f64 / image.width() as f64).min(height as f64 / image.height() as f64);
    let resized_width = (image.width() as f64 * scale).round().max(1.0) as u32;
    let resized_height = (image.height() as f64 * scale).round().max(1.0) as u32;
    let resized =
        image::imageops::resize(&image, resized_width, resized_height, FilterType::Lanczos3);
    let mut canvas = ImageBuffer::from_pixel(width, height, background);
    canvas.copy_from(
        &resized,
        (width - resized_width) / 2,
        (height - resized_height) / 2,
    )?;
    Ok(canvas)
}

pub fn contact_sheet_command(args: &[String]) -> Result<()> {
    let output = PathBuf::from(args.first().ok_or_else(|| anyhow!("missing output PNG"))?);
    let inputs = existing_images(&args[1..]);
    ensure!(!inputs.is_empty(), "no input images");
    let tile_width = 320;
    let tile_height = 240;
    let gutter = 8;
    let columns = inputs.len().min(3) as u32;
    let rows = inputs.len().div_ceil(columns as usize) as u32;
    let mut sheet = ImageBuffer::from_pixel(
        columns * tile_width,
        rows * (tile_height + gutter),
        Rgb([24, 24, 24]),
    );
    for (index, path) in inputs.iter().enumerate() {
        let tile = fitted(path, tile_width, tile_height, Rgb([24, 24, 24]))?;
        sheet.copy_from(
            &tile,
            index as u32 % columns * tile_width,
            index as u32 / columns * (tile_height + gutter),
        )?;
    }
    if let Some(parent) = output
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    sheet.save(&output)?;
    println!("{}", output.display());
    Ok(())
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn json_number(value: &Value, key: &str) -> f64 {
    value.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

pub fn report_index_command(args: &[String]) -> Result<()> {
    let root = fs::canonicalize(
        args.first()
            .ok_or_else(|| anyhow!("missing report directory"))?,
    )?;
    let mut reports: Vec<_> = fs::read_dir(&root)?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .collect();
    reports.sort();
    ensure!(!reports.is_empty(), "no reports in {}", root.display());
    let mut rows = String::new();
    for report in &reports {
        let data: Value = serde_json::from_slice(&fs::read(report)?)?;
        let Some(candidate) = data.get("candidate").and_then(Value::as_str) else {
            continue;
        };
        let comparison = data
            .get("comparison_sheet")
            .and_then(Value::as_str)
            .unwrap_or("");
        let candidate_rel = Path::new(candidate)
            .strip_prefix(&root)
            .unwrap_or(Path::new(candidate))
            .to_string_lossy();
        let comparison_rel = Path::new(comparison)
            .strip_prefix(&root)
            .unwrap_or(Path::new(comparison))
            .to_string_lossy();
        let timing = fs::read(format!("{candidate}.render.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .map(|metadata| {
                format!(
                    ", render <b>{:.2} ms</b>, <b>{:.2}</b> MPixSamples/s",
                    json_number(&metadata, "elapsed_ms"),
                    json_number(&metadata, "megapixel_samples_per_second")
                )
            })
            .unwrap_or_default();
        let recognition = data
            .get("recognition_gate")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let strict = data
            .get("strict_gate")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let stem = report
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("report");
        rows.push_str(&format!(
            "<section class=\"{}\"><h2>{} <span>{}</span> <span>{}</span></h2><p>MAE <b>{:.2}</b>, RMSE <b>{:.2}</b>, SSIM <b>{:.4}</b>, LF-SSIM <b>{:.4}</b>{}</p><div class=\"images\"><figure><img src=\"{}\"><figcaption>candidate</figcaption></figure><figure><img src=\"{}\"><figcaption>baseline / candidate / diff</figcaption></figure></div></section>\n",
            if recognition { "pass" } else { "fail" }, html_escape(stem), if recognition { "PASS" } else { "FAIL" }, if strict { "STRICT PASS" } else { "strict pending" },
            json_number(&data, "mean_absolute_error"), json_number(&data, "root_mean_square_error"), json_number(&data, "luminance_ssim"), json_number(&data, "low_frequency_luminance_ssim"), timing,
            html_escape(&candidate_rel), html_escape(&comparison_rel),
        ));
    }
    let page = format!(
        r#"<!doctype html><meta charset="utf-8"><title>FPT Metal parity report</title>
<style>body{{margin:24px;font-family:-apple-system,BlinkMacSystemFont,sans-serif;background:#151515;color:#eee}}section{{border:1px solid #333;border-radius:6px;padding:14px;margin:14px 0;background:#1d1d1d}}section.pass{{border-color:#315f3a}}section.fail{{border-color:#763333}}h2{{font-size:18px}}h2 span{{font-size:12px;margin-left:8px}}.images{{display:grid;grid-template-columns:minmax(260px,420px) minmax(360px,1fr);gap:12px}}figure{{margin:0}}img{{max-width:100%;height:auto}}figcaption{{color:#999;font-size:12px}}</style>
<h1>FPT Metal parity report</h1><p>{} scenes</p>{}"#,
        reports.len(),
        rows
    );
    let output = root.join("index.html");
    fs::write(&output, page)?;
    println!("{}", output.display());
    Ok(())
}

pub fn check_parity_reports_command(args: &[String]) -> Result<()> {
    ensure!(!args.is_empty(), "missing report directories");
    let mut failed = Vec::new();
    let mut strict_pending = Vec::new();
    let mut total = 0;
    for directory in args {
        let root = Path::new(directory);
        let mut reports: Vec<_> = fs::read_dir(root)?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
            .collect();
        reports.sort();
        for report in reports {
            let data: Value = serde_json::from_slice(&fs::read(&report)?)?;
            if data.get("recognition_gate").is_none() {
                continue;
            }
            total += 1;
            let label = format!(
                "{}/{}",
                root.file_name().unwrap_or_default().to_string_lossy(),
                report.file_stem().unwrap_or_default().to_string_lossy()
            );
            if data.get("recognition_gate").and_then(Value::as_bool) != Some(true) {
                failed.push(label.clone());
            }
            if data.get("strict_gate").and_then(Value::as_bool) != Some(true) {
                strict_pending.push(label);
            }
        }
    }
    if !failed.is_empty() {
        bail!("recognition parity failures:\n  {}", failed.join("\n  "));
    }
    println!("recognition parity passed for {total} reports");
    if strict_pending.is_empty() {
        println!("strict parity passed for all reports");
    } else {
        println!(
            "strict parity pending for {} reports:\n  {}",
            strict_pending.len(),
            strict_pending.join("\n  ")
        );
    }
    Ok(())
}

fn image_for(directory: &Path, prefix: &str) -> Result<PathBuf> {
    let mut matches: Vec<_> = fs::read_dir(directory)?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.starts_with(&format!("{prefix}-")))
                && matches!(
                    path.extension()
                        .and_then(|value| value.to_str())
                        .map(str::to_ascii_lowercase)
                        .as_deref(),
                    Some("png" | "jpg" | "jpeg")
                )
        })
        .collect();
    matches.sort();
    matches.into_iter().next().ok_or_else(|| {
        anyhow!(
            "no image beginning with {prefix}- in {}",
            directory.display()
        )
    })
}

pub fn readme_comparison_command(args: &[String]) -> Result<()> {
    ensure!(
        args.len() == 3,
        "usage: readme-comparison ORIGINALS GENERATED OUT.png"
    );
    let originals = Path::new(&args[0]);
    let generated = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    let prefixes = ["01", "02", "03", "04", "07", "08", "09"];
    let tile_width = 560;
    let tile_height = 360;
    let gutter = 12;
    let mut sheet = ImageBuffer::from_pixel(
        tile_width * 2 + gutter * 3,
        (tile_height + gutter) * prefixes.len() as u32 + gutter,
        Rgb([24, 24, 24]),
    );
    for (row, prefix) in prefixes.iter().enumerate() {
        let left = fitted(
            &image_for(originals, prefix)?,
            tile_width,
            tile_height,
            Rgb([12, 12, 12]),
        )?;
        let right = fitted(
            &image_for(generated, prefix)?,
            tile_width,
            tile_height,
            Rgb([12, 12, 12]),
        )?;
        let y = gutter + row as u32 * (tile_height + gutter);
        sheet.copy_from(&left, gutter, y)?;
        sheet.copy_from(&right, gutter * 2 + tile_width, y)?;
    }
    if let Some(parent) = output
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    sheet.save(output)?;
    println!("{}", output.display());
    Ok(())
}

fn write_test_hdr(path: &Path) -> Result<()> {
    let width = 32_u8;
    let height = 16;
    let mut data = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 16 +X 32\n".to_vec();
    for y in 0..height {
        data.extend([2, 2, 0, width]);
        for value in [48 + y * 5, 90 + y * 3, 180 - y * 4, 129] {
            data.extend([128 + width, value.max(1)]);
        }
    }
    fs::write(path, data)?;
    Ok(())
}

pub fn capability_fixtures_command(args: &[String]) -> Result<()> {
    ensure!(
        args.len() == 2,
        "usage: capability-fixtures FPT_ROOT OUT_DIR"
    );
    let fpt_root = Path::new(&args[0]);
    let out = Path::new(&args[1]);
    fs::create_dir_all(out)?;
    write_test_hdr(&out.join("test.hdr"))?;
    let mut gradient: Value = serde_json::from_slice(&fs::read(
        fpt_root.join("Beauty/Fractals/Tree_Fractal.json"),
    )?)?;
    gradient["preset"] = Value::String(
        fpt_root
            .join("Fractals/Gradient_Example.fpt")
            .to_string_lossy()
            .into_owned(),
    );
    gradient["output"] = json!("gradient-compat.png");
    gradient["width"] = json!(160);
    gradient["height"] = json!(120);
    fs::write(
        out.join("gradient-compat.json"),
        serde_json::to_vec_pretty(&gradient)?,
    )?;
    let typed = json!({
        "preset": fpt_root.join("Fractals/Gradient_Example.fpt"), "output": "typed-program.png", "width": 160, "height": 120, "samples": 16,
        "camera": {"position": [0,0,-4], "yaw_pitch": [0,0], "fov": 70, "dof": 0, "focus_distance": 3},
        "render": {"bounces":2,"marching_steps":128,"normal_epsilon":0.001,"min_distance":0.001,"max_distance":30,"adaptive_marching":0.25,"error_protection":0,"random_mode":0},
        "world": {"mode":2,"light_size":1,"rotation":15,"elevation":0,"power":1,"contrast":1,"gradient_background":0,"hdri":out.join("test.hdr")},
        "sun": {"enabled":1,"rotation":35,"elevation":40,"power":1.5,"softness":0.05},
        "post": {"tone_map":3,"exposure":1,"brightness":0,"saturation":1,"contrast":1,"chromatic_aberration":0.03,"highlights":0.08},
        "sdf_program": {"operations":[{"op":"translate","value":[0,0,0.4]},{"op":"sphere","radius":1.1,"orbit_weight":1}],"material":{"mode":"gradient","roughness":0.78,"specular":0.12,"translucency":0,"ior":1.5,"emission":0},"gradient":[{"position":0,"color":[0.08,0.62,0.95]},{"position":0.5,"color":[0.95,0.30,0.12]},{"position":1,"color":[0.82,0.92,0.18]}]}
    });
    fs::write(
        out.join("typed-program.json"),
        serde_json::to_vec_pretty(&typed)?,
    )?;
    Ok(())
}

pub fn path_cost_summary_command(args: &[String]) -> Result<()> {
    let root = Path::new(
        args.first()
            .ok_or_else(|| anyhow!("missing output directory"))?,
    );
    let modes = [
        ("sdf-primary-steps", "primary march work", "primary"),
        ("sdf-shadow-steps", "shadow march work", "shadow"),
        ("sdf-normal-evals", "normal estimation work", "normal"),
        ("sdf-bounces", "surface bounce count", "bounces"),
    ];
    let mut rows = Vec::new();
    for (mode, label, _) in modes {
        let directory = root.join(mode);
        let mut images: Vec<_> = fs::read_dir(&directory)?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("png"))
            .collect();
        images.sort();
        for path in images {
            let pixels = image::open(&path)?.to_luma8().into_raw();
            let mut ordered = pixels.clone();
            ordered.sort_unstable();
            let mean =
                pixels.iter().map(|&value| value as f64).sum::<f64>() / pixels.len() as f64 / 255.0;
            let nonzero =
                pixels.iter().filter(|&&value| value > 0).count() as f64 / pixels.len() as f64;
            let p95 = ordered[((ordered.len() as f64 * 0.95) as usize).min(ordered.len() - 1)]
                as f64
                / 255.0;
            rows.push(json!({
                "scene": path.file_stem().unwrap_or_default().to_string_lossy(), "mode": mode, "label": label,
                "mean_normalized": mean, "p95_normalized": p95, "nonzero_fraction": nonzero,
            }));
        }
    }
    let mut scenes = std::collections::BTreeMap::<String, [f64; 4]>::new();
    for row in &rows {
        let scene = row["scene"].as_str().unwrap_or_default().to_owned();
        let index = modes
            .iter()
            .position(|(mode, _, _)| *mode == row["mode"].as_str().unwrap_or_default())
            .unwrap();
        scenes.entry(scene).or_default()[index] = row["mean_normalized"].as_f64().unwrap_or(0.0);
    }
    let mut ranked: Vec<Value> = scenes.into_iter().map(|(scene, scores)| {
        let (index, score) = scores.iter().copied().enumerate().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
        json!({"scene":scene,"dominant_blocker":modes[index].2,"dominant_score":score,"primary_score":scores[0],"shadow_score":scores[1],"normal_score":scores[2],"bounce_score":scores[3]})
    }).collect();
    ranked.sort_by(|a, b| {
        json_number(b, "dominant_score").total_cmp(&json_number(a, "dominant_score"))
    });
    fs::write(
        root.join("summary.json"),
        format!("{}\n", serde_json::to_string_pretty(&rows)?),
    )?;
    fs::write(
        root.join("blocker_ranking.json"),
        format!("{}\n", serde_json::to_string_pretty(&ranked)?),
    )?;
    println!(
        "sdf path cost diagnostic report: {}\n{}",
        root.display(),
        serde_json::to_string_pretty(&ranked.iter().take(5).collect::<Vec<_>>())?
    );
    Ok(())
}

pub fn bounce_summary_command(args: &[String]) -> Result<()> {
    ensure!(args.len() >= 2, "usage: bounce-summary OUT_DIR BOUNCE...");
    let root = Path::new(&args[0]);
    let mut summary = Vec::new();
    for value in &args[1..] {
        let bounce: u32 = value.parse()?;
        let directory = root.join(format!("bounce_{bounce}"));
        let metas: Vec<_> = fs::read_dir(&directory)?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.to_string_lossy().ends_with(".png.render.json"))
            .collect();
        let mut total_ms = 0.0;
        for path in &metas {
            total_ms += json_number(&serde_json::from_slice(&fs::read(path)?)?, "elapsed_ms");
        }
        summary.push(json!({"bounce":bounce,"scene_count":metas.len(),"diagnostic_total_ms":total_ms,"sheet":root.join(format!("bounce_{bounce}_sheet.png"))}));
    }
    fs::write(
        root.join("summary.json"),
        format!("{}\n", serde_json::to_string_pretty(&summary)?),
    )?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}

pub fn optimization_summary_command(args: &[String]) -> Result<()> {
    ensure!(
        args.len() == 6 || args.len() == 7,
        "usage: optimization-summary OUT_DIR MAX_MAE MAX_RMSE MIN_SSIM MIN_LF_SSIM RUNS [EXPECTED_SCENES]"
    );
    let root = Path::new(&args[0]);
    let max_mae: f64 = args[1].parse()?;
    let max_rmse: f64 = args[2].parse()?;
    let min_ssim: f64 = args[3].parse()?;
    let min_lf_ssim: f64 = args[4].parse()?;
    let runs: usize = args[5].parse()?;
    let expected_scenes: usize = args
        .get(6)
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(9);
    let mut reports: Vec<_> = fs::read_dir(root.join("compare"))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .collect();
    reports.sort();
    ensure!(
        reports.len() == expected_scenes,
        "expected {expected_scenes} comparison reports, got {}",
        reports.len()
    );
    let mut summary = Vec::new();
    let mut failures = Vec::new();
    for report in reports {
        let scene = report
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let comparison: Value = serde_json::from_slice(&fs::read(&report)?)?;
        let mut baseline_times = Vec::new();
        let mut candidate_times = Vec::new();
        let mut baseline_throughput = Vec::new();
        let mut candidate_throughput = Vec::new();
        for run in 0..runs {
            let baseline: Value = serde_json::from_slice(&fs::read(
                root.join(format!("baseline/run_{run}/{scene}.png.render.json")),
            )?)?;
            let candidate: Value = serde_json::from_slice(&fs::read(
                root.join(format!("candidate/run_{run}/{scene}.png.render.json")),
            )?)?;
            baseline_times.push(json_number(&baseline, "elapsed_ms"));
            candidate_times.push(json_number(&candidate, "elapsed_ms"));
            baseline_throughput.push(json_number(&baseline, "megapixel_samples_per_second"));
            candidate_throughput.push(json_number(&candidate, "megapixel_samples_per_second"));
        }
        let baseline_ms = median(baseline_times.clone());
        let candidate_ms = median(candidate_times.clone());
        let mae = json_number(&comparison, "mean_absolute_error");
        let rmse = json_number(&comparison, "root_mean_square_error");
        let ssim = json_number(&comparison, "luminance_ssim");
        let lf_ssim = json_number(&comparison, "low_frequency_luminance_ssim");
        if mae > max_mae {
            failures.push(format!("{scene}: MAE {mae:.4} exceeds {max_mae:.4}"));
        }
        if rmse > max_rmse {
            failures.push(format!("{scene}: RMSE {rmse:.4} exceeds {max_rmse:.4}"));
        }
        if ssim < min_ssim {
            failures.push(format!("{scene}: SSIM {ssim:.6} below {min_ssim:.6}"));
        }
        if lf_ssim < min_lf_ssim {
            failures.push(format!(
                "{scene}: low-frequency SSIM {lf_ssim:.6} below {min_lf_ssim:.6}"
            ));
        }
        summary.push(json!({
            "scene":scene,"runs":runs,"baseline_ms":baseline_ms,"candidate_ms":candidate_ms,
            "speedup":if candidate_ms > 0.0 { baseline_ms / candidate_ms } else { 0.0 },
            "baseline_timing":{"runs":baseline_times},"candidate_timing":{"runs":candidate_times},
            "baseline_mpix_samples_per_second":median(baseline_throughput),"candidate_mpix_samples_per_second":median(candidate_throughput),
            "mae":mae,"rmse":rmse,"luminance_ssim":ssim,"low_frequency_luminance_ssim":lf_ssim,
            "changed_pixel_percent":comparison.get("changed_pixel_percent"),"candidate_unique_colours":comparison.pointer("/unique_colours/candidate"),
        }));
    }
    let baseline_total: f64 = summary
        .iter()
        .map(|row| json_number(row, "baseline_ms"))
        .sum();
    let candidate_total: f64 = summary
        .iter()
        .map(|row| json_number(row, "candidate_ms"))
        .sum();
    let aggregate = json!({
        "scene_count":summary.len(),"runs":runs,"baseline_total_ms":baseline_total,"candidate_total_ms":candidate_total,
        "overall_speedup":if candidate_total > 0.0 { baseline_total/candidate_total } else { 0.0 },
        "max_mae":summary.iter().map(|row|json_number(row,"mae")).fold(0.0,f64::max),
        "max_rmse":summary.iter().map(|row|json_number(row,"rmse")).fold(0.0,f64::max),
        "min_luminance_ssim":summary.iter().map(|row|json_number(row,"luminance_ssim")).fold(1.0,f64::min),
        "min_low_frequency_luminance_ssim":summary.iter().map(|row|json_number(row,"low_frequency_luminance_ssim")).fold(1.0,f64::min),
        "thresholds":{"max_mae":max_mae,"max_rmse":max_rmse,"min_luminance_ssim":min_ssim,"min_low_frequency_luminance_ssim":min_lf_ssim},
        "failures":failures,
    });
    fs::write(
        root.join("summary.json"),
        format!("{}\n", serde_json::to_string_pretty(&summary)?),
    )?;
    fs::write(
        root.join("performance_summary.json"),
        format!("{}\n", serde_json::to_string_pretty(&aggregate)?),
    )?;
    println!("{}", serde_json::to_string_pretty(&aggregate)?);
    ensure!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

pub fn voxel_summary_command(args: &[String]) -> Result<()> {
    let output = PathBuf::from(
        args.first()
            .ok_or_else(|| anyhow!("missing voxel summary output path"))?,
    );
    ensure!(
        args.len() >= 3 && (args.len() - 1).is_multiple_of(2),
        "voxel-summary expects SDF/voxel metadata pairs"
    );
    let mut scenes = Vec::new();
    for pair in args[1..].chunks_exact(2) {
        let sdf: Value = serde_json::from_slice(&fs::read(&pair[0])?)?;
        let voxel: Value = serde_json::from_slice(&fs::read(&pair[1])?)?;
        let sdf_ms = json_number(&sdf, "elapsed_ms");
        let build_ms = json_number(&voxel, "voxel_build_ms");
        let voxel_ms = json_number(&voxel, "elapsed_ms");
        let scene = sdf
            .get("scene")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        scenes.push(json!({
            "scene": scene,
            "width": voxel.get("width"),
            "height": voxel.get("height"),
            "samples": voxel.get("samples"),
            "voxel_resolution": voxel.get("voxel_resolution"),
            "voxel_storage": voxel.get("voxel_storage"),
            "voxel_memory_bytes": voxel.get("voxel_memory_bytes"),
            "voxel_active_bricks": voxel.get("voxel_active_bricks"),
            "sdf_render_ms": sdf_ms,
            "sdf_fps": if sdf_ms > 0.0 { 1000.0 / sdf_ms } else { 0.0 },
            "voxel_build_ms": build_ms,
            "voxel_render_ms": voxel_ms,
            "voxel_render_fps": if voxel_ms > 0.0 { 1000.0 / voxel_ms } else { 0.0 },
            "voxel_first_frame_ms": build_ms + voxel_ms,
            "cached_render_speedup": if voxel_ms > 0.0 { sdf_ms / voxel_ms } else { 0.0 },
        }));
    }
    let sdf_total: f64 = scenes
        .iter()
        .map(|scene| json_number(scene, "sdf_render_ms"))
        .sum();
    let voxel_build_total: f64 = scenes
        .iter()
        .map(|scene| json_number(scene, "voxel_build_ms"))
        .sum();
    let voxel_render_total: f64 = scenes
        .iter()
        .map(|scene| json_number(scene, "voxel_render_ms"))
        .sum();
    let report = json!({
        "scenes": scenes,
        "aggregate": {
            "sdf_render_ms": sdf_total,
            "voxel_build_ms": voxel_build_total,
            "voxel_render_ms": voxel_render_total,
            "voxel_first_frame_ms": voxel_build_total + voxel_render_total,
            "cached_render_speedup": if voxel_render_total > 0.0 { sdf_total / voxel_render_total } else { 0.0 },
        }
    });
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &output,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
