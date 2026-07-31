use std::ffi::{c_char, c_int};

pub const SDF_CORNELL_BOX: u32 = 0;
pub const SDF_GLASS_BALL: u32 = 1;
pub const SDF_BALL_FRACTAL: u32 = 2;
pub const SDF_CAGE_FRACTAL: u32 = 3;
pub const SDF_IFS_FRACTAL: u32 = 4;
pub const SDF_MANDELBOX_FRACTAL: u32 = 5;
pub const SDF_MENGER_SPONGE: u32 = 6;
pub const SDF_TOWER_FRACTAL: u32 = 7;
pub const SDF_TREE_FRACTAL: u32 = 8;
pub const SDF_PROGRAM: u32 = 9;
pub const SDF_README_CORNELL: u32 = 10;
pub const SDF_README_GLASS: u32 = 11;

pub const SDF_PROGRAM_MAX_OPS: usize = 64;
pub const SDF_GRADIENT_MAX_STOPS: usize = 16;
pub const HDRI_PATH_CAPACITY: usize = 256;
pub const HDRI_LUT_WIDTH: usize = 32;
pub const HDRI_LUT_HEIGHT: usize = 16;

pub const SDF_OP_ABS: u32 = 1;
pub const SDF_OP_TRANSLATE: u32 = 2;
pub const SDF_OP_SCALE: u32 = 3;
pub const SDF_OP_ROTATE_X: u32 = 4;
pub const SDF_OP_ROTATE_Y: u32 = 5;
pub const SDF_OP_ROTATE_Z: u32 = 6;
pub const SDF_OP_REPEAT: u32 = 7;
pub const SDF_OP_SORT_DESC: u32 = 8;
pub const SDF_OP_SPHERE: u32 = 16;
pub const SDF_OP_BOX: u32 = 17;
pub const SDF_OP_PLANE: u32 = 18;
pub const SDF_OP_ORBIT_ADD: u32 = 32;

pub const SDF_ACCUMULATION_AUTO: u32 = 0;
pub const SDF_ACCUMULATION_PER_SAMPLE: u32 = 1;
pub const SDF_ACCUMULATION_BATCH: u32 = 2;
pub const SDF_ACCUMULATION_CHUNKED: u32 = 3;

pub const RENDERER_SDF: u32 = 0;
pub const RENDERER_VOXEL: u32 = 1;
pub const VOXEL_NORMAL_FACE: u32 = 0;
pub const VOXEL_NORMAL_SMOOTH: u32 = 1;
pub const VOXEL_STORAGE_DENSE: u32 = 0;
pub const VOXEL_STORAGE_SPARSE_BRICKS: u32 = 1;

pub const DIAGNOSTIC_DEPTH: u32 = 0;
pub const DIAGNOSTIC_NORMAL: u32 = 1;
pub const DIAGNOSTIC_MATERIAL: u32 = 2;
pub const DIAGNOSTIC_PATH_DIRECT: u32 = 4;
pub const DIAGNOSTIC_PATH_ENVIRONMENT: u32 = 5;
pub const DIAGNOSTIC_PATH_THROUGHPUT: u32 = 6;
pub const DIAGNOSTIC_PATH_FINAL: u32 = 7;
pub const DIAGNOSTIC_SDF_PRIMARY_STEPS: u32 = 11;
pub const DIAGNOSTIC_SDF_SHADOW_STEPS: u32 = 12;
pub const DIAGNOSTIC_SDF_NORMAL_EVALS: u32 = 13;
pub const DIAGNOSTIC_SDF_BOUNCES: u32 = 14;
pub const DIAGNOSTIC_SDF_BOUNCE_CONTRIBUTION: u32 = 15;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FptSdfInstruction {
    pub opcode: u32,
    pub flags: u32,
    pub material_index: u32,
    pub _pad0: u32,
    pub data: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FptRenderConfig {
    pub width: u32,
    pub height: u32,
    pub samples: u32,
    pub preview: u32,
    pub sdf_id: u32,
    pub glass_mode: u32,
    pub sdf_accumulation_mode: u32,
    pub sdf_profile: u32,
    pub sdf_bounce_cap: u32,
    pub sdf_russian_roulette: u32,
    pub sdf_normal_mode: u32,
    pub sdf_chunk_samples: u32,
    pub sdf_rr_start: f32,
    pub sdf_rr_min_prob: f32,
    pub camera_position: [f32; 3],
    pub camera_yaw_pitch: [f32; 2],
    pub camera_fov: f32,
    pub camera_dof: f32,
    pub focus_distance: f32,
    pub render: [f32; 8],
    pub world: [f32; 7],
    pub world_one_color: [f32; 3],
    pub sun: [f32; 5],
    pub sun_color: [f32; 3],
    pub background_gradient: [f32; 6],
    pub post: [f32; 7],
    pub set_values: [f32; 40],
    pub vset_values: [f32; 120],
    pub sdf_program_count: u32,
    pub gradient_count: u32,
    pub material_mode: u32,
    pub hdri_enabled: u32,
    pub hdri_width: u32,
    pub hdri_height: u32,
    pub _pad_hdri: [u32; 2],
    pub fractal_style_mode: u32,
    pub _pad_style: [u32; 3],
    pub fractal_style: [f32; 12],
    pub program_material: [f32; 8],
    pub sdf_program: [FptSdfInstruction; SDF_PROGRAM_MAX_OPS],
    pub gradient_stops: [[f32; 4]; SDF_GRADIENT_MAX_STOPS],
    pub hdri_path: [u8; HDRI_PATH_CAPACITY],
    pub hdri_lut: [u16; HDRI_LUT_WIDTH * HDRI_LUT_HEIGHT * 3],
    pub renderer_backend: u32,
    pub voxel_resolution: u32,
    pub voxel_normal_mode: u32,
    pub voxel_storage: u32,
    pub voxel_bounds_min: [f32; 3],
    pub voxel_surface_band: f32,
    pub voxel_bounds_max: [f32; 3],
    pub voxel_fill_interior: u32,
}

impl Default for FptRenderConfig {
    fn default() -> Self {
        // The ABI is a plain-old-data structure shared with C++ and Metal.
        unsafe { std::mem::zeroed() }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FptDiagnosticConfig {
    pub mode: u32,
    pub _pad0: u32,
    pub max_distance: f32,
    pub normal_mix: f32,
}

unsafe extern "C" {
    pub fn fpt_metal_render(
        metallib_path: *const c_char,
        output_path: *const c_char,
        config: *const FptRenderConfig,
        build_ms: *mut f64,
        elapsed_ms: *mut f64,
        voxel_memory_bytes: *mut u64,
        voxel_active_bricks: *mut u32,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
    pub fn fpt_metal_diagnostic_render(
        metallib_path: *const c_char,
        output_path: *const c_char,
        config: *const FptRenderConfig,
        diagnostic: *const FptDiagnosticConfig,
        elapsed_ms: *mut f64,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
    pub fn fpt_compare_images(
        baseline_path: *const c_char,
        candidate_path: *const c_char,
        report_path: *const c_char,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
    pub fn fpt_metal_device_name(name: *mut c_char, name_len: usize) -> c_int;
    pub fn fpt_metal_preview(
        metallib_path: *const c_char,
        config: *const FptRenderConfig,
        scene_configs: *const FptRenderConfig,
        scene_config_count: u32,
        scene_config_index: u32,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_layout_matches_c_bridge() {
        assert_eq!(std::mem::size_of::<FptSdfInstruction>(), 32);
        assert_eq!(std::mem::size_of::<FptRenderConfig>(), 6692);
        assert_eq!(std::mem::size_of::<FptDiagnosticConfig>(), 16);
    }
}
