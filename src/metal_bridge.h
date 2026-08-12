#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum FptSdfId {
    FPT_SDF_CORNELL_BOX = 0,
    FPT_SDF_GLASS_BALL = 1,
    FPT_SDF_BALL_FRACTAL = 2,
    FPT_SDF_CAGE_FRACTAL = 3,
    FPT_SDF_IFS_FRACTAL = 4,
    FPT_SDF_MANDELBOX_FRACTAL = 5,
    FPT_SDF_MENGER_SPONGE = 6,
    FPT_SDF_TOWER_FRACTAL = 7,
    FPT_SDF_TREE_FRACTAL = 8,
    FPT_SDF_PROGRAM = 9,
    FPT_SDF_README_CORNELL = 10,
    FPT_SDF_README_GLASS = 11,
    FPT_SDF_MANDELBULBER = 12,
} FptSdfId;

enum {
    FPT_SDF_PROGRAM_MAX_OPS = 64,
    FPT_SDF_FLAT_UNION_MAX_PRIMITIVES = 64,
    FPT_SDF_GRADIENT_MAX_STOPS = 16,
    FPT_HDRI_PATH_CAPACITY = 256,
    FPT_HDRI_LUT_WIDTH = 32,
    FPT_HDRI_LUT_HEIGHT = 16,
};

enum {
    FPT_SDF_STITCH_STATE_FULL = 0,
    FPT_SDF_STITCH_STATE_LEAN = 1,
};

typedef enum FptSdfOpcode {
    FPT_SDF_OP_ABS = 1,
    FPT_SDF_OP_TRANSLATE = 2,
    FPT_SDF_OP_SCALE = 3,
    FPT_SDF_OP_ROTATE_X = 4,
    FPT_SDF_OP_ROTATE_Y = 5,
    FPT_SDF_OP_ROTATE_Z = 6,
    FPT_SDF_OP_REPEAT = 7,
    FPT_SDF_OP_SORT_DESC = 8,
    FPT_SDF_OP_SPHERE = 16,
    FPT_SDF_OP_BOX = 17,
    FPT_SDF_OP_PLANE = 18,
    FPT_SDF_OP_UNION = 24,
    FPT_SDF_OP_INTERSECTION = 25,
    FPT_SDF_OP_SUBTRACT = 26,
    FPT_SDF_OP_ORBIT_ADD = 32,
} FptSdfOpcode;

struct FptSdfInstruction {
    uint32_t opcode;
    uint32_t flags;
    uint32_t material_index;
    uint32_t _pad0;
    float data[4];
};

struct FptPrimitiveInstance {
    float transform[12];
    float data[4];
    uint32_t opcode;
    float distance_scale;
    uint32_t source_instruction;
    uint32_t _pad0;
};

struct FptAffineTransform {
    float transform[12];
    float distance_scale;
    uint32_t _pad0[3];
};

struct FptIndexedPrimitive {
    float data[4];
    uint32_t opcode;
    uint32_t transform_index;
    uint32_t source_instruction;
    uint32_t combine_mode;
};

struct FptTypedSoAProgram {
    uint32_t sphere_count;
    uint32_t box_count;
    uint32_t plane_count;
    uint32_t _pad0;
    float sphere_x[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float sphere_y[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float sphere_z[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float sphere_radius[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    uint32_t sphere_source[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float box_x[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float box_y[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float box_z[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float box_half_x[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float box_half_y[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float box_half_z[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    uint32_t box_source[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float plane_x[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float plane_y[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float plane_z[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float plane_center_x[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float plane_center_y[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float plane_center_z[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float plane_offset[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    uint32_t plane_source[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
};

typedef enum FptSdfAccumulationMode {
    FPT_SDF_ACCUMULATION_AUTO = 0,
    FPT_SDF_ACCUMULATION_PER_SAMPLE = 1,
    FPT_SDF_ACCUMULATION_BATCH = 2,
    FPT_SDF_ACCUMULATION_CHUNKED = 3,
} FptSdfAccumulationMode;

typedef enum FptRendererBackend {
    FPT_RENDERER_SDF = 0,
    FPT_RENDERER_VOXEL = 1,
    FPT_RENDERER_BOUND_GRID = 2,
    FPT_RENDERER_REGIONAL = 3,
} FptRendererBackend;

typedef enum FptVoxelNormalMode {
    FPT_VOXEL_NORMAL_FACE = 0,
    FPT_VOXEL_NORMAL_SMOOTH = 1,
    FPT_VOXEL_NORMAL_EXACT = 2,
} FptVoxelNormalMode;

typedef enum FptVoxelMaterialMode {
    FPT_VOXEL_MATERIAL_STORED = 0,
    FPT_VOXEL_MATERIAL_EXACT = 1,
} FptVoxelMaterialMode;

typedef enum FptVoxelOffsetMode {
    FPT_VOXEL_OFFSET_LEGACY = 0,
    FPT_VOXEL_OFFSET_PRECISION = 1,
} FptVoxelOffsetMode;

typedef enum FptVoxelStorage {
    FPT_VOXEL_STORAGE_DENSE = 0,
    FPT_VOXEL_STORAGE_SPARSE_BRICKS = 1,
    FPT_VOXEL_STORAGE_TEMPLATE_BRICKS = 3,
} FptVoxelStorage;

typedef enum FptVoxelCoverageMode {
    FPT_VOXEL_COVERAGE_LEGACY = 0,
    FPT_VOXEL_COVERAGE_LIPSCHITZ = 1,
    FPT_VOXEL_COVERAGE_INTERVAL = 2,
} FptVoxelCoverageMode;

typedef enum FptVoxelSurfacePayload {
    FPT_VOXEL_SURFACE_NONE = 0,
    FPT_VOXEL_SURFACE_NORMAL = 1,
    FPT_VOXEL_SURFACE_PLANE = 2,
    FPT_VOXEL_SURFACE_BOUNDED_PATCH = 3,
    FPT_VOXEL_SURFACE_COMPLEX_PATCH = 4,
} FptVoxelSurfacePayload;

typedef enum FptVoxelBuildMode {
    FPT_VOXEL_BUILD_STAGING = 0,
    FPT_VOXEL_BUILD_DIRECT = 1,
} FptVoxelBuildMode;

typedef enum FptVoxelLeafRefinement {
    FPT_VOXEL_LEAF_REFINEMENT_NONE = 0,
    FPT_VOXEL_LEAF_REFINEMENT_SECANT_BISECTION = 1,
    FPT_VOXEL_LEAF_REFINEMENT_RESTRICTED_TRACE = 2,
    FPT_VOXEL_LEAF_REFINEMENT_FIXED_DE = 3,
} FptVoxelLeafRefinement;

struct FptRenderConfig {
    uint32_t width;
    uint32_t height;
    uint32_t samples;
    uint32_t preview;
    uint32_t sdf_id;
    uint32_t glass_mode;
    uint32_t sdf_accumulation_mode;
    uint32_t sdf_profile;
    uint32_t sdf_bounce_cap;
    uint32_t sdf_russian_roulette;
    uint32_t sdf_normal_mode;
    uint32_t sdf_chunk_samples;
    float sdf_rr_start;
    float sdf_rr_min_prob;
    float mandel_iteration_scale;

    float camera_position[3];
    float camera_yaw_pitch[2];
    float camera_roll;
    float camera_fov;
    float camera_dof;
    float focus_distance;

    float render[8];
    float world[7];
    float world_one_color[3];
    float sun[5];
    float sun_color[3];
    float background_gradient[6];
    float post[7];
    float set_values[40];
    float vset_values[133];

    uint32_t sdf_program_count;
    uint32_t gradient_count;
    uint32_t material_mode;
    uint32_t hdri_enabled;
    uint32_t hdri_width;
    uint32_t hdri_height;
    uint32_t _pad_hdri[2];
    uint32_t fractal_style_mode;
    uint32_t _pad_style[3];
    float fractal_style[12];
    float program_material[8];
    struct FptSdfInstruction sdf_program[FPT_SDF_PROGRAM_MAX_OPS];
    float gradient_stops[FPT_SDF_GRADIENT_MAX_STOPS][4];
    uint8_t hdri_path[FPT_HDRI_PATH_CAPACITY];
    uint16_t hdri_lut[FPT_HDRI_LUT_WIDTH * FPT_HDRI_LUT_HEIGHT * 3];
    uint32_t renderer_backend;
    uint32_t voxel_resolution;
    uint32_t voxel_normal_mode;
    uint32_t voxel_storage;
    float voxel_bounds_min[3];
    float voxel_surface_band;
    float voxel_bounds_max[3];
    uint32_t voxel_fill_interior;
    uint32_t voxel_coverage_mode;
    uint32_t voxel_build_mode;
    uint32_t voxel_brick_rejection;
    uint32_t voxel_leaf_refinement;
    uint32_t voxel_material_mode;
    uint32_t voxel_offset_mode;
    uint32_t bound_grid_resolution;
    uint32_t bound_grid_profile;
    uint32_t bound_grid_profile_stride;
    uint32_t bound_grid_cage_bounds;
    uint32_t bound_grid_directional;
    uint32_t sdf_program_source_count;
    uint32_t sdf_program_optimization;
    uint32_t regional_program_resolution;
    uint32_t bound_grid_fp16;
    uint32_t sdf_flat_union_count;
    struct FptPrimitiveInstance
        sdf_flat_union_instances[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    uint32_t sdf_shading_program_count;
    uint32_t sdf_geometry_split;
    struct FptSdfInstruction sdf_shading_program[FPT_SDF_PROGRAM_MAX_OPS];
    uint32_t sdf_topology_specialization;
    uint32_t sdf_runtime_source_bytecode;
    uint32_t sdf_function_stitching;
    uint32_t sdf_stitched_surface;
    uint32_t sdf_stitch_validation;
    uint32_t sdf_stitch_distance_only;
    uint32_t sdf_stitch_split_graph;
    uint32_t sdf_stitch_fusion;
    uint32_t sdf_canonical_count;
    uint32_t sdf_canonical_source_count;
    struct FptPrimitiveInstance
        sdf_canonical_primitives[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    uint32_t sdf_canonical_transform_count;
    uint32_t _pad_canonical_indexed[3];
    struct FptAffineTransform
        sdf_canonical_transforms[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    struct FptIndexedPrimitive
        sdf_indexed_primitives[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    struct FptTypedSoAProgram sdf_typed_soa;
};

struct FptBoundGridStats {
    uint64_t macro_cells[3];
    uint64_t certified_skips[3];
    uint64_t candidate_intervals[3];
    uint64_t candidate_misses[3];
    uint64_t candidate_hits[3];
    uint64_t unknown_intervals[3];
    uint64_t field_evaluations[3];
    uint64_t directional_steps[3];
    uint64_t cell_exit_clamps[3];
    uint64_t unknown_derivative_intervals[3];
    uint64_t profiled_paths;
    uint64_t certified_cells;
    uint64_t unknown_cells;
    uint64_t sampled_bound_failures;
    uint64_t sampled_false_skips;
    uint64_t certified_derivative_cells;
    uint64_t unknown_derivative_cells;
    uint64_t sampled_derivative_failures;
    uint64_t regional_cells;
    uint64_t regional_fallback_cells;
    uint64_t regional_unique_programs;
    uint64_t regional_retained_instructions;
    uint64_t regional_sampled_distance_failures;
    uint64_t regional_profiled_paths;
    uint64_t regional_distance_evaluations[3];
    uint64_t regional_atlas_evaluations[3];
    uint64_t regional_full_program_evaluations[3];
    uint64_t regional_cell_entries[3];
    uint64_t regional_same_cell_reuses[3];
    uint64_t regional_same_program_reuses[3];
    uint64_t regional_program_id_loads[3];
    uint64_t regional_header_loads[3];
    uint64_t regional_dynamic_instructions[3];
    uint64_t regional_pruned_cells;
    uint64_t regional_pruned_primitives;
    uint64_t regional_repeat_seam_fallback_cells;
    uint64_t regional_unsupported_fallback_cells;
    uint64_t regional_no_dominance_cells;
};

struct FptStitchValidationStats {
    uint64_t sample_count;
    uint64_t distance_failures;
    uint64_t gradient_failures;
    float max_distance_error;
    float max_gradient_error;
};

struct FptStitchPipelineStats {
    uint32_t instruction_count;
    uint32_t transform_instruction_count;
    uint32_t primitive_instruction_count;
    uint32_t primitive_type_runs;
    uint32_t union_count;
    uint32_t intersection_count;
    uint32_t subtraction_count;
    uint32_t thread_execution_width;
    uint32_t max_total_threads_per_threadgroup;
    uint32_t graph_node_count;
    uint64_t static_threadgroup_memory_bytes;
    uint64_t runtime_source_bytes;
    double runtime_library_compile_ms;
    double runtime_pipeline_link_ms;
};

struct FptSdfProfileStats {
    uint64_t primary_steps;
    uint64_t secondary_steps;
    uint64_t shadow_steps;
    uint64_t normal_evals;
    uint64_t bounces;
    uint64_t pixels;
    uint64_t distance_evals;
    uint64_t march_orbit_iterations;
    uint64_t refinement_steps;
    uint64_t normal_field_evals;
    uint64_t material_evals;
    uint64_t max_ray_steps;
    uint64_t max_pixel_steps;
    uint64_t distance_evals_by_phase[4];
    uint64_t orbit_iterations_by_phase[4];
    uint64_t formula_slot_iterations[9];
    uint64_t refinement_distance_evals;
    double primary_ms_estimate;
    double secondary_ms_estimate;
    double shadow_ms_estimate;
    double normal_ms_estimate;
    double bounce_ms_estimate;
};

struct FptAsyncJitStats {
    double fallback_render_ms;
    double jit_build_ms;
    double stitched_render_ms;
    uint32_t cache_status;
    uint32_t fallback_completed_before_jit;
    float max_absolute_error;
    uint32_t _pad0;
};

typedef enum FptDiagnosticMode {
    FPT_DIAGNOSTIC_DEPTH = 0,
    FPT_DIAGNOSTIC_NORMAL = 1,
    FPT_DIAGNOSTIC_MATERIAL = 2,
    FPT_DIAGNOSTIC_HIT_MASK = 3,
    FPT_DIAGNOSTIC_PATH_DIRECT = 4,
    FPT_DIAGNOSTIC_PATH_ENVIRONMENT = 5,
    FPT_DIAGNOSTIC_PATH_THROUGHPUT = 6,
    FPT_DIAGNOSTIC_PATH_FINAL = 7,
    FPT_DIAGNOSTIC_DIFFUSE_NORMAL = 8,
    FPT_DIAGNOSTIC_MANDEL_COLOR_INDEX = 9,
    FPT_DIAGNOSTIC_MANDEL_PALETTE_POSITION = 10,
    FPT_DIAGNOSTIC_SDF_PRIMARY_STEPS = 11,
    FPT_DIAGNOSTIC_SDF_SHADOW_STEPS = 12,
    FPT_DIAGNOSTIC_SDF_NORMAL_EVALS = 13,
    FPT_DIAGNOSTIC_SDF_BOUNCES = 14,
    FPT_DIAGNOSTIC_SDF_BOUNCE_CONTRIBUTION = 15,
} FptDiagnosticMode;

struct FptDiagnosticConfig {
    uint32_t mode;
    uint32_t _pad0;
    float max_distance;
    float normal_mix;
    uint32_t dispatch_origin[2];
};

struct FptMandelbulberFieldSample {
    float distance;
    float radius;
    float derivative;
    float iterations;
};

int fpt_metal_render(const char *metallib_path,
                     const char *stitch_metallib_path,
                     const char *stitch_archive_path,
                     const char *output_path,
                     const char *shader_source,
                     size_t shader_source_len,
                     const struct FptRenderConfig *config,
                     double *build_ms,
                     double *elapsed_ms,
                     uint64_t *voxel_memory_bytes,
                     uint32_t *voxel_active_bricks,
                     uint64_t *voxel_active_cells,
                     uint32_t *voxel_rejected_bricks,
                     struct FptBoundGridStats *bound_grid_stats,
                     uint32_t *stitch_cache_status,
                     struct FptStitchValidationStats *stitch_validation_stats,
                     struct FptStitchPipelineStats *stitch_pipeline_stats,
                     struct FptSdfProfileStats *sdf_profile_stats,
                     float *linear_output,
                     size_t linear_output_len,
                     char *error,
                     size_t error_len);

int fpt_metal_diagnostic_render(const char *metallib_path,
                                const char *output_path,
                                const char *structural_output_path,
                                const char *shader_source,
                                size_t shader_source_len,
                                const struct FptRenderConfig *config,
                                const struct FptDiagnosticConfig *diagnostic,
                                double *elapsed_ms,
                                char *error,
                                size_t error_len);

int fpt_mandelbulber_sample_field(
    const char *metallib_path,
    const char *shader_source,
    size_t shader_source_len,
    const struct FptRenderConfig *config,
    const float *points_xyzw,
    size_t point_count,
    struct FptMandelbulberFieldSample *samples,
    char *error,
    size_t error_len);

int fpt_metal_voxel_build(const char *metallib_path,
                          const struct FptRenderConfig *config,
                          void *cells,
                          size_t cells_len,
                          uint32_t surface_payload_mode,
                          uint32_t *packed_surface,
                          size_t packed_surface_len,
                          double *build_ms,
                          char *error,
                          size_t error_len);

int fpt_compare_images(const char *baseline_path,
                       const char *candidate_path,
                       const char *report_path,
                       char *error,
                       size_t error_len);

int fpt_metal_device_name(char *name, size_t name_len);

int fpt_test_voxel_dda(const char *metallib_path,
                       char *error,
                       size_t error_len);

int fpt_test_async_stitch_context(
    const char *metallib_path,
    const char *stitch_metallib_path,
    const char *stitch_archive_path,
    const struct FptRenderConfig *config,
    struct FptAsyncJitStats *stats,
    char *error,
    size_t error_len);

int fpt_test_typed_soa(
    const char *metallib_path,
    const struct FptRenderConfig *config,
    struct FptStitchValidationStats *stats,
    char *error,
    size_t error_len);

int fpt_metal_preview(const char *metallib_path,
                      const char *stitch_metallib_path,
                      const char *const *stitch_archive_paths,
                      const char *shader_source,
                      size_t shader_source_len,
                      const struct FptRenderConfig *config,
                      const struct FptRenderConfig *scene_configs,
                      uint32_t scene_config_count,
                      uint32_t scene_config_index,
                      char *error,
                      size_t error_len);

#ifdef __cplusplus
}
#endif
