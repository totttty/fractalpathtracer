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
} FptSdfId;

enum {
    FPT_SDF_PROGRAM_MAX_OPS = 64,
    FPT_SDF_GRADIENT_MAX_STOPS = 16,
    FPT_HDRI_PATH_CAPACITY = 256,
    FPT_HDRI_LUT_WIDTH = 32,
    FPT_HDRI_LUT_HEIGHT = 16,
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

typedef enum FptSdfAccumulationMode {
    FPT_SDF_ACCUMULATION_AUTO = 0,
    FPT_SDF_ACCUMULATION_PER_SAMPLE = 1,
    FPT_SDF_ACCUMULATION_BATCH = 2,
    FPT_SDF_ACCUMULATION_CHUNKED = 3,
} FptSdfAccumulationMode;

typedef enum FptRendererBackend {
    FPT_RENDERER_SDF = 0,
    FPT_RENDERER_VOXEL = 1,
} FptRendererBackend;

typedef enum FptVoxelNormalMode {
    FPT_VOXEL_NORMAL_FACE = 0,
    FPT_VOXEL_NORMAL_SMOOTH = 1,
} FptVoxelNormalMode;

typedef enum FptVoxelStorage {
    FPT_VOXEL_STORAGE_DENSE = 0,
    FPT_VOXEL_STORAGE_SPARSE_BRICKS = 1,
} FptVoxelStorage;

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

    float camera_position[3];
    float camera_yaw_pitch[2];
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
    float vset_values[120];

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
};

typedef enum FptDiagnosticMode {
    FPT_DIAGNOSTIC_DEPTH = 0,
    FPT_DIAGNOSTIC_NORMAL = 1,
    FPT_DIAGNOSTIC_MATERIAL = 2,
    FPT_DIAGNOSTIC_PATH_DIRECT = 4,
    FPT_DIAGNOSTIC_PATH_ENVIRONMENT = 5,
    FPT_DIAGNOSTIC_PATH_THROUGHPUT = 6,
    FPT_DIAGNOSTIC_PATH_FINAL = 7,
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
};

int fpt_metal_render(const char *metallib_path,
                     const char *output_path,
                     const struct FptRenderConfig *config,
                     double *build_ms,
                     double *elapsed_ms,
                     uint64_t *voxel_memory_bytes,
                     uint32_t *voxel_active_bricks,
                     char *error,
                     size_t error_len);

int fpt_metal_diagnostic_render(const char *metallib_path,
                                const char *output_path,
                                const struct FptRenderConfig *config,
                                const struct FptDiagnosticConfig *diagnostic,
                                double *elapsed_ms,
                                char *error,
                                size_t error_len);

int fpt_compare_images(const char *baseline_path,
                       const char *candidate_path,
                       const char *report_path,
                       char *error,
                       size_t error_len);

int fpt_metal_device_name(char *name, size_t name_len);

int fpt_metal_preview(const char *metallib_path,
                      const struct FptRenderConfig *config,
                      const struct FptRenderConfig *scene_configs,
                      uint32_t scene_config_count,
                      uint32_t scene_config_index,
                      char *error,
                      size_t error_len);

#ifdef __cplusplus
}
#endif
