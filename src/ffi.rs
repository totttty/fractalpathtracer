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
pub const SDF_MANDELBULBER: u32 = 12;

pub const SDF_PROGRAM_MAX_OPS: usize = 64;
pub const SDF_FLAT_UNION_MAX_PRIMITIVES: usize = 64;
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
pub const SDF_OP_UNION: u32 = 24;
pub const SDF_OP_INTERSECTION: u32 = 25;
pub const SDF_OP_SUBTRACT: u32 = 26;
pub const SDF_OP_ORBIT_ADD: u32 = 32;

pub const SDF_ACCUMULATION_AUTO: u32 = 0;
pub const SDF_ACCUMULATION_PER_SAMPLE: u32 = 1;
pub const SDF_ACCUMULATION_BATCH: u32 = 2;
pub const SDF_ACCUMULATION_CHUNKED: u32 = 3;

pub const SDF_STITCH_STATE_FULL: u32 = 0;
pub const SDF_STITCH_STATE_LEAN: u32 = 1;

pub const RENDERER_SDF: u32 = 0;
pub const RENDERER_VOXEL: u32 = 1;
pub const RENDERER_BOUND_GRID: u32 = 2;
pub const RENDERER_REGIONAL: u32 = 3;
pub const VOXEL_NORMAL_FACE: u32 = 0;
pub const VOXEL_NORMAL_SMOOTH: u32 = 1;
pub const VOXEL_NORMAL_EXACT: u32 = 2;
pub const VOXEL_MATERIAL_STORED: u32 = 0;
pub const VOXEL_MATERIAL_EXACT: u32 = 1;
pub const VOXEL_OFFSET_LEGACY: u32 = 0;
pub const VOXEL_OFFSET_PRECISION: u32 = 1;
pub const VOXEL_STORAGE_DENSE: u32 = 0;
pub const VOXEL_STORAGE_SPARSE_BRICKS: u32 = 1;
pub const VOXEL_STORAGE_TEMPLATE_BRICKS: u32 = 3;
pub const VOXEL_COVERAGE_LEGACY: u32 = 0;
pub const VOXEL_COVERAGE_LIPSCHITZ: u32 = 1;
pub const VOXEL_COVERAGE_INTERVAL: u32 = 2;
pub const VOXEL_BUILD_STAGING: u32 = 0;
pub const VOXEL_BUILD_DIRECT: u32 = 1;
pub const VOXEL_LEAF_REFINEMENT_NONE: u32 = 0;
pub const VOXEL_LEAF_REFINEMENT_SECANT_BISECTION: u32 = 1;
pub const VOXEL_LEAF_REFINEMENT_RESTRICTED_TRACE: u32 = 2;
pub const VOXEL_LEAF_REFINEMENT_FIXED_DE: u32 = 3;

pub const DIAGNOSTIC_DEPTH: u32 = 0;
pub const DIAGNOSTIC_NORMAL: u32 = 1;
pub const DIAGNOSTIC_MATERIAL: u32 = 2;
pub const DIAGNOSTIC_HIT_MASK: u32 = 3;
pub const DIAGNOSTIC_PATH_DIRECT: u32 = 4;
pub const DIAGNOSTIC_PATH_ENVIRONMENT: u32 = 5;
pub const DIAGNOSTIC_PATH_THROUGHPUT: u32 = 6;
pub const DIAGNOSTIC_PATH_FINAL: u32 = 7;
pub const DIAGNOSTIC_DIFFUSE_NORMAL: u32 = 8;
pub const DIAGNOSTIC_MANDEL_COLOR_INDEX: u32 = 9;
pub const DIAGNOSTIC_MANDEL_PALETTE_POSITION: u32 = 10;
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
#[derive(Clone, Copy, Default)]
pub struct FptPrimitiveInstance {
    pub transform: [f32; 12],
    pub data: [f32; 4],
    pub opcode: u32,
    pub distance_scale: f32,
    pub source_instruction: u32,
    pub _pad0: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FptAffineTransform {
    pub transform: [f32; 12],
    pub distance_scale: f32,
    pub _pad0: [u32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FptIndexedPrimitive {
    pub data: [f32; 4],
    pub opcode: u32,
    pub transform_index: u32,
    pub source_instruction: u32,
    pub combine_mode: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FptTypedSoAProgram {
    pub sphere_count: u32,
    pub box_count: u32,
    pub plane_count: u32,
    pub _pad0: u32,
    pub sphere_x: [f32; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub sphere_y: [f32; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub sphere_z: [f32; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub sphere_radius: [f32; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub sphere_source: [u32; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub box_x: [f32; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub box_y: [f32; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub box_z: [f32; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub box_half_x: [f32; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub box_half_y: [f32; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub box_half_z: [f32; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub box_source: [u32; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub plane_x: [f32; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub plane_y: [f32; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub plane_z: [f32; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub plane_center_x: [f32; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub plane_center_y: [f32; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub plane_center_z: [f32; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub plane_offset: [f32; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub plane_source: [u32; SDF_FLAT_UNION_MAX_PRIMITIVES],
}

impl Default for FptTypedSoAProgram {
    fn default() -> Self {
        // Plain-old-data shared with C++ and Metal.
        unsafe { std::mem::zeroed() }
    }
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
    pub mandel_iteration_scale: f32,
    pub camera_position: [f32; 3],
    pub camera_yaw_pitch: [f32; 2],
    pub camera_roll: f32,
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
    pub vset_values: [f32; 133],
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
    pub voxel_coverage_mode: u32,
    pub voxel_build_mode: u32,
    pub voxel_brick_rejection: u32,
    pub voxel_leaf_refinement: u32,
    pub voxel_material_mode: u32,
    pub voxel_offset_mode: u32,
    pub bound_grid_resolution: u32,
    pub bound_grid_profile: u32,
    pub bound_grid_profile_stride: u32,
    pub bound_grid_cage_bounds: u32,
    pub bound_grid_directional: u32,
    pub sdf_program_source_count: u32,
    pub sdf_program_optimization: u32,
    pub regional_program_resolution: u32,
    pub bound_grid_fp16: u32,
    pub sdf_flat_union_count: u32,
    pub sdf_flat_union_instances: [FptPrimitiveInstance; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub sdf_shading_program_count: u32,
    pub sdf_geometry_split: u32,
    pub sdf_shading_program: [FptSdfInstruction; SDF_PROGRAM_MAX_OPS],
    pub sdf_topology_specialization: u32,
    pub sdf_runtime_source_bytecode: u32,
    pub sdf_function_stitching: u32,
    pub sdf_stitched_surface: u32,
    pub sdf_stitch_validation: u32,
    pub sdf_stitch_distance_only: u32,
    pub sdf_stitch_split_graph: u32,
    pub sdf_stitch_fusion: u32,
    pub sdf_canonical_count: u32,
    pub sdf_canonical_source_count: u32,
    pub sdf_canonical_primitives: [FptPrimitiveInstance; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub sdf_canonical_transform_count: u32,
    pub _pad_canonical_indexed: [u32; 3],
    pub sdf_canonical_transforms: [FptAffineTransform; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub sdf_indexed_primitives: [FptIndexedPrimitive; SDF_FLAT_UNION_MAX_PRIMITIVES],
    pub sdf_typed_soa: FptTypedSoAProgram,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FptBoundGridStats {
    pub macro_cells: [u64; 3],
    pub certified_skips: [u64; 3],
    pub candidate_intervals: [u64; 3],
    pub candidate_misses: [u64; 3],
    pub candidate_hits: [u64; 3],
    pub unknown_intervals: [u64; 3],
    pub field_evaluations: [u64; 3],
    pub directional_steps: [u64; 3],
    pub cell_exit_clamps: [u64; 3],
    pub unknown_derivative_intervals: [u64; 3],
    pub profiled_paths: u64,
    pub certified_cells: u64,
    pub unknown_cells: u64,
    pub sampled_bound_failures: u64,
    pub sampled_false_skips: u64,
    pub certified_derivative_cells: u64,
    pub unknown_derivative_cells: u64,
    pub sampled_derivative_failures: u64,
    pub regional_cells: u64,
    pub regional_fallback_cells: u64,
    pub regional_unique_programs: u64,
    pub regional_retained_instructions: u64,
    pub regional_sampled_distance_failures: u64,
    pub regional_profiled_paths: u64,
    pub regional_distance_evaluations: [u64; 3],
    pub regional_atlas_evaluations: [u64; 3],
    pub regional_full_program_evaluations: [u64; 3],
    pub regional_cell_entries: [u64; 3],
    pub regional_same_cell_reuses: [u64; 3],
    pub regional_same_program_reuses: [u64; 3],
    pub regional_program_id_loads: [u64; 3],
    pub regional_header_loads: [u64; 3],
    pub regional_dynamic_instructions: [u64; 3],
    pub regional_pruned_cells: u64,
    pub regional_pruned_primitives: u64,
    pub regional_repeat_seam_fallback_cells: u64,
    pub regional_unsupported_fallback_cells: u64,
    pub regional_no_dominance_cells: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FptStitchValidationStats {
    pub sample_count: u64,
    pub distance_failures: u64,
    pub gradient_failures: u64,
    pub max_distance_error: f32,
    pub max_gradient_error: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct FptStitchPipelineStats {
    pub instruction_count: u32,
    pub transform_instruction_count: u32,
    pub primitive_instruction_count: u32,
    pub primitive_type_runs: u32,
    pub union_count: u32,
    pub intersection_count: u32,
    pub subtraction_count: u32,
    pub thread_execution_width: u32,
    pub max_total_threads_per_threadgroup: u32,
    pub graph_node_count: u32,
    pub static_threadgroup_memory_bytes: u64,
    pub runtime_source_bytes: u64,
    pub runtime_library_compile_ms: f64,
    pub runtime_pipeline_link_ms: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct FptSdfProfileStats {
    pub primary_steps: u64,
    pub secondary_steps: u64,
    pub shadow_steps: u64,
    pub normal_evals: u64,
    pub bounces: u64,
    pub pixels: u64,
    pub distance_evals: u64,
    pub march_orbit_iterations: u64,
    pub refinement_steps: u64,
    pub normal_field_evals: u64,
    pub material_evals: u64,
    pub max_ray_steps: u64,
    pub max_pixel_steps: u64,
    pub distance_evals_by_phase: [u64; 4],
    pub orbit_iterations_by_phase: [u64; 4],
    pub formula_slot_iterations: [u64; 9],
    pub refinement_distance_evals: u64,
    pub primary_ms_estimate: f64,
    pub secondary_ms_estimate: f64,
    pub shadow_ms_estimate: f64,
    pub normal_ms_estimate: f64,
    pub bounce_ms_estimate: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct FptAsyncJitStats {
    pub fallback_render_ms: f64,
    pub jit_build_ms: f64,
    pub stitched_render_ms: f64,
    pub cache_status: u32,
    pub fallback_completed_before_jit: u32,
    pub max_absolute_error: f32,
    pub _pad0: u32,
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
    pub dispatch_origin: [u32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct FptMandelbulberFieldSample {
    pub distance: f32,
    pub radius: f32,
    pub derivative: f32,
    pub iterations: f32,
}

unsafe extern "C" {
    pub fn fpt_metal_render(
        metallib_path: *const c_char,
        stitch_metallib_path: *const c_char,
        stitch_archive_path: *const c_char,
        output_path: *const c_char,
        shader_source: *const c_char,
        shader_source_len: usize,
        config: *const FptRenderConfig,
        build_ms: *mut f64,
        elapsed_ms: *mut f64,
        voxel_memory_bytes: *mut u64,
        voxel_active_bricks: *mut u32,
        voxel_active_cells: *mut u64,
        voxel_rejected_bricks: *mut u32,
        bound_grid_stats: *mut FptBoundGridStats,
        stitch_cache_status: *mut u32,
        stitch_validation_stats: *mut FptStitchValidationStats,
        stitch_pipeline_stats: *mut FptStitchPipelineStats,
        sdf_profile_stats: *mut FptSdfProfileStats,
        linear_output: *mut f32,
        linear_output_len: usize,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
    pub fn fpt_metal_diagnostic_render(
        metallib_path: *const c_char,
        output_path: *const c_char,
        shader_source: *const c_char,
        shader_source_len: usize,
        config: *const FptRenderConfig,
        diagnostic: *const FptDiagnosticConfig,
        elapsed_ms: *mut f64,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
    pub fn fpt_mandelbulber_sample_field(
        metallib_path: *const c_char,
        shader_source: *const c_char,
        shader_source_len: usize,
        config: *const FptRenderConfig,
        points_xyzw: *const f32,
        point_count: usize,
        samples: *mut FptMandelbulberFieldSample,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
    pub fn fpt_metal_voxel_build(
        metallib_path: *const c_char,
        config: *const FptRenderConfig,
        cells: *mut u8,
        cells_len: usize,
        build_ms: *mut f64,
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
    pub fn fpt_test_voxel_dda(
        metallib_path: *const c_char,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
    pub fn fpt_test_async_stitch_context(
        metallib_path: *const c_char,
        stitch_metallib_path: *const c_char,
        stitch_archive_path: *const c_char,
        config: *const FptRenderConfig,
        stats: *mut FptAsyncJitStats,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
    pub fn fpt_test_typed_soa(
        metallib_path: *const c_char,
        config: *const FptRenderConfig,
        stats: *mut FptStitchValidationStats,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
    pub fn fpt_metal_preview(
        metallib_path: *const c_char,
        stitch_metallib_path: *const c_char,
        stitch_archive_paths: *const *const c_char,
        shader_source: *const c_char,
        shader_source_len: usize,
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
        assert_eq!(std::mem::size_of::<FptPrimitiveInstance>(), 80);
        assert_eq!(std::mem::size_of::<FptAffineTransform>(), 64);
        assert_eq!(std::mem::size_of::<FptIndexedPrimitive>(), 32);
        assert_eq!(std::mem::size_of::<FptTypedSoAProgram>(), 5136);
        assert_eq!(std::mem::size_of::<FptStitchPipelineStats>(), 72);
        assert_eq!(std::mem::size_of::<FptRenderConfig>(), 30448);
        assert_eq!(std::mem::size_of::<FptSdfProfileStats>(), 288);
        assert_eq!(std::mem::size_of::<FptDiagnosticConfig>(), 24);
        assert_eq!(std::mem::size_of::<FptMandelbulberFieldSample>(), 16);
        assert_eq!(std::mem::size_of::<FptAsyncJitStats>(), 40);
    }

    #[test]
    fn voxel_dda_contract_holds_on_metal() {
        let _metal_test_guard = crate::metal_test_guard();
        let metallib = std::ffi::CString::new(env!("FPT_METALLIB_PATH")).unwrap();
        let mut error = [0_i8; 512];
        let status =
            unsafe { fpt_test_voxel_dda(metallib.as_ptr(), error.as_mut_ptr(), error.len()) };
        if status != 0 {
            let message = unsafe { std::ffi::CStr::from_ptr(error.as_ptr()) }.to_string_lossy();
            panic!("{message}");
        }
    }
}
