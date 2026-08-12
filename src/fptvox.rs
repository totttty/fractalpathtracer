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
