//! Typed access to the production Metal scene renderer and artifact exporter.
//!
//! No subprocess invocation of `fpt-metal` and no alternate CPU Mandel evaluator
//! is used here. CLI and library calls share the same compiler and bridge.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

use crate::scene::{LoadedScene, RenderArgs, load_scene_config};
use crate::{Aabb, MandelbulberScene, runtime_impl};

/// Explicit source dependencies. Resource lookup never depends on the caller's
/// working directory or the historical developer checkout.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneSource {
    pub path: PathBuf,
    pub sha256: String,
    pub mandelbulber_root: PathBuf,
}

impl SceneSource {
    pub fn verify(&self) -> Result<()> {
        ensure!(self.path.is_absolute(), "scene path must be absolute");
        ensure!(
            self.mandelbulber_root.is_absolute(),
            "Mandelbulber root must be absolute"
        );
        ensure!(
            self.sha256.len() == 64 && self.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid source SHA-256"
        );
        ensure!(
            file_sha256(&self.path)? == self.sha256.to_ascii_lowercase(),
            "scene source hash mismatch"
        );
        ensure!(
            self.mandelbulber_root.is_dir(),
            "missing Mandelbulber resource root"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    Neutral,
    Authored,
}

/// A bounded offline render. The existing CLI defaults remain unchanged.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderRequest {
    pub source: SceneSource,
    pub output_directory: PathBuf,
    pub maximum_axis: u32,
    pub samples: u32,
    pub appearance: Appearance,
    pub bounces: Option<u32>,
    pub chunk_samples: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GeometryRequest {
    /// Field-sampled cubes throughout explicit finite bounds. This is not a
    /// certification of conservative occupancy or arbitrary-view completeness.
    BoundedCubes { resolution: u32, bounds: Aabb },
    /// Authored-camera-derived FPTVOX11 reference surfaces, not cube occupancy.
    AuthoredSurface {
        resolution: u32,
        maximum_axis: u32,
        path_bounces: u32,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportRequest {
    pub source: SceneSource,
    pub output: PathBuf,
    pub geometry: GeometryRequest,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArtifactReceipt {
    pub path: PathBuf,
    pub sha256: String,
    pub source_sha256: String,
    pub details: Value,
}

/// Prepared source and generated program. Private Metal configuration is not
/// part of the public contract. Preparing does not launch a render.
pub struct PreparedRender {
    args: RenderArgs,
    loaded: LoadedScene,
    source: SceneSource,
}

#[derive(Default)]
pub struct Runtime;

impl Runtime {
    pub fn prepare(&self, request: &RenderRequest) -> Result<PreparedRender> {
        request.source.verify()?;
        ensure!(
            (1..=8192).contains(&request.maximum_axis),
            "maximum axis must be 1..8192"
        );
        ensure!(
            (1..=512).contains(&request.samples),
            "samples must be 1..512"
        );
        ensure!(
            request.chunk_samples > 0
                && request.chunk_samples <= request.samples
                && request.chunk_samples <= 64,
            "invalid sample chunk"
        );
        ensure!(
            request.bounces.is_none_or(|b| (1..=32).contains(&b)),
            "invalid bounce count"
        );
        let scene = MandelbulberScene::load(&request.source.path)?;
        let size = aspect_size(request.maximum_axis, [scene.width, scene.height]);
        let mut args = RenderArgs::new(&request.source.path);
        args.mandelbulber_root = Some(request.source.mandelbulber_root.clone());
        args.out_dir = request.output_directory.clone();
        args.width = Some(size[0]);
        args.height = Some(size[1]);
        args.samples = Some(request.samples);
        args.sdf_chunk_samples = request.chunk_samples;
        args.sdf_bounce_cap = request.bounces;
        args.mandel_authored_path = matches!(request.appearance, Appearance::Authored);
        let loaded = load_scene_config(&args)?;
        Ok(PreparedRender {
            args,
            loaded,
            source: request.source.clone(),
        })
    }

    pub fn render(&self, prepared: PreparedRender) -> Result<ArtifactReceipt> {
        prepared.source.verify()?;
        let path = prepared.args.out_dir.join(&prepared.loaded.output_name);
        ensure!(!path.exists(), "render output already exists");
        runtime_impl::render_loaded(&prepared.args, prepared.loaded)?;
        let details = serde_json::from_slice(&fs::read(PathBuf::from(format!(
            "{}.render.json",
            path.display()
        )))?)?;
        Ok(ArtifactReceipt {
            sha256: file_sha256(&path)?,
            path,
            source_sha256: prepared.source.sha256,
            details,
        })
    }

    pub fn export(&self, request: &ExportRequest) -> Result<ArtifactReceipt> {
        request.source.verify()?;
        ensure!(!request.output.exists(), "export output already exists");
        ensure!(
            request.output.extension().is_some_and(|e| e == "fptvox"),
            "library export requires .fptvox"
        );
        let mut options = runtime_impl::ExportOptions {
            scene: request
                .source
                .path
                .to_str()
                .context("non-UTF8 scene path")?
                .to_owned(),
            output: Some(request.output.clone()),
            mandelbulber_root: Some(request.source.mandelbulber_root.clone()),
            ..Default::default()
        };
        match request.geometry {
            GeometryRequest::BoundedCubes { resolution, bounds } => {
                ensure!(
                    (1..=512).contains(&resolution),
                    "cube resolution must be 1..512"
                );
                ensure!(
                    (0..3).all(|i| bounds.min[i].is_finite()
                        && bounds.max[i].is_finite()
                        && bounds.min[i] < bounds.max[i]),
                    "invalid bounds"
                );
                options.resolution = Some(resolution);
                options.bounds_min = Some(bounds.min);
                options.bounds_max = Some(bounds.max);
            }
            GeometryRequest::AuthoredSurface {
                resolution,
                maximum_axis,
                path_bounces,
            } => {
                ensure!(
                    (1..=512).contains(&resolution),
                    "surface output resolution must be 1..512"
                );
                ensure!(
                    (32..=1024).contains(&maximum_axis),
                    "surface capture axis must be 32..1024"
                );
                ensure!(path_bounces <= 3, "surface path bounces must be 0..3");
                options.resolution = Some(resolution);
                options.surface_triangles = true;
                options.surface_view_triangles = true;
                options.surface_view_indexed_triangle_bvh = true;
                options.surface_triangle_resolution = Some(maximum_axis);
                options.surface_view_splats = true;
                options.surface_view_ray_consistent_splats = true;
                options.surface_view_fit_bounds = true;
                options.surface_view_path_bounces = path_bounces;
                options.surface_view_path_ray_consistent_splats = path_bounces > 0;
            }
        }
        let details = runtime_impl::export(options)?;
        Ok(ArtifactReceipt {
            path: request.output.clone(),
            sha256: file_sha256(&request.output)?,
            source_sha256: request.source.sha256.clone(),
            details,
        })
    }
}

/// One relocatable asset package. The committed manifest is written last;
/// incomplete builds never look like loadable bundles.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleRequest {
    pub source: SceneSource,
    pub output_directory: PathBuf,
    pub geometry: GeometryRequest,
}

impl Runtime {
    pub fn build_bundle(&self, request: &BundleRequest) -> Result<PathBuf> {
        request.source.verify()?;
        ensure!(
            !request.output_directory.exists(),
            "bundle directory already exists"
        );
        let scene = MandelbulberScene::load(&request.source.path)?;
        // Resolve generated lights using precisely the same preparation as rendering.
        let prepared = self.prepare(&RenderRequest {
            source: request.source.clone(),
            output_directory: request.output_directory.clone(),
            maximum_axis: 300,
            samples: 32,
            appearance: Appearance::Authored,
            bounces: None,
            chunk_samples: 1,
        })?;
        fs::create_dir_all(&request.output_directory)?;
        let output = request.output_directory.join("geometry.fptvox");
        let receipt = self.export(&ExportRequest {
            source: request.source.clone(),
            output: output.clone(),
            geometry: request.geometry.clone(),
        })?;
        let auxiliary = prepared.loaded.mandel_auxiliary.as_ref();
        let mut limitations = vec![
            "NAADF visual acceptance requires a separate image review".to_owned(),
            "Geometry-normal and transport differences are not certified by export success"
                .to_owned(),
        ];
        let mut required_extensions = Vec::new();
        if let Some(aux) = auxiliary {
            if !aux.directional.is_empty() || !aux.point.is_empty() {
                required_extensions.push("auxiliary_light_table");
            }
            if aux.point.len() + aux.directional.len() > 64 {
                required_extensions.push("auxiliary_lights_over_64");
            }
            if aux.fake.is_some() {
                required_extensions.push("orbit_trap_lighting");
            }
            if aux
                .point
                .iter()
                .any(|l| l.cast_shadows && l.cone_radians > 0.0)
                || aux
                    .directional
                    .iter()
                    .any(|l| l.cast_shadows && (l.cone_radians > 0.0 || l.penetrating))
            {
                required_extensions.push("distance_field_light_shadows");
                limitations.push("NAADF geometric shadows approximate FPT distance-field soft/penetrating shadows".to_owned());
            }
            limitations.extend(
                aux.unsupported
                    .iter()
                    .map(|s| format!("FPT unsupported light: {s}")),
            );
        }
        let geometry_scope = match request.geometry {
            GeometryRequest::BoundedCubes { .. } => "bounded_field_samples",
            GeometryRequest::AuthoredSurface { .. } => {
                limitations.push(
                    "View-derived source may omit unseen and secondary-ray geometry".to_owned(),
                );
                "authored_view"
            }
        };
        let environment = scene.fptvox_environment()?;
        let semantics = serde_json::json!({
            "version":1, "camera":scene.fptvox_camera(), "appearance":scene.fptvox_appearance(),
            "materials":scene.fptvox_materials(), "auxiliary_lighting":auxiliary,
            "environment":{"flags":environment.flags,"map_type":environment.hdri_map_type,
                "values":environment.values.to_vec(),"lut":"embedded in geometry.fptvox FPTENV2"},
            "authored_size":[scene.width,scene.height], "geometry_scope":geometry_scope,
            "required_consumer_extensions":required_extensions, "limitations":limitations,
        });
        let semantics_path = request.output_directory.join("scene.json");
        fs::write(&semantics_path, serde_json::to_vec_pretty(&semantics)?)?;
        let manifest = serde_json::json!({
            "format":"fpt_scene_bundle", "version":1,
            "source":{"sha256":request.source.sha256,"name":request.source.path.file_name().and_then(|n|n.to_str())},
            "generator":{"crate_version":env!("CARGO_PKG_VERSION"),"metal_compiler":env!("FPT_METAL_COMPILER_IDENTITY")},
            "geometry_request":request.geometry, "geometry_scope":geometry_scope,
            "artifacts":{
                "geometry":{"path":"geometry.fptvox","sha256":receipt.sha256,"bytes":fs::metadata(&output)?.len()},
                "scene":{"path":"scene.json","sha256":file_sha256(&semantics_path)?,"bytes":fs::metadata(&semantics_path)?.len()},
            },
            "export":receipt.details,
            "render_defaults":{"maximum_axis":300,"samples":32,"bounces":4},
            "validation":{"exported":true,"naadf_rendered":false,"naadf_accepted":false},
        });
        let manifest_path = request.output_directory.join("manifest.json");
        fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
        Ok(manifest_path)
    }
}

pub fn file_sha256(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = fs::File::open(path).with_context(|| format!("read {}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn aspect_size(maximum_axis: u32, authored: [u32; 2]) -> [u32; 2] {
    let largest = authored[0].max(authored[1]).max(1) as f64;
    authored.map(|v| ((v as f64 * maximum_axis as f64 / largest).round() as u32).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn library_requests_reject_unknown_settings() {
        assert!(serde_json::from_value::<GeometryRequest>(serde_json::json!({"kind":"bounded_cubes","resolution":64,"bounds":{"min":[-1.,-1.,-1.],"max":[1.,1.,1.]},"silent_approximation":true})).is_err());
    }
    #[test]
    fn authored_aspect_is_preserved() {
        assert_eq!(aspect_size(300, [1920, 1080]), [300, 169]);
        assert_eq!(aspect_size(300, [1080, 1920]), [169, 300]);
    }
}
