use anyhow::{Context, Result, anyhow, bail, ensure};
use fpt_metal::{Aabb, CoordinateSystem, SparseVoxel, SurfaceMaterial, VoxelCell, VoxelGrid};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const NORMAL_CLUSTER_COSINE: f32 = 0.94;
static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct MandelMeshOptions<'a> {
    pub binary: &'a Path,
    pub scene: &'a Path,
    pub raw_ply_output: Option<&'a Path>,
    pub bounds: Aabb,
    pub voxel_resolution: u32,
    pub mesh_resolution: u32,
    pub max_iterations: u32,
    pub use_opencl: bool,
    pub roughness: f32,
    pub specular: f32,
    pub emission: f32,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct MandelMeshSummary {
    pub mesh_export_ms: f64,
    pub ply_parse_ms: f64,
    pub voxelize_ms: f64,
    pub ply_bytes: u64,
    pub mesh_vertices: usize,
    pub mesh_triangles: usize,
    pub triangle_cell_tests: u64,
    pub triangle_cell_intersections: u64,
    pub occupied_cells: usize,
    pub cells_with_secondary_patch: usize,
    pub discarded_patch_clusters: u64,
}

pub struct MandelMeshVoxelization {
    pub grid: VoxelGrid,
    pub patches: Vec<[u32; 3]>,
    pub summary: MandelMeshSummary,
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
        options.binary.is_file(),
        "Mandelbulber binary does not exist: {}",
        options.binary.display()
    );
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

    let temporary = TemporaryDirectory::create()?;
    let ply_path = temporary.path().join("surface.ply");
    let source_min = y_up_to_mandelbulber(options.bounds.min);
    let source_max = y_up_to_mandelbulber(options.bounds.max);
    let vector = |value: [f32; 3]| format!("{} {} {}", value[0], value[1], value[2]);
    let overrides = [
        "voxel_custom_limit_enabled=1".to_owned(),
        format!("voxel_limit_min={}", vector(source_min)),
        format!("voxel_limit_max={}", vector(source_max)),
        format!("voxel_samples_x={}", options.mesh_resolution),
        format!("voxel_samples_y={}", options.mesh_resolution),
        format!("voxel_samples_z={}", options.mesh_resolution),
        format!("voxel_max_iter={}", options.max_iterations),
        format!("voxel_image_path={}", temporary.path().display()),
        format!("mesh_output_filename={}", ply_path.display()),
        "mesh_color=1".to_owned(),
        "mesh_file_mode=0".to_owned(),
        format!("opencl_enabled={}", u8::from(options.use_opencl)),
        "opencl_platform=0".to_owned(),
        "opencl_device_type=gpu".to_owned(),
        "opencl_mode=full".to_owned(),
        "opencl_precision=single".to_owned(),
    ]
    .join("#");

    let export_started = Instant::now();
    let mut command = Command::new(options.binary);
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
        .with_context(|| {
            format!(
                "run Mandelbulber mesh exporter {}",
                options.binary.display()
            )
        })?;
    let mesh_export_ms = export_started.elapsed().as_secs_f64() * 1000.0;
    if !output.status.success() {
        bail!(
            "Mandelbulber mesh export failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    ensure!(
        ply_path.is_file(),
        "Mandelbulber completed without producing {}",
        ply_path.display()
    );
    let ply_bytes = ply_path.metadata()?.len();
    if let Some(output) = options.raw_ply_output {
        if let Some(parent) = output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        fs::copy(&ply_path, output).with_context(|| {
            format!(
                "preserve Mandelbulber mesh {} as {}",
                ply_path.display(),
                output.display()
            )
        })?;
    }

    let parse_started = Instant::now();
    let mesh = read_binary_ply(&ply_path)?;
    ensure!(
        !mesh.vertices.is_empty() && !mesh.triangles.is_empty(),
        "Mandelbulber mesh is empty inside the requested bounds"
    );
    let ply_parse_ms = parse_started.elapsed().as_secs_f64() * 1000.0;

    let voxelize_started = Instant::now();
    let mut stats = VoxelizeStats::default();
    let (grid, patches) = mesh_to_grid(&mesh, options, &mut stats)?;
    let voxelize_ms = voxelize_started.elapsed().as_secs_f64() * 1000.0;
    let summary = MandelMeshSummary {
        mesh_export_ms,
        ply_parse_ms,
        voxelize_ms,
        ply_bytes,
        mesh_vertices: mesh.vertices.len(),
        mesh_triangles: mesh.triangles.len(),
        triangle_cell_tests: stats.tests,
        triangle_cell_intersections: stats.intersections,
        occupied_cells: grid.voxels.len(),
        cells_with_secondary_patch: patches.iter().filter(|patch| patch[1] != 0).count(),
        discarded_patch_clusters: stats.discarded_clusters,
    };
    Ok(MandelMeshVoxelization {
        grid,
        patches,
        summary,
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
    let resolution = [options.voxel_resolution; 3];
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
}
