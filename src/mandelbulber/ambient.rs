//! Authored multi-ray ambient lighting. Assets stay external; the sampled table
//! is part of the generated source and therefore of the metallib cache key.
use super::{
    MandelbulberScene, map_mandel_vector, parse_rgb16, parse_vec3, resolve_mandelbulber_asset,
};
use anyhow::{Context, Result, ensure};
use image::DynamicImage;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fmt::Write,
    fs,
    path::{Path, PathBuf},
};

const MARKER: &str = "// FPT_MANDELBULBER_GENERATED_INSERTION_POINT";
const MAX_DIRECTIONS: usize = 10_000;
pub const MAX_VISIBILITY_STEPS: u32 = 4096;

#[derive(Clone, Debug, Serialize)]
pub struct AmbientMetadata {
    pub mode: &'static str,
    pub lightmap_path: PathBuf,
    pub lightmap_sha256: String,
    pub lightmap_size: [u32; 2],
    pub rotation_degrees: [f64; 3],
    pub quality: u32,
    pub strength: f64,
    pub tint: [f32; 3],
    pub direction_count: usize,
    pub range_per_camera_distance: f64,
    pub lighting_policy: &'static str,
    pub exhaustion_policy: &'static str,
    pub max_visibility_steps: u32,
}

#[derive(Clone, Debug)]
pub struct AmbientLighting {
    pub metadata: AmbientMetadata,
    pub directions: Vec<AmbientDirection>,
}

#[derive(Clone, Debug)]
pub struct AmbientDirection {
    pub direction: [f32; 3],
    pub color: [f32; 3],
}

impl AmbientLighting {
    pub fn load(scene: &MandelbulberScene, source_root: Option<&Path>) -> Result<Option<Self>> {
        if !scene.ambient_occlusion_enabled || scene.ambient_occlusion_mode != 1 {
            return Ok(None);
        }
        ensure!(
            !scene.iteration_threshold_mode,
            "authored multi-ray AO does not yet support iteration-threshold distance evaluation"
        );
        ensure!(
            scene
                .main_parameters
                .get("iteration_fog_enable")
                .is_none_or(|v| v == "false"),
            "authored multi-ray AO does not yet support iteration fog"
        );
        let path = match scene.main_parameters.get("file_lightmap") {
            Some(value) => resolve_mandelbulber_asset(scene.source_directory.as_deref(), value),
            None => source_root
                .context("authored multi-ray AO needs file_lightmap or --mandelbulber-root")?
                .join("deploy/share/mandelbulber2/textures/lightmap.jpg"),
        };
        let path = path
            .canonicalize()
            .with_context(|| format!("missing authored AO lightmap {}", path.display()))?;
        let bytes = fs::read(&path)
            .with_context(|| format!("reading authored AO lightmap {}", path.display()))?;
        let image = image::load_from_memory(&bytes).context("decoding authored AO lightmap")?;
        let rotation = scene
            .main_parameters
            .get("ao_light_map_rotation")
            .map(|v| {
                if v == "0" || v == "0,0" || v == "0.0" {
                    Ok([0.0; 3])
                } else {
                    parse_vec3(v)
                }
            })
            .transpose()?
            .unwrap_or([0.0; 3]);
        let tint = scene
            .main_parameters
            .get("ambient_occlusion_color")
            .map(|v| parse_rgb16(v))
            .transpose()?
            .unwrap_or([1.0; 3]);
        ensure!(
            rotation.iter().all(|v| v.is_finite())
                && scene.ambient_occlusion.is_finite()
                && scene.ambient_occlusion >= 0.0,
            "invalid authored AO rotation/strength"
        );
        let directions = sample_directions(&image, scene.ambient_occlusion_quality, rotation)?;
        let fov = scene.fov_degrees.to_radians();
        let span = match scene.camera_projection {
            0 => 2.0 * (fov * 0.5).tan(),
            2 => fov * 0.5,
            _ => fov,
        };
        ensure!(
            span.is_finite() && span > 0.0,
            "invalid authored AO camera range"
        );
        Ok(Some(Self {
            metadata: AmbientMetadata {
                mode: "authored-multi-ray-lightmap",
                lightmap_path: path,
                lightmap_sha256: format!("{:x}", Sha256::digest(&bytes)),
                lightmap_size: [image.width(), image.height()],
                rotation_degrees: rotation,
                quality: scene.ambient_occlusion_quality,
                strength: scene.ambient_occlusion,
                tint,
                direction_count: directions.len(),
                range_per_camera_distance: span,
                lighting_policy: "primary-hit compatibility AO; replaces generic ambient and secondary environment; direct/indirect sun retained",
                exhaustion_policy: "invalid/stalled/exhausted visibility is a magenta diagnostic, never assumed clear",
                max_visibility_steps: MAX_VISIBILITY_STEPS,
            },
            directions,
        }))
    }

    pub fn specialize(&self, source: &str) -> Result<String> {
        ensure!(
            source.matches(MARKER).count() == 1,
            "missing/duplicate Mandel source insertion marker"
        );
        let mut fragment = format!(
            "#define FPT_MANDEL_GENERATED_AMBIENT 1\n// AO asset SHA256: {}\n",
            self.metadata.lightmap_sha256
        );
        writeln!(
            fragment,
            "constant uint mandelAmbientCount = {}u;",
            self.directions.len()
        )?;
        writeln!(
            fragment,
            "constant uint mandelAmbientMaxSteps = {}u;",
            MAX_VISIBILITY_STEPS
        )?;
        writeln!(
            fragment,
            "constant float mandelAmbientRange = {:.9e}f;",
            self.metadata.range_per_camera_distance as f32
        )?;
        let tint = self
            .metadata
            .tint
            .map(|v| v * self.metadata.strength as f32);
        writeln!(
            fragment,
            "constant float3 mandelAmbientTint = float3({:.9e}f,{:.9e}f,{:.9e}f);",
            tint[0], tint[1], tint[2]
        )?;
        for (name, color) in [
            ("mandelAmbientDirections", false),
            ("mandelAmbientColors", true),
        ] {
            writeln!(
                fragment,
                "constant float3 {name}[{}] = {{",
                self.directions.len()
            )?;
            for entry in &self.directions {
                let v = if color { entry.color } else { entry.direction };
                writeln!(
                    fragment,
                    "float3({:.9e}f,{:.9e}f,{:.9e}f),",
                    v[0], v[1], v[2]
                )?;
            }
            fragment.push_str("};\n");
        }
        Ok(source.replace(MARKER, &format!("{fragment}\n{MARKER}")))
    }
}

fn texture_pixels(image: &DynamicImage) -> Vec<[f32; 3]> {
    // Mandel textures do not apply an sRGB transfer function. Integer textures
    // use 2^bit_depth divisors, including PNG16; HDR retains floating values.
    match image {
        DynamicImage::ImageRgb32F(_) | DynamicImage::ImageRgba32F(_) => {
            image.to_rgb32f().pixels().map(|p| p.0).collect()
        }
        DynamicImage::ImageLuma16(_)
        | DynamicImage::ImageLumaA16(_)
        | DynamicImage::ImageRgb16(_)
        | DynamicImage::ImageRgba16(_) => image
            .to_rgb16()
            .pixels()
            .map(|p| p.0.map(|v| v as f32 / 65536.0))
            .collect(),
        _ => image
            .to_rgb8()
            .pixels()
            .map(|p| p.0.map(|v| v as f32 / 256.0))
            .collect(),
    }
}

fn rotate_native(mut v: [f64; 3], degrees: [f64; 3]) -> [f32; 3] {
    // Native matrix is Rz(alpha) * Rx(beta) * Ry(gamma).
    let [alpha, beta, gamma] = degrees.map(f64::to_radians);
    v = [
        gamma.cos() * v[0] + gamma.sin() * v[2],
        v[1],
        -gamma.sin() * v[0] + gamma.cos() * v[2],
    ];
    v = [
        v[0],
        beta.cos() * v[1] - beta.sin() * v[2],
        beta.sin() * v[1] + beta.cos() * v[2],
    ];
    v = [
        alpha.cos() * v[0] - alpha.sin() * v[1],
        alpha.sin() * v[0] + alpha.cos() * v[1],
        v[2],
    ];
    map_mandel_vector(v).map(|v| v as f32)
}

fn sample_directions(
    image: &DynamicImage,
    quality: u32,
    rotation: [f64; 3],
) -> Result<Vec<AmbientDirection>> {
    ensure!(
        (1..=39).contains(&quality),
        "authored AO quality must be 1..39 (no truncated direction tables)"
    );
    let pixels = texture_pixels(image);
    ensure!(
        pixels.iter().flatten().all(|v| v.is_finite() && *v >= 0.0),
        "AO lightmap must have finite non-negative pixels"
    );
    let (w, h) = (image.width() as usize, image.height() as usize);
    ensure!(w > 0 && h > 0, "AO lightmap is empty");
    let mut result = Vec::new();
    let pi = std::f64::consts::PI;
    let mut latitude = -0.49 * pi;
    while latitude < 0.49 * pi {
        let mut longitude = 0.0;
        while longitude < 2.0 * pi {
            let azimuth = longitude + latitude;
            let x = (azimuth / (2.0 * pi) * w as f64 + w as f64 * 8.5) as usize % w;
            let y = (latitude / pi * h as f64 + h as f64 * 8.5) as usize % h;
            let color = pixels[y * w + x];
            if color.iter().any(|v| *v > 0.001) {
                result.push(AmbientDirection {
                    direction: rotate_native(
                        [
                            azimuth.cos() * latitude.cos(),
                            azimuth.sin() * latitude.cos(),
                            latitude.sin(),
                        ],
                        rotation,
                    ),
                    color,
                });
                ensure!(
                    result.len() <= MAX_DIRECTIONS,
                    "AO direction table exceeds 10000 samples"
                );
            }
            longitude += 2.0 / quality as f64 / latitude.cos();
        }
        latitude += 1.0 / quality as f64;
    }
    if result.is_empty() {
        result.push(AmbientDirection {
            direction: [0.0; 3],
            color: [0.0; 3],
        });
    }
    Ok(result)
}

/// Deterministic scalar oracle for the shader's visibility contract.
#[cfg(test)]
fn visibility(threshold: f32, end: f32, mut distance: impl FnMut(f32) -> f32) -> Option<f32> {
    if !threshold.is_finite() || threshold <= 0.0 || !end.is_finite() || end <= 0.0 {
        return None;
    }
    let mut r = threshold;
    for _ in 0..MAX_VISIBILITY_STEPS {
        if r >= end {
            return Some(1.0);
        }
        let d = distance(r);
        if !d.is_finite() {
            return None;
        }
        if d < threshold {
            return Some((r / end).clamp(0.0, 1.0));
        }
        let next = r + d * 2.0;
        if !next.is_finite() || next <= r {
            return None;
        }
        r = next;
    }
    if r >= end { Some(1.0) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assets_are_explicit_hashed_and_only_required_for_multi_ray() {
        let mut scene = MandelbulberScene::parse("# Mandelbulber settings file\n# version 2.33\n[main_parameters]\nformula_1 10;\ncamera 0 -3 0;\ntarget 0 0 0;\n").unwrap();
        assert!(AmbientLighting::load(&scene, None).unwrap().is_none());
        scene.ambient_occlusion_enabled = true;
        scene.ambient_occlusion_mode = 1;
        assert!(AmbientLighting::load(&scene, None).is_err());
        let directory = std::env::temp_dir().join(format!(
            "fpt-ambient-assets-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(directory.clone());
        scene.source_directory = Some(directory.clone());
        scene
            .main_parameters
            .insert("file_lightmap".into(), "map.png".into());
        assert!(AmbientLighting::load(&scene, None).is_err());
        let path = directory.join("map.png");
        image::RgbImage::from_pixel(4, 4, image::Rgb([128u8, 64, 32]))
            .save(&path)
            .unwrap();
        let first = AmbientLighting::load(&scene, None).unwrap().unwrap();
        assert_eq!(first.metadata.lightmap_path, path.canonicalize().unwrap());
        assert_eq!(
            first.metadata.lightmap_sha256,
            format!("{:x}", Sha256::digest(fs::read(&path).unwrap()))
        );
        let source = first.specialize(MARKER).unwrap();
        assert_eq!(
            source,
            AmbientLighting::load(&scene, None)
                .unwrap()
                .unwrap()
                .specialize(MARKER)
                .unwrap()
        );
        image::RgbImage::from_pixel(4, 4, image::Rgb([64u8, 128, 32]))
            .save(&path)
            .unwrap();
        let second = AmbientLighting::load(&scene, None).unwrap().unwrap();
        assert_ne!(
            first.metadata.lightmap_sha256,
            second.metadata.lightmap_sha256
        );
        assert_ne!(source, second.specialize(MARKER).unwrap());
        assert!(second.specialize("no marker").is_err());
        scene
            .main_parameters
            .insert("iteration_fog_enable".into(), "true".into());
        assert!(AmbientLighting::load(&scene, None).is_err());
        scene.main_parameters.remove("iteration_fog_enable");
        scene.iteration_threshold_mode = true;
        assert!(AmbientLighting::load(&scene, None).is_err());
        scene.iteration_threshold_mode = false;
        scene.camera_projection = 2;
        scene.fov_degrees = 360.0;
        assert!(
            (AmbientLighting::load(&scene, None)
                .unwrap()
                .unwrap()
                .metadata
                .range_per_camera_distance
                - std::f64::consts::PI)
                .abs()
                < 1e-12
        );
        scene.camera_projection = 1;
        scene.fov_degrees = 180.0;
        assert!(
            (AmbientLighting::load(&scene, None)
                .unwrap()
                .unwrap()
                .metadata
                .range_per_camera_distance
                - std::f64::consts::PI)
                .abs()
                < 1e-12
        );
    }

    #[test]
    fn texture_bit_depth_and_quality_contract() {
        let image = DynamicImage::ImageRgb16(image::ImageBuffer::from_pixel(
            2,
            2,
            image::Rgb([32768u16, 16384, 8192]),
        ));
        assert_eq!(texture_pixels(&image)[0], [0.5, 0.25, 0.125]);
        let hdr = DynamicImage::ImageRgb32F(image::Rgb32FImage::from_pixel(
            2,
            2,
            image::Rgb([2.0, 0.5, 0.0]),
        ));
        assert_eq!(texture_pixels(&hdr)[0], [2.0, 0.5, 0.0]);
        assert!(sample_directions(&image, 0, [0.0; 3]).is_err());
        assert!(sample_directions(&image, 40, [0.0; 3]).is_err());
    }

    #[test]
    fn constant_black_and_directional_maps() {
        let image =
            DynamicImage::ImageRgb8(image::RgbImage::from_pixel(8, 4, image::Rgb([128, 64, 32])));
        let entries = sample_directions(&image, 4, [0.0; 3]).unwrap();
        assert_eq!(entries.len(), 108);
        assert!(entries.iter().all(|e| e.color == [0.5, 0.25, 0.125]));
        let rotated = sample_directions(&image, 4, [90.0, 0.0, 0.0]).unwrap();
        for (a, b) in entries.iter().zip(rotated) {
            assert_eq!(a.color, b.color);
            assert!((a.direction[0] - b.direction[2]).abs() < 1e-6);
            assert!((a.direction[2] + b.direction[0]).abs() < 1e-6);
        }
        let black = sample_directions(&DynamicImage::new_rgb8(2, 2), 4, [0.0; 3]).unwrap();
        assert_eq!(black.len(), 1);
        assert_eq!(black[0].color, [0.0; 3]);
        let mut directional = image::RgbImage::new(8, 4);
        directional.put_pixel(4, 0, image::Rgb([255, 0, 0]));
        let samples =
            sample_directions(&DynamicImage::ImageRgb8(directional), 4, [0.0; 3]).unwrap();
        assert!(!samples.is_empty());
        assert!(
            samples
                .iter()
                .all(|s| s.direction[1] < 0.0 && s.color == [255.0 / 256.0, 0.0, 0.0])
        );
    }

    #[test]
    fn scalar_visibility_open_blocked_scaled_and_exhausted() {
        for scale in [0.001, 1.0, 1000.0] {
            assert_eq!(
                visibility(scale, 100.0 * scale, |_| 100.0 * scale),
                Some(1.0)
            );
            assert!((visibility(scale, 100.0 * scale, |_| 0.0).unwrap() - 0.01).abs() < 1e-6);
            assert!(
                (visibility(scale, 100.0 * scale, |r| (10.0 * scale - r).max(0.0)).unwrap() - 0.19)
                    .abs()
                    < 1e-6
            );
        }
        assert_eq!(visibility(1.0, 1e9, |_| 1.0), None);
        assert_eq!(visibility(1.0, 100.0, |_| f32::NAN), None);
        assert_eq!(visibility(0.0, 100.0, |_| 1.0), None);
    }
}
