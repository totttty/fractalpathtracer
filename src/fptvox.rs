//! Lossless, deterministic serialization of [`crate::VoxelGrid`].
//!
//! Version 1 is a 64-byte header followed by 24-byte sparse voxel records.
//! Every numeric field is little-endian and records retain the exact Metal
//! voxel material payload.

use crate::voxel::{FractalError, FractalErrorCode, VoxelGrid, validate_grid};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

/// Eight-byte file signature at offset 0.
pub const FPTVOX_MAGIC: [u8; 8] = *b"FPTVOX1\0";
/// Binary format version stored at offset 12.
pub const FPTVOX_VERSION: u32 = 1;
/// Header bytes before the first record. Version 1 has no reserved bytes.
pub const FPTVOX_HEADER_SIZE: u32 = 64;
/// Bytes per sparse voxel record.
pub const FPTVOX_RECORD_SIZE: u32 = 24;
/// Eight-byte file signature for records carrying a packed surface normal.
pub const FPTVOX_SURFACE_MAGIC: [u8; 8] = *b"FPTVOX2\0";
/// Surface-payload binary format version stored at offset 12.
pub const FPTVOX_SURFACE_VERSION: u32 = 2;
/// Bytes per version-2 sparse voxel record.
pub const FPTVOX_SURFACE_RECORD_SIZE: u32 = 28;
/// Eight-byte file signature for records carrying a packed local surface plane.
pub const FPTVOX_PLANE_MAGIC: [u8; 8] = *b"FPTVOX3\0";
/// Local-plane binary format version stored at offset 12.
pub const FPTVOX_PLANE_VERSION: u32 = 3;
/// Bytes per version-3 sparse voxel record.
pub const FPTVOX_PLANE_RECORD_SIZE: u32 = 28;
/// Eight-byte file signature for records carrying up to two local surface planes.
pub const FPTVOX_PLANE_PAIR_MAGIC: [u8; 8] = *b"FPTVOX5\0";
/// Two-plane binary format version stored at offset 12. Version 4 was an
/// experimental patch-mask layout and is intentionally not part of the contract.
pub const FPTVOX_PLANE_PAIR_VERSION: u32 = 5;
/// Bytes per version-5 sparse voxel record.
pub const FPTVOX_PLANE_PAIR_RECORD_SIZE: u32 = 32;
/// Eight-byte file signature for records carrying up to two bounded local
/// surface patches.
pub const FPTVOX_BOUNDED_PATCH_MAGIC: [u8; 8] = *b"FPTVOX6\0";
/// Bounded-patch binary format version stored at offset 12.
pub const FPTVOX_BOUNDED_PATCH_VERSION: u32 = 6;
/// Bytes per version-6 sparse voxel record.
pub const FPTVOX_BOUNDED_PATCH_RECORD_SIZE: u32 = 36;
/// Eight-byte signature for exact cell-clipped triangle surface records.
pub const FPTVOX_TRIANGLE_MAGIC: [u8; 8] = *b"FPTVOX7\0";
/// Triangle-surface binary format version stored at offset 12.
pub const FPTVOX_TRIANGLE_VERSION: u32 = 7;
/// Version-7 header bytes. The extended header records triangle-stream sizes.
pub const FPTVOX_TRIANGLE_HEADER_SIZE: u32 = 96;
/// Bytes per occupied version-7 cell record.
pub const FPTVOX_TRIANGLE_CELL_RECORD_SIZE: u32 = 32;
/// Bytes per quantized version-7 triangle record.
pub const FPTVOX_TRIANGLE_RECORD_SIZE: u32 = 12;
/// Eight-byte signature for source-triangle records indexed by occupied cells.
pub const FPTVOX_INDEXED_TRIANGLE_MAGIC: [u8; 8] = *b"FPTVOX8\0";
/// Indexed triangle-surface binary format version stored at offset 12.
pub const FPTVOX_INDEXED_TRIANGLE_VERSION: u32 = 8;
/// Version-8 header bytes. The extended header records reference-stream sizes.
pub const FPTVOX_INDEXED_TRIANGLE_HEADER_SIZE: u32 = 112;
/// Bytes per occupied version-8 cell record.
pub const FPTVOX_INDEXED_TRIANGLE_CELL_RECORD_SIZE: u32 = 32;
/// Bytes per globally quantized source-triangle record.
pub const FPTVOX_INDEXED_TRIANGLE_RECORD_SIZE: u32 = 20;
/// Bytes per cell-to-triangle reference.
pub const FPTVOX_INDEXED_TRIANGLE_REFERENCE_SIZE: u32 = 4;
/// Maximum indexed triangles tested for one occupied cell by the V8 consumer.
pub const FPTVOX_INDEXED_TRIANGLE_MAX_REFERENCES_PER_CELL: u32 = 1024;

/// One cell-local triangle. Each vertex word stores three 10-bit UNORM
/// coordinates in X/Y/Z order; the two high bits are reserved and zero.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FptvoxTriangle {
    pub vertices: [u32; 3],
}

/// One occupied V7 cell and its contiguous triangle range.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FptvoxTriangleCell {
    pub coordinate: [u32; 3],
    pub cell: crate::voxel::VoxelCell,
    pub first_triangle: u32,
    pub triangle_count: u32,
}

/// Complete exact-surface payload written by FPTVOX7.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FptvoxTriangleSurface {
    pub resolution: [u32; 3],
    pub sampling_resolution: [u32; 3],
    pub bounds: crate::voxel::Aabb,
    pub coordinate_system: crate::voxel::CoordinateSystem,
    pub cells: Vec<FptvoxTriangleCell>,
    pub triangles: Vec<FptvoxTriangle>,
}

/// One source triangle with global UNORM16 coordinates relative to the volume bounds.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FptvoxIndexedTriangle {
    pub vertices: [[u16; 3]; 3],
}

/// One occupied V8 cell and its contiguous triangle-reference range.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FptvoxIndexedTriangleCell {
    pub coordinate: [u32; 3],
    pub cell: crate::voxel::VoxelCell,
    pub first_reference: u32,
    pub reference_count: u32,
}

/// Source triangles plus the occupied-cell index used by FPTVOX8.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FptvoxIndexedTriangleSurface {
    pub resolution: [u32; 3],
    pub sampling_resolution: [u32; 3],
    pub bounds: crate::voxel::Aabb,
    pub coordinate_system: crate::voxel::CoordinateSystem,
    pub cells: Vec<FptvoxIndexedTriangleCell>,
    pub triangles: Vec<FptvoxIndexedTriangle>,
    pub references: Vec<u32>,
}

/// Counts returned after writing a lossless `.fptvox` artifact.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FptvoxExportSummary {
    pub voxel_count: u64,
    pub bytes: u64,
}

/// Write a [`VoxelGrid`] as the version-1 little-endian `.fptvox` format.
///
/// Records preserve the grid's deterministic order. Invalid coordinates,
/// duplicate/out-of-order cells, empty dimensions, non-finite emission, and
/// impossible record counts are rejected before the output file is created.
pub fn export_fptvox(
    grid: &VoxelGrid,
    path: impl AsRef<Path>,
) -> Result<FptvoxExportSummary, FractalError> {
    validate_grid(grid)?;
    let voxel_count = u64::try_from(grid.voxels.len()).map_err(|_| {
        FractalError::new(
            FractalErrorCode::Artifact,
            "voxel record count exceeds the format's u64 limit",
        )
    })?;
    let bytes = u64::from(FPTVOX_HEADER_SIZE)
        .checked_add(
            voxel_count
                .checked_mul(u64::from(FPTVOX_RECORD_SIZE))
                .ok_or_else(|| {
                    FractalError::new(FractalErrorCode::Artifact, "FPTVOX size overflow")
                })?,
        )
        .ok_or_else(|| FractalError::new(FractalErrorCode::Artifact, "FPTVOX size overflow"))?;

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
    let file = File::create(path).map_err(|error| {
        FractalError::new(
            FractalErrorCode::Io,
            format!("create {}: {error}", path.display()),
        )
    })?;
    let mut writer = BufWriter::new(file);
    let write_result = (|| -> std::io::Result<()> {
        writer.write_all(&FPTVOX_MAGIC)?;
        writer.write_all(&FPTVOX_HEADER_SIZE.to_le_bytes())?;
        writer.write_all(&FPTVOX_VERSION.to_le_bytes())?;
        for value in grid.resolution {
            writer.write_all(&value.to_le_bytes())?;
        }
        writer.write_all(&(grid.coordinate_system as u32).to_le_bytes())?;
        for value in grid.bounds.min {
            writer.write_all(&value.to_bits().to_le_bytes())?;
        }
        for value in grid.bounds.max {
            writer.write_all(&value.to_bits().to_le_bytes())?;
        }
        writer.write_all(&voxel_count.to_le_bytes())?;
        for voxel in &grid.voxels {
            for value in voxel.coordinate {
                writer.write_all(&value.to_le_bytes())?;
            }
            writer.write_all(&voxel.cell.packed_color.to_le_bytes())?;
            writer.write_all(&voxel.cell.packed_properties.to_le_bytes())?;
            writer.write_all(&voxel.cell.emission.to_bits().to_le_bytes())?;
        }
        writer.flush()
    })();
    write_result.map_err(|error| {
        FractalError::new(
            FractalErrorCode::Io,
            format!("write {}: {error}", path.display()),
        )
    })?;
    Ok(FptvoxExportSummary { voxel_count, bytes })
}

/// Write a version-7 exact triangle-surface artifact.
///
/// Cells must be strictly X-fastest sorted. Triangle ranges must be contiguous,
/// non-empty, and together cover the triangle stream exactly once. Coordinates
/// are quantized before this function so serialization is deterministic.
pub fn export_fptvox_triangle_surface(
    surface: &FptvoxTriangleSurface,
    path: impl AsRef<Path>,
) -> Result<FptvoxExportSummary, FractalError> {
    if surface.resolution.iter().any(|value| *value == 0)
        || surface.sampling_resolution.iter().any(|value| *value < 2)
    {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX7 resolutions are invalid",
        ));
    }
    if surface
        .bounds
        .min
        .iter()
        .zip(surface.bounds.max)
        .any(|(min, max)| !min.is_finite() || !max.is_finite() || *min >= max)
    {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX7 bounds are invalid",
        ));
    }
    if surface.cells.is_empty() || surface.triangles.is_empty() {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX7 requires non-empty cells and triangles",
        ));
    }
    let mut previous_linear = None;
    let mut next_triangle = 0_u64;
    for cell in &surface.cells {
        let coordinate_invalid = cell
            .coordinate
            .iter()
            .zip(surface.resolution)
            .any(|(value, limit)| *value >= limit);
        if coordinate_invalid
            || !cell.cell.is_occupied()
            || !cell.cell.emission.is_finite()
            || cell.triangle_count == 0
            || u64::from(cell.first_triangle) != next_triangle
        {
            return Err(FractalError::new(
                FractalErrorCode::Artifact,
                format!(
                    "FPTVOX7 invalid cell {:?}: coordinate_invalid={coordinate_invalid}, occupied={}, emission={}, first={}, expected_first={next_triangle}, count={}",
                    cell.coordinate,
                    cell.cell.is_occupied(),
                    cell.cell.emission,
                    cell.first_triangle,
                    cell.triangle_count
                ),
            ));
        }
        let linear = u64::from(cell.coordinate[0])
            + u64::from(cell.coordinate[1]) * u64::from(surface.resolution[0])
            + u64::from(cell.coordinate[2])
                * u64::from(surface.resolution[0])
                * u64::from(surface.resolution[1]);
        if previous_linear.is_some_and(|previous| linear <= previous) {
            return Err(FractalError::new(
                FractalErrorCode::Artifact,
                "FPTVOX7 cells must be strictly sorted without duplicates",
            ));
        }
        previous_linear = Some(linear);
        next_triangle = next_triangle
            .checked_add(u64::from(cell.triangle_count))
            .ok_or_else(|| {
                FractalError::new(FractalErrorCode::Artifact, "triangle count overflow")
            })?;
    }
    if next_triangle != surface.triangles.len() as u64
        || surface
            .triangles
            .iter()
            .flat_map(|triangle| triangle.vertices)
            .any(|vertex| vertex >> 30 != 0)
    {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX7 triangle stream is invalid",
        ));
    }
    let cell_count = u64::try_from(surface.cells.len()).map_err(|_| {
        FractalError::new(FractalErrorCode::Artifact, "FPTVOX7 cell count overflow")
    })?;
    let triangle_count = u64::try_from(surface.triangles.len()).map_err(|_| {
        FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX7 triangle count overflow",
        )
    })?;
    if triangle_count > u64::from(u32::MAX) {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX7 triangle stream exceeds the 32-bit cell offset range",
        ));
    }
    let bytes = u64::from(FPTVOX_TRIANGLE_HEADER_SIZE)
        .checked_add(cell_count * u64::from(FPTVOX_TRIANGLE_CELL_RECORD_SIZE))
        .and_then(|bytes| {
            bytes.checked_add(triangle_count * u64::from(FPTVOX_TRIANGLE_RECORD_SIZE))
        })
        .ok_or_else(|| FractalError::new(FractalErrorCode::Artifact, "FPTVOX7 size overflow"))?;

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
    let file = File::create(path).map_err(|error| {
        FractalError::new(
            FractalErrorCode::Io,
            format!("create {}: {error}", path.display()),
        )
    })?;
    let mut writer = BufWriter::new(file);
    let write_result = (|| -> std::io::Result<()> {
        writer.write_all(&FPTVOX_TRIANGLE_MAGIC)?;
        writer.write_all(&FPTVOX_TRIANGLE_HEADER_SIZE.to_le_bytes())?;
        writer.write_all(&FPTVOX_TRIANGLE_VERSION.to_le_bytes())?;
        for value in surface.resolution {
            writer.write_all(&value.to_le_bytes())?;
        }
        writer.write_all(&(surface.coordinate_system as u32).to_le_bytes())?;
        for value in surface.bounds.min {
            writer.write_all(&value.to_bits().to_le_bytes())?;
        }
        for value in surface.bounds.max {
            writer.write_all(&value.to_bits().to_le_bytes())?;
        }
        writer.write_all(&cell_count.to_le_bytes())?;
        for value in surface.sampling_resolution {
            writer.write_all(&value.to_le_bytes())?;
        }
        writer.write_all(&FPTVOX_TRIANGLE_CELL_RECORD_SIZE.to_le_bytes())?;
        writer.write_all(&triangle_count.to_le_bytes())?;
        writer.write_all(&FPTVOX_TRIANGLE_RECORD_SIZE.to_le_bytes())?;
        writer.write_all(&0_u32.to_le_bytes())?;
        for cell in &surface.cells {
            for value in cell.coordinate {
                writer.write_all(&value.to_le_bytes())?;
            }
            writer.write_all(&cell.cell.packed_color.to_le_bytes())?;
            writer.write_all(&cell.cell.packed_properties.to_le_bytes())?;
            writer.write_all(&cell.cell.emission.to_bits().to_le_bytes())?;
            writer.write_all(&cell.first_triangle.to_le_bytes())?;
            writer.write_all(&cell.triangle_count.to_le_bytes())?;
        }
        for triangle in &surface.triangles {
            for vertex in triangle.vertices {
                writer.write_all(&vertex.to_le_bytes())?;
            }
        }
        writer.flush()
    })();
    write_result.map_err(|error| {
        FractalError::new(
            FractalErrorCode::Io,
            format!("write {}: {error}", path.display()),
        )
    })?;
    Ok(FptvoxExportSummary {
        voxel_count: cell_count,
        bytes,
    })
}

/// Write a version-8 indexed source-triangle surface artifact.
///
/// Unlike V7, source triangles are stored once. Each occupied cell owns a
/// contiguous list of source-triangle indices whose geometry intersects it.
pub fn export_fptvox_indexed_triangle_surface(
    surface: &FptvoxIndexedTriangleSurface,
    path: impl AsRef<Path>,
) -> Result<FptvoxExportSummary, FractalError> {
    if surface.resolution.iter().any(|value| *value == 0)
        || surface.sampling_resolution.iter().any(|value| *value < 2)
    {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX8 resolutions are invalid",
        ));
    }
    if surface
        .bounds
        .min
        .iter()
        .zip(surface.bounds.max)
        .any(|(min, max)| !min.is_finite() || !max.is_finite() || *min >= max)
    {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX8 bounds are invalid",
        ));
    }
    if surface.cells.is_empty() || surface.triangles.is_empty() || surface.references.is_empty() {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX8 requires non-empty cells, triangles, and references",
        ));
    }
    let triangle_count = u32::try_from(surface.triangles.len()).map_err(|_| {
        FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX8 triangle count exceeds the 32-bit reference range",
        )
    })?;
    let mut previous_linear = None;
    let mut next_reference = 0_u64;
    for cell in &surface.cells {
        let coordinate_invalid = cell
            .coordinate
            .iter()
            .zip(surface.resolution)
            .any(|(value, limit)| *value >= limit);
        if cell.reference_count > FPTVOX_INDEXED_TRIANGLE_MAX_REFERENCES_PER_CELL {
            return Err(FractalError::new(
                FractalErrorCode::Artifact,
                format!(
                    "FPTVOX8 cell requires {} triangle references; limit is {}",
                    cell.reference_count, FPTVOX_INDEXED_TRIANGLE_MAX_REFERENCES_PER_CELL
                ),
            ));
        }
        if coordinate_invalid
            || !cell.cell.is_occupied()
            || !cell.cell.emission.is_finite()
            || cell.reference_count == 0
            || u64::from(cell.first_reference) != next_reference
        {
            return Err(FractalError::new(
                FractalErrorCode::Artifact,
                "FPTVOX8 contains an invalid cell or reference range",
            ));
        }
        let linear = u64::from(cell.coordinate[0])
            + u64::from(cell.coordinate[1]) * u64::from(surface.resolution[0])
            + u64::from(cell.coordinate[2])
                * u64::from(surface.resolution[0])
                * u64::from(surface.resolution[1]);
        if previous_linear.is_some_and(|previous| linear <= previous) {
            return Err(FractalError::new(
                FractalErrorCode::Artifact,
                "FPTVOX8 cells must be strictly sorted without duplicates",
            ));
        }
        previous_linear = Some(linear);
        next_reference = next_reference
            .checked_add(u64::from(cell.reference_count))
            .ok_or_else(|| {
                FractalError::new(FractalErrorCode::Artifact, "reference count overflow")
            })?;
    }
    if next_reference != surface.references.len() as u64
        || surface
            .references
            .iter()
            .any(|reference| *reference >= triangle_count)
    {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX8 reference stream is invalid",
        ));
    }
    let cell_count = u64::try_from(surface.cells.len()).map_err(|_| {
        FractalError::new(FractalErrorCode::Artifact, "FPTVOX8 cell count overflow")
    })?;
    let triangle_count = u64::from(triangle_count);
    let reference_count = u64::try_from(surface.references.len()).map_err(|_| {
        FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX8 reference count overflow",
        )
    })?;
    let bytes = u64::from(FPTVOX_INDEXED_TRIANGLE_HEADER_SIZE)
        .checked_add(cell_count * u64::from(FPTVOX_INDEXED_TRIANGLE_CELL_RECORD_SIZE))
        .and_then(|bytes| {
            bytes.checked_add(triangle_count * u64::from(FPTVOX_INDEXED_TRIANGLE_RECORD_SIZE))
        })
        .and_then(|bytes| {
            bytes.checked_add(reference_count * u64::from(FPTVOX_INDEXED_TRIANGLE_REFERENCE_SIZE))
        })
        .ok_or_else(|| FractalError::new(FractalErrorCode::Artifact, "FPTVOX8 size overflow"))?;

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
    let file = File::create(path).map_err(|error| {
        FractalError::new(
            FractalErrorCode::Io,
            format!("create {}: {error}", path.display()),
        )
    })?;
    let mut writer = BufWriter::new(file);
    let write_result = (|| -> std::io::Result<()> {
        writer.write_all(&FPTVOX_INDEXED_TRIANGLE_MAGIC)?;
        writer.write_all(&FPTVOX_INDEXED_TRIANGLE_HEADER_SIZE.to_le_bytes())?;
        writer.write_all(&FPTVOX_INDEXED_TRIANGLE_VERSION.to_le_bytes())?;
        for value in surface.resolution {
            writer.write_all(&value.to_le_bytes())?;
        }
        writer.write_all(&(surface.coordinate_system as u32).to_le_bytes())?;
        for value in surface.bounds.min {
            writer.write_all(&value.to_bits().to_le_bytes())?;
        }
        for value in surface.bounds.max {
            writer.write_all(&value.to_bits().to_le_bytes())?;
        }
        writer.write_all(&cell_count.to_le_bytes())?;
        for value in surface.sampling_resolution {
            writer.write_all(&value.to_le_bytes())?;
        }
        writer.write_all(&FPTVOX_INDEXED_TRIANGLE_CELL_RECORD_SIZE.to_le_bytes())?;
        writer.write_all(&triangle_count.to_le_bytes())?;
        writer.write_all(&FPTVOX_INDEXED_TRIANGLE_RECORD_SIZE.to_le_bytes())?;
        writer.write_all(&0_u32.to_le_bytes())?;
        writer.write_all(&reference_count.to_le_bytes())?;
        writer.write_all(&FPTVOX_INDEXED_TRIANGLE_REFERENCE_SIZE.to_le_bytes())?;
        writer.write_all(&0_u32.to_le_bytes())?;
        for cell in &surface.cells {
            for value in cell.coordinate {
                writer.write_all(&value.to_le_bytes())?;
            }
            writer.write_all(&cell.cell.packed_color.to_le_bytes())?;
            writer.write_all(&cell.cell.packed_properties.to_le_bytes())?;
            writer.write_all(&cell.cell.emission.to_bits().to_le_bytes())?;
            writer.write_all(&cell.first_reference.to_le_bytes())?;
            writer.write_all(&cell.reference_count.to_le_bytes())?;
        }
        for triangle in &surface.triangles {
            for vertex in triangle.vertices {
                for component in vertex {
                    writer.write_all(&component.to_le_bytes())?;
                }
            }
            writer.write_all(&0_u16.to_le_bytes())?;
        }
        for reference in &surface.references {
            writer.write_all(&reference.to_le_bytes())?;
        }
        writer.flush()
    })();
    write_result.map_err(|error| {
        FractalError::new(
            FractalErrorCode::Io,
            format!("write {}: {error}", path.display()),
        )
    })?;
    Ok(FptvoxExportSummary {
        voxel_count: cell_count,
        bytes,
    })
}

/// Write a version-2 `.fptvox` artifact with one packed octahedral normal per
/// sparse voxel. `packed_normals` must follow the grid's deterministic sparse
/// voxel order. Version 1 remains the default format for callers that do not
/// request the additional surface payload.
pub fn export_fptvox_with_normals(
    grid: &VoxelGrid,
    packed_normals: &[u32],
    path: impl AsRef<Path>,
) -> Result<FptvoxExportSummary, FractalError> {
    validate_grid(grid)?;
    if packed_normals.len() != grid.voxels.len() {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "surface normal count must match the sparse voxel count",
        ));
    }
    let voxel_count = u64::try_from(grid.voxels.len()).map_err(|_| {
        FractalError::new(
            FractalErrorCode::Artifact,
            "voxel record count exceeds the format's u64 limit",
        )
    })?;
    let bytes = u64::from(FPTVOX_HEADER_SIZE)
        .checked_add(
            voxel_count
                .checked_mul(u64::from(FPTVOX_SURFACE_RECORD_SIZE))
                .ok_or_else(|| {
                    FractalError::new(FractalErrorCode::Artifact, "FPTVOX size overflow")
                })?,
        )
        .ok_or_else(|| FractalError::new(FractalErrorCode::Artifact, "FPTVOX size overflow"))?;

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
    let file = File::create(path).map_err(|error| {
        FractalError::new(
            FractalErrorCode::Io,
            format!("create {}: {error}", path.display()),
        )
    })?;
    let mut writer = BufWriter::new(file);
    let write_result = (|| -> std::io::Result<()> {
        writer.write_all(&FPTVOX_SURFACE_MAGIC)?;
        writer.write_all(&FPTVOX_HEADER_SIZE.to_le_bytes())?;
        writer.write_all(&FPTVOX_SURFACE_VERSION.to_le_bytes())?;
        for value in grid.resolution {
            writer.write_all(&value.to_le_bytes())?;
        }
        writer.write_all(&(grid.coordinate_system as u32).to_le_bytes())?;
        for value in grid.bounds.min {
            writer.write_all(&value.to_bits().to_le_bytes())?;
        }
        for value in grid.bounds.max {
            writer.write_all(&value.to_bits().to_le_bytes())?;
        }
        writer.write_all(&voxel_count.to_le_bytes())?;
        for (voxel, packed_normal) in grid.voxels.iter().zip(packed_normals) {
            for value in voxel.coordinate {
                writer.write_all(&value.to_le_bytes())?;
            }
            writer.write_all(&voxel.cell.packed_color.to_le_bytes())?;
            writer.write_all(&voxel.cell.packed_properties.to_le_bytes())?;
            writer.write_all(&voxel.cell.emission.to_bits().to_le_bytes())?;
            writer.write_all(&packed_normal.to_le_bytes())?;
        }
        writer.flush()
    })();
    write_result.map_err(|error| {
        FractalError::new(
            FractalErrorCode::Io,
            format!("write {}: {error}", path.display()),
        )
    })?;
    Ok(FptvoxExportSummary { voxel_count, bytes })
}

/// Write a version-3 `.fptvox` artifact with one packed local tangent plane
/// per sparse voxel. The low 24 bits store a 12-bit-per-axis octahedral normal;
/// the high byte stores a signed sub-voxel plane offset. Version 1 remains the
/// default when no structural surface payload is requested.
pub fn export_fptvox_with_planes(
    grid: &VoxelGrid,
    packed_planes: &[u32],
    path: impl AsRef<Path>,
) -> Result<FptvoxExportSummary, FractalError> {
    export_fptvox_with_plane_payload(
        grid,
        packed_planes,
        path.as_ref(),
        FPTVOX_PLANE_MAGIC,
        FPTVOX_PLANE_VERSION,
        FPTVOX_PLANE_RECORD_SIZE,
    )
}

/// Write a version-5 `.fptvox` artifact with up to two packed local tangent
/// planes per sparse voxel. The first plane is required for every occupied
/// record; a zero second plane means that the cell has only one surface patch.
pub fn export_fptvox_with_plane_pairs(
    grid: &VoxelGrid,
    packed_plane_pairs: &[[u32; 2]],
    path: impl AsRef<Path>,
) -> Result<FptvoxExportSummary, FractalError> {
    validate_grid(grid)?;
    if packed_plane_pairs.len() != grid.voxels.len() {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "surface plane-pair count must match the sparse voxel count",
        ));
    }
    if packed_plane_pairs.iter().any(|pair| pair[0] == 0) {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "surface plane-pair records require a nonzero primary plane",
        ));
    }
    let voxel_count = u64::try_from(grid.voxels.len()).map_err(|_| {
        FractalError::new(
            FractalErrorCode::Artifact,
            "voxel record count exceeds the format's u64 limit",
        )
    })?;
    let bytes = u64::from(FPTVOX_HEADER_SIZE)
        .checked_add(
            voxel_count
                .checked_mul(u64::from(FPTVOX_PLANE_PAIR_RECORD_SIZE))
                .ok_or_else(|| {
                    FractalError::new(FractalErrorCode::Artifact, "FPTVOX size overflow")
                })?,
        )
        .ok_or_else(|| FractalError::new(FractalErrorCode::Artifact, "FPTVOX size overflow"))?;
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
    let file = File::create(path).map_err(|error| {
        FractalError::new(
            FractalErrorCode::Io,
            format!("create {}: {error}", path.display()),
        )
    })?;
    let mut writer = BufWriter::new(file);
    let write_result = (|| -> std::io::Result<()> {
        writer.write_all(&FPTVOX_PLANE_PAIR_MAGIC)?;
        writer.write_all(&FPTVOX_HEADER_SIZE.to_le_bytes())?;
        writer.write_all(&FPTVOX_PLANE_PAIR_VERSION.to_le_bytes())?;
        for value in grid.resolution {
            writer.write_all(&value.to_le_bytes())?;
        }
        writer.write_all(&(grid.coordinate_system as u32).to_le_bytes())?;
        for value in grid.bounds.min {
            writer.write_all(&value.to_bits().to_le_bytes())?;
        }
        for value in grid.bounds.max {
            writer.write_all(&value.to_bits().to_le_bytes())?;
        }
        writer.write_all(&voxel_count.to_le_bytes())?;
        for (voxel, planes) in grid.voxels.iter().zip(packed_plane_pairs) {
            for value in voxel.coordinate {
                writer.write_all(&value.to_le_bytes())?;
            }
            writer.write_all(&voxel.cell.packed_color.to_le_bytes())?;
            writer.write_all(&voxel.cell.packed_properties.to_le_bytes())?;
            writer.write_all(&voxel.cell.emission.to_bits().to_le_bytes())?;
            writer.write_all(&planes[0].to_le_bytes())?;
            writer.write_all(&planes[1].to_le_bytes())?;
        }
        writer.flush()
    })();
    write_result.map_err(|error| {
        FractalError::new(
            FractalErrorCode::Io,
            format!("write {}: {error}", path.display()),
        )
    })?;
    Ok(FptvoxExportSummary { voxel_count, bytes })
}

/// Write a version-6 `.fptvox` artifact with a V3 primary tangent plane and an
/// optional bounded secondary patch per sparse voxel. Two packed V3 planes are followed by four
/// unsigned-byte projected bounds for the optional secondary patch: `(min_u,
/// max_u, min_v, max_v)`. The primary plane is required; a zero secondary plane
/// means one patch and requires zero bounds.
pub fn export_fptvox_with_bounded_patches(
    grid: &VoxelGrid,
    packed_patches: &[[u32; 3]],
    path: impl AsRef<Path>,
) -> Result<FptvoxExportSummary, FractalError> {
    validate_grid(grid)?;
    if packed_patches.len() != grid.voxels.len() {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "bounded surface-patch count must match the sparse voxel count",
        ));
    }
    if packed_patches.iter().any(|patches| patches[0] == 0) {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "bounded surface-patch records require a nonzero primary plane",
        ));
    }
    if packed_patches
        .iter()
        .any(|patches| patches[1] == 0 && patches[2] != 0)
    {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "bounded surface-patch records cannot store secondary bounds without a secondary plane",
        ));
    }
    let voxel_count = u64::try_from(grid.voxels.len()).map_err(|_| {
        FractalError::new(
            FractalErrorCode::Artifact,
            "voxel record count exceeds the format's u64 limit",
        )
    })?;
    let bytes = u64::from(FPTVOX_HEADER_SIZE)
        .checked_add(
            voxel_count
                .checked_mul(u64::from(FPTVOX_BOUNDED_PATCH_RECORD_SIZE))
                .ok_or_else(|| {
                    FractalError::new(FractalErrorCode::Artifact, "FPTVOX size overflow")
                })?,
        )
        .ok_or_else(|| FractalError::new(FractalErrorCode::Artifact, "FPTVOX size overflow"))?;
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
    let file = File::create(path).map_err(|error| {
        FractalError::new(
            FractalErrorCode::Io,
            format!("create {}: {error}", path.display()),
        )
    })?;
    let mut writer = BufWriter::new(file);
    let write_result = (|| -> std::io::Result<()> {
        writer.write_all(&FPTVOX_BOUNDED_PATCH_MAGIC)?;
        writer.write_all(&FPTVOX_HEADER_SIZE.to_le_bytes())?;
        writer.write_all(&FPTVOX_BOUNDED_PATCH_VERSION.to_le_bytes())?;
        for value in grid.resolution {
            writer.write_all(&value.to_le_bytes())?;
        }
        writer.write_all(&(grid.coordinate_system as u32).to_le_bytes())?;
        for value in grid.bounds.min {
            writer.write_all(&value.to_bits().to_le_bytes())?;
        }
        for value in grid.bounds.max {
            writer.write_all(&value.to_bits().to_le_bytes())?;
        }
        writer.write_all(&voxel_count.to_le_bytes())?;
        for (voxel, patches) in grid.voxels.iter().zip(packed_patches) {
            for value in voxel.coordinate {
                writer.write_all(&value.to_le_bytes())?;
            }
            writer.write_all(&voxel.cell.packed_color.to_le_bytes())?;
            writer.write_all(&voxel.cell.packed_properties.to_le_bytes())?;
            writer.write_all(&voxel.cell.emission.to_bits().to_le_bytes())?;
            for word in patches {
                writer.write_all(&word.to_le_bytes())?;
            }
        }
        writer.flush()
    })();
    write_result.map_err(|error| {
        FractalError::new(
            FractalErrorCode::Io,
            format!("write {}: {error}", path.display()),
        )
    })?;
    Ok(FptvoxExportSummary { voxel_count, bytes })
}

fn export_fptvox_with_plane_payload(
    grid: &VoxelGrid,
    packed_planes: &[u32],
    path: &Path,
    magic: [u8; 8],
    version: u32,
    record_size: u32,
) -> Result<FptvoxExportSummary, FractalError> {
    validate_grid(grid)?;
    if packed_planes.len() != grid.voxels.len() {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "surface plane count must match the sparse voxel count",
        ));
    }
    let voxel_count = u64::try_from(grid.voxels.len()).map_err(|_| {
        FractalError::new(
            FractalErrorCode::Artifact,
            "voxel record count exceeds the format's u64 limit",
        )
    })?;
    let bytes = u64::from(FPTVOX_HEADER_SIZE)
        .checked_add(
            voxel_count
                .checked_mul(u64::from(record_size))
                .ok_or_else(|| {
                    FractalError::new(FractalErrorCode::Artifact, "FPTVOX size overflow")
                })?,
        )
        .ok_or_else(|| FractalError::new(FractalErrorCode::Artifact, "FPTVOX size overflow"))?;

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
    let file = File::create(path).map_err(|error| {
        FractalError::new(
            FractalErrorCode::Io,
            format!("create {}: {error}", path.display()),
        )
    })?;
    let mut writer = BufWriter::new(file);
    let write_result = (|| -> std::io::Result<()> {
        writer.write_all(&magic)?;
        writer.write_all(&FPTVOX_HEADER_SIZE.to_le_bytes())?;
        writer.write_all(&version.to_le_bytes())?;
        for value in grid.resolution {
            writer.write_all(&value.to_le_bytes())?;
        }
        writer.write_all(&(grid.coordinate_system as u32).to_le_bytes())?;
        for value in grid.bounds.min {
            writer.write_all(&value.to_bits().to_le_bytes())?;
        }
        for value in grid.bounds.max {
            writer.write_all(&value.to_bits().to_le_bytes())?;
        }
        writer.write_all(&voxel_count.to_le_bytes())?;
        for (voxel, packed_plane) in grid.voxels.iter().zip(packed_planes) {
            for value in voxel.coordinate {
                writer.write_all(&value.to_le_bytes())?;
            }
            writer.write_all(&voxel.cell.packed_color.to_le_bytes())?;
            writer.write_all(&voxel.cell.packed_properties.to_le_bytes())?;
            writer.write_all(&voxel.cell.emission.to_bits().to_le_bytes())?;
            writer.write_all(&packed_plane.to_le_bytes())?;
        }
        writer.flush()
    })();
    write_result.map_err(|error| {
        FractalError::new(
            FractalErrorCode::Io,
            format!("write {}: {error}", path.display()),
        )
    })?;
    Ok(FptvoxExportSummary { voxel_count, bytes })
}
