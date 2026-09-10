//! Explicit, scene-pinned offline precision reference, never the production renderer.
use anyhow::{Context, Result, anyhow, bail, ensure};
use fpt_metal::MandelbulberScene;
use image::{Rgb, RgbImage};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command, time::Instant};

const SCENE_SHA: &str = "98baaeb09ffaf586320bd07492fb72ed47f59bc504f5be3dc612db7fca2ed27e";
const FORMULA_SHA: &str = "c239434a5e7b853b9d9c0bb3245d71facf8bb73001f16dec44fbeb8451c581ae";
const EXPANSION: &str = include_str!("precision/Expansion.metal");
const TEMPLATE: &str = include_str!("precision/Reference.metal");
const DRIVER: &str = include_str!("precision/probe.mm");

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn check_hash(bytes: &[u8], expected: &str, label: &str) -> Result<()> {
    ensure!(
        hash(bytes) == expected,
        "unsupported {label}: this offline experiment accepts only the validated RoadToExascale scene and formula revision; no float32 fallback"
    );
    Ok(())
}

fn split(value: f64) -> Result<[f32; 3]> {
    let hi = value as f32;
    let mid = (value - f64::from(hi)) as f32;
    let lo = (value - f64::from(hi) - f64::from(mid)) as f32;
    ensure!(
        value.is_finite() && [hi, mid, lo].iter().all(|x| x.is_finite()),
        "unsupported expansion input"
    );
    Ok([hi, mid, lo])
}

fn scalar(value: f64) -> Result<String> {
    let [a, b, c] = split(value)?;
    Ok(format!("R({a:.17e}f,{b:.17e}f,{c:.17e}f)"))
}

fn vector(value: [f64; 3]) -> Result<String> {
    Ok(format!(
        "V({},{},{})",
        scalar(value[0])?,
        scalar(value[1])?,
        scalar(value[2])?
    ))
}

fn parameter(scene: &MandelbulberScene, key: &str, default: &str) -> Result<Vec<f64>> {
    scene
        .formula_parameters
        .get(key)
        .map(String::as_str)
        .unwrap_or(default)
        .split_whitespace()
        .map(|s| {
            s.replace(',', ".")
                .parse()
                .with_context(|| format!("invalid parameter {key}"))
        })
        .collect()
}

// The imported body is hash-pinned. This is intentionally not a general C++ parser.
fn block(source: &str, marker: &str) -> Result<String> {
    let start = source
        .find(marker)
        .ok_or_else(|| anyhow!("missing formula branch {marker}"))?;
    let open = start + source[start..].find('{').context("missing branch body")?;
    let mut depth = 0;
    for (offset, c) in source[open..].char_indices() {
        if c == '{' {
            depth += 1;
        }
        if c == '}' {
            depth -= 1;
            if depth == 0 {
                return Ok(source[start..=open + offset].to_owned());
            }
        }
    }
    bail!("unterminated formula branch")
}

fn formula(source: &str, scene: &MandelbulberScene) -> Result<String> {
    check_hash(source.as_bytes(), FORMULA_SHA, "formula")?;
    let mut body = String::from(
        "void formula(thread V &z, thread Aux &aux) {\nV oldZ=z, zCol=z; R k(1);\nV cSize=@CSIZE@;\n",
    );
    for name in [
        "functionEnabledxFalse",
        "functionEnabledAy",
        "functionEnabledAyFalse",
        "functionEnabledFFalse",
    ] {
        let marker = format!("if (fractal->transformCommon.{name}\n");
        let mut part = block(source, &marker)?;
        if name == "functionEnabledFFalse" {
            let inactive = block(&part, "if (fractal->transformCommon.functionEnabledFalse\n")?;
            part = part.replace(&inactive, "");
        }
        // Strip the upstream's commented-out color block before rejecting unresolved members.
        while let Some(start) = part.find("/*") {
            let end = start
                + part[start..]
                    .find("*/")
                    .context("unclosed formula comment")?
                + 2;
            part.replace_range(start..end, "");
        }
        body.push_str(&part);
        body.push('\n');
    }
    body.push_str("}\n");
    for name in [
        "functionEnabledxFalse",
        "functionEnabledAyFalse",
        "functionEnabledAy",
        "functionEnabledFFalse",
    ] {
        body = body.replace(&format!("fractal->transformCommon.{name}"), "true");
    }
    for (member, key, default) in [
        ("startIterationsD", "transf_start_iterations_D", "0"),
        ("stopIterationsTM1", "transf_stop_iterationsTM_1", "250"),
        ("startIterationsC", "transf_start_iterations_C", "0"),
        ("stopIterationsC", "transf_stop_iterations_C", "250"),
        ("startIterationsB", "transf_start_iterations_B", "0"),
        ("stopIterationsB", "transf_stop_iterations_B", "250"),
        ("startIterationsF", "transf_start_iterations_F", "0"),
        ("stopIterationsF", "transf_stop_iterations_F", "250"),
    ] {
        let p = parameter(scene, key, default)?;
        ensure!(
            p.len() == 1 && p[0].fract() == 0.0,
            "invalid iteration range"
        );
        body = body.replace(
            &format!("fractal->transformCommon.{member}"),
            &(p[0] as i32).to_string(),
        );
    }
    for (member, key, default) in [
        (
            "fractal->transformCommon.scale3D333",
            "transf_scale3D_333",
            "3 3 3",
        ),
        (
            "fractal->transformCommon.additionConstant111",
            "transf_addition_constant_111",
            "1 1 1",
        ),
        (
            "fractal->transformCommon.offsetA000",
            "transf_offsetA_000",
            "0 0 0",
        ),
        ("@CSIZE@", "transf_addition_constant_0777", "0.5 0.7 0.7"),
    ] {
        let p = parameter(scene, key, default)?;
        ensure!(p.len() == 3, "invalid vector {key}");
        body = body.replace(member, &vector([p[0], p[1], p[2]])?);
    }
    for (member, key, default) in [
        (
            "fractal->transformCommon.minR05",
            "transf_minimum_radius_05",
            "0.5",
        ),
        ("fractal->analyticDE.scale1", "analyticDE_scale_1", "1"),
        (
            "fractal->analyticDE.tweak005",
            "analyticDE_tweak_005",
            "0.05",
        ),
    ] {
        let p = parameter(scene, key, default)?;
        ensure!(p.len() == 1, "invalid scalar {key}");
        body = body.replace(member, &scalar(p[0])?);
    }
    for (key, value) in [
        ("SQRT_2_3", (2.0_f64 / 3.0).sqrt()),
        ("SQRT_1_3", (1.0_f64 / 3.0).sqrt()),
        ("SQRT_1_2", 0.5_f64.sqrt()),
    ] {
        body = body.replace(key, &scalar(value)?);
    }
    body = body
        .replace("CVector4", "V")
        .replace("double ", "R ")
        .replace("z *= k;", "z = z * k;")
        .replace("aux.DE *= k + ", "aux.DE = aux.DE * (k + ");
    // Close the two rewritten compound assignments, preserving native evaluation order.
    body = body
        .lines()
        .map(|line| {
            if line.contains("aux.DE = aux.DE * (k +") {
                line.replacen(';', ");", 1)
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    ensure!(
        !body.contains("fractal->") && !body.contains('@'),
        "unmapped formula feature"
    );
    Ok(body)
}

fn normalize(v: [f64; 3]) -> Result<[f64; 3]> {
    let length = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    ensure!(
        length.is_finite() && length > 0.0,
        "degenerate camera basis"
    );
    Ok(v.map(|x| x / length))
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn command(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("start {program}"))?;
    ensure!(
        output.status.success(),
        "{program} failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?)
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--help"] {
        println!(
            "mandel_precise_render SCENE.fract MANDELBULBER_ROOT NEW_OUTPUT_DIR [WIDTHxHEIGHT]\nOffline, white headlight, one deterministic ray/pixel. Only the validated RoadToExascale source is supported. Default 160x120; maximum 320x240. No production renderer fallback."
        );
        return Ok(());
    }
    ensure!(args.len() == 3 || args.len() == 4, "use --help for usage");
    let (w, h) = args
        .get(3)
        .map(String::as_str)
        .unwrap_or("160x120")
        .split_once('x')
        .context("expected WIDTHxHEIGHT")?;
    let (w, h): (u32, u32) = (w.parse()?, h.parse()?);
    ensure!(
        w > 0 && h > 0 && w <= 320 && h <= 240,
        "offline size must be 1..320 by 1..240"
    );
    let out = Path::new(&args[2]);
    ensure!(
        !out.exists(),
        "refusing to overwrite an existing output directory"
    );
    let source = fs::read(&args[0])?;
    check_hash(&source, SCENE_SHA, "scene")?;
    let scene = MandelbulberScene::parse(std::str::from_utf8(&source)?)?;
    let native_path =
        Path::new(&args[1]).join("formula/definition/fractal_pseudo_kleinian_mod2.cpp");
    let native = fs::read_to_string(&native_path)?;
    let imported = formula(&native, &scene)?;
    let fov = 2.0 * (scene.fov_degrees.to_radians() * 0.5).tan();
    let forward = normalize(std::array::from_fn(|i| scene.target[i] - scene.camera[i]))?;
    let right = normalize(cross(forward, scene.camera_top))?;
    let up = normalize(cross(right, forward))?;
    let mut shader = TEMPLATE
        .replace("@FORMULA@", &imported)
        .replace("@ITERATIONS@", &scene.max_iterations.to_string())
        .replace("@MAX_STEPS@", &scene.max_raymarching_steps.to_string())
        .replace("@ORIGIN@", &vector(scene.camera)?);
    for (key, value) in [
        ("@ORBIT_EPS@", 0.001),
        ("@MIN_THRESHOLD@", 1e-12),
        ("@THRESHOLD_SCALE@", f64::from(h) * scene.detail_level / fov),
        ("@DE_FACTOR@", scene.de_factor),
        ("@VIEW_MAX@", scene.view_distance_max),
        ("@REFINE_RATIO@", 0.998),
        ("@NORMAL_SCALE@", 0.1),
    ] {
        shader = shader.replace(key, &scalar(value)?);
    }
    ensure!(!shader.contains('@'), "unexpanded shader placeholder");
    let notice = native
        .split("#include")
        .next()
        .context("missing native notice")?;
    shader = format!(
        "// Generated with external Mandelbulber GPLv3 formula code. Do not package as Apache source.\n{notice}\n{EXPANSION}\n{shader}"
    );
    let mut rays = Vec::new();
    let mut rays_f64 = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let u = (f64::from(x) + 0.5 - f64::from(w) / 2.0) / f64::from(h) * fov;
            let v = (f64::from(h) / 2.0 - f64::from(y) - 0.5) / f64::from(h) * fov;
            let ray = normalize(std::array::from_fn(|i| {
                forward[i] + right[i] * u + up[i] * v
            }))?;
            for value in ray {
                rays_f64.extend(value.to_le_bytes());
                for term in split(value)? {
                    rays.extend(term.to_le_bytes());
                }
            }
        }
    }
    fs::create_dir_all(out.parent().unwrap_or(Path::new(".")))?;
    fs::create_dir(out)?;
    fs::write(out.join("reference.metal"), &shader)?;
    fs::write(out.join("probe.mm"), DRIVER)?;
    fs::write(out.join("rays.bin"), rays)?;
    fs::write(out.join("rays.f64"), rays_f64)?;
    let executable = out.join("probe");
    let start = Instant::now();
    command(
        "clang++",
        &[
            "-std=c++17",
            "-O2",
            "-fobjc-arc",
            out.join("probe.mm").to_str().context("non-UTF8 path")?,
            "-framework",
            "Foundation",
            "-framework",
            "Metal",
            "-o",
            executable.to_str().context("non-UTF8 path")?,
        ],
    )?;
    let driver_compile_ms = start.elapsed().as_secs_f64() * 1000.0;
    eprintln!(
        "Offline reference: {w}x{h}, safe three-term arithmetic. This can take tens of seconds per image."
    );
    let start = Instant::now();
    let pipeline: serde_json::Value = serde_json::from_str(&command(
        executable.to_str().unwrap(),
        &[
            out.join("reference.metal").to_str().unwrap(),
            out.join("rays.bin").to_str().unwrap(),
            out.join("samples.bin").to_str().unwrap(),
        ],
    )?)?;
    let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
    let raw = fs::read(out.join("samples.bin"))?;
    ensure!(raw.len() == (w * h) as usize * 32, "incomplete GPU result");
    let mut image = RgbImage::new(w, h);
    let (mut hits, mut stalls, mut exhausted) = (0, 0, 0);
    let mut depths = Vec::new();
    for (i, bytes) in raw.chunks_exact(32).enumerate() {
        let r: [f64; 8] = std::array::from_fn(|j| {
            f64::from(f32::from_le_bytes(
                bytes[j * 4..j * 4 + 4].try_into().unwrap(),
            ))
        });
        ensure!(
            r.iter().all(|v| v.is_finite()),
            "non-finite GPU result at pixel {i}"
        );
        hits += usize::from(r[2] != 0.0);
        stalls += usize::from(r[6] != 0.0);
        exhausted += usize::from(r[3] >= f64::from(scene.max_raymarching_steps));
        depths.extend((r[0] + r[1]).to_le_bytes());
        let shade = ((r[4] + r[5]).clamp(0.0, 1.0) * 255.0).round() as u8;
        image.put_pixel(
            i as u32 % w,
            i as u32 / w,
            Rgb(if r[2] != 0.0 {
                [shade; 3]
            } else {
                [30, 50, 75]
            }),
        );
    }
    image.save(out.join("reference.png"))?;
    fs::write(out.join("depth.f64"), depths)?;
    let report = json!({"scope":"scene-pinned offline white-headlight precision reference; not authored beauty or production support", "width":w,"height":h,"rays":w*h,"hits":hits,"stalls":stalls,"max_step_exits":exhausted,"healthy":stalls==0 && exhausted==0,
        "scene_sha256":hash(&source),"formula_sha256":hash(native.as_bytes()),"shader_sha256":hash(shader.as_bytes()),"arithmetic_sha256":hash(EXPANSION.as_bytes()),"driver_sha256":hash(DRIVER.as_bytes()),
        "camera_f64":scene.camera,"target_f64":scene.target,"top_f64":scene.camera_top,"fov_degrees":scene.fov_degrees,"detail_level":scene.detail_level,"iterations":scene.max_iterations,"formula_parameters":scene.formula_parameters,
        "driver_compile_ms":driver_compile_ms,"gpu_process_wall_ms":wall_ms,"pipeline":pipeline,"arguments":args});
    fs::write(
        out.join("summary.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    ensure!(
        stalls == 0 && exhausted == 0,
        "reference incomplete; see recorded stalls/step exits"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expansion_preserves_origin() {
        for v in [
            -1.87049551608698,
            -19.6377709180615,
            1.08136927634342,
            1e-12,
        ] {
            let s = split(v).unwrap();
            assert_eq!(s.into_iter().map(f64::from).sum::<f64>(), v);
        }
    }
    #[test]
    fn rejects_non_finite_and_overflow() {
        for v in [f64::NAN, f64::INFINITY, f64::MAX] {
            assert!(split(v).is_err());
        }
    }
    #[test]
    fn unsupported_source_fails_closed() {
        assert!(check_hash(b"changed scene", SCENE_SHA, "scene").is_err());
        assert!(check_hash(b"changed formula", FORMULA_SHA, "formula").is_err());
    }
    #[test]
    fn balanced_branch_extraction() {
        assert_eq!(
            block("before if (a) { if(b) {x;} } after", "if (a)").unwrap(),
            "if (a) { if(b) {x;} }"
        );
        assert!(block("if(a) {", "if(a)").is_err());
    }
    #[test]
    fn rejects_degenerate_camera() {
        assert!(normalize([0.0; 3]).is_err());
    }
}
