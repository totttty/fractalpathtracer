//! Deterministic reference voxelization and portable GLB export.
//!
//! The CPU path intentionally covers only built-in fixtures. Production
//! `.fract` export is performed by the package CLI through the authoritative
//! Metal `voxel_build_kernel`; its exact 12-byte cells are then passed through
//! [`VoxelGrid::from_dense_cells`] and this module's GLB encoder.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Version of the public voxel cell/grid and GLB extras contract.
pub const FPT_VOXEL_CONTRACT_VERSION: u32 = 1;
/// Stable string marker embedded at `asset.extras.fpt_voxel_contract.name`.
pub const FPT_VOXEL_CONTRACT_NAME: &str = "fpt_voxel_grid";

/// A finite axis-aligned world-space box.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Aabb {
    pub const fn new(min: [f32; 3], max: [f32; 3]) -> Self {
        Self { min, max }
    }

    fn validate(self) -> Result<Self, FractalError> {
        for axis in 0..3 {
            if !self.min[axis].is_finite()
                || !self.max[axis].is_finite()
                || self.min[axis] >= self.max[axis]
            {
                return Err(FractalError::new(
                    FractalErrorCode::InvalidBounds,
                    format!(
                        "bounds axis {axis} must contain finite min < max values (got {}..{})",
                        self.min[axis], self.max[axis]
                    ),
                ));
            }
        }
        Ok(self)
    }

    pub fn size(self) -> [f32; 3] {
        std::array::from_fn(|axis| self.max[axis] - self.min[axis])
    }
}

/// Coordinate system used by the sampled field before GLB conversion.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateSystem {
    /// Right-handed, positive Y is up. Binary artifact enum value 1.
    YUpRightHanded = 1,
    /// Mandelbulber's source scene basis: right-handed, positive Z is up.
    /// Binary artifact enum value 2.
    MandelbulberZUpRightHanded = 2,
}

/// Material values represented by the Metal voxel payload.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct SurfaceMaterial {
    pub base_color: [f32; 3],
    pub roughness: f32,
    pub specular: f32,
    pub transmission: f32,
    pub ior: f32,
    pub emission_strength: f32,
}

impl Default for SurfaceMaterial {
    fn default() -> Self {
        Self {
            base_color: [1.0; 3],
            roughness: 1.0,
            specular: 0.0,
            transmission: 0.0,
            ior: 1.5,
            emission_strength: 0.0,
        }
    }
}

/// Exact 12-byte payload consumed by the existing Metal voxel renderer.
///
/// `packed_color` stores RGB as little-channel-order UNORM8 in bits 0..23;
/// bit 31 is the occupancy marker. `packed_properties` stores roughness,
/// specular, transmission, and `(ior - 1) / 1.5` as UNORM8. Emission remains
/// a full 32-bit float.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct VoxelCell {
    pub packed_color: u32,
    pub packed_properties: u32,
    pub emission: f32,
}

const _: [(); 12] = [(); std::mem::size_of::<VoxelCell>()];

impl VoxelCell {
    pub const OCCUPIED_MASK: u32 = 0x8000_0000;

    pub fn from_material(material: SurfaceMaterial) -> Self {
        let packed_color = pack_unorm4([
            material.base_color[0],
            material.base_color[1],
            material.base_color[2],
            0.0,
        ]) | Self::OCCUPIED_MASK;
        let packed_properties = pack_unorm4([
            material.roughness,
            material.specular,
            material.transmission,
            (material.ior - 1.0) / 1.5,
        ]);
        Self {
            packed_color,
            packed_properties,
            emission: material.emission_strength,
        }
    }

    pub const fn is_occupied(self) -> bool {
        self.packed_color & Self::OCCUPIED_MASK != 0
    }

    pub fn material(self) -> SurfaceMaterial {
        let color = unpack_unorm4(self.packed_color);
        let properties = unpack_unorm4(self.packed_properties);
        SurfaceMaterial {
            base_color: [color[0], color[1], color[2]],
            roughness: properties[0],
            specular: properties[1],
            transmission: properties[2],
            ior: 1.0 + 1.5 * properties[3],
            emission_strength: self.emission,
        }
    }

    /// Stable little-endian portable encoding of the 12-byte payload.
    pub fn to_le_bytes(self) -> [u8; 12] {
        let mut bytes = [0; 12];
        bytes[0..4].copy_from_slice(&self.packed_color.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.packed_properties.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.emission.to_bits().to_le_bytes());
        bytes
    }

    pub fn from_le_bytes(bytes: [u8; 12]) -> Self {
        Self {
            packed_color: u32::from_le_bytes(bytes[0..4].try_into().expect("fixed slice")),
            packed_properties: u32::from_le_bytes(bytes[4..8].try_into().expect("fixed slice")),
            emission: f32::from_bits(u32::from_le_bytes(
                bytes[8..12].try_into().expect("fixed slice"),
            )),
        }
    }

    fn key(self) -> MaterialKey {
        MaterialKey {
            packed_color: self.packed_color,
            packed_properties: self.packed_properties,
            emission_bits: self.emission.to_bits(),
        }
    }
}

impl PartialEq for VoxelCell {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for VoxelCell {}

impl PartialOrd for VoxelCell {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for VoxelCell {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key().cmp(&other.key())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct MaterialKey {
    packed_color: u32,
    packed_properties: u32,
    emission_bits: u32,
}

impl MaterialKey {
    fn cell(self) -> VoxelCell {
        VoxelCell {
            packed_color: self.packed_color,
            packed_properties: self.packed_properties,
            emission: f32::from_bits(self.emission_bits),
        }
    }
}

/// One occupied cell in a sparse grid.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SparseVoxel {
    pub coordinate: [u32; 3],
    pub cell: VoxelCell,
}

/// Deterministic sparse volume returned by [`voxelize`].
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VoxelGrid {
    pub contract_version: u32,
    pub resolution: [u32; 3],
    pub bounds: Aabb,
    pub coordinate_system: CoordinateSystem,
    pub source_label: String,
    pub source_sha256: String,
    /// Occupied cells sorted in Z-major/Y-major/X-major linear index order.
    pub voxels: Vec<SparseVoxel>,
}

impl VoxelGrid {
    /// Convert the Metal builder's X-fastest dense cell buffer into the stable
    /// sparse representation, preserving every occupied 12-byte payload.
    pub fn from_dense_cells(
        resolution: [u32; 3],
        bounds: Aabb,
        coordinate_system: CoordinateSystem,
        source_label: impl Into<String>,
        source_sha256: impl Into<String>,
        cells: &[VoxelCell],
    ) -> Result<Self, FractalError> {
        bounds.validate()?;
        if resolution.iter().any(|value| *value == 0) {
            return Err(FractalError::new(
                FractalErrorCode::InvalidResolution,
                "grid resolution must be non-zero",
            ));
        }
        let expected = resolution
            .iter()
            .try_fold(1_usize, |total, value| total.checked_mul(*value as usize));
        if expected != Some(cells.len()) {
            return Err(FractalError::new(
                FractalErrorCode::Artifact,
                format!(
                    "dense voxel buffer has {} cells; expected {} for {:?}",
                    cells.len(),
                    expected.map_or_else(|| "overflow".to_owned(), |value| value.to_string()),
                    resolution
                ),
            ));
        }
        let mut voxels = Vec::new();
        for (index, cell) in cells.iter().copied().enumerate() {
            if !cell.is_occupied() {
                continue;
            }
            let plane = resolution[0] as usize * resolution[1] as usize;
            let z = index / plane;
            let remainder = index % plane;
            let y = remainder / resolution[0] as usize;
            let x = remainder % resolution[0] as usize;
            voxels.push(SparseVoxel {
                coordinate: [x as u32, y as u32, z as u32],
                cell,
            });
        }
        Ok(Self {
            contract_version: FPT_VOXEL_CONTRACT_VERSION,
            resolution,
            bounds,
            coordinate_system,
            source_label: source_label.into(),
            source_sha256: source_sha256.into(),
            voxels,
        })
    }

    pub fn occupied_voxels(&self) -> usize {
        self.voxels.len()
    }

    pub fn cell_size(&self) -> [f32; 3] {
        let size = self.bounds.size();
        std::array::from_fn(|axis| size[axis] / self.resolution[axis] as f32)
    }

    pub fn cell(&self, coordinate: [u32; 3]) -> Option<VoxelCell> {
        self.voxels
            .binary_search_by_key(&linear_index(coordinate, self.resolution), |voxel| {
                linear_index(voxel.coordinate, self.resolution)
            })
            .ok()
            .map(|index| self.voxels[index].cell)
    }
}

/// Built-in CPU-reference fractals available without an external source tree.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinFractal {
    MengerSponge,
}

/// Serializable parameters for a built-in fractal field.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BuiltinFractalScene {
    pub fractal: BuiltinFractal,
    pub iterations: u32,
    pub scale: f64,
    pub scale_divisor: f64,
    pub rotation: [f64; 2],
    pub material: SurfaceMaterial,
}

impl Default for BuiltinFractalScene {
    fn default() -> Self {
        Self {
            fractal: BuiltinFractal::MengerSponge,
            iterations: 12,
            scale: 1.0,
            scale_divisor: 3.0,
            rotation: [0.0, 0.0],
            material: SurfaceMaterial::default(),
        }
    }
}

/// External Mandelbulber scene source for a process-backed Metal export.
///
/// Calling [`voxelize`] with this variant returns
/// [`FractalErrorCode::CpuReferenceUnavailable`]. Use `fpt-metal voxel-export`
/// so generated formulas are evaluated by the existing Metal kernel.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MandelbulberSceneSource {
    pub scene_path: PathBuf,
    pub mandelbulber_root: Option<PathBuf>,
}

/// Serializable fractal scene descriptor.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FractalScene {
    Builtin { scene: BuiltinFractalScene },
    Mandelbulber { source: MandelbulberSceneSource },
}

impl FractalScene {
    pub fn menger_sponge() -> Self {
        Self::Builtin {
            scene: BuiltinFractalScene::default(),
        }
    }

    pub fn mandelbulber(scene_path: impl Into<PathBuf>) -> Self {
        Self::Mandelbulber {
            source: MandelbulberSceneSource {
                scene_path: scene_path.into(),
                mandelbulber_root: None,
            },
        }
    }
}

/// Deterministic sampling rule used inside each voxel.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CellSampling {
    /// Match the existing Metal legacy-coverage path: sample at cell center.
    Center,
}

/// Voxelization controls shared by the in-memory and CLI interfaces.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VoxelizationParameters {
    pub resolution: [u32; 3],
    pub bounds: Aabb,
    pub sampling: CellSampling,
    /// Multiplier applied to the cell half-diagonal. Values below 0.25 are
    /// clamped to 0.25 to match the Metal voxel builder.
    pub surface_band: f32,
    pub fill_interior: bool,
}

impl VoxelizationParameters {
    pub fn cubic(resolution: u32, bounds: Aabb) -> Self {
        Self {
            resolution: [resolution; 3],
            bounds,
            sampling: CellSampling::Center,
            surface_band: 1.0,
            fill_interior: false,
        }
    }

    fn validate(&self) -> Result<(), FractalError> {
        self.bounds.validate()?;
        if self
            .resolution
            .iter()
            .any(|value| !(1..=512).contains(value))
        {
            return Err(FractalError::new(
                FractalErrorCode::InvalidResolution,
                format!(
                    "voxel resolution must be 1..512 per axis, got {:?}",
                    self.resolution
                ),
            ));
        }
        let total = self
            .resolution
            .iter()
            .try_fold(1_u64, |total, value| total.checked_mul(u64::from(*value)))
            .ok_or_else(|| {
                FractalError::new(
                    FractalErrorCode::InvalidResolution,
                    "voxel grid cell count overflowed u64",
                )
            })?;
        if total > 512_u64.pow(3) {
            return Err(FractalError::new(
                FractalErrorCode::InvalidResolution,
                "voxel grid exceeds the 512^3 reference-path limit",
            ));
        }
        if !self.surface_band.is_finite() || self.surface_band < 0.0 {
            return Err(FractalError::new(
                FractalErrorCode::InvalidSampling,
                "surface_band must be finite and non-negative",
            ));
        }
        Ok(())
    }
}

/// Complete serializable request accepted by [`voxelize`].
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VoxelizationRequest {
    pub scene: FractalScene,
    pub voxelization: VoxelizationParameters,
}

/// Stable machine-readable error categories.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FractalErrorCode {
    InvalidBounds,
    InvalidResolution,
    InvalidSampling,
    InvalidScene,
    CpuReferenceUnavailable,
    Io,
    Serialization,
    Artifact,
}

/// Serializable library error with a stable category and human-readable detail.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FractalError {
    pub code: FractalErrorCode,
    pub message: String,
}

impl FractalError {
    pub(crate) fn new(code: FractalErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for FractalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for FractalError {}

/// Counts produced while encoding a voxel grid as GLB.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlbExportSummary {
    pub occupied_voxels: usize,
    pub quads: usize,
    pub triangles: usize,
    pub vertices: usize,
    pub materials: usize,
    pub bytes: usize,
}

enum PreparedField {
    Menger(BuiltinFractalScene),
}

impl PreparedField {
    fn prepare(
        scene: &FractalScene,
    ) -> Result<(Self, CoordinateSystem, String, String), FractalError> {
        match scene {
            FractalScene::Builtin { scene } => {
                if scene.iterations == 0 || scene.iterations > 64 {
                    return Err(FractalError::new(
                        FractalErrorCode::InvalidScene,
                        "built-in iteration count must be 1..64",
                    ));
                }
                if !scene.scale.is_finite()
                    || !scene.scale_divisor.is_finite()
                    || scene.scale <= 0.0
                    || scene.scale_divisor <= 1.0
                    || scene.rotation.iter().any(|value| !value.is_finite())
                {
                    return Err(FractalError::new(
                        FractalErrorCode::InvalidScene,
                        "invalid built-in Menger parameters",
                    ));
                }
                let serialized = serde_json::to_vec(scene).map_err(serialization_error)?;
                Ok((
                    Self::Menger(scene.clone()),
                    CoordinateSystem::YUpRightHanded,
                    "builtin:menger-sponge".to_owned(),
                    sha256_hex(&serialized),
                ))
            }
            FractalScene::Mandelbulber { source } => Err(FractalError::new(
                FractalErrorCode::CpuReferenceUnavailable,
                format!(
                    "{} requires the authoritative Metal evaluator; use `fpt-metal voxel-export`",
                    source.scene_path.display()
                ),
            )),
        }
    }

    fn distance_and_material(&self, point: [f64; 3]) -> (f64, SurfaceMaterial) {
        match self {
            Self::Menger(scene) => (menger_distance(point, scene), scene.material),
        }
    }
}

/// Deterministically sample a fractal field into a sparse voxel grid.
pub fn voxelize(request: &VoxelizationRequest) -> Result<VoxelGrid, FractalError> {
    request.voxelization.validate()?;
    let (field, coordinate_system, source_label, source_sha256) =
        PreparedField::prepare(&request.scene)?;
    let parameters = &request.voxelization;
    let size = parameters.bounds.size();
    let cell_size: [f64; 3] =
        std::array::from_fn(|axis| f64::from(size[axis]) / f64::from(parameters.resolution[axis]));
    let half_extent: [f64; 3] = cell_size.map(|value| value * 0.5);
    let radius = (half_extent[0] * half_extent[0]
        + half_extent[1] * half_extent[1]
        + half_extent[2] * half_extent[2])
        .sqrt();
    let threshold = radius * f64::from(parameters.surface_band.max(0.25));
    let capacity = parameters
        .resolution
        .iter()
        .fold(1_usize, |total, value| {
            total.saturating_mul(*value as usize)
        })
        .min(1_000_000);
    let mut voxels = Vec::with_capacity(capacity / 8);

    for z in 0..parameters.resolution[2] {
        for y in 0..parameters.resolution[1] {
            for x in 0..parameters.resolution[0] {
                let coordinate = [x, y, z];
                let center: [f64; 3] = std::array::from_fn(|axis| {
                    f64::from(parameters.bounds.min[axis])
                        + (f64::from(coordinate[axis]) + 0.5) * cell_size[axis]
                });
                let (distance, material) = field.distance_and_material(center);
                let occupied = distance.is_finite()
                    && (distance.abs() <= threshold
                        || (parameters.fill_interior && distance < 0.0));
                if occupied {
                    voxels.push(SparseVoxel {
                        coordinate,
                        cell: VoxelCell::from_material(material),
                    });
                }
            }
        }
    }

    Ok(VoxelGrid {
        contract_version: FPT_VOXEL_CONTRACT_VERSION,
        resolution: parameters.resolution,
        bounds: parameters.bounds,
        coordinate_system,
        source_label,
        source_sha256,
        voxels,
    })
}

/// Voxelize and immediately encode a portable GLB artifact.
pub fn voxelize_to_glb(
    request: &VoxelizationRequest,
    path: impl AsRef<Path>,
) -> Result<(VoxelGrid, GlbExportSummary), FractalError> {
    let grid = voxelize(request)?;
    let summary = export_glb(&grid, path)?;
    Ok((grid, summary))
}

/// Greedy-mesh occupied boundary faces and write a deterministic glTF 2.0 GLB.
pub fn export_glb(
    grid: &VoxelGrid,
    path: impl AsRef<Path>,
) -> Result<GlbExportSummary, FractalError> {
    validate_grid(grid)?;
    let mesh = greedy_mesh(grid);
    let (json_document, binary) = encode_gltf(grid, &mesh)?;
    let glb = encode_glb(&json_document, &binary)?;
    let path = path.as_ref();
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            FractalError::new(
                FractalErrorCode::Io,
                format!("create {}: {error}", parent.display()),
            )
        })?;
    }
    fs::write(path, &glb).map_err(|error| {
        FractalError::new(
            FractalErrorCode::Io,
            format!("write {}: {error}", path.display()),
        )
    })?;
    Ok(GlbExportSummary {
        occupied_voxels: grid.voxels.len(),
        quads: mesh.quads,
        triangles: mesh.quads * 2,
        vertices: mesh.positions.len(),
        materials: mesh.indices.len(),
        bytes: glb.len(),
    })
}

pub(crate) fn validate_grid(grid: &VoxelGrid) -> Result<(), FractalError> {
    if grid.contract_version != FPT_VOXEL_CONTRACT_VERSION {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            format!(
                "unsupported voxel contract version {}",
                grid.contract_version
            ),
        ));
    }
    grid.bounds.validate()?;
    if grid.resolution.iter().any(|value| *value == 0) {
        return Err(FractalError::new(
            FractalErrorCode::InvalidResolution,
            "grid resolution must be non-zero",
        ));
    }
    let cell_capacity = grid
        .resolution
        .iter()
        .fold(1_u128, |total, value| total * u128::from(*value));
    if grid.voxels.len() as u128 > cell_capacity {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            format!(
                "voxel count {} exceeds grid capacity {cell_capacity}",
                grid.voxels.len()
            ),
        ));
    }
    let mut previous = None;
    for voxel in &grid.voxels {
        if voxel
            .coordinate
            .iter()
            .zip(grid.resolution)
            .any(|(coordinate, resolution)| *coordinate >= resolution)
            || !voxel.cell.is_occupied()
            || !voxel.cell.emission.is_finite()
        {
            return Err(FractalError::new(
                FractalErrorCode::Artifact,
                "grid contains an invalid sparse voxel",
            ));
        }
        let index = linear_index(voxel.coordinate, grid.resolution);
        if previous.is_some_and(|value| value >= index) {
            return Err(FractalError::new(
                FractalErrorCode::Artifact,
                "sparse voxels must be strictly sorted without duplicates",
            ));
        }
        previous = Some(index);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FaceMask {
    material: MaterialKey,
    sign: i8,
}

struct GreedyMesh {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: BTreeMap<MaterialKey, Vec<u32>>,
    quads: usize,
}

fn greedy_mesh(grid: &VoxelGrid) -> GreedyMesh {
    let lookup = grid
        .voxels
        .iter()
        .map(|voxel| (voxel.coordinate, voxel.cell))
        .collect::<HashMap<_, _>>();
    let mut mesh = GreedyMesh {
        positions: Vec::new(),
        normals: Vec::new(),
        indices: BTreeMap::new(),
        quads: 0,
    };
    let dimensions = grid.resolution;
    let cell_size = grid.cell_size();

    for axis in 0..3 {
        let u = (axis + 1) % 3;
        let v = (axis + 2) % 3;
        let width = dimensions[u] as usize;
        let height = dimensions[v] as usize;
        let mut mask = vec![None::<FaceMask>; width * height];
        for slice in 0..=dimensions[axis] {
            for j in 0..dimensions[v] {
                for i in 0..dimensions[u] {
                    let mut lower = [0; 3];
                    lower[axis] = slice.saturating_sub(1);
                    lower[u] = i;
                    lower[v] = j;
                    let mut upper = lower;
                    upper[axis] = slice;
                    let a = (slice > 0).then(|| lookup.get(&lower).copied()).flatten();
                    let b = (slice < dimensions[axis])
                        .then(|| lookup.get(&upper).copied())
                        .flatten();
                    mask[j as usize * width + i as usize] = match (a, b) {
                        (Some(cell), None) => Some(FaceMask {
                            material: cell.key(),
                            sign: 1,
                        }),
                        (None, Some(cell)) => Some(FaceMask {
                            material: cell.key(),
                            sign: -1,
                        }),
                        _ => None,
                    };
                }
            }

            let mut j = 0usize;
            while j < height {
                let mut i = 0usize;
                while i < width {
                    let Some(face) = mask[j * width + i] else {
                        i += 1;
                        continue;
                    };
                    let mut run_width = 1usize;
                    while i + run_width < width && mask[j * width + i + run_width] == Some(face) {
                        run_width += 1;
                    }
                    let mut run_height = 1usize;
                    'height: while j + run_height < height {
                        for offset in 0..run_width {
                            if mask[(j + run_height) * width + i + offset] != Some(face) {
                                break 'height;
                            }
                        }
                        run_height += 1;
                    }
                    emit_quad(
                        &mut mesh,
                        grid,
                        cell_size,
                        axis,
                        u,
                        v,
                        slice,
                        i as u32,
                        j as u32,
                        run_width as u32,
                        run_height as u32,
                        face,
                    );
                    for row in 0..run_height {
                        for column in 0..run_width {
                            mask[(j + row) * width + i + column] = None;
                        }
                    }
                    i += run_width;
                }
                j += 1;
            }
        }
    }
    mesh
}

#[allow(clippy::too_many_arguments)]
fn emit_quad(
    mesh: &mut GreedyMesh,
    grid: &VoxelGrid,
    cell_size: [f32; 3],
    axis: usize,
    u: usize,
    v: usize,
    slice: u32,
    i: u32,
    j: u32,
    width: u32,
    height: u32,
    face: FaceMask,
) {
    let mut base = [0.0_f32; 3];
    base[axis] = slice as f32;
    base[u] = i as f32;
    base[v] = j as f32;
    let mut du = [0.0_f32; 3];
    du[u] = width as f32;
    let mut dv = [0.0_f32; 3];
    dv[v] = height as f32;
    let grid_vertices = if face.sign > 0 {
        [
            base,
            add3(base, du),
            add3(add3(base, du), dv),
            add3(base, dv),
        ]
    } else {
        [
            base,
            add3(base, dv),
            add3(add3(base, du), dv),
            add3(base, du),
        ]
    };
    let first = mesh.positions.len() as u32;
    let mut source_normal = [0.0_f32; 3];
    source_normal[axis] = f32::from(face.sign);
    let normal = to_gltf_vector(source_normal, grid.coordinate_system);
    for vertex in grid_vertices {
        let source_position = std::array::from_fn(|component| {
            grid.bounds.min[component] + vertex[component] * cell_size[component]
        });
        mesh.positions
            .push(to_gltf_position(source_position, grid.coordinate_system));
        mesh.normals.push(normal);
    }
    mesh.indices
        .entry(face.material)
        .or_default()
        .extend_from_slice(&[first, first + 1, first + 2, first, first + 2, first + 3]);
    mesh.quads += 1;
}

fn encode_gltf(grid: &VoxelGrid, mesh: &GreedyMesh) -> Result<(Value, Vec<u8>), FractalError> {
    let mut binary = Vec::new();
    let position_offset = append_f32_vec3(&mut binary, &mesh.positions);
    let position_length = mesh.positions.len() * 12;
    let normal_offset = append_f32_vec3(&mut binary, &mesh.normals);
    let normal_length = mesh.normals.len() * 12;
    let (position_min, position_max) = position_bounds(&mesh.positions);

    let mut buffer_views = vec![
        json!({"buffer":0,"byteOffset":position_offset,"byteLength":position_length,"target":34962}),
        json!({"buffer":0,"byteOffset":normal_offset,"byteLength":normal_length,"target":34962}),
    ];
    let mut accessors = vec![
        json!({
            "bufferView":0,"componentType":5126,"count":mesh.positions.len(),"type":"VEC3",
            "min":position_min,"max":position_max
        }),
        json!({"bufferView":1,"componentType":5126,"count":mesh.normals.len(),"type":"VEC3"}),
    ];
    let mut materials = Vec::new();
    let mut primitives = Vec::new();
    let mut uses_emissive_strength = false;

    for (material_index, (key, indices)) in mesh.indices.iter().enumerate() {
        align4(&mut binary, 0);
        let index_offset = binary.len();
        for index in indices {
            binary.extend_from_slice(&index.to_le_bytes());
        }
        let view_index = buffer_views.len();
        buffer_views.push(json!({
            "buffer":0,"byteOffset":index_offset,"byteLength":indices.len()*4,"target":34963
        }));
        let accessor_index = accessors.len();
        accessors.push(json!({
            "bufferView":view_index,"componentType":5125,"count":indices.len(),"type":"SCALAR"
        }));
        primitives.push(json!({
            "attributes":{"POSITION":0,"NORMAL":1},
            "indices":accessor_index,
            "material":material_index,
            "mode":4
        }));
        let cell = key.cell();
        let material = cell.material();
        let mut extensions = serde_json::Map::new();
        extensions.insert(
            "KHR_materials_specular".into(),
            json!({"specularFactor":material.specular}),
        );
        extensions.insert(
            "KHR_materials_transmission".into(),
            json!({"transmissionFactor":material.transmission}),
        );
        extensions.insert("KHR_materials_ior".into(), json!({"ior":material.ior}));
        let emissive = if material.emission_strength > 0.0 {
            uses_emissive_strength = true;
            extensions.insert(
                "KHR_materials_emissive_strength".into(),
                json!({"emissiveStrength":material.emission_strength}),
            );
            material.base_color
        } else {
            [0.0; 3]
        };
        materials.push(json!({
            "name":format!("fpt_material_{material_index}"),
            "pbrMetallicRoughness":{
                "baseColorFactor":[material.base_color[0],material.base_color[1],material.base_color[2],1.0],
                "metallicFactor":0.0,
                "roughnessFactor":material.roughness
            },
            "emissiveFactor":emissive,
            "extensions":extensions,
            "extras":{"fpt_voxel_cell":{
                "packed_color":cell.packed_color,
                "packed_properties":cell.packed_properties,
                "emission":cell.emission
            }}
        }));
    }

    let mut extensions_used = vec![
        "KHR_materials_ior",
        "KHR_materials_specular",
        "KHR_materials_transmission",
    ];
    if uses_emissive_strength {
        extensions_used.push("KHR_materials_emissive_strength");
    }
    let document = json!({
        "asset":{
            "version":"2.0",
            "generator":"fpt-metal",
            "extras":{"fpt_voxel_contract":{
                "name":FPT_VOXEL_CONTRACT_NAME,
                "version":FPT_VOXEL_CONTRACT_VERSION,
                "voxel_cell_bytes":12,
                "cell_encoding":"rgba8_occupancy_bit31+pbr_unorm8+emission_f32",
                "source_label":grid.source_label,
                "source_sha256":grid.source_sha256,
                "source_coordinate_system":grid.coordinate_system,
                "gltf_coordinate_system":"y_up_right_handed",
                "resolution":grid.resolution,
                "bounds":grid.bounds,
                "bounds_min":grid.bounds.min,
                "bounds_max":grid.bounds.max,
                "occupied_cells":grid.voxels.len(),
                "surface_quads":mesh.quads,
                "occupied_voxels":grid.voxels.len(),
                "greedy_quads":mesh.quads
            }}
        },
        "extensionsUsed":extensions_used,
        "scene":0,
        "scenes":[{"nodes":[0]}],
        "nodes":[{"mesh":0,"name":grid.source_label}],
        "meshes":[{"name":grid.source_label,"primitives":primitives}],
        "materials":materials,
        "buffers":[{"byteLength":binary.len()}],
        "bufferViews":buffer_views,
        "accessors":accessors
    });
    Ok((document, binary))
}

fn encode_glb(document: &Value, binary: &[u8]) -> Result<Vec<u8>, FractalError> {
    let mut json_bytes = serde_json::to_vec(document).map_err(serialization_error)?;
    align4(&mut json_bytes, b' ');
    let mut binary = binary.to_vec();
    align4(&mut binary, 0);
    let total_length = 12usize
        .checked_add(8 + json_bytes.len())
        .and_then(|length| length.checked_add(8 + binary.len()))
        .ok_or_else(|| FractalError::new(FractalErrorCode::Artifact, "GLB size overflow"))?;
    let total_length = u32::try_from(total_length).map_err(|_| {
        FractalError::new(
            FractalErrorCode::Artifact,
            "GLB exceeds the 4 GiB format limit",
        )
    })?;
    let mut glb = Vec::with_capacity(total_length as usize);
    glb.extend_from_slice(&0x4654_6c67_u32.to_le_bytes());
    glb.extend_from_slice(&2_u32.to_le_bytes());
    glb.extend_from_slice(&total_length.to_le_bytes());
    glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x4e4f_534a_u32.to_le_bytes());
    glb.extend_from_slice(&json_bytes);
    glb.extend_from_slice(&(binary.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x004e_4942_u32.to_le_bytes());
    glb.extend_from_slice(&binary);
    Ok(glb)
}

fn append_f32_vec3(binary: &mut Vec<u8>, values: &[[f32; 3]]) -> usize {
    align4(binary, 0);
    let offset = binary.len();
    for value in values {
        for component in value {
            binary.extend_from_slice(&component.to_le_bytes());
        }
    }
    offset
}

fn position_bounds(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    if positions.is_empty() {
        return ([0.0; 3], [0.0; 3]);
    }
    let mut minimum = [f32::INFINITY; 3];
    let mut maximum = [f32::NEG_INFINITY; 3];
    for position in positions {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(position[axis]);
            maximum[axis] = maximum[axis].max(position[axis]);
        }
    }
    (minimum, maximum)
}

fn to_gltf_position(position: [f32; 3], system: CoordinateSystem) -> [f32; 3] {
    match system {
        CoordinateSystem::YUpRightHanded => position,
        CoordinateSystem::MandelbulberZUpRightHanded => [position[0], position[2], -position[1]],
    }
}

fn to_gltf_vector(vector: [f32; 3], system: CoordinateSystem) -> [f32; 3] {
    to_gltf_position(vector, system)
}

fn menger_distance(mut point: [f64; 3], scene: &BuiltinFractalScene) -> f64 {
    let mut distance = cube_sdf(point);
    let mut scale = scene.scale;
    for _ in 0..scene.iterations {
        distance = distance.max(-menger_cut(point, scale));
        scale /= scene.scale_divisor;
        point = rotate_camera(point, scene.rotation);
    }
    distance
}

fn cube_sdf(point: [f64; 3]) -> f64 {
    (point[0].abs() - 1.0)
        .max(point[1].abs() - 1.0)
        .max(point[2].abs() - 1.0)
}

fn menger_cut(mut point: [f64; 3], scale: f64) -> f64 {
    let period = scale * 2.0;
    for value in &mut point {
        *value = (*value - scale * 0.5).rem_euclid(period) - scale * 0.5;
    }
    let a = point[0].abs().max(point[1].abs());
    let b = point[1].abs().max(point[2].abs());
    let c = point[2].abs().max(point[0].abs());
    a.min(b).min(c) - period / 6.0
}

fn rotate_camera(mut value: [f64; 3], rotation: [f64; 2]) -> [f64; 3] {
    let (sp, cp) = rotation[1].sin_cos();
    [value[1], value[2]] = [value[2] * sp + value[1] * cp, value[2] * cp - value[1] * sp];
    let (sy, cy) = rotation[0].sin_cos();
    [value[0], value[2]] = [
        value[0] * cy + value[2] * sy,
        -value[0] * sy + value[2] * cy,
    ];
    value
}

fn pack_unorm4(values: [f32; 4]) -> u32 {
    values
        .into_iter()
        .enumerate()
        .fold(0_u32, |packed, (index, value)| {
            let quantized = (value.clamp(0.0, 1.0) * 255.0).round() as u32;
            packed | (quantized << (index * 8))
        })
}

fn unpack_unorm4(value: u32) -> [f32; 4] {
    std::array::from_fn(|index| ((value >> (index * 8)) & 255) as f32 / 255.0)
}

fn add3(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| left[axis] + right[axis])
}

fn align4(bytes: &mut Vec<u8>, padding: u8) {
    while !bytes.len().is_multiple_of(4) {
        bytes.push(padding);
    }
}

fn linear_index(coordinate: [u32; 3], resolution: [u32; 3]) -> u128 {
    u128::from(coordinate[0])
        + u128::from(coordinate[1]) * u128::from(resolution[0])
        + u128::from(coordinate[2]) * u128::from(resolution[0]) * u128::from(resolution[1])
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn serialization_error(error: serde_json::Error) -> FractalError {
    FractalError::new(FractalErrorCode::Serialization, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voxel_cell_matches_metal_layout_and_round_trips() {
        let material = SurfaceMaterial {
            base_color: [0.25, 0.5, 0.75],
            roughness: 0.4,
            specular: 0.6,
            transmission: 0.2,
            ior: 1.75,
            emission_strength: 3.5,
        };
        let cell = VoxelCell::from_material(material);
        assert_eq!(std::mem::size_of::<VoxelCell>(), 12);
        assert!(cell.is_occupied());
        assert_eq!(VoxelCell::from_le_bytes(cell.to_le_bytes()), cell);
        let unpacked = cell.material();
        assert!((unpacked.base_color[0] - material.base_color[0]).abs() <= 1.0 / 255.0);
        assert!((unpacked.ior - material.ior).abs() <= 1.5 / 255.0);
        assert_eq!(unpacked.emission_strength, 3.5);
    }

    #[test]
    fn greedy_mesh_collapses_a_solid_block_to_six_quads() {
        let cell = VoxelCell::from_material(SurfaceMaterial::default());
        let mut voxels = Vec::new();
        for z in 0..2 {
            for y in 0..2 {
                for x in 0..2 {
                    voxels.push(SparseVoxel {
                        coordinate: [x, y, z],
                        cell,
                    });
                }
            }
        }
        let grid = VoxelGrid {
            contract_version: FPT_VOXEL_CONTRACT_VERSION,
            resolution: [2; 3],
            bounds: Aabb::new([-1.0; 3], [1.0; 3]),
            coordinate_system: CoordinateSystem::YUpRightHanded,
            source_label: "block".into(),
            source_sha256: "test".into(),
            voxels,
        };
        let mesh = greedy_mesh(&grid);
        assert_eq!(mesh.quads, 6);
        assert_eq!(mesh.positions.len(), 24);
        assert_eq!(mesh.indices.values().map(Vec::len).sum::<usize>(), 36);
    }
}
