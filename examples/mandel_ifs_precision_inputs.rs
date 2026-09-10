//! Scene-pinned double-precision inputs using FPT's parser, without native linking.
use anyhow::{Context, Result, ensure};
use fpt_metal::MandelbulberScene;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt::Write, fs, path::Path};

const SCENE_SHA: &str = "170ca7e9d8e4f6e404de61df2c978532b738a024ad99c442b528bccf168ddf7f";
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn normalize(a: [f64; 3]) -> Result<[f64; 3]> {
    let length = a.iter().map(|v| v * v).sum::<f64>().sqrt();
    ensure!(length.is_finite() && length > 0., "invalid vector");
    Ok(a.map(|v| v / length))
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn rotation(degrees: [f64; 3]) -> Vec<f64> {
    let [z, y, x] = degrees.map(f64::to_radians);
    let (sz, cz) = z.sin_cos();
    let (sy, cy) = y.sin_cos();
    let (sx, cx) = x.sin_cos();
    vec![
        cz * cy,
        cz * sy * sx - sz * cx,
        cz * sy * cx + sz * sx,
        sz * cy,
        sz * sy * sx + cz * cx,
        sz * sy * cx - cz * sx,
        -sy,
        cy * sx,
        cy * cx,
    ]
}
fn values(s: &MandelbulberScene, key: &str, defaults: &[f64]) -> Result<Vec<f64>> {
    let Some(text) = s.formula_parameters.get(key) else {
        return Ok(defaults.to_vec());
    };
    let result: Vec<f64> = text
        .split_whitespace()
        .map(|v| v.replace(',', ".").parse())
        .collect::<std::result::Result<_, _>>()?;
    ensure!(
        result.len() == defaults.len() && result.iter().all(|x| x.is_finite()),
        "invalid {key}"
    );
    Ok(result)
}
fn flag(s: &MandelbulberScene, key: &str) -> Result<f64> {
    match s.formula_parameters.get(key).map(String::as_str) {
        None | Some("false") | Some("0") => Ok(0.),
        Some("true") | Some("1") => Ok(1.),
        _ => anyhow::bail!("invalid {key}"),
    }
}
fn parameters(s: &MandelbulberScene) -> Result<BTreeMap<String, Vec<f64>>> {
    let mut p = BTreeMap::new();
    for (key, value) in [
        ("camera", s.camera),
        ("target", s.target),
        ("top", s.camera_top),
        ("offset", s.ifs_offset),
    ] {
        p.insert(key.into(), value.to_vec());
    }
    let fov = 2. * (s.fov_degrees.to_radians() * 0.5).tan();
    p.insert(
        "controls".into(),
        vec![
            s.max_iterations as f64,
            1.,
            s.max_raymarching_steps as f64,
            s.de_factor,
            s.detail_level,
            fov,
            s.view_distance_max,
        ],
    );
    // These defaults and the preferred IFS finalizer are valid only for the pinned scene.
    p.insert("bailout".into(), vec![100., 0.]);
    p.insert("scale".into(), vec![s.ifs_scale]);
    p.insert("mainRot".into(), rotation(s.ifs_rotation));
    p.insert("edge".into(), values(s, "IFS_edge", &[0.; 3])?);
    p.insert(
        "flags".into(),
        [
            "IFS_abs_x",
            "IFS_abs_y",
            "IFS_abs_z",
            "IFS_rotation_enabled",
            "IFS_edge_enabled",
            "IFS_menger_sponge_mode",
        ]
        .iter()
        .map(|key| flag(s, key))
        .collect::<Result<Vec<_>>>()?,
    );
    for i in 0..9 {
        p.insert(
            format!("plane{i}"),
            vec![
                f64::from(s.ifs_enabled[i]),
                values(s, &format!("IFS_distance_{i}"), &[0.])?[0],
                values(s, &format!("IFS_intensity_{i}"), &[1.])?[0],
            ],
        );
        // Native normalizes at load and again when preparing formula matrices.
        p.insert(
            format!("plane{i}Direction"),
            normalize(s.ifs_directions[i])?.to_vec(),
        );
        p.insert(
            format!("plane{i}Rot"),
            rotation(
                values(s, &format!("IFS_rotations_{i}"), &[0.; 3])?
                    .try_into()
                    .unwrap(),
            ),
        );
    }
    Ok(p)
}
fn rays(s: &MandelbulberScene, width: u32, height: u32, center: bool) -> Result<Vec<[f64; 3]>> {
    let forward = normalize(std::array::from_fn(|i| s.target[i] - s.camera[i]))?;
    let right = normalize(cross(forward, s.camera_top))?;
    let up = normalize(cross(right, forward))?;
    let fov = 2. * (s.fov_degrees.to_radians() * 0.5).tan();
    let offset = if center { 0.5 } else { 0. };
    let mut result = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let u = (x as f64 + offset - width as f64 * 0.5) / height as f64 * fov;
            let v = (height as f64 * 0.5 - y as f64 - offset) / height as f64 * fov;
            result.push(normalize(std::array::from_fn(|i| {
                forward[i] + right[i] * u + up[i] * v
            }))?);
        }
    }
    Ok(result)
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 4,
        "usage: mandel_ifs_precision_inputs SCENE NEW_DIR WIDTHxHEIGHT center|native"
    );
    let raw = fs::read(&args[0])?;
    ensure!(
        hash(&raw) == SCENE_SHA,
        "only the validated IFS31_anim scene is supported"
    );
    let scene = MandelbulberScene::parse(std::str::from_utf8(&raw)?)?;
    let (w, h) = args[2].split_once('x').context("expected WIDTHxHEIGHT")?;
    let (w, h): (u32, u32) = (w.parse()?, h.parse()?);
    ensure!(
        w > 0 && h > 0 && w <= 320 && h <= 240,
        "invalid diagnostic size"
    );
    ensure!(
        matches!(args[3].as_str(), "center" | "native"),
        "invalid sampling"
    );
    let params = parameters(&scene)?;
    let directions = rays(&scene, w, h, args[3] == "center")?;
    let mut cfg = String::new();
    for (key, values) in &params {
        write!(cfg, "{key}")?;
        for v in values {
            write!(cfg, "\t{v:.17e}")?;
        }
        cfg.push('\n');
    }
    let mut ray_text = String::new();
    for [x, y, z] in directions {
        writeln!(ray_text, "{x:.17e}\t{y:.17e}\t{z:.17e}")?;
    }
    let out = Path::new(&args[1]);
    ensure!(!out.exists(), "refusing to overwrite inputs");
    fs::create_dir_all(out.parent().context("missing parent")?)?;
    fs::create_dir(out)?;
    fs::write(out.join("config.tsv"), &cfg)?;
    fs::write(out.join("rays.tsv"), &ray_text)?;
    fs::write(
        out.join("summary.json"),
        serde_json::to_vec_pretty(&json!({
            "provider":"FPT Rust parser; double camera and formula constants; no native process",
            "scene_sha256":hash(&raw),"width":w,"height":h,"pixel_sampling":args[3],
            "config_sha256":hash(cfg.as_bytes()),"rays_sha256":hash(ray_text.as_bytes()),
            "scope":"scene-pinned diagnostic; preferred IFS legacy distance migration; not general production support"
        }))?,
    )?;
    println!("{}", out.display());
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_rotation() {
        assert_eq!(rotation([0.; 3]), vec![1., 0., 0., 0., 1., 0., -0., 0., 1.]);
    }
    #[test]
    fn rotation_native_z60() {
        let m = rotation([60., 0., 0.]);
        assert_eq!(m[0], 0.5000000000000001);
        assert_eq!(m[1], -0.8660254037844386);
    }
    #[test]
    fn rejects_zero_vector() {
        assert!(normalize([0.; 3]).is_err());
    }
}
