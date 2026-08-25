use anyhow::{Context, Result, anyhow, bail, ensure};
use fpt_metal::fptvox7::{
    MeshSurfaceVertex, aspect_resolutions, build_triangle_surface_from_normalized_mesh,
    build_triangle_surface_from_normalized_mesh_3d,
};
use fpt_metal::{
    Aabb, CoordinateSystem, FptvoxTriangleSurface, SparseVoxel, SurfaceMaterial, VoxelCell,
    VoxelGrid,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const NORMAL_CLUSTER_COSINE: f32 = 0.94;
const AUTO_BOUNDS_MAX_SECONDARY_PATCH_RATIO: f64 = 0.50;
const MANDELBULBER_OPENCL_DEVICE_TYPE_GPU: &str = "opencl_device_type=0";
const MANDELBULBER_OPENCL_MODE_FULL: &str = "opencl_mode=3";
const MANDELBULBER_OPENCL_PRECISION_SINGLE: &str = "opencl_precision=0";
static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct MandelMeshOptions<'a> {
    pub binary: Option<&'a Path>,
    pub scene: &'a Path,
    pub ply_input: Option<&'a Path>,
    pub raw_ply_output: Option<&'a Path>,
    pub bounds: Aabb,
    pub voxel_resolution: u32,
    pub voxel_resolution_3d: Option<[u32; 3]>,
    pub voxel_min_axis_resolution: Option<u32>,
    pub voxel_dilation: u32,
    pub mesh_resolution: u32,
    pub max_iterations: u32,
    pub use_opencl: bool,
    pub roughness: f32,
    pub specular: f32,
    pub emission: f32,
    pub auto_bounds: bool,
    pub auto_bounds_margin: f32,
    pub surface_triangles: bool,
    pub voxel_cells_only: bool,
}

pub fn cubic_voxel_resolutions(bounds: Aabb, minimum_axis_resolution: u32) -> [u32; 3] {
    let size = bounds.size();
    let minimum_span = size.into_iter().fold(f32::INFINITY, f32::min);
    let cell_size = minimum_span / minimum_axis_resolution.max(1) as f32;
    std::array::from_fn(|axis| (size[axis] / cell_size).ceil().max(1.0) as u32)
}

/// Expand an interior-camera volume just enough to keep the camera away from
/// every clipping plane. Exterior object cameras intentionally leave the
/// supplied bounds unchanged.
pub fn camera_safe_bounds(bounds: Aabb, camera: [f32; 3], margin: f32) -> Aabb {
    let inside =
        (0..3).all(|axis| camera[axis] >= bounds.min[axis] && camera[axis] <= bounds.max[axis]);
    if !inside {
        return bounds;
    }
    let padding = bounds.size().into_iter().fold(0.0_f32, f32::max) * margin.max(0.0);
    Aabb::new(
        std::array::from_fn(|axis| bounds.min[axis].min(camera[axis] - padding)),
        std::array::from_fn(|axis| bounds.max[axis].max(camera[axis] + padding)),
    )
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct MandelMeshAutoBoundsSummary {
    pub requested: bool,
    pub accepted: bool,
    pub reason: String,
    pub margin: f32,
    pub original_bounds: Aabb,
    pub discovery_mesh_bounds: Option<Aabb>,
    pub candidate_bounds: Option<Aabb>,
    pub discovery_boundary_cells: usize,
    pub candidate_boundary_cells: Option<usize>,
    pub candidate_secondary_patch_ratio: Option<f64>,
    pub max_secondary_patch_ratio: f64,
    pub discovery_pass_ms: f64,
    pub candidate_pass_ms: Option<f64>,
    pub total_pass_ms: f64,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct MandelMeshSummary {
    pub mesh_export_ms: f64,
    pub ply_parse_ms: f64,
    pub voxelize_ms: f64,
    pub ply_bytes: u64,
    pub mesh_vertices: usize,
    pub mesh_triangles: usize,
    pub discarded_nonlocal_triangles: usize,
    pub discarded_out_of_bounds_triangles: usize,
    pub triangle_cell_tests: u64,
    pub triangle_cell_intersections: u64,
    pub occupied_cells: usize,
    pub cells_with_secondary_patch: usize,
    pub discarded_patch_clusters: u64,
    pub mesh_bounds: Aabb,
}

pub struct MandelMeshVoxelization {
    pub grid: VoxelGrid,
    pub patches: Vec<[u32; 3]>,
    pub triangle_surface: Option<FptvoxTriangleSurface>,
    pub summary: MandelMeshSummary,
    pub auto_bounds: MandelMeshAutoBoundsSummary,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct MandelReferenceSummary {
    pub output: PathBuf,
    pub width: u32,
    pub height: u32,
    pub render_ms: f64,
    pub opencl: bool,
}

pub fn render_mandelbulber_reference(
    binary: &Path,
    scene: &Path,
    output: &Path,
    width: u32,
    height: u32,
    use_opencl: bool,
) -> Result<MandelReferenceSummary> {
    ensure!(
        binary.is_file(),
        "Mandelbulber binary does not exist: {}",
        binary.display()
    );
    ensure!(
        scene.is_file(),
        "Mandelbulber scene does not exist: {}",
        scene.display()
    );
    ensure!(
        width > 0 && height > 0,
        "Mandelbulber reference size must be nonzero"
    );
    ensure!(
        output
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("png")),
        "Mandelbulber reference output must use a .png extension"
    );
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }

    let started = Instant::now();
    let mut command = Command::new(binary);
    command.arg("-n").arg("-C");
    if use_opencl {
        command.arg("-g");
    }
    let result = command
        .arg("-f")
        .arg("png")
        .arg("-r")
        .arg(format!("{width}x{height}"))
        .arg("-o")
        .arg(output)
        .arg(scene)
        .output()
        .with_context(|| format!("run Mandelbulber reference renderer {}", binary.display()))?;
    if !result.status.success() {
        bail!(
            "Mandelbulber reference render failed with {}: {}{}",
            result.status,
            String::from_utf8_lossy(&result.stderr).trim(),
            String::from_utf8_lossy(&result.stdout).trim()
        );
    }
    ensure!(
        output.is_file(),
        "Mandelbulber completed without producing {}",
        output.display()
    );
    Ok(MandelReferenceSummary {
        output: output.to_path_buf(),
        width,
        height,
        render_ms: started.elapsed().as_secs_f64() * 1000.0,
        opencl: use_opencl,
    })
}

#[derive(Clone, Copy, Debug)]
struct MeshVertex {
    position: [f32; 3],
    color: [f32; 3],
}

#[derive(Clone, Debug)]
struct PlyMesh {
    vertices: Vec<MeshVertex>,
    triangles: Vec<[u32; 3]>,
}

fn discard_nonlocal_marching_cubes_triangles(
    mesh: &mut PlyMesh,
    bounds: Aabb,
    mesh_resolution: u32,
) -> usize {
    let step = bounds.size().into_iter().fold(0.0_f32, f32::max) / mesh_resolution as f32;
    let maximum_edge_squared = 3.0 * step * step * 1.05 * 1.05;
    let original_count = mesh.triangles.len();
    mesh.triangles.retain(|triangle| {
        let vertices = triangle.map(|index| mesh.vertices[index as usize].position);
        [(0, 1), (1, 2), (2, 0)].into_iter().all(|(first, second)| {
            subtract(vertices[first], vertices[second])
                .into_iter()
                .map(|value| value * value)
                .sum::<f32>()
                <= maximum_edge_squared
        })
    });
    original_count - mesh.triangles.len()
}

fn discard_out_of_bounds_triangles(mesh: &mut PlyMesh, bounds: Aabb) -> usize {
    let original_count = mesh.triangles.len();
    mesh.triangles.retain(|triangle| {
        let vertices = triangle.map(|index| mesh.vertices[index as usize].position);
        (0..3).all(|axis| {
            let minimum = vertices
                .iter()
                .map(|vertex| vertex[axis])
                .fold(f32::INFINITY, f32::min);
            let maximum = vertices
                .iter()
                .map(|vertex| vertex[axis])
                .fold(f32::NEG_INFINITY, f32::max);
            maximum >= bounds.min[axis] && minimum <= bounds.max[axis]
        })
    });
    original_count - mesh.triangles.len()
}

fn mesh_bounds(mesh: &PlyMesh) -> Result<Aabb> {
    ensure!(
        !mesh.triangles.is_empty(),
        "cannot derive bounds from an empty mesh"
    );
    let min = std::array::from_fn(|axis| {
        mesh.triangles
            .iter()
            .flat_map(|triangle| triangle.iter())
            .map(|index| mesh.vertices[*index as usize].position[axis])
            .fold(f32::INFINITY, f32::min)
    });
    let max = std::array::from_fn(|axis| {
        mesh.triangles
            .iter()
            .flat_map(|triangle| triangle.iter())
            .map(|index| mesh.vertices[*index as usize].position[axis])
            .fold(f32::NEG_INFINITY, f32::max)
    });
    ensure!(
        (0..3).all(|axis| min[axis].is_finite() && max[axis].is_finite() && min[axis] < max[axis]),
        "Mandelbulber mesh bounds are invalid"
    );
    Ok(Aabb::new(min, max))
}

fn derive_centered_bounds(mesh: Aabb, margin: f32) -> Option<Aabb> {
    let size = mesh.size();
    let side = size.into_iter().fold(0.0_f32, f32::max) * (1.0 + margin);
    if !side.is_finite() || side <= 0.0 {
        return None;
    }
    let center: [f32; 3] = std::array::from_fn(|axis| (mesh.min[axis] + mesh.max[axis]) * 0.5);
    let half = side * 0.5;
    let candidate = Aabb::new(
        center.map(|value| value - half),
        center.map(|value| value + half),
    );
    Some(candidate)
}

fn derive_auto_bounds(mesh: Aabb, original: Aabb, margin: f32) -> Option<Aabb> {
    let candidate = derive_centered_bounds(mesh, margin)?;
    (0..3)
        .all(|axis| {
            candidate.min[axis] > original.min[axis] && candidate.max[axis] < original.max[axis]
        })
        .then_some(candidate)
}

fn boundary_cell_count(grid: &VoxelGrid) -> usize {
    grid.voxels
        .iter()
        .filter(|voxel| {
            (0..3).any(|axis| {
                voxel.coordinate[axis] == 0 || voxel.coordinate[axis] + 1 == grid.resolution[axis]
            })
        })
        .count()
}

fn secondary_patch_ratio(secondary_cells: usize, occupied_cells: usize) -> f64 {
    secondary_cells as f64 / occupied_cells.max(1) as f64
}

struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn create() -> Result<Self> {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "fpt-mandel-mesh-{}-{suffix}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).with_context(|| format!("create {}", path.display()))?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone, Copy)]
struct ClipVertex {
    position: [f32; 3],
    color: [f32; 3],
}

#[derive(Clone, Copy)]
struct PatchCluster {
    weight: f32,
    normal_sum: [f32; 3],
    offset_sum: f32,
    color_sum: [f32; 3],
    min: [f32; 3],
    max: [f32; 3],
}

impl PatchCluster {
    fn new(
        normal: [f32; 3],
        offset: f32,
        color: [f32; 3],
        min: [f32; 3],
        max: [f32; 3],
        weight: f32,
    ) -> Self {
        Self {
            weight,
            normal_sum: normal.map(|value| value * weight),
            offset_sum: offset * weight,
            color_sum: color.map(|value| value * weight),
            min,
            max,
        }
    }

    fn normal(self) -> [f32; 3] {
        normalize(self.normal_sum).unwrap_or([0.0, 1.0, 0.0])
    }

    fn merge(
        &mut self,
        mut normal: [f32; 3],
        offset: f32,
        color: [f32; 3],
        min: [f32; 3],
        max: [f32; 3],
        weight: f32,
    ) {
        if dot(self.normal(), normal) < 0.0 {
            normal = normal.map(|value| -value);
        }
        self.weight += weight;
        for axis in 0..3 {
            self.normal_sum[axis] += normal[axis] * weight;
            self.color_sum[axis] += color[axis] * weight;
            self.min[axis] = self.min[axis].min(min[axis]);
            self.max[axis] = self.max[axis].max(max[axis]);
        }
        self.offset_sum += offset * weight;
    }

    fn packed_plane(self) -> u32 {
        pack_surface_plane(
            self.normal(),
            self.offset_sum / self.weight.max(f32::EPSILON),
        )
    }

    fn packed_bounds(self) -> u32 {
        let normal = self.normal();
        let depth_axis = dominant_axis(normal);
        let u_axis = (depth_axis + 1) % 3;
        let v_axis = (depth_axis + 2) % 3;
        let encode = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
        encode(self.min[u_axis])
            | (encode(self.max[u_axis]) << 8)
            | (encode(self.min[v_axis]) << 16)
            | (encode(self.max[v_axis]) << 24)
    }

    fn material(self, options: &MandelMeshOptions<'_>) -> VoxelCell {
        let inverse = self.weight.max(f32::EPSILON).recip();
        VoxelCell::from_material(SurfaceMaterial {
            base_color: self
                .color_sum
                .map(|value| (value * inverse).clamp(0.0, 1.0)),
            roughness: options.roughness.clamp(0.0, 1.0),
            specular: options.specular.clamp(0.0, 1.0),
            transmission: 0.0,
            ior: 1.5,
            emission_strength: options.emission.max(0.0),
        })
    }
}

#[derive(Default)]
struct CellPatches {
    clusters: Vec<PatchCluster>,
    discarded: u64,
}

impl CellPatches {
    fn add(
        &mut self,
        normal: [f32; 3],
        offset: f32,
        color: [f32; 3],
        min: [f32; 3],
        max: [f32; 3],
        weight: f32,
    ) {
        if let Some(cluster) = self
            .clusters
            .iter_mut()
            .find(|cluster| dot(cluster.normal(), normal).abs() >= NORMAL_CLUSTER_COSINE)
        {
            cluster.merge(normal, offset, color, min, max, weight);
            return;
        }
        self.clusters
            .push(PatchCluster::new(normal, offset, color, min, max, weight));
        self.clusters
            .sort_by(|left, right| right.weight.total_cmp(&left.weight));
        if self.clusters.len() > 2 {
            self.clusters.pop();
            self.discarded += 1;
        }
    }
}

pub fn voxelize_mandelbulber_mesh(
    options: &MandelMeshOptions<'_>,
) -> Result<MandelMeshVoxelization> {
    ensure!(
        options.binary.is_some() || options.ply_input.is_some(),
        "Mandelbulber mesh export requires a binary or an existing PLY input"
    );
    if let Some(binary) = options.binary {
        ensure!(
            binary.is_file(),
            "Mandelbulber binary does not exist: {}",
            binary.display()
        );
    }
    if let Some(ply_input) = options.ply_input {
        ensure!(
            ply_input.is_file(),
            "Mandelbulber PLY input does not exist: {}",
            ply_input.display()
        );
    }
    ensure!(
        options.scene.is_file(),
        "Mandelbulber scene does not exist: {}",
        options.scene.display()
    );
    ensure!(
        (2..=512).contains(&options.voxel_resolution),
        "voxel resolution must be 2..512"
    );
    ensure!(
        (2..=1024).contains(&options.mesh_resolution),
        "Mandelbulber mesh resolution must be 2..1024"
    );
    ensure!(
        options.auto_bounds_margin.is_finite()
            && (0.01..=1.0).contains(&options.auto_bounds_margin),
        "automatic mesh bounds margin must be 0.01..1.0"
    );

    let total_started = Instant::now();
    let mut discovery_options = options.clone();
    discovery_options.auto_bounds = false;
    let discovery_started = Instant::now();
    let mut discovery = voxelize_mandelbulber_mesh_once(&discovery_options)?;
    let discovery_pass_ms = discovery_started.elapsed().as_secs_f64() * 1000.0;
    let discovery_mesh_bounds = discovery.summary.mesh_bounds;
    let discovery_boundary_cells = boundary_cell_count(&discovery.grid);
    let mut auto_bounds = MandelMeshAutoBoundsSummary {
        requested: options.auto_bounds,
        accepted: false,
        reason: if options.auto_bounds {
            "candidate_not_evaluated".to_owned()
        } else {
            "disabled".to_owned()
        },
        margin: options.auto_bounds_margin,
        original_bounds: options.bounds,
        discovery_mesh_bounds: Some(discovery_mesh_bounds),
        candidate_bounds: None,
        discovery_boundary_cells,
        candidate_boundary_cells: None,
        candidate_secondary_patch_ratio: None,
        max_secondary_patch_ratio: AUTO_BOUNDS_MAX_SECONDARY_PATCH_RATIO,
        discovery_pass_ms,
        candidate_pass_ms: None,
        total_pass_ms: total_started.elapsed().as_secs_f64() * 1000.0,
    };
    if !options.auto_bounds {
        discovery.auto_bounds = auto_bounds;
        return Ok(discovery);
    }
    if discovery_boundary_cells != 0 {
        auto_bounds.reason = "discovery_touches_volume_boundary".to_owned();
        discovery.auto_bounds = auto_bounds;
        return Ok(discovery);
    }
    let candidate_bounds = if options.voxel_cells_only {
        derive_centered_bounds(discovery_mesh_bounds, options.auto_bounds_margin)
    } else {
        derive_auto_bounds(
            discovery_mesh_bounds,
            options.bounds,
            options.auto_bounds_margin,
        )
    };
    let Some(candidate_bounds) = candidate_bounds else {
        auto_bounds.reason = "candidate_not_strictly_inside_original_bounds".to_owned();
        discovery.auto_bounds = auto_bounds;
        return Ok(discovery);
    };
    auto_bounds.candidate_bounds = Some(candidate_bounds);

    let candidate_raw = options.raw_ply_output.map(|_| {
        std::env::temp_dir().join(format!(
            "fpt-mandel-auto-bounds-{}-{}.ply",
            std::process::id(),
            TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ))
    });
    let mut candidate_options = options.clone();
    candidate_options.auto_bounds = false;
    candidate_options.bounds = candidate_bounds;
    if options.voxel_cells_only {
        candidate_options.voxel_resolution_3d =
            Some(options.voxel_min_axis_resolution.map_or_else(
                || aspect_resolutions(candidate_bounds, options.voxel_resolution),
                |minimum| cubic_voxel_resolutions(candidate_bounds, minimum),
            ));
    }
    candidate_options.raw_ply_output = candidate_raw.as_deref();
    let candidate_started = Instant::now();
    let candidate_result = voxelize_mandelbulber_mesh_once(&candidate_options);
    auto_bounds.candidate_pass_ms = Some(candidate_started.elapsed().as_secs_f64() * 1000.0);
    auto_bounds.total_pass_ms = total_started.elapsed().as_secs_f64() * 1000.0;
    let mut candidate = match candidate_result {
        Ok(candidate) => candidate,
        Err(error) => {
            if let Some(path) = candidate_raw.as_deref() {
                let _ = fs::remove_file(path);
            }
            auto_bounds.reason = format!("candidate_export_failed: {error}");
            discovery.auto_bounds = auto_bounds;
            return Ok(discovery);
        }
    };
    let candidate_boundary_cells = boundary_cell_count(&candidate.grid);
    auto_bounds.candidate_boundary_cells = Some(candidate_boundary_cells);
    if !options.voxel_cells_only && candidate_boundary_cells != 0 {
        if let Some(path) = candidate_raw.as_deref() {
            let _ = fs::remove_file(path);
        }
        auto_bounds.reason = "candidate_touches_volume_boundary".to_owned();
        discovery.auto_bounds = auto_bounds;
        return Ok(discovery);
    }
    let candidate_secondary_patch_ratio = secondary_patch_ratio(
        candidate.summary.cells_with_secondary_patch,
        candidate.summary.occupied_cells,
    );
    auto_bounds.candidate_secondary_patch_ratio = Some(candidate_secondary_patch_ratio);
    if !options.voxel_cells_only
        && candidate_secondary_patch_ratio > AUTO_BOUNDS_MAX_SECONDARY_PATCH_RATIO
    {
        if let Some(path) = candidate_raw.as_deref() {
            let _ = fs::remove_file(path);
        }
        auto_bounds.reason = "candidate_surface_complexity_exceeds_limit".to_owned();
        discovery.auto_bounds = auto_bounds;
        return Ok(discovery);
    }
    if let (Some(staged), Some(output)) = (candidate_raw.as_deref(), options.raw_ply_output) {
        fs::copy(staged, output).with_context(|| {
            format!(
                "replace baseline Mandelbulber mesh {} with accepted auto-bounds mesh {}",
                output.display(),
                staged.display()
            )
        })?;
        let _ = fs::remove_file(staged);
    }
    auto_bounds.accepted = true;
    auto_bounds.reason = if candidate_boundary_cells != 0 {
        "accepted_cropped_voxel_cell_bounds".to_owned()
    } else {
        "accepted_bounds_and_surface_complexity".to_owned()
    };
    candidate.auto_bounds = auto_bounds;
    Ok(candidate)
}

fn voxelize_mandelbulber_mesh_once(
    options: &MandelMeshOptions<'_>,
) -> Result<MandelMeshVoxelization> {
    let temporary = TemporaryDirectory::create()?;
    let generated_ply = temporary.path().join("surface.ply");
    let (ply_path, mesh_export_ms) = if let Some(ply_input) = options.ply_input {
        (ply_input, 0.0)
    } else {
        let binary = options.binary.context("Mandelbulber binary is required")?;
        let source_min = y_up_to_mandelbulber(options.bounds.min);
        let source_max = y_up_to_mandelbulber(options.bounds.max);
        let overrides = [
            "voxel_custom_limit_enabled=1".to_owned(),
            format!(
                "voxel_limit_min={}",
                mandelbulber_vector_override(source_min)
            ),
            format!(
                "voxel_limit_max={}",
                mandelbulber_vector_override(source_max)
            ),
            format!("voxel_samples_x={}", options.mesh_resolution),
            format!("voxel_samples_y={}", options.mesh_resolution),
            format!("voxel_samples_z={}", options.mesh_resolution),
            format!("voxel_max_iter={}", options.max_iterations),
            format!("voxel_image_path={}", temporary.path().display()),
            format!("mesh_output_filename={}", generated_ply.display()),
            "mesh_color=1".to_owned(),
            "mesh_file_mode=0".to_owned(),
            format!("opencl_enabled={}", u8::from(options.use_opencl)),
            "opencl_platform=0".to_owned(),
            // Mandelbulber's -O decoder requires integer values for enum
            // parameters; their display labels silently decode as zero.
            MANDELBULBER_OPENCL_DEVICE_TYPE_GPU.to_owned(),
            MANDELBULBER_OPENCL_MODE_FULL.to_owned(),
            MANDELBULBER_OPENCL_PRECISION_SINGLE.to_owned(),
        ]
        .join("#");

        let export_started = Instant::now();
        let mut command = Command::new(binary);
        command.arg("--voxel").arg("ply");
        if options.use_opencl {
            command.arg("-g");
        }
        let output = command
            .arg("-n")
            .arg(options.scene)
            .arg("-O")
            .arg(overrides)
            .output()
            .with_context(|| format!("run Mandelbulber mesh exporter {}", binary.display()))?;
        let mesh_export_ms = export_started.elapsed().as_secs_f64() * 1000.0;
        if !output.status.success() {
            bail!(
                "Mandelbulber mesh export failed with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        ensure!(
            generated_ply.is_file(),
            "Mandelbulber completed without producing {}",
            generated_ply.display()
        );
        (generated_ply.as_path(), mesh_export_ms)
    };
    let ply_bytes = ply_path.metadata()?.len();
    if let Some(output) = options.raw_ply_output {
        if let Some(parent) = output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        if output != ply_path {
            fs::copy(ply_path, output).with_context(|| {
                format!(
                    "preserve Mandelbulber mesh {} as {}",
                    ply_path.display(),
                    output.display()
                )
            })?;
        }
    }

    let parse_started = Instant::now();
    let mut mesh = read_binary_ply(&ply_path)?;
    let discarded_nonlocal_triangles = discard_nonlocal_marching_cubes_triangles(
        &mut mesh,
        options.bounds,
        options.mesh_resolution,
    );
    let discarded_out_of_bounds_triangles =
        discard_out_of_bounds_triangles(&mut mesh, options.bounds);
    ensure!(
        !mesh.vertices.is_empty() && !mesh.triangles.is_empty(),
        "Mandelbulber mesh is empty inside the requested bounds"
    );
    let ply_parse_ms = parse_started.elapsed().as_secs_f64() * 1000.0;

    let voxelize_started = Instant::now();
    let mut stats = VoxelizeStats::default();
    let (mut grid, mut patches, triangle_surface) = if options.surface_triangles {
        let size = options.bounds.size();
        let triangles = mesh.triangles.iter().map(|triangle| {
            triangle.map(|index| {
                let vertex = mesh.vertices[index as usize];
                MeshSurfaceVertex {
                    position: std::array::from_fn(|axis| {
                        (vertex.position[axis] - options.bounds.min[axis]) / size[axis]
                    }),
                    color: vertex.color,
                }
            })
        });
        let material = SurfaceMaterial {
            base_color: [1.0; 3],
            roughness: options.roughness.clamp(0.0, 1.0),
            specular: options.specular.clamp(0.0, 1.0),
            transmission: 0.0,
            ior: 1.5,
            emission_strength: options.emission.max(0.0),
        };
        let surface = if let Some(resolution) = options.voxel_resolution_3d {
            build_triangle_surface_from_normalized_mesh_3d(
                triangles,
                mesh.triangles.len(),
                resolution,
                aspect_resolutions(options.bounds, options.mesh_resolution),
                options.bounds,
                material,
            )?
        } else {
            build_triangle_surface_from_normalized_mesh(
                triangles,
                mesh.triangles.len(),
                options.voxel_resolution,
                options.mesh_resolution,
                options.bounds,
                material,
            )?
        };
        let source_bytes = fs::read(options.scene)?;
        let source_sha256 = format!("{:x}", Sha256::digest(&source_bytes));
        let grid = VoxelGrid {
            contract_version: 1,
            resolution: surface.resolution,
            bounds: surface.bounds,
            coordinate_system: surface.coordinate_system,
            source_label: options.scene.display().to_string(),
            source_sha256,
            voxels: surface
                .cells
                .iter()
                .map(|cell| SparseVoxel {
                    coordinate: cell.coordinate,
                    cell: cell.cell,
                })
                .collect(),
        };
        stats.intersections = surface.triangles.len() as u64;
        (grid, Vec::new(), Some(surface))
    } else {
        let (grid, patches) = mesh_to_grid(&mesh, options, &mut stats)?;
        (grid, patches, None)
    };
    if triangle_surface.is_none() {
        dilate_voxel_grid(&mut grid, &mut patches, options.voxel_dilation);
    }
    ensure!(
        grid.occupied_voxels() > 0,
        "Mandelbulber mesh did not intersect the requested voxel bounds"
    );
    let voxelize_ms = voxelize_started.elapsed().as_secs_f64() * 1000.0;
    let summary = MandelMeshSummary {
        mesh_export_ms,
        ply_parse_ms,
        voxelize_ms,
        ply_bytes,
        mesh_vertices: mesh.vertices.len(),
        mesh_triangles: mesh.triangles.len(),
        discarded_nonlocal_triangles,
        discarded_out_of_bounds_triangles,
        triangle_cell_tests: stats.tests,
        triangle_cell_intersections: stats.intersections,
        occupied_cells: grid.voxels.len(),
        cells_with_secondary_patch: patches.iter().filter(|patch| patch[1] != 0).count(),
        discarded_patch_clusters: stats.discarded_clusters,
        mesh_bounds: mesh_bounds(&mesh)?,
    };
    Ok(MandelMeshVoxelization {
        grid,
        patches,
        triangle_surface,
        summary,
        auto_bounds: MandelMeshAutoBoundsSummary {
            requested: false,
            accepted: false,
            reason: "single_pass".to_owned(),
            margin: options.auto_bounds_margin,
            original_bounds: options.bounds,
            discovery_mesh_bounds: None,
            candidate_bounds: None,
            discovery_boundary_cells: 0,
            candidate_boundary_cells: None,
            candidate_secondary_patch_ratio: None,
            max_secondary_patch_ratio: AUTO_BOUNDS_MAX_SECONDARY_PATCH_RATIO,
            discovery_pass_ms: 0.0,
            candidate_pass_ms: None,
            total_pass_ms: 0.0,
        },
    })
}

#[derive(Default)]
struct VoxelizeStats {
    tests: u64,
    intersections: u64,
    discarded_clusters: u64,
}

fn mesh_to_grid(
    mesh: &PlyMesh,
    options: &MandelMeshOptions<'_>,
    stats: &mut VoxelizeStats,
) -> Result<(VoxelGrid, Vec<[u32; 3]>)> {
    let resolution = options
        .voxel_resolution_3d
        .unwrap_or([options.voxel_resolution; 3]);
    let size = options.bounds.size();
    let scale: [f32; 3] = std::array::from_fn(|axis| resolution[axis] as f32 / size[axis]);
    let mut cells = BTreeMap::<u64, ([u32; 3], CellPatches)>::new();

    for triangle in &mesh.triangles {
        let vertices = triangle.map(|index| mesh.vertices[index as usize]);
        let grid_vertices = vertices.map(|vertex| ClipVertex {
            position: std::array::from_fn(|axis| {
                (vertex.position[axis] - options.bounds.min[axis]) * scale[axis]
            }),
            color: vertex.color,
        });
        let edge_a = subtract(grid_vertices[1].position, grid_vertices[0].position);
        let edge_b = subtract(grid_vertices[2].position, grid_vertices[0].position);
        let Some(normal) = normalize(cross(edge_a, edge_b)) else {
            continue;
        };
        let minimum: [f32; 3] = std::array::from_fn(|axis| {
            grid_vertices
                .iter()
                .map(|vertex| vertex.position[axis])
                .fold(f32::INFINITY, f32::min)
        });
        let maximum: [f32; 3] = std::array::from_fn(|axis| {
            grid_vertices
                .iter()
                .map(|vertex| vertex.position[axis])
                .fold(f32::NEG_INFINITY, f32::max)
        });
        let start: [i32; 3] = std::array::from_fn(|axis| minimum[axis].floor().max(0.0) as i32);
        let end: [i32; 3] = std::array::from_fn(|axis| {
            maximum[axis].floor().min(resolution[axis] as f32 - 1.0) as i32
        });
        if (0..3).any(|axis| start[axis] > end[axis]) {
            continue;
        }
        for z in start[2]..=end[2] {
            for y in start[1]..=end[1] {
                for x in start[0]..=end[0] {
                    stats.tests += 1;
                    let coordinate = [x as u32, y as u32, z as u32];
                    let center = [x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5];
                    let positions = grid_vertices.map(|vertex| subtract(vertex.position, center));
                    if !triangle_box_overlap(positions) {
                        continue;
                    }
                    let polygon =
                        clip_triangle_to_cell(grid_vertices, [x as f32, y as f32, z as f32]);
                    if polygon.len() < 3 {
                        continue;
                    }
                    stats.intersections += 1;
                    let local_origin = [x as f32, y as f32, z as f32];
                    let local_min = std::array::from_fn(|axis| {
                        polygon
                            .iter()
                            .map(|vertex| vertex.position[axis] - local_origin[axis])
                            .fold(f32::INFINITY, f32::min)
                    });
                    let local_max = std::array::from_fn(|axis| {
                        polygon
                            .iter()
                            .map(|vertex| vertex.position[axis] - local_origin[axis])
                            .fold(f32::NEG_INFINITY, f32::max)
                    });
                    let color = std::array::from_fn(|axis| {
                        polygon.iter().map(|vertex| vertex.color[axis]).sum::<f32>()
                            / polygon.len() as f32
                    });
                    let offset = dot(subtract(grid_vertices[0].position, center), normal);
                    let weight = polygon_area(&polygon, normal).max(1.0e-6);
                    let linear = u64::from(coordinate[0])
                        + u64::from(coordinate[1]) * u64::from(resolution[0])
                        + u64::from(coordinate[2])
                            * u64::from(resolution[0])
                            * u64::from(resolution[1]);
                    let entry = cells
                        .entry(linear)
                        .or_insert_with(|| (coordinate, CellPatches::default()));
                    entry
                        .1
                        .add(normal, offset, color, local_min, local_max, weight);
                }
            }
        }
    }

    let source_bytes = fs::read(options.scene)?;
    let source_sha256 = format!("{:x}", Sha256::digest(&source_bytes));
    let mut voxels = Vec::with_capacity(cells.len());
    let mut patches = Vec::with_capacity(cells.len());
    for (_, (coordinate, mut cell)) in cells {
        cell.clusters
            .sort_by(|left, right| right.weight.total_cmp(&left.weight));
        let primary = cell.clusters[0];
        let secondary = cell.clusters.get(1).copied();
        voxels.push(SparseVoxel {
            coordinate,
            cell: primary.material(options),
        });
        patches.push([
            primary.packed_plane(),
            secondary.map_or(0, PatchCluster::packed_plane),
            secondary.map_or(0, PatchCluster::packed_bounds),
        ]);
        stats.discarded_clusters += cell.discarded;
    }
    let grid = VoxelGrid {
        contract_version: 1,
        resolution,
        bounds: options.bounds,
        coordinate_system: CoordinateSystem::YUpRightHanded,
        source_label: options.scene.display().to_string(),
        source_sha256,
        voxels,
    };
    Ok((grid, patches))
}

fn dilate_voxel_grid(grid: &mut VoxelGrid, patches: &mut Vec<[u32; 3]>, radius: u32) {
    if radius == 0 || grid.voxels.is_empty() {
        return;
    }
    let originals: Vec<_> = grid
        .voxels
        .iter()
        .copied()
        .zip(patches.iter().copied())
        .collect();
    let linear = |coordinate: [u32; 3]| {
        u64::from(coordinate[0])
            + u64::from(coordinate[1]) * u64::from(grid.resolution[0])
            + u64::from(coordinate[2])
                * u64::from(grid.resolution[0])
                * u64::from(grid.resolution[1])
    };
    let mut expanded = BTreeMap::<u64, (SparseVoxel, [u32; 3])>::new();
    for (voxel, patch) in &originals {
        expanded.insert(linear(voxel.coordinate), (*voxel, *patch));
    }
    let radius = radius as i32;
    for (voxel, _) in originals {
        for dz in -radius..=radius {
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx * dx + dy * dy + dz * dz > radius * radius {
                        continue;
                    }
                    let coordinate = [
                        voxel.coordinate[0] as i64 + i64::from(dx),
                        voxel.coordinate[1] as i64 + i64::from(dy),
                        voxel.coordinate[2] as i64 + i64::from(dz),
                    ];
                    if coordinate
                        .iter()
                        .zip(grid.resolution)
                        .any(|(value, limit)| *value < 0 || *value >= i64::from(limit))
                    {
                        continue;
                    }
                    let coordinate = [
                        coordinate[0] as u32,
                        coordinate[1] as u32,
                        coordinate[2] as u32,
                    ];
                    expanded.entry(linear(coordinate)).or_insert((
                        SparseVoxel {
                            coordinate,
                            cell: voxel.cell,
                        },
                        [0; 3],
                    ));
                }
            }
        }
    }
    grid.voxels.clear();
    patches.clear();
    grid.voxels.reserve(expanded.len());
    patches.reserve(expanded.len());
    for (_, (voxel, patch)) in expanded {
        grid.voxels.push(voxel);
        patches.push(patch);
    }
}

fn read_binary_ply(path: &Path) -> Result<PlyMesh> {
    let data = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    ensure!(
        data.starts_with(b"ply\n"),
        "{} is not a PLY file",
        path.display()
    );
    let marker = b"end_header\n";
    let header_end = data
        .windows(marker.len())
        .position(|window| window == marker)
        .map(|offset| offset + marker.len())
        .ok_or_else(|| anyhow!("{} has no PLY end_header", path.display()))?;
    let header = std::str::from_utf8(&data[..header_end]).context("PLY header is not UTF-8")?;
    ensure!(
        header
            .lines()
            .any(|line| line == "format binary_little_endian 1.0"),
        "only binary little-endian PLY 1.0 is supported"
    );
    let vertex_count = ply_element_count(header, "vertex")?;
    let triangle_count = ply_element_count(header, "face")?;
    let expected_vertex_properties = [
        "property double x",
        "property double y",
        "property double z",
        "property double s",
        "property double t",
        "property uchar red",
        "property uchar green",
        "property uchar blue",
    ];
    let lines = header.lines().collect::<Vec<_>>();
    let vertex_element = lines
        .iter()
        .position(|line| line.starts_with("element vertex "))
        .context("PLY has no vertex element")?;
    let face_element = lines
        .iter()
        .position(|line| line.starts_with("element face "))
        .context("PLY has no face element")?;
    ensure!(
        vertex_element < face_element,
        "unsupported Mandelbulber PLY element order"
    );
    let vertex_properties = lines[vertex_element + 1..face_element]
        .iter()
        .copied()
        .filter(|line| line.starts_with("property "))
        .collect::<Vec<_>>();
    ensure!(
        vertex_properties == expected_vertex_properties,
        "unsupported Mandelbulber PLY vertex property order"
    );
    let face_properties = lines[face_element + 1..]
        .iter()
        .copied()
        .filter(|line| line.starts_with("property "))
        .collect::<Vec<_>>();
    ensure!(
        face_properties == ["property list uchar int vertex_index"],
        "unsupported Mandelbulber PLY face layout"
    );
    const VERTEX_STRIDE: usize = 43;
    const FACE_STRIDE: usize = 13;
    let vertices_end = header_end
        .checked_add(
            vertex_count
                .checked_mul(VERTEX_STRIDE)
                .context("PLY vertex size overflow")?,
        )
        .context("PLY vertex offset overflow")?;
    let expected_size = vertices_end
        .checked_add(
            triangle_count
                .checked_mul(FACE_STRIDE)
                .context("PLY face size overflow")?,
        )
        .context("PLY file size overflow")?;
    ensure!(
        data.len() == expected_size,
        "unexpected PLY byte count: got {}, expected {expected_size}",
        data.len()
    );

    let mut vertices = Vec::with_capacity(vertex_count);
    for index in 0..vertex_count {
        let record =
            &data[header_end + index * VERTEX_STRIDE..header_end + (index + 1) * VERTEX_STRIDE];
        let source = [
            read_f64(record, 0)?,
            read_f64(record, 8)?,
            read_f64(record, 16)?,
        ];
        ensure!(
            source.iter().all(|value| value.is_finite()),
            "PLY vertex {index} contains a non-finite position"
        );
        vertices.push(MeshVertex {
            position: mandelbulber_to_y_up(source.map(|value| value as f32)),
            color: [record[40], record[41], record[42]].map(|value| value as f32 / 255.0),
        });
    }
    let mut triangles = Vec::with_capacity(triangle_count);
    for index in 0..triangle_count {
        let record =
            &data[vertices_end + index * FACE_STRIDE..vertices_end + (index + 1) * FACE_STRIDE];
        ensure!(record[0] == 3, "PLY face {index} is not a triangle");
        let triangle = [
            read_i32(record, 1)?,
            read_i32(record, 5)?,
            read_i32(record, 9)?,
        ];
        ensure!(
            triangle
                .iter()
                .all(|value| *value >= 0 && (*value as usize) < vertex_count),
            "PLY face {index} contains an invalid vertex index"
        );
        triangles.push(triangle.map(|value| value as u32));
    }
    Ok(PlyMesh {
        vertices,
        triangles,
    })
}

fn ply_element_count(header: &str, name: &str) -> Result<usize> {
    let prefix = format!("element {name} ");
    let line = header
        .lines()
        .find(|line| line.starts_with(&prefix))
        .ok_or_else(|| anyhow!("PLY header has no {name} element"))?;
    Ok(line[prefix.len()..].parse()?)
}

fn read_f64(record: &[u8], offset: usize) -> Result<f64> {
    Ok(f64::from_le_bytes(record[offset..offset + 8].try_into()?))
}

fn read_i32(record: &[u8], offset: usize) -> Result<i32> {
    Ok(i32::from_le_bytes(record[offset..offset + 4].try_into()?))
}

fn y_up_to_mandelbulber(value: [f32; 3]) -> [f32; 3] {
    [value[0], value[2], value[1]]
}

fn mandelbulber_vector_override(value: [f32; 3]) -> String {
    format!(
        "{:.17} {:.17} {:.17}",
        f64::from(value[0]),
        f64::from(value[1]),
        f64::from(value[2])
    )
}

fn mandelbulber_to_y_up(value: [f32; 3]) -> [f32; 3] {
    [value[0], value[2], value[1]]
}

fn subtract(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| left[axis] - right[axis])
}

fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn normalize(value: [f32; 3]) -> Option<[f32; 3]> {
    let length_squared = dot(value, value);
    (length_squared.is_finite() && length_squared > 1.0e-12).then(|| {
        let inverse = length_squared.sqrt().recip();
        value.map(|component| component * inverse)
    })
}

fn dominant_axis(normal: [f32; 3]) -> usize {
    if normal[0].abs() >= normal[1].abs() && normal[0].abs() >= normal[2].abs() {
        0
    } else if normal[1].abs() >= normal[2].abs() {
        1
    } else {
        2
    }
}

fn axis_separates(axis: [f32; 3], triangle: [[f32; 3]; 3]) -> bool {
    if dot(axis, axis) <= 1.0e-12 {
        return false;
    }
    let projection = triangle.map(|vertex| dot(axis, vertex));
    let minimum = projection.into_iter().fold(f32::INFINITY, f32::min);
    let maximum = projection.into_iter().fold(f32::NEG_INFINITY, f32::max);
    let radius = 0.5 * (axis[0].abs() + axis[1].abs() + axis[2].abs());
    minimum > radius || maximum < -radius
}

fn triangle_box_overlap(triangle: [[f32; 3]; 3]) -> bool {
    for axis in 0..3 {
        let minimum = triangle
            .iter()
            .map(|vertex| vertex[axis])
            .fold(f32::INFINITY, f32::min);
        let maximum = triangle
            .iter()
            .map(|vertex| vertex[axis])
            .fold(f32::NEG_INFINITY, f32::max);
        if minimum > 0.5 || maximum < -0.5 {
            return false;
        }
    }
    let edges = [
        subtract(triangle[1], triangle[0]),
        subtract(triangle[2], triangle[1]),
        subtract(triangle[0], triangle[2]),
    ];
    let normal = cross(edges[0], subtract(triangle[2], triangle[0]));
    if axis_separates(normal, triangle) {
        return false;
    }
    for edge in edges {
        for axis in [
            [0.0, -edge[2], edge[1]],
            [edge[2], 0.0, -edge[0]],
            [-edge[1], edge[0], 0.0],
        ] {
            if axis_separates(axis, triangle) {
                return false;
            }
        }
    }
    true
}

fn clip_triangle_to_cell(triangle: [ClipVertex; 3], origin: [f32; 3]) -> Vec<ClipVertex> {
    let mut polygon = triangle.to_vec();
    for axis in 0..3 {
        polygon = clip_polygon(polygon, axis, origin[axis], true);
        polygon = clip_polygon(polygon, axis, origin[axis] + 1.0, false);
    }
    polygon
}

fn clip_polygon(
    input: Vec<ClipVertex>,
    axis: usize,
    boundary: f32,
    keep_greater: bool,
) -> Vec<ClipVertex> {
    if input.is_empty() {
        return input;
    }
    let inside = |vertex: ClipVertex| {
        if keep_greater {
            vertex.position[axis] >= boundary - 1.0e-6
        } else {
            vertex.position[axis] <= boundary + 1.0e-6
        }
    };
    let mut output = Vec::with_capacity(input.len() + 1);
    let mut previous = *input.last().expect("nonempty polygon");
    let mut previous_inside = inside(previous);
    for current in input {
        let current_inside = inside(current);
        if current_inside != previous_inside {
            let denominator = current.position[axis] - previous.position[axis];
            let amount = if denominator.abs() <= 1.0e-12 {
                0.0
            } else {
                ((boundary - previous.position[axis]) / denominator).clamp(0.0, 1.0)
            };
            output.push(ClipVertex {
                position: std::array::from_fn(|component| {
                    previous.position[component]
                        + (current.position[component] - previous.position[component]) * amount
                }),
                color: std::array::from_fn(|component| {
                    previous.color[component]
                        + (current.color[component] - previous.color[component]) * amount
                }),
            });
        }
        if current_inside {
            output.push(current);
        }
        previous = current;
        previous_inside = current_inside;
    }
    output
}

fn polygon_area(polygon: &[ClipVertex], normal: [f32; 3]) -> f32 {
    if polygon.len() < 3 {
        return 0.0;
    }
    let origin = polygon[0].position;
    (1..polygon.len() - 1)
        .map(|index| {
            let left = subtract(polygon[index].position, origin);
            let right = subtract(polygon[index + 1].position, origin);
            dot(cross(left, right), normal).abs() * 0.5
        })
        .sum()
}

fn pack_surface_plane(normal: [f32; 3], offset: f32) -> u32 {
    let denominator = (normal[0].abs() + normal[1].abs() + normal[2].abs()).max(1.0e-8);
    let mut octahedral = [normal[0] / denominator, normal[1] / denominator];
    if normal[2] < 0.0 {
        let source = octahedral;
        let sign = |value: f32| value.signum();
        octahedral = [
            (1.0 - source[1].abs()) * sign(source[0]),
            (1.0 - source[0].abs()) * sign(source[1]),
        ];
    }
    let encode_normal = |value: f32| ((value * 0.5 + 0.5).clamp(0.0, 1.0) * 4095.0).round() as u32;
    let encoded_offset = ((offset * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0).round() as u32;
    (encode_normal(octahedral[0]) | (encode_normal(octahedral[1]) << 12) | (encoded_offset << 24))
        .max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opencl_cli_overrides_use_mandelbulber_numeric_enums() {
        assert_eq!(MANDELBULBER_OPENCL_DEVICE_TYPE_GPU, "opencl_device_type=0");
        assert_eq!(MANDELBULBER_OPENCL_MODE_FULL, "opencl_mode=3");
        assert_eq!(MANDELBULBER_OPENCL_PRECISION_SINGLE, "opencl_precision=0");
    }

    #[test]
    fn vector_overrides_preserve_the_exact_f32_bounds_in_double() {
        let values = [
            1.680698275566101_f32,
            2.0853164196014404,
            -3.2698488235473633,
        ];
        let parsed = mandelbulber_vector_override(values)
            .split_whitespace()
            .map(|value| value.parse::<f64>().expect("double component"))
            .collect::<Vec<_>>();
        assert_eq!(parsed, values.map(f64::from));
    }

    #[test]
    fn nonlocal_marching_cube_faces_are_discarded() {
        let vertex = |position| MeshVertex {
            position,
            color: [1.0; 3],
        };
        let mut mesh = PlyMesh {
            vertices: vec![
                vertex([0.0, 0.0, 0.0]),
                vertex([0.01, 0.0, 0.0]),
                vertex([0.0, 0.01, 0.0]),
                vertex([1.0, 1.0, 1.0]),
            ],
            triangles: vec![[0, 1, 2], [0, 1, 3]],
        };
        let discarded = discard_nonlocal_marching_cubes_triangles(
            &mut mesh,
            Aabb::new([0.0; 3], [1.0; 3]),
            100,
        );
        assert_eq!(discarded, 1);
        assert_eq!(mesh.triangles, vec![[0, 1, 2]]);
    }

    #[test]
    fn out_of_bounds_faces_do_not_expand_discovery_mesh_bounds() {
        let vertex = |position| MeshVertex {
            position,
            color: [1.0; 3],
        };
        let mut mesh = PlyMesh {
            vertices: vec![
                vertex([-1.1, -1.0, -1.0]),
                vertex([-1.1, 1.0, -1.0]),
                vertex([-1.1, -1.0, 1.0]),
                vertex([-0.5, -0.25, -0.1]),
                vertex([0.5, -0.25, 0.2]),
                vertex([0.0, 0.5, 0.0]),
            ],
            triangles: vec![[0, 1, 2], [3, 4, 5]],
        };
        let discarded = discard_out_of_bounds_triangles(&mut mesh, Aabb::new([-1.0; 3], [1.0; 3]));
        assert_eq!(discarded, 1);
        assert_eq!(mesh.triangles, vec![[3, 4, 5]]);
        assert_eq!(
            mesh_bounds(&mesh).expect("retained mesh bounds"),
            Aabb::new([-0.5, -0.25, -0.1], [0.5, 0.5, 0.2])
        );
    }

    fn sample_binary_ply(vertex_properties: &[&str]) -> Vec<u8> {
        let mut bytes = format!(
            "ply\nformat binary_little_endian 1.0\nelement vertex 3\n{}\nelement face 1\nproperty list uchar int vertex_index\nend_header\n",
            vertex_properties.join("\n")
        )
        .into_bytes();
        for (position, color) in [
            ([0.0_f64, 0.0, 0.0], [255_u8, 0, 0]),
            ([1.0_f64, 0.0, 0.0], [0_u8, 255, 0]),
            ([0.0_f64, 1.0, 0.0], [0_u8, 0, 255]),
        ] {
            for value in position {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            bytes.extend_from_slice(&0.0_f64.to_le_bytes());
            bytes.extend_from_slice(&0.0_f64.to_le_bytes());
            bytes.extend_from_slice(&color);
        }
        bytes.push(3);
        for index in [0_i32, 1, 2] {
            bytes.extend_from_slice(&index.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn triangle_box_sat_accepts_crossing_and_rejects_distant_triangles() {
        assert!(triangle_box_overlap([
            [-1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        ]));
        assert!(!triangle_box_overlap([
            [0.6, 0.6, 0.6],
            [0.8, 0.6, 0.6],
            [0.6, 0.8, 0.6],
        ]));
    }

    #[test]
    fn clipping_preserves_interpolated_color() {
        let polygon = clip_triangle_to_cell(
            [
                ClipVertex {
                    position: [-1.0, 0.5, 0.5],
                    color: [1.0, 0.0, 0.0],
                },
                ClipVertex {
                    position: [0.5, 0.0, 0.5],
                    color: [0.0, 1.0, 0.0],
                },
                ClipVertex {
                    position: [0.5, 1.0, 0.5],
                    color: [0.0, 0.0, 1.0],
                },
            ],
            [0.0, 0.0, 0.0],
        );
        assert!(polygon.len() >= 3);
        assert!(polygon.iter().all(|vertex| {
            vertex
                .position
                .iter()
                .all(|value| (-1.0e-6..=1.000001).contains(value))
        }));
        assert!(
            polygon
                .iter()
                .all(|vertex| vertex.color.iter().all(|value| (0.0..=1.0).contains(value)))
        );
    }

    #[test]
    fn binary_ply_parser_accepts_exact_mandelbulber_layout() {
        let directory = TemporaryDirectory::create().expect("temporary directory");
        let path = directory.path().join("mesh.ply");
        fs::write(
            &path,
            sample_binary_ply(&[
                "property double x",
                "property double y",
                "property double z",
                "property double s",
                "property double t",
                "property uchar red",
                "property uchar green",
                "property uchar blue",
            ]),
        )
        .expect("write PLY");
        let mesh = read_binary_ply(&path).expect("parse PLY");
        assert_eq!(mesh.vertices.len(), 3);
        assert_eq!(mesh.triangles, vec![[0, 1, 2]]);
        assert_eq!(mesh.vertices[1].position, [1.0, 0.0, 0.0]);
        assert_eq!(mesh.vertices[2].position, [0.0, 0.0, 1.0]);
    }

    #[test]
    fn binary_ply_parser_rejects_reordered_properties() {
        let directory = TemporaryDirectory::create().expect("temporary directory");
        let path = directory.path().join("mesh.ply");
        fs::write(
            &path,
            sample_binary_ply(&[
                "property double y",
                "property double x",
                "property double z",
                "property double s",
                "property double t",
                "property uchar red",
                "property uchar green",
                "property uchar blue",
            ]),
        )
        .expect("write PLY");
        assert!(
            read_binary_ply(&path)
                .expect_err("reordered layout must fail")
                .to_string()
                .contains("property order")
        );
    }

    #[test]
    fn automatic_bounds_build_a_centered_cube_with_total_margin() {
        let mesh = Aabb::new([-1.0, -0.1, -2.0], [3.0, 0.1, 2.0]);
        let bounds = derive_auto_bounds(mesh, Aabb::new([-10.0; 3], [10.0; 3]), 0.10)
            .expect("strictly contained candidate");
        assert_eq!(bounds, Aabb::new([-1.2, -2.2, -2.2], [3.2, 2.2, 2.2]));
    }

    #[test]
    fn automatic_bounds_reject_candidates_outside_the_original_domain() {
        let mesh = Aabb::new([-1.0; 3], [1.0; 3]);
        assert!(derive_auto_bounds(mesh, Aabb::new([-1.0; 3], [1.0; 3]), 0.10).is_none());
    }

    #[test]
    fn centered_bounds_can_expand_a_thin_voxel_cell_domain() {
        let mesh = Aabb::new([-4.0, -0.1, -2.0], [4.0, 0.1, 2.0]);
        assert_eq!(
            derive_centered_bounds(mesh, 0.10),
            Some(Aabb::new([-4.4; 3], [4.4; 3]))
        );
    }

    #[test]
    fn minimum_axis_resolution_preserves_approximately_cubic_cells() {
        let bounds = Aabb::new([0.0, 0.0, 0.0], [12.0, 0.5, 4.0]);
        assert_eq!(cubic_voxel_resolutions(bounds, 64), [1536, 64, 512]);
    }

    #[test]
    fn camera_safe_bounds_pad_an_interior_camera_without_moving_other_faces() {
        let bounds = Aabb::new([0.0; 3], [10.0, 8.0, 6.0]);
        assert_eq!(
            camera_safe_bounds(bounds, [9.8, 0.1, 3.0], 0.1),
            Aabb::new([0.0, -0.9, 0.0], [10.8, 8.0, 6.0])
        );
    }

    #[test]
    fn camera_safe_bounds_do_not_absorb_an_exterior_object_camera() {
        let bounds = Aabb::new([-1.0; 3], [1.0; 3]);
        assert_eq!(camera_safe_bounds(bounds, [0.0, 0.0, -5.0], 0.1), bounds);
    }

    #[test]
    fn voxel_dilation_expands_a_surface_cell_deterministically() {
        let cell = VoxelCell::from_material(SurfaceMaterial::default());
        let mut grid = VoxelGrid {
            contract_version: 1,
            resolution: [3; 3],
            bounds: Aabb::new([0.0; 3], [3.0; 3]),
            coordinate_system: CoordinateSystem::YUpRightHanded,
            source_label: "test".to_owned(),
            source_sha256: "0".repeat(64),
            voxels: vec![SparseVoxel {
                coordinate: [1; 3],
                cell,
            }],
        };
        let mut patches = vec![[1, 2, 3]];
        dilate_voxel_grid(&mut grid, &mut patches, 1);
        assert_eq!(grid.voxels.len(), 7);
        assert_eq!(patches.len(), 7);
        assert_eq!(grid.voxels[3].coordinate, [1; 3]);
        assert_eq!(patches[3], [1, 2, 3]);
    }

    #[test]
    fn boundary_gate_checks_all_six_volume_faces() {
        let cell = VoxelCell::from_material(SurfaceMaterial::default());
        let make = |coordinate| SparseVoxel { coordinate, cell };
        let mut grid = VoxelGrid {
            contract_version: 1,
            resolution: [8; 3],
            bounds: Aabb::new([-1.0; 3], [1.0; 3]),
            coordinate_system: CoordinateSystem::YUpRightHanded,
            source_label: "test".to_owned(),
            source_sha256: "0".repeat(64),
            voxels: vec![make([2, 3, 4])],
        };
        assert_eq!(boundary_cell_count(&grid), 0);
        grid.voxels.extend([
            make([0, 3, 4]),
            make([7, 3, 4]),
            make([2, 0, 4]),
            make([2, 7, 4]),
            make([2, 3, 0]),
            make([2, 3, 7]),
        ]);
        assert_eq!(boundary_cell_count(&grid), 6);
    }

    #[test]
    fn automatic_bounds_reject_complex_multi_plane_candidates() {
        assert!(secondary_patch_ratio(27, 100) <= AUTO_BOUNDS_MAX_SECONDARY_PATCH_RATIO);
        assert!(secondary_patch_ratio(89, 100) > AUTO_BOUNDS_MAX_SECONDARY_PATCH_RATIO);
    }
}
