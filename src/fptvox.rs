//! Lossless, deterministic serialization of [`crate::VoxelGrid`].
//!
//! Version 1 is a 64-byte header followed by 24-byte sparse voxel records.
//! Every numeric field is little-endian and records retain the exact Metal
//! voxel material payload.

use crate::voxel::{FractalError, FractalErrorCode, VoxelGrid, validate_grid};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
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
/// Eight-byte signature for stackless per-cell triangle BVHs.
pub const FPTVOX_TRIANGLE_BVH_MAGIC: [u8; 8] = *b"FPTVOX10";
/// Triangle-BVH binary format version stored at offset 12.
pub const FPTVOX_TRIANGLE_BVH_VERSION: u32 = 10;
/// Version-10 header bytes.
pub const FPTVOX_TRIANGLE_BVH_HEADER_SIZE: u32 = 112;
/// Bytes per occupied version-10 cell record.
pub const FPTVOX_TRIANGLE_BVH_CELL_RECORD_SIZE: u32 = 32;
/// Bytes per reordered V7 triangle plus its original per-cell order.
pub const FPTVOX_TRIANGLE_BVH_TRIANGLE_RECORD_SIZE: u32 = 16;
/// Bytes per stackless BVH node.
pub const FPTVOX_TRIANGLE_BVH_NODE_RECORD_SIZE: u32 = 16;
/// Eight-byte signature for global indexed triangles with per-cell BVHs.
pub const FPTVOX_INDEXED_TRIANGLE_BVH_MAGIC: [u8; 8] = *b"FPTVOX11";
/// Indexed triangle-BVH binary format version stored at offset 12.
pub const FPTVOX_INDEXED_TRIANGLE_BVH_VERSION: u32 = 11;
/// Version-11 header bytes.
pub const FPTVOX_INDEXED_TRIANGLE_BVH_HEADER_SIZE: u32 = 128;
/// Bytes per occupied version-11 cell record.
pub const FPTVOX_INDEXED_TRIANGLE_BVH_CELL_RECORD_SIZE: u32 = 32;
/// Bytes per global UNORM16 source triangle.
pub const FPTVOX_INDEXED_TRIANGLE_BVH_TRIANGLE_RECORD_SIZE: u32 = 20;
/// Bytes per reordered cell-to-triangle reference.
pub const FPTVOX_INDEXED_TRIANGLE_BVH_REFERENCE_RECORD_SIZE: u32 = 4;
/// Bytes per stackless per-cell BVH node.
pub const FPTVOX_INDEXED_TRIANGLE_BVH_NODE_RECORD_SIZE: u32 = 16;

/// Optional authored-appearance trailer appended after any FPTVOX geometry payload.
pub const FPTVOX_APPEARANCE_MAGIC: [u8; 8] = *b"FPTAPP1\0";
/// Binary appearance-contract version.
pub const FPTVOX_APPEARANCE_VERSION: u32 = 1;
/// Fixed byte size of the version-1 appearance trailer.
pub const FPTVOX_APPEARANCE_SIZE: u32 = 256;
/// Optional authored camera trailer appended after FPTAPP1.
pub const FPTVOX_CAMERA_MAGIC: [u8; 8] = *b"FPTCAM1\0";
pub const FPTVOX_CAMERA_VERSION: u32 = 1;
pub const FPTVOX_CAMERA_SIZE: u32 = 96;
pub const FPTVOX_ENVIRONMENT_MAGIC: [u8; 8] = *b"FPTENV2\0";
pub const FPTVOX_ENVIRONMENT_VERSION: u32 = 2;
pub const FPTVOX_ENVIRONMENT_HEADER_SIZE: u32 = 512;
pub const FPTVOX_ENVIRONMENT_LUT_WIDTH: usize = 128;
pub const FPTVOX_ENVIRONMENT_LUT_HEIGHT: usize = 64;
pub const FPTVOX_ENVIRONMENT_LUT_VALUES: usize =
    FPTVOX_ENVIRONMENT_LUT_WIDTH * FPTVOX_ENVIRONMENT_LUT_HEIGHT * 3;
pub const FPTVOX_ENVIRONMENT_SIZE: u32 =
    FPTVOX_ENVIRONMENT_HEADER_SIZE + (FPTVOX_ENVIRONMENT_LUT_VALUES as u32) * 2;
pub const FPTVOX_ENVIRONMENT_HDRI: u32 = 1 << 0;
pub const FPTVOX_ENVIRONMENT_BASIC_FOG: u32 = 1 << 1;
pub const FPTVOX_ENVIRONMENT_VOLUMETRIC_FOG: u32 = 1 << 2;
pub const FPTVOX_ENVIRONMENT_ITERATION_FOG: u32 = 1 << 3;
pub const FPTVOX_ENVIRONMENT_CLOUDS: u32 = 1 << 4;
/// The source scene enables Mandelbulber ambient occlusion. Consumers without
/// distance-estimator orbit state may use `values[90]` as an ambient-fill
/// strength for an explicitly documented approximation.
pub const FPTVOX_ENVIRONMENT_AMBIENT_OCCLUSION: u32 = 1 << 13;
pub const FPTVOX_MATERIAL_MAGIC: [u8; 8] = *b"FPTMAT1\0";
pub const FPTVOX_MATERIAL_VERSION: u32 = 1;
pub const FPTVOX_MATERIAL_HEADER_SIZE: u32 = 32;
pub const FPTVOX_MATERIAL_RECORD_SIZE: u32 = 68;
/// Optional packed RGB8 stream associated with indexed source triangles.
pub const FPTVOX_TRIANGLE_COLOR_MAGIC: [u8; 8] = *b"FPTCOL1\0";
pub const FPTVOX_TRIANGLE_COLOR_VERSION: u32 = 1;
pub const FPTVOX_TRIANGLE_COLOR_HEADER_SIZE: u32 = 32;
pub const FPTVOX_TRIANGLE_COLOR_RECORD_SIZE: u32 = 4;
/// Three packed RGB8 vertices per indexed source triangle.
pub const FPTVOX_TRIANGLE_VERTEX_COLOR_MAGIC: [u8; 8] = *b"FPTCOL2\0";
pub const FPTVOX_TRIANGLE_VERTEX_COLOR_VERSION: u32 = 2;
pub const FPTVOX_TRIANGLE_VERTEX_COLOR_HEADER_SIZE: u32 = 32;
pub const FPTVOX_TRIANGLE_VERTEX_COLOR_RECORD_SIZE: u32 = 12;
/// One authored Mandelbulber material ID per indexed source triangle.
pub const FPTVOX_TRIANGLE_MATERIAL_MAGIC: [u8; 8] = *b"FPTMID1\0";
pub const FPTVOX_TRIANGLE_MATERIAL_VERSION: u32 = 1;
pub const FPTVOX_TRIANGLE_MATERIAL_HEADER_SIZE: u32 = 32;
pub const FPTVOX_TRIANGLE_MATERIAL_RECORD_SIZE: u32 = 4;
/// Optional packed 10-bit UNORM shading-normal override per indexed triangle.
pub const FPTVOX_TRIANGLE_NORMAL_MAGIC: [u8; 8] = *b"FPTNRM1\0";
pub const FPTVOX_TRIANGLE_NORMAL_VERSION: u32 = 1;
pub const FPTVOX_TRIANGLE_NORMAL_HEADER_SIZE: u32 = 32;
pub const FPTVOX_TRIANGLE_NORMAL_RECORD_SIZE: u32 = 4;

pub const FPTVOX_APPEARANCE_THREE_COLOR_BACKGROUND: u32 = 1 << 0;
pub const FPTVOX_APPEARANCE_MAIN_LIGHT_ENABLED: u32 = 1 << 1;
pub const FPTVOX_APPEARANCE_MAIN_LIGHT_SHADOWS: u32 = 1 << 2;
pub const FPTVOX_APPEARANCE_AUX_LIGHT_ENABLED: u32 = 1 << 3;
pub const FPTVOX_APPEARANCE_AUX_LIGHT_SHADOWS: u32 = 1 << 4;
pub const FPTVOX_APPEARANCE_SPECULAR_PLASTIC: u32 = 1 << 5;

/// Authored Mandelbulber appearance data. Geometry and appearance are kept
/// independently versioned so existing FPTVOX geometry layouts remain stable.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct FptvoxAppearance {
    pub flags: u32,
    /// Lower, middle, and upper background colors.
    pub background_colors: [[f32; 3]; 3],
    pub background_brightness: f32,
    pub background_gamma: f32,
    pub main_light_direction: [f32; 3],
    pub main_light_intensity: f32,
    pub main_light_color: [f32; 3],
    pub main_light_soft_shadow_radians: f32,
    /// Position in exported FPTVOX Y-up coordinates.
    pub auxiliary_light_position: [f32; 3],
    pub auxiliary_light_intensity: f32,
    pub auxiliary_light_color: [f32; 3],
    pub image_gamma: f32,
    pub image_brightness: f32,
    pub image_contrast: f32,
    pub image_saturation: f32,
    pub material_shading: f32,
    pub material_specular: f32,
    pub material_specular_width: f32,
    pub material_roughness: f32,
    pub material_reflectance: f32,
    /// Strength applied to the one-color environment on secondary path misses.
    pub secondary_environment_strength: f32,
    /// Accepted V11 triangle prefix belonging to the primary capture. Zero
    /// means that every triangle is valid for camera rays.
    pub primary_surface_triangle_count: u32,
}

impl FptvoxAppearance {
    fn values(self) -> [f32; 41] {
        let mut values = [0.0; 41];
        values[0..3].copy_from_slice(&self.background_colors[0]);
        values[3..6].copy_from_slice(&self.background_colors[1]);
        values[6..9].copy_from_slice(&self.background_colors[2]);
        values[9] = self.background_brightness;
        values[10] = self.background_gamma;
        values[11..14].copy_from_slice(&self.main_light_direction);
        values[14] = self.main_light_intensity;
        values[15..18].copy_from_slice(&self.main_light_color);
        values[18] = self.main_light_soft_shadow_radians;
        values[19..22].copy_from_slice(&self.auxiliary_light_position);
        values[22] = self.auxiliary_light_intensity;
        values[23..26].copy_from_slice(&self.auxiliary_light_color);
        values[26] = self.image_gamma;
        values[27] = self.image_brightness;
        values[28] = self.image_contrast;
        values[29] = self.image_saturation;
        values[30] = self.material_shading;
        values[31] = self.material_specular;
        values[32] = self.material_specular_width;
        values[33] = self.material_roughness;
        values[34] = self.material_reflectance;
        values[35] = self.secondary_environment_strength;
        values[36] = self.primary_surface_triangle_count as f32;
        values
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct FptvoxCamera {
    /// Camera position in exported, normalized Y-up scene coordinates.
    pub position: [f32; 3],
    pub yaw_pitch: [f32; 2],
    pub roll: f32,
    pub fov_degrees: f32,
    pub image_y_sign: f32,
    pub projection: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FptvoxEnvironment {
    pub flags: u32,
    pub hdri_map_type: u32,
    pub values: [f32; 96],
    pub hdri_lut: Vec<u16>,
}

impl Default for FptvoxEnvironment {
    fn default() -> Self {
        Self {
            flags: 0,
            hdri_map_type: 0,
            values: [0.0; 96],
            hdri_lut: vec![0; FPTVOX_ENVIRONMENT_LUT_VALUES],
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct FptvoxAuthoredMaterial {
    pub id: u32,
    pub flags: u32,
    pub base_color: [f32; 3],
    pub roughness: f32,
    pub specular: f32,
    pub specular_width: f32,
    pub metallic: f32,
    pub reflectance: f32,
    pub transmission: f32,
    pub interior_opacity: f32,
    pub ior: f32,
    pub emission: f32,
    pub transmission_color: [f32; 3],
}

/// Append a versioned authored-appearance trailer to a completed FPTVOX file.
pub fn append_fptvox_appearance(
    path: impl AsRef<Path>,
    appearance: &FptvoxAppearance,
) -> Result<u64, FractalError> {
    let values = appearance.values();
    if values.iter().any(|value| !value.is_finite()) {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX appearance contains a non-finite value",
        ));
    }
    let path = path.as_ref();
    let file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| {
            FractalError::new(
                FractalErrorCode::Io,
                format!("open {} for appearance append: {error}", path.display()),
            )
        })?;
    let mut writer = BufWriter::new(file);
    writer
        .write_all(&FPTVOX_APPEARANCE_MAGIC)
        .map_err(|error| {
            FractalError::new(
                FractalErrorCode::Io,
                format!("write {}: {error}", path.display()),
            )
        })?;
    writer
        .write_all(&FPTVOX_APPEARANCE_SIZE.to_le_bytes())
        .and_then(|_| writer.write_all(&FPTVOX_APPEARANCE_VERSION.to_le_bytes()))
        .and_then(|_| writer.write_all(&appearance.flags.to_le_bytes()))
        .and_then(|_| writer.write_all(&0_u32.to_le_bytes()))
        .and_then(|_| {
            for value in values {
                writer.write_all(&value.to_bits().to_le_bytes())?;
            }
            writer.write_all(&[0_u8; 68])
        })
        .and_then(|_| writer.flush())
        .map_err(|error| {
            FractalError::new(
                FractalErrorCode::Io,
                format!("write {}: {error}", path.display()),
            )
        })?;
    Ok(u64::from(FPTVOX_APPEARANCE_SIZE))
}

pub fn append_fptvox_camera(
    path: impl AsRef<Path>,
    camera: &FptvoxCamera,
) -> Result<u64, FractalError> {
    let values = [
        camera.position[0],
        camera.position[1],
        camera.position[2],
        camera.yaw_pitch[0],
        camera.yaw_pitch[1],
        camera.roll,
        camera.fov_degrees,
        camera.image_y_sign,
    ];
    if values.iter().any(|value| !value.is_finite()) || camera.fov_degrees <= 0.0 {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX camera contains an invalid value",
        ));
    }
    let path = path.as_ref();
    let file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| {
            FractalError::new(
                FractalErrorCode::Io,
                format!("open {} for camera append: {error}", path.display()),
            )
        })?;
    let mut writer = BufWriter::new(file);
    writer
        .write_all(&FPTVOX_CAMERA_MAGIC)
        .and_then(|_| writer.write_all(&FPTVOX_CAMERA_SIZE.to_le_bytes()))
        .and_then(|_| writer.write_all(&FPTVOX_CAMERA_VERSION.to_le_bytes()))
        .and_then(|_| writer.write_all(&camera.projection.to_le_bytes()))
        .and_then(|_| writer.write_all(&0_u32.to_le_bytes()))
        .and_then(|_| {
            for value in values {
                writer.write_all(&value.to_bits().to_le_bytes())?;
            }
            writer.write_all(&[0_u8; 40])
        })
        .and_then(|_| writer.flush())
        .map_err(|error| {
            FractalError::new(
                FractalErrorCode::Io,
                format!("write {}: {error}", path.display()),
            )
        })?;
    Ok(u64::from(FPTVOX_CAMERA_SIZE))
}

pub fn append_fptvox_environment(
    path: impl AsRef<Path>,
    environment: &FptvoxEnvironment,
) -> Result<u64, FractalError> {
    if environment.hdri_map_type > 2
        || environment.values.iter().any(|value| !value.is_finite())
        || environment.hdri_lut.len() != FPTVOX_ENVIRONMENT_LUT_VALUES
    {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX environment contains invalid values",
        ));
    }
    let path = path.as_ref();
    let file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| {
            FractalError::new(
                FractalErrorCode::Io,
                format!("open {} for environment append: {error}", path.display()),
            )
        })?;
    let mut writer = BufWriter::new(file);
    let result = (|| -> std::io::Result<()> {
        writer.write_all(&FPTVOX_ENVIRONMENT_MAGIC)?;
        writer.write_all(&FPTVOX_ENVIRONMENT_SIZE.to_le_bytes())?;
        writer.write_all(&FPTVOX_ENVIRONMENT_VERSION.to_le_bytes())?;
        writer.write_all(&environment.flags.to_le_bytes())?;
        writer.write_all(&environment.hdri_map_type.to_le_bytes())?;
        writer.write_all(&(FPTVOX_ENVIRONMENT_LUT_WIDTH as u32).to_le_bytes())?;
        writer.write_all(&(FPTVOX_ENVIRONMENT_LUT_HEIGHT as u32).to_le_bytes())?;
        for value in environment.values {
            writer.write_all(&value.to_bits().to_le_bytes())?;
        }
        writer.write_all(&[0_u8; 96])?;
        for value in &environment.hdri_lut {
            writer.write_all(&value.to_le_bytes())?;
        }
        writer.flush()
    })();
    result.map_err(|error| {
        FractalError::new(
            FractalErrorCode::Io,
            format!("write {}: {error}", path.display()),
        )
    })?;
    Ok(u64::from(FPTVOX_ENVIRONMENT_SIZE))
}

pub fn append_fptvox_materials(
    path: impl AsRef<Path>,
    materials: &[FptvoxAuthoredMaterial],
) -> Result<u64, FractalError> {
    if materials.is_empty()
        || materials.iter().any(|material| {
            material.id == 0
                || [
                    material.base_color[0],
                    material.base_color[1],
                    material.base_color[2],
                    material.roughness,
                    material.specular,
                    material.specular_width,
                    material.metallic,
                    material.reflectance,
                    material.transmission,
                    material.interior_opacity,
                    material.ior,
                    material.emission,
                    material.transmission_color[0],
                    material.transmission_color[1],
                    material.transmission_color[2],
                ]
                .iter()
                .any(|value| !value.is_finite())
        })
    {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX material table contains invalid values",
        ));
    }
    let count = u64::try_from(materials.len())
        .map_err(|_| FractalError::new(FractalErrorCode::Artifact, "material count overflow"))?;
    let bytes =
        u64::from(FPTVOX_MATERIAL_HEADER_SIZE) + count * u64::from(FPTVOX_MATERIAL_RECORD_SIZE);
    let path = path.as_ref();
    let file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| {
            FractalError::new(
                FractalErrorCode::Io,
                format!("open {} for material append: {error}", path.display()),
            )
        })?;
    let mut writer = BufWriter::new(file);
    let result = (|| -> std::io::Result<()> {
        writer.write_all(&FPTVOX_MATERIAL_MAGIC)?;
        writer.write_all(&FPTVOX_MATERIAL_HEADER_SIZE.to_le_bytes())?;
        writer.write_all(&FPTVOX_MATERIAL_VERSION.to_le_bytes())?;
        writer.write_all(&FPTVOX_MATERIAL_RECORD_SIZE.to_le_bytes())?;
        writer.write_all(&0_u32.to_le_bytes())?;
        writer.write_all(&count.to_le_bytes())?;
        for material in materials {
            writer.write_all(&material.id.to_le_bytes())?;
            writer.write_all(&material.flags.to_le_bytes())?;
            for value in [
                material.base_color[0],
                material.base_color[1],
                material.base_color[2],
                material.roughness,
                material.specular,
                material.specular_width,
                material.metallic,
                material.reflectance,
                material.transmission,
                material.interior_opacity,
                material.ior,
                material.emission,
                material.transmission_color[0],
                material.transmission_color[1],
                material.transmission_color[2],
            ] {
                writer.write_all(&value.to_bits().to_le_bytes())?;
            }
        }
        writer.flush()
    })();
    result.map_err(|error| {
        FractalError::new(
            FractalErrorCode::Io,
            format!("write {}: {error}", path.display()),
        )
    })?;
    Ok(bytes)
}

/// Append one packed RGB8 color for each indexed source triangle.
pub fn append_fptvox_triangle_colors(
    path: impl AsRef<Path>,
    colors: &[u32],
) -> Result<u64, FractalError> {
    if colors.is_empty() || colors.iter().any(|color| color >> 24u32 != 0u32) {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX triangle colors require non-empty RGB8 records",
        ));
    }
    let count = u64::try_from(colors.len()).map_err(|_| {
        FractalError::new(FractalErrorCode::Artifact, "triangle color count overflow")
    })?;
    let bytes = u64::from(FPTVOX_TRIANGLE_COLOR_HEADER_SIZE)
        .checked_add(count * u64::from(FPTVOX_TRIANGLE_COLOR_RECORD_SIZE))
        .ok_or_else(|| {
            FractalError::new(FractalErrorCode::Artifact, "triangle color size overflow")
        })?;
    let path = path.as_ref();
    let file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| {
            FractalError::new(
                FractalErrorCode::Io,
                format!("open {} for triangle-color append: {error}", path.display()),
            )
        })?;
    let mut writer = BufWriter::new(file);
    let write_result = (|| -> std::io::Result<()> {
        writer.write_all(&FPTVOX_TRIANGLE_COLOR_MAGIC)?;
        writer.write_all(&FPTVOX_TRIANGLE_COLOR_HEADER_SIZE.to_le_bytes())?;
        writer.write_all(&FPTVOX_TRIANGLE_COLOR_VERSION.to_le_bytes())?;
        writer.write_all(&FPTVOX_TRIANGLE_COLOR_RECORD_SIZE.to_le_bytes())?;
        writer.write_all(&0_u32.to_le_bytes())?;
        writer.write_all(&count.to_le_bytes())?;
        for color in colors {
            writer.write_all(&color.to_le_bytes())?;
        }
        writer.flush()
    })();
    write_result.map_err(|error| {
        FractalError::new(
            FractalErrorCode::Io,
            format!("write {}: {error}", path.display()),
        )
    })?;
    Ok(bytes)
}

pub fn append_fptvox_triangle_vertex_colors(
    path: impl AsRef<Path>,
    colors: &[[u32; 3]],
) -> Result<u64, FractalError> {
    if colors.is_empty() || colors.iter().flatten().any(|color| color >> 24u32 != 0u32) {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX triangle vertex colors require non-empty RGB8 records",
        ));
    }
    let count = u64::try_from(colors.len()).map_err(|_| {
        FractalError::new(
            FractalErrorCode::Artifact,
            "triangle vertex color count overflow",
        )
    })?;
    let bytes = u64::from(FPTVOX_TRIANGLE_VERTEX_COLOR_HEADER_SIZE)
        .checked_add(count * u64::from(FPTVOX_TRIANGLE_VERTEX_COLOR_RECORD_SIZE))
        .ok_or_else(|| {
            FractalError::new(
                FractalErrorCode::Artifact,
                "triangle vertex color size overflow",
            )
        })?;
    let path = path.as_ref();
    let file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| {
            FractalError::new(
                FractalErrorCode::Io,
                format!(
                    "open {} for triangle-vertex-color append: {error}",
                    path.display()
                ),
            )
        })?;
    let mut writer = BufWriter::new(file);
    writer
        .write_all(&FPTVOX_TRIANGLE_VERTEX_COLOR_MAGIC)
        .and_then(|_| writer.write_all(&FPTVOX_TRIANGLE_VERTEX_COLOR_HEADER_SIZE.to_le_bytes()))
        .and_then(|_| writer.write_all(&FPTVOX_TRIANGLE_VERTEX_COLOR_VERSION.to_le_bytes()))
        .and_then(|_| writer.write_all(&FPTVOX_TRIANGLE_VERTEX_COLOR_RECORD_SIZE.to_le_bytes()))
        .and_then(|_| writer.write_all(&0_u32.to_le_bytes()))
        .and_then(|_| writer.write_all(&count.to_le_bytes()))
        .and_then(|_| {
            for triangle in colors {
                for color in triangle {
                    writer.write_all(&color.to_le_bytes())?;
                }
            }
            writer.flush()
        })
        .map_err(|error| {
            FractalError::new(
                FractalErrorCode::Io,
                format!("write {}: {error}", path.display()),
            )
        })?;
    Ok(bytes)
}

pub fn append_fptvox_triangle_material_ids(
    path: impl AsRef<Path>,
    material_ids: &[u32],
) -> Result<u64, FractalError> {
    if material_ids.is_empty() || material_ids.contains(&0u32) {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX triangle material IDs must be non-empty and nonzero",
        ));
    }
    let count = u64::try_from(material_ids.len()).map_err(|_| {
        FractalError::new(
            FractalErrorCode::Artifact,
            "triangle material ID count overflow",
        )
    })?;
    let bytes = u64::from(FPTVOX_TRIANGLE_MATERIAL_HEADER_SIZE)
        .checked_add(count * u64::from(FPTVOX_TRIANGLE_MATERIAL_RECORD_SIZE))
        .ok_or_else(|| {
            FractalError::new(
                FractalErrorCode::Artifact,
                "triangle material ID size overflow",
            )
        })?;
    let path = path.as_ref();
    let file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| {
            FractalError::new(
                FractalErrorCode::Io,
                format!(
                    "open {} for triangle-material append: {error}",
                    path.display()
                ),
            )
        })?;
    let mut writer = BufWriter::new(file);
    let write_result = (|| -> std::io::Result<()> {
        writer.write_all(&FPTVOX_TRIANGLE_MATERIAL_MAGIC)?;
        writer.write_all(&FPTVOX_TRIANGLE_MATERIAL_HEADER_SIZE.to_le_bytes())?;
        writer.write_all(&FPTVOX_TRIANGLE_MATERIAL_VERSION.to_le_bytes())?;
        writer.write_all(&FPTVOX_TRIANGLE_MATERIAL_RECORD_SIZE.to_le_bytes())?;
        writer.write_all(&0_u32.to_le_bytes())?;
        writer.write_all(&count.to_le_bytes())?;
        for material_id in material_ids {
            writer.write_all(&material_id.to_le_bytes())?;
        }
        writer.flush()
    })();
    write_result.map_err(|error| {
        FractalError::new(
            FractalErrorCode::Io,
            format!("write {}: {error}", path.display()),
        )
    })?;
    Ok(bytes)
}

pub fn append_fptvox_triangle_normals(
    path: impl AsRef<Path>,
    packed_normals: &[u32],
) -> Result<u64, FractalError> {
    if packed_normals.is_empty()
        || packed_normals.iter().any(|normal| {
            normal >> 31 != 0u32 || (normal & (1u32 << 30) == 0u32 && *normal != 0u32)
        })
    {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX triangle normals require canonical optional 31-bit records",
        ));
    }
    let count = u64::try_from(packed_normals.len()).map_err(|_| {
        FractalError::new(FractalErrorCode::Artifact, "triangle normal count overflow")
    })?;
    let bytes = u64::from(FPTVOX_TRIANGLE_NORMAL_HEADER_SIZE)
        .checked_add(count * u64::from(FPTVOX_TRIANGLE_NORMAL_RECORD_SIZE))
        .ok_or_else(|| {
            FractalError::new(FractalErrorCode::Artifact, "triangle normal size overflow")
        })?;
    let path = path.as_ref();
    let file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| {
            FractalError::new(
                FractalErrorCode::Io,
                format!(
                    "open {} for triangle-normal append: {error}",
                    path.display()
                ),
            )
        })?;
    let mut writer = BufWriter::new(file);
    let write_result = (|| -> std::io::Result<()> {
        writer.write_all(&FPTVOX_TRIANGLE_NORMAL_MAGIC)?;
        writer.write_all(&FPTVOX_TRIANGLE_NORMAL_HEADER_SIZE.to_le_bytes())?;
        writer.write_all(&FPTVOX_TRIANGLE_NORMAL_VERSION.to_le_bytes())?;
        writer.write_all(&FPTVOX_TRIANGLE_NORMAL_RECORD_SIZE.to_le_bytes())?;
        writer.write_all(&0_u32.to_le_bytes())?;
        writer.write_all(&count.to_le_bytes())?;
        for normal in packed_normals {
            writer.write_all(&normal.to_le_bytes())?;
        }
        writer.flush()
    })();
    write_result.map_err(|error| {
        FractalError::new(
            FractalErrorCode::Io,
            format!("write {}: {error}", path.display()),
        )
    })?;
    Ok(bytes)
}

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
    /// Packed RGB8 colors parallel to `triangles`.
    pub triangle_colors: Vec<u32>,
    /// Packed RGB8 colors for each source triangle vertex.
    pub triangle_vertex_colors: Vec<[u32; 3]>,
    /// Authored Mandelbulber material IDs parallel to `triangles`.
    pub triangle_material_ids: Vec<u32>,
    /// Zero selects geometric normal; nonzero stores a packed override.
    pub triangle_shading_normals: Vec<u32>,
    pub references: Vec<u32>,
}

/// One reordered exact V7 triangle and its original per-cell order.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FptvoxBvhTriangle {
    pub triangle: FptvoxTriangle,
    pub original_order: u32,
}

/// One compact preorder BVH node. Bounds are conservative cell-local UNORM8.
/// `escape` is the first node after this subtree. A nonzero `triangle_count`
/// marks a leaf and `first_triangle` addresses the reordered triangle stream.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FptvoxBvhNode {
    pub bounds: [u8; 6],
    pub triangle_count: u16,
    pub first_triangle: u32,
    pub escape: u32,
}

/// One occupied V10 cell and its contiguous preorder node range.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FptvoxBvhCell {
    pub coordinate: [u32; 3],
    pub cell: crate::voxel::VoxelCell,
    pub first_node: u32,
    pub node_count: u32,
}

/// Exact V7 triangles accelerated by independent stackless per-cell BVHs.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FptvoxTriangleBvhSurface {
    pub resolution: [u32; 3],
    pub sampling_resolution: [u32; 3],
    pub bounds: crate::voxel::Aabb,
    pub coordinate_system: crate::voxel::CoordinateSystem,
    pub cells: Vec<FptvoxBvhCell>,
    pub triangles: Vec<FptvoxBvhTriangle>,
    pub nodes: Vec<FptvoxBvhNode>,
}

/// Global source triangles accelerated by independent stackless per-cell BVHs.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FptvoxIndexedTriangleBvhSurface {
    pub resolution: [u32; 3],
    pub sampling_resolution: [u32; 3],
    pub bounds: crate::voxel::Aabb,
    pub coordinate_system: crate::voxel::CoordinateSystem,
    pub cells: Vec<FptvoxBvhCell>,
    pub triangles: Vec<FptvoxIndexedTriangle>,
    pub triangle_colors: Vec<u32>,
    pub triangle_vertex_colors: Vec<[u32; 3]>,
    pub triangle_material_ids: Vec<u32>,
    pub triangle_shading_normals: Vec<u32>,
    pub references: Vec<u32>,
    pub nodes: Vec<FptvoxBvhNode>,
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

/// Write a version-10 stackless per-cell triangle BVH artifact.
pub fn export_fptvox_triangle_bvh_surface(
    surface: &FptvoxTriangleBvhSurface,
    path: impl AsRef<Path>,
) -> Result<FptvoxExportSummary, FractalError> {
    if surface.resolution.iter().any(|value| *value == 0)
        || surface.sampling_resolution.iter().any(|value| *value < 2)
    {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX10 resolutions are invalid",
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
            "FPTVOX10 bounds are invalid",
        ));
    }
    if surface.cells.is_empty() || surface.triangles.is_empty() || surface.nodes.is_empty() {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX10 requires non-empty cells, triangles, and nodes",
        ));
    }
    let mut previous_linear = None;
    let mut next_node = 0_u64;
    for cell in &surface.cells {
        let coordinate_invalid = cell
            .coordinate
            .iter()
            .zip(surface.resolution)
            .any(|(value, limit)| *value >= limit);
        if coordinate_invalid
            || !cell.cell.is_occupied()
            || !cell.cell.emission.is_finite()
            || cell.node_count == 0
            || u64::from(cell.first_node) != next_node
        {
            return Err(FractalError::new(
                FractalErrorCode::Artifact,
                "FPTVOX10 contains an invalid cell or node range",
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
                "FPTVOX10 cells must be strictly sorted without duplicates",
            ));
        }
        previous_linear = Some(linear);
        next_node = next_node
            .checked_add(u64::from(cell.node_count))
            .ok_or_else(|| {
                FractalError::new(FractalErrorCode::Artifact, "FPTVOX10 node range overflow")
            })?;
    }
    if next_node != surface.nodes.len() as u64 {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX10 cells do not cover the node stream exactly once",
        ));
    }
    for cell in &surface.cells {
        let start = cell.first_node as usize;
        let end = start + cell.node_count as usize;
        if surface.nodes[start].escape as usize != end
            || surface.nodes[start..end]
                .iter()
                .any(|node| node.escape as usize > end)
        {
            return Err(FractalError::new(
                FractalErrorCode::Artifact,
                "FPTVOX10 BVH escape leaves its owning cell",
            ));
        }
    }
    let mut next_triangle = 0_u64;
    for (index, node) in surface.nodes.iter().enumerate() {
        if node.bounds[0] > node.bounds[3]
            || node.bounds[1] > node.bounds[4]
            || node.bounds[2] > node.bounds[5]
            || node.escape as usize <= index
            || node.escape as usize > surface.nodes.len()
        {
            return Err(FractalError::new(
                FractalErrorCode::Artifact,
                "FPTVOX10 contains an invalid BVH node",
            ));
        }
        if node.triangle_count != 0 {
            if u64::from(node.first_triangle) != next_triangle {
                return Err(FractalError::new(
                    FractalErrorCode::Artifact,
                    "FPTVOX10 leaf triangle ranges are not contiguous",
                ));
            }
            next_triangle = next_triangle
                .checked_add(u64::from(node.triangle_count))
                .ok_or_else(|| {
                    FractalError::new(
                        FractalErrorCode::Artifact,
                        "FPTVOX10 triangle range overflow",
                    )
                })?;
        } else if node.first_triangle != 0 {
            return Err(FractalError::new(
                FractalErrorCode::Artifact,
                "FPTVOX10 internal node carries a triangle range",
            ));
        }
    }
    if next_triangle != surface.triangles.len() as u64
        || surface
            .triangles
            .iter()
            .flat_map(|triangle| triangle.triangle.vertices)
            .any(|vertex| vertex >> 30 != 0)
    {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX10 triangle stream is invalid",
        ));
    }

    let cell_count = u64::try_from(surface.cells.len()).map_err(|_| {
        FractalError::new(FractalErrorCode::Artifact, "FPTVOX10 cell count overflow")
    })?;
    let triangle_count = u64::try_from(surface.triangles.len()).map_err(|_| {
        FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX10 triangle count overflow",
        )
    })?;
    let node_count = u64::try_from(surface.nodes.len()).map_err(|_| {
        FractalError::new(FractalErrorCode::Artifact, "FPTVOX10 node count overflow")
    })?;
    if triangle_count > u64::from(u32::MAX) || node_count > u64::from(u32::MAX) {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX10 streams exceed their 32-bit address range",
        ));
    }
    let bytes = u64::from(FPTVOX_TRIANGLE_BVH_HEADER_SIZE)
        .checked_add(cell_count * u64::from(FPTVOX_TRIANGLE_BVH_CELL_RECORD_SIZE))
        .and_then(|value| {
            value.checked_add(triangle_count * u64::from(FPTVOX_TRIANGLE_BVH_TRIANGLE_RECORD_SIZE))
        })
        .and_then(|value| {
            value.checked_add(node_count * u64::from(FPTVOX_TRIANGLE_BVH_NODE_RECORD_SIZE))
        })
        .ok_or_else(|| FractalError::new(FractalErrorCode::Artifact, "FPTVOX10 size overflow"))?;

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
        writer.write_all(&FPTVOX_TRIANGLE_BVH_MAGIC)?;
        writer.write_all(&FPTVOX_TRIANGLE_BVH_HEADER_SIZE.to_le_bytes())?;
        writer.write_all(&FPTVOX_TRIANGLE_BVH_VERSION.to_le_bytes())?;
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
        writer.write_all(&FPTVOX_TRIANGLE_BVH_CELL_RECORD_SIZE.to_le_bytes())?;
        writer.write_all(&triangle_count.to_le_bytes())?;
        writer.write_all(&FPTVOX_TRIANGLE_BVH_TRIANGLE_RECORD_SIZE.to_le_bytes())?;
        writer.write_all(&0_u32.to_le_bytes())?;
        writer.write_all(&node_count.to_le_bytes())?;
        writer.write_all(&FPTVOX_TRIANGLE_BVH_NODE_RECORD_SIZE.to_le_bytes())?;
        writer.write_all(&0_u32.to_le_bytes())?;
        for cell in &surface.cells {
            for value in cell.coordinate {
                writer.write_all(&value.to_le_bytes())?;
            }
            writer.write_all(&cell.cell.packed_color.to_le_bytes())?;
            writer.write_all(&cell.cell.packed_properties.to_le_bytes())?;
            writer.write_all(&cell.cell.emission.to_bits().to_le_bytes())?;
            writer.write_all(&cell.first_node.to_le_bytes())?;
            writer.write_all(&cell.node_count.to_le_bytes())?;
        }
        for triangle in &surface.triangles {
            for vertex in triangle.triangle.vertices {
                writer.write_all(&vertex.to_le_bytes())?;
            }
            writer.write_all(&triangle.original_order.to_le_bytes())?;
        }
        for node in &surface.nodes {
            writer.write_all(&node.bounds)?;
            writer.write_all(&node.triangle_count.to_le_bytes())?;
            writer.write_all(&node.first_triangle.to_le_bytes())?;
            writer.write_all(&node.escape.to_le_bytes())?;
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

/// Write a version-11 global indexed-triangle stream with per-cell stackless BVHs.
pub fn export_fptvox_indexed_triangle_bvh_surface(
    surface: &FptvoxIndexedTriangleBvhSurface,
    path: impl AsRef<Path>,
) -> Result<FptvoxExportSummary, FractalError> {
    if surface.resolution.iter().any(|value| *value == 0)
        || surface.sampling_resolution.iter().any(|value| *value < 2)
    {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX11 resolutions are invalid",
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
            "FPTVOX11 bounds are invalid",
        ));
    }
    if surface.cells.is_empty()
        || surface.triangles.is_empty()
        || surface.references.is_empty()
        || surface.nodes.is_empty()
    {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX11 requires non-empty cells, triangles, references, and nodes",
        ));
    }

    let mut previous_linear = None;
    let mut next_node = 0_u64;
    for cell in &surface.cells {
        let coordinate_invalid = cell
            .coordinate
            .iter()
            .zip(surface.resolution)
            .any(|(value, limit)| *value >= limit);
        if coordinate_invalid
            || !cell.cell.is_occupied()
            || !cell.cell.emission.is_finite()
            || cell.node_count == 0
            || u64::from(cell.first_node) != next_node
        {
            return Err(FractalError::new(
                FractalErrorCode::Artifact,
                "FPTVOX11 contains an invalid cell or node range",
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
                "FPTVOX11 cells must be strictly sorted without duplicates",
            ));
        }
        previous_linear = Some(linear);
        next_node = next_node
            .checked_add(u64::from(cell.node_count))
            .ok_or_else(|| {
                FractalError::new(FractalErrorCode::Artifact, "FPTVOX11 node range overflow")
            })?;
    }
    if next_node != surface.nodes.len() as u64 {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX11 cells do not cover the node stream exactly once",
        ));
    }
    for cell in &surface.cells {
        let start = cell.first_node as usize;
        let end = start + cell.node_count as usize;
        if surface.nodes[start].escape as usize != end
            || surface.nodes[start..end]
                .iter()
                .any(|node| node.escape as usize > end)
        {
            return Err(FractalError::new(
                FractalErrorCode::Artifact,
                "FPTVOX11 BVH escape leaves its owning cell",
            ));
        }
    }
    let mut next_reference = 0_u64;
    for (index, node) in surface.nodes.iter().enumerate() {
        if node.bounds[0] > node.bounds[3]
            || node.bounds[1] > node.bounds[4]
            || node.bounds[2] > node.bounds[5]
            || node.escape as usize <= index
            || node.escape as usize > surface.nodes.len()
        {
            return Err(FractalError::new(
                FractalErrorCode::Artifact,
                "FPTVOX11 contains an invalid BVH node",
            ));
        }
        if node.triangle_count != 0 {
            if u64::from(node.first_triangle) != next_reference {
                return Err(FractalError::new(
                    FractalErrorCode::Artifact,
                    "FPTVOX11 leaf reference ranges are not contiguous",
                ));
            }
            next_reference = next_reference
                .checked_add(u64::from(node.triangle_count))
                .ok_or_else(|| {
                    FractalError::new(
                        FractalErrorCode::Artifact,
                        "FPTVOX11 reference range overflow",
                    )
                })?;
        } else if node.first_triangle != 0 {
            return Err(FractalError::new(
                FractalErrorCode::Artifact,
                "FPTVOX11 internal node carries references",
            ));
        }
    }
    if next_reference != surface.references.len() as u64
        || surface
            .references
            .iter()
            .any(|reference| *reference as usize >= surface.triangles.len())
    {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX11 reference or triangle stream is invalid",
        ));
    }

    let cell_count = u64::try_from(surface.cells.len()).map_err(|_| {
        FractalError::new(FractalErrorCode::Artifact, "FPTVOX11 cell count overflow")
    })?;
    let triangle_count = u64::try_from(surface.triangles.len()).map_err(|_| {
        FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX11 triangle count overflow",
        )
    })?;
    let reference_count = u64::try_from(surface.references.len()).map_err(|_| {
        FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX11 reference count overflow",
        )
    })?;
    let node_count = u64::try_from(surface.nodes.len()).map_err(|_| {
        FractalError::new(FractalErrorCode::Artifact, "FPTVOX11 node count overflow")
    })?;
    if triangle_count > u64::from(u32::MAX)
        || reference_count > u64::from(u32::MAX)
        || node_count > u64::from(u32::MAX)
    {
        return Err(FractalError::new(
            FractalErrorCode::Artifact,
            "FPTVOX11 streams exceed their 32-bit address range",
        ));
    }
    let bytes = u64::from(FPTVOX_INDEXED_TRIANGLE_BVH_HEADER_SIZE)
        .checked_add(cell_count * u64::from(FPTVOX_INDEXED_TRIANGLE_BVH_CELL_RECORD_SIZE))
        .and_then(|value| {
            value.checked_add(
                triangle_count * u64::from(FPTVOX_INDEXED_TRIANGLE_BVH_TRIANGLE_RECORD_SIZE),
            )
        })
        .and_then(|value| {
            value.checked_add(
                reference_count * u64::from(FPTVOX_INDEXED_TRIANGLE_BVH_REFERENCE_RECORD_SIZE),
            )
        })
        .and_then(|value| {
            value.checked_add(node_count * u64::from(FPTVOX_INDEXED_TRIANGLE_BVH_NODE_RECORD_SIZE))
        })
        .ok_or_else(|| FractalError::new(FractalErrorCode::Artifact, "FPTVOX11 size overflow"))?;

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
        writer.write_all(&FPTVOX_INDEXED_TRIANGLE_BVH_MAGIC)?;
        writer.write_all(&FPTVOX_INDEXED_TRIANGLE_BVH_HEADER_SIZE.to_le_bytes())?;
        writer.write_all(&FPTVOX_INDEXED_TRIANGLE_BVH_VERSION.to_le_bytes())?;
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
        writer.write_all(&FPTVOX_INDEXED_TRIANGLE_BVH_CELL_RECORD_SIZE.to_le_bytes())?;
        writer.write_all(&triangle_count.to_le_bytes())?;
        writer.write_all(&FPTVOX_INDEXED_TRIANGLE_BVH_TRIANGLE_RECORD_SIZE.to_le_bytes())?;
        writer.write_all(&0_u32.to_le_bytes())?;
        writer.write_all(&reference_count.to_le_bytes())?;
        writer.write_all(&FPTVOX_INDEXED_TRIANGLE_BVH_REFERENCE_RECORD_SIZE.to_le_bytes())?;
        writer.write_all(&0_u32.to_le_bytes())?;
        writer.write_all(&node_count.to_le_bytes())?;
        writer.write_all(&FPTVOX_INDEXED_TRIANGLE_BVH_NODE_RECORD_SIZE.to_le_bytes())?;
        writer.write_all(&0_u32.to_le_bytes())?;
        for cell in &surface.cells {
            for value in cell.coordinate {
                writer.write_all(&value.to_le_bytes())?;
            }
            writer.write_all(&cell.cell.packed_color.to_le_bytes())?;
            writer.write_all(&cell.cell.packed_properties.to_le_bytes())?;
            writer.write_all(&cell.cell.emission.to_bits().to_le_bytes())?;
            writer.write_all(&cell.first_node.to_le_bytes())?;
            writer.write_all(&cell.node_count.to_le_bytes())?;
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
        for node in &surface.nodes {
            writer.write_all(&node.bounds)?;
            writer.write_all(&node.triangle_count.to_le_bytes())?;
            writer.write_all(&node.first_triangle.to_le_bytes())?;
            writer.write_all(&node.escape.to_le_bytes())?;
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
