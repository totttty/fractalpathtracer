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
