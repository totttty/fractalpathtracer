#include <metal_stdlib>
using namespace metal;

constant float pi = 3.14159265359f;
constant float inf = 1.0e20f;

#if defined(FPT_TOPOLOGY_DUAL_SURFACE)
constant bool fptTopologyUseGeneratedSurface [[function_constant(0)]];
#endif

enum {
    SDF_CORNELL_BOX = 0,
    SDF_GLASS_BALL = 1,
    SDF_BALL_FRACTAL = 2,
    SDF_CAGE_FRACTAL = 3,
    SDF_IFS_FRACTAL = 4,
    SDF_MANDELBOX_FRACTAL = 5,
    SDF_MENGER_SPONGE = 6,
    SDF_TOWER_FRACTAL = 7,
    SDF_TREE_FRACTAL = 8,
    SDF_PROGRAM = 9,
    SDF_README_CORNELL = 10,
    SDF_README_GLASS = 11,
    SDF_MANDELBULBER = 12,
};

enum {
    VOXEL_BUILD_STAGING = 0,
    VOXEL_BUILD_DIRECT = 1,
};

enum {
    VOXEL_STORAGE_DENSE = 0,
    VOXEL_STORAGE_SPARSE_BRICKS = 1,
    VOXEL_STORAGE_TEMPLATE_BRICKS = 3,
};

enum {
    VOXEL_COVERAGE_LEGACY = 0,
    VOXEL_COVERAGE_LIPSCHITZ = 1,
    VOXEL_COVERAGE_INTERVAL = 2,
};

enum {
    VOXEL_LEAF_REFINEMENT_NONE = 0,
    VOXEL_LEAF_REFINEMENT_SECANT_BISECTION = 1,
    VOXEL_LEAF_REFINEMENT_RESTRICTED_TRACE = 2,
    VOXEL_LEAF_REFINEMENT_FIXED_DE = 3,
};

enum {
    VOXEL_MATERIAL_STORED = 0,
    VOXEL_MATERIAL_EXACT = 1,
};

enum {
    VOXEL_OFFSET_LEGACY = 0,
    VOXEL_OFFSET_PRECISION = 1,
};

constant uint FPT_SDF_PROGRAM_MAX_OPS = 64u;
#if defined(FPT_BUILTIN_CONFIG)
constant uint FPT_SDF_FLAT_UNION_MAX_PRIMITIVES = 1u;
#else
constant uint FPT_SDF_FLAT_UNION_MAX_PRIMITIVES = 64u;
#endif
constant uint FPT_SDF_GRADIENT_MAX_STOPS = 16u;

enum {
    SDF_OP_ABS = 1,
    SDF_OP_TRANSLATE = 2,
    SDF_OP_SCALE = 3,
    SDF_OP_ROTATE_X = 4,
    SDF_OP_ROTATE_Y = 5,
    SDF_OP_ROTATE_Z = 6,
    SDF_OP_REPEAT = 7,
    SDF_OP_SORT_DESC = 8,
    SDF_OP_SPHERE = 16,
    SDF_OP_BOX = 17,
    SDF_OP_PLANE = 18,
    SDF_OP_UNION = 24,
    SDF_OP_INTERSECTION = 25,
    SDF_OP_SUBTRACT = 26,
    SDF_OP_ORBIT_ADD = 32,
};

struct FptSdfInstruction {
    uint opcode;
    uint flags;
    uint material_index;
    uint sdf_chunk_samples;
    float data[4];
};

struct FptPrimitiveInstance {
    float transform[12];
    float data[4];
    uint opcode;
    float distance_scale;
    uint source_instruction;
    uint _pad0;
};

struct FptAffineTransform {
    float transform[12];
    float distance_scale;
    uint _pad0[3];
};

struct FptIndexedPrimitive {
    float data[4];
    uint opcode;
    uint transform_index;
    uint source_instruction;
    uint combine_mode;
};

struct FptTypedSoAProgram {
    uint sphere_count;
    uint box_count;
    uint plane_count;
    uint _pad0;
    float sphere_x[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float sphere_y[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float sphere_z[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float sphere_radius[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    uint sphere_source[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float box_x[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float box_y[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float box_z[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float box_half_x[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float box_half_y[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float box_half_z[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    uint box_source[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float plane_x[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float plane_y[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float plane_z[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float plane_center_x[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float plane_center_y[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float plane_center_z[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    float plane_offset[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    uint plane_source[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
};

struct FptRenderConfig {
    uint width;
    uint height;
    uint samples;
    uint preview;
    uint sdf_id;
    uint glass_mode;
    uint sdf_accumulation_mode;
    uint sdf_profile;
    uint sdf_bounce_cap;
    uint sdf_russian_roulette;
    uint sdf_normal_mode;
    uint _pad0;
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
    uint sdf_program_count;
    uint gradient_count;
    uint material_mode;
    uint hdri_enabled;
    uint hdri_width;
    uint hdri_height;
    uint _pad_hdri[2];
    uint fractal_style_mode;
    uint _pad_style[3];
    float fractal_style[12];
    float program_material[8];
    FptSdfInstruction sdf_program[FPT_SDF_PROGRAM_MAX_OPS];
    float gradient_stops[FPT_SDF_GRADIENT_MAX_STOPS][4];
    uchar hdri_path[256];
    ushort hdri_lut[32 * 16 * 3];
    uint renderer_backend;
    uint voxel_resolution;
    uint voxel_normal_mode;
    uint voxel_storage;
    float voxel_bounds_min[3];
    float voxel_surface_band;
    float voxel_bounds_max[3];
    uint voxel_fill_interior;
    uint voxel_coverage_mode;
    uint voxel_build_mode;
    uint voxel_brick_rejection;
    uint voxel_leaf_refinement;
    uint voxel_material_mode;
    uint voxel_offset_mode;
    uint bound_grid_resolution;
    uint bound_grid_profile;
    uint bound_grid_profile_stride;
    uint bound_grid_cage_bounds;
    uint bound_grid_directional;
    uint sdf_program_source_count;
    uint sdf_program_optimization;
    uint regional_program_resolution;
    uint bound_grid_fp16;
    uint sdf_flat_union_count;
    FptPrimitiveInstance
        sdf_flat_union_instances[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    uint sdf_shading_program_count;
    uint sdf_geometry_split;
    FptSdfInstruction sdf_shading_program[FPT_SDF_PROGRAM_MAX_OPS];
    uint sdf_topology_specialization;
    uint sdf_runtime_source_bytecode;
    uint sdf_function_stitching;
    uint sdf_stitched_surface;
    uint sdf_stitch_validation;
    uint sdf_stitch_distance_only;
    uint sdf_stitch_split_graph;
    uint sdf_stitch_fusion;
    uint sdf_canonical_count;
    uint sdf_canonical_source_count;
    FptPrimitiveInstance
        sdf_canonical_primitives[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    uint sdf_canonical_transform_count;
    uint _pad_canonical_indexed[3];
    FptAffineTransform
        sdf_canonical_transforms[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    FptIndexedPrimitive
        sdf_indexed_primitives[FPT_SDF_FLAT_UNION_MAX_PRIMITIVES];
    FptTypedSoAProgram sdf_typed_soa;
};

enum {
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
};

struct FptDiagnosticConfig {
    uint mode;
    uint _pad0;
    float max_distance;
    float normal_mix;
    uint2 dispatch_origin;
};

struct FptSdfProfileConfig {
    uint frame_index;
    uint stride;
    uint _pad0;
    uint _pad1;
};

struct FptAccumulationChunk {
    uint start_sample;
    uint sample_count;
};

struct FptAccumulationTile {
    uint2 dispatch_origin;
};

struct FptSdfProfileCounts {
    atomic_uint primary_steps;
    atomic_uint secondary_steps;
    atomic_uint shadow_steps;
    atomic_uint normal_evals;
    atomic_uint bounces;
    atomic_uint pixels;
    atomic_uint distance_evals;
    atomic_uint march_orbit_iterations;
    atomic_uint refinement_steps;
    atomic_uint normal_field_evals;
    atomic_uint material_evals;
    atomic_uint max_ray_steps;
    atomic_uint max_pixel_steps;
    atomic_uint distance_evals_by_phase[4];
    atomic_uint orbit_iterations_by_phase[4];
    atomic_uint formula_slot_iterations[9];
    atomic_uint refinement_distance_evals;
};

struct Material {
    float3 rgb;
    float roughness;
    float specular;
    float translucency;
    float ior;
    float emission;
};

struct SDFResult {
    float distance;
    Material material;
};

struct SceneHit {
    float d;
    int id;
};

struct DeResult {
    float d;
    float orbit;
};

struct ProgramSurface {
    float distance;
    float3 gradient;
};

struct ProgramBounds {
    float minimum;
    float maximum;
    float lipschitz;
    uint certified;
};

struct ProgramDerivativeBounds {
    float3 lower;
    float3 upper;
    uint certified;
};

struct RegionalProgramHeader {
    uint instruction_offset;
    uint instruction_count;
    uint primitive_offset;
    uint primitive_count;
};

struct RegionalProgramLocalStats {
    uint distance_evaluations[3];
    uint atlas_evaluations[3];
    uint full_program_evaluations[3];
    uint cell_entries[3];
    uint same_cell_reuses[3];
    uint same_program_reuses[3];
    uint program_id_loads[3];
    uint header_loads[3];
    uint dynamic_instructions[3];
    uint profiled_paths;
};

struct RegionalProgramLookupState {
    uint cell_index;
    uint program_id;
    bool valid;
};

static RegionalProgramLocalStats emptyRegionalProgramLocalStats() {
    RegionalProgramLocalStats stats;
    for (uint ray_class = 0u; ray_class < 3u; ++ray_class) {
        stats.distance_evaluations[ray_class] = 0u;
        stats.atlas_evaluations[ray_class] = 0u;
        stats.full_program_evaluations[ray_class] = 0u;
        stats.cell_entries[ray_class] = 0u;
        stats.same_cell_reuses[ray_class] = 0u;
        stats.same_program_reuses[ray_class] = 0u;
        stats.program_id_loads[ray_class] = 0u;
        stats.header_loads[ray_class] = 0u;
        stats.dynamic_instructions[ray_class] = 0u;
    }
    stats.profiled_paths = 0u;
    return stats;
}

static float floatRoundDown(float value) {
    if (!isfinite(value)) return value;
    if (value == 0.0f) return -as_type<float>(1u);
    uint bits = as_type<uint>(value);
    bits = value < 0.0f ? bits + 1u : bits - 1u;
    return as_type<float>(bits);
}

static float floatRoundUp(float value) {
    if (!isfinite(value)) return value;
    if (value == 0.0f) return as_type<float>(1u);
    uint bits = as_type<uint>(value);
    bits = value < 0.0f ? bits - 1u : bits + 1u;
    return as_type<float>(bits);
}

static Material defaultMaterial() {
    Material m;
    m.rgb = float3(1.0f);
    m.roughness = 1.0f;
    m.specular = 0.0f;
    m.translucency = 0.0f;
    m.ior = 1.5f;
    m.emission = 0.0f;
    return m;
}

static float setv(constant FptRenderConfig &cfg, int i) {
    return cfg.set_values[i];
}

static float3 cameraPos(constant FptRenderConfig &cfg) {
    return float3(cfg.camera_position[0], cfg.camera_position[1], cfg.camera_position[2]);
}

static float2 cameraYawPitch(constant FptRenderConfig &cfg) {
    return float2(cfg.camera_yaw_pitch[0], cfg.camera_yaw_pitch[1]);
}

static float mod1(float x, float y) {
    return x - y * floor(x / y);
}

static float3 mod3(float3 x, float y) {
    return x - y * floor(x / y);
}

static float pow5(float x) {
    float x2 = x * x;
    return x2 * x2 * x;
}

// OpenCL's length/division path stays inside the inverse-trig domain for the
// bundled formulas. Metal fast-math can round a normalized component a few
// ulps past either endpoint, so generated formulas use these guarded forms.
static float mandelSafeAsin(float value) {
    return asin(clamp(value, -1.0f, 1.0f));
}

static float mandelSafeAcos(float value) {
    return acos(clamp(value, -1.0f, 1.0f));
}

static float2 rot2(float2 p, float a) {
    float s = sin(a);
    float c = cos(a);
    return float2(c * p.x - s * p.y, s * p.x + c * p.y);
}

static float3 rotateCamera(float3 v, float2 cam_yp) {
    float yaw = cam_yp.x;
    float pitch = cam_yp.y;
    v = float3(v.x, v.z * sin(pitch) + v.y * cos(pitch), v.z * cos(pitch) - v.y * sin(pitch));
    v = float3(v.x * cos(yaw) + v.z * sin(yaw), v.y, -v.x * sin(yaw) + v.z * cos(yaw));
    return v;
}

static float3 rotateCamera(float3 v, float2 cam_yp, float roll) {
    v.xy = rot2(v.xy, roll);
    return rotateCamera(v, cam_yp);
}

static bool mandelbulberProjectionVisible(float2 xy,
                                          constant FptRenderConfig &cfg) {
    if (cfg.vset_values[107] < 2.5f || cfg.vset_values[107] > 3.5f) {
        return true;
    }
    float internal_fov = cfg.camera_fov * pi / 180.0f;
    return length(xy) <= (0.5f * pi / max(internal_fov, 1.0e-6f));
}

static float3 mandelbulberCameraRay(float2 xy,
                                    constant FptRenderConfig &cfg) {
    float projection = cfg.vset_values[107];
    float internal_fov = cfg.camera_fov * pi / 180.0f;
    float3 local_direction;
    if (projection > 0.5f && projection < 1.5f ||
        projection > 2.5f && projection < 3.5f) {
        float radius = length(xy);
        if (radius <= 1.0e-8f) {
            local_direction = float3(0.0f, 0.0f, 1.0f);
        } else {
            float radial_sine = sin(radius * internal_fov) / radius;
            local_direction = float3(xy.x * radial_sine,
                                     xy.y * radial_sine,
                                     cos(radius * internal_fov));
        }
    } else if (projection > 1.5f && projection < 2.5f) {
        float aspect = float(cfg.width) / float(cfg.height);
        float2 normalized_screen = float2(xy.x * (2.0f / aspect), xy.y);
        float2 angular = normalized_screen * (0.5f * internal_fov);
        local_direction = float3(sin(angular.x) * cos(angular.y),
                                 sin(angular.y),
                                 cos(angular.x) * cos(angular.y));
    } else {
        float focal_length = 1.0f / tan(0.5f * internal_fov);
        local_direction = normalize(float3(xy, focal_length));
    }
    return rotateCamera(normalize(local_direction),
                        cameraYawPitch(cfg), cfg.camera_roll);
}

static float3 rotateIFS(float3 z, float AngPFXY, float AngPFYZ, float AngPFXZ) {
    float sPFXY = sin(AngPFXY * pi / 180.0f);
    float cPFXY = cos(AngPFXY * pi / 180.0f);
    float sPFYZ = sin(AngPFYZ * pi / 180.0f);
    float cPFYZ = cos(AngPFYZ * pi / 180.0f);
    float sPFXZ = sin(AngPFXZ * pi / 180.0f);
    float cPFXZ = cos(AngPFXZ * pi / 180.0f);
    float zx = z.x;
    float zy = z.y;
    float zz = z.z;
    float t = zx;
    zx = cPFXY * t - sPFXY * zy;
    zy = sPFXY * t + cPFXY * zy;
    t = zx;
    zx = cPFXZ * t + sPFXZ * zz;
    zz = -sPFXZ * t + cPFXZ * zz;
    t = zy;
    zy = cPFYZ * t - sPFYZ * zz;
    zz = sPFYZ * t + cPFYZ * zz;
    return float3(zx, zy, zz);
}

static float3 hsv2rgb(float3 c) {
    float4 K = float4(1.0f, 2.0f / 3.0f, 1.0f / 3.0f, 3.0f);
    float3 p = abs(fract(c.xxx + K.xyz) * 6.0f - K.www);
    return c.z * mix(K.xxx, clamp(p - K.xxx, 0.0f, 1.0f), c.y);
}

static uint hash13bits(float3 src) {
    uint3 bits = as_type<uint3>(src);
    uint h = 1190494759u;
    h = (h ^ bits.x) * 0x5bd1e995u;
    h = (h ^ bits.y) * 0x5bd1e995u;
    h = (h ^ bits.z) * 0x5bd1e995u;
    h ^= h >> 13u;
    h *= 0x5bd1e995u;
    h ^= h >> 15u;
    return h;
}

static float hash13(float3 src) {
    return float(hash13bits(src) & 0x00ffffffu) / float(0x01000000u);
}

static float3 randomVector(float3 normal, float2 xy, float seed) {
    float h1 = hash13(float3(xy, seed * 2.0f + 0.0f));
    float h2 = hash13(float3(xy, seed * 2.0f + 1.0f));
    float3 n = normalize(normal);
    float3 uu = normalize(cross(n, float3(0.0f, 1.0f, 1.0f)));
    float3 vv = cross(uu, n);
    float ra = sqrt(h2);
    float rx = ra * cos(pi * 2.0f * h1);
    float ry = ra * sin(pi * 2.0f * h1);
    float rz = sqrt(max(1.0f - h2, 0.0f));
    return normalize(rx * uu + ry * vv + rz * n);
}

static float2 randomPoint(float power, float2 xy, float seed) {
    float h1 = hash13(float3(xy, seed * 2.0f + 0.0f));
    float h2 = hash13(float3(xy, seed * 2.0f + 1.0f));
    float r = 2.0f * h1 * pi;
    float r2 = 2.0f * h2 * pi;
    return float2(cos(r), sin(r)) * sqrt(r2) * power;
}

static float sdBox(float3 p, float3 b) {
    float3 q = abs(p) - b;
    return length(max(q, float3(0.0f))) + min(max(q.x, max(q.y, q.z)), 0.0f);
}

static float sdSphere(float3 p, float r) {
    return length(p) - r;
}

static float sdCappedCylinder(float3 p, float half_height, float radius) {
    float2 d = abs(float2(length(p.xz), p.y)) - float2(radius, half_height);
    return min(max(d.x, d.y), 0.0f) + length(max(d, float2(0.0f)));
}

static SceneHit hit(float d, int id) {
    SceneHit h;
    h.d = d;
    h.id = id;
    return h;
}

static SceneHit opUnion(SceneHit a, SceneHit b) {
    return (a.d < b.d) ? a : b;
}

static float roomSDF(float3 p, thread int &id) {
    float d = 1000.0f;
    id = 0;
    float floor_d = p.y + 1.0f;
    if (floor_d < d) { d = floor_d; id = 0; }
    float ceil_d = 1.0f - p.y;
    if (ceil_d < d) { d = ceil_d; id = 0; }
    float back_d = 1.0f - p.z;
    if (back_d < d) { d = back_d; id = 0; }
    float left_d = p.x + 1.0f;
    if (left_d < d) { d = left_d; id = 1; }
    float right_d = 1.0f - p.x;
    if (right_d < d) { d = right_d; id = 2; }
    return d;
}

static SceneHit cornellScene(float3 p) {
    int room_id = 0;
    SceneHit scene = hit(roomSDF(p, room_id), room_id);
    scene = opUnion(scene, hit(sdBox(p - float3(0.0f, 0.985f, 0.15f), float3(0.34f, 0.012f, 0.24f)), 3));
    float3 bp = p - float3(-0.36f, -0.68f, 0.20f);
    bp.xz = rot2(bp.xz, 0.28f);
    scene = opUnion(scene, hit(sdBox(bp, float3(0.28f, 0.32f, 0.28f)), 5));
    float3 tp = p - float3(0.36f, -0.48f, -0.16f);
    tp.xz = rot2(tp.xz, -0.34f);
    scene = opUnion(scene, hit(sdBox(tp, float3(0.25f, 0.52f, 0.25f)), 4));
    return scene;
}

static SceneHit glassBallScene(float3 p) {
    SceneHit scene = hit(p.y + 0.72f, 0);
    scene = opUnion(scene, hit(1.8f - p.z, 2));
    scene = opUnion(scene, hit(sdSphere(p - float3(0.0f, -0.22f, 0.35f), 0.50f), 1));
    scene = opUnion(scene, hit(sdBox(p - float3(-0.58f, 1.05f, -0.28f), float3(0.32f, 0.04f, 0.32f)), 3));
    return scene;
}

static SceneHit readmeCornellScene(float3 p) {
    int room_id = 0;
    SceneHit scene = hit(roomSDF(p, room_id), room_id);
    scene = opUnion(scene, hit(sdSphere(p - float3(0.0f, 0.02f, 0.28f), 0.31f), 6));
    scene = opUnion(scene, hit(sdBox(p - float3(0.0f, 0.985f, 0.16f), float3(0.30f, 0.010f, 0.20f)), 3));
    return scene;
}

static SceneHit readmeGlassScene(float3 p) {
    int room_id = 0;
    SceneHit scene = hit(roomSDF(p, room_id), room_id);
    scene = opUnion(scene, hit(sdCappedCylinder(p - float3(0.0f, -0.52f, 0.28f), 0.40f, 0.31f), 7));
    scene = opUnion(scene, hit(sdSphere(p - float3(0.0f, 0.17f, 0.28f), 0.30f), 6));
    scene = opUnion(scene, hit(sdBox(p - float3(0.0f, 0.985f, 0.16f), float3(0.30f, 0.010f, 0.20f)), 3));
    return scene;
}

static DeResult deBall(float3 p0, constant FptRenderConfig &cfg) {
    float4 p = float4(p0, 1.0f);
    float sdf = 0.0f;
    float orbit = 0.0f;
    float a = setv(cfg, 2) * 2.0f + 2.0f;
    float b = setv(cfg, 3) * 2.0f + 2.680f;
    float c = setv(cfg, 4) * 2.0f + 2.0f;
    float3 half_period = float3(a, b, c) * 0.5f;
    float fold_scale = 1.4f + setv(cfg, 5);
    for (int i = 0; i < 12; i++) {
        p.x = mod1(p.x - half_period.x, a) - half_period.x;
        p.y = mod1(p.y - half_period.y, b) - half_period.y;
        p.z = mod1(p.z - half_period.z, c) - half_period.z;
        p *= fold_scale / max(dot(p.xyz, p.xyz), 0.0001f);
        sdf = abs(p.y / p.w) * 0.25f;
        orbit += sdf;
    }
    DeResult r;
    r.d = max(abs(p.y / p.w) * 0.25f * 0.7f, length(p0) - setv(cfg, 6));
    r.d = max(r.d, abs(p0.y) - 1.0f);
    r.orbit = orbit * 2.0f;
    return r;
}

static DeResult deCage(float3 p0, constant FptRenderConfig &cfg) {
    float sdf = 0.0f;
    float col = 0.0f;
    float4 p = float4(p0, 3.0f);
    p *= 2.0f / min(dot(p.xyz, p.xyz), 30.0f);
    float3 cage_fold = float3(1.0f + setv(cfg, 2) * 4.0f,
                              12.0f + (setv(cfg, 3) * 4.0f) / 3.0f,
                              2.0f + setv(cfg, 4) * 4.0f);
    for (int i = 0; i < 12; i++) {
        p.xyz = float3(2.0f, 4.0f, 2.0f) - (abs(p.xyz) - cage_fold);
        p.xyz = mod3(p.xyz - 4.0f, 8.0f) - 4.0f;
        p *= 9.0f / min(dot(p.xyz, p.xyz), 12.0f);
        sdf = length(p.xyz - clamp(p.xyz, -1.2f, 1.2f)) / p.w;
        col += sdf;
    }
    DeResult r;
    r.d = max(sdf, length(p0) - setv(cfg, 5));
    r.orbit = col;
    return r;
}

static DeResult deIFS(float3 p, constant FptRenderConfig &cfg) {
    float Scale = 1.34f;
    float FoldY = 1.025709f;
    float FoldX = 1.025709f;
    float FoldZ = 0.035271f;
    float3 julia = float3(-1.763517f + setv(cfg, 5), 0.392486f + setv(cfg, 6), -1.734913f + setv(cfg, 7));
    float AngX = -51.080209f + setv(cfg, 2) * 90.0f;
    float AngY = setv(cfg, 3) * 90.0f;
    float AngZ = -29.096322f + setv(cfg, 4) * 90.0f;
    p = -3.036726f + abs(p);
    float sdf = 0.0f;
    float col = 0.0f;
    for (int i = 0; i < 50; i++) {
        p = rotateIFS(p, AngX, AngY, AngZ);
        p.x = abs(p.x + FoldX) - FoldX;
        p.y = abs(p.y + FoldY) - FoldY;
        p.z = abs(p.z + FoldZ) - FoldZ;
        p = p * Scale + julia;
        float l = length(p);
        sdf = l * pow(Scale, -float(i));
        col += sdf;
    }
    DeResult r;
    r.d = sdf * 0.7f;
    r.orbit = col;
    return r;
}

static DeResult deMandelbox(float3 pos, constant FptRenderConfig &cfg) {
    float SCALE = 2.8f + setv(cfg, 2);
    float MINRAD2 = 0.25f + setv(cfg, 3);
    float4 scale = float4(SCALE, SCALE, SCALE, abs(SCALE)) / MINRAD2;
    float minRad2 = clamp(MINRAD2, 1.0e-9f, 1.0f);
    float absScalem1 = abs(SCALE - 1.0f);
    float AbsScale = pow(abs(SCALE), -9.0f);
    float4 offset = float4(setv(cfg, 4), setv(cfg, 5), setv(cfg, 6), 0.0f);
    float4 p = float4(pos, 1.0f);
    float4 p0 = p;
    float sdf = 0.0f;
    float col = 0.0f;
    for (int i = 0; i < 12; i++) {
        p.xyz = clamp(p.xyz, -1.0f, 1.0f) * 2.0f - p.xyz;
        float r2 = dot(p.xyz, p.xyz);
        p *= clamp(max(minRad2 / max(r2, 0.00001f), minRad2), 0.0f, 1.0f);
        p = p * scale + p0 + offset;
        sdf = ((length(p.xyz) - absScalem1) / p.w - AbsScale);
        col += sdf;
    }
    DeResult r;
    r.d = sdf;
    r.orbit = col;
    return r;
}

static float cubeSdf(float3 p) {
    float3 d = abs(p) - float3(1.0f);
    return max(max(d.x, d.y), d.z);
}

static float mengerCut(float3 p, float s) {
    s *= 2.0f;
    p = mod3(p - s / 2.0f, s) - s / 2.0f;
    float da = max(abs(p.x), abs(p.y));
    float db = max(abs(p.y), abs(p.z));
    float dc = max(abs(p.z), abs(p.x));
    return min(da, min(db, dc)) - s / 6.0f;
}

static DeResult deMenger(float3 p, constant FptRenderConfig &cfg) {
    float sdf = cubeSdf(p);
    float scale = max(setv(cfg, 0), 0.0001f);
    float scale_divisor = max(setv(cfg, 1), 0.0001f);
    float2 rotation = float2(setv(cfg, 2), setv(cfg, 3));
    for (int i = 0; i < 12; i++) {
        sdf = max(sdf, -mengerCut(p, scale));
        scale /= scale_divisor;
        p = rotateCamera(p, rotation);
    }
    DeResult r;
    r.d = sdf;
    r.orbit = 1.0f;
    return r;
}

// Mandelbulber's SetRotation3 order is Rz(rotation.x) * Ry(rotation.y) *
// Rx(rotation.z). Apply it explicitly to avoid matrix-layout ambiguity.
static float3 mandelbulberRotation3(float3 value, float3 rotation) {
    float sx = sin(rotation.z);
    float cx = cos(rotation.z);
    value.yz = float2(value.y * cx - value.z * sx,
                      value.y * sx + value.z * cx);
    float sy = sin(rotation.y);
    float cy = cos(rotation.y);
    value.xz = float2(value.x * cy + value.z * sy,
                      -value.x * sy + value.z * cy);
    float sz = sin(rotation.x);
    float cz = cos(rotation.x);
    value.xy = float2(value.x * cz - value.y * sz,
                      value.x * sz + value.y * cz);
    return value;
}

// Mandelbulber's global fractal transform uses SetRotation2:
// Rz(rotation.z) * Ry(rotation.y) * Rx(rotation.x).
static float3 mandelbulberRotation2(float3 value, float3 rotation) {
    float sx = sin(rotation.x);
    float cx = cos(rotation.x);
    value.yz = float2(value.y * cx - value.z * sx,
                      value.y * sx + value.z * cx);
    float sy = sin(rotation.y);
    float cy = cos(rotation.y);
    value.xz = float2(value.x * cy + value.z * sy,
                      -value.x * sy + value.z * cy);
    float sz = sin(rotation.z);
    float cz = cos(rotation.z);
    value.xy = float2(value.x * cz - value.y * sz,
                      value.x * sz + value.y * cz);
    return value;
}

static float mandelbulberRepeatCoordinate(float value, float period) {
    if (!(period > 0.0f)) return value;
    return value - period * floor((value + 0.5f * period) / period);
}

static float3 mandelbulberGlobalPoint(float3 p,
                                     constant FptRenderConfig &cfg) {
    float world_scale = max(setv(cfg, 0), 1.0f);
    float3 point = float3(p.x, p.z, p.y) / world_scale;
    point -= float3(cfg.vset_values[120], cfg.vset_values[121],
                    cfg.vset_values[122]);
    point = mandelbulberRotation2(
        point,
        float3(cfg.vset_values[123], cfg.vset_values[124],
               cfg.vset_values[125]));
    float3 repeat = float3(cfg.vset_values[126], cfg.vset_values[127],
                           cfg.vset_values[128]);
    point = float3(mandelbulberRepeatCoordinate(point.x, repeat.x),
                   mandelbulberRepeatCoordinate(point.y, repeat.y),
                   mandelbulberRepeatCoordinate(point.z, repeat.z));
    return float3(point.x, point.z, point.y) * world_scale;
}

struct MandelFormulaIterationCounts {
    uint slots[9];
};

static float mandelInteractiveIterationScale(constant FptRenderConfig &cfg) {
#if defined(FPT_MANDEL_INTERACTIVE_REFINEMENT)
    float state = cfg.vset_values[131];
    if (state < 0.0f) return clamp(-state, 0.125f, 1.0f);
#else
    (void)cfg;
#endif
    return 1.0f;
}

// Remove formula iterations only when the projected pixel footprint is
// coarser than the scene's minimum detail threshold. The rate is expressed as
// iterations removed per doubling of that footprint, making the experiment
// position-dependent rather than a scene-wide iteration multiplier.
static int mandelbulberScreenIterationBudget(float3 world_position,
                                              constant FptRenderConfig &cfg,
                                              int iteration_multiplier) {
    int configured = clamp(int(setv(cfg, 1)) * max(iteration_multiplier, 1),
                           1, 4096);
    float rate = cfg.vset_values[130];
    if (!(rate > 0.0f) || !isfinite(rate)) return configured;
    float footprint = cfg.vset_values[115] > 0.5f
        ? length(cameraPos(cfg) - world_position) * cfg.vset_values[116]
        : cfg.vset_values[117];
    footprint = clamp(footprint, cfg.vset_values[118], cfg.vset_values[119]);
    float minimum = max(cfg.vset_values[118], 1.0e-12f);
    float octaves = max(log2(max(footprint / minimum, 1.0f)), 0.0f);
    int reduction = int(floor(octaves * rate));
    // Never discard more than the 25% scene-wide reduction that already
    // survived the visual gate. Low-iteration hybrid schedules otherwise
    // collapse to one pass when their min/max detail ratio spans many octaves.
    int floor_budget = max(int(ceil(float(configured) * 0.75f)), 1);
    return max(configured - reduction, floor_budget);
}

// FPT_MANDELBULBER_GENERATED_INSERTION_POINT

#ifndef FPT_MANDEL_GENERATED_FIELD
static MandelFormulaIterationCounts mandelbulberProfileFormulaIterations(
    int completed_iterations) {
    MandelFormulaIterationCounts counts = {};
    counts.slots[0] = uint(max(completed_iterations, 0));
    return counts;
}

// Exact procedural port of Mandelbulber's Kaleidoscopic IFS (formula ID 10).
// The uniform world scale is a similarity transform used only to keep this
// tiny reference scene in Metal-FPT's normal numerical range.
static float4 mandelbulberFieldSample(float3 p,
                                      constant FptRenderConfig &cfg,
                                      int iteration_multiplier) {
    int iteration_budget = mandelbulberScreenIterationBudget(
        p, cfg, iteration_multiplier);
    p = mandelbulberGlobalPoint(p, cfg);
    float world_scale = max(setv(cfg, 0), 1.0f);
    float3 scaled = p / world_scale;
    float3 z = float3(scaled.x, scaled.z, scaled.y);
    float derivative = 1.0f;
    float radius = length(z);
    int completed_iterations = 0;
    bool escaped = false;
    int max_iterations = iteration_budget;
    float bailout = max(setv(cfg, 2), 1.0f);
    float ifs_scale = max(setv(cfg, 3), 1.000001f);
    uint abs_mask = uint(round(setv(cfg, 4)));
    uint enabled_mask = uint(round(setv(cfg, 5)));
    float3 offset = float3(setv(cfg, 6), setv(cfg, 7), setv(cfg, 8));
    float3 rotation = float3(setv(cfg, 9), setv(cfg, 10), setv(cfg, 11));

    for (int iteration = 0; iteration < max_iterations; ++iteration) {
        if ((abs_mask & 1u) != 0u) z.x = abs(z.x);
        if ((abs_mask & 2u) != 0u) z.y = abs(z.y);
        if ((abs_mask & 4u) != 0u) z.z = abs(z.z);

        for (uint plane = 0u; plane < 9u; ++plane) {
            if ((enabled_mask & (1u << plane)) == 0u) continue;
            uint base = 12u + plane * 3u;
            float3 direction = float3(setv(cfg, base), setv(cfg, base + 1u),
                                      setv(cfg, base + 2u));
            float projection = dot(z, direction);
            if (projection < 0.0f) z -= direction * (2.0f * projection);
        }

        z = mandelbulberRotation3(z - offset, rotation) + offset;
        z = z * ifs_scale - offset * (ifs_scale - 1.0f);
        derivative *= abs(ifs_scale);
        radius = length(z);
        completed_iterations = iteration + 1;
        if (radius > bailout) {
            escaped = true;
            break;
        }
    }

    float distance = (radius - 2.0f) /
        max(abs(derivative), 1.0e-30f) * world_scale;
    float iteration_state = escaped
        ? -float(completed_iterations)
        : float(completed_iterations);
    return float4(distance, radius, derivative, iteration_state);
}

#else
static float4 mandelbulberFieldSample(float3 p,
                                      constant FptRenderConfig &cfg,
                                      int iteration_multiplier) {
    int iteration_budget = cfg.vset_values[130] > 0.0f
        ? mandelbulberScreenIterationBudget(p, cfg, iteration_multiplier)
        : 0;
    return mandelbulberGeneratedFieldSample(
        mandelbulberGlobalPoint(p, cfg), cfg, iteration_multiplier,
        iteration_budget);
}
#endif

static float mandelbulberNormalDistance(float3 p,
                                        constant FptRenderConfig &cfg) {
    int iteration_multiplier = cfg.vset_values[115] > 1.5f ? 5 : 1;
    return mandelbulberFieldSample(p, cfg, iteration_multiplier).x;
}

static float mandelbulberNormalOrbitRadius(float3 p,
                                           constant FptRenderConfig &cfg) {
    int iteration_multiplier = cfg.vset_values[115] > 1.5f ? 5 : 1;
    float radius = mandelbulberFieldSample(p, cfg, iteration_multiplier).y;
    return log(max(abs(radius), 1.0e-30f));
}

static DeResult deMandelbulber(float3 p,
                              constant FptRenderConfig &cfg) {
    float4 sample = mandelbulberFieldSample(p, cfg, 1);
    DeResult result;
    result.d = sample.x;
    // Mandelbulber's iteration-threshold mode treats points which reach the
    // configured iteration limit as interior. Escaped points are kept just
    // outside the current pixel-sized threshold so they cannot become false
    // hits as the threshold grows with camera distance.
    if (cfg.vset_values[115] > 1.5f) {
        float threshold = clamp(
            length(cameraPos(cfg) - p) * cfg.vset_values[116],
            cfg.vset_values[118], cfg.vset_values[119]);
        bool reached_iteration_limit = sample.w > 0.0f;
        if (reached_iteration_limit) {
            result.d = 0.0f;
        } else if (result.d < threshold) {
            result.d = threshold * 1.01f;
        }
    }
    result.orbit = abs(sample.w) / max(setv(cfg, 1), 1.0f);
    return result;
}

kernel void mandelbulber_field_sample_kernel(
    device const float4 *points [[buffer(0)]],
    device float4 *samples [[buffer(1)]],
    constant FptRenderConfig &cfg [[buffer(2)]],
    uint gid [[thread_position_in_grid]]) {
    float4 sample = mandelbulberFieldSample(points[gid].xyz, cfg, 1);
    sample.w = abs(sample.w);
    samples[gid] = sample;
}

static DeResult deTower(float3 p, constant FptRenderConfig &cfg) {
    float sdf = 0.0f;
    float col = 0.0f;
    float3 p0 = p;
    p = mod3(p, 2.0f) - 1.0f;
    p = abs(p) - 1.0f;
    if (p.x < p.z) p.xz = p.zx;
    if (p.y < p.z) p.yz = p.zy;
    if (p.x < p.y) p.xy = p.yx;
    float s = 1.0f;
    float3 tower_shift = float3(0.6f * (setv(cfg, 5) * 2.0f + 1.0f),
                                0.6f * (setv(cfg, 4) * 2.0f + 1.0f),
                                3.5f * (setv(cfg, 3) * 2.0f + 1.0f));
    for (int i = 0; i < 14; i++) {
        float r2 = 2.0f / clamp(dot(p, p), 0.1f, 1.0f);
        p = abs(p) * r2 - tower_shift;
        s *= r2;
        sdf = length(p) / s;
        col += sdf;
    }
    DeResult r;
    r.d = max(sdf, length(p0) - setv(cfg, 2));
    r.orbit = col;
    return r;
}

static DeResult deTree(float3 p, constant FptRenderConfig &cfg) {
    float orbit = 0.0f;
    float sdf = 0.0f;
    float scale = 1.9f;
    float angle1 = -9.83f + setv(cfg, 2) / 2.0f;
    float angle2 = -1.16f + setv(cfg, 3) / 2.0f;
    float3 shift = float3(-3.508f, -3.593f, 3.295f) + float3(setv(cfg, 4), setv(cfg, 5), setv(cfg, 6)) * 7.0f;
    float s = 1.0f;
    for (int i = 0; i < 20; ++i) {
        p = abs(p);
        p.xy = rot2(p.xy, -angle1);
        p.xy += min(p.x - p.y, 0.0f) * float2(-1.0f, 1.0f);
        p.xz += min(p.x - p.z, 0.0f) * float2(-1.0f, 1.0f);
        p.yz += min(p.y - p.z, 0.0f) * float2(-1.0f, 1.0f);
        p.yz = rot2(p.yz, -angle2);
        p *= scale;
        s *= scale;
        p += shift;
        float3 d = abs(p) - float3(6.0f);
        sdf = (min(max(d.x, max(d.y, d.z)), 0.0f) + length(max(d, float3(0.0f)))) / s;
        orbit += sdf;
    }
    DeResult r;
    r.d = sdf;
    r.orbit = orbit;
    return r;
}

static float combineProgramDistance(float current, float candidate, uint mode) {
    if (mode == 1u) return max(current, candidate);
    if (mode == 2u) return max(current, -candidate);
    return min(current, candidate);
}

static ProgramBounds unknownProgramBounds() {
    ProgramBounds bounds;
    bounds.minimum = -inf;
    bounds.maximum = inf;
    bounds.lipschitz = inf;
    bounds.certified = 0u;
    return bounds;
}

static ProgramBounds combineProgramBounds(ProgramBounds a, ProgramBounds b, uint mode) {
    if (a.certified == 0u || b.certified == 0u) return unknownProgramBounds();
    ProgramBounds result;
    if (mode == 1u) {
        result.minimum = max(a.minimum, b.minimum);
        result.maximum = max(a.maximum, b.maximum);
    } else if (mode == 2u) {
        result.minimum = max(a.minimum, -b.maximum);
        result.maximum = max(a.maximum, -b.minimum);
    } else {
        result.minimum = min(a.minimum, b.minimum);
        result.maximum = min(a.maximum, b.maximum);
    }
    result.lipschitz = max(a.lipschitz, b.lipschitz);
    result.certified = 1u;
    return result;
}

static void rotateProgramAabb(thread float3 &center,
                              thread float3 &half_extent,
                              uint axis,
                              float angle) {
    float s = sin(angle);
    float c = cos(angle);
    float ac = abs(c);
    float as = abs(s);
    if (axis == 0u) {
        center.yz = rot2(center.yz, angle);
        float ey = half_extent.y;
        float ez = half_extent.z;
        half_extent.y = ac * ey + as * ez;
        half_extent.z = as * ey + ac * ez;
    } else if (axis == 1u) {
        center.xz = rot2(center.xz, angle);
        float ex = half_extent.x;
        float ez = half_extent.z;
        half_extent.x = ac * ex + as * ez;
        half_extent.z = as * ex + ac * ez;
    } else {
        center.xy = rot2(center.xy, angle);
        float ex = half_extent.x;
        float ey = half_extent.y;
        half_extent.x = ac * ex + as * ey;
        half_extent.y = as * ex + ac * ey;
    }
}

static float3 sortProgramVectorDescending(float3 value) {
    if (value.x < value.z) value.xz = value.zx;
    if (value.y < value.z) value.yz = value.zy;
    if (value.x < value.y) value.xy = value.yx;
    return value;
}

static ProgramBounds programBounds(float3 source_center,
                                   float3 source_half_extent,
                                   constant FptRenderConfig &cfg) {
    float3 center = source_center;
    float3 half_extent = abs(source_half_extent);
    float distance_scale = 1.0f;
    ProgramBounds accumulated = unknownProgramBounds();
    bool has_primitive = false;
    bool certified = true;
    uint count = min(cfg.sdf_program_count, FPT_SDF_PROGRAM_MAX_OPS);
    for (uint i = 0u; i < count; ++i) {
        const FptSdfInstruction op = cfg.sdf_program[i];
        float4 data = float4(op.data[0], op.data[1], op.data[2], op.data[3]);
        switch (op.opcode) {
            case SDF_OP_ABS: {
                float3 lower = center - half_extent;
                float3 upper = center + half_extent;
                float3 abs_min = select(min(abs(lower), abs(upper)), float3(0.0f),
                                        (lower <= 0.0f) & (upper >= 0.0f));
                float3 abs_max = max(abs(lower), abs(upper));
                center = (abs_min + abs_max) * 0.5f;
                half_extent = (abs_max - abs_min) * 0.5f;
                break;
            }
            case SDF_OP_TRANSLATE:
                center -= data.xyz;
                break;
            case SDF_OP_SCALE: {
                float s = abs(data.x) > 1.0e-6f ? data.x : 1.0f;
                center *= s;
                half_extent *= abs(s);
                distance_scale *= abs(s);
                break;
            }
            case SDF_OP_ROTATE_X:
                rotateProgramAabb(center, half_extent, 0u, data.x);
                break;
            case SDF_OP_ROTATE_Y:
                rotateProgramAabb(center, half_extent, 1u, data.x);
                break;
            case SDF_OP_ROTATE_Z:
                rotateProgramAabb(center, half_extent, 2u, data.x);
                break;
            case SDF_OP_REPEAT: {
                float3 period = max(abs(data.xyz), float3(1.0e-5f));
                float3 lower_cell = floor((center - half_extent) / period + 0.5f);
                float3 upper_cell = floor((center + half_extent) / period + 0.5f);
                if (any(lower_cell != upper_cell)) certified = false;
                center -= period * floor(center / period + 0.5f);
                break;
            }
            case SDF_OP_SORT_DESC: {
                // Every order statistic is monotone in each input component.
                // Sorting the component-wise lower and upper corners therefore
                // gives the exact interval hull of the sorted AABB.
                float3 lower = sortProgramVectorDescending(center - half_extent);
                float3 upper = sortProgramVectorDescending(center + half_extent);
                center = (lower + upper) * 0.5f;
                half_extent = max((upper - lower) * 0.5f, float3(0.0f));
                break;
            }
            case SDF_OP_SPHERE: {
                ProgramBounds primitive;
                float divisor = max(distance_scale, 1.0e-6f);
                primitive.minimum = (length(max(abs(center) - half_extent, float3(0.0f))) - data.x) / divisor;
                primitive.maximum = (length(abs(center) + half_extent) - data.x) / divisor;
                primitive.lipschitz = 1.0f;
                primitive.certified = certified ? 1u : 0u;
                accumulated = has_primitive
                    ? combineProgramBounds(accumulated, primitive, op.flags)
                    : primitive;
                has_primitive = true;
                break;
            }
            case SDF_OP_BOX: {
                ProgramBounds primitive;
                float divisor = max(distance_scale, 1.0e-6f);
                float center_distance = sdBox(center, abs(data.xyz)) / divisor;
                float radius = length(half_extent) / divisor;
                primitive.minimum = center_distance - radius;
                primitive.maximum = center_distance + radius;
                primitive.lipschitz = 1.0f;
                primitive.certified = certified ? 1u : 0u;
                accumulated = has_primitive
                    ? combineProgramBounds(accumulated, primitive, op.flags)
                    : primitive;
                has_primitive = true;
                break;
            }
            case SDF_OP_PLANE: {
                ProgramBounds primitive;
                float normal_length = length(data.xyz);
                float divisor = max(distance_scale, 1.0e-6f);
                if (normal_length <= 1.0e-8f) {
                    primitive = unknownProgramBounds();
                } else {
                    float3 normal = data.xyz / normal_length;
                    float midpoint = (dot(center, normal) + data.w) / divisor;
                    float radius = dot(half_extent, abs(normal)) / divisor;
                    primitive.minimum = midpoint - radius;
                    primitive.maximum = midpoint + radius;
                    primitive.lipschitz = 1.0f;
                    primitive.certified = certified ? 1u : 0u;
                }
                accumulated = has_primitive
                    ? combineProgramBounds(accumulated, primitive, op.flags)
                    : primitive;
                has_primitive = true;
                break;
            }
            case SDF_OP_ORBIT_ADD:
            case SDF_OP_UNION:
            case SDF_OP_INTERSECTION:
            case SDF_OP_SUBTRACT:
                break;
            default:
                certified = false;
                break;
        }
    }
    if (!has_primitive || !certified || accumulated.certified == 0u ||
        !isfinite(accumulated.minimum) || !isfinite(accumulated.maximum)) {
        return unknownProgramBounds();
    }
    // Interval operations are evaluated in float, so reserve an explicit
    // outward error allowance before exposing a cell as certified. The term
    // scales with program length and transformed-coordinate magnitude; the
    // fixed floor covers transcendental and short-program rounding.
    constexpr float float_epsilon = 1.1920929e-7f;
    float magnitude = max(abs(accumulated.minimum), abs(accumulated.maximum));
    float arithmetic_scale = 1.0f + magnitude + length(center) +
                             length(half_extent) + abs(distance_scale);
    float rounding_allowance = arithmetic_scale * float(count + 1u) *
                               8.0f * float_epsilon;
    float outward_allowance = max(2.0e-5f * (1.0f + magnitude),
                                  rounding_allowance);
    accumulated.minimum = floatRoundDown(accumulated.minimum - outward_allowance);
    accumulated.maximum = floatRoundUp(accumulated.maximum + outward_allowance);
    return accumulated;
}

static ulong programDistanceInstructionMask(constant FptRenderConfig &cfg) {
    ulong mask = 0ul;
    uint count = min(cfg.sdf_program_count, FPT_SDF_PROGRAM_MAX_OPS);
    for (uint i = 0u; i < count; ++i) {
        // Orbit-only instructions cannot affect marching distance.
        if (cfg.sdf_program[i].opcode != SDF_OP_ORBIT_ADD) mask |= 1ul << i;
    }
    return mask;
}

enum RegionalProgramProofFlag : uint {
    RegionalProofStrictDominance = 1u << 0u,
    RegionalProofRepeatSeamFallback = 1u << 1u,
    RegionalProofUnsupportedFallback = 1u << 2u,
    RegionalProofInvalidPrimitiveFallback = 1u << 3u,
    RegionalProofNoPrimitiveFallback = 1u << 4u,
    RegionalProofNoDominance = 1u << 5u,
};

struct RegionalProgramDecision {
    ulong instruction_mask;
    uint proof_flags;
    uint pruned_primitives;
};

// The residual-program atlas uses a 64-bit program signature as the cell's
// initial identifier. Only hard-union primitive branches are eligible: strict
// interval dominance proves that a removed branch can never win anywhere in
// the cell. Every failure to establish that proof retains the complete
// distance program and records why in profiling builds.
static RegionalProgramDecision regionalProgramDecision(
    float3 source_center,
    float3 source_half_extent,
    constant FptRenderConfig &cfg) {
    ulong full_mask = programDistanceInstructionMask(cfg);
    RegionalProgramDecision decision = {full_mask, 0u, 0u};
    if (cfg.sdf_id != SDF_PROGRAM || cfg.sdf_program_count == 0u) {
        decision.proof_flags = RegionalProofUnsupportedFallback;
        return decision;
    }

    float3 center = source_center;
    float3 half_extent = abs(source_half_extent);
    float distance_scale = 1.0f;
    float minimums[FPT_SDF_PROGRAM_MAX_OPS];
    float maximums[FPT_SDF_PROGRAM_MAX_OPS];
    uint primitive_indices[FPT_SDF_PROGRAM_MAX_OPS];
    uint primitive_count = 0u;
    bool certified = true;
    uint count = min(cfg.sdf_program_count, FPT_SDF_PROGRAM_MAX_OPS);
    for (uint i = 0u; i < count; ++i) {
        const FptSdfInstruction op = cfg.sdf_program[i];
        float4 data = float4(op.data[0], op.data[1], op.data[2], op.data[3]);
        switch (op.opcode) {
            case SDF_OP_ABS: {
                float3 lower = center - half_extent;
                float3 upper = center + half_extent;
                float3 abs_min = select(min(abs(lower), abs(upper)), float3(0.0f),
                                        (lower <= 0.0f) & (upper >= 0.0f));
                float3 abs_max = max(abs(lower), abs(upper));
                center = (abs_min + abs_max) * 0.5f;
                half_extent = (abs_max - abs_min) * 0.5f;
                break;
            }
            case SDF_OP_TRANSLATE:
                center -= data.xyz;
                break;
            case SDF_OP_SCALE: {
                float scale = abs(data.x) > 1.0e-6f ? data.x : 1.0f;
                center *= scale;
                half_extent *= abs(scale);
                distance_scale *= abs(scale);
                break;
            }
            case SDF_OP_ROTATE_X:
                rotateProgramAabb(center, half_extent, 0u, data.x);
                break;
            case SDF_OP_ROTATE_Y:
                rotateProgramAabb(center, half_extent, 1u, data.x);
                break;
            case SDF_OP_ROTATE_Z:
                rotateProgramAabb(center, half_extent, 2u, data.x);
                break;
            case SDF_OP_REPEAT: {
                float3 period = max(abs(data.xyz), float3(1.0e-5f));
                float3 lower_cell = floor((center - half_extent) / period + 0.5f);
                float3 upper_cell = floor((center + half_extent) / period + 0.5f);
                if (any(lower_cell != upper_cell)) {
                    certified = false;
                    decision.proof_flags |= RegionalProofRepeatSeamFallback;
                }
                center -= period * floor(center / period + 0.5f);
                break;
            }
            case SDF_OP_SORT_DESC: {
                float3 lower = sortProgramVectorDescending(center - half_extent);
                float3 upper = sortProgramVectorDescending(center + half_extent);
                center = (lower + upper) * 0.5f;
                half_extent = max((upper - lower) * 0.5f, float3(0.0f));
                break;
            }
            case SDF_OP_SPHERE:
            case SDF_OP_BOX:
            case SDF_OP_PLANE: {
                if (op.flags != 0u) {
                    certified = false;
                    decision.proof_flags |= RegionalProofUnsupportedFallback;
                }
                float divisor = max(distance_scale, 1.0e-6f);
                float lower;
                float upper;
                if (op.opcode == SDF_OP_SPHERE) {
                    lower = (length(max(abs(center) - half_extent, float3(0.0f))) -
                             data.x) / divisor;
                    upper = (length(abs(center) + half_extent) - data.x) / divisor;
                } else if (op.opcode == SDF_OP_BOX) {
                    float midpoint = sdBox(center, abs(data.xyz)) / divisor;
                    float radius = length(half_extent) / divisor;
                    lower = midpoint - radius;
                    upper = midpoint + radius;
                } else {
                    float normal_length = length(data.xyz);
                    if (!(normal_length > 1.0e-8f)) {
                        certified = false;
                        decision.proof_flags |=
                            RegionalProofInvalidPrimitiveFallback;
                        lower = -inf;
                        upper = inf;
                    } else {
                        float3 normal = data.xyz / normal_length;
                        float midpoint = (dot(center, normal) + data.w) / divisor;
                        float radius = dot(half_extent, abs(normal)) / divisor;
                        lower = midpoint - radius;
                        upper = midpoint + radius;
                    }
                }
                float magnitude = max(abs(lower), abs(upper));
                float allowance = 1.0e-4f * (1.0f + magnitude + length(center) +
                                             length(half_extent));
                minimums[primitive_count] = floatRoundDown(lower - allowance);
                maximums[primitive_count] = floatRoundUp(upper + allowance);
                primitive_indices[primitive_count] = i;
                primitive_count++;
                break;
            }
            case SDF_OP_ORBIT_ADD:
                break;
            default:
                certified = false;
                decision.proof_flags |= RegionalProofUnsupportedFallback;
                break;
        }
    }
    if (primitive_count == 0u) {
        decision.proof_flags |= RegionalProofNoPrimitiveFallback;
        return decision;
    }
    if (!certified) return decision;

    ulong mask = full_mask;
    uint retained = primitive_count;
    for (uint candidate = 0u; candidate < primitive_count; ++candidate) {
        for (uint winner = 0u; winner < primitive_count; ++winner) {
            if (candidate == winner) continue;
            if (minimums[candidate] > maximums[winner]) {
                mask &= ~(1ul << primitive_indices[candidate]);
                retained--;
                break;
            }
        }
    }
    if (retained == 0u) {
        decision.proof_flags |= RegionalProofNoDominance;
        return decision;
    }
    decision.instruction_mask = mask;
    decision.pruned_primitives = primitive_count - retained;
    decision.proof_flags |= decision.pruned_primitives > 0u
        ? RegionalProofStrictDominance : RegionalProofNoDominance;
    return decision;
}

struct ProgramInterval3 {
    float3 lower;
    float3 upper;
};

static float intervalSquareMinimum(float lower, float upper) {
    if (lower <= 0.0f && upper >= 0.0f) return 0.0f;
    return min(lower * lower, upper * upper);
}

static float intervalSquareMaximum(float lower, float upper) {
    return max(lower * lower, upper * upper);
}

static float2 intervalNormSquared(ProgramInterval3 value) {
    float minimum = intervalSquareMinimum(value.lower.x, value.upper.x) +
                    intervalSquareMinimum(value.lower.y, value.upper.y) +
                    intervalSquareMinimum(value.lower.z, value.upper.z);
    float maximum = intervalSquareMaximum(value.lower.x, value.upper.x) +
                    intervalSquareMaximum(value.lower.y, value.upper.y) +
                    intervalSquareMaximum(value.lower.z, value.upper.z);
    return float2(max(minimum, 0.0f), max(maximum, 0.0f));
}

static float2 multiplyInterval(float lower,
                               float upper,
                               float factor_lower,
                               float factor_upper) {
    float4 products = float4(lower * factor_lower, lower * factor_upper,
                             upper * factor_lower, upper * factor_upper);
    return float2(min(products.x, min(products.y, min(products.z, products.w))),
                  max(products.x, max(products.y, max(products.z, products.w))));
}

static void multiplyInterval3(thread ProgramInterval3 &value,
                              float factor_lower,
                              float factor_upper) {
    for (uint axis = 0u; axis < 3u; ++axis) {
        float2 product = multiplyInterval(value.lower[axis], value.upper[axis],
                                          factor_lower, factor_upper);
        value.lower[axis] = product.x;
        value.upper[axis] = product.y;
    }
}

static ProgramInterval3 exactProgramInterval3(float3 value) {
    ProgramInterval3 result = {value, value};
    return result;
}

static ProgramInterval3 linearCombineProgramInterval3(ProgramInterval3 a,
                                                       float a_factor,
                                                       ProgramInterval3 b,
                                                       float b_factor) {
    ProgramInterval3 result;
    for (uint axis = 0u; axis < 3u; ++axis) {
        float2 scaled_a = multiplyInterval(a.lower[axis], a.upper[axis],
                                           a_factor, a_factor);
        float2 scaled_b = multiplyInterval(b.lower[axis], b.upper[axis],
                                           b_factor, b_factor);
        result.lower[axis] = scaled_a.x + scaled_b.x;
        result.upper[axis] = scaled_a.y + scaled_b.y;
    }
    return result;
}

static void applyAbsoluteJacobianHull(thread ProgramInterval3 &row,
                                      float coordinate_lower,
                                      float coordinate_upper) {
    if (coordinate_lower > 0.0f) return;
    if (coordinate_upper < 0.0f) {
        multiplyInterval3(row, -1.0f, -1.0f);
        return;
    }
    float3 positive_lower = row.lower;
    float3 positive_upper = row.upper;
    row.lower = min(positive_lower, -positive_upper);
    row.upper = max(positive_upper, -positive_lower);
}

static ProgramDerivativeBounds unknownProgramDerivativeBounds() {
    ProgramDerivativeBounds result;
    result.lower = float3(-inf);
    result.upper = float3(inf);
    result.certified = 0u;
    return result;
}

static ProgramDerivativeBounds transformProgramDerivativeInterval(
    ProgramInterval3 local,
    ProgramInterval3 jx,
    ProgramInterval3 jy,
    ProgramInterval3 jz,
    float distance_scale) {
    ProgramDerivativeBounds result;
    result.lower = float3(0.0f);
    result.upper = float3(0.0f);
    float divisor = max(distance_scale, 1.0e-6f);
    for (uint world_axis = 0u; world_axis < 3u; ++world_axis) {
        for (uint local_axis = 0u; local_axis < 3u; ++local_axis) {
            ProgramInterval3 jacobian_row = local_axis == 0u
                ? jx : (local_axis == 1u ? jy : jz);
            float2 contribution = multiplyInterval(
                local.lower[local_axis], local.upper[local_axis],
                jacobian_row.lower[world_axis] / divisor,
                jacobian_row.upper[world_axis] / divisor);
            result.lower[world_axis] += contribution.x;
            result.upper[world_axis] += contribution.y;
        }
    }
    result.certified = all(isfinite(result.lower)) && all(isfinite(result.upper))
        ? 1u : 0u;
    return result.certified != 0u ? result : unknownProgramDerivativeBounds();
}

static ProgramDerivativeBounds combineProgramDerivativeBounds(
    ProgramDerivativeBounds accumulated,
    ProgramDerivativeBounds primitive,
    uint mode) {
    if (accumulated.certified == 0u || primitive.certified == 0u) {
        return unknownProgramDerivativeBounds();
    }
    if (mode == 2u) {
        float3 negated_lower = -primitive.upper;
        primitive.upper = -primitive.lower;
        primitive.lower = negated_lower;
    }
    ProgramDerivativeBounds result;
    // A hard CSG result selects one branch almost everywhere. The component-
    // wise hull of both branch gradients therefore contains every derivative,
    // including either limiting derivative at a branch switch.
    result.lower = min(accumulated.lower, primitive.lower);
    result.upper = max(accumulated.upper, primitive.upper);
    result.certified = 1u;
    return result;
}

static ProgramInterval3 sphereLocalDerivativeInterval(float3 center,
                                                       float3 half_extent) {
    ProgramInterval3 position = {center - abs(half_extent),
                                 center + abs(half_extent)};
    float2 norm_squared = intervalNormSquared(position);
    float minimum_norm = sqrt(max(norm_squared.x, 0.0f));
    float maximum_norm = sqrt(max(norm_squared.y, 0.0f));
    ProgramInterval3 gradient;
    if (!(minimum_norm > 1.0e-8f) || !(maximum_norm > 1.0e-8f)) {
        gradient.lower = float3(-1.0f);
        gradient.upper = float3(1.0f);
        return gradient;
    }
    for (uint axis = 0u; axis < 3u; ++axis) {
        float2 component = multiplyInterval(position.lower[axis],
                                            position.upper[axis],
                                            1.0f / maximum_norm,
                                            1.0f / minimum_norm);
        gradient.lower[axis] = max(component.x, -1.0f);
        gradient.upper[axis] = min(component.y, 1.0f);
    }
    return gradient;
}

// Certified signed gradient intervals for typed programs. Jacobian rows are
// intervals, allowing ABS folds to hull both signs and REPEAT to remain exact
// inside one tile. Repeat seams and unsupported nonlinear operations return
// Unknown and keep exact procedural rendering through the fallback path.
static ProgramDerivativeBounds programDerivativeBounds(
    float3 source_center,
    float3 source_half_extent,
    constant FptRenderConfig &cfg) {
    float3 center = source_center;
    float3 half_extent = abs(source_half_extent);
    ProgramInterval3 jx = exactProgramInterval3(float3(1.0f, 0.0f, 0.0f));
    ProgramInterval3 jy = exactProgramInterval3(float3(0.0f, 1.0f, 0.0f));
    ProgramInterval3 jz = exactProgramInterval3(float3(0.0f, 0.0f, 1.0f));
    float distance_scale = 1.0f;
    ProgramDerivativeBounds accumulated = unknownProgramDerivativeBounds();
    bool has_primitive = false;
    bool certified = true;
    uint count = min(cfg.sdf_program_count, FPT_SDF_PROGRAM_MAX_OPS);
    for (uint i = 0u; i < count; ++i) {
        const FptSdfInstruction op = cfg.sdf_program[i];
        float4 data = float4(op.data[0], op.data[1], op.data[2], op.data[3]);
        switch (op.opcode) {
            case SDF_OP_ABS: {
                float3 lower = center - half_extent;
                float3 upper = center + half_extent;
                applyAbsoluteJacobianHull(jx, lower.x, upper.x);
                applyAbsoluteJacobianHull(jy, lower.y, upper.y);
                applyAbsoluteJacobianHull(jz, lower.z, upper.z);
                float3 abs_min = select(min(abs(lower), abs(upper)), float3(0.0f),
                                        (lower <= 0.0f) & (upper >= 0.0f));
                float3 abs_max = max(abs(lower), abs(upper));
                center = (abs_min + abs_max) * 0.5f;
                half_extent = (abs_max - abs_min) * 0.5f;
                break;
            }
            case SDF_OP_TRANSLATE:
                center -= data.xyz;
                break;
            case SDF_OP_SCALE: {
                float s = abs(data.x) > 1.0e-6f ? data.x : 1.0f;
                center *= s;
                half_extent *= abs(s);
                multiplyInterval3(jx, s, s);
                multiplyInterval3(jy, s, s);
                multiplyInterval3(jz, s, s);
                distance_scale *= abs(s);
                break;
            }
            case SDF_OP_ROTATE_X: {
                rotateProgramAabb(center, half_extent, 0u, data.x);
                float s = sin(data.x), c = cos(data.x);
                ProgramInterval3 old_jy = jy;
                ProgramInterval3 old_jz = jz;
                jy = linearCombineProgramInterval3(old_jy, c, old_jz, -s);
                jz = linearCombineProgramInterval3(old_jy, s, old_jz, c);
                break;
            }
            case SDF_OP_ROTATE_Y: {
                rotateProgramAabb(center, half_extent, 1u, data.x);
                float s = sin(data.x), c = cos(data.x);
                ProgramInterval3 old_jx = jx;
                ProgramInterval3 old_jz = jz;
                jx = linearCombineProgramInterval3(old_jx, c, old_jz, -s);
                jz = linearCombineProgramInterval3(old_jx, s, old_jz, c);
                break;
            }
            case SDF_OP_ROTATE_Z: {
                rotateProgramAabb(center, half_extent, 2u, data.x);
                float s = sin(data.x), c = cos(data.x);
                ProgramInterval3 old_jx = jx;
                ProgramInterval3 old_jy = jy;
                jx = linearCombineProgramInterval3(old_jx, c, old_jy, -s);
                jy = linearCombineProgramInterval3(old_jx, s, old_jy, c);
                break;
            }
            case SDF_OP_REPEAT: {
                float3 period = max(abs(data.xyz), float3(1.0e-5f));
                float3 lower_cell = floor((center - half_extent) / period + 0.5f);
                float3 upper_cell = floor((center + half_extent) / period + 0.5f);
                if (any(lower_cell != upper_cell)) certified = false;
                center -= period * floor(center / period + 0.5f);
                break;
            }
            case SDF_OP_SORT_DESC:
                certified = false;
                break;
            case SDF_OP_SPHERE:
            case SDF_OP_BOX:
            case SDF_OP_PLANE: {
                ProgramInterval3 local_gradient;
                if (op.opcode == SDF_OP_SPHERE) {
                    local_gradient = sphereLocalDerivativeInterval(center,
                                                                   half_extent);
                } else if (op.opcode == SDF_OP_BOX) {
                    // Every branch of the exact box SDF has components in
                    // [-1,1]. This broad interval is conservative at edges and
                    // corners where the derivative is not unique.
                    local_gradient.lower = float3(-1.0f);
                    local_gradient.upper = float3(1.0f);
                } else {
                    float normal_length = length(data.xyz);
                    if (!(normal_length > 1.0e-8f)) {
                        certified = false;
                        local_gradient.lower = float3(-inf);
                        local_gradient.upper = float3(inf);
                    } else {
                        float3 normal = data.xyz / normal_length;
                        local_gradient.lower = normal;
                        local_gradient.upper = normal;
                    }
                }
                ProgramDerivativeBounds primitive = certified
                    ? transformProgramDerivativeInterval(local_gradient, jx, jy,
                                                         jz, distance_scale)
                    : unknownProgramDerivativeBounds();
                accumulated = has_primitive
                    ? combineProgramDerivativeBounds(accumulated, primitive,
                                                     op.flags)
                    : primitive;
                has_primitive = true;
                break;
            }
            case SDF_OP_ORBIT_ADD:
            case SDF_OP_UNION:
            case SDF_OP_INTERSECTION:
            case SDF_OP_SUBTRACT:
                break;
            default:
                certified = false;
                break;
        }
    }
    if (!has_primitive || !certified || accumulated.certified == 0u ||
        any(!isfinite(accumulated.lower)) || any(!isfinite(accumulated.upper))) {
        return unknownProgramDerivativeBounds();
    }
    constexpr float float_epsilon = 1.1920929e-7f;
    float3 magnitude = max(abs(accumulated.lower), abs(accumulated.upper));
    float coordinate_scale = 1.0f + length(center) + length(half_extent) +
                             abs(distance_scale);
    float3 rounding_allowance = coordinate_scale * float(count + 1u) *
                                12.0f * float_epsilon * (1.0f + magnitude);
    float3 outward_allowance = max(5.0e-5f * (1.0f + magnitude),
                                   rounding_allowance);
    accumulated.lower -= outward_allowance;
    accumulated.upper += outward_allowance;
    return accumulated;
}

static float2 repeatCageInterval(float lower, float upper) {
    constexpr float period = 8.0f;
    constexpr float half_period = 4.0f;
    if (!isfinite(lower) || !isfinite(upper) || upper - lower >= period) {
        lower = -half_period;
        upper = half_period;
        return float2(lower, upper);
    }
    float lower_cell = floor((lower + half_period) / period);
    float upper_cell = floor((upper + half_period) / period);
    if (lower_cell != upper_cell) {
        lower = -half_period;
        upper = half_period;
        return float2(lower, upper);
    }
    lower -= lower_cell * period;
    upper -= upper_cell * period;
    lower = max(lower, -half_period);
    upper = min(upper, half_period);
    return float2(lower, upper);
}

// Conservative interval propagation through the built-in Cage estimator. It
// mirrors deCage's homogeneous inversions, folds and repeats. Singular source
// or loop intervals return Unknown instead of risking a false empty-space skip.
static ProgramBounds cageProgramBounds(float3 source_center,
                                       float3 source_half_extent,
                                       constant FptRenderConfig &cfg) {
    ProgramInterval3 source = {source_center - abs(source_half_extent),
                               source_center + abs(source_half_extent)};
    ProgramInterval3 p = source;
    float2 source_norm_squared = intervalNormSquared(p);
    float denominator_lower = min(source_norm_squared.x, 30.0f);
    float denominator_upper = min(source_norm_squared.y, 30.0f);
    if (!(denominator_lower > 1.0e-8f) || !isfinite(denominator_upper)) {
        return unknownProgramBounds();
    }
    float factor_lower = 2.0f / denominator_upper;
    float factor_upper = 2.0f / denominator_lower;
    multiplyInterval3(p, factor_lower, factor_upper);
    float w_lower = 3.0f * factor_lower;
    float w_upper = 3.0f * factor_upper;

    float3 cage_fold = float3(1.0f + setv(cfg, 2) * 4.0f,
                              12.0f + (setv(cfg, 3) * 4.0f) / 3.0f,
                              2.0f + setv(cfg, 4) * 4.0f);
    float3 fold_constant = float3(2.0f, 4.0f, 2.0f) + cage_fold;
    for (uint iteration = 0u; iteration < 12u; ++iteration) {
        float3 abs_min = select(min(abs(p.lower), abs(p.upper)), float3(0.0f),
                                (p.lower <= 0.0f) & (p.upper >= 0.0f));
        float3 abs_max = max(abs(p.lower), abs(p.upper));
        p.lower = fold_constant - abs_max;
        p.upper = fold_constant - abs_min;
        for (uint axis = 0u; axis < 3u; ++axis) {
            float2 repeated = repeatCageInterval(p.lower[axis], p.upper[axis]);
            p.lower[axis] = repeated.x;
            p.upper[axis] = repeated.y;
        }

        float2 norm_squared = intervalNormSquared(p);
        denominator_lower = min(norm_squared.x, 12.0f);
        denominator_upper = min(norm_squared.y, 12.0f);
        if (!(denominator_lower > 1.0e-8f) || !isfinite(denominator_upper)) {
            return unknownProgramBounds();
        }
        factor_lower = 9.0f / denominator_upper;
        factor_upper = 9.0f / denominator_lower;
        multiplyInterval3(p, factor_lower, factor_upper);
        float2 next_w = multiplyInterval(w_lower, w_upper,
                                         factor_lower, factor_upper);
        w_lower = next_w.x;
        w_upper = next_w.y;
        if (!(w_lower > 1.0e-12f) || !isfinite(w_upper) ||
            any(!isfinite(p.lower)) || any(!isfinite(p.upper))) {
            return unknownProgramBounds();
        }
    }

    float3 abs_min = select(min(abs(p.lower), abs(p.upper)), float3(0.0f),
                            (p.lower <= 0.0f) & (p.upper >= 0.0f));
    float3 abs_max = max(abs(p.lower), abs(p.upper));
    float numerator_minimum = length(max(abs_min - 1.2f, float3(0.0f)));
    float numerator_maximum = length(max(abs_max - 1.2f, float3(0.0f)));
    float cage_minimum = numerator_minimum / w_upper;
    float cage_maximum = numerator_maximum / w_lower;

    float3 source_abs_min = max(abs(source_center) - abs(source_half_extent),
                                float3(0.0f));
    float sphere_minimum = length(source_abs_min) - setv(cfg, 5);
    float sphere_maximum = length(abs(source_center) + abs(source_half_extent)) -
                           setv(cfg, 5);
    ProgramBounds result;
    result.minimum = max(cage_minimum, sphere_minimum);
    result.maximum = max(cage_maximum, sphere_maximum);
    float magnitude = max(abs(result.minimum), abs(result.maximum));
    float outward_padding = 1.0e-4f * (1.0f + magnitude);
    result.minimum -= outward_padding;
    result.maximum += outward_padding;
    result.lipschitz = inf;
    result.certified = isfinite(result.minimum) && isfinite(result.maximum) ? 1u : 0u;
    return result.certified != 0u ? result : unknownProgramBounds();
}

static ProgramBounds accelerationProgramBounds(float3 center,
                                               float3 half_extent,
                                               constant FptRenderConfig &cfg) {
    if (cfg.sdf_id == SDF_PROGRAM && cfg.sdf_program_count > 0u) {
        return programBounds(center, half_extent, cfg);
    }
    if (cfg.sdf_id == SDF_CAGE_FRACTAL && cfg.bound_grid_cage_bounds != 0u) {
        return cageProgramBounds(center, half_extent, cfg);
    }
    return unknownProgramBounds();
}

static DeResult deShadingProgram(float3 source,
                                 constant FptRenderConfig &cfg) {
    float3 p = source;
    float distance_scale = 1.0f;
    float distance = inf;
    float orbit = 0.0f;
    bool has_primitive = false;
    uint count = min(cfg.sdf_shading_program_count,
                     FPT_SDF_PROGRAM_MAX_OPS);
    for (uint i = 0u; i < count; ++i) {
        const FptSdfInstruction op = cfg.sdf_shading_program[i];
        float4 data = float4(op.data[0], op.data[1], op.data[2], op.data[3]);
        switch (op.opcode) {
            case SDF_OP_ABS:
                p = abs(p);
                break;
            case SDF_OP_TRANSLATE:
                p -= data.xyz;
                break;
            case SDF_OP_SCALE: {
                float s = abs(data.x) > 1.0e-6f ? data.x : 1.0f;
                p *= s;
                distance_scale *= abs(s);
                break;
            }
            case SDF_OP_ROTATE_X:
                p.yz = rot2(p.yz, data.x);
                break;
            case SDF_OP_ROTATE_Y:
                p.xz = rot2(p.xz, data.x);
                break;
            case SDF_OP_ROTATE_Z:
                p.xy = rot2(p.xy, data.x);
                break;
            case SDF_OP_REPEAT: {
                float3 period = max(abs(data.xyz), float3(1.0e-5f));
                p = p - period * floor(p / period + 0.5f);
                break;
            }
            case SDF_OP_SORT_DESC:
                if (p.x < p.z) p.xz = p.zx;
                if (p.y < p.z) p.yz = p.zy;
                if (p.x < p.y) p.xy = p.yx;
                break;
            case SDF_OP_SPHERE: {
                float candidate = (length(p) - data.x) / max(distance_scale, 1.0e-6f);
                distance = has_primitive ? combineProgramDistance(distance, candidate, op.flags) : candidate;
                has_primitive = true;
                orbit += abs(candidate) * data.w;
                break;
            }
            case SDF_OP_BOX: {
                float3 q = abs(p) - abs(data.xyz);
                float candidate = (min(max(q.x, max(q.y, q.z)), 0.0f) + length(max(q, float3(0.0f)))) / max(distance_scale, 1.0e-6f);
                distance = has_primitive ? combineProgramDistance(distance, candidate, op.flags) : candidate;
                has_primitive = true;
                orbit += abs(candidate) * data.w;
                break;
            }
            case SDF_OP_PLANE: {
                float3 n = normalize(data.xyz);
                float candidate = (dot(p, n) + data.w) / max(distance_scale, 1.0e-6f);
                distance = has_primitive ? combineProgramDistance(distance, candidate, op.flags) : candidate;
                has_primitive = true;
                break;
            }
            case SDF_OP_ORBIT_ADD:
                orbit += length(p * data.xyz) * data.w;
                break;
            default:
                break;
        }
    }
    DeResult result;
    result.d = distance;
    result.orbit = orbit;
    return result;
}

static float deProgramDistance(float3 source, constant FptRenderConfig &cfg) {
    float3 p = source;
    float distance_scale = 1.0f;
    float distance = inf;
    bool has_primitive = false;
    uint count = min(cfg.sdf_program_count, FPT_SDF_PROGRAM_MAX_OPS);
    for (uint i = 0u; i < count; ++i) {
        const FptSdfInstruction op = cfg.sdf_program[i];
        float4 data = float4(op.data[0], op.data[1], op.data[2], op.data[3]);
        switch (op.opcode) {
            case SDF_OP_ABS:
                p = abs(p);
                break;
            case SDF_OP_TRANSLATE:
                p -= data.xyz;
                break;
            case SDF_OP_SCALE: {
                float scale = abs(data.x) > 1.0e-6f ? data.x : 1.0f;
                p *= scale;
                distance_scale *= abs(scale);
                break;
            }
            case SDF_OP_ROTATE_X:
                p.yz = rot2(p.yz, data.x);
                break;
            case SDF_OP_ROTATE_Y:
                p.xz = rot2(p.xz, data.x);
                break;
            case SDF_OP_ROTATE_Z:
                p.xy = rot2(p.xy, data.x);
                break;
            case SDF_OP_REPEAT: {
                float3 period = max(abs(data.xyz), float3(1.0e-5f));
                p -= period * floor(p / period + 0.5f);
                break;
            }
            case SDF_OP_SORT_DESC:
                if (p.x < p.z) p.xz = p.zx;
                if (p.y < p.z) p.yz = p.zy;
                if (p.x < p.y) p.xy = p.yx;
                break;
            case SDF_OP_SPHERE: {
                float candidate = (length(p) - data.x) /
                                  max(distance_scale, 1.0e-6f);
                distance = has_primitive
                    ? combineProgramDistance(distance, candidate, op.flags)
                    : candidate;
                has_primitive = true;
                break;
            }
            case SDF_OP_BOX: {
                float3 q = abs(p) - abs(data.xyz);
                float candidate = (min(max(q.x, max(q.y, q.z)), 0.0f) +
                                   length(max(q, float3(0.0f)))) /
                                  max(distance_scale, 1.0e-6f);
                distance = has_primitive
                    ? combineProgramDistance(distance, candidate, op.flags)
                    : candidate;
                has_primitive = true;
                break;
            }
            case SDF_OP_PLANE: {
                float candidate = (dot(p, normalize(data.xyz)) + data.w) /
                                  max(distance_scale, 1.0e-6f);
                distance = has_primitive
                    ? combineProgramDistance(distance, candidate, op.flags)
                    : candidate;
                has_primitive = true;
                break;
            }
            default:
                break;
        }
    }
    return distance;
}

// The host replaces this body when topology specialization is requested.
// Scene parameters remain in cfg.sdf_program; only control flow is generated.
#if defined(FPT_STITCH_HOST)
[[visible]] extern float deTopologyStitchedDistance(
    float3 source,
    constant FptRenderConfig &cfg);
[[visible]] extern ProgramSurface deTopologyStitchedSurface(
    float3 source,
    constant FptRenderConfig &cfg);
#else
static float deTopologyStitchedDistance(
    float3 source,
    constant FptRenderConfig &cfg) {
    return deProgramDistance(source, cfg);
}
#endif

static float flatUnionPrimitiveDistance(
    float3 source,
    const FptPrimitiveInstance primitive);
static ProgramSurface canonicalPrimitiveSurface(
    float3 source,
    const FptPrimitiveInstance primitive);

static float deTopologySpecializedDistance(
    float3 source,
    constant FptRenderConfig &cfg) {
    // FPT_TOPOLOGY_SPECIALIZED_BODY
    return deProgramDistance(source, cfg);
}

#if defined(FPT_STITCH_HOST)
struct StitchProgramState {
    float3 position;
    float3 jx;
    float3 jy;
    float3 jz;
    float distance_scale;
    float distance;
    float3 gradient;
    uint has_primitive;
};

[[stitchable]] StitchProgramState fpt_stitch_init(float3 source) {
    StitchProgramState state;
    state.position = source;
    state.jx = float3(1.0f, 0.0f, 0.0f);
    state.jy = float3(0.0f, 1.0f, 0.0f);
    state.jz = float3(0.0f, 0.0f, 1.0f);
    state.distance_scale = 1.0f;
    state.distance = inf;
    state.gradient = float3(0.0f);
    state.has_primitive = 0u;
    return state;
}

[[stitchable]] float fpt_stitch_finish(StitchProgramState state) {
    return state.distance;
}

[[stitchable]] ProgramSurface fpt_stitch_finish_surface(
    StitchProgramState state) {
    ProgramSurface surface = {state.distance, state.gradient};
    return surface;
}

// Distance-only stitching deliberately excludes the position Jacobian and
// winning gradient. It isolates distance-loop state pressure from accepted-hit
// analytic surface evaluation; surfaces fall back to the interpreted program.
struct StitchDistanceState {
    float3 position;
    float distance_scale;
    float distance;
    uint has_primitive;
};

[[stitchable]] StitchDistanceState fpt_stitch_distance_init(float3 source) {
    StitchDistanceState state;
    state.position = source;
    state.distance_scale = 1.0f;
    state.distance = inf;
    state.has_primitive = 0u;
    return state;
}

[[stitchable]] float fpt_stitch_distance_finish(StitchDistanceState state) {
    return state.distance;
}

// The split-graph experiment needs a real optimizer boundary rather than the
// absence of an always-inline hint. Preserve the state exactly across a call
// that the Metal compiler is explicitly forbidden to inline.
[[stitchable]] __attribute__((noinline)) StitchDistanceState
fpt_stitch_distance_barrier(StitchDistanceState state) {
    return state;
}

template <uint Index>
static StitchDistanceState fptStitchDistanceAbs(
    StitchDistanceState state,
    constant FptRenderConfig &cfg) {
    (void)cfg;
    state.position = abs(state.position);
    return state;
}

template <uint Index>
static StitchDistanceState fptStitchDistanceTranslate(
    StitchDistanceState state,
    constant FptRenderConfig &cfg) {
    state.position -= float3(cfg.sdf_program[Index].data[0],
                             cfg.sdf_program[Index].data[1],
                             cfg.sdf_program[Index].data[2]);
    return state;
}

template <uint Index>
static StitchDistanceState fptStitchDistanceScale(
    StitchDistanceState state,
    constant FptRenderConfig &cfg) {
    float scale = abs(cfg.sdf_program[Index].data[0]) > 1.0e-6f
        ? cfg.sdf_program[Index].data[0] : 1.0f;
    state.position *= scale;
    state.distance_scale *= abs(scale);
    return state;
}

template <uint Index>
static StitchDistanceState fptStitchDistanceRotateX(
    StitchDistanceState state,
    constant FptRenderConfig &cfg) {
    state.position.yz = rot2(state.position.yz, cfg.sdf_program[Index].data[0]);
    return state;
}

template <uint Index>
static StitchDistanceState fptStitchDistanceRotateY(
    StitchDistanceState state,
    constant FptRenderConfig &cfg) {
    state.position.xz = rot2(state.position.xz, cfg.sdf_program[Index].data[0]);
    return state;
}

template <uint Index>
static StitchDistanceState fptStitchDistanceRotateZ(
    StitchDistanceState state,
    constant FptRenderConfig &cfg) {
    state.position.xy = rot2(state.position.xy, cfg.sdf_program[Index].data[0]);
    return state;
}

template <uint Index>
static StitchDistanceState fptStitchDistanceRepeat(
    StitchDistanceState state,
    constant FptRenderConfig &cfg) {
    float3 period = max(abs(float3(cfg.sdf_program[Index].data[0],
                                   cfg.sdf_program[Index].data[1],
                                   cfg.sdf_program[Index].data[2])),
                        float3(1.0e-5f));
    state.position -= period * floor(state.position / period + 0.5f);
    return state;
}

template <uint Index>
static StitchDistanceState fptStitchDistanceSortDesc(
    StitchDistanceState state,
    constant FptRenderConfig &cfg) {
    (void)cfg;
    if (state.position.x < state.position.z) state.position.xz = state.position.zx;
    if (state.position.y < state.position.z) state.position.yz = state.position.zy;
    if (state.position.x < state.position.y) state.position.xy = state.position.yx;
    return state;
}

template <uint Mode>
static StitchDistanceState fptStitchDistanceCombine(
    StitchDistanceState state,
    float candidate) {
    bool choose_candidate = state.has_primitive == 0u ||
        (Mode == 1u ? candidate > state.distance :
         (Mode == 2u ? -candidate > state.distance :
                       candidate < state.distance));
    if (choose_candidate) {
        state.distance = Mode == 2u && state.has_primitive != 0u
            ? -candidate : candidate;
    }
    state.has_primitive = 1u;
    return state;
}

template <uint Index, uint Mode>
static StitchDistanceState fptStitchDistanceSphere(
    StitchDistanceState state,
    constant FptRenderConfig &cfg) {
    float candidate = (length(state.position) - cfg.sdf_program[Index].data[0]) /
        max(state.distance_scale, 1.0e-6f);
    return fptStitchDistanceCombine<Mode>(state, candidate);
}

template <uint Index, uint Mode>
static StitchDistanceState fptStitchDistanceBox(
    StitchDistanceState state,
    constant FptRenderConfig &cfg) {
    float3 half_extent = abs(float3(cfg.sdf_program[Index].data[0],
                                    cfg.sdf_program[Index].data[1],
                                    cfg.sdf_program[Index].data[2]));
    float3 q = abs(state.position) - half_extent;
    float candidate = (min(max(q.x, max(q.y, q.z)), 0.0f) +
                       length(max(q, float3(0.0f)))) /
        max(state.distance_scale, 1.0e-6f);
    return fptStitchDistanceCombine<Mode>(state, candidate);
}

template <uint Index, uint Mode>
static StitchDistanceState fptStitchDistancePlane(
    StitchDistanceState state,
    constant FptRenderConfig &cfg) {
    float3 normal = normalize(float3(cfg.sdf_program[Index].data[0],
                                     cfg.sdf_program[Index].data[1],
                                     cfg.sdf_program[Index].data[2]));
    float candidate = (dot(state.position, normal) +
                       cfg.sdf_program[Index].data[3]) /
        max(state.distance_scale, 1.0e-6f);
    return fptStitchDistanceCombine<Mode>(state, candidate);
}

template <uint Index>
static StitchProgramState fptStitchAbs(
    StitchProgramState state,
    constant FptRenderConfig &cfg) {
    (void)cfg;
    state.jx *= sign(state.position.x);
    state.jy *= sign(state.position.y);
    state.jz *= sign(state.position.z);
    state.position = abs(state.position);
    return state;
}

template <uint Index>
static StitchProgramState fptStitchTranslate(
    StitchProgramState state,
    constant FptRenderConfig &cfg) {
    state.position -= float3(cfg.sdf_program[Index].data[0],
                             cfg.sdf_program[Index].data[1],
                             cfg.sdf_program[Index].data[2]);
    return state;
}

template <uint Index>
static StitchProgramState fptStitchScale(
    StitchProgramState state,
    constant FptRenderConfig &cfg) {
    float scale = abs(cfg.sdf_program[Index].data[0]) > 1.0e-6f
        ? cfg.sdf_program[Index].data[0] : 1.0f;
    state.position *= scale;
    state.jx *= scale;
    state.jy *= scale;
    state.jz *= scale;
    state.distance_scale *= abs(scale);
    return state;
}

template <uint Index>
static StitchProgramState fptStitchRotateX(
    StitchProgramState state,
    constant FptRenderConfig &cfg) {
    float angle = cfg.sdf_program[Index].data[0];
    float s = sin(angle), c = cos(angle);
    float py = state.position.y;
    float3 old_jy = state.jy;
    state.position.y = c * py - s * state.position.z;
    state.position.z = s * py + c * state.position.z;
    state.jy = c * old_jy - s * state.jz;
    state.jz = s * old_jy + c * state.jz;
    return state;
}

template <uint Index>
static StitchProgramState fptStitchRotateY(
    StitchProgramState state,
    constant FptRenderConfig &cfg) {
    float angle = cfg.sdf_program[Index].data[0];
    float s = sin(angle), c = cos(angle);
    float px = state.position.x;
    float3 old_jx = state.jx;
    state.position.x = c * px - s * state.position.z;
    state.position.z = s * px + c * state.position.z;
    state.jx = c * old_jx - s * state.jz;
    state.jz = s * old_jx + c * state.jz;
    return state;
}

template <uint Index>
static StitchProgramState fptStitchRotateZ(
    StitchProgramState state,
    constant FptRenderConfig &cfg) {
    float angle = cfg.sdf_program[Index].data[0];
    float s = sin(angle), c = cos(angle);
    float px = state.position.x;
    float3 old_jx = state.jx;
    state.position.x = c * px - s * state.position.y;
    state.position.y = s * px + c * state.position.y;
    state.jx = c * old_jx - s * state.jy;
    state.jy = s * old_jx + c * state.jy;
    return state;
}

template <uint Index>
static StitchProgramState fptStitchRepeat(
    StitchProgramState state,
    constant FptRenderConfig &cfg) {
    float3 period = max(abs(float3(cfg.sdf_program[Index].data[0],
                                   cfg.sdf_program[Index].data[1],
                                   cfg.sdf_program[Index].data[2])),
                        float3(1.0e-5f));
    state.position -= period * floor(state.position / period + 0.5f);
    return state;
}

template <uint Index>
static StitchProgramState fptStitchSortDesc(
    StitchProgramState state,
    constant FptRenderConfig &cfg) {
    (void)cfg;
    if (state.position.x < state.position.z) {
        state.position.xz = state.position.zx;
        float3 swap = state.jx; state.jx = state.jz; state.jz = swap;
    }
    if (state.position.y < state.position.z) {
        state.position.yz = state.position.zy;
        float3 swap = state.jy; state.jy = state.jz; state.jz = swap;
    }
    if (state.position.x < state.position.y) {
        state.position.xy = state.position.yx;
        float3 swap = state.jx; state.jx = state.jy; state.jy = swap;
    }
    return state;
}

template <uint Mode>
static StitchProgramState fptStitchCombine(
    StitchProgramState state,
    float candidate,
    float3 candidate_gradient) {
    bool choose_candidate = state.has_primitive == 0u ||
        (Mode == 1u ? candidate > state.distance :
         (Mode == 2u ? -candidate > state.distance :
                       candidate < state.distance));
    if (choose_candidate) {
        bool subtract = Mode == 2u && state.has_primitive != 0u;
        state.distance = subtract ? -candidate : candidate;
        state.gradient = subtract ? -candidate_gradient : candidate_gradient;
    }
    state.has_primitive = 1u;
    return state;
}

template <uint Index, uint Mode>
static StitchProgramState fptStitchSphere(
    StitchProgramState state,
    constant FptRenderConfig &cfg) {
    float scale = max(state.distance_scale, 1.0e-6f);
    float radius = length(state.position);
    float candidate = (radius - cfg.sdf_program[Index].data[0]) / scale;
    float3 local_gradient = radius > 1.0e-8f
        ? state.position / radius : float3(0.0f, 1.0f, 0.0f);
    float3 gradient = (local_gradient.x * state.jx +
                       local_gradient.y * state.jy +
                       local_gradient.z * state.jz) / scale;
    return fptStitchCombine<Mode>(state, candidate, gradient);
}

template <uint TranslateIndex, uint SphereIndex>
static StitchProgramState fptStitchTranslatedSphereUnion(
    StitchProgramState state,
    constant FptRenderConfig &cfg) {
    state = fptStitchTranslate<TranslateIndex>(state, cfg);
    return fptStitchSphere<SphereIndex, 0u>(state, cfg);
}

template <uint TranslateIndex0, uint SphereIndex0,
          uint TranslateIndex1, uint SphereIndex1>
static StitchProgramState fptStitchTwoTranslatedSphereUnions(
    StitchProgramState state,
    constant FptRenderConfig &cfg) {
    state = fptStitchTranslatedSphereUnion<TranslateIndex0, SphereIndex0>(
        state, cfg);
    return fptStitchTranslatedSphereUnion<TranslateIndex1, SphereIndex1>(
        state, cfg);
}

template <uint Index, uint Mode>
static StitchProgramState fptStitchBox(
    StitchProgramState state,
    constant FptRenderConfig &cfg) {
    float scale = max(state.distance_scale, 1.0e-6f);
    float3 half_extent = abs(float3(cfg.sdf_program[Index].data[0],
                                    cfg.sdf_program[Index].data[1],
                                    cfg.sdf_program[Index].data[2]));
    float3 q = abs(state.position) - half_extent;
    float3 outside = max(q, float3(0.0f));
    float outside_length = length(outside);
    float candidate = (min(max(q.x, max(q.y, q.z)), 0.0f) +
                       outside_length) / scale;
    float3 local_gradient;
    if (outside_length > 1.0e-8f) {
        local_gradient = sign(state.position) * outside / outside_length;
    } else if (q.x >= q.y && q.x >= q.z) {
        local_gradient = float3(sign(state.position.x), 0.0f, 0.0f);
    } else if (q.y >= q.z) {
        local_gradient = float3(0.0f, sign(state.position.y), 0.0f);
    } else {
        local_gradient = float3(0.0f, 0.0f, sign(state.position.z));
    }
    float3 gradient = (local_gradient.x * state.jx +
                       local_gradient.y * state.jy +
                       local_gradient.z * state.jz) / scale;
    return fptStitchCombine<Mode>(state, candidate, gradient);
}

template <uint Index, uint Mode>
static StitchProgramState fptStitchPlane(
    StitchProgramState state,
    constant FptRenderConfig &cfg) {
    float scale = max(state.distance_scale, 1.0e-6f);
    float3 local_gradient = normalize(float3(cfg.sdf_program[Index].data[0],
                                             cfg.sdf_program[Index].data[1],
                                             cfg.sdf_program[Index].data[2]));
    float candidate = (dot(state.position, local_gradient) +
                       cfg.sdf_program[Index].data[3]) / scale;
    float3 gradient = (local_gradient.x * state.jx +
                       local_gradient.y * state.jy +
                       local_gradient.z * state.jz) / scale;
    return fptStitchCombine<Mode>(state, candidate, gradient);
}

#define FPT_DEFINE_STITCH_PRIMITIVE_SLOT(Index, Type, Function, ModeName, Mode) \
[[stitchable]] StitchProgramState fpt_stitch_##Type##_##ModeName##_##Index(    \
    StitchProgramState state, constant FptRenderConfig &cfg) {                 \
    return Function<Index, Mode>(state, cfg);                                  \
}

#define FPT_DEFINE_STITCH_PRIMITIVE_MODES(Index, Type, Function)           \
FPT_DEFINE_STITCH_PRIMITIVE_SLOT(Index, Type, Function, union, 0u)         \
FPT_DEFINE_STITCH_PRIMITIVE_SLOT(Index, Type, Function, intersection, 1u)  \
FPT_DEFINE_STITCH_PRIMITIVE_SLOT(Index, Type, Function, subtract, 2u)

#define FPT_DEFINE_STITCH_DISTANCE_PRIMITIVE_SLOT(                         \
    Index, Type, Function, ModeName, Mode)                                  \
[[stitchable]] StitchDistanceState                                          \
fpt_stitch_distance_##Type##_##ModeName##_##Index(                          \
    StitchDistanceState state, constant FptRenderConfig &cfg) {             \
    return Function<Index, Mode>(state, cfg);                                \
}

#define FPT_DEFINE_STITCH_DISTANCE_PRIMITIVE_MODES(Index, Type, Function)  \
FPT_DEFINE_STITCH_DISTANCE_PRIMITIVE_SLOT(                                  \
    Index, Type, Function, union, 0u)                                       \
FPT_DEFINE_STITCH_DISTANCE_PRIMITIVE_SLOT(                                  \
    Index, Type, Function, intersection, 1u)                                \
FPT_DEFINE_STITCH_DISTANCE_PRIMITIVE_SLOT(                                  \
    Index, Type, Function, subtract, 2u)

#define FPT_DEFINE_STITCH_SLOT(Index)                                      \
[[stitchable]] StitchProgramState fpt_stitch_abs_##Index(                  \
    StitchProgramState state, constant FptRenderConfig &cfg) {             \
    return fptStitchAbs<Index>(state, cfg);                                 \
}                                                                          \
[[stitchable]] StitchProgramState fpt_stitch_translate_##Index(            \
    StitchProgramState state, constant FptRenderConfig &cfg) {             \
    return fptStitchTranslate<Index>(state, cfg);                           \
}                                                                          \
[[stitchable]] StitchProgramState fpt_stitch_scale_##Index(                \
    StitchProgramState state, constant FptRenderConfig &cfg) {             \
    return fptStitchScale<Index>(state, cfg);                               \
}                                                                          \
[[stitchable]] StitchProgramState fpt_stitch_rotate_x_##Index(             \
    StitchProgramState state, constant FptRenderConfig &cfg) {             \
    return fptStitchRotateX<Index>(state, cfg);                             \
}                                                                          \
[[stitchable]] StitchProgramState fpt_stitch_rotate_y_##Index(             \
    StitchProgramState state, constant FptRenderConfig &cfg) {             \
    return fptStitchRotateY<Index>(state, cfg);                             \
}                                                                          \
[[stitchable]] StitchProgramState fpt_stitch_rotate_z_##Index(             \
    StitchProgramState state, constant FptRenderConfig &cfg) {             \
    return fptStitchRotateZ<Index>(state, cfg);                             \
}                                                                          \
[[stitchable]] StitchProgramState fpt_stitch_repeat_##Index(               \
    StitchProgramState state, constant FptRenderConfig &cfg) {             \
    return fptStitchRepeat<Index>(state, cfg);                              \
}                                                                          \
[[stitchable]] StitchProgramState fpt_stitch_sort_desc_##Index(            \
    StitchProgramState state, constant FptRenderConfig &cfg) {             \
    return fptStitchSortDesc<Index>(state, cfg);                            \
}                                                                          \
FPT_DEFINE_STITCH_PRIMITIVE_MODES(Index, sphere, fptStitchSphere)          \
FPT_DEFINE_STITCH_PRIMITIVE_MODES(Index, box, fptStitchBox)                \
FPT_DEFINE_STITCH_PRIMITIVE_MODES(Index, plane, fptStitchPlane)            \
[[stitchable]] StitchDistanceState fpt_stitch_distance_abs_##Index(        \
    StitchDistanceState state, constant FptRenderConfig &cfg) {            \
    return fptStitchDistanceAbs<Index>(state, cfg);                         \
}                                                                          \
[[stitchable]] StitchDistanceState fpt_stitch_distance_translate_##Index(  \
    StitchDistanceState state, constant FptRenderConfig &cfg) {            \
    return fptStitchDistanceTranslate<Index>(state, cfg);                   \
}                                                                          \
[[stitchable]] StitchDistanceState fpt_stitch_distance_scale_##Index(      \
    StitchDistanceState state, constant FptRenderConfig &cfg) {            \
    return fptStitchDistanceScale<Index>(state, cfg);                       \
}                                                                          \
[[stitchable]] StitchDistanceState fpt_stitch_distance_rotate_x_##Index(   \
    StitchDistanceState state, constant FptRenderConfig &cfg) {            \
    return fptStitchDistanceRotateX<Index>(state, cfg);                     \
}                                                                          \
[[stitchable]] StitchDistanceState fpt_stitch_distance_rotate_y_##Index(   \
    StitchDistanceState state, constant FptRenderConfig &cfg) {            \
    return fptStitchDistanceRotateY<Index>(state, cfg);                     \
}                                                                          \
[[stitchable]] StitchDistanceState fpt_stitch_distance_rotate_z_##Index(   \
    StitchDistanceState state, constant FptRenderConfig &cfg) {            \
    return fptStitchDistanceRotateZ<Index>(state, cfg);                     \
}                                                                          \
[[stitchable]] StitchDistanceState fpt_stitch_distance_repeat_##Index(     \
    StitchDistanceState state, constant FptRenderConfig &cfg) {            \
    return fptStitchDistanceRepeat<Index>(state, cfg);                      \
}                                                                          \
[[stitchable]] StitchDistanceState fpt_stitch_distance_sort_desc_##Index(  \
    StitchDistanceState state, constant FptRenderConfig &cfg) {            \
    return fptStitchDistanceSortDesc<Index>(state, cfg);                    \
}                                                                          \
FPT_DEFINE_STITCH_DISTANCE_PRIMITIVE_MODES(                                 \
    Index, sphere, fptStitchDistanceSphere)                                \
FPT_DEFINE_STITCH_DISTANCE_PRIMITIVE_MODES(                                 \
    Index, box, fptStitchDistanceBox)                                      \
FPT_DEFINE_STITCH_DISTANCE_PRIMITIVE_MODES(                                 \
    Index, plane, fptStitchDistancePlane)

#define FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(Index)                   \
[[stitchable]] StitchProgramState                                           \
fpt_stitch_translated_sphere_union_##Index(                                \
    StitchProgramState state, constant FptRenderConfig &cfg) {             \
    return fptStitchTranslatedSphereUnion<Index, Index + 1u>(state, cfg);   \
}

#define FPT_DEFINE_STITCH_TWO_TRANSLATED_SPHERE_UNIONS(Index)              \
[[stitchable]] StitchProgramState                                           \
fpt_stitch_two_translated_sphere_unions_##Index(                           \
    StitchProgramState state, constant FptRenderConfig &cfg) {             \
    return fptStitchTwoTranslatedSphereUnions<                              \
        Index, Index + 1u, Index + 2u, Index + 3u>(state, cfg);             \
}

FPT_DEFINE_STITCH_SLOT(0)
FPT_DEFINE_STITCH_SLOT(1)
FPT_DEFINE_STITCH_SLOT(2)
FPT_DEFINE_STITCH_SLOT(3)
FPT_DEFINE_STITCH_SLOT(4)
FPT_DEFINE_STITCH_SLOT(5)
FPT_DEFINE_STITCH_SLOT(6)
FPT_DEFINE_STITCH_SLOT(7)
FPT_DEFINE_STITCH_SLOT(8)
FPT_DEFINE_STITCH_SLOT(9)
FPT_DEFINE_STITCH_SLOT(10)
FPT_DEFINE_STITCH_SLOT(11)
FPT_DEFINE_STITCH_SLOT(12)
FPT_DEFINE_STITCH_SLOT(13)
FPT_DEFINE_STITCH_SLOT(14)
FPT_DEFINE_STITCH_SLOT(15)
FPT_DEFINE_STITCH_SLOT(16)
FPT_DEFINE_STITCH_SLOT(17)
FPT_DEFINE_STITCH_SLOT(18)
FPT_DEFINE_STITCH_SLOT(19)
FPT_DEFINE_STITCH_SLOT(20)
FPT_DEFINE_STITCH_SLOT(21)
FPT_DEFINE_STITCH_SLOT(22)
FPT_DEFINE_STITCH_SLOT(23)
FPT_DEFINE_STITCH_SLOT(24)
FPT_DEFINE_STITCH_SLOT(25)
FPT_DEFINE_STITCH_SLOT(26)
FPT_DEFINE_STITCH_SLOT(27)
FPT_DEFINE_STITCH_SLOT(28)
FPT_DEFINE_STITCH_SLOT(29)
FPT_DEFINE_STITCH_SLOT(30)
FPT_DEFINE_STITCH_SLOT(31)
FPT_DEFINE_STITCH_SLOT(32)
FPT_DEFINE_STITCH_SLOT(33)
FPT_DEFINE_STITCH_SLOT(34)
FPT_DEFINE_STITCH_SLOT(35)
FPT_DEFINE_STITCH_SLOT(36)
FPT_DEFINE_STITCH_SLOT(37)
FPT_DEFINE_STITCH_SLOT(38)
FPT_DEFINE_STITCH_SLOT(39)
FPT_DEFINE_STITCH_SLOT(40)
FPT_DEFINE_STITCH_SLOT(41)
FPT_DEFINE_STITCH_SLOT(42)
FPT_DEFINE_STITCH_SLOT(43)
FPT_DEFINE_STITCH_SLOT(44)
FPT_DEFINE_STITCH_SLOT(45)
FPT_DEFINE_STITCH_SLOT(46)
FPT_DEFINE_STITCH_SLOT(47)
FPT_DEFINE_STITCH_SLOT(48)
FPT_DEFINE_STITCH_SLOT(49)
FPT_DEFINE_STITCH_SLOT(50)
FPT_DEFINE_STITCH_SLOT(51)
FPT_DEFINE_STITCH_SLOT(52)
FPT_DEFINE_STITCH_SLOT(53)
FPT_DEFINE_STITCH_SLOT(54)
FPT_DEFINE_STITCH_SLOT(55)
FPT_DEFINE_STITCH_SLOT(56)
FPT_DEFINE_STITCH_SLOT(57)
FPT_DEFINE_STITCH_SLOT(58)
FPT_DEFINE_STITCH_SLOT(59)
FPT_DEFINE_STITCH_SLOT(60)
FPT_DEFINE_STITCH_SLOT(61)
FPT_DEFINE_STITCH_SLOT(62)
FPT_DEFINE_STITCH_SLOT(63)

FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(0)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(2)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(4)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(6)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(8)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(10)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(12)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(14)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(16)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(18)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(20)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(22)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(24)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(26)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(28)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(30)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(32)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(34)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(36)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(38)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(40)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(42)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(44)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(46)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(48)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(50)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(52)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(54)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(56)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(58)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(60)
FPT_DEFINE_STITCH_TRANSLATED_SPHERE_UNION(62)

FPT_DEFINE_STITCH_TWO_TRANSLATED_SPHERE_UNIONS(0)
FPT_DEFINE_STITCH_TWO_TRANSLATED_SPHERE_UNIONS(4)
FPT_DEFINE_STITCH_TWO_TRANSLATED_SPHERE_UNIONS(8)
FPT_DEFINE_STITCH_TWO_TRANSLATED_SPHERE_UNIONS(12)
FPT_DEFINE_STITCH_TWO_TRANSLATED_SPHERE_UNIONS(16)
FPT_DEFINE_STITCH_TWO_TRANSLATED_SPHERE_UNIONS(20)
FPT_DEFINE_STITCH_TWO_TRANSLATED_SPHERE_UNIONS(24)
FPT_DEFINE_STITCH_TWO_TRANSLATED_SPHERE_UNIONS(28)
FPT_DEFINE_STITCH_TWO_TRANSLATED_SPHERE_UNIONS(32)
FPT_DEFINE_STITCH_TWO_TRANSLATED_SPHERE_UNIONS(36)
FPT_DEFINE_STITCH_TWO_TRANSLATED_SPHERE_UNIONS(40)
FPT_DEFINE_STITCH_TWO_TRANSLATED_SPHERE_UNIONS(44)
FPT_DEFINE_STITCH_TWO_TRANSLATED_SPHERE_UNIONS(48)
FPT_DEFINE_STITCH_TWO_TRANSLATED_SPHERE_UNIONS(52)
FPT_DEFINE_STITCH_TWO_TRANSLATED_SPHERE_UNIONS(56)
FPT_DEFINE_STITCH_TWO_TRANSLATED_SPHERE_UNIONS(60)

#undef FPT_DEFINE_STITCH_SLOT
#undef FPT_DEFINE_STITCH_DISTANCE_PRIMITIVE_MODES
#undef FPT_DEFINE_STITCH_DISTANCE_PRIMITIVE_SLOT
#undef FPT_DEFINE_STITCH_PRIMITIVE_MODES
#undef FPT_DEFINE_STITCH_PRIMITIVE_SLOT
#endif

static float flatUnionPrimitiveDistance(
    float3 source,
    const FptPrimitiveInstance instance) {
    float3 p = float3(
        dot(float3(instance.transform[0], instance.transform[1],
                   instance.transform[2]), source) + instance.transform[3],
        dot(float3(instance.transform[4], instance.transform[5],
                   instance.transform[6]), source) + instance.transform[7],
        dot(float3(instance.transform[8], instance.transform[9],
                   instance.transform[10]), source) + instance.transform[11]);
    float4 data = float4(instance.data[0], instance.data[1],
                         instance.data[2], instance.data[3]);
    float divisor = max(instance.distance_scale, 1.0e-6f);
    if (instance.opcode == SDF_OP_SPHERE) {
        return (length(p) - data.x) / divisor;
    }
    if (instance.opcode == SDF_OP_BOX) {
        float3 q = abs(p) - abs(data.xyz);
        return (min(max(q.x, max(q.y, q.z)), 0.0f) +
                length(max(q, float3(0.0f)))) / divisor;
    }
    if (instance.opcode == SDF_OP_PLANE) {
        return (dot(p, data.xyz) + data.w) / divisor;
    }
    return inf;
}

static float deFlatUnionDistance(float3 source,
                                 constant FptRenderConfig &cfg) {
    float distance = inf;
    uint count = min(cfg.sdf_flat_union_count,
                     FPT_SDF_FLAT_UNION_MAX_PRIMITIVES);
    for (uint index = 0u; index < count; ++index) {
        distance = min(distance, flatUnionPrimitiveDistance(
            source, cfg.sdf_flat_union_instances[index]));
    }
    return distance;
}

static float deCanonicalDistance(float3 source,
                                 constant FptRenderConfig &cfg) {
    float distance = inf;
    uint count = min(cfg.sdf_canonical_count,
                     uint(FPT_SDF_FLAT_UNION_MAX_PRIMITIVES));
    for (uint index = 0u; index < count; ++index) {
        const FptPrimitiveInstance primitive =
            cfg.sdf_canonical_primitives[index];
        float candidate = flatUnionPrimitiveDistance(source, primitive);
        if (index == 0u) {
            distance = candidate;
        } else if (primitive._pad0 == 1u) {
            distance = max(distance, candidate);
        } else if (primitive._pad0 == 2u) {
            distance = max(distance, -candidate);
        } else {
            distance = min(distance, candidate);
        }
    }
    return distance;
}

static ProgramSurface canonicalPrimitiveSurface(
    float3 source,
    const FptPrimitiveInstance primitive) {
        float3 row0 = float3(primitive.transform[0], primitive.transform[1],
                             primitive.transform[2]);
        float3 row1 = float3(primitive.transform[4], primitive.transform[5],
                             primitive.transform[6]);
        float3 row2 = float3(primitive.transform[8], primitive.transform[9],
                             primitive.transform[10]);
        float3 p = float3(dot(row0, source) + primitive.transform[3],
                          dot(row1, source) + primitive.transform[7],
                          dot(row2, source) + primitive.transform[11]);
        float4 data = float4(primitive.data[0], primitive.data[1],
                             primitive.data[2], primitive.data[3]);
        float divisor = max(primitive.distance_scale, 1.0e-6f);
        float candidate = inf;
        float3 local_gradient = float3(0.0f, 1.0f, 0.0f);
        if (primitive.opcode == SDF_OP_SPHERE) {
            float radius = length(p);
            candidate = (radius - data.x) / divisor;
            if (radius > 1.0e-8f) local_gradient = p / radius;
        } else if (primitive.opcode == SDF_OP_BOX) {
            float3 q = abs(p) - abs(data.xyz);
            float3 outside = max(q, float3(0.0f));
            float outside_length = length(outside);
            candidate = (min(max(q.x, max(q.y, q.z)), 0.0f) +
                         outside_length) / divisor;
            if (outside_length > 1.0e-8f) {
                local_gradient = sign(p) * outside / outside_length;
            } else if (q.x >= q.y && q.x >= q.z) {
                local_gradient = float3(sign(p.x), 0.0f, 0.0f);
            } else if (q.y >= q.z) {
                local_gradient = float3(0.0f, sign(p.y), 0.0f);
            } else {
                local_gradient = float3(0.0f, 0.0f, sign(p.z));
            }
        } else if (primitive.opcode == SDF_OP_PLANE) {
            local_gradient = data.xyz;
            candidate = (dot(p, local_gradient) + data.w) / divisor;
        }
        float3 candidate_gradient =
            (local_gradient.x * row0 + local_gradient.y * row1 +
             local_gradient.z * row2) / divisor;
        return {candidate, candidate_gradient};
}

static ProgramSurface canonicalSurface(float3 source,
                                       constant FptRenderConfig &cfg) {
    ProgramSurface surface = {inf, float3(0.0f)};
    uint count = min(cfg.sdf_canonical_count,
                     uint(FPT_SDF_FLAT_UNION_MAX_PRIMITIVES));
    for (uint index = 0u; index < count; ++index) {
        const FptPrimitiveInstance primitive =
            cfg.sdf_canonical_primitives[index];
        ProgramSurface candidate_surface =
            canonicalPrimitiveSurface(source, primitive);
        float candidate = candidate_surface.distance;
        float3 candidate_gradient = candidate_surface.gradient;
        bool choose = index == 0u ||
            (primitive._pad0 == 1u
                ? candidate > surface.distance
                : (primitive._pad0 == 2u
                    ? -candidate > surface.distance
                    : candidate < surface.distance));
        if (choose) {
            surface.distance = primitive._pad0 == 2u && index > 0u
                ? -candidate : candidate;
            surface.gradient = primitive._pad0 == 2u && index > 0u
                ? -candidate_gradient : candidate_gradient;
        }
    }
    return surface;
}

static uint typedSoAPrimitiveCount(constant FptRenderConfig &cfg) {
    return min(cfg.sdf_typed_soa.sphere_count,
               uint(FPT_SDF_FLAT_UNION_MAX_PRIMITIVES)) +
           min(cfg.sdf_typed_soa.box_count,
               uint(FPT_SDF_FLAT_UNION_MAX_PRIMITIVES)) +
           min(cfg.sdf_typed_soa.plane_count,
               uint(FPT_SDF_FLAT_UNION_MAX_PRIMITIVES));
}

static float deTypedSoADistance(float3 source,
                                constant FptRenderConfig &cfg) {
    float distance = inf;
    uint sphere_count = min(cfg.sdf_typed_soa.sphere_count,
                            uint(FPT_SDF_FLAT_UNION_MAX_PRIMITIVES));
    for (uint index = 0u; index < sphere_count; ++index) {
        float3 center = float3(cfg.sdf_typed_soa.sphere_x[index],
                               cfg.sdf_typed_soa.sphere_y[index],
                               cfg.sdf_typed_soa.sphere_z[index]);
        distance = min(distance, length(source - center) -
                       cfg.sdf_typed_soa.sphere_radius[index]);
    }
    uint box_count = min(cfg.sdf_typed_soa.box_count,
                         uint(FPT_SDF_FLAT_UNION_MAX_PRIMITIVES));
    for (uint index = 0u; index < box_count; ++index) {
        float3 center = float3(cfg.sdf_typed_soa.box_x[index],
                               cfg.sdf_typed_soa.box_y[index],
                               cfg.sdf_typed_soa.box_z[index]);
        float3 half_extent = float3(cfg.sdf_typed_soa.box_half_x[index],
                                    cfg.sdf_typed_soa.box_half_y[index],
                                    cfg.sdf_typed_soa.box_half_z[index]);
        float3 q = abs(source - center) - half_extent;
        float candidate = min(max(q.x, max(q.y, q.z)), 0.0f) +
                          length(max(q, float3(0.0f)));
        distance = min(distance, candidate);
    }
    uint plane_count = min(cfg.sdf_typed_soa.plane_count,
                           uint(FPT_SDF_FLAT_UNION_MAX_PRIMITIVES));
    for (uint index = 0u; index < plane_count; ++index) {
        float3 normal = normalize(float3(cfg.sdf_typed_soa.plane_x[index],
                                         cfg.sdf_typed_soa.plane_y[index],
                                         cfg.sdf_typed_soa.plane_z[index]));
        float3 center = float3(cfg.sdf_typed_soa.plane_center_x[index],
                               cfg.sdf_typed_soa.plane_center_y[index],
                               cfg.sdf_typed_soa.plane_center_z[index]);
        distance = min(distance, dot(source - center, normal) +
                       cfg.sdf_typed_soa.plane_offset[index]);
    }
    return distance;
}

static ProgramSurface typedSoASurface(float3 source,
                                      constant FptRenderConfig &cfg) {
    ProgramSurface surface = {inf, float3(0.0f)};
    uint winner_source = 0xffffffffu;
    uint sphere_count = min(cfg.sdf_typed_soa.sphere_count,
                            uint(FPT_SDF_FLAT_UNION_MAX_PRIMITIVES));
    for (uint index = 0u; index < sphere_count; ++index) {
        float3 center = float3(cfg.sdf_typed_soa.sphere_x[index],
                               cfg.sdf_typed_soa.sphere_y[index],
                               cfg.sdf_typed_soa.sphere_z[index]);
        float3 local = source - center;
        float radius = length(local);
        float candidate = radius - cfg.sdf_typed_soa.sphere_radius[index];
        uint candidate_source = cfg.sdf_typed_soa.sphere_source[index];
        if (candidate < surface.distance ||
            (candidate == surface.distance && candidate_source < winner_source)) {
            surface.distance = candidate;
            surface.gradient = radius > 1.0e-8f
                ? local / radius : float3(0.0f, 1.0f, 0.0f);
            winner_source = candidate_source;
        }
    }
    uint box_count = min(cfg.sdf_typed_soa.box_count,
                         uint(FPT_SDF_FLAT_UNION_MAX_PRIMITIVES));
    for (uint index = 0u; index < box_count; ++index) {
        float3 center = float3(cfg.sdf_typed_soa.box_x[index],
                               cfg.sdf_typed_soa.box_y[index],
                               cfg.sdf_typed_soa.box_z[index]);
        float3 half_extent = float3(cfg.sdf_typed_soa.box_half_x[index],
                                    cfg.sdf_typed_soa.box_half_y[index],
                                    cfg.sdf_typed_soa.box_half_z[index]);
        float3 local = source - center;
        float3 q = abs(local) - half_extent;
        float3 outside = max(q, float3(0.0f));
        float outside_length = length(outside);
        float candidate = min(max(q.x, max(q.y, q.z)), 0.0f) + outside_length;
        uint candidate_source = cfg.sdf_typed_soa.box_source[index];
        if (candidate < surface.distance ||
            (candidate == surface.distance && candidate_source < winner_source)) {
            surface.distance = candidate;
            if (outside_length > 1.0e-8f) {
                surface.gradient = sign(local) * outside / outside_length;
            } else if (q.x >= q.y && q.x >= q.z) {
                surface.gradient = float3(sign(local.x), 0.0f, 0.0f);
            } else if (q.y >= q.z) {
                surface.gradient = float3(0.0f, sign(local.y), 0.0f);
            } else {
                surface.gradient = float3(0.0f, 0.0f, sign(local.z));
            }
            winner_source = candidate_source;
        }
    }
    uint plane_count = min(cfg.sdf_typed_soa.plane_count,
                           uint(FPT_SDF_FLAT_UNION_MAX_PRIMITIVES));
    for (uint index = 0u; index < plane_count; ++index) {
        float3 normal = normalize(float3(cfg.sdf_typed_soa.plane_x[index],
                                         cfg.sdf_typed_soa.plane_y[index],
                                         cfg.sdf_typed_soa.plane_z[index]));
        float3 center = float3(cfg.sdf_typed_soa.plane_center_x[index],
                               cfg.sdf_typed_soa.plane_center_y[index],
                               cfg.sdf_typed_soa.plane_center_z[index]);
        float candidate = dot(source - center, normal) +
                          cfg.sdf_typed_soa.plane_offset[index];
        uint candidate_source = cfg.sdf_typed_soa.plane_source[index];
        if (candidate < surface.distance ||
            (candidate == surface.distance && candidate_source < winner_source)) {
            surface.distance = candidate;
            surface.gradient = normal;
            winner_source = candidate_source;
        }
    }
    return surface;
}

static ulong regionalProgramMaskAt(float3 position,
                                   constant FptRenderConfig &cfg,
                                   device const ulong *program_masks) {
    float3 bounds_min = float3(cfg.voxel_bounds_min[0], cfg.voxel_bounds_min[1],
                               cfg.voxel_bounds_min[2]);
    float3 bounds_max = float3(cfg.voxel_bounds_max[0], cfg.voxel_bounds_max[1],
                               cfg.voxel_bounds_max[2]);
    if (any(position < bounds_min) || any(position >= bounds_max)) {
        return programDistanceInstructionMask(cfg);
    }
    uint resolution = max(cfg.regional_program_resolution, 1u);
    float3 unit = clamp((position - bounds_min) / (bounds_max - bounds_min),
                        float3(0.0f), float3(0.99999994f));
    uint3 cell = min(uint3(unit * float(resolution)), uint3(resolution - 1u));
    uint index = cell.x + cell.y * resolution + cell.z * resolution * resolution;
    ulong mask = program_masks[index];
    return mask != 0ul ? mask : programDistanceInstructionMask(cfg);
}

static float deProgramMaskedDistance(float3 source,
                                     ulong instruction_mask,
                                     constant FptRenderConfig &cfg) {
    float3 p = source;
    float distance_scale = 1.0f;
    float distance = inf;
    bool has_primitive = false;
    uint count = min(cfg.sdf_program_count, FPT_SDF_PROGRAM_MAX_OPS);
    for (uint i = 0u; i < count; ++i) {
        if ((instruction_mask & (1ul << i)) == 0ul) continue;
        const FptSdfInstruction op = cfg.sdf_program[i];
        float4 data = float4(op.data[0], op.data[1], op.data[2], op.data[3]);
        switch (op.opcode) {
            case SDF_OP_ABS:
                p = abs(p);
                break;
            case SDF_OP_TRANSLATE:
                p -= data.xyz;
                break;
            case SDF_OP_SCALE: {
                float scale = abs(data.x) > 1.0e-6f ? data.x : 1.0f;
                p *= scale;
                distance_scale *= abs(scale);
                break;
            }
            case SDF_OP_ROTATE_X:
                p.yz = rot2(p.yz, data.x);
                break;
            case SDF_OP_ROTATE_Y:
                p.xz = rot2(p.xz, data.x);
                break;
            case SDF_OP_ROTATE_Z:
                p.xy = rot2(p.xy, data.x);
                break;
            case SDF_OP_REPEAT: {
                float3 period = max(abs(data.xyz), float3(1.0e-5f));
                p -= period * floor(p / period + 0.5f);
                break;
            }
            case SDF_OP_SORT_DESC:
                if (p.x < p.z) p.xz = p.zx;
                if (p.y < p.z) p.yz = p.zy;
                if (p.x < p.y) p.xy = p.yx;
                break;
            case SDF_OP_SPHERE: {
                float candidate = (length(p) - data.x) /
                                  max(distance_scale, 1.0e-6f);
                distance = has_primitive
                    ? combineProgramDistance(distance, candidate, op.flags)
                    : candidate;
                has_primitive = true;
                break;
            }
            case SDF_OP_BOX: {
                float3 q = abs(p) - abs(data.xyz);
                float candidate = (min(max(q.x, max(q.y, q.z)), 0.0f) +
                                   length(max(q, float3(0.0f)))) /
                                  max(distance_scale, 1.0e-6f);
                distance = has_primitive
                    ? combineProgramDistance(distance, candidate, op.flags)
                    : candidate;
                has_primitive = true;
                break;
            }
            case SDF_OP_PLANE: {
                float candidate = (dot(p, normalize(data.xyz)) + data.w) /
                                  max(distance_scale, 1.0e-6f);
                distance = has_primitive
                    ? combineProgramDistance(distance, candidate, op.flags)
                    : candidate;
                has_primitive = true;
                break;
            }
            default:
                break;
        }
    }
    return distance;
}

static float deRegionalInstructionPoolDistance(
    float3 source,
    RegionalProgramHeader header,
    device const FptSdfInstruction *instruction_pool) {
    float3 p = source;
    float distance_scale = 1.0f;
    float distance = inf;
    bool has_primitive = false;
    for (uint i = 0u; i < header.instruction_count; ++i) {
        const FptSdfInstruction op =
            instruction_pool[header.instruction_offset + i];
        float4 data = float4(op.data[0], op.data[1], op.data[2], op.data[3]);
        switch (op.opcode) {
            case SDF_OP_ABS:
                p = abs(p);
                break;
            case SDF_OP_TRANSLATE:
                p -= data.xyz;
                break;
            case SDF_OP_SCALE: {
                float scale = abs(data.x) > 1.0e-6f ? data.x : 1.0f;
                p *= scale;
                distance_scale *= abs(scale);
                break;
            }
            case SDF_OP_ROTATE_X:
                p.yz = rot2(p.yz, data.x);
                break;
            case SDF_OP_ROTATE_Y:
                p.xz = rot2(p.xz, data.x);
                break;
            case SDF_OP_ROTATE_Z:
                p.xy = rot2(p.xy, data.x);
                break;
            case SDF_OP_REPEAT: {
                float3 period = max(abs(data.xyz), float3(1.0e-5f));
                p -= period * floor(p / period + 0.5f);
                break;
            }
            case SDF_OP_SORT_DESC:
                if (p.x < p.z) p.xz = p.zx;
                if (p.y < p.z) p.yz = p.zy;
                if (p.x < p.y) p.xy = p.yx;
                break;
            case SDF_OP_SPHERE: {
                float candidate = (length(p) - data.x) /
                                  max(distance_scale, 1.0e-6f);
                distance = has_primitive
                    ? combineProgramDistance(distance, candidate, op.flags)
                    : candidate;
                has_primitive = true;
                break;
            }
            case SDF_OP_BOX: {
                float3 q = abs(p) - abs(data.xyz);
                float candidate = (min(max(q.x, max(q.y, q.z)), 0.0f) +
                                   length(max(q, float3(0.0f)))) /
                                  max(distance_scale, 1.0e-6f);
                distance = has_primitive
                    ? combineProgramDistance(distance, candidate, op.flags)
                    : candidate;
                has_primitive = true;
                break;
            }
            case SDF_OP_PLANE: {
                float candidate = (dot(p, normalize(data.xyz)) + data.w) /
                                  max(distance_scale, 1.0e-6f);
                distance = has_primitive
                    ? combineProgramDistance(distance, candidate, op.flags)
                    : candidate;
                has_primitive = true;
                break;
            }
            default:
                break;
        }
    }
    return distance;
}

static float deRegionalPrimitivePoolDistance(
    float3 source,
    RegionalProgramHeader header,
    device const ushort *primitive_pool,
    constant FptRenderConfig &cfg) {
    float distance = inf;
    for (uint i = 0u; i < header.primitive_count; ++i) {
        uint primitive_index = uint(
            primitive_pool[header.primitive_offset + i]);
        if (primitive_index >= cfg.sdf_flat_union_count ||
            primitive_index >= FPT_SDF_FLAT_UNION_MAX_PRIMITIVES) {
            continue;
        }
        distance = min(distance, flatUnionPrimitiveDistance(
            source, cfg.sdf_flat_union_instances[primitive_index]));
    }
    return distance;
}

static float deRegionalProgramDistance(
    float3 source,
    RegionalProgramHeader header,
    device const FptSdfInstruction *instruction_pool,
    device const ushort *primitive_pool,
    constant FptRenderConfig &cfg) {
    return cfg.sdf_flat_union_count > 0u
        ? deRegionalPrimitivePoolDistance(source, header, primitive_pool, cfg)
        : deRegionalInstructionPoolDistance(source, header, instruction_pool);
}

static float mapRegionalProgram(float3 position,
                                constant FptRenderConfig &cfg,
                                device const ushort *program_ids,
                                device const RegionalProgramHeader *program_headers,
                                device const FptSdfInstruction *instruction_pool,
                                device const ushort *primitive_pool) {
    float3 bounds_min = float3(cfg.voxel_bounds_min[0], cfg.voxel_bounds_min[1],
                               cfg.voxel_bounds_min[2]);
    float3 bounds_max = float3(cfg.voxel_bounds_max[0], cfg.voxel_bounds_max[1],
                               cfg.voxel_bounds_max[2]);
    if (any(position < bounds_min) || any(position >= bounds_max)) {
        float distance = cfg.sdf_flat_union_count > 0u
            ? deFlatUnionDistance(position, cfg)
            : deProgramDistance(position, cfg);
        return mix(distance, distance / 2.0f, cfg.render[6]);
    }
    uint resolution = max(cfg.regional_program_resolution, 1u);
    float3 unit = clamp((position - bounds_min) / (bounds_max - bounds_min),
                        float3(0.0f), float3(0.99999994f));
    uint3 cell = min(uint3(unit * float(resolution)), uint3(resolution - 1u));
    uint cell_index = cell.x + cell.y * resolution +
                      cell.z * resolution * resolution;
    RegionalProgramHeader header = program_headers[program_ids[cell_index]];
    float distance = deRegionalProgramDistance(
        position, header, instruction_pool, primitive_pool, cfg);
    return mix(distance, distance / 2.0f, cfg.render[6]);
}

static float mapRegionalProgramTracked(
    float3 position,
    constant FptRenderConfig &cfg,
    device const ushort *program_ids,
    device const RegionalProgramHeader *program_headers,
    device const FptSdfInstruction *instruction_pool,
    device const ushort *primitive_pool,
    uint ray_class,
    thread RegionalProgramLookupState &lookup_state,
    thread RegionalProgramLocalStats &stats) {
    stats.distance_evaluations[ray_class]++;
    float3 bounds_min = float3(cfg.voxel_bounds_min[0], cfg.voxel_bounds_min[1],
                               cfg.voxel_bounds_min[2]);
    float3 bounds_max = float3(cfg.voxel_bounds_max[0], cfg.voxel_bounds_max[1],
                               cfg.voxel_bounds_max[2]);
    if (any(position < bounds_min) || any(position >= bounds_max)) {
        stats.full_program_evaluations[ray_class]++;
        lookup_state.valid = false;
        float distance = cfg.sdf_flat_union_count > 0u
            ? deFlatUnionDistance(position, cfg)
            : deProgramDistance(position, cfg);
        return mix(distance, distance / 2.0f, cfg.render[6]);
    }

    uint resolution = max(cfg.regional_program_resolution, 1u);
    float3 unit = clamp((position - bounds_min) / (bounds_max - bounds_min),
                        float3(0.0f), float3(0.99999994f));
    uint3 cell = min(uint3(unit * float(resolution)), uint3(resolution - 1u));
    uint cell_index = cell.x + cell.y * resolution +
                      cell.z * resolution * resolution;
    stats.atlas_evaluations[ray_class]++;
    bool same_cell = lookup_state.valid &&
                     lookup_state.cell_index == cell_index;
    if (same_cell) {
        stats.same_cell_reuses[ray_class]++;
    } else {
        stats.cell_entries[ray_class]++;
    }

    uint program_id = uint(program_ids[cell_index]);
    RegionalProgramHeader header = program_headers[program_id];
    stats.program_id_loads[ray_class]++;
    stats.header_loads[ray_class]++;
    if (lookup_state.valid && lookup_state.program_id == program_id) {
        stats.same_program_reuses[ray_class]++;
    }
    lookup_state.cell_index = cell_index;
    lookup_state.program_id = program_id;
    lookup_state.valid = true;
    stats.dynamic_instructions[ray_class] += cfg.sdf_flat_union_count > 0u
        ? header.primitive_count : header.instruction_count;

    float distance = deRegionalProgramDistance(
        position, header, instruction_pool, primitive_pool, cfg);
    return mix(distance, distance / 2.0f, cfg.render[6]);
}

static ProgramSurface programSurfaceInterpreted(
    float3 source,
    constant FptRenderConfig &cfg) {
    float3 p = source;
    float3 jx = float3(1.0f, 0.0f, 0.0f);
    float3 jy = float3(0.0f, 1.0f, 0.0f);
    float3 jz = float3(0.0f, 0.0f, 1.0f);
    float distance_scale = 1.0f;
    ProgramSurface surface = {inf, float3(0.0f)};
    bool has_primitive = false;
    uint count = min(cfg.sdf_program_count, FPT_SDF_PROGRAM_MAX_OPS);
    for (uint i = 0u; i < count; ++i) {
        const FptSdfInstruction op = cfg.sdf_program[i];
        float4 data = float4(op.data[0], op.data[1], op.data[2], op.data[3]);
        switch (op.opcode) {
            case SDF_OP_ABS:
                jx *= sign(p.x); jy *= sign(p.y); jz *= sign(p.z);
                p = abs(p);
                break;
            case SDF_OP_TRANSLATE:
                p -= data.xyz;
                break;
            case SDF_OP_SCALE: {
                float s = abs(data.x) > 1.0e-6f ? data.x : 1.0f;
                p *= s; jx *= s; jy *= s; jz *= s;
                distance_scale *= abs(s);
                break;
            }
            case SDF_OP_ROTATE_X: {
                float s = sin(data.x), c = cos(data.x);
                float py = p.y; float3 old_jy = jy;
                p.y = c * py - s * p.z; p.z = s * py + c * p.z;
                jy = c * old_jy - s * jz; jz = s * old_jy + c * jz;
                break;
            }
            case SDF_OP_ROTATE_Y: {
                float s = sin(data.x), c = cos(data.x);
                float px = p.x; float3 old_jx = jx;
                p.x = c * px - s * p.z; p.z = s * px + c * p.z;
                jx = c * old_jx - s * jz; jz = s * old_jx + c * jz;
                break;
            }
            case SDF_OP_ROTATE_Z: {
                float s = sin(data.x), c = cos(data.x);
                float px = p.x; float3 old_jx = jx;
                p.x = c * px - s * p.y; p.y = s * px + c * p.y;
                jx = c * old_jx - s * jy; jy = s * old_jx + c * jy;
                break;
            }
            case SDF_OP_REPEAT: {
                float3 period = max(abs(data.xyz), float3(1.0e-5f));
                p = p - period * floor(p / period + 0.5f);
                break;
            }
            case SDF_OP_SORT_DESC:
                if (p.x < p.z) { p.xz = p.zx; float3 swap = jx; jx = jz; jz = swap; }
                if (p.y < p.z) { p.yz = p.zy; float3 swap = jy; jy = jz; jz = swap; }
                if (p.x < p.y) { p.xy = p.yx; float3 swap = jx; jx = jy; jy = swap; }
                break;
            case SDF_OP_SPHERE:
            case SDF_OP_BOX:
            case SDF_OP_PLANE: {
                float scale = max(distance_scale, 1.0e-6f);
                float candidate;
                float3 local_gradient;
                if (op.opcode == SDF_OP_SPHERE) {
                    float radius = length(p);
                    candidate = (radius - data.x) / scale;
                    local_gradient = radius > 1.0e-8f ? p / radius : float3(0.0f, 1.0f, 0.0f);
                } else if (op.opcode == SDF_OP_BOX) {
                    float3 q = abs(p) - abs(data.xyz);
                    float3 outside = max(q, float3(0.0f));
                    float outside_length = length(outside);
                    candidate = (min(max(q.x, max(q.y, q.z)), 0.0f) + outside_length) / scale;
                    if (outside_length > 1.0e-8f) {
                        local_gradient = sign(p) * outside / outside_length;
                    } else if (q.x >= q.y && q.x >= q.z) {
                        local_gradient = float3(sign(p.x), 0.0f, 0.0f);
                    } else if (q.y >= q.z) {
                        local_gradient = float3(0.0f, sign(p.y), 0.0f);
                    } else {
                        local_gradient = float3(0.0f, 0.0f, sign(p.z));
                    }
                } else {
                    local_gradient = normalize(data.xyz);
                    candidate = (dot(p, local_gradient) + data.w) / scale;
                }
                float3 candidate_gradient = (local_gradient.x * jx + local_gradient.y * jy + local_gradient.z * jz) / scale;
                bool choose_candidate = !has_primitive ||
                    (op.flags == 1u ? candidate > surface.distance :
                     (op.flags == 2u ? -candidate > surface.distance : candidate < surface.distance));
                if (choose_candidate) {
                    surface.distance = op.flags == 2u && has_primitive ? -candidate : candidate;
                    surface.gradient = op.flags == 2u && has_primitive ? -candidate_gradient : candidate_gradient;
                }
                has_primitive = true;
                break;
            }
            default:
                break;
        }
    }
    return surface;
}

#if defined(FPT_STITCH_HOST)
[[stitchable]] ProgramSurface fpt_stitch_interpreted_surface(
    float3 source,
    constant FptRenderConfig &cfg) {
    return programSurfaceInterpreted(source, cfg);
}
#endif

#if defined(FPT_TOPOLOGY_GENERATED_SURFACE) || defined(FPT_TOPOLOGY_DUAL_SURFACE)
static __attribute__((noinline)) ProgramSurface deTopologySpecializedSurface(
    float3 source,
    constant FptRenderConfig &cfg) {
    // FPT_TOPOLOGY_SPECIALIZED_SURFACE_BODY
    return programSurfaceInterpreted(source, cfg);
}
#endif

static ProgramSurface programSurface(float3 source, constant FptRenderConfig &cfg) {
#if defined(FPT_TOPOLOGY_RUNTIME_SOURCE)
#if defined(FPT_TOPOLOGY_DUAL_SURFACE)
    if (fptTopologyUseGeneratedSurface) {
        return deTopologySpecializedSurface(source, cfg);
    }
    return programSurfaceInterpreted(source, cfg);
#elif defined(FPT_TOPOLOGY_GENERATED_SURFACE)
    return deTopologySpecializedSurface(source, cfg);
#elif defined(FPT_TOPOLOGY_CANONICAL_RUNTIME_SURFACE)
    return canonicalSurface(source, cfg);
#else
    return programSurfaceInterpreted(source, cfg);
#endif
#else
    if (typedSoAPrimitiveCount(cfg) > 0u) {
        return typedSoASurface(source, cfg);
    }
    if (cfg.sdf_canonical_count > 0u) {
        return canonicalSurface(source, cfg);
    }
#if defined(FPT_STITCH_HOST)
    if (cfg.sdf_function_stitching != 0u && cfg.sdf_stitched_surface != 0u) {
        return deTopologyStitchedSurface(source, cfg);
    }
#endif
    return programSurfaceInterpreted(source, cfg);
#endif
}

static float3 programGradient(float value, constant FptRenderConfig &cfg) {
    uint count = min(cfg.gradient_count, FPT_SDF_GRADIENT_MAX_STOPS);
    if (count == 0u) return float3(cfg.program_material[0], cfg.program_material[1], cfg.program_material[2]);
    if (count == 1u) return float3(cfg.gradient_stops[0][1], cfg.gradient_stops[0][2], cfg.gradient_stops[0][3]);
    float t = fract(value);
    float4 previous = float4(cfg.gradient_stops[0][0], cfg.gradient_stops[0][1], cfg.gradient_stops[0][2], cfg.gradient_stops[0][3]);
    for (uint i = 1u; i < count; ++i) {
        float4 current = float4(cfg.gradient_stops[i][0], cfg.gradient_stops[i][1], cfg.gradient_stops[i][2], cfg.gradient_stops[i][3]);
        if (t <= current.x) {
            float f = clamp((t - previous.x) / max(current.x - previous.x, 1.0e-6f), 0.0f, 1.0f);
            return mix(previous.yzw, current.yzw, f);
        }
        previous = current;
    }
    return previous.yzw;
}

static Material programMaterial(float orbit, constant FptRenderConfig &cfg) {
    Material material = defaultMaterial();
    if (cfg.material_mode == 1u) {
        material.rgb = hsv2rgb(float3(orbit * (1.0f + setv(cfg, 1)) + setv(cfg, 0), 0.5f, 1.0f));
    } else if (cfg.material_mode == 2u) {
        material.rgb = programGradient(orbit * (1.0f + setv(cfg, 1)) + setv(cfg, 0), cfg);
    } else {
        material.rgb = float3(cfg.program_material[0], cfg.program_material[1], cfg.program_material[2]);
    }
    material.roughness = cfg.program_material[3];
    material.specular = cfg.program_material[4];
    material.translucency = cfg.program_material[5];
    material.ior = cfg.program_material[6];
    material.emission = cfg.program_material[7];
    return material;
}

static Material fractalMaterial(float orbit, constant FptRenderConfig &cfg, bool menger) {
    Material m = defaultMaterial();
    if (cfg.fractal_style_mode != 0u) {
        float o = orbit * cfg.fractal_style[1] + cfg.fractal_style[0];
        if (cfg.fractal_style_mode == 1u) {
            m.rgb = hsv2rgb(float3(o, cfg.fractal_style[2], cfg.fractal_style[3]));
        } else if (cfg.fractal_style_mode == 2u) {
            m.rgb = float3(cfg.fractal_style[8], cfg.fractal_style[9], cfg.fractal_style[10]) * cfg.fractal_style[3];
        } else {
            m.rgb = programGradient(o, cfg) * cfg.fractal_style[3];
        }
        float3 solid = float3(cfg.fractal_style[8], cfg.fractal_style[9], cfg.fractal_style[10]);
        m.rgb = mix(m.rgb, solid, clamp(cfg.fractal_style[7], 0.0f, 1.0f));
        m.roughness = cfg.fractal_style[4];
        m.specular = cfg.fractal_style[5];
        m.emission = cfg.fractal_style[6];
        return m;
    }
    if (menger) {
        m.rgb = float3(1.0f);
        return m;
    }
    float o = orbit * (1.0f + setv(cfg, 1)) + setv(cfg, 0);
    if (cfg.sdf_id == SDF_CAGE_FRACTAL || cfg.sdf_id == SDF_IFS_FRACTAL || cfg.sdf_id == SDF_TREE_FRACTAL) {
        o = orbit * (1.0f + setv(cfg, 1)) / 5.0f + 0.3f + setv(cfg, 0);
    }
    if (cfg.sdf_id == SDF_TOWER_FRACTAL) {
        o = orbit * (1.0f + setv(cfg, 1)) + 0.3f + setv(cfg, 0);
    }
    m.rgb = hsv2rgb(float3(o, 0.5f, 1.0f));
    return m;
}

static SDFResult userSdf(float3 p, constant FptRenderConfig &cfg) {
    SDFResult r;
    r.material = defaultMaterial();
    r.distance = inf;
#if defined(FPT_MANDEL_SPECIALIZED_KERNEL)
    DeResult mandel_de = deMandelbulber(p, cfg);
    r.distance = mandel_de.d;
#if defined(FPT_MANDEL_GENERATED_MATERIAL)
    r.material = mandelbulberGeneratedMaterial(p, cfg);
#else
    r.material = fractalMaterial(mandel_de.orbit, cfg, false);
#endif
    return r;
#endif
    if (cfg.sdf_id == SDF_PROGRAM && cfg.sdf_program_count > 0u) {
        DeResult program = deShadingProgram(p, cfg);
        r.distance = program.d;
        r.material = programMaterial(program.orbit, cfg);
        return r;
    }
    if (cfg.sdf_id == SDF_README_CORNELL || cfg.sdf_id == SDF_README_GLASS) {
        SceneHit h = cfg.sdf_id == SDF_README_CORNELL ? readmeCornellScene(p) : readmeGlassScene(p);
        r.distance = h.d;
        Material white = defaultMaterial(); white.rgb = float3(0.78f, 0.80f, 0.75f); white.roughness = 0.92f; white.emission = 0.015f;
        Material red = defaultMaterial(); red.rgb = float3(0.62f, 0.055f, 0.030f); red.roughness = 0.95f; red.emission = 0.010f;
        Material green = defaultMaterial(); green.rgb = float3(0.035f, 0.82f, 0.09f); green.roughness = 0.92f; green.emission = 0.010f;
        Material light = defaultMaterial(); light.rgb = float3(1.0f, 0.94f, 0.82f); light.emission = 12.0f;
        Material sphere = defaultMaterial(); sphere.rgb = float3(0.72f, 0.78f, 0.74f); sphere.roughness = 0.24f; sphere.specular = 0.72f; sphere.emission = 0.012f;
        Material glass = defaultMaterial(); glass.rgb = float3(0.42f, 0.68f, 0.52f); glass.roughness = 0.025f; glass.specular = 1.0f; glass.translucency = 0.90f; glass.ior = 1.45f;
        Material pedestal = defaultMaterial(); pedestal.rgb = float3(0.84f, 0.88f, 0.86f); pedestal.roughness = 0.45f; pedestal.specular = 0.24f; pedestal.emission = 0.018f;
        r.material = h.id == 1 ? red : (h.id == 2 ? green : (h.id == 3 ? light : (h.id == 6 ? (cfg.sdf_id == SDF_README_GLASS ? glass : sphere) : (h.id == 7 ? pedestal : white))));
        return r;
    }
    if (cfg.sdf_id == SDF_CORNELL_BOX) {
        SceneHit h = cornellScene(p);
        r.distance = h.d;
        Material white = defaultMaterial(); white.rgb = float3(0.82f, 0.80f, 0.74f);
        Material red = defaultMaterial(); red.rgb = float3(0.82f, 0.12f, 0.08f);
        Material green = defaultMaterial(); green.rgb = float3(0.10f, 0.55f, 0.18f);
        Material light = defaultMaterial(); light.rgb = float3(1.0f, 0.88f, 0.64f); light.emission = 7.0f;
        Material tall = defaultMaterial(); tall.rgb = float3(0.72f, 0.70f, 0.64f); tall.roughness = 0.82f;
        Material shortm = defaultMaterial(); shortm.rgb = float3(0.70f, 0.68f, 0.62f); shortm.roughness = 0.92f;
        r.material = h.id == 1 ? red : (h.id == 2 ? green : (h.id == 3 ? light : (h.id == 4 ? tall : (h.id == 5 ? shortm : white))));
        return r;
    }
    if (cfg.sdf_id == SDF_GLASS_BALL) {
        SceneHit h = glassBallScene(p);
        r.distance = h.d;
        Material floor = defaultMaterial(); floor.rgb = float3(0.70f, 0.68f, 0.62f); floor.roughness = 0.72f; floor.specular = 0.08f;
        Material glass = defaultMaterial(); glass.rgb = float3(0.92f, 0.97f, 1.0f); glass.roughness = 0.02f; glass.specular = 1.0f; glass.translucency = 1.0f; glass.ior = 1.5f;
        Material back = defaultMaterial(); back.rgb = float3(0.48f, 0.58f, 0.72f); back.roughness = 0.85f;
        Material light = defaultMaterial(); light.rgb = float3(1.0f, 0.92f, 0.78f); light.emission = 5.5f;
        r.material = h.id == 1 ? glass : (h.id == 2 ? back : (h.id == 3 ? light : floor));
        return r;
    }
    DeResult de;
    bool menger = false;
    switch (cfg.sdf_id) {
        case SDF_BALL_FRACTAL: de = deBall(p, cfg); break;
        case SDF_CAGE_FRACTAL: de = deCage(p, cfg); break;
        case SDF_IFS_FRACTAL: de = deIFS(p, cfg); break;
        case SDF_MANDELBOX_FRACTAL: de = deMandelbox(p, cfg); break;
        case SDF_MENGER_SPONGE: de = deMenger(p, cfg); menger = true; break;
        case SDF_TOWER_FRACTAL: de = deTower(p, cfg); break;
        case SDF_TREE_FRACTAL: de = deTree(p, cfg); break;
        case SDF_MANDELBULBER: de = deMandelbulber(p, cfg); break;
        default: de.d = 1000.0f; de.orbit = 0.0f; break;
    }
    r.distance = de.d;
#if defined(FPT_MANDEL_GENERATED_MATERIAL)
    if (cfg.sdf_id == SDF_MANDELBULBER) {
        r.material = mandelbulberGeneratedMaterial(p, cfg);
        return r;
    }
#endif
    r.material = fractalMaterial(de.orbit, cfg, menger);
    return r;
}

static float distanceSdf(float3 p, constant FptRenderConfig &cfg) {
#if defined(FPT_BUILTIN_CAGE_ONLY)
    return deCage(p, cfg).d;
#elif defined(FPT_BUILTIN_TOWER_ONLY)
    return deTower(p, cfg).d;
#elif defined(FPT_MANDEL_SPECIALIZED_KERNEL)
    return deMandelbulber(p, cfg).d;
#else
    if (cfg.sdf_id == SDF_PROGRAM && cfg.sdf_program_count > 0u) {
#if defined(FPT_TOPOLOGY_RUNTIME_SOURCE)
        return deTopologySpecializedDistance(p, cfg);
#else
        if (typedSoAPrimitiveCount(cfg) > 0u) {
            return deTypedSoADistance(p, cfg);
        }
        if (cfg.sdf_canonical_count > 0u) {
            return deCanonicalDistance(p, cfg);
        }
        if (cfg.sdf_flat_union_count > 0u) {
            return deFlatUnionDistance(p, cfg);
        }
        if (cfg.sdf_function_stitching != 0u) {
            return deTopologyStitchedDistance(p, cfg);
        }
        if (cfg.sdf_topology_specialization != 0u) {
            return deTopologySpecializedDistance(p, cfg);
        }
        return deProgramDistance(p, cfg);
#endif
    }
    if (cfg.sdf_id == SDF_README_CORNELL) return readmeCornellScene(p).d;
    if (cfg.sdf_id == SDF_README_GLASS) return readmeGlassScene(p).d;
    if (cfg.sdf_id == SDF_CORNELL_BOX) return cornellScene(p).d;
    if (cfg.sdf_id == SDF_GLASS_BALL) return glassBallScene(p).d;
    switch (cfg.sdf_id) {
        case SDF_BALL_FRACTAL: return deBall(p, cfg).d;
        case SDF_CAGE_FRACTAL: return deCage(p, cfg).d;
        case SDF_IFS_FRACTAL: return deIFS(p, cfg).d;
        case SDF_MANDELBOX_FRACTAL: return deMandelbox(p, cfg).d;
        case SDF_MENGER_SPONGE: return deMenger(p, cfg).d;
        case SDF_TOWER_FRACTAL: return deTower(p, cfg).d;
        case SDF_TREE_FRACTAL: return deTree(p, cfg).d;
        case SDF_MANDELBULBER: return deMandelbulber(p, cfg).d;
        default: return 1000.0f;
    }
#endif
}

static float mapSdf(float3 p, constant FptRenderConfig &cfg) {
    float d = distanceSdf(p, cfg);
    return mix(d, d / 2.0f, cfg.render[6]);
}

static bool sdfHasTranslucentSurfaces(constant FptRenderConfig &cfg) {
#if defined(FPT_MANDEL_SPECIALIZED_KERNEL)
    return false;
#else
    return cfg.sdf_id == SDF_GLASS_BALL || cfg.sdf_id == SDF_README_GLASS ||
           (cfg.sdf_id == SDF_PROGRAM && cfg.program_material[5] > 0.0f);
#endif
}

static int sdfMarchIterationLimit(int requested,
                                  constant FptRenderConfig &cfg) {
#if defined(FPT_MANDEL_SPECIALIZED_KERNEL)
    return min(requested, 10000);
#else
    return min(requested, cfg.sdf_id == SDF_MANDELBULBER ? 10000 : 360);
#endif
}

static float mandelbulberMarchThreshold(float3 position,
                                        constant FptRenderConfig &cfg) {
    float threshold = cfg.vset_values[115] > 0.5f
        ? length(cameraPos(cfg) - position) * cfg.vset_values[116]
        : cfg.vset_values[117];
    return clamp(threshold, cfg.vset_values[118], cfg.vset_values[119]);
}

static float sdfMarchStep(float distance,
                          float threshold,
                          constant FptRenderConfig &cfg) {
#if defined(FPT_MANDEL_SPECIALIZED_KERNEL)
    float step = max(distance - 0.5f * threshold, 0.0f) *
                 cfg.vset_values[113];
    if (cfg.vset_values[108] > 0.5f) {
        step = clamp(step, cfg.vset_values[109], cfg.vset_values[110]);
        if (threshold > cfg.vset_values[109]) {
            step = clamp(step,
                         cfg.vset_values[111] * threshold,
                         cfg.vset_values[112] * threshold);
        }
    } else {
        step = min(step, cfg.vset_values[110]);
    }
    return step;
#else
    if (cfg.sdf_id == SDF_MANDELBULBER) {
        float step = max(distance - 0.5f * threshold, 0.0f) *
                     cfg.vset_values[113];
        if (cfg.vset_values[108] > 0.5f) {
            step = clamp(step, cfg.vset_values[109], cfg.vset_values[110]);
            if (threshold > cfg.vset_values[109]) {
                step = clamp(step,
                             cfg.vset_values[111] * threshold,
                             cfg.vset_values[112] * threshold);
            }
        } else {
            // The ordinary Mandelbulber marcher still caps every step at
            // three fractal units. Loose analytic estimators rely on this cap
            // to avoid jumping over compact 4D and Amazing Box surfaces.
            step = min(step, cfg.vset_values[110]);
        }
        return step;
    }
    return abs(distance) * 0.99f;
#endif
}

static bool sdfMarchConverged(float distance,
                              float step,
                              float threshold,
                              constant FptRenderConfig &cfg) {
#if defined(FPT_MANDEL_SPECIALIZED_KERNEL)
    return distance < threshold;
#else
    if (cfg.sdf_id == SDF_MANDELBULBER) return distance < threshold;
    return step < threshold;
#endif
}

struct MandelbulberMarchResult {
    float3 position;
    bool found;
};

static MandelbulberMarchResult marchMandelbulber(float3 direction,
                                                  float3 position,
                                                  int iteration_count,
                                                  constant FptRenderConfig &cfg) {
    float3 start = position;
    float distance = 0.0f;
    float threshold = mandelbulberMarchThreshold(position, cfg);
    float step = 0.0f;
    bool found = false;
    int maximum_iterations = sdfMarchIterationLimit(iteration_count, cfg);
    for (int iteration = 0; iteration < maximum_iterations; ++iteration) {
        threshold = mandelbulberMarchThreshold(position, cfg);
        distance = mapSdf(position, cfg);
        if (!isfinite(distance)) break;
        // Mandelbulber's full OpenCL ray recursion accepts the whole
        // distance < threshold band. The 0.95 factor is used only to bracket
        // the boundary during the subsequent refinement search.
        if (distance < threshold) {
            found = true;
            break;
        }
        step = sdfMarchStep(distance, threshold, cfg);
        float3 next_position = position + direction * step;
        // Once fp32 can no longer represent the requested displacement, every
        // remaining iteration evaluates the exact same point and returns the
        // same final position. Stop that provably redundant fixed-point loop.
        if (all(next_position == position)) break;
        position = next_position;
        if (length(position - start) >= cfg.render[4]) break;
    }
    if (!found) return MandelbulberMarchResult{position, false};

    // Mandelbulber's full OpenCL engine brackets the threshold boundary with
    // up to thirty half-steps. Its accepted band scales with detail_level.
    float search_limit = 1.0f - 0.001f * max(cfg.vset_values[129], 0.0f);
    step *= 0.5f;
    for (int refinement = 0; refinement < 30; ++refinement) {
        if (distance < threshold && distance > threshold * search_limit) break;
        if (distance > threshold) {
            float3 next_position = position + direction * step;
            if (all(next_position == position)) break;
            position = next_position;
        } else if (distance < threshold * search_limit) {
            float3 next_position = position - direction * step;
            if (all(next_position == position)) break;
            position = next_position;
        }
        distance = mapSdf(position, cfg);
        step *= 0.5f;
    }
    return MandelbulberMarchResult{position, true};
}

static float3 march(float3 dr, float3 rp, int ni, float min_dist, float lod_falloff, constant FptRenderConfig &cfg) {
#if defined(FPT_MANDEL_SPECIALIZED_KERNEL)
    return marchMandelbulber(dr, rp, ni, cfg).position;
#else
    if (cfg.sdf_id == SDF_MANDELBULBER) {
        return marchMandelbulber(dr, rp, ni, cfg).position;
    }
    float3 cam_pos = rp;
    int max_iter = sdfMarchIterationLimit(ni, cfg);
    bool check_translucency = sdfHasTranslucentSurfaces(cfg);
    for (int i = 0; i < max_iter; i++) {
        float d = mapSdf(rp, cfg);
        if (!isfinite(d)) break;
        float threshold = cfg.sdf_id == SDF_MANDELBULBER
            ? mandelbulberMarchThreshold(rp, cfg)
            : min_dist;
        float o = sdfMarchStep(d, threshold, cfg);
        rp += dr * o;
        float lod = threshold;
        if (lod_falloff > 0.00001f) {
            float fog_lod = dot(cam_pos - rp, cam_pos - rp);
            lod = mix(lod, 0.1f, fog_lod * lod_falloff);
            lod = mix(threshold, lod, cfg.render[5]);
        }
        if (check_translucency && userSdf(rp, cfg).material.translucency > 0.0f) lod = 0.0001f;
        if (sdfMarchConverged(d, o, lod, cfg)) break;
        if (cfg.render[4] < o) break;
    }
    return rp;
#endif
}

static float3 marchRegionalProgram(float3 direction,
                                   float3 position,
                                   int iteration_count,
                                   float minimum_distance,
                                   float lod_falloff,
                                   constant FptRenderConfig &cfg,
                                   device const ushort *program_ids,
                                   device const RegionalProgramHeader *program_headers,
                                   device const FptSdfInstruction *instruction_pool,
                                   device const ushort *primitive_pool) {
    float3 camera_position = position;
    int maximum_iterations = min(iteration_count, 360);
    bool check_translucency = sdfHasTranslucentSurfaces(cfg);
    for (int iteration = 0; iteration < maximum_iterations; ++iteration) {
        float distance = mapRegionalProgram(position, cfg, program_ids,
                                            program_headers, instruction_pool,
                                            primitive_pool);
        if (!isfinite(distance)) break;
        float step = abs(distance) * 0.99f;
        position += direction * step;
        float lod = minimum_distance;
        if (lod_falloff > 0.00001f) {
            float fog_lod = dot(camera_position - position,
                                camera_position - position);
            lod = mix(lod, 0.1f, fog_lod * lod_falloff);
            lod = mix(minimum_distance, lod, cfg.render[5]);
        }
        if (check_translucency &&
            userSdf(position, cfg).material.translucency > 0.0f) {
            lod = 0.0001f;
        }
        if (step < lod || cfg.render[4] < step) break;
    }
    return position;
}

static float3 marchRegionalProgramTracked(
    float3 direction,
    float3 position,
    int iteration_count,
    float minimum_distance,
    float lod_falloff,
    constant FptRenderConfig &cfg,
    device const ushort *program_ids,
    device const RegionalProgramHeader *program_headers,
    device const FptSdfInstruction *instruction_pool,
    device const ushort *primitive_pool,
    uint ray_class,
    thread RegionalProgramLocalStats &stats) {
    float3 camera_position = position;
    int maximum_iterations = min(iteration_count, 360);
    bool check_translucency = sdfHasTranslucentSurfaces(cfg);
    RegionalProgramLookupState lookup_state;
    lookup_state.valid = false;
    for (int iteration = 0; iteration < maximum_iterations; ++iteration) {
        float distance = mapRegionalProgramTracked(
            position, cfg, program_ids, program_headers, instruction_pool,
            primitive_pool,
            ray_class, lookup_state, stats);
        if (!isfinite(distance)) break;
        float step = abs(distance) * 0.99f;
        position += direction * step;
        float lod = minimum_distance;
        if (lod_falloff > 0.00001f) {
            float fog_lod = dot(camera_position - position,
                                camera_position - position);
            lod = mix(lod, 0.1f, fog_lod * lod_falloff);
            lod = mix(minimum_distance, lod, cfg.render[5]);
        }
        if (check_translucency &&
            userSdf(position, cfg).material.translucency > 0.0f) {
            lod = 0.0001f;
        }
        if (step < lod || cfg.render[4] < step) break;
    }
    return position;
}

static float3 normalizeFiniteDifference(float3 gradient) {
    float magnitude_scale = max(abs(gradient.x), max(abs(gradient.y), abs(gradient.z)));
    if (!(magnitude_scale > 0.0f) || !isfinite(magnitude_scale)) {
        return float3(0.0f, 1.0f, 0.0f);
    }
    return normalize(gradient / magnitude_scale);
}

static float3 normalAt(float3 p, constant FptRenderConfig &cfg) {
    if (cfg.sdf_id == SDF_MANDELBULBER && cfg.render[7] > 0.0f) {
        // Mandelbulber's slow-shading mode estimates the normal from the
        // iteration-count field rather than differentiating the DE. Preserve
        // its 11^3 binary central-difference stencil for presets which request
        // that deliberately smoother (and much more expensive) normal.
        float delta = length(cameraPos(cfg) - p) * cfg.render[7] *
                      cfg.vset_values[114];
        float max_iterations = cfg.set_values[1];
        float3 slow_normal = float3(0.0f);
        for (int ix = -5; ix <= 5; ++ix) {
            for (int iy = -5; iy <= 5; ++iy) {
                for (int iz = -5; iz <= 5; ++iz) {
                    float3 sample_direction = float3(float(ix), float(iy),
                                                     float(iz)) * 0.2f;
                    float4 sample = mandelbulberFieldSample(
                        p + sample_direction * delta, cfg, 1);
                    float pseudo_distance = 1.0f + max_iterations - abs(sample.w);
                    slow_normal += sample_direction * pseudo_distance;
                }
            }
        }
        float magnitude_scale = max(abs(slow_normal.x),
                                    max(abs(slow_normal.y), abs(slow_normal.z)));
        if (!(magnitude_scale > 0.0f) || !isfinite(magnitude_scale)) {
            return float3(1.0f, 0.0f, 0.0f);
        }
        return normalize(slow_normal / magnitude_scale);
    }
    float e = cfg.sdf_id == SDF_MANDELBULBER
        ? mandelbulberMarchThreshold(p, cfg) * cfg.vset_values[114]
        : max(cfg.render[2], 0.0002f);
    if ((cfg.sdf_normal_mode == 0u || cfg.sdf_normal_mode == 2u) && cfg.sdf_id == SDF_PROGRAM && cfg.sdf_program_count > 0u) {
        float3 gradient = programSurface(p, cfg).gradient;
        if (dot(gradient, gradient) >= 1.0e-12f && isfinite(gradient.x) && isfinite(gradient.y) && isfinite(gradient.z)) {
            return normalize(gradient);
        }
    }
    if (cfg.sdf_normal_mode == 1u) {
        // Match the radial sampling distance of the six-axis central stencil.
        // Each tetrahedral corner has length sqrt(3), so scale its offset down
        // by the reciprocal before evaluating the four-point gradient.
        float tetra_e = e * 0.57735026919f;
        float3 k0 = float3(1.0f, -1.0f, -1.0f);
        float3 k1 = float3(-1.0f, -1.0f, 1.0f);
        float3 k2 = float3(-1.0f, 1.0f, -1.0f);
        float3 k3 = float3(1.0f, 1.0f, 1.0f);
        float d0 = cfg.sdf_id == SDF_MANDELBULBER
            ? mandelbulberNormalDistance(p + k0 * tetra_e, cfg)
            : mapSdf(p + k0 * tetra_e, cfg);
        float d1 = cfg.sdf_id == SDF_MANDELBULBER
            ? mandelbulberNormalDistance(p + k1 * tetra_e, cfg)
            : mapSdf(p + k1 * tetra_e, cfg);
        float d2 = cfg.sdf_id == SDF_MANDELBULBER
            ? mandelbulberNormalDistance(p + k2 * tetra_e, cfg)
            : mapSdf(p + k2 * tetra_e, cfg);
        float d3 = cfg.sdf_id == SDF_MANDELBULBER
            ? mandelbulberNormalDistance(p + k3 * tetra_e, cfg)
            : mapSdf(p + k3 * tetra_e, cfg);
        float3 n4 = k0 * d0 + k1 * d1 + k2 * d2 + k3 * d3;
        return normalizeFiniteDifference(n4);
    }
    float3 x_offset = float3(e, 0.0f, 0.0f);
    float3 y_offset = float3(0.0f, e, 0.0f);
    float3 z_offset = float3(0.0f, 0.0f, e);
    float xp = cfg.sdf_id == SDF_MANDELBULBER
        ? mandelbulberNormalDistance(p + x_offset, cfg)
        : mapSdf(p + x_offset, cfg);
    float xn = cfg.sdf_id == SDF_MANDELBULBER
        ? mandelbulberNormalDistance(p - x_offset, cfg)
        : mapSdf(p - x_offset, cfg);
    float yp = cfg.sdf_id == SDF_MANDELBULBER
        ? mandelbulberNormalDistance(p + y_offset, cfg)
        : mapSdf(p + y_offset, cfg);
    float yn = cfg.sdf_id == SDF_MANDELBULBER
        ? mandelbulberNormalDistance(p - y_offset, cfg)
        : mapSdf(p - y_offset, cfg);
    float zp = cfg.sdf_id == SDF_MANDELBULBER
        ? mandelbulberNormalDistance(p + z_offset, cfg)
        : mapSdf(p + z_offset, cfg);
    float zn = cfg.sdf_id == SDF_MANDELBULBER
        ? mandelbulberNormalDistance(p - z_offset, cfg)
        : mapSdf(p - z_offset, cfg);
    float3 n = float3(xp - xn, yp - yn, zp - zn);
    if (cfg.sdf_id == SDF_MANDELBULBER &&
        !(max(abs(n.x), max(abs(n.y), abs(n.z))) > 0.0f)) {
        // Very deep analytic orbits can produce representable distances whose
        // six float samples nevertheless collapse to one value. The orbit
        // radius remains well-conditioned and has the same local level-set
        // orientation, so use its logarithmic gradient as a precision fallback
        // instead of returning a constant normal over the entire surface.
        n = float3(
            mandelbulberNormalOrbitRadius(p + x_offset, cfg) -
                mandelbulberNormalOrbitRadius(p - x_offset, cfg),
            mandelbulberNormalOrbitRadius(p + y_offset, cfg) -
                mandelbulberNormalOrbitRadius(p - y_offset, cfg),
            mandelbulberNormalOrbitRadius(p + z_offset, cfg) -
                mandelbulberNormalOrbitRadius(p - z_offset, cfg));
    }
    return normalizeFiniteDifference(n);
}

struct VoxelCell {
    uint packed_color;
    uint packed_properties;
    float emission;
};

struct VoxelBuildState {
    atomic_uint next_page;
    atomic_uint overflow;
    atomic_uint active_cells;
    atomic_uint rejected_bricks;
};

enum AccelerationCellClass : uint {
    AccelerationCellOutsideNoSurface = 0u,
    AccelerationCellInsideNoSurface = 1u,
    AccelerationCellPotentialSurface = 2u,
    AccelerationCellUnknown = 3u,
};

struct AccelerationCellClassification {
    AccelerationCellClass cell_class;
    float interval_min;
    float interval_max;
    uint certified;
    uint legacy_occupied;
    uint lipschitz_occupied;
    uint interval_occupied;
};

struct VoxelHit {
    bool hit;
    float distance;
    float exit_distance;
    float surface_distance;
    float3 position;
    float3 cell_center;
    float3 normal;
    Material material;
    uint steps;
};

static float3 voxelShadingNormal(VoxelHit hit, constant FptRenderConfig &cfg) {
    if (cfg.voxel_normal_mode == 1u) return normalAt(hit.cell_center, cfg);
    if (cfg.voxel_normal_mode == 2u) return normalAt(hit.position, cfg);
    return hit.normal;
}

static uint packVoxelUnorm4(float4 value) {
    uint4 q = uint4(round(clamp(value, 0.0f, 1.0f) * 255.0f));
    return q.x | (q.y << 8u) | (q.z << 16u) | (q.w << 24u);
}

static float4 unpackVoxelUnorm4(uint value) {
    return float4(value & 255u,
                  (value >> 8u) & 255u,
                  (value >> 16u) & 255u,
                  (value >> 24u) & 255u) / 255.0f;
}

static Material unpackVoxelMaterial(VoxelCell cell) {
    float4 color = unpackVoxelUnorm4(cell.packed_color);
    float4 properties = unpackVoxelUnorm4(cell.packed_properties);
    Material material = defaultMaterial();
    material.rgb = color.rgb;
    material.roughness = properties.x;
    material.specular = properties.y;
    material.translucency = properties.z;
    material.ior = mix(1.0f, 2.5f, properties.w);
    material.emission = cell.emission;
    return material;
}

static Material voxelMaterialAtHit(VoxelHit hit, constant FptRenderConfig &cfg) {
    if (cfg.voxel_material_mode == VOXEL_MATERIAL_EXACT ||
        cfg.voxel_storage == VOXEL_STORAGE_TEMPLATE_BRICKS) {
        return userSdf(hit.position, cfg).material;
    }
    return hit.material;
}

// Scale-aware ray-origin offset from Ray Tracing Gems. Away from the origin
// it moves a fixed number of representable floats; near zero it uses the
// corresponding absolute fallback. This avoids tying precision to voxel size.
static float3 precisionOffsetRayOrigin(float3 position, float3 normal) {
    const float origin = 1.0f / 32.0f;
    const float float_scale = 1.0f / 65536.0f;
    const float int_scale = 256.0f;
    float3 result;
    for (uint axis = 0u; axis < 3u; ++axis) {
        int offset = int(int_scale * normal[axis]);
        int bits = as_type<int>(position[axis]);
        bits += position[axis] < 0.0f ? -offset : offset;
        float shifted = as_type<float>(bits);
        result[axis] = abs(position[axis]) < origin
            ? position[axis] + float_scale * normal[axis]
            : shifted;
    }
    return result;
}

static float3 offsetVoxelRayOrigin(float3 position,
                                   float3 normal,
                                   float3 direction,
                                   constant FptRenderConfig &cfg) {
    float side = dot(direction, normal) >= 0.0f ? 1.0f : -1.0f;
    float3 directed_normal = normal * side;
    if (cfg.voxel_offset_mode == VOXEL_OFFSET_PRECISION) {
        return precisionOffsetRayOrigin(position, directed_normal);
    }
    float3 cell_size = (float3(cfg.voxel_bounds_max[0], cfg.voxel_bounds_max[1],
                               cfg.voxel_bounds_max[2]) -
                        float3(cfg.voxel_bounds_min[0], cfg.voxel_bounds_min[1],
                               cfg.voxel_bounds_min[2])) /
                       float(max(cfg.voxel_resolution, 1u));
    float epsilon = min(cell_size.x, min(cell_size.y, cell_size.z)) * 0.01f;
    return position + directed_normal * epsilon;
}
static uint voxelIndex(uint3 cell, uint resolution) {
    return cell.x + cell.y * resolution + cell.z * resolution * resolution;
}

static float3 voxelBoundsMin(constant FptRenderConfig &cfg) {
    return float3(cfg.voxel_bounds_min[0], cfg.voxel_bounds_min[1], cfg.voxel_bounds_min[2]);
}

static float3 voxelBoundsMax(constant FptRenderConfig &cfg) {
    return float3(cfg.voxel_bounds_max[0], cfg.voxel_bounds_max[1], cfg.voxel_bounds_max[2]);
}

static float3 voxelCellSize(constant FptRenderConfig &cfg) {
    return (voxelBoundsMax(cfg) - voxelBoundsMin(cfg)) / float(max(cfg.voxel_resolution, 1u));
}

static bool voxelRayAabb(float3 origin,
                         float3 direction,
                         float3 bounds_min,
                         float3 bounds_max,
                         thread float &near_t,
                         thread float &far_t,
                         thread float3 &entry_normal) {
    near_t = -inf;
    far_t = inf;
    entry_normal = float3(0.0f);
    for (uint axis = 0u; axis < 3u; ++axis) {
        if (direction[axis] == 0.0f) {
            if (origin[axis] < bounds_min[axis] || origin[axis] > bounds_max[axis]) return false;
            continue;
        }
        float a = (bounds_min[axis] - origin[axis]) / direction[axis];
        float b = (bounds_max[axis] - origin[axis]) / direction[axis];
        float axis_near = min(a, b);
        float axis_far = max(a, b);
        if (axis_near > near_t) {
            near_t = axis_near;
            entry_normal = float3(0.0f);
            entry_normal[axis] = direction[axis] > 0.0f ? -1.0f : 1.0f;
        }
        far_t = min(far_t, axis_far);
        if (near_t > far_t) return false;
    }
    float entry_t = max(near_t, 0.0f);
    return isfinite(entry_t) && isfinite(far_t) && far_t >= entry_t;
}

// Direction-aware half-open ownership prevents an entry point on a grid
// boundary from selecting the cell the ray is leaving. Zero-direction axes
// use an inclusive slab and clamp the volume's maximum face to the last cell.
static inline bool voxelCellCoordinate(float3 point,
                                       float3 direction,
                                       float3 bounds_min,
                                       float3 bounds_max,
                                       float3 cell_size,
                                       uint resolution,
                                       thread int3 &cell) {
    if (any(point < bounds_min) || any(point > bounds_max)) return false;
    bool3 negative = direction < 0.0f;
    bool3 positive = direction > 0.0f;
    if (any((negative & (point <= bounds_min)) | (positive & (point >= bounds_max)))) return false;
    float3 grid_position = (point - bounds_min) / cell_size;
    float3 floored = floor(grid_position);
    bool3 leaving_cell_boundary = negative & (grid_position == floored);
    cell = int3(floored) - select(int3(0), int3(1), leaving_cell_boundary);
    cell = min(cell, int3(int(resolution) - 1));
    return !any(cell < int3(0)) && !any(cell >= int3(int(resolution)));
}

// Metal does not expose nextafter on every deployment target. Traversal t is
// non-negative, so incrementing the IEEE-754 bit pattern gives the next
// representable value (including the smallest subnormal after either zero).
static inline float voxelNextPositiveFloat(float value) {
    if (!isfinite(value) || value < 0.0f) return inf;
    if (value == 0.0f) return as_type<float>(1u);
    return as_type<float>(as_type<uint>(value) + 1u);
}

static inline bool voxelAdvanceDda(thread int3 &cell,
                                   int3 step_direction,
                                   thread float3 &next_t,
                                   float3 delta_t,
                                   thread float &entry_t,
                                   thread float3 &entry_normal) {
    float boundary_t = min(next_t.x, min(next_t.y, next_t.z));
    if (!isfinite(boundary_t)) return false;
    bool3 crossed = next_t == float3(boundary_t);
    float next_entry_t = boundary_t;
    if (!(next_entry_t > entry_t)) {
        next_entry_t = voxelNextPositiveFloat(entry_t);
        if (!isfinite(next_entry_t) || !(next_entry_t > entry_t)) return false;
    }
    entry_t = next_entry_t;
    entry_normal = float3(0.0f);
    uint normal_axis = crossed.x ? 0u : (crossed.y ? 1u : 2u);
    entry_normal[normal_axis] = step_direction[normal_axis] > 0 ? -1.0f : 1.0f;
    cell += select(int3(0), step_direction, crossed);
    next_t += select(float3(0.0f), delta_t, crossed);
    return true;
}

// Card 4 establishes the classifier boundary without claiming that every
// fractal distance estimator is a certified signed distance. The current
// policy preserves legacy occupancy exactly and reports all rejected cells as
// Unknown; later policies can safely return certified outside/inside states.
static inline AccelerationCellClassification classifyAccelerationCell(
    float3 center,
    float3 half_extent,
    constant FptRenderConfig &cfg) {
    AccelerationCellClassification result;
    float distance = distanceSdf(center, cfg);
    float radius = length(half_extent);
    result.interval_min = distance - radius;
    result.interval_max = distance + radius;
    result.certified = 0u;
    bool occupied = isfinite(distance) &&
        abs(distance) <= radius * max(cfg.voxel_surface_band, 0.25f);
    if (cfg.voxel_fill_interior != 0u && isfinite(distance) && distance < 0.0f) {
        occupied = true;
    }
    result.legacy_occupied = occupied ? 1u : 0u;
    result.lipschitz_occupied = result.legacy_occupied;
    result.interval_occupied = result.legacy_occupied;
    result.cell_class = AccelerationCellUnknown;
    if (!isfinite(distance)) {
        result.interval_min = -inf;
        result.interval_max = inf;
    }
    if ((cfg.sdf_id == SDF_PROGRAM && cfg.sdf_program_count > 0u) ||
        cfg.sdf_id == SDF_CAGE_FRACTAL) {
        ProgramBounds bounds = accelerationProgramBounds(center, half_extent, cfg);
        if (bounds.certified != 0u) {
            result.interval_min = bounds.minimum;
            result.interval_max = bounds.maximum;
            result.certified = 1u;
            bool fill_interior = cfg.voxel_fill_interior != 0u &&
                isfinite(distance) && distance < 0.0f;
            result.lipschitz_occupied =
                (isfinite(distance) && abs(distance) <= bounds.lipschitz * radius) ||
                fill_interior ? 1u : 0u;
            result.interval_occupied =
                (bounds.minimum <= 0.0f && bounds.maximum >= 0.0f) ||
                fill_interior ? 1u : 0u;
            if (bounds.minimum > 0.0f) {
                result.cell_class = AccelerationCellOutsideNoSurface;
            } else if (bounds.maximum < 0.0f) {
                result.cell_class = AccelerationCellInsideNoSurface;
            } else {
                result.cell_class = AccelerationCellPotentialSurface;
            }
        }
    }
    return result;
}

static bool programBoundsContainGridSamples(float3 center,
                                            float3 half_extent,
                                            constant FptRenderConfig &cfg) {
    ProgramBounds bounds = programBounds(center, half_extent, cfg);
    if (bounds.certified == 0u || !isfinite(bounds.minimum) ||
        !isfinite(bounds.maximum) || !isfinite(bounds.lipschitz)) return false;
    float tolerance = 2.0e-5f;
    for (uint z = 0u; z < 3u; ++z) {
        for (uint y = 0u; y < 3u; ++y) {
            for (uint x = 0u; x < 3u; ++x) {
                float3 unit = float3(float(x), float(y), float(z)) - 1.0f;
                float distance = deProgramDistance(
                    center + unit * half_extent, cfg);
                if (!isfinite(distance) || distance < bounds.minimum - tolerance ||
                    distance > bounds.maximum + tolerance) return false;
            }
        }
    }
    return true;
}

static bool programDerivativeBoundsContainGridSamples(
    float3 center,
    float3 half_extent,
    constant FptRenderConfig &cfg) {
    ProgramDerivativeBounds bounds = programDerivativeBounds(center, half_extent,
                                                             cfg);
    if (bounds.certified == 0u || any(!isfinite(bounds.lower)) ||
        any(!isfinite(bounds.upper))) return false;
    float tolerance = 5.0e-4f;
    for (uint z = 0u; z < 5u; ++z) {
        for (uint y = 0u; y < 5u; ++y) {
            for (uint x = 0u; x < 5u; ++x) {
                float3 unit = float3(float(x), float(y), float(z)) * 0.5f - 1.0f;
                float3 gradient = programSurface(center + unit * half_extent,
                                                 cfg).gradient;
                if (any(!isfinite(gradient)) ||
                    any(gradient < bounds.lower - tolerance) ||
                    any(gradient > bounds.upper + tolerance)) return false;
            }
        }
    }
    return true;
}

static bool accelerationBoundsContainGridSamples(float3 center,
                                                 float3 half_extent,
                                                 constant FptRenderConfig &cfg) {
    ProgramBounds bounds = accelerationProgramBounds(center, half_extent, cfg);
    if (bounds.certified == 0u || !isfinite(bounds.minimum) ||
        !isfinite(bounds.maximum)) return false;
    float tolerance = 2.0e-4f *
        (1.0f + max(abs(bounds.minimum), abs(bounds.maximum)));
    for (uint z = 0u; z < 5u; ++z) {
        for (uint y = 0u; y < 5u; ++y) {
            for (uint x = 0u; x < 5u; ++x) {
                float3 unit = float3(float(x), float(y), float(z)) * 0.5f - 1.0f;
                float distance = distanceSdf(center + unit * half_extent, cfg);
                if (!isfinite(distance) || distance < bounds.minimum - tolerance ||
                    distance > bounds.maximum + tolerance) return false;
            }
        }
    }
    return true;
}

// Find the first signed zero crossing inside one occupied voxel. The coarse
// scan makes the bracket explicit; refinement alternates safeguarded secant
// and bisection steps so every procedural query remains inside the leaf.
static bool refineVoxelLeafSecantBisection(float3 origin,
                                           float3 direction,
                                           float entry_t,
                                           float exit_t,
                                           constant FptRenderConfig &cfg,
                                           thread float &refined_t) {
    if (!isfinite(entry_t) || !isfinite(exit_t) || !(exit_t > entry_t)) return false;

    const uint scan_steps = 4u;
    float previous_t = entry_t;
    float previous_d = mapSdf(origin + direction * previous_t, cfg);
    if (!isfinite(previous_d)) return false;
    const float zero_tolerance = 1.0e-6f;
    if (abs(previous_d) <= zero_tolerance) {
        refined_t = previous_t;
        return true;
    }

    float lower_t = entry_t;
    float upper_t = exit_t;
    float lower_d = previous_d;
    float upper_d = previous_d;
    bool bracketed = false;
    for (uint step = 1u; step <= scan_steps; ++step) {
        float current_t = mix(entry_t, exit_t, float(step) / float(scan_steps));
        float current_d = mapSdf(origin + direction * current_t, cfg);
        if (!isfinite(current_d)) return false;
        if (abs(current_d) <= zero_tolerance) {
            refined_t = current_t;
            return true;
        }
        if (signbit(previous_d) != signbit(current_d)) {
            lower_t = previous_t;
            lower_d = previous_d;
            upper_t = current_t;
            upper_d = current_d;
            bracketed = true;
            break;
        }
        previous_t = current_t;
        previous_d = current_d;
    }
    if (!bracketed) return false;

    for (uint iteration = 0u; iteration < 8u; ++iteration) {
        float candidate_t = 0.5f * (lower_t + upper_t);
        if ((iteration & 1u) == 0u) {
            float denominator = upper_d - lower_d;
            if (isfinite(denominator) && abs(denominator) > 1.0e-12f) {
                float secant_t = (lower_t * upper_d - upper_t * lower_d) / denominator;
                float guard = (upper_t - lower_t) * 0.1f;
                if (secant_t > lower_t + guard && secant_t < upper_t - guard) {
                    candidate_t = secant_t;
                }
            }
        }
        float candidate_d = mapSdf(origin + direction * candidate_t, cfg);
        if (!isfinite(candidate_d)) return false;
        if (abs(candidate_d) <= zero_tolerance) {
            refined_t = candidate_t;
            return true;
        }
        if (signbit(lower_d) != signbit(candidate_d)) {
            upper_t = candidate_t;
            upper_d = candidate_d;
        } else {
            lower_t = candidate_t;
            lower_d = candidate_d;
        }
    }

    refined_t = abs(lower_d) <= abs(upper_d) ? lower_t : upper_t;
    refined_t = clamp(refined_t, entry_t, exit_t);
    return true;
}

// Sphere-trace only certified typed programs, divide steps by the proven
// Lipschitz bound, and refuse to cross the voxel exit. A miss falls back to
// the original voxel entry hit rather than inventing geometry.
static bool refineVoxelLeafRestrictedTrace(float3 origin,
                                           float3 direction,
                                           float entry_t,
                                           float exit_t,
                                           float3 cell_center,
                                           float3 half_extent,
                                           constant FptRenderConfig &cfg,
                                           thread float &refined_t) {
    if (cfg.sdf_id != SDF_PROGRAM || !isfinite(entry_t) || !isfinite(exit_t) ||
        !(exit_t > entry_t)) return false;
    ProgramBounds bounds = programBounds(cell_center, half_extent, cfg);
    if (bounds.certified == 0u || !isfinite(bounds.lipschitz)) return false;

    float t = entry_t;
    float tolerance = max(1.0e-6f, (exit_t - entry_t) * 1.0e-4f);
    float inverse_lipschitz = 1.0f / max(bounds.lipschitz, 1.0f);
    for (uint iteration = 0u; iteration < 12u; ++iteration) {
        float distance = mapSdf(origin + direction * t, cfg);
        if (!isfinite(distance)) return false;
        if (abs(distance) <= tolerance) {
            refined_t = clamp(t, entry_t, exit_t);
            return true;
        }
        float step_length = max(abs(distance) * inverse_lipschitz * 0.99f,
                                tolerance * 0.5f);
        float next_t = t + step_length;
        if (!(next_t < exit_t)) {
            float exit_distance = mapSdf(origin + direction * exit_t, cfg);
            if (isfinite(exit_distance) && abs(exit_distance) <= tolerance) {
                refined_t = exit_t;
                return true;
            }
            return false;
        }
        t = next_t;
    }
    return false;
}

// Fractal estimators are generally unsigned, so sign bracketing cannot locate
// their surface. Sample a fixed budget across the voxel interval, then perform
// three bounded local reductions around the best procedural DE value. Accept
// only a renderer-scale near-surface point; otherwise preserve the voxel hit.
static bool refineVoxelLeafFixedDe(float3 origin,
                                   float3 direction,
                                   float entry_t,
                                   float exit_t,
                                   constant FptRenderConfig &cfg,
                                   thread float &refined_t) {
    bool fractal = cfg.sdf_id >= SDF_BALL_FRACTAL && cfg.sdf_id <= SDF_TREE_FRACTAL;
    if (!fractal || !isfinite(entry_t) || !isfinite(exit_t) || !(exit_t > entry_t)) {
        return false;
    }

    const uint scan_steps = 12u;
    float span = exit_t - entry_t;
    float scan_step = span / float(scan_steps);
    float best_t = entry_t;
    float best_distance = inf;
    for (uint sample = 0u; sample <= scan_steps; ++sample) {
        float t = entry_t + float(sample) * scan_step;
        float distance = abs(mapSdf(origin + direction * t, cfg));
        if (!isfinite(distance)) return false;
        if (distance < best_distance) {
            best_distance = distance;
            best_t = t;
        }
    }

    float lower_t = max(entry_t, best_t - scan_step);
    float upper_t = min(exit_t, best_t + scan_step);
    for (uint iteration = 0u; iteration < 3u; ++iteration) {
        float left_t = mix(lower_t, upper_t, 1.0f / 3.0f);
        float right_t = mix(lower_t, upper_t, 2.0f / 3.0f);
        float left_distance = abs(mapSdf(origin + direction * left_t, cfg));
        float right_distance = abs(mapSdf(origin + direction * right_t, cfg));
        if (!isfinite(left_distance) || !isfinite(right_distance)) return false;
        if (left_distance < best_distance) {
            best_distance = left_distance;
            best_t = left_t;
        }
        if (right_distance < best_distance) {
            best_distance = right_distance;
            best_t = right_t;
        }
        if (left_distance <= right_distance) {
            upper_t = right_t;
        } else {
            lower_t = left_t;
        }
    }

    float acceptance = max(max(cfg.render[3], 1.0e-5f), span / 64.0f);
    if (best_distance > acceptance) return false;
    refined_t = clamp(best_t, entry_t, exit_t);
    return true;
}

static VoxelHit traceVoxel(float3 origin,
                           float3 direction,
                           constant FptRenderConfig &cfg,
                           device const VoxelCell *cells,
                           device const uint *page_table) {
    VoxelHit result;
    result.hit = false;
    result.distance = inf;
    result.exit_distance = inf;
    result.surface_distance = inf;
    result.steps = 0u;
    uint resolution = max(cfg.voxel_resolution, 1u);
    float3 bounds_min = voxelBoundsMin(cfg);
    float3 bounds_max = voxelBoundsMax(cfg);
    float3 cell_size = voxelCellSize(cfg);
    float near_t;
    float far_t;
    float3 entry_normal;
    if (!voxelRayAabb(origin, direction, bounds_min, bounds_max, near_t, far_t, entry_normal)) return result;

    float entry_t = max(near_t, 0.0f);
    float3 start = clamp(origin + direction * entry_t, bounds_min, bounds_max);
    int3 cell = int3(0);
    if (!voxelCellCoordinate(start, direction, bounds_min, bounds_max,
                             cell_size, resolution, cell)) return result;
    int3 step_direction = int3(0);
    float3 next_t = float3(inf);
    float3 delta_t = float3(inf);
    for (uint axis = 0u; axis < 3u; ++axis) {
        if (direction[axis] > 0.0f) {
            step_direction[axis] = 1;
            float boundary = bounds_min[axis] + float(cell[axis] + 1) * cell_size[axis];
            next_t[axis] = (boundary - origin[axis]) / direction[axis];
            delta_t[axis] = cell_size[axis] / direction[axis];
        } else if (direction[axis] < 0.0f) {
            step_direction[axis] = -1;
            float boundary = bounds_min[axis] + float(cell[axis]) * cell_size[axis];
            next_t[axis] = (boundary - origin[axis]) / direction[axis];
            delta_t[axis] = -cell_size[axis] / direction[axis];
        }
        if (!isfinite(next_t[axis]) || !isfinite(delta_t[axis])) {
            next_t[axis] = inf;
            delta_t[axis] = inf;
        }
    }

    uint max_steps = resolution * 3u + 3u;
    for (uint step = 0u; step < max_steps && entry_t <= far_t; ++step) {
        if (any(cell < int3(0)) || any(cell >= int3(int(resolution)))) break;
        uint3 coordinate = uint3(cell);
        VoxelCell voxel;
        voxel.packed_color = 0u;
        voxel.packed_properties = 0u;
        voxel.emission = 0.0f;
        uint sparse_page = 0u;
        if (cfg.voxel_storage == VOXEL_STORAGE_TEMPLATE_BRICKS) {
            uint brick_grid = (resolution + 3u) >> 2u;
            uint3 brick = coordinate >> 2u;
            uint page = page_table[brick.x + brick.y * brick_grid +
                                   brick.z * brick_grid * brick_grid];
            sparse_page = page;
            if (page != 0u) {
                uint3 local = coordinate & 3u;
                uint local_index = local.x + local.y * 4u + local.z * 16u;
                device const ulong *templates =
                    reinterpret_cast<device const ulong *>(cells);
                if ((templates[page - 1u] & (1ul << local_index)) != 0ul) {
                    voxel.packed_color = 0x80000000u;
                }
            }
        } else if (cfg.voxel_storage != VOXEL_STORAGE_DENSE) {
            uint brick_grid = (resolution + 3u) >> 2u;
            uint3 brick = coordinate >> 2u;
            uint page = page_table[brick.x + brick.y * brick_grid + brick.z * brick_grid * brick_grid];
            sparse_page = page;
            if (page != 0u) {
                uint3 local = coordinate & 3u;
                uint local_index = local.x + local.y * 4u + local.z * 16u;
                voxel = cells[(page - 1u) * 64u + local_index];
            }
        } else {
            voxel = cells[voxelIndex(coordinate, resolution)];
        }
        if ((voxel.packed_color & 0x80000000u) != 0u) {
            result.hit = true;
            result.distance = entry_t;
            result.exit_distance = min(next_t.x, min(next_t.y, next_t.z));
            result.surface_distance = entry_t;
            result.position = origin + direction * entry_t;
            result.cell_center = bounds_min + (float3(coordinate) + 0.5f) * cell_size;
            float refined_t = entry_t;
            if (cfg.voxel_leaf_refinement == VOXEL_LEAF_REFINEMENT_SECANT_BISECTION) {
                if (refineVoxelLeafSecantBisection(origin, direction, entry_t,
                                                    result.exit_distance, cfg, refined_t)) {
                    result.surface_distance = refined_t;
                    result.position = origin + direction * refined_t;
                }
            } else if (cfg.voxel_leaf_refinement == VOXEL_LEAF_REFINEMENT_RESTRICTED_TRACE) {
                if (refineVoxelLeafRestrictedTrace(origin, direction, entry_t,
                                                   result.exit_distance, result.cell_center,
                                                   cell_size * 0.5f, cfg, refined_t)) {
                    result.surface_distance = refined_t;
                    result.position = origin + direction * refined_t;
                }
            } else if (cfg.voxel_leaf_refinement == VOXEL_LEAF_REFINEMENT_FIXED_DE) {
                if (refineVoxelLeafFixedDe(origin, direction, entry_t,
                                           result.exit_distance, cfg, refined_t)) {
                    result.surface_distance = refined_t;
                    result.position = origin + direction * refined_t;
                }
            }
            result.normal = entry_normal;
            result.material = unpackVoxelMaterial(voxel);
            result.steps = step + 1u;
            return result;
        }
        if (!voxelAdvanceDda(cell, step_direction, next_t, delta_t,
                             entry_t, entry_normal)) break;
    }
    return result;
}

struct BoundGridTraceResult {
    bool hit;
    float distance;
    float3 position;
};

enum BoundGridRayClass : uint {
    BoundGridRayPrimary = 0u,
    BoundGridRayShadow = 1u,
    BoundGridRaySecondary = 2u,
};

struct BoundGridLocalStats {
    uint macro_cells[3];
    uint certified_skips[3];
    uint candidate_intervals[3];
    uint candidate_misses[3];
    uint candidate_hits[3];
    uint unknown_intervals[3];
    uint field_evaluations[3];
    uint directional_steps[3];
    uint cell_exit_clamps[3];
    uint unknown_derivative_intervals[3];
    uint profiled_paths;
};

static BoundGridLocalStats emptyBoundGridLocalStats() {
    BoundGridLocalStats stats;
    for (uint ray_class = 0u; ray_class < 3u; ++ray_class) {
        stats.macro_cells[ray_class] = 0u;
        stats.certified_skips[ray_class] = 0u;
        stats.candidate_intervals[ray_class] = 0u;
        stats.candidate_misses[ray_class] = 0u;
        stats.candidate_hits[ray_class] = 0u;
        stats.unknown_intervals[ray_class] = 0u;
        stats.field_evaluations[ray_class] = 0u;
        stats.directional_steps[ray_class] = 0u;
        stats.cell_exit_clamps[ray_class] = 0u;
        stats.unknown_derivative_intervals[ray_class] = 0u;
    }
    stats.profiled_paths = 0u;
    return stats;
}

// Run the existing distance-estimator march inside one candidate ray interval.
// A miss is not promoted to a surface hit: the macro traversal resumes at the
// interval exit, which is the semantic distinction from voxel leaf refinement.
static BoundGridTraceResult traceProceduralClipped(float3 origin,
                                                   float3 direction,
                                                   float entry_t,
                                                   float exit_t,
                                                   float3 lod_origin,
                                                   int min_iterations,
                                                   float min_distance,
                                                   float lod_falloff,
                                                   constant FptRenderConfig &cfg,
                                                   thread uint &remaining_steps,
                                                   uint ray_class,
                                                   thread BoundGridLocalStats &stats) {
    BoundGridTraceResult result;
    result.hit = false;
    result.distance = inf;
    result.position = origin + direction * exit_t;
    if (!(exit_t >= entry_t) || remaining_steps == 0u) return result;

    float t = max(entry_t, 0.0f);
    bool check_translucency = sdfHasTranslucentSurfaces(cfg);
    uint interval_budget = min(remaining_steps, uint(max(min_iterations, 0)));
    for (uint iteration = 0u; iteration < interval_budget && t <= exit_t; ++iteration) {
        float3 sample_position = origin + direction * t;
        float distance = mapSdf(sample_position, cfg);
        stats.field_evaluations[ray_class] += 1u;
        remaining_steps -= 1u;
        if (!isfinite(distance)) return result;
        float step_length = abs(distance) * 0.99f;
        float next_t = t + step_length;
        if (!(next_t >= t) || next_t > exit_t) return result;
        float3 position = origin + direction * next_t;
        float lod = min_distance;
        if (lod_falloff > 0.00001f) {
            float fog_lod = dot(lod_origin - position, lod_origin - position);
            lod = mix(lod, 0.1f, fog_lod * lod_falloff);
            lod = mix(min_distance, lod, cfg.render[5]);
        }
        if (check_translucency && userSdf(position, cfg).material.translucency > 0.0f) {
            lod = 0.0001f;
        }
        if (step_length < lod) {
            result.hit = true;
            result.distance = next_t;
            result.position = position;
            return result;
        }
        if (cfg.render[4] < step_length) return result;
        float progressed_t = t + max(step_length, 1.0e-7f);
        if (!(progressed_t > t) || progressed_t > exit_t) return result;
        t = progressed_t;
    }
    return result;
}

static float directionalDerivativeMagnitude(float3 direction,
                                            float3 derivative_lower,
                                            float3 derivative_upper) {
    float lower = 0.0f;
    float upper = 0.0f;
    for (uint axis = 0u; axis < 3u; ++axis) {
        float a = direction[axis] * derivative_lower[axis];
        float b = direction[axis] * derivative_upper[axis];
        lower += min(a, b);
        upper += max(a, b);
    }
    return max(abs(lower), abs(upper));
}

static void readDirectionalGridCell(
    uint3 coordinate,
    constant FptRenderConfig &cfg,
    texture3d<float, access::sample> bound_grid,
    texture3d<float, access::sample> derivative_lower_grid,
    texture3d<float, access::sample> derivative_upper_grid,
    thread float2 &range,
    thread float3 &derivative_lower,
    thread float3 &derivative_upper,
    thread bool &derivative_certified) {
    float4 first = bound_grid.read(coordinate);
    range = first.xy;
    derivative_lower = float3(-inf);
    derivative_upper = float3(inf);
    derivative_certified = false;
    if (cfg.bound_grid_directional == 0u) return;
    if (cfg.bound_grid_fp16 != 0u) {
        float4 second = derivative_lower_grid.read(coordinate);
        derivative_lower = float3(first.z, second.x, second.z);
        derivative_upper = float3(first.w, second.y, second.w);
        derivative_certified = all(isfinite(derivative_lower)) &&
                               all(isfinite(derivative_upper));
    } else {
        float4 lower = derivative_lower_grid.read(coordinate);
        derivative_lower = lower.xyz;
        derivative_upper = derivative_upper_grid.read(coordinate).xyz;
        derivative_certified = lower.w > 0.5f &&
                               all(isfinite(derivative_lower)) &&
                               all(isfinite(derivative_upper));
    }
}

// Exact procedural evaluation remains authoritative. The grid only supplies a
// certified bound on |d/dt f(origin + t*direction)|, so the quotient below is
// a safe ray-directional step and is never allowed to cross the current cell.
static BoundGridTraceResult traceDirectionalClipped(
    float3 origin,
    float3 direction,
    float entry_t,
    float exit_t,
    float3 derivative_lower,
    float3 derivative_upper,
    float3 lod_origin,
    int min_iterations,
    float min_distance,
    float lod_falloff,
    constant FptRenderConfig &cfg,
    thread uint &remaining_steps,
    uint ray_class,
    thread BoundGridLocalStats &stats) {
    BoundGridTraceResult result;
    result.hit = false;
    result.distance = inf;
    result.position = origin + direction * exit_t;
    if (!(exit_t >= entry_t) || remaining_steps == 0u) return result;

    float directional_lipschitz = directionalDerivativeMagnitude(
        direction, derivative_lower, derivative_upper);
    if (!isfinite(directional_lipschitz)) return result;
    float t = max(entry_t, 0.0f);
    bool check_translucency = sdfHasTranslucentSurfaces(cfg);
    uint interval_budget = min(remaining_steps, uint(max(min_iterations, 0)));
    for (uint iteration = 0u; iteration < interval_budget && t <= exit_t;
         ++iteration) {
        float3 sample_position = origin + direction * t;
        float distance = mapSdf(sample_position, cfg);
        stats.field_evaluations[ray_class] += 1u;
        stats.directional_steps[ray_class] += 1u;
        remaining_steps -= 1u;
        if (!isfinite(distance)) return result;

        float safe_step = directional_lipschitz > 1.0e-8f
            ? abs(distance) * 0.99f / directional_lipschitz
            : inf;
        float interval_remaining = max(exit_t - t, 0.0f);
        bool reaches_exit = !isfinite(safe_step) || safe_step >= interval_remaining;
        float step_length = reaches_exit ? interval_remaining : safe_step;
        float next_t = t + step_length;
        float3 position = origin + direction * next_t;
        float lod = min_distance;
        if (lod_falloff > 0.00001f) {
            float fog_lod = dot(lod_origin - position, lod_origin - position);
            lod = mix(lod, 0.1f, fog_lod * lod_falloff);
            lod = mix(min_distance, lod, cfg.render[5]);
        }
        if (check_translucency &&
            userSdf(position, cfg).material.translucency > 0.0f) {
            lod = 0.0001f;
        }
        if (!reaches_exit && step_length < lod) {
            result.hit = true;
            result.distance = next_t;
            result.position = position;
            return result;
        }
        if (reaches_exit) {
            stats.cell_exit_clamps[ray_class] += 1u;
            return result;
        }
        float progressed_t = t + max(step_length, 1.0e-7f);
        if (!(progressed_t > t) || progressed_t > exit_t) return result;
        t = progressed_t;
    }
    return result;
}

static float3 marchBoundGridTracked(float3 direction,
                                    float3 origin,
                                    int iterations,
                                    float min_distance,
                                    float lod_falloff,
                                    constant FptRenderConfig &cfg,
                                    texture3d<float, access::sample> bound_grid,
                                    texture3d<float, access::sample> derivative_lower_grid,
                                    texture3d<float, access::sample> derivative_upper_grid,
                                    uint ray_class,
                                    thread BoundGridLocalStats &stats) {
    // Unsupported procedural programs are deliberately not classified by an
    // approximate center sample. Until their bounds are certified they retain
    // the original renderer and its exact historical behaviour.
    bool supported_program = cfg.sdf_id == SDF_PROGRAM && cfg.sdf_program_count > 0u;
    bool supported_cage = cfg.sdf_id == SDF_CAGE_FRACTAL &&
                          cfg.bound_grid_cage_bounds != 0u;
    if (!supported_program && !supported_cage) {
        return march(direction, origin, iterations, min_distance, lod_falloff, cfg);
    }

    uint resolution = max(cfg.bound_grid_resolution, 1u);
    float3 bounds_min = voxelBoundsMin(cfg);
    float3 bounds_max = voxelBoundsMax(cfg);
    float3 cell_size = (bounds_max - bounds_min) / float(resolution);
    float near_t;
    float far_t;
    float3 entry_normal;
    if (!voxelRayAabb(origin, direction, bounds_min, bounds_max,
                      near_t, far_t, entry_normal)) {
        return march(direction, origin, iterations, min_distance, lod_falloff, cfg);
    }

    uint remaining_steps = uint(max(min(iterations, 360), 0));
    float maximum_t = max(cfg.render[4], far_t + length(bounds_max - bounds_min));
    if (near_t > 0.0f) {
        BoundGridTraceResult prefix = traceProceduralClipped(
            origin, direction, 0.0f, min(near_t, maximum_t), origin,
            iterations, min_distance, lod_falloff, cfg, remaining_steps,
            ray_class, stats);
        if (prefix.hit) return prefix.position;
    }

    float entry_t = max(near_t, 0.0f);
    float3 start = clamp(origin + direction * entry_t, bounds_min, bounds_max);
    int3 cell = int3(0);
    if (!voxelCellCoordinate(start, direction, bounds_min, bounds_max,
                             cell_size, resolution, cell)) {
        return march(direction, origin, iterations, min_distance, lod_falloff, cfg);
    }
    int3 step_direction = int3(0);
    float3 next_t = float3(inf);
    float3 delta_t = float3(inf);
    for (uint axis = 0u; axis < 3u; ++axis) {
        if (direction[axis] > 0.0f) {
            step_direction[axis] = 1;
            float boundary = bounds_min[axis] + float(cell[axis] + 1) * cell_size[axis];
            next_t[axis] = (boundary - origin[axis]) / direction[axis];
            delta_t[axis] = cell_size[axis] / direction[axis];
        } else if (direction[axis] < 0.0f) {
            step_direction[axis] = -1;
            float boundary = bounds_min[axis] + float(cell[axis]) * cell_size[axis];
            next_t[axis] = (boundary - origin[axis]) / direction[axis];
            delta_t[axis] = -cell_size[axis] / direction[axis];
        }
        if (!isfinite(next_t[axis]) || !isfinite(delta_t[axis])) {
            next_t[axis] = inf;
            delta_t[axis] = inf;
        }
    }

    uint max_macro_steps = resolution * 3u + 3u;
    for (uint step = 0u;
         step < max_macro_steps && entry_t <= far_t && remaining_steps > 0u;
         ++step) {
        if (any(cell < int3(0)) || any(cell >= int3(int(resolution)))) break;
        uint3 coordinate = uint3(cell);
        stats.macro_cells[ray_class] += 1u;
        float interval_exit = min(far_t, min(next_t.x, min(next_t.y, next_t.z)));
        float2 range;
        float3 derivative_lower;
        float3 derivative_upper;
        bool derivative_certified = false;
        readDirectionalGridCell(coordinate, cfg, bound_grid,
                                derivative_lower_grid,
                                derivative_upper_grid, range,
                                derivative_lower, derivative_upper,
                                derivative_certified);
        float hit_epsilon = max(min_distance, 1.0e-5f);
        bool unknown = !isfinite(range.x) || !isfinite(range.y);
        if (unknown) stats.unknown_intervals[ray_class] += 1u;
        bool certified_empty = isfinite(range.x) && isfinite(range.y) &&
            (range.x > hit_epsilon || range.y < -hit_epsilon);
        if (certified_empty) {
            stats.certified_skips[ray_class] += 1u;
        } else {
            stats.candidate_intervals[ray_class] += 1u;
            bool directional = cfg.bound_grid_directional != 0u &&
                derivative_certified;
            if (cfg.bound_grid_directional != 0u && !directional) {
                stats.unknown_derivative_intervals[ray_class] += 1u;
            }
            BoundGridTraceResult candidate = directional
                ? traceDirectionalClipped(
                    origin, direction, entry_t, interval_exit,
                    derivative_lower, derivative_upper, origin,
                    iterations, min_distance, lod_falloff, cfg,
                    remaining_steps, ray_class, stats)
                : traceProceduralClipped(
                    origin, direction, entry_t, interval_exit, origin,
                    iterations, min_distance, lod_falloff, cfg,
                    remaining_steps, ray_class, stats);
            if (candidate.hit) {
                stats.candidate_hits[ray_class] += 1u;
                return candidate.position;
            }
            stats.candidate_misses[ray_class] += 1u;
        }
        if (!voxelAdvanceDda(cell, step_direction, next_t, delta_t,
                             entry_t, entry_normal)) break;
    }

    if (remaining_steps > 0u && far_t < maximum_t) {
        BoundGridTraceResult suffix = traceProceduralClipped(
            origin, direction, max(far_t, 0.0f), maximum_t, origin,
            iterations, min_distance, lod_falloff, cfg, remaining_steps,
            ray_class, stats);
        if (suffix.hit) return suffix.position;
    }
    return origin + direction * (cfg.render[4] * 1.01f);
}

static float3 marchBoundGrid(float3 direction,
                             float3 origin,
                             int iterations,
                             float min_distance,
                             float lod_falloff,
                             constant FptRenderConfig &cfg,
                             texture3d<float, access::sample> bound_grid,
                             texture3d<float, access::sample> derivative_lower_grid,
                             texture3d<float, access::sample> derivative_upper_grid) {
    BoundGridLocalStats unused_stats = emptyBoundGridLocalStats();
    return marchBoundGridTracked(direction, origin, iterations, min_distance,
                                 lod_falloff, cfg, bound_grid,
                                 derivative_lower_grid, derivative_upper_grid,
                                 BoundGridRayPrimary, unused_stats);
}

// A one-thread contract test executed by the Rust test suite. Each bit
// identifies a distinct traversal invariant so failures remain actionable.
struct ExactValidationCounts {
    atomic_uint distance_failures;
    atomic_uint gradient_failures;
    atomic_uint max_distance_error_bits;
    atomic_uint max_gradient_error_bits;
};

static uint fptStitchValidationHash(uint value) {
    value ^= value >> 16u;
    value *= 0x7feb352du;
    value ^= value >> 15u;
    value *= 0x846ca68bu;
    value ^= value >> 16u;
    return value;
}

#if defined(FPT_STITCH_HOST)
kernel void stitched_validation_kernel(
    device ExactValidationCounts *counts [[buffer(0)]],
    constant FptRenderConfig &cfg [[buffer(1)]],
    uint gid [[thread_position_in_grid]]) {
    uint hx = fptStitchValidationHash(gid * 3u + 0x9e3779b9u);
    uint hy = fptStitchValidationHash(gid * 3u + 0x243f6a88u);
    uint hz = fptStitchValidationHash(gid * 3u + 0xb7e15162u);
    float3 unit = float3(hx, hy, hz) * (1.0f / 4294967295.0f);
    float3 bounds_min = float3(cfg.voxel_bounds_min[0],
                               cfg.voxel_bounds_min[1],
                               cfg.voxel_bounds_min[2]);
    float3 bounds_max = float3(cfg.voxel_bounds_max[0],
                               cfg.voxel_bounds_max[1],
                               cfg.voxel_bounds_max[2]);
    float3 position = mix(bounds_min, bounds_max, unit);
    // Reserve deterministic samples for exact CSG ties and transform seams;
    // the remaining points retain broad randomized coverage.
    if (gid == 0u) position = float3(0.0f);
    if (gid == 1u) position = bounds_min;
    if (gid == 2u) position = bounds_max;
    if (gid == 3u) position = (bounds_min + bounds_max) * 0.5f;
    float reference_distance = deProgramDistance(position, cfg);
    float stitched_distance = deTopologyStitchedDistance(position, cfg);
    float distance_error = abs(reference_distance - stitched_distance);
    float distance_tolerance = 2.0e-5f * (1.0f + abs(reference_distance));
    if (!isfinite(reference_distance) || !isfinite(stitched_distance) ||
        distance_error > distance_tolerance) {
        atomic_fetch_add_explicit(&counts->distance_failures, 1u,
                                  memory_order_relaxed);
    }
    atomic_fetch_max_explicit(&counts->max_distance_error_bits,
                              as_type<uint>(distance_error),
                              memory_order_relaxed);

    ProgramSurface reference_surface = programSurfaceInterpreted(position, cfg);
    ProgramSurface stitched_surface = deTopologyStitchedSurface(position, cfg);
    float gradient_error = max(abs(reference_surface.gradient.x -
                                   stitched_surface.gradient.x),
        max(abs(reference_surface.gradient.y - stitched_surface.gradient.y),
            abs(reference_surface.gradient.z - stitched_surface.gradient.z)));
    if (any(!isfinite(reference_surface.gradient)) ||
        any(!isfinite(stitched_surface.gradient)) ||
        gradient_error > 1.0e-4f) {
        atomic_fetch_add_explicit(&counts->gradient_failures, 1u,
                                  memory_order_relaxed);
    }
    atomic_fetch_max_explicit(&counts->max_gradient_error_bits,
                              as_type<uint>(gradient_error),
                              memory_order_relaxed);
}
#endif

#if defined(FPT_TOPOLOGY_RUNTIME_SOURCE)
kernel void topology_validation_kernel(
    device ExactValidationCounts *counts [[buffer(0)]],
    constant FptRenderConfig &cfg [[buffer(1)]],
    uint gid [[thread_position_in_grid]]) {
    uint hx = fptStitchValidationHash(gid * 3u + 0x9e3779b9u);
    uint hy = fptStitchValidationHash(gid * 3u + 0x243f6a88u);
    uint hz = fptStitchValidationHash(gid * 3u + 0xb7e15162u);
    float3 unit = float3(hx, hy, hz) * (1.0f / 4294967295.0f);
    float3 bounds_min = float3(cfg.voxel_bounds_min[0],
                               cfg.voxel_bounds_min[1],
                               cfg.voxel_bounds_min[2]);
    float3 bounds_max = float3(cfg.voxel_bounds_max[0],
                               cfg.voxel_bounds_max[1],
                               cfg.voxel_bounds_max[2]);
    float3 position = mix(bounds_min, bounds_max, unit);
    if (gid == 0u) position = float3(0.0f);
    if (gid == 1u) position = bounds_min;
    if (gid == 2u) position = bounds_max;
    if (gid == 3u) position = (bounds_min + bounds_max) * 0.5f;

    float reference_distance = deProgramDistance(position, cfg);
    float specialized_distance = deTopologySpecializedDistance(position, cfg);
    float distance_error = abs(reference_distance - specialized_distance);
    float distance_tolerance = 2.0e-5f * (1.0f + abs(reference_distance));
    if (!isfinite(reference_distance) || !isfinite(specialized_distance) ||
        distance_error > distance_tolerance) {
        atomic_fetch_add_explicit(&counts->distance_failures, 1u,
                                  memory_order_relaxed);
    }
    atomic_fetch_max_explicit(&counts->max_distance_error_bits,
                              as_type<uint>(distance_error),
                              memory_order_relaxed);

    ProgramSurface reference_surface = programSurfaceInterpreted(position, cfg);
    ProgramSurface specialized_surface = programSurface(position, cfg);
    float gradient_error = max(abs(reference_surface.gradient.x -
                                   specialized_surface.gradient.x),
        max(abs(reference_surface.gradient.y - specialized_surface.gradient.y),
            abs(reference_surface.gradient.z - specialized_surface.gradient.z)));
    if (any(!isfinite(reference_surface.gradient)) ||
        any(!isfinite(specialized_surface.gradient)) ||
        gradient_error > 1.0e-4f) {
        atomic_fetch_add_explicit(&counts->gradient_failures, 1u,
                                  memory_order_relaxed);
    }
    atomic_fetch_max_explicit(&counts->max_gradient_error_bits,
                              as_type<uint>(gradient_error),
                              memory_order_relaxed);
}
#endif

kernel void typed_soa_validation_kernel(
    device ExactValidationCounts *counts [[buffer(0)]],
    constant FptRenderConfig &cfg [[buffer(1)]],
    uint gid [[thread_position_in_grid]]) {
    uint hx = fptStitchValidationHash(gid * 3u + 0x9e3779b9u);
    uint hy = fptStitchValidationHash(gid * 3u + 0x243f6a88u);
    uint hz = fptStitchValidationHash(gid * 3u + 0xb7e15162u);
    float3 unit = float3(hx, hy, hz) * (1.0f / 4294967295.0f);
    float3 bounds_min = float3(cfg.voxel_bounds_min[0],
                               cfg.voxel_bounds_min[1],
                               cfg.voxel_bounds_min[2]);
    float3 bounds_max = float3(cfg.voxel_bounds_max[0],
                               cfg.voxel_bounds_max[1],
                               cfg.voxel_bounds_max[2]);
    float3 position = mix(bounds_min, bounds_max, unit);
    if (gid == 0u) position = float3(0.0f);
    if (gid == 1u) position = bounds_min;
    if (gid == 2u) position = bounds_max;
    if (gid == 3u) position = (bounds_min + bounds_max) * 0.5f;

    float reference_distance = deProgramDistance(position, cfg);
    float direct_distance = deTypedSoADistance(position, cfg);
    float distance_error = abs(reference_distance - direct_distance);
    float distance_tolerance = 2.0e-5f * (1.0f + abs(reference_distance));
    if (!isfinite(reference_distance) || !isfinite(direct_distance) ||
        distance_error > distance_tolerance) {
        atomic_fetch_add_explicit(&counts->distance_failures, 1u,
                                  memory_order_relaxed);
    }
    atomic_fetch_max_explicit(&counts->max_distance_error_bits,
                              as_type<uint>(distance_error),
                              memory_order_relaxed);

    ProgramSurface reference_surface = programSurfaceInterpreted(position, cfg);
    ProgramSurface direct_surface = typedSoASurface(position, cfg);
    float gradient_error = max(abs(reference_surface.gradient.x -
                                   direct_surface.gradient.x),
        max(abs(reference_surface.gradient.y - direct_surface.gradient.y),
            abs(reference_surface.gradient.z - direct_surface.gradient.z)));
    if (any(!isfinite(reference_surface.gradient)) ||
        any(!isfinite(direct_surface.gradient)) ||
        gradient_error > 1.0e-4f) {
        atomic_fetch_add_explicit(&counts->gradient_failures, 1u,
                                  memory_order_relaxed);
    }
    atomic_fetch_max_explicit(&counts->max_gradient_error_bits,
                              as_type<uint>(gradient_error),
                              memory_order_relaxed);
}

kernel void voxel_dda_contract_test(device uint *failure_mask [[buffer(0)]],
                                    constant FptRenderConfig *program_configs [[buffer(1)]]) {
    uint failures = 0u;
    const float3 bounds_min = float3(0.0f);
    const float3 bounds_max = float3(4.0f);
    const float3 cell_size = float3(1.0f);
    int3 cell = int3(0);

    if (!voxelCellCoordinate(float3(1.0f, 2.0f, 3.0f), float3(1.0f),
                             bounds_min, bounds_max, cell_size, 4u, cell) ||
        any(cell != int3(1, 2, 3))) failures |= 1u << 0u;
    if (!voxelCellCoordinate(float3(1.0f, 2.0f, 3.0f), float3(-1.0f),
                             bounds_min, bounds_max, cell_size, 4u, cell) ||
        any(cell != int3(0, 1, 2))) failures |= 1u << 1u;
    if (!voxelCellCoordinate(float3(4.0f), float3(-1.0f),
                             bounds_min, bounds_max, cell_size, 4u, cell) ||
        any(cell != int3(3))) failures |= 1u << 2u;
    if (voxelCellCoordinate(float3(4.0f), float3(1.0f),
                            bounds_min, bounds_max, cell_size, 4u, cell)) failures |= 1u << 3u;
    if (!voxelCellCoordinate(float3(4.0f), float3(0.0f),
                             bounds_min, bounds_max, cell_size, 4u, cell) ||
        any(cell != int3(3))) failures |= 1u << 4u;

    cell = int3(0);
    int3 step_direction = int3(1);
    float3 next_t = float3(1.0f, 1.0f, 2.0f);
    float3 delta_t = float3(1.0f);
    float entry_t = 0.0f;
    float3 entry_normal = float3(0.0f);
    if (!voxelAdvanceDda(cell, step_direction, next_t, delta_t,
                         entry_t, entry_normal) ||
        any(cell != int3(1, 1, 0)) || any(next_t != float3(2.0f, 2.0f, 2.0f)) ||
        entry_t != 1.0f || any(entry_normal != float3(-1.0f, 0.0f, 0.0f))) {
        failures |= 1u << 5u;
    }
    if (!voxelAdvanceDda(cell, step_direction, next_t, delta_t,
                         entry_t, entry_normal) ||
        any(cell != int3(2, 2, 1)) || any(next_t != float3(3.0f)) || entry_t != 2.0f) {
        failures |= 1u << 6u;
    }

    cell = int3(0);
    next_t = float3(1.0f, 2.0f, 3.0f);
    entry_t = 1.0f;
    if (!voxelAdvanceDda(cell, step_direction, next_t, delta_t,
                         entry_t, entry_normal) ||
        as_type<uint>(entry_t) != as_type<uint>(1.0f) + 1u ||
        any(cell != int3(1, 0, 0))) failures |= 1u << 7u;

    float near_t = 0.0f;
    float far_t = 0.0f;
    if (!voxelRayAabb(float3(-1.0f, 2.0f, 2.0f), float3(1.0e-9f, 0.0f, 0.0f),
                      bounds_min, bounds_max, near_t, far_t, entry_normal) ||
        !(near_t > 0.0f) || !(far_t > near_t)) failures |= 1u << 8u;
    if (voxelRayAabb(float3(-1.0f, 5.0f, 2.0f), float3(1.0f, 0.0f, 0.0f),
                     bounds_min, bounds_max, near_t, far_t, entry_normal)) failures |= 1u << 9u;

    if (uint(AccelerationCellOutsideNoSurface) != 0u ||
        uint(AccelerationCellInsideNoSurface) != 1u ||
        uint(AccelerationCellPotentialSurface) != 2u ||
        uint(AccelerationCellUnknown) != 3u) failures |= 1u << 10u;

    const float3 query_half_extent = float3(0.25f);
    ProgramBounds sphere_bounds = programBounds(float3(0.0f), query_half_extent,
                                                program_configs[0]);
    AccelerationCellClassification sphere_inside =
        classifyAccelerationCell(float3(0.0f), query_half_extent, program_configs[0]);
    AccelerationCellClassification sphere_outside =
        classifyAccelerationCell(float3(2.0f, 0.0f, 0.0f), query_half_extent,
                                 program_configs[0]);
    AccelerationCellClassification sphere_surface =
        classifyAccelerationCell(float3(1.0f, 0.0f, 0.0f), float3(0.1f),
                                 program_configs[0]);
    if (sphere_bounds.certified == 0u || sphere_bounds.lipschitz != 1.0f ||
        sphere_inside.certified == 0u ||
        sphere_inside.cell_class != AccelerationCellInsideNoSurface ||
        sphere_outside.cell_class != AccelerationCellOutsideNoSurface ||
        sphere_surface.cell_class != AccelerationCellPotentialSurface ||
        !programBoundsContainGridSamples(float3(0.0f), query_half_extent,
                                        program_configs[0])) failures |= 1u << 11u;
    if (sphere_surface.lipschitz_occupied == 0u ||
        sphere_surface.interval_occupied == 0u ||
        sphere_inside.lipschitz_occupied != 0u ||
        sphere_inside.interval_occupied != 0u) failures |= 1u << 20u;
    AccelerationCellClassification legacy_miss =
        classifyAccelerationCell(float3(0.75f), query_half_extent, program_configs[0]);
    if (legacy_miss.legacy_occupied != 0u ||
        legacy_miss.lipschitz_occupied == 0u ||
        legacy_miss.interval_occupied == 0u ||
        legacy_miss.cell_class != AccelerationCellPotentialSurface) failures |= 1u << 21u;

    if (!programBoundsContainGridSamples(float3(0.35f, -0.15f, 0.2f),
                                         float3(0.4f, 0.3f, 0.2f),
                                         program_configs[1])) failures |= 1u << 12u;

    ProgramBounds plane_bounds = programBounds(float3(0.2f, -0.1f, 0.3f),
                                               float3(0.4f, 0.2f, 0.5f),
                                               program_configs[2]);
    if (plane_bounds.certified == 0u ||
        !programBoundsContainGridSamples(float3(0.2f, -0.1f, 0.3f),
                                         float3(0.4f, 0.2f, 0.5f),
                                         program_configs[2])) failures |= 1u << 13u;

    ProgramBounds repeat_safe = programBounds(float3(0.0f), float3(0.3f),
                                              program_configs[3]);
    ProgramBounds repeat_seam = programBounds(float3(2.0f, 0.0f, 0.0f),
                                              float3(0.1f), program_configs[3]);
    if (repeat_safe.certified == 0u || repeat_seam.certified != 0u ||
        !programBoundsContainGridSamples(float3(0.0f), float3(0.3f),
                                         program_configs[3])) failures |= 1u << 14u;

    if (!programBoundsContainGridSamples(float3(0.4f, -0.2f, 0.1f),
                                         float3(0.6f, 0.5f, 0.4f),
                                         program_configs[4])) failures |= 1u << 15u;
    if (!programBoundsContainGridSamples(float3(0.2f, 0.0f, 0.0f),
                                         float3(0.8f, 0.5f, 0.4f),
                                         program_configs[5])) failures |= 1u << 16u;
    if (!programBoundsContainGridSamples(float3(0.4f, 0.0f, 0.0f),
                                         float3(1.1f, 0.5f, 0.4f),
                                         program_configs[6])) failures |= 1u << 17u;

    ProgramBounds sorted_bounds = programBounds(float3(0.0f), float3(0.25f),
                                                program_configs[7]);
    AccelerationCellClassification sorted_surface =
        classifyAccelerationCell(float3(1.0f, 0.0f, 0.0f), float3(0.1f),
                                 program_configs[7]);
    if (sorted_bounds.certified == 0u ||
        sorted_surface.cell_class != AccelerationCellPotentialSurface ||
        sorted_surface.certified == 0u ||
        sorted_surface.legacy_occupied == 0u ||
        sorted_surface.lipschitz_occupied == 0u ||
        sorted_surface.interval_occupied == 0u ||
        !programBoundsContainGridSamples(float3(0.0f), float3(0.25f),
                                         program_configs[7])) failures |= 1u << 18u;

    ProgramBounds sorted_box_bounds = programBounds(float3(0.2f, -0.35f, 0.1f),
                                                     float3(0.7f, 0.45f, 0.55f),
                                                     program_configs[8]);
    if (sorted_box_bounds.certified == 0u ||
        !programBoundsContainGridSamples(float3(0.2f, -0.35f, 0.1f),
                                         float3(0.7f, 0.45f, 0.55f),
                                         program_configs[8])) failures |= 1u << 29u;

    ProgramBounds cage_bounds = accelerationProgramBounds(float3(6.0f, 0.7f, 0.4f),
                                                           float3(0.01f),
                                                           program_configs[9]);
    if (cage_bounds.certified == 0u || cage_bounds.minimum <= 0.0f ||
        !accelerationBoundsContainGridSamples(float3(6.0f, 0.7f, 0.4f),
                                              float3(0.01f),
                                              program_configs[9])) failures |= 1u << 30u;

    if (sphere_outside.legacy_occupied != 0u ||
        sphere_surface.legacy_occupied == 0u) failures |= 1u << 19u;

    float refined_t = 0.0f;
    if (!refineVoxelLeafSecantBisection(float3(-2.0f, 0.0f, 0.0f),
                                        float3(1.0f, 0.0f, 0.0f),
                                        0.75f, 1.25f, program_configs[0], refined_t) ||
        refined_t < 0.75f || refined_t > 1.25f ||
        abs(refined_t - 1.0f) > 1.0e-4f) failures |= 1u << 22u;
    refined_t = 0.0f;
    if (refineVoxelLeafSecantBisection(float3(-2.0f, 1.5f, 0.0f),
                                       float3(1.0f, 0.0f, 0.0f),
                                       0.75f, 1.25f, program_configs[0], refined_t)) {
        failures |= 1u << 23u;
    }
    refined_t = 0.0f;
    if (!refineVoxelLeafRestrictedTrace(float3(-2.0f, 0.0f, 0.0f),
                                        float3(1.0f, 0.0f, 0.0f),
                                        0.75f, 1.25f, float3(-1.0f, 0.0f, 0.0f),
                                        float3(0.25f), program_configs[0], refined_t) ||
        refined_t < 0.75f || refined_t > 1.25f ||
        abs(refined_t - 1.0f) > 1.0e-4f) failures |= 1u << 24u;
    refined_t = 0.0f;
    if (refineVoxelLeafRestrictedTrace(float3(-2.0f, 1.5f, 0.0f),
                                       float3(1.0f, 0.0f, 0.0f),
                                       0.75f, 1.25f, float3(-1.0f, 1.5f, 0.0f),
                                       float3(0.25f), program_configs[0], refined_t)) {
        failures |= 1u << 25u;
    }
    refined_t = 0.0f;
    if (refineVoxelLeafFixedDe(float3(-2.0f, 0.0f, 0.0f),
                               float3(1.0f, 0.0f, 0.0f),
                               0.75f, 1.25f, program_configs[0], refined_t)) {
        failures |= 1u << 26u;
    }
    float3 positive_offset = precisionOffsetRayOrigin(float3(1.0f, -1.0f, 0.0f),
                                                      float3(1.0f, 0.0f, 0.0f));
    float3 negative_offset = precisionOffsetRayOrigin(float3(1.0f, -1.0f, 0.0f),
                                                      float3(-1.0f, 0.0f, 0.0f));
    if (!(positive_offset.x > 1.0f) || positive_offset.y != -1.0f ||
        !(negative_offset.x < 1.0f) || negative_offset.y != -1.0f) {
        failures |= 1u << 27u;
    }
    float3 origin_offset = precisionOffsetRayOrigin(float3(0.0f),
                                                    float3(1.0f, -1.0f, 0.0f));
    if (!(origin_offset.x > 0.0f) || !(origin_offset.y < 0.0f) || origin_offset.z != 0.0f) {
        failures |= 1u << 28u;
    }

    if (!programDerivativeBoundsContainGridSamples(
            float3(0.35f, -0.15f, 0.2f), float3(0.4f, 0.3f, 0.2f),
            program_configs[0]) ||
        !programDerivativeBoundsContainGridSamples(
            float3(0.2f, -0.1f, 0.3f), float3(0.4f, 0.2f, 0.5f),
            program_configs[1]) ||
        !programDerivativeBoundsContainGridSamples(
            float3(0.2f, -0.1f, 0.3f), float3(0.4f, 0.2f, 0.5f),
            program_configs[2]) ||
        !programDerivativeBoundsContainGridSamples(
            float3(0.1f, -0.2f, 0.3f), float3(0.5f, 0.4f, 0.3f),
            program_configs[5]) ||
        !programDerivativeBoundsContainGridSamples(
            float3(0.4f, -0.2f, 0.1f), float3(0.6f, 0.5f, 0.4f),
            program_configs[4]) ||
        !programDerivativeBoundsContainGridSamples(
            float3(0.25f, -0.2f, 0.1f), float3(0.45f, 0.35f, 0.25f),
            program_configs[10]) ||
        !programDerivativeBoundsContainGridSamples(
            float3(0.1f), float3(0.1f), program_configs[11]) ||
        !programDerivativeBoundsContainGridSamples(
            float3(0.0f), float3(0.2f), program_configs[3]) ||
        programDerivativeBounds(float3(2.0f, 0.0f, 0.0f), float3(0.2f),
                                program_configs[3]).certified != 0u ||
        programDerivativeBounds(float3(0.0f), float3(0.2f),
                                program_configs[7]).certified != 0u) {
        failures |= 1u << 31u;
    }

    failure_mask[0] = failures;
}

static VoxelCell buildVoxelCell(float3 position,
                                float3 half_extent,
                                constant FptRenderConfig &cfg) {
    AccelerationCellClassification classification =
        classifyAccelerationCell(position, half_extent, cfg);
    bool occupied = classification.legacy_occupied != 0u;
    if (cfg.voxel_coverage_mode == VOXEL_COVERAGE_LIPSCHITZ) {
        occupied = classification.lipschitz_occupied != 0u;
    } else if (cfg.voxel_coverage_mode == VOXEL_COVERAGE_INTERVAL) {
        occupied = classification.interval_occupied != 0u;
    }
    VoxelCell cell;
    cell.packed_color = 0u;
    cell.packed_properties = 0u;
    cell.emission = 0.0f;
    if (occupied) {
        Material material = userSdf(position, cfg).material;
        uint packed_color = packVoxelUnorm4(float4(material.rgb, 0.0f));
        cell.packed_color = packed_color | 0x80000000u;
        cell.packed_properties = packVoxelUnorm4(float4(material.roughness,
                                                        material.specular,
                                                        material.translucency,
                                                        (material.ior - 1.0f) / 1.5f));
        cell.emission = material.emission;
    }
    return cell;
}

kernel void voxel_build_kernel(device VoxelCell *cells [[buffer(0)]],
                               constant FptRenderConfig &cfg [[buffer(1)]],
                               uint3 gid [[thread_position_in_grid]]) {
    uint resolution = max(cfg.voxel_resolution, 1u);
    if (any(gid >= uint3(resolution))) return;
    float3 cell_size = voxelCellSize(cfg);
    float3 position = voxelBoundsMin(cfg) + (float3(gid) + 0.5f) * cell_size;
    cells[voxelIndex(gid, resolution)] = buildVoxelCell(position, cell_size * 0.5f, cfg);
}

kernel void bound_grid_build_kernel(texture3d<float, access::write> bound_grid [[texture(0)]],
                                    constant FptRenderConfig &cfg [[buffer(1)]],
                                    uint3 gid [[thread_position_in_grid]]) {
    uint resolution = max(cfg.bound_grid_resolution, 1u);
    if (any(gid >= uint3(resolution))) return;
    float3 bounds_min = voxelBoundsMin(cfg);
    float3 bounds_max = voxelBoundsMax(cfg);
    float3 cell_size = (bounds_max - bounds_min) / float(resolution);
    float3 center = bounds_min + (float3(gid) + 0.5f) * cell_size;
    ProgramBounds bounds = accelerationProgramBounds(center, cell_size * 0.5f, cfg);
    float2 range = bounds.certified != 0u
        ? float2(bounds.minimum, bounds.maximum)
        : float2(-inf, inf);
    bound_grid.write(float4(range, 0.0f, 0.0f), gid);
}

kernel void regional_program_build_kernel(
    device ulong *program_masks [[buffer(0)]],
    constant FptRenderConfig &cfg [[buffer(1)]],
    device uint2 *program_proofs [[buffer(2)]],
    uint3 gid [[thread_position_in_grid]]) {
    uint resolution = max(cfg.regional_program_resolution, 1u);
    if (any(gid >= uint3(resolution))) return;
    float3 bounds_min = voxelBoundsMin(cfg);
    float3 bounds_max = voxelBoundsMax(cfg);
    float3 cell_size = (bounds_max - bounds_min) / float(resolution);
    float3 center = bounds_min + (float3(gid) + 0.5f) * cell_size;
    uint index = gid.x + gid.y * resolution + gid.z * resolution * resolution;
    RegionalProgramDecision decision = regionalProgramDecision(
        center, cell_size * 0.5f, cfg);
    program_masks[index] = decision.instruction_mask;
    if (cfg.bound_grid_profile != 0u) {
        program_proofs[index] = uint2(decision.proof_flags,
                                      decision.pruned_primitives);
    }
}

struct RegionalProgramValidationCounts {
    atomic_uint sampled_distance_failures;
};

kernel void regional_program_validate_kernel(
    device RegionalProgramValidationCounts *counts [[buffer(0)]],
    constant FptRenderConfig &cfg [[buffer(1)]],
    device const ushort *program_ids [[buffer(2)]],
    device const RegionalProgramHeader *program_headers [[buffer(3)]],
    device const FptSdfInstruction *instruction_pool [[buffer(4)]],
    device const ushort *primitive_pool [[buffer(5)]],
    uint3 gid [[thread_position_in_grid]]) {
    uint resolution = max(cfg.regional_program_resolution, 1u);
    if (any(gid >= uint3(resolution))) return;
    float3 bounds_min = voxelBoundsMin(cfg);
    float3 bounds_max = voxelBoundsMax(cfg);
    float3 cell_size = (bounds_max - bounds_min) / float(resolution);
    float3 cell_minimum = bounds_min + float3(gid) * cell_size;
    uint cell_index = gid.x + gid.y * resolution +
                      gid.z * resolution * resolution;
    RegionalProgramHeader header = program_headers[program_ids[cell_index]];
    for (uint z = 0u; z < 3u; ++z) {
        for (uint y = 0u; y < 3u; ++y) {
            for (uint x = 0u; x < 3u; ++x) {
                float3 unit = float3(float(x), float(y), float(z)) * 0.5f;
                float3 position = cell_minimum + unit * cell_size;
                float full_distance = deShadingProgram(position, cfg).d;
                float regional_distance = deRegionalProgramDistance(
                    position, header, instruction_pool, primitive_pool, cfg);
                float tolerance = 5.0e-5f * (1.0f + abs(full_distance));
                if (!isfinite(full_distance) || !isfinite(regional_distance) ||
                    abs(full_distance - regional_distance) > tolerance) {
                    atomic_fetch_add_explicit(&counts->sampled_distance_failures,
                                              1u, memory_order_relaxed);
                }
            }
        }
    }
}

kernel void directional_grid_build_kernel(
    texture3d<float, access::write> bound_grid [[texture(0)]],
    texture3d<float, access::write> derivative_lower_grid [[texture(1)]],
    texture3d<float, access::write> derivative_upper_grid [[texture(2)]],
    constant FptRenderConfig &cfg [[buffer(1)]],
    uint3 gid [[thread_position_in_grid]]) {
    uint resolution = max(cfg.bound_grid_resolution, 1u);
    if (any(gid >= uint3(resolution))) return;
    float3 bounds_min = voxelBoundsMin(cfg);
    float3 bounds_max = voxelBoundsMax(cfg);
    float3 cell_size = (bounds_max - bounds_min) / float(resolution);
    float3 center = bounds_min + (float3(gid) + 0.5f) * cell_size;
    ProgramBounds bounds = accelerationProgramBounds(center, cell_size * 0.5f, cfg);
    float2 range = bounds.certified != 0u
        ? float2(bounds.minimum, bounds.maximum)
        : float2(-inf, inf);
    bound_grid.write(float4(range, 0.0f, 0.0f), gid);

    ProgramDerivativeBounds derivative =
        cfg.sdf_id == SDF_PROGRAM && cfg.sdf_program_count > 0u
        ? programDerivativeBounds(center, cell_size * 0.5f, cfg)
        : unknownProgramDerivativeBounds();
    float certified = derivative.certified != 0u ? 1.0f : 0.0f;
    float3 lower = derivative.certified != 0u ? derivative.lower : float3(-inf);
    float3 upper = derivative.certified != 0u ? derivative.upper : float3(inf);
    derivative_lower_grid.write(float4(lower, certified), gid);
    derivative_upper_grid.write(float4(upper, 0.0f), gid);
}

static float halfRoundDown(float value) {
    if (!isfinite(value)) return value;
    half quantized_half = half(value);
    float quantized = float(quantized_half);
    if (quantized <= value) return quantized;
    ushort bits = as_type<ushort>(quantized_half);
    if (quantized > 0.0f) {
        bits -= 1u;
    } else if (quantized < 0.0f) {
        bits += 1u;
    } else {
        bits = 0x8001u;
    }
    return float(as_type<half>(bits));
}

static float halfRoundUp(float value) {
    if (!isfinite(value)) return value;
    half quantized_half = half(value);
    float quantized = float(quantized_half);
    if (quantized >= value) return quantized;
    ushort bits = as_type<ushort>(quantized_half);
    if (quantized > 0.0f) {
        bits += 1u;
    } else if (quantized < 0.0f) {
        bits -= 1u;
    } else {
        bits = 0x0001u;
    }
    return float(as_type<half>(bits));
}

kernel void directional_grid_build_fp16_kernel(
    texture3d<float, access::write> packed_x_grid [[texture(0)]],
    texture3d<float, access::write> packed_yz_grid [[texture(1)]],
    constant FptRenderConfig &cfg [[buffer(1)]],
    uint3 gid [[thread_position_in_grid]]) {
    uint resolution = max(cfg.bound_grid_resolution, 1u);
    if (any(gid >= uint3(resolution))) return;
    float3 bounds_min = voxelBoundsMin(cfg);
    float3 bounds_max = voxelBoundsMax(cfg);
    float3 cell_size = (bounds_max - bounds_min) / float(resolution);
    float3 center = bounds_min + (float3(gid) + 0.5f) * cell_size;
    ProgramBounds bounds = accelerationProgramBounds(center, cell_size * 0.5f,
                                                     cfg);
    ProgramDerivativeBounds derivative =
        cfg.sdf_id == SDF_PROGRAM && cfg.sdf_program_count > 0u
        ? programDerivativeBounds(center, cell_size * 0.5f, cfg)
        : unknownProgramDerivativeBounds();
    float value_lower = bounds.certified != 0u
        ? halfRoundDown(bounds.minimum) : -inf;
    float value_upper = bounds.certified != 0u
        ? halfRoundUp(bounds.maximum) : inf;
    float3 derivative_lower = derivative.certified != 0u
        ? float3(halfRoundDown(derivative.lower.x),
                 halfRoundDown(derivative.lower.y),
                 halfRoundDown(derivative.lower.z))
        : float3(-inf);
    float3 derivative_upper = derivative.certified != 0u
        ? float3(halfRoundUp(derivative.upper.x),
                 halfRoundUp(derivative.upper.y),
                 halfRoundUp(derivative.upper.z))
        : float3(inf);
    packed_x_grid.write(float4(value_lower, value_upper,
                               derivative_lower.x, derivative_upper.x), gid);
    packed_yz_grid.write(float4(derivative_lower.y, derivative_upper.y,
                                derivative_lower.z, derivative_upper.z), gid);
}

struct BoundGridValidationCounts {
    atomic_uint certified_cells;
    atomic_uint unknown_cells;
    atomic_uint sampled_bound_failures;
    atomic_uint sampled_false_skips;
    atomic_uint certified_derivative_cells;
    atomic_uint unknown_derivative_cells;
    atomic_uint sampled_derivative_failures;
};

kernel void bound_grid_validate_kernel(
    device BoundGridValidationCounts *counts [[buffer(0)]],
    constant FptRenderConfig &cfg [[buffer(1)]],
    texture3d<float, access::sample> bound_grid [[texture(0)]],
    texture3d<float, access::sample> derivative_lower_grid [[texture(1)]],
    texture3d<float, access::sample> derivative_upper_grid [[texture(2)]],
    uint3 gid [[thread_position_in_grid]]) {
    uint resolution = max(cfg.bound_grid_resolution, 1u);
    if (any(gid >= uint3(resolution))) return;
    float2 range;
    float3 derivative_lower;
    float3 derivative_upper;
    bool derivative_certified = false;
    readDirectionalGridCell(gid, cfg, bound_grid, derivative_lower_grid,
                            derivative_upper_grid, range, derivative_lower,
                            derivative_upper, derivative_certified);
    bool certified = isfinite(range.x) && isfinite(range.y);
    if (!certified) {
        atomic_fetch_add_explicit(&counts->unknown_cells, 1u, memory_order_relaxed);
        return;
    }
    atomic_fetch_add_explicit(&counts->certified_cells, 1u, memory_order_relaxed);
    float3 bounds_min = voxelBoundsMin(cfg);
    float3 bounds_max = voxelBoundsMax(cfg);
    float3 cell_size = (bounds_max - bounds_min) / float(resolution);
    float3 cell_min = bounds_min + float3(gid) * cell_size;
    float tolerance = 2.0e-4f * (1.0f + max(abs(range.x), abs(range.y)));
    float hit_epsilon = max(cfg.render[3], 1.0e-5f);
    bool excludes_surface = range.x > hit_epsilon || range.y < -hit_epsilon;
    derivative_certified = cfg.bound_grid_directional != 0u &&
                           derivative_certified;
    if (cfg.bound_grid_directional != 0u) {
        if (derivative_certified) {
            atomic_fetch_add_explicit(&counts->certified_derivative_cells, 1u,
                                      memory_order_relaxed);
        } else {
            atomic_fetch_add_explicit(&counts->unknown_derivative_cells, 1u,
                                      memory_order_relaxed);
        }
    }
    for (uint sample = 0u; sample < 9u; ++sample) {
        float3 unit;
        if (sample == 8u) {
            unit = float3(0.5f);
        } else {
            unit = float3(float(sample & 1u), float((sample >> 1u) & 1u),
                          float((sample >> 2u) & 1u));
        }
        float distance = distanceSdf(cell_min + unit * cell_size, cfg);
        if (!isfinite(distance) || distance < range.x - tolerance ||
            distance > range.y + tolerance) {
            atomic_fetch_add_explicit(&counts->sampled_bound_failures, 1u,
                                      memory_order_relaxed);
        }
        if (excludes_surface && isfinite(distance) && abs(distance) <= hit_epsilon) {
            atomic_fetch_add_explicit(&counts->sampled_false_skips, 1u,
                                      memory_order_relaxed);
        }
        if (derivative_certified) {
            float3 gradient = programSurface(cell_min + unit * cell_size, cfg).gradient;
            float derivative_tolerance = 4.0e-4f;
            if (any(!isfinite(gradient)) ||
                any(gradient < derivative_lower - derivative_tolerance) ||
                any(gradient > derivative_upper + derivative_tolerance)) {
                atomic_fetch_add_explicit(&counts->sampled_derivative_failures, 1u,
                                          memory_order_relaxed);
            }
        }
    }
}

kernel void voxel_build_sparse_direct_kernel(
    device VoxelCell *brick_cells [[buffer(0)]],
    device uint *page_table [[buffer(1)]],
    device VoxelBuildState *state [[buffer(2)]],
    constant FptRenderConfig &cfg [[buffer(3)]],
    constant uint &page_capacity [[buffer(4)]],
    uint3 local_id [[thread_position_in_threadgroup]],
    uint3 brick [[threadgroup_position_in_grid]]) {
    threadgroup VoxelCell staged[64];
    threadgroup atomic_uint occupied_count;
    threadgroup uint allocated_page;
    threadgroup uint reject_brick;
    uint local_index = local_id.x + local_id.y * 4u + local_id.z * 16u;
    if (local_index == 0u) {
        atomic_store_explicit(&occupied_count, 0u, memory_order_relaxed);
        allocated_page = 0u;
        reject_brick = 0u;
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    uint resolution = max(cfg.voxel_resolution, 1u);
    uint3 coordinate = brick * 4u + local_id;
    if (local_index == 0u && cfg.voxel_brick_rejection != 0u &&
        cfg.sdf_id == SDF_PROGRAM && cfg.sdf_program_count > 0u) {
        uint3 first = brick * 4u;
        uint3 end = min(first + 4u, uint3(resolution));
        float3 cell_size = voxelCellSize(cfg);
        float3 brick_center = voxelBoundsMin(cfg) +
            (float3(first) + float3(end - first) * 0.5f) * cell_size;
        float3 brick_half_extent = float3(end - first) * cell_size * 0.5f;
        AccelerationCellClassification brick_classification =
            classifyAccelerationCell(brick_center, brick_half_extent, cfg);
        bool outside = brick_classification.certified != 0u &&
            brick_classification.cell_class == AccelerationCellOutsideNoSurface;
        bool inside_shell = brick_classification.certified != 0u &&
            cfg.voxel_fill_interior == 0u &&
            brick_classification.cell_class == AccelerationCellInsideNoSurface;
        if (outside || inside_shell) {
            reject_brick = 1u;
            atomic_fetch_add_explicit(&state->rejected_bricks, 1u,
                                      memory_order_relaxed);
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    if (reject_brick != 0u) return;
    VoxelCell cell = {0u, 0u, 0.0f};
    if (all(coordinate < uint3(resolution))) {
        float3 cell_size = voxelCellSize(cfg);
        float3 position = voxelBoundsMin(cfg) + (float3(coordinate) + 0.5f) * cell_size;
        cell = buildVoxelCell(position, cell_size * 0.5f, cfg);
    }
    staged[local_index] = cell;
    bool occupied = (cell.packed_color & 0x80000000u) != 0u;
    if (occupied) {
        atomic_fetch_add_explicit(&occupied_count, 1u, memory_order_relaxed);
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    if (local_index == 0u) {
        uint count = atomic_load_explicit(&occupied_count, memory_order_relaxed);
        if (count > 0u) {
            uint page = atomic_fetch_add_explicit(&state->next_page, 1u,
                                                  memory_order_relaxed);
            if (page < page_capacity) {
                allocated_page = page + 1u;
                uint brick_grid = (resolution + 3u) >> 2u;
                uint page_index = brick.x + brick.y * brick_grid +
                                  brick.z * brick_grid * brick_grid;
                page_table[page_index] = allocated_page;
                atomic_fetch_add_explicit(&state->active_cells, count,
                                          memory_order_relaxed);
            } else {
                atomic_store_explicit(&state->overflow, 1u, memory_order_relaxed);
            }
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    if (allocated_page != 0u) {
        brick_cells[(allocated_page - 1u) * 64u + local_index] = staged[local_index];
    }
}

static float preetham(float3 dr, float3 sunDir) {
    float T = 2.0f;
    float A = 0.1787f * T - 1.4630f;
    float B = -0.3554f * T + 0.4275f;
    float C = -0.0227f * T + 5.3251f;
    float D = 0.1206f * T - 2.5771f;
    float E = -0.0670f * T + 0.3703f;
    float theta = acos(clamp(dr.y, -1.0f, 1.0f));
    float gamma = acos(clamp(dot(dr, sunDir), -1.0f, 1.0f));
    return (1.0f + A * exp(B / max(0.1f, cos(theta)))) * (1.0f + C * exp(D * gamma) + E * pow(cos(gamma), 2.0f));
}

static float3 sky(float3 dr, float3 sunDir) {
    float sk = preetham(dr, sunDir);
    float3 col = mix(float3(0.004f, 0.048f, 0.253f), float3(0.8f, 0.9f, 1.0f), sk);
    float height_dr = (dr.y + 1.0f) / 2.0f;
    float height_sun = (sunDir.y + 1.0f) / 2.0f;
    col = mix(float3(1.0f, 0.85f, 0.53f) / 4.0f, col * 1.2f, height_dr) * height_sun;
    col = (col - 0.5f) * 1.2f + 0.5f;
    float sun_disk = max((dot(dr, sunDir) - 1.0f) / (1.0f - cos(0.05f)) + 1.0f, 0.0f);
    sun_disk *= pow(height_dr, 10.0f);
    return col * 0.8f + sun_disk * 100.0f;
}

static float3 backgroundGradient(float3 dir, constant FptRenderConfig &cfg) {
    float t = clamp((dir.y + 1.0f) / 2.0f, 0.0f, 1.0f);
    float3 a = float3(cfg.background_gradient[0], cfg.background_gradient[1], cfg.background_gradient[2]);
    float3 b = float3(cfg.background_gradient[3], cfg.background_gradient[4], cfg.background_gradient[5]);
    return mix(b, a, t);
}

static float hdriLutChannel(constant FptRenderConfig &cfg, uint x, uint y, uint channel) {
    uint width = clamp(cfg.hdri_width, 1u, 32u);
    uint height = clamp(cfg.hdri_height, 1u, 16u);
    uint index = (min(y, height - 1u) * width + min(x, width - 1u)) * 3u + channel;
    return float(as_type<half>(cfg.hdri_lut[index]));
}

static float3 sampleHdriLut(float3 direction, constant FptRenderConfig &cfg) {
    uint width = clamp(cfg.hdri_width, 1u, 32u);
    uint height = clamp(cfg.hdri_height, 1u, 16u);
    float3 dir = normalize(direction);
    float u = fract(atan2(dir.z, dir.x) / (2.0f * pi) + 0.5f);
    float v = clamp(acos(clamp(dir.y, -1.0f, 1.0f)) / pi, 0.0f, 1.0f);
    float x = u * float(width) - 0.5f;
    float y = v * float(height - 1u);
    int x0i = int(floor(x));
    uint x0 = uint((x0i % int(width) + int(width)) % int(width));
    uint x1 = (x0 + 1u) % width;
    uint y0 = uint(clamp(int(floor(y)), 0, int(height - 1u)));
    uint y1 = min(y0 + 1u, height - 1u);
    float2 f = float2(fract(x), fract(y));
    float3 c00 = float3(hdriLutChannel(cfg, x0, y0, 0u), hdriLutChannel(cfg, x0, y0, 1u), hdriLutChannel(cfg, x0, y0, 2u));
    float3 c10 = float3(hdriLutChannel(cfg, x1, y0, 0u), hdriLutChannel(cfg, x1, y0, 1u), hdriLutChannel(cfg, x1, y0, 2u));
    float3 c01 = float3(hdriLutChannel(cfg, x0, y1, 0u), hdriLutChannel(cfg, x0, y1, 1u), hdriLutChannel(cfg, x0, y1, 2u));
    float3 c11 = float3(hdriLutChannel(cfg, x1, y1, 0u), hdriLutChannel(cfg, x1, y1, 1u), hdriLutChannel(cfg, x1, y1, 2u));
    return mix(mix(c00, c10, f.x), mix(c01, c11, f.x), f.y);
}

static float3 environment(float3 viewDir, constant FptRenderConfig &cfg) {
    if (cfg.world[4] < 0.00001f) return float3(0.0f);
    float3 light_dir = rotateCamera(float3(0.0f, 0.0f, 1.0f), float2(cfg.world[2] * pi / 180.0f, cfg.world[3] * pi / 180.0f));
    float3 env = float3(0.0f);
    if (cfg.world[0] == 0.0f) {
        float light_size = max(cfg.world[1], 0.0001f);
        float light = max((dot(viewDir, light_dir) - 1.0f) / (1.0f - cos(light_size)) + 1.0f, 0.0f) / light_size * 3.0f;
        env = light;
    } else if (cfg.world[0] == 1.0f) {
        env = sky(viewDir, light_dir);
    } else if (cfg.world[0] == 2.0f && cfg.hdri_enabled != 0u && cfg.hdri_width > 0u) {
        float3 hdri_dir = rotateCamera(viewDir, float2(cfg.world[2] * pi / 180.0f, 0.0f));
        env = sampleHdriLut(hdri_dir, cfg);
    } else if (cfg.world[0] == 3.0f) {
        env = float3(cfg.world_one_color[0], cfg.world_one_color[1], cfg.world_one_color[2]);
    } else {
        env = float3(1.0f);
    }
    float power = cfg.world[4];
    float contrast = cfg.world[0] == 3.0f ? 1.0f : cfg.world[5];
    return max(env * power - float3(0.5f) * contrast + float3(0.5f), float3(0.0f));
}

static float3 sunContributionWithSurface(float3 rp, float2 xy, float seed, Material mat, float3 n, constant FptRenderConfig &cfg) {
    float p1 = mat.roughness;
    float p2 = 1.0f - mat.translucency;
    float3 light_dir = rotateCamera(float3(0.0f, 0.0f, 1.0f), float2(cfg.sun[1] * pi / 180.0f, cfg.sun[2] * pi / 180.0f));
    float3 rp0 = rp;
    rp += n * 0.001f;
    float h1 = hash13(float3(xy, seed * 5.0f + 1.0f));
    float h2 = hash13(float3(xy, seed * 3.0f + 5.0f));
    float2 div = float2(cos(h1 * 2.0f * pi), sin(h1 * 2.0f * pi)) * sqrt(h2) * cfg.sun[4];
    light_dir = rotateCamera(light_dir, div);
    rp = march(light_dir, rp, int(cfg.render[1]), cfg.render[3], 0.0002f, cfg);
    if (length(rp0 - rp) > cfg.render[4] * 0.99f) {
        return cfg.sun[3] * max(dot(n, light_dir), 0.0f) * p1 * p2 * float3(cfg.sun_color[0], cfg.sun_color[1], cfg.sun_color[2]);
    }
    return float3(0.0f);
}

static float3 sunContribution(float3 rp, float2 xy, float seed, constant FptRenderConfig &cfg) {
    Material mat = userSdf(rp, cfg).material;
    float3 n = normalAt(rp, cfg);
    return sunContributionWithSurface(rp, xy, seed, mat, n, cfg);
}

static float estimateFocusDistance(constant FptRenderConfig &cfg) {
    float focal_length = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
    float3 dr = rotateCamera(normalize(float3(0.0f, 0.0f, focal_length)), cameraYawPitch(cfg), cfg.camera_roll);
    float3 hit = march(dr, cameraPos(cfg), 120, 0.001f, 0.001f, cfg);
    float d = length(hit - cameraPos(cfg));
    if (!isfinite(d) || d <= 0.001f || d > cfg.render[4] * 0.99f) {
        return 5.0f;
    }
    return d;
}

kernel void estimate_focus_distance_kernel(device float *focus_distance [[buffer(0)]],
                                           constant FptRenderConfig &cfg [[buffer(1)]],
                                           uint gid [[thread_position_in_grid]]) {
    if (gid == 0u) focus_distance[0] = estimateFocusDistance(cfg);
}

kernel void estimate_regional_focus_distance_kernel(
    device float *focus_distance [[buffer(0)]],
    constant FptRenderConfig &cfg [[buffer(1)]],
    device const ushort *program_ids [[buffer(2)]],
    device const RegionalProgramHeader *program_headers [[buffer(3)]],
    device const FptSdfInstruction *instruction_pool [[buffer(4)]],
    device const ushort *primitive_pool [[buffer(5)]],
    uint gid [[thread_position_in_grid]]) {
    if (gid != 0u) return;
    float focal_length = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
    float3 direction = rotateCamera(
        normalize(float3(0.0f, 0.0f, focal_length)), cameraYawPitch(cfg), cfg.camera_roll);
    float3 position = marchRegionalProgram(direction, cameraPos(cfg), 120,
                                           0.001f, 0.001f, cfg,
                                           program_ids, program_headers,
                                           instruction_pool, primitive_pool);
    float distance = length(position - cameraPos(cfg));
    focus_distance[0] = (!isfinite(distance) || distance <= 0.001f ||
                         distance > cfg.render[4] * 0.99f) ? 5.0f : distance;
}

kernel void estimate_bound_grid_focus_distance_kernel(
    device float *focus_distance [[buffer(0)]],
    constant FptRenderConfig &cfg [[buffer(1)]],
    texture3d<float, access::sample> bound_grid [[texture(0)]],
    texture3d<float, access::sample> derivative_lower_grid [[texture(1)]],
    texture3d<float, access::sample> derivative_upper_grid [[texture(2)]],
    uint gid [[thread_position_in_grid]]) {
    if (gid != 0u) return;
    float focal_length = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
    float3 direction = rotateCamera(normalize(float3(0.0f, 0.0f, focal_length)),
                                    cameraYawPitch(cfg), cfg.camera_roll);
    float3 hit = marchBoundGrid(direction, cameraPos(cfg), 120, 0.001f, 0.001f,
                                cfg, bound_grid, derivative_lower_grid,
                                derivative_upper_grid);
    float distance = length(hit - cameraPos(cfg));
    focus_distance[0] = (!isfinite(distance) || distance <= 0.001f ||
                         distance > cfg.render[4] * 0.99f) ? 5.0f : distance;
}

static float3 renderPath(float2 xy, uint sample_idx, constant FptRenderConfig &cfg) {
    float frame = float(sample_idx);
    float3 cam_pos = cameraPos(cfg);
    float3 rp = cam_pos;
    float aa_strength = 0.3f / max(float(cfg.width), float(cfg.height));
    float2 camera_sample = xy + randomPoint(aa_strength, xy, frame);
#if defined(FPT_MANDEL_SPECIALIZED_KERNEL)
    if (!mandelbulberProjectionVisible(camera_sample, cfg)) {
        return float3(0.0f);
    }
    float3 dr = mandelbulberCameraRay(camera_sample, cfg);
#else
    if (cfg.sdf_id == SDF_MANDELBULBER &&
        !mandelbulberProjectionVisible(camera_sample, cfg)) {
        return float3(0.0f);
    }
    float3 dr = cfg.sdf_id == SDF_MANDELBULBER
        ? mandelbulberCameraRay(camera_sample, cfg)
        : rotateCamera(
              normalize(float3(
                  camera_sample,
                  1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f))),
              cameraYawPitch(cfg), cfg.camera_roll);
#endif
    if (cfg.camera_dof > 0.0f) {
        float focus = cfg.focus_distance > 0.0f ? cfg.focus_distance : estimateFocusDistance(cfg);
        float3 fp = rp + dr * focus;
        float2 lens = randomPoint(cfg.camera_dof, xy, frame);
        rp += rotateCamera(float3(lens.x, lens.y, 0.0f), cameraYawPitch(cfg), cfg.camera_roll);
        dr = normalize(fp - rp);
    }

    int local_ni = int(cfg.render[1]);
    float side = 1.0f;
    float3 pixellight = float3(0.0f);
    float3 pixelcolor = float3(1.0f);
    float sky_mask = 0.0f;
    float3 gradient_col = backgroundGradient(dr, cfg);
    int bounces = min(int(cfg.render[0]), 8);
    if (cfg.sdf_bounce_cap > 0u) bounces = min(bounces, int(cfg.sdf_bounce_cap));
    float max_dist = cfg.render[4];
    float max_dist_sq = max_dist * max_dist;
    float far_dist = max_dist * 0.99f;
    float far_dist_sq = far_dist * far_dist;
    for (int i = 0; i < bounces; i++) {
        rp = march(dr, rp, local_ni, cfg.render[3], 0.0002f, cfg);
        float travel_sq = dot(rp - cam_pos, rp - cam_pos);
        if (i == 0 && travel_sq > max_dist_sq) sky_mask = 1.0f;
        local_ni = int(cfg.render[1] / (cfg.render[5] * 2.0f + 1.0f));
        if (travel_sq > far_dist_sq) {
            pixellight += environment(dr, cfg);
            break;
        }
        Material material = userSdf(rp, cfg).material;
        if (material.emission > 0.001f) pixellight += material.rgb * material.emission;

        float3 n = normalAt(rp, cfg);
        if (cfg.sun[0] == 1.0f) pixellight += sunContributionWithSurface(rp, xy, frame, material, n, cfg);
        float r1 = hash13(float3(xy, frame * 1.37f + float(i)));
        float r2 = hash13(float3(xy, frame * 7.91f + float(i)));
        if (r1 > material.translucency) {
            float3 metal = reflect(dr, n);
            float3 diffuse = randomVector(n, xy, frame * 13.37f + float(i));
            float f0 = pow((material.ior - 1.0f) / (material.ior + 1.0f), 2.0f);
            float cosTheta = clamp(dot(n, -dr), 0.0f, 1.0f);
            float fresnel = f0 + (1.0f - f0) * pow5(1.0f - cosTheta);
            dr = normalize(mix(metal, diffuse, material.roughness));
            if (r2 < fresnel * material.specular) {
                dr = metal;
                pixelcolor *= float3(1.0f);
            } else {
                pixelcolor *= material.rgb;
            }
        } else {
            float eta = side == 1.0f ? 1.0f / material.ior : material.ior;
            if (side != 1.0f) n = -n;
            float f0 = pow((material.ior - 1.0f) / (material.ior + 1.0f), 2.0f);
            float cosTheta = clamp(dot(n, -dr), 0.0f, 1.0f);
            float fresnel = f0 + (1.0f - f0) * pow5(1.0f - cosTheta);
            float3 refracted = refract(dr, n, eta);
            float3 reflected = reflect(dr, n);
            if (dot(refracted, refracted) < 0.000001f || !isfinite(refracted.x) || r2 < fresnel) {
                dr = normalize(reflected);
            } else {
                dr = normalize(refracted);
                side *= -1.0f;
                pixelcolor *= mix(float3(1.0f), material.rgb, 0.35f) * (1.0f - fresnel * 0.5f);
            }
        }
        rp += n * 0.001f * sign(dot(dr, n));
        if (cfg.sdf_russian_roulette != 0u && i + 1 < bounces && float(i + 1) >= cfg.sdf_rr_start) {
            float survival = clamp(max(pixelcolor.x, max(pixelcolor.y, pixelcolor.z)), clamp(cfg.sdf_rr_min_prob, 0.01f, 1.0f), 1.0f);
            float rr = hash13(float3(xy, frame * 19.19f + float(i) * 3.17f));
            if (rr > survival) break;
            pixelcolor /= survival;
        }
    }
    float3 col = pixellight * pixelcolor;
    if (cfg.world[6] == 1.0f) col = mix(col, gradient_col, sky_mask);
    return min(col, float3(8.0f));
}

static float3 regionalProgramSunContributionWithSurface(
    float3 position,
    float2 xy,
    float seed,
    Material material,
    float3 normal,
    constant FptRenderConfig &cfg,
    device const ushort *program_ids,
    device const RegionalProgramHeader *program_headers,
    device const FptSdfInstruction *instruction_pool,
    device const ushort *primitive_pool) {
    float3 light_direction = rotateCamera(
        float3(0.0f, 0.0f, 1.0f),
        float2(cfg.sun[1] * pi / 180.0f, cfg.sun[2] * pi / 180.0f));
    float3 original_position = position;
    position += normal * 0.001f;
    float h1 = hash13(float3(xy, seed * 5.0f + 1.0f));
    float h2 = hash13(float3(xy, seed * 3.0f + 5.0f));
    float2 divergence = float2(cos(h1 * 2.0f * pi), sin(h1 * 2.0f * pi)) *
                        sqrt(h2) * cfg.sun[4];
    light_direction = rotateCamera(light_direction, divergence);
    position = marchRegionalProgram(light_direction, position, int(cfg.render[1]),
                                    cfg.render[3], 0.0002f, cfg, program_ids,
                                    program_headers, instruction_pool,
                                    primitive_pool);
    if (length(original_position - position) > cfg.render[4] * 0.99f) {
        return cfg.sun[3] * max(dot(normal, light_direction), 0.0f) *
               material.roughness * (1.0f - material.translucency) *
               float3(cfg.sun_color[0], cfg.sun_color[1], cfg.sun_color[2]);
    }
    return float3(0.0f);
}

static float3 renderRegionalProgramPath(float2 xy,
                                        uint sample_index,
                                        constant FptRenderConfig &cfg,
                                        device const ushort *program_ids,
                                        device const RegionalProgramHeader *program_headers,
                                        device const FptSdfInstruction *instruction_pool,
                                        device const ushort *primitive_pool) {
    float frame = float(sample_index);
    float3 camera_position = cameraPos(cfg);
    float3 position = camera_position;
    float focal_length = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
    float aa_strength = 0.3f / max(float(cfg.width), float(cfg.height));
    float3 direction = normalize(float3(xy + randomPoint(aa_strength, xy, frame),
                                        focal_length));
    direction = rotateCamera(direction, cameraYawPitch(cfg), cfg.camera_roll);
    float focus = cfg.focus_distance > 0.0f ? cfg.focus_distance : 5.0f;
    float3 focus_point = position + direction * focus;
    float2 lens = randomPoint(cfg.camera_dof, xy, frame);
    position += rotateCamera(float3(lens.x, lens.y, 0.0f), cameraYawPitch(cfg), cfg.camera_roll);
    direction = normalize(focus_point - position);

    int local_iterations = int(cfg.render[1]);
    float side = 1.0f;
    float3 pixel_light = float3(0.0f);
    float3 pixel_color = float3(1.0f);
    float sky_mask = 0.0f;
    float3 gradient_color = backgroundGradient(direction, cfg);
    int bounces = min(int(cfg.render[0]), 8);
    if (cfg.sdf_bounce_cap > 0u) bounces = min(bounces, int(cfg.sdf_bounce_cap));
    float maximum_distance = cfg.render[4];
    float maximum_distance_squared = maximum_distance * maximum_distance;
    float far_distance = maximum_distance * 0.99f;
    float far_distance_squared = far_distance * far_distance;
    for (int bounce = 0; bounce < bounces; ++bounce) {
        position = marchRegionalProgram(direction, position, local_iterations,
                                        cfg.render[3], 0.0002f, cfg,
                                        program_ids, program_headers,
                                        instruction_pool, primitive_pool);
        float travel_squared = dot(position - camera_position,
                                   position - camera_position);
        if (bounce == 0 && travel_squared > maximum_distance_squared) {
            sky_mask = 1.0f;
        }
        local_iterations = int(cfg.render[1] / (cfg.render[5] * 2.0f + 1.0f));
        if (travel_squared > far_distance_squared) {
            pixel_light += environment(direction, cfg);
            break;
        }
        Material material = userSdf(position, cfg).material;
        if (material.emission > 0.001f) {
            pixel_light += material.rgb * material.emission;
        }
        float3 normal = normalAt(position, cfg);
        if (cfg.sun[0] == 1.0f) {
            pixel_light += regionalProgramSunContributionWithSurface(
                position, xy, frame, material, normal, cfg, program_ids,
                program_headers, instruction_pool, primitive_pool);
        }
        float random_scatter = hash13(float3(xy, frame * 1.37f + float(bounce)));
        float random_fresnel = hash13(float3(xy, frame * 7.91f + float(bounce)));
        if (random_scatter > material.translucency) {
            float3 reflected = reflect(direction, normal);
            float3 diffuse = randomVector(normal, xy,
                                          frame * 13.37f + float(bounce));
            float f0 = pow((material.ior - 1.0f) /
                           (material.ior + 1.0f), 2.0f);
            float cos_theta = clamp(dot(normal, -direction), 0.0f, 1.0f);
            float fresnel = f0 + (1.0f - f0) * pow5(1.0f - cos_theta);
            direction = normalize(mix(reflected, diffuse, material.roughness));
            if (random_fresnel < fresnel * material.specular) {
                direction = reflected;
            } else {
                pixel_color *= material.rgb;
            }
        } else {
            float eta = side == 1.0f ? 1.0f / material.ior : material.ior;
            if (side != 1.0f) normal = -normal;
            float f0 = pow((material.ior - 1.0f) /
                           (material.ior + 1.0f), 2.0f);
            float cos_theta = clamp(dot(normal, -direction), 0.0f, 1.0f);
            float fresnel = f0 + (1.0f - f0) * pow5(1.0f - cos_theta);
            float3 refracted = refract(direction, normal, eta);
            float3 reflected = reflect(direction, normal);
            if (dot(refracted, refracted) < 0.000001f ||
                !isfinite(refracted.x) || random_fresnel < fresnel) {
                direction = normalize(reflected);
            } else {
                direction = normalize(refracted);
                side *= -1.0f;
                pixel_color *= mix(float3(1.0f), material.rgb, 0.35f) *
                               (1.0f - fresnel * 0.5f);
            }
        }
        position += normal * 0.001f * sign(dot(direction, normal));
        if (cfg.sdf_russian_roulette != 0u && bounce + 1 < bounces &&
            float(bounce + 1) >= cfg.sdf_rr_start) {
            float survival = clamp(
                max(pixel_color.x, max(pixel_color.y, pixel_color.z)),
                clamp(cfg.sdf_rr_min_prob, 0.01f, 1.0f), 1.0f);
            float roulette = hash13(float3(
                xy, frame * 19.19f + float(bounce) * 3.17f));
            if (roulette > survival) break;
            pixel_color /= survival;
        }
    }
    float3 color = pixel_light * pixel_color;
    if (cfg.world[6] == 1.0f) color = mix(color, gradient_color, sky_mask);
    return min(color, float3(8.0f));
}

static RegionalProgramLocalStats profileRegionalProgramPath(
    float2 xy,
    constant FptRenderConfig &cfg,
    device const ushort *program_ids,
    device const RegionalProgramHeader *program_headers,
    device const FptSdfInstruction *instruction_pool,
    device const ushort *primitive_pool) {
    RegionalProgramLocalStats stats = emptyRegionalProgramLocalStats();
    stats.profiled_paths = 1u;
    float frame = 0.0f;
    float3 camera_position = cameraPos(cfg);
    float3 position = camera_position;
    float focal_length = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
    float aa_strength = 0.3f / max(float(cfg.width), float(cfg.height));
    float3 direction = normalize(float3(xy + randomPoint(aa_strength, xy, frame),
                                        focal_length));
    direction = rotateCamera(direction, cameraYawPitch(cfg), cfg.camera_roll);
    float focus = cfg.focus_distance > 0.0f ? cfg.focus_distance : 5.0f;
    float3 focus_point = position + direction * focus;
    float2 lens = randomPoint(cfg.camera_dof, xy, frame);
    position += rotateCamera(float3(lens.x, lens.y, 0.0f), cameraYawPitch(cfg), cfg.camera_roll);
    direction = normalize(focus_point - position);

    int local_iterations = int(cfg.render[1]);
    float side = 1.0f;
    int bounces = min(int(cfg.render[0]), 8);
    if (cfg.sdf_bounce_cap > 0u) bounces = min(bounces, int(cfg.sdf_bounce_cap));
    float far_distance = cfg.render[4] * 0.99f;
    float far_distance_squared = far_distance * far_distance;
    for (int bounce = 0; bounce < bounces; ++bounce) {
        uint ray_class = bounce == 0 ? BoundGridRayPrimary : BoundGridRaySecondary;
        position = marchRegionalProgramTracked(
            direction, position, local_iterations, cfg.render[3], 0.0002f, cfg,
            program_ids, program_headers, instruction_pool, primitive_pool,
            ray_class, stats);
        local_iterations = int(cfg.render[1] / (cfg.render[5] * 2.0f + 1.0f));
        if (dot(position - camera_position, position - camera_position) >
            far_distance_squared) break;

        Material material = userSdf(position, cfg).material;
        float3 normal = normalAt(position, cfg);
        if (cfg.sun[0] == 1.0f) {
            float3 light_direction = rotateCamera(
                float3(0.0f, 0.0f, 1.0f),
                float2(cfg.sun[1] * pi / 180.0f,
                       cfg.sun[2] * pi / 180.0f));
            float h1 = hash13(float3(xy, frame * 5.0f + 1.0f));
            float h2 = hash13(float3(xy, frame * 3.0f + 5.0f));
            float2 divergence = float2(cos(h1 * 2.0f * pi),
                                       sin(h1 * 2.0f * pi)) *
                                sqrt(h2) * cfg.sun[4];
            light_direction = rotateCamera(light_direction, divergence);
            (void)marchRegionalProgramTracked(
                light_direction, position + normal * 0.001f,
                int(cfg.render[1]), cfg.render[3], 0.0002f, cfg, program_ids,
                program_headers, instruction_pool, primitive_pool,
                BoundGridRayShadow, stats);
        }

        float random_scatter = hash13(float3(xy, frame * 1.37f + float(bounce)));
        float random_fresnel = hash13(float3(xy, frame * 7.91f + float(bounce)));
        if (random_scatter > material.translucency) {
            float3 reflected = reflect(direction, normal);
            float3 diffuse = randomVector(normal, xy,
                                          frame * 13.37f + float(bounce));
            float f0 = pow((material.ior - 1.0f) /
                           (material.ior + 1.0f), 2.0f);
            float cos_theta = clamp(dot(normal, -direction), 0.0f, 1.0f);
            float fresnel = f0 + (1.0f - f0) * pow5(1.0f - cos_theta);
            direction = random_fresnel < fresnel * material.specular
                ? normalize(reflected)
                : normalize(mix(reflected, diffuse, material.roughness));
        } else {
            float eta = side == 1.0f ? 1.0f / material.ior : material.ior;
            if (side != 1.0f) normal = -normal;
            float f0 = pow((material.ior - 1.0f) /
                           (material.ior + 1.0f), 2.0f);
            float cos_theta = clamp(dot(normal, -direction), 0.0f, 1.0f);
            float fresnel = f0 + (1.0f - f0) * pow5(1.0f - cos_theta);
            float3 refracted = refract(direction, normal, eta);
            float3 reflected = reflect(direction, normal);
            if (dot(refracted, refracted) < 0.000001f ||
                !isfinite(refracted.x) || random_fresnel < fresnel) {
                direction = normalize(reflected);
            } else {
                direction = normalize(refracted);
                side *= -1.0f;
            }
        }
        position += normal * 0.001f * sign(dot(direction, normal));
    }
    return stats;
}

static float3 boundGridSunContributionWithSurface(
    float3 position,
    float2 xy,
    float seed,
    Material material,
    float3 normal,
    constant FptRenderConfig &cfg,
    texture3d<float, access::sample> bound_grid,
    texture3d<float, access::sample> derivative_lower_grid,
    texture3d<float, access::sample> derivative_upper_grid) {
    float3 light_direction = rotateCamera(
        float3(0.0f, 0.0f, 1.0f),
        float2(cfg.sun[1] * pi / 180.0f, cfg.sun[2] * pi / 180.0f));
    float3 original_position = position;
    position += normal * 0.001f;
    float h1 = hash13(float3(xy, seed * 5.0f + 1.0f));
    float h2 = hash13(float3(xy, seed * 3.0f + 5.0f));
    float2 divergence = float2(cos(h1 * 2.0f * pi), sin(h1 * 2.0f * pi)) *
                        sqrt(h2) * cfg.sun[4];
    light_direction = rotateCamera(light_direction, divergence);
    position = marchBoundGrid(light_direction, position, int(cfg.render[1]),
                              cfg.render[3], 0.0002f, cfg, bound_grid,
                              derivative_lower_grid, derivative_upper_grid);
    if (length(original_position - position) > cfg.render[4] * 0.99f) {
        return cfg.sun[3] * max(dot(normal, light_direction), 0.0f) *
               material.roughness * (1.0f - material.translucency) *
               float3(cfg.sun_color[0], cfg.sun_color[1], cfg.sun_color[2]);
    }
    return float3(0.0f);
}

static float3 renderBoundGridPath(float2 xy,
                                  uint sample_index,
                                  constant FptRenderConfig &cfg,
                                  texture3d<float, access::sample> bound_grid,
                                  texture3d<float, access::sample> derivative_lower_grid,
                                  texture3d<float, access::sample> derivative_upper_grid) {
    float frame = float(sample_index);
    float3 camera_position = cameraPos(cfg);
    float3 position = camera_position;
    float aa_strength = 0.3f / max(float(cfg.width), float(cfg.height));
    float2 camera_sample = xy + randomPoint(aa_strength, xy, frame);
    float focal_length = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
    float3 direction = rotateCamera(normalize(float3(camera_sample, focal_length)),
                                    cameraYawPitch(cfg), cfg.camera_roll);
    if (cfg.camera_dof > 0.0f) {
        float focus = cfg.focus_distance > 0.0f ? cfg.focus_distance : 5.0f;
        float3 focus_point = position + direction * focus;
        float2 lens = randomPoint(cfg.camera_dof, xy, frame);
        position += rotateCamera(float3(lens.x, lens.y, 0.0f),
                                 cameraYawPitch(cfg), cfg.camera_roll);
        direction = normalize(focus_point - position);
    }

    int local_iterations = int(cfg.render[1]);
    float side = 1.0f;
    float3 pixel_light = float3(0.0f);
    float3 pixel_color = float3(1.0f);
    float sky_mask = 0.0f;
    float3 gradient_color = backgroundGradient(direction, cfg);
    int bounces = min(int(cfg.render[0]), 8);
    if (cfg.sdf_bounce_cap > 0u) bounces = min(bounces, int(cfg.sdf_bounce_cap));
    float max_distance = cfg.render[4];
    float max_distance_squared = max_distance * max_distance;
    float far_distance = max_distance * 0.99f;
    float far_distance_squared = far_distance * far_distance;
    for (int bounce = 0; bounce < bounces; ++bounce) {
        position = marchBoundGrid(direction, position, local_iterations,
                                  cfg.render[3], 0.0002f, cfg, bound_grid,
                                  derivative_lower_grid, derivative_upper_grid);
        float travel_squared = dot(position - camera_position,
                                   position - camera_position);
        if (bounce == 0 && travel_squared > max_distance_squared) sky_mask = 1.0f;
        local_iterations = int(cfg.render[1] / (cfg.render[5] * 2.0f + 1.0f));
        if (travel_squared > far_distance_squared) {
            pixel_light += environment(direction, cfg);
            break;
        }
        Material material = userSdf(position, cfg).material;
        if (material.emission > 0.001f) {
            pixel_light += material.rgb * material.emission;
        }

        float3 normal = normalAt(position, cfg);
        if (cfg.sun[0] == 1.0f) {
            pixel_light += boundGridSunContributionWithSurface(
                position, xy, frame, material, normal, cfg, bound_grid,
                derivative_lower_grid, derivative_upper_grid);
        }
        float random_scatter = hash13(float3(xy, frame * 1.37f + float(bounce)));
        float random_fresnel = hash13(float3(xy, frame * 7.91f + float(bounce)));
        if (random_scatter > material.translucency) {
            float3 reflected = reflect(direction, normal);
            float3 diffuse = randomVector(normal, xy, frame * 13.37f + float(bounce));
            float f0 = pow((material.ior - 1.0f) / (material.ior + 1.0f), 2.0f);
            float cos_theta = clamp(dot(normal, -direction), 0.0f, 1.0f);
            float fresnel = f0 + (1.0f - f0) * pow5(1.0f - cos_theta);
            direction = normalize(mix(reflected, diffuse, material.roughness));
            if (random_fresnel < fresnel * material.specular) {
                direction = reflected;
            } else {
                pixel_color *= material.rgb;
            }
        } else {
            float eta = side == 1.0f ? 1.0f / material.ior : material.ior;
            if (side != 1.0f) normal = -normal;
            float f0 = pow((material.ior - 1.0f) / (material.ior + 1.0f), 2.0f);
            float cos_theta = clamp(dot(normal, -direction), 0.0f, 1.0f);
            float fresnel = f0 + (1.0f - f0) * pow5(1.0f - cos_theta);
            float3 refracted = refract(direction, normal, eta);
            float3 reflected = reflect(direction, normal);
            if (dot(refracted, refracted) < 0.000001f || !isfinite(refracted.x) ||
                random_fresnel < fresnel) {
                direction = normalize(reflected);
            } else {
                direction = normalize(refracted);
                side *= -1.0f;
                pixel_color *= mix(float3(1.0f), material.rgb, 0.35f) *
                               (1.0f - fresnel * 0.5f);
            }
        }
        position += normal * 0.001f * sign(dot(direction, normal));
        if (cfg.sdf_russian_roulette != 0u && bounce + 1 < bounces &&
            float(bounce + 1) >= cfg.sdf_rr_start) {
            float survival = clamp(max(pixel_color.x, max(pixel_color.y, pixel_color.z)),
                                   clamp(cfg.sdf_rr_min_prob, 0.01f, 1.0f), 1.0f);
            float roulette = hash13(float3(xy, frame * 19.19f + float(bounce) * 3.17f));
            if (roulette > survival) break;
            pixel_color /= survival;
        }
    }
    float3 color = pixel_light * pixel_color;
    if (cfg.world[6] == 1.0f) color = mix(color, gradient_color, sky_mask);
    return min(color, float3(8.0f));
}

static BoundGridLocalStats profileBoundGridPath(
    float2 xy,
    constant FptRenderConfig &cfg,
    texture3d<float, access::sample> bound_grid,
    texture3d<float, access::sample> derivative_lower_grid,
    texture3d<float, access::sample> derivative_upper_grid) {
    BoundGridLocalStats stats = emptyBoundGridLocalStats();
    stats.profiled_paths = 1u;
    float frame = 0.0f;
    float3 camera_position = cameraPos(cfg);
    float3 position = camera_position;
    float aa_strength = 0.3f / max(float(cfg.width), float(cfg.height));
    float2 camera_sample = xy + randomPoint(aa_strength, xy, frame);
    float focal_length = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
    float3 direction = rotateCamera(normalize(float3(camera_sample, focal_length)),
                                    cameraYawPitch(cfg), cfg.camera_roll);
    float focus = cfg.focus_distance > 0.0f ? cfg.focus_distance : 5.0f;
    float3 focus_point = position + direction * focus;
    float2 lens = randomPoint(cfg.camera_dof, xy, frame);
    position += rotateCamera(float3(lens.x, lens.y, 0.0f), cameraYawPitch(cfg), cfg.camera_roll);
    direction = normalize(focus_point - position);

    int local_iterations = int(cfg.render[1]);
    float side = 1.0f;
    int bounces = min(int(cfg.render[0]), 8);
    if (cfg.sdf_bounce_cap > 0u) bounces = min(bounces, int(cfg.sdf_bounce_cap));
    float far_distance = cfg.render[4] * 0.99f;
    float far_distance_squared = far_distance * far_distance;
    for (int bounce = 0; bounce < bounces; ++bounce) {
        uint ray_class = bounce == 0 ? BoundGridRayPrimary : BoundGridRaySecondary;
        position = marchBoundGridTracked(direction, position, local_iterations,
                                         cfg.render[3], 0.0002f, cfg, bound_grid,
                                         derivative_lower_grid,
                                         derivative_upper_grid,
                                         ray_class, stats);
        local_iterations = int(cfg.render[1] / (cfg.render[5] * 2.0f + 1.0f));
        if (dot(position - camera_position, position - camera_position) >
            far_distance_squared) break;

        Material material = userSdf(position, cfg).material;
        float3 normal = normalAt(position, cfg);
        if (cfg.sun[0] == 1.0f) {
            float3 light_direction = rotateCamera(
                float3(0.0f, 0.0f, 1.0f),
                float2(cfg.sun[1] * pi / 180.0f, cfg.sun[2] * pi / 180.0f));
            float h1 = hash13(float3(xy, frame * 5.0f + 1.0f));
            float h2 = hash13(float3(xy, frame * 3.0f + 5.0f));
            float2 divergence = float2(cos(h1 * 2.0f * pi), sin(h1 * 2.0f * pi)) *
                                sqrt(h2) * cfg.sun[4];
            light_direction = rotateCamera(light_direction, divergence);
            (void)marchBoundGridTracked(light_direction, position + normal * 0.001f,
                                        int(cfg.render[1]), cfg.render[3], 0.0002f,
                                        cfg, bound_grid, derivative_lower_grid,
                                        derivative_upper_grid,
                                        BoundGridRayShadow, stats);
        }

        float random_scatter = hash13(float3(xy, frame * 1.37f + float(bounce)));
        float random_fresnel = hash13(float3(xy, frame * 7.91f + float(bounce)));
        if (random_scatter > material.translucency) {
            float3 reflected = reflect(direction, normal);
            float3 diffuse = randomVector(normal, xy, frame * 13.37f + float(bounce));
            float f0 = pow((material.ior - 1.0f) / (material.ior + 1.0f), 2.0f);
            float cos_theta = clamp(dot(normal, -direction), 0.0f, 1.0f);
            float fresnel = f0 + (1.0f - f0) * pow5(1.0f - cos_theta);
            direction = random_fresnel < fresnel * material.specular
                ? normalize(reflected)
                : normalize(mix(reflected, diffuse, material.roughness));
        } else {
            float eta = side == 1.0f ? 1.0f / material.ior : material.ior;
            if (side != 1.0f) normal = -normal;
            float f0 = pow((material.ior - 1.0f) / (material.ior + 1.0f), 2.0f);
            float cos_theta = clamp(dot(normal, -direction), 0.0f, 1.0f);
            float fresnel = f0 + (1.0f - f0) * pow5(1.0f - cos_theta);
            float3 refracted = refract(direction, normal, eta);
            float3 reflected = reflect(direction, normal);
            if (dot(refracted, refracted) < 0.000001f || !isfinite(refracted.x) ||
                random_fresnel < fresnel) {
                direction = normalize(reflected);
            } else {
                direction = normalize(refracted);
                side *= -1.0f;
            }
        }
        position += normal * 0.001f * sign(dot(direction, normal));
    }
    return stats;
}

static float3 voxelSunContributionWithSurface(float3 position,
                                               float2 xy,
                                               float seed,
                                               Material material,
                                               float3 shading_normal,
                                               float3 offset_normal,
                                               constant FptRenderConfig &cfg,
                                               device const VoxelCell *cells,
                                               device const uint *page_table) {
    float3 light_direction = rotateCamera(float3(0.0f, 0.0f, 1.0f),
                                          float2(cfg.sun[1] * pi / 180.0f, cfg.sun[2] * pi / 180.0f));
    float h1 = hash13(float3(xy, seed * 5.0f + 1.0f));
    float h2 = hash13(float3(xy, seed * 3.0f + 5.0f));
    float2 divergence = float2(cos(h1 * 2.0f * pi), sin(h1 * 2.0f * pi)) * sqrt(h2) * cfg.sun[4];
    light_direction = rotateCamera(light_direction, divergence);
    float3 shadow_origin = offsetVoxelRayOrigin(position, offset_normal,
                                                light_direction, cfg);
    VoxelHit shadow = traceVoxel(shadow_origin, light_direction,
                                 cfg, cells, page_table);
    if (!shadow.hit) {
        float diffuse = max(dot(shading_normal, light_direction), 0.0f);
        return cfg.sun[3] * diffuse * material.roughness * (1.0f - material.translucency) *
               float3(cfg.sun_color[0], cfg.sun_color[1], cfg.sun_color[2]);
    }
    return float3(0.0f);
}

static float estimateVoxelFocusDistance(constant FptRenderConfig &cfg,
                                        device const VoxelCell *cells,
                                        device const uint *page_table) {
    float focal_length = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
    float3 direction = rotateCamera(normalize(float3(0.0f, 0.0f, focal_length)), cameraYawPitch(cfg), cfg.camera_roll);
    VoxelHit hit = traceVoxel(cameraPos(cfg), direction, cfg, cells, page_table);
    return hit.hit && hit.surface_distance > 0.001f ? hit.surface_distance : 5.0f;
}

static float3 renderVoxelPath(float2 xy,
                              uint sample_index,
                              constant FptRenderConfig &cfg,
                              device const VoxelCell *cells,
                              device const uint *page_table) {
    float frame = float(sample_index);
    float3 position = cameraPos(cfg);
    float focal_length = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
    float aa_strength = 0.3f / max(float(cfg.width), float(cfg.height));
    float3 direction = normalize(float3(xy + randomPoint(aa_strength, xy, frame), focal_length));
    direction = rotateCamera(direction, cameraYawPitch(cfg), cfg.camera_roll);
    float focus = cfg.focus_distance > 0.0f ? cfg.focus_distance : estimateVoxelFocusDistance(cfg, cells, page_table);
    float3 focus_point = position + direction * focus;
    float2 lens = randomPoint(cfg.camera_dof, xy, frame);
    position += rotateCamera(float3(lens.x, lens.y, 0.0f), cameraYawPitch(cfg), cfg.camera_roll);
    direction = normalize(focus_point - position);

    float3 pixel_light = float3(0.0f);
    float3 pixel_color = float3(1.0f);
    float sky_mask = 0.0f;
    float3 gradient_color = backgroundGradient(direction, cfg);
    int bounces = min(int(cfg.render[0]), 8);
    if (cfg.sdf_bounce_cap > 0u) bounces = min(bounces, int(cfg.sdf_bounce_cap));
    float cell_size = min(voxelCellSize(cfg).x, min(voxelCellSize(cfg).y, voxelCellSize(cfg).z));
    for (int bounce = 0; bounce < bounces; ++bounce) {
        VoxelHit hit = traceVoxel(position, direction, cfg, cells, page_table);
        if (!hit.hit) {
            if (bounce == 0) sky_mask = 1.0f;
            pixel_light += environment(direction, cfg);
            break;
        }

        Material material = voxelMaterialAtHit(hit, cfg);
        if (material.emission > 0.001f) pixel_light += material.rgb * material.emission;
        float3 normal = voxelShadingNormal(hit, cfg);
        if (dot(normal, direction) > 0.0f) normal = -normal;
        float3 offset_normal = cfg.voxel_normal_mode == 2u ? hit.normal : normal;
        if (dot(offset_normal, direction) > 0.0f) offset_normal = -offset_normal;
        if (cfg.sun[0] == 1.0f) {
            pixel_light += voxelSunContributionWithSurface(hit.position,
                                                            xy,
                                                            frame,
                                                            material,
                                                            normal,
                                                            offset_normal,
                                                            cfg,
                                                            cells,
                                                            page_table);
        }

        float random_scatter = hash13(float3(xy, frame * 1.37f + float(bounce)));
        float random_fresnel = hash13(float3(xy, frame * 7.91f + float(bounce)));
        bool transmitted = false;
        if (random_scatter > material.translucency) {
            float3 reflected = reflect(direction, normal);
            float3 diffuse = randomVector(normal, xy, frame * 13.37f + float(bounce));
            float f0 = pow((material.ior - 1.0f) / (material.ior + 1.0f), 2.0f);
            float cos_theta = clamp(dot(normal, -direction), 0.0f, 1.0f);
            float fresnel = f0 + (1.0f - f0) * pow5(1.0f - cos_theta);
            direction = normalize(mix(reflected, diffuse, material.roughness));
            if (random_fresnel < fresnel * material.specular) {
                direction = reflected;
            } else {
                pixel_color *= material.rgb;
            }
        } else {
            float f0 = pow((material.ior - 1.0f) / (material.ior + 1.0f), 2.0f);
            float cos_theta = clamp(dot(normal, -direction), 0.0f, 1.0f);
            float fresnel = f0 + (1.0f - f0) * pow5(1.0f - cos_theta);
            float3 refracted = refract(direction, normal, 1.0f / material.ior);
            float3 reflected = reflect(direction, normal);
            if (dot(refracted, refracted) < 0.000001f || !isfinite(refracted.x) || random_fresnel < fresnel) {
                direction = normalize(reflected);
            } else {
                direction = normalize(refracted);
                pixel_color *= mix(float3(1.0f), material.rgb, 0.35f) * (1.0f - fresnel * 0.5f);
                transmitted = true;
            }
        }
        position = transmitted
            ? hit.position + direction * cell_size * 1.05f
            : offsetVoxelRayOrigin(hit.position, offset_normal, direction, cfg);

        if (cfg.sdf_russian_roulette != 0u && bounce + 1 < bounces && float(bounce + 1) >= cfg.sdf_rr_start) {
            float survival = clamp(max(pixel_color.x, max(pixel_color.y, pixel_color.z)),
                                   clamp(cfg.sdf_rr_min_prob, 0.01f, 1.0f),
                                   1.0f);
            float roulette = hash13(float3(xy, frame * 19.19f + float(bounce) * 3.17f));
            if (roulette > survival) break;
            pixel_color /= survival;
        }
    }
    float3 color = pixel_light * pixel_color;
    if (cfg.world[6] == 1.0f) color = mix(color, gradient_color, sky_mask);
    return min(color, float3(8.0f));
}

static bool raySphere(float3 ro, float3 rd, float3 center, float radius, thread float &t0, thread float &t1) {
    float3 oc = ro - center;
    float b = dot(oc, rd);
    float c = dot(oc, oc) - radius * radius;
    float h = b * b - c;
    if (h < 0.0f) return false;
    h = sqrt(h);
    t0 = -b - h;
    t1 = -b + h;
    return t1 > 0.0f;
}

static float3 glassSceneSurface(float3 p, float3 rd, constant FptRenderConfig &cfg) {
    float3 floor_col = float3(0.43f, 0.44f, 0.40f);
    float3 back_col = float3(0.50f, 0.62f, 0.78f);
    float3 col = environment(rd, cfg) * 0.12f + back_col * 0.75f;
    float best_t = 1.0e20f;
    if (rd.y < -0.0001f) {
        float tf = (-0.72f - p.y) / rd.y;
        if (tf > 0.0f && tf < best_t) {
            best_t = tf;
            float3 fp = p + rd * tf;
            float grid = smoothstep(0.018f, 0.0f, min(abs(fract(fp.x * 2.0f) - 0.5f), abs(fract(fp.z * 2.0f) - 0.5f)));
            float dist_fade = clamp(length(fp.xz) * 0.08f, 0.0f, 0.22f);
            float3 shadow_center = float3(0.0f, -0.72f, 0.35f);
            float shadow = smoothstep(0.82f, 0.0f, length((fp - shadow_center).xz));
            col = mix(floor_col, floor_col * 0.82f, grid * 0.08f) + dist_fade;
            col *= mix(1.0f, 0.60f, shadow * 0.42f);
        }
    }
    if (rd.z > 0.0001f) {
        float tb = (1.8f - p.z) / rd.z;
        if (tb > 0.0f && tb < best_t) {
            best_t = tb;
            float3 bp = p + rd * tb;
            float h = clamp((bp.y + 0.72f) / 1.9f, 0.0f, 1.0f);
            col = mix(float3(0.62f, 0.74f, 0.88f), back_col, h);
        }
    }
    return col;
}

static float3 renderGlassAnalytic(float2 xy, uint sample_idx, constant FptRenderConfig &cfg) {
    float focal_length = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
    float aa = 0.18f / max(float(cfg.width), float(cfg.height));
    float2 jitter = randomPoint(aa, xy, float(sample_idx));
    float3 ro = cameraPos(cfg);
    float3 rd = rotateCamera(normalize(float3(xy + jitter, focal_length)), cameraYawPitch(cfg), cfg.camera_roll);

    float3 center = float3(0.0f, -0.22f, 0.35f);
    float radius = 0.50f;
    float t0 = 0.0f;
    float t1 = 0.0f;
    float3 col = glassSceneSurface(ro, rd, cfg);
    if (raySphere(ro, rd, center, radius, t0, t1) && t1 > 0.0f) {
        float t = max(t0, 0.0f);
        float3 p0 = ro + rd * t;
        float3 n0 = normalize(p0 - center);
        float cosi = clamp(dot(n0, -rd), 0.0f, 1.0f);
        float fresnel = 0.04f + 0.96f * pow5(1.0f - cosi);

        float3 into = refract(rd, n0, 1.0f / 1.5f);
        if (length(into) < 0.001f || !isfinite(into.x)) into = reflect(rd, n0);
        float inner_h = sqrt(max(radius * radius - dot(p0 + into * 0.001f - center, p0 + into * 0.001f - center), 0.0f));
        float exit_t = max(t1 - t, 0.02f);
        float3 p1 = p0 + into * exit_t;
        float3 n1 = -normalize(p1 - center);
        float3 out_dir = refract(into, n1, 1.5f);
        if (length(out_dir) < 0.001f || !isfinite(out_dir.x)) out_dir = reflect(into, n1);
        out_dir = normalize(out_dir);

        float3 refr_col = glassSceneSurface(p1 + out_dir * 0.01f, out_dir, cfg);
        float3 refl_col = environment(reflect(rd, n0), cfg) * 0.28f + float3(0.82f, 0.90f, 1.0f) * 0.24f;
        float rim = pow(1.0f - cosi, 2.2f);
        float highlight = smoothstep(0.985f, 1.0f, dot(reflect(rd, n0), normalize(float3(-0.35f, 0.8f, -0.45f))));
        float absorption = exp(-exit_t * 0.16f);
        col = mix(refr_col * float3(0.86f, 0.94f, 1.0f) * absorption, refl_col, fresnel);
        col += rim * float3(0.92f, 0.98f, 1.0f) * 0.22f;
        col += highlight * float3(1.0f, 0.92f, 0.78f) * 1.2f;
        float contact = smoothstep(0.62f, 0.0f, length((ro + rd * max(t1, 0.0f)).xz - center.xz));
        col = mix(col, col * 0.88f, contact * 0.10f);
        (void)inner_h;
    }
    return min(col, float3(8.0f));
}

static float3 viewportShade(float3 rp,
                            float3 dr,
                            bool hit,
                            float hit_lod,
                            constant FptRenderConfig &cfg) {
    float3 sky_col = clamp(environment(dr, cfg), 0.0f, 1.0f);
    if (cfg.world[6] == 1.0f) sky_col = backgroundGradient(dr, cfg);
    if (!hit) return sky_col;
    float3 n = normalAt(rp, cfg);
    float3 li = normalize(float3(1.0f, 0.3f, 0.0f));
    float3 col = clamp(userSdf(rp, cfg).material.rgb, float3(0.0f), float3(1.0f));
    float confidence = smoothstep(hit_lod * 12.0f, hit_lod * 2.0f, abs(mapSdf(rp, cfg)));
    float diffuse = max(dot(li, n), 0.01f);
    float shadow_area = max(-dot(n, li), 0.0f);
    col = diffuse * col;
    col = mix(col, col / 3.0f, shadow_area);
    col = pow(col, float3(1.0f / 2.2f));
    return mix(sky_col, col, confidence);
}

static float3 viewport(float2 xy, constant FptRenderConfig &cfg) {
    float3 rp = cameraPos(cfg);
    float f = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
    float3 dr = rotateCamera(normalize(float3(xy, f)), cameraYawPitch(cfg), cfg.camera_roll);
    float3 cam_pos = rp;
    float min_dist = 0.001f;
    float lod_falloff = 0.0002f;
    bool hit = false;
    float travel = 0.0f;
    float hit_lod = min_dist;
    for (int i = 0; i < 160; i++) {
        float d = mapSdf(rp, cfg);
        if (!isfinite(d)) break;
        float o = sdfMarchStep(d, min_dist, cfg);
        float lod = min_dist;
        float fog_lod = dot(cam_pos - rp, cam_pos - rp);
        lod = mix(lod, 0.1f, fog_lod * lod_falloff);
        hit_lod = lod;
        if (o < lod && travel > min_dist * 8.0f) { hit = true; break; }
        if (travel > cfg.render[4]) break;
        float step_len = clamp(o, min_dist, 1.5f);
        rp += dr * step_len;
        travel += step_len;
    }
    return viewportShade(rp, dr, hit, hit_lod, cfg);
}

static float3 voxelViewport(float2 xy,
                            constant FptRenderConfig &cfg,
                            device const VoxelCell *cells,
                            device const uint *page_table) {
    float focal_length = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
    float3 direction = rotateCamera(normalize(float3(xy, focal_length)), cameraYawPitch(cfg), cfg.camera_roll);
    VoxelHit hit = traceVoxel(cameraPos(cfg), direction, cfg, cells, page_table);
    float3 sky_color = cfg.world[6] == 1.0f
        ? backgroundGradient(direction, cfg)
        : clamp(environment(direction, cfg), 0.0f, 1.0f);
    if (!hit.hit) return sky_color;
    float3 normal = voxelShadingNormal(hit, cfg);
    if (dot(normal, direction) > 0.0f) normal = -normal;
    float3 light_direction = normalize(float3(1.0f, 0.3f, 0.0f));
    float diffuse = max(dot(light_direction, normal), 0.01f);
    float shadow_area = max(-dot(normal, light_direction), 0.0f);
    Material material = voxelMaterialAtHit(hit, cfg);
    float3 color = diffuse * material.rgb;
    color = mix(color, color / 3.0f, shadow_area);
    return pow(clamp(color, 0.0f, 1.0f), float3(1.0f / 2.2f));
}

static float3 gammaSrgb(float3 lin) {
    float3 lo = lin * 12.92f;
    float3 hi = 1.055f * pow(max(lin, float3(0.0f)), float3(1.0f / 2.4f)) - 0.055f;
    return select(hi, lo, lin <= float3(0.0031308f));
}

static float3 aces(float3 x) {
    const float a = 2.51f;
    const float b = 0.03f;
    const float c = 2.43f;
    const float d = 0.59f;
    const float e = 0.14f;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), 0.0f, 1.0f);
}

static float3 pbrNeutral(float3 color) {
    const float startCompression = 0.76f;
    const float desaturation = 0.15f;
    float x = min(color.r, min(color.g, color.b));
    float offset = x < 0.08f ? x - 6.25f * x * x : 0.04f;
    color -= offset;
    float peak = max(color.r, max(color.g, color.b));
    if (peak < startCompression) return color;
    float d = 1.0f - startCompression;
    float newPeak = 1.0f - d * d / (peak + d - startCompression);
    color *= newPeak / peak;
    float g = 1.0f - 1.0f / (desaturation * (peak - newPeak) + 1.0f);
    return mix(color, newPeak * float3(1.0f), g);
}

static float3 toneMap(float3 color, constant FptRenderConfig &cfg) {
    float n = cfg.post[0];
    if (n == 0.0f) return gammaSrgb(color);
    if (n == 1.0f) return pow(color, float3(1.0f / 2.4f));
    if (n == 2.0f) return pow(color, float3(1.0f / 2.6f));
    if (n == 3.0f) return aces(color);
    if (n == 4.0f) return pbrNeutral(color);
    return color;
}

static float3 postProcess(float3 color, constant FptRenderConfig &cfg) {
    color = toneMap(color, cfg);
    color = color * cfg.post[1] + cfg.post[2];
    color = (color - float3(0.5f)) * cfg.post[4] + float3(0.5f);
    float l = dot(color, float3(0.2126f, 0.7152f, 0.0722f));
    color = mix(float3(l), color, cfg.post[3]);
    return clamp(color, 0.0f, 1.0f);
}

static float3 diagnosticEncodeDepth(float travel, bool hit, constant FptRenderConfig &cfg, constant FptDiagnosticConfig &diag) {
    if (!hit) return float3(0.0f);
    float max_distance = diag.max_distance > 0.001f ? diag.max_distance : cfg.render[4];
    float v = 1.0f - clamp(travel / max(max_distance, 0.001f), 0.0f, 1.0f);
    return float3(v);
}

static float3 diagnosticEncodeNormal(float3 n) {
    return clamp(n * 0.5f + 0.5f, float3(0.0f), float3(1.0f));
}

static float3 diagnosticEncodeHdr(float3 color, constant FptRenderConfig &cfg) {
    return postProcess(min(max(color, float3(0.0f)), float3(8.0f)), cfg);
}

static float3 diagnosticEncodeScalar(float value, float scale) {
    float v = clamp(value / max(scale, 1.0f), 0.0f, 1.0f);
    return float3(v, v, v);
}

static float2 sdfScreenUv(uint2 gid, constant FptRenderConfig &cfg) {
    float2 pixel_offset = cfg.sdf_id == SDF_MANDELBULBER
        ? float2(0.0f, 1.0f)
        : float2(0.5f);
    float2 uv = (float2(gid) + pixel_offset) /
                    float2(float(cfg.width), float(cfg.height)) - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
    return uv;
}

static uint marchStepCount(float3 dr, float3 rp, int ni, float min_dist, float lod_falloff, constant FptRenderConfig &cfg) {
    float3 cam_pos = rp;
    int max_iter = sdfMarchIterationLimit(ni, cfg);
    uint count = 0u;
    bool check_translucency = sdfHasTranslucentSurfaces(cfg);
    for (int i = 0; i < max_iter; i++) {
        count++;
        float d = mapSdf(rp, cfg);
        if (!isfinite(d)) break;
        float threshold = cfg.sdf_id == SDF_MANDELBULBER
            ? mandelbulberMarchThreshold(rp, cfg)
            : min_dist;
        float o = sdfMarchStep(d, threshold, cfg);
        rp += dr * o;
        float lod = threshold;
        if (lod_falloff > 0.00001f) {
            float fog_lod = dot(cam_pos - rp, cam_pos - rp);
            lod = mix(lod, 0.1f, fog_lod * lod_falloff);
            lod = mix(threshold, lod, cfg.render[5]);
        }
        if (check_translucency && userSdf(rp, cfg).material.translucency > 0.0f) lod = 0.0001f;
        if (sdfMarchConverged(d, o, lod, cfg)) break;
        if (cfg.render[4] < o) break;
    }
    return count;
}

static float3 marchCounted(float3 dr, float3 rp, int ni, float min_dist, float lod_falloff, constant FptRenderConfig &cfg, thread uint &steps) {
    float3 cam_pos = rp;
    int max_iter = sdfMarchIterationLimit(ni, cfg);
    bool check_translucency = sdfHasTranslucentSurfaces(cfg);
    for (int i = 0; i < max_iter; i++) {
        steps++;
        float d = mapSdf(rp, cfg);
        if (!isfinite(d)) break;
        float threshold = cfg.sdf_id == SDF_MANDELBULBER
            ? mandelbulberMarchThreshold(rp, cfg)
            : min_dist;
        float o = sdfMarchStep(d, threshold, cfg);
        rp += dr * o;
        float lod = threshold;
        if (lod_falloff > 0.00001f) {
            float fog_lod = dot(cam_pos - rp, cam_pos - rp);
            lod = mix(lod, 0.1f, fog_lod * lod_falloff);
            lod = mix(threshold, lod, cfg.render[5]);
        }
        if (check_translucency && userSdf(rp, cfg).material.translucency > 0.0f) lod = 0.0001f;
        if (sdfMarchConverged(d, o, lod, cfg)) break;
        if (cfg.render[4] < o) break;
    }
    return rp;
}

static uint sunStepCountWithNormal(float3 rp, float2 xy, float seed, float3 n, constant FptRenderConfig &cfg) {
    float3 light_dir = rotateCamera(float3(0.0f, 0.0f, 1.0f), float2(cfg.sun[1] * pi / 180.0f, cfg.sun[2] * pi / 180.0f));
    float h1 = hash13(float3(xy, seed * 5.0f + 1.0f));
    float h2 = hash13(float3(xy, seed * 3.0f + 5.0f));
    float2 div = float2(cos(h1 * 2.0f * pi), sin(h1 * 2.0f * pi)) * sqrt(h2) * cfg.sun[4];
    light_dir = rotateCamera(light_dir, div);
    return marchStepCount(light_dir, rp + n * 0.001f, int(cfg.render[1]), cfg.render[3], 0.0002f, cfg);
}

static float3 sdfPathCostDiagnostic(float2 xy, constant FptRenderConfig &cfg, constant FptDiagnosticConfig &diag) {
    uint samples = min(max(cfg.samples, 1u), 8u);
    float total_primary = 0.0f;
    float total_shadow = 0.0f;
    float total_normal = 0.0f;
    float total_bounces = 0.0f;
    for (uint sample_idx = 0u; sample_idx < samples; ++sample_idx) {
        float frame = float(sample_idx);
        float3 cam_pos = cameraPos(cfg);
        float3 rp = cam_pos;
        float focal_length = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
        float aa_strength = 0.3f / max(float(cfg.width), float(cfg.height));
        float3 dr = normalize(float3(xy + randomPoint(aa_strength, xy, frame), focal_length));
        dr = rotateCamera(dr, cameraYawPitch(cfg), cfg.camera_roll);
        if (cfg.camera_dof > 0.0f) {
            float focus = cfg.focus_distance > 0.0f ? cfg.focus_distance : estimateFocusDistance(cfg);
            float3 fp = rp + dr * focus;
            float2 lens = randomPoint(cfg.camera_dof, xy, frame);
            rp += rotateCamera(float3(lens.x, lens.y, 0.0f), cameraYawPitch(cfg), cfg.camera_roll);
            dr = normalize(fp - rp);
        }

        int local_ni = int(cfg.render[1]);
        int bounces = min(int(cfg.render[0]), 8);
        if (cfg.sdf_bounce_cap > 0u) bounces = min(bounces, int(cfg.sdf_bounce_cap));
        float far_dist = cfg.render[4] * 0.99f;
        float far_dist_sq = far_dist * far_dist;
        for (int i = 0; i < bounces; i++) {
            uint primary_steps = 0u;
            rp = marchCounted(dr, rp, local_ni, cfg.render[3], 0.0002f, cfg, primary_steps);
            total_primary += float(primary_steps);
            local_ni = int(cfg.render[1] / (cfg.render[5] * 2.0f + 1.0f));
            if (dot(rp - cam_pos, rp - cam_pos) > far_dist_sq) break;
            Material material = userSdf(rp, cfg).material;
            total_bounces += 1.0f;
            total_normal += 1.0f;
            float3 n = normalAt(rp, cfg);
            if (cfg.sun[0] == 1.0f) total_shadow += float(sunStepCountWithNormal(rp, xy, frame, n, cfg));
            float r1 = hash13(float3(xy, frame * 1.37f + float(i)));
            float r2 = hash13(float3(xy, frame * 7.91f + float(i)));
            if (r1 > material.translucency) {
                float3 metal = reflect(dr, n);
                float3 diffuse = randomVector(n, xy, frame * 13.37f + float(i));
                float3 specular = reflect(dr, n);
                float f0 = pow((material.ior - 1.0f) / (material.ior + 1.0f), 2.0f);
                float cosTheta = clamp(dot(n, -dr), 0.0f, 1.0f);
                float fresnel = f0 + (1.0f - f0) * pow5(1.0f - cosTheta);
                dr = normalize(mix(metal, diffuse, material.roughness));
                if (r2 < fresnel * material.specular) dr = specular;
            } else {
                float eta = 1.0f / material.ior;
                float f0 = pow((material.ior - 1.0f) / (material.ior + 1.0f), 2.0f);
                float cosTheta = clamp(dot(n, -dr), 0.0f, 1.0f);
                float fresnel = f0 + (1.0f - f0) * pow5(1.0f - cosTheta);
                float3 refracted = refract(dr, n, eta);
                float3 reflected = reflect(dr, n);
                dr = (dot(refracted, refracted) < 0.000001f || !isfinite(refracted.x) || r2 < fresnel) ? normalize(reflected) : normalize(refracted);
            }
            rp += n * 0.001f * sign(dot(dr, n));
        }
    }
    float inv_samples = 1.0f / float(samples);
    if (diag.mode == FPT_DIAGNOSTIC_SDF_PRIMARY_STEPS) return diagnosticEncodeScalar(total_primary * inv_samples, max(cfg.render[1] * max(cfg.render[0], 1.0f), 1.0f));
    if (diag.mode == FPT_DIAGNOSTIC_SDF_SHADOW_STEPS) return diagnosticEncodeScalar(total_shadow * inv_samples, max(cfg.render[1] * max(cfg.render[0], 1.0f), 1.0f));
    if (diag.mode == FPT_DIAGNOSTIC_SDF_NORMAL_EVALS) return diagnosticEncodeScalar(total_normal * inv_samples, max(cfg.render[0] * 2.0f, 1.0f));
    return diagnosticEncodeScalar(total_bounces * inv_samples, max(cfg.render[0], 1.0f));
}

struct SdfProfileLocalStats {
    uint primary_steps;
    uint secondary_steps;
    uint shadow_steps;
    uint normal_evals;
    uint bounces;
    uint distance_evals;
    uint march_orbit_iterations;
    uint refinement_steps;
    uint normal_field_evals;
    uint material_evals;
    uint max_ray_steps;
    uint phase;
    uint distance_evals_by_phase[4];
    uint orbit_iterations_by_phase[4];
    uint formula_slot_iterations[9];
    uint refinement_distance_evals;
};

static float mandelbulberProfileDistance(float3 position,
                                         constant FptRenderConfig &cfg,
                                         thread SdfProfileLocalStats &stats) {
    float4 sample = mandelbulberFieldSample(position, cfg, 1);
    stats.distance_evals += 1u;
    uint completed_iterations = uint(abs(sample.w));
    MandelFormulaIterationCounts formula_iterations =
        mandelbulberProfileFormulaIterations(int(completed_iterations));
    uint actual_orbit_iterations = 0u;
    for (uint slot = 0u; slot < 9u; ++slot) {
        uint iterations = formula_iterations.slots[slot];
        stats.formula_slot_iterations[slot] += iterations;
        actual_orbit_iterations += iterations;
    }
    if (actual_orbit_iterations == 0u) {
        actual_orbit_iterations = completed_iterations;
    }
    uint phase = min(stats.phase, 3u);
    stats.distance_evals_by_phase[phase] += 1u;
    stats.orbit_iterations_by_phase[phase] += actual_orbit_iterations;
    stats.march_orbit_iterations += actual_orbit_iterations;
    float distance = sample.x;
    if (cfg.vset_values[115] > 1.5f) {
        float threshold = clamp(
            length(cameraPos(cfg) - position) * cfg.vset_values[116],
            cfg.vset_values[118], cfg.vset_values[119]);
        if (sample.w > 0.0f) {
            distance = 0.0f;
        } else if (distance < threshold) {
            distance = threshold * 1.01f;
        }
    }
    return mix(distance, distance * 0.5f, cfg.render[6]);
}

static float3 marchMandelbulberProfiled(float3 direction,
                                        float3 position,
                                        int iteration_count,
                                        constant FptRenderConfig &cfg,
                                        thread SdfProfileLocalStats &stats,
                                        thread uint &ray_steps) {
    float3 start = position;
    float distance = 0.0f;
    float threshold = mandelbulberMarchThreshold(position, cfg);
    float step = 0.0f;
    bool found = false;
    int maximum_iterations = sdfMarchIterationLimit(iteration_count, cfg);
    for (int iteration = 0; iteration < maximum_iterations; ++iteration) {
        ray_steps += 1u;
        threshold = mandelbulberMarchThreshold(position, cfg);
        distance = mandelbulberProfileDistance(position, cfg, stats);
        if (!isfinite(distance)) break;
        if (distance < threshold) {
            found = true;
            break;
        }
        step = sdfMarchStep(distance, threshold, cfg);
        float3 next_position = position + direction * step;
        if (all(next_position == position)) break;
        position = next_position;
        if (length(position - start) >= cfg.render[4]) break;
    }
    if (!found) return position;

    float search_limit = 1.0f - 0.001f * max(cfg.vset_values[129], 0.0f);
    step *= 0.5f;
    for (int refinement = 0; refinement < 30; ++refinement) {
        if (distance < threshold && distance > threshold * search_limit) break;
        stats.refinement_steps += 1u;
        stats.refinement_distance_evals += 1u;
        if (distance > threshold) {
            float3 next_position = position + direction * step;
            if (all(next_position == position)) break;
            position = next_position;
        } else if (distance < threshold * search_limit) {
            float3 next_position = position - direction * step;
            if (all(next_position == position)) break;
            position = next_position;
        }
        distance = mandelbulberProfileDistance(position, cfg, stats);
        step *= 0.5f;
    }
    return position;
}

static float3 marchProfiled(float3 direction,
                            float3 position,
                            int iteration_count,
                            float minimum_distance,
                            float lod_falloff,
                            constant FptRenderConfig &cfg,
                            thread SdfProfileLocalStats &stats,
                            thread uint &ray_steps) {
    if (cfg.sdf_id == SDF_MANDELBULBER) {
        return marchMandelbulberProfiled(direction, position, iteration_count,
                                         cfg, stats, ray_steps);
    }
    float3 result = marchCounted(direction, position, iteration_count,
                                 minimum_distance, lod_falloff, cfg, ray_steps);
    stats.distance_evals += ray_steps;
    return result;
}

static uint sunProfiledWithNormal(float3 position,
                                  float2 xy,
                                  float seed,
                                  float3 normal,
                                  constant FptRenderConfig &cfg,
                                  thread SdfProfileLocalStats &stats) {
    float3 light_direction = rotateCamera(
        float3(0.0f, 0.0f, 1.0f),
        float2(cfg.sun[1] * pi / 180.0f, cfg.sun[2] * pi / 180.0f));
    float h1 = hash13(float3(xy, seed * 5.0f + 1.0f));
    float h2 = hash13(float3(xy, seed * 3.0f + 5.0f));
    float2 divergence = float2(cos(h1 * 2.0f * pi), sin(h1 * 2.0f * pi)) *
                        sqrt(h2) * cfg.sun[4];
    light_direction = rotateCamera(light_direction, divergence);
    uint ray_steps = 0u;
    uint previous_phase = stats.phase;
    stats.phase = 2u;
    (void)marchProfiled(light_direction, position + normal * 0.001f,
                        int(cfg.render[1]), cfg.render[3], 0.0002f, cfg,
                        stats, ray_steps);
    stats.phase = previous_phase;
    stats.max_ray_steps = max(stats.max_ray_steps, ray_steps);
    return ray_steps;
}

static void sdfProfilePath(float2 xy,
                           uint sample_idx,
                           constant FptRenderConfig &cfg,
                           thread SdfProfileLocalStats &stats) {
    float frame = float(sample_idx);
    float3 cam_pos = cameraPos(cfg);
    float3 rp = cam_pos;
    float focal_length = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
    float aa_strength = 0.3f / max(float(cfg.width), float(cfg.height));
    float3 dr = normalize(float3(xy + randomPoint(aa_strength, xy, frame), focal_length));
    dr = rotateCamera(dr, cameraYawPitch(cfg), cfg.camera_roll);
    if (cfg.camera_dof > 0.0f) {
        float focus = cfg.focus_distance > 0.0f ? cfg.focus_distance : estimateFocusDistance(cfg);
        float3 fp = rp + dr * focus;
        float2 lens = randomPoint(cfg.camera_dof, xy, frame);
        rp += rotateCamera(float3(lens.x, lens.y, 0.0f), cameraYawPitch(cfg), cfg.camera_roll);
        dr = normalize(fp - rp);
    }

    int local_ni = int(cfg.render[1]);
    int bounce_limit = min(int(cfg.render[0]), 8);
    if (cfg.sdf_bounce_cap > 0u) bounce_limit = min(bounce_limit, int(cfg.sdf_bounce_cap));
    float far_dist = cfg.render[4] * 0.99f;
    float far_dist_sq = far_dist * far_dist;
    for (int i = 0; i < bounce_limit; i++) {
        uint local_primary = 0u;
        stats.phase = i == 0 ? 0u : 1u;
        rp = marchProfiled(dr, rp, local_ni, cfg.render[3], 0.0002f, cfg,
                           stats, local_primary);
        stats.max_ray_steps = max(stats.max_ray_steps, local_primary);
        if (i == 0) stats.primary_steps += local_primary;
        else stats.secondary_steps += local_primary;
        local_ni = int(cfg.render[1] / (cfg.render[5] * 2.0f + 1.0f));
        if (dot(rp - cam_pos, rp - cam_pos) > far_dist_sq) break;
        Material material = userSdf(rp, cfg).material;
        stats.material_evals += 1u;
        stats.bounces += 1u;
        stats.normal_evals += 1u;
        if (cfg.sdf_id == SDF_MANDELBULBER) {
            stats.normal_field_evals += cfg.render[7] > 0.0f
                ? 1331u : (cfg.sdf_normal_mode == 1u ? 4u : 6u);
        } else if (cfg.sdf_id == SDF_PROGRAM &&
                   (cfg.sdf_normal_mode == 0u || cfg.sdf_normal_mode == 2u)) {
            stats.normal_field_evals += 1u;
        } else {
            stats.normal_field_evals += cfg.sdf_normal_mode == 1u ? 4u : 6u;
        }
        float3 n = normalAt(rp, cfg);
        if (cfg.sun[0] == 1.0f) {
            stats.shadow_steps += sunProfiledWithNormal(rp, xy, frame, n, cfg,
                                                        stats);
        }
        float r1 = hash13(float3(xy, frame * 1.37f + float(i)));
        float r2 = hash13(float3(xy, frame * 7.91f + float(i)));
        if (r1 > material.translucency) {
            float3 metal = reflect(dr, n);
            float3 diffuse = randomVector(n, xy, frame * 13.37f + float(i));
            float3 specular = reflect(dr, n);
            float f0 = pow((material.ior - 1.0f) / (material.ior + 1.0f), 2.0f);
            float cosTheta = clamp(dot(n, -dr), 0.0f, 1.0f);
            float fresnel = f0 + (1.0f - f0) * pow5(1.0f - cosTheta);
            dr = normalize(mix(metal, diffuse, material.roughness));
            if (r2 < fresnel * material.specular) dr = specular;
        } else {
            float eta = 1.0f / material.ior;
            float f0 = pow((material.ior - 1.0f) / (material.ior + 1.0f), 2.0f);
            float cosTheta = clamp(dot(n, -dr), 0.0f, 1.0f);
            float fresnel = f0 + (1.0f - f0) * pow5(1.0f - cosTheta);
            float3 refracted = refract(dr, n, eta);
            float3 reflected = reflect(dr, n);
            dr = (dot(refracted, refracted) < 0.000001f || !isfinite(refracted.x) || r2 < fresnel) ? normalize(reflected) : normalize(refracted);
        }
        rp += n * 0.001f * sign(dot(dr, n));
    }
}

static float3 sdfBounceContributionDiagnostic(float2 xy, constant FptRenderConfig &cfg, constant FptDiagnosticConfig &diag) {
    uint target_bounce = diag._pad0;
    uint samples = min(max(cfg.samples, 1u), 8u);
    float3 total = float3(0.0f);
    for (uint sample_idx = 0u; sample_idx < samples; ++sample_idx) {
        float frame = float(sample_idx);
        float3 cam_pos = cameraPos(cfg);
        float3 rp = cam_pos;
        float focal_length = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
        float aa_strength = 0.3f / max(float(cfg.width), float(cfg.height));
        float3 dr = normalize(float3(xy + randomPoint(aa_strength, xy, frame), focal_length));
        dr = rotateCamera(dr, cameraYawPitch(cfg), cfg.camera_roll);
        if (cfg.camera_dof > 0.0f) {
            float focus = cfg.focus_distance > 0.0f ? cfg.focus_distance : estimateFocusDistance(cfg);
            float3 fp = rp + dr * focus;
            float2 lens = randomPoint(cfg.camera_dof, xy, frame);
            rp += rotateCamera(float3(lens.x, lens.y, 0.0f), cameraYawPitch(cfg), cfg.camera_roll);
            dr = normalize(fp - rp);
        }

        int local_ni = int(cfg.render[1]);
        int bounce_limit = min(int(cfg.render[0]), 8);
        if (cfg.sdf_bounce_cap > 0u) bounce_limit = min(bounce_limit, int(cfg.sdf_bounce_cap));
        float far_dist = cfg.render[4] * 0.99f;
        float far_dist_sq = far_dist * far_dist;
        float3 throughput = float3(1.0f);
        for (int i = 0; i < bounce_limit; i++) {
            rp = march(dr, rp, local_ni, cfg.render[3], 0.0002f, cfg);
            float travel_sq = dot(rp - cam_pos, rp - cam_pos);
            local_ni = int(cfg.render[1] / (cfg.render[5] * 2.0f + 1.0f));
            if (travel_sq > far_dist_sq) {
                if (uint(i) == target_bounce) total += environment(dr, cfg) * throughput;
                break;
            }
            Material material = userSdf(rp, cfg).material;
            float3 n = normalAt(rp, cfg);
            float3 contribution = float3(0.0f);
            if (material.emission > 0.001f) contribution += material.rgb * material.emission;
            if (cfg.sun[0] == 1.0f) contribution += sunContributionWithSurface(rp, xy, frame, material, n, cfg);
            if (uint(i) == target_bounce) total += contribution * throughput;

            float r1 = hash13(float3(xy, frame * 1.37f + float(i)));
            float r2 = hash13(float3(xy, frame * 7.91f + float(i)));
            if (r1 > material.translucency) {
                float3 metal = reflect(dr, n);
                float3 diffuse = randomVector(n, xy, frame * 13.37f + float(i));
                float3 specular = reflect(dr, n);
                float f0 = pow((material.ior - 1.0f) / (material.ior + 1.0f), 2.0f);
                float cosTheta = clamp(dot(n, -dr), 0.0f, 1.0f);
                float fresnel = f0 + (1.0f - f0) * pow5(1.0f - cosTheta);
                dr = normalize(mix(metal, diffuse, material.roughness));
                if (r2 >= fresnel * material.specular) throughput *= material.rgb;
                else dr = specular;
            } else {
                float f0 = pow((material.ior - 1.0f) / (material.ior + 1.0f), 2.0f);
                float cosTheta = clamp(dot(n, -dr), 0.0f, 1.0f);
                float fresnel = f0 + (1.0f - f0) * pow5(1.0f - cosTheta);
                float3 refracted = refract(dr, n, 1.0f / material.ior);
                float3 reflected = reflect(dr, n);
                if (dot(refracted, refracted) < 0.000001f || !isfinite(refracted.x) || r2 < fresnel) {
                    dr = normalize(reflected);
                } else {
                    dr = normalize(refracted);
                    throughput *= mix(float3(1.0f), material.rgb, 0.35f) * (1.0f - fresnel * 0.5f);
                }
            }
            rp += n * 0.001f * sign(dot(dr, n));
        }
    }
    return diagnosticEncodeHdr(total / float(samples), cfg);
}

static float3 sdfDiagnostic(float2 xy, constant FptRenderConfig &cfg, constant FptDiagnosticConfig &diag) {
    if (diag.mode == FPT_DIAGNOSTIC_SDF_BOUNCE_CONTRIBUTION) {
        return sdfBounceContributionDiagnostic(xy, cfg, diag);
    }
    if (diag.mode == FPT_DIAGNOSTIC_SDF_PRIMARY_STEPS ||
        diag.mode == FPT_DIAGNOSTIC_SDF_SHADOW_STEPS ||
        diag.mode == FPT_DIAGNOSTIC_SDF_NORMAL_EVALS ||
        diag.mode == FPT_DIAGNOSTIC_SDF_BOUNCES) {
        return sdfPathCostDiagnostic(xy, cfg, diag);
    }
    if (diag.mode == FPT_DIAGNOSTIC_PATH_FINAL) {
        float3 color = float3(0.0f);
        uint samples = min(max(cfg.samples, 16u), 64u);
        for (uint i = 0u; i < samples; ++i) {
            if (cfg.sdf_id == SDF_GLASS_BALL && cfg.glass_mode == 1u) {
                color += renderGlassAnalytic(xy, i, cfg);
            } else {
                color += renderPath(xy, i, cfg);
            }
        }
        return postProcess(color / float(samples), cfg);
    }
    float3 ro = cameraPos(cfg);
    float3 rp = ro;
    float3 rd;
    if (cfg.sdf_id == SDF_MANDELBULBER) {
        if (!mandelbulberProjectionVisible(xy, cfg)) return float3(0.0f);
        rd = mandelbulberCameraRay(xy, cfg);
    } else {
        float focal_length = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
        rd = rotateCamera(normalize(float3(xy, focal_length)),
                          cameraYawPitch(cfg), cfg.camera_roll);
    }
    int requested_steps = int(max(cfg.render[1], 32.0f));
    int steps = cfg.sdf_id == SDF_MANDELBULBER
        ? min(requested_steps, 10000)
        : min(requested_steps, 420);
    bool hit = false;
    float threshold = cfg.sdf_id == SDF_MANDELBULBER
        ? mandelbulberMarchThreshold(rp, cfg)
        : max(cfg.render[3], 0.0005f);
    if (cfg.sdf_id == SDF_MANDELBULBER) {
        MandelbulberMarchResult march_result =
            marchMandelbulber(rd, rp, steps, cfg);
        rp = march_result.position;
        hit = march_result.found;
    } else {
        for (int i = 0; i < steps; ++i) {
            float d = mapSdf(rp, cfg);
            if (!isfinite(d)) break;
            if (d < threshold && length(rp - ro) > 0.001f) {
                hit = true;
                break;
            }
            rp += rd * sdfMarchStep(d, threshold, cfg);
            if (length(rp - ro) > cfg.render[4]) break;
        }
    }
    if (diag.mode == FPT_DIAGNOSTIC_HIT_MASK) return hit ? float3(1.0f) : float3(0.0f);
    if (diag.mode == FPT_DIAGNOSTIC_DEPTH) return diagnosticEncodeDepth(length(rp - ro), hit, cfg, diag);
    if (!hit) return float3(0.0f);
    if (diag.mode == FPT_DIAGNOSTIC_DIFFUSE_NORMAL) {
        float3 normal = normalAt(rp, cfg);
        float3 camera_forward = rotateCamera(float3(0.0f, 0.0f, 1.0f), cameraYawPitch(cfg), cfg.camera_roll);
        float diffuse = max(dot(normal, -camera_forward), 0.0f);
        return float3(0.12f + 0.88f * diffuse);
    }
#if defined(FPT_MANDEL_GENERATED_MATERIAL)
    if (diag.mode == FPT_DIAGNOSTIC_MANDEL_COLOR_INDEX) {
        return float3(mandelbulberGeneratedColorCoordinate(rp, cfg));
    }
    if (diag.mode == FPT_DIAGNOSTIC_MANDEL_PALETTE_POSITION) {
        return float3(mandelbulberGeneratedPalettePosition(rp, cfg));
    }
#else
    if (diag.mode == FPT_DIAGNOSTIC_MANDEL_COLOR_INDEX ||
        diag.mode == FPT_DIAGNOSTIC_MANDEL_PALETTE_POSITION) {
        return float3(0.0f);
    }
#endif
    SDFResult result = userSdf(rp, cfg);
    if (diag.mode == FPT_DIAGNOSTIC_MATERIAL) return clamp(result.material.rgb, float3(0.0f), float3(1.0f));
    if (diag.mode == FPT_DIAGNOSTIC_PATH_DIRECT) return diagnosticEncodeHdr(cfg.sun[0] == 1.0f ? sunContribution(rp, xy, 0.0f, cfg) : float3(0.0f), cfg);
    if (diag.mode == FPT_DIAGNOSTIC_PATH_ENVIRONMENT) return diagnosticEncodeHdr(environment(rd, cfg), cfg);
    if (diag.mode == FPT_DIAGNOSTIC_PATH_THROUGHPUT) {
        float3 n = normalAt(rp, cfg);
        float3 next = normalize(mix(reflect(rd, n), randomVector(n, xy, 0.0f), result.material.roughness));
        float alignment = clamp(dot(next, n) * 0.5f + 0.5f, 0.0f, 1.0f);
        return clamp(result.material.rgb * alignment, float3(0.0f), float3(1.0f));
    }
    return diagnosticEncodeNormal(normalAt(rp, cfg));
}

kernel void sdf_diagnostic_kernel(device uchar4 *out [[buffer(0)]],
                                  constant FptRenderConfig &cfg [[buffer(1)]],
                                  constant FptDiagnosticConfig &diag [[buffer(2)]],
                                  uint2 gid [[thread_position_in_grid]]) {
    uint2 pixel = gid + diag.dispatch_origin;
    if (pixel.x >= cfg.width || pixel.y >= cfg.height) return;
    float2 uv = sdfScreenUv(pixel, cfg);
    float3 color = sdfDiagnostic(uv, cfg, diag);
    uint idx = (cfg.height - 1u - pixel.y) * cfg.width + pixel.x;
    out[idx] = uchar4(uchar(clamp(color.r, 0.0f, 1.0f) * 255.0f),
                      uchar(clamp(color.g, 0.0f, 1.0f) * 255.0f),
                      uchar(clamp(color.b, 0.0f, 1.0f) * 255.0f),
                      255);
}

static float3 voxelDiagnostic(float2 xy,
                              constant FptRenderConfig &cfg,
                              constant FptDiagnosticConfig &diag,
                              device const VoxelCell *cells,
                              device const uint *page_table) {
    if (diag.mode == FPT_DIAGNOSTIC_PATH_FINAL) {
        uint samples = min(max(cfg.samples, 1u), 64u);
        float3 color = float3(0.0f);
        for (uint sample = 0u; sample < samples; ++sample) {
            color += renderVoxelPath(xy, sample, cfg, cells, page_table);
        }
        return postProcess(color / float(samples), cfg);
    }
    float focal_length = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
    float3 direction = rotateCamera(normalize(float3(xy, focal_length)), cameraYawPitch(cfg), cfg.camera_roll);
    VoxelHit hit = traceVoxel(cameraPos(cfg), direction, cfg, cells, page_table);
    if (diag.mode == FPT_DIAGNOSTIC_HIT_MASK) {
        return hit.hit ? float3(1.0f) : float3(0.0f);
    }
    if (diag.mode == FPT_DIAGNOSTIC_DEPTH) {
        return diagnosticEncodeDepth(hit.surface_distance, hit.hit, cfg, diag);
    }
    if (diag.mode == FPT_DIAGNOSTIC_SDF_PRIMARY_STEPS) {
        return diagnosticEncodeScalar(float(hit.steps), float(max(cfg.voxel_resolution, 1u) * 3u));
    }
    if (!hit.hit) return float3(0.0f);
    float3 normal = voxelShadingNormal(hit, cfg);
    if (dot(normal, direction) > 0.0f) normal = -normal;
    if (diag.mode == FPT_DIAGNOSTIC_DIFFUSE_NORMAL) {
        float3 camera_forward = rotateCamera(float3(0.0f, 0.0f, 1.0f), cameraYawPitch(cfg), cfg.camera_roll);
        float diffuse = max(dot(normal, -camera_forward), 0.0f);
        return float3(0.12f + 0.88f * diffuse);
    }
    float3 offset_normal = cfg.voxel_normal_mode == 2u ? hit.normal : normal;
    if (dot(offset_normal, direction) > 0.0f) offset_normal = -offset_normal;
    Material material = voxelMaterialAtHit(hit, cfg);
    if (diag.mode == FPT_DIAGNOSTIC_MATERIAL) return clamp(material.rgb, 0.0f, 1.0f);
    if (diag.mode == FPT_DIAGNOSTIC_PATH_DIRECT) {
        return diagnosticEncodeHdr(cfg.sun[0] == 1.0f
            ? voxelSunContributionWithSurface(hit.position, xy, 0.0f, material,
                                              normal, offset_normal, cfg, cells, page_table)
            : float3(0.0f), cfg);
    }
    if (diag.mode == FPT_DIAGNOSTIC_PATH_ENVIRONMENT) return diagnosticEncodeHdr(environment(direction, cfg), cfg);
    if (diag.mode == FPT_DIAGNOSTIC_PATH_THROUGHPUT) {
        float3 next = normalize(mix(reflect(direction, normal),
                                    randomVector(normal, xy, 0.0f),
                                    material.roughness));
        float alignment = clamp(dot(next, normal) * 0.5f + 0.5f, 0.0f, 1.0f);
        return clamp(material.rgb * alignment, 0.0f, 1.0f);
    }
    return diagnosticEncodeNormal(normal);
}

kernel void voxel_diagnostic_kernel(device uchar4 *out [[buffer(0)]],
                                    constant FptRenderConfig &cfg [[buffer(1)]],
                                    constant FptDiagnosticConfig &diag [[buffer(2)]],
                                    device const VoxelCell *cells [[buffer(3)]],
                                    device const uint *page_table [[buffer(4)]],
                                    uint2 gid [[thread_position_in_grid]]) {
    uint2 pixel = gid + diag.dispatch_origin;
    if (pixel.x >= cfg.width || pixel.y >= cfg.height) return;
    float2 suv = (float2(pixel) + 0.5f) / float2(float(cfg.width), float(cfg.height));
    float2 uv = suv - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
    float3 color = voxelDiagnostic(uv, cfg, diag, cells, page_table);
    uint index = (cfg.height - 1u - pixel.y) * cfg.width + pixel.x;
    out[index] = uchar4(uchar(clamp(color.r, 0.0f, 1.0f) * 255.0f),
                        uchar(clamp(color.g, 0.0f, 1.0f) * 255.0f),
                        uchar(clamp(color.b, 0.0f, 1.0f) * 255.0f),
                        255);
}
static uint outputIndex(uint2 gid, constant FptRenderConfig &cfg) {
    return (cfg.height - 1u - gid.y) * cfg.width + gid.x;
}

static float3 highlightAt(device const float4 *accum, uint2 gid, constant FptRenderConfig &cfg) {
    float intensity = cfg.post[6];
    if (intensity <= 0.00001f) return float3(0.0f);
    uint w = cfg.width;
    uint h = cfg.height;
    uint x0 = gid.x > 0u ? gid.x - 1u : gid.x;
    uint x1 = min(gid.x + 1u, w - 1u);
    uint y0 = gid.y > 0u ? gid.y - 1u : gid.y;
    uint y1 = min(gid.y + 1u, h - 1u);
    float3 bright = max(accum[gid.y * w + gid.x].xyz - float3(0.5f), float3(0.0f));
    bright += accum[gid.y * w + x1].xyz * 0.25f;
    bright += accum[gid.y * w + x0].xyz * 0.25f;
    bright += accum[y1 * w + gid.x].xyz * 0.25f;
    bright += accum[y0 * w + gid.x].xyz * 0.25f;
    return bright * intensity;
}

static float3 sampleAccumBilinear(device const float4 *accum, float2 uv, constant FptRenderConfig &cfg) {
    float2 pixel = clamp(uv, float2(0.0f), float2(1.0f)) * float2(float(cfg.width), float(cfg.height)) - 0.5f;
    int2 base = int2(floor(pixel));
    float2 fraction = fract(pixel);
    uint2 p00 = uint2(clamp(base, int2(0), int2(int(cfg.width - 1u), int(cfg.height - 1u))));
    uint2 p10 = uint2(min(p00 + uint2(1u, 0u), uint2(cfg.width - 1u, cfg.height - 1u)));
    uint2 p01 = uint2(min(p00 + uint2(0u, 1u), uint2(cfg.width - 1u, cfg.height - 1u)));
    uint2 p11 = uint2(min(p00 + uint2(1u, 1u), uint2(cfg.width - 1u, cfg.height - 1u)));
    float3 c00 = accum[p00.y * cfg.width + p00.x].xyz;
    float3 c10 = accum[p10.y * cfg.width + p10.x].xyz;
    float3 c01 = accum[p01.y * cfg.width + p01.x].xyz;
    float3 c11 = accum[p11.y * cfg.width + p11.x].xyz;
    return mix(mix(c00, c10, fraction.x), mix(c01, c11, fraction.x), fraction.y);
}

static float3 chromaticAt(device const float4 *accum, uint2 gid, constant FptRenderConfig &cfg) {
    float2 uv = (float2(gid) + 0.5f) / float2(float(cfg.width), float(cfg.height));
    float2 direction = uv - 0.5f;
    float2 offset = direction * cfg.post[5] * length(direction) / 8.0f;
    float3 center = sampleAccumBilinear(accum, uv, cfg);
    if (abs(cfg.post[5]) <= 0.00001f) return center;
    float3 green = sampleAccumBilinear(accum, uv - offset, cfg);
    float3 blue = sampleAccumBilinear(accum, uv - 2.0f * offset, cfg);
    return float3(center.r, green.g, blue.b);
}

kernel void render_kernel(device uchar4 *out [[buffer(0)]],
                          constant FptRenderConfig &cfg [[buffer(1)]],
                          uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height) return;
    float2 uv = sdfScreenUv(gid, cfg);

    float3 color = float3(0.0f);
    if (cfg.preview != 0u) {
        color = viewport(uv, cfg);
    } else {
        uint samples = max(cfg.samples, 16u);
        samples = min(samples, 512u);
        for (uint i = 0; i < samples; ++i) {
            float3 sample_color;
            if (cfg.sdf_id == SDF_GLASS_BALL && cfg.glass_mode == 1u) {
                sample_color = renderGlassAnalytic(uv, i, cfg);
            } else {
                sample_color = renderPath(uv, i, cfg);
            }
            color = mix(color, sample_color, 1.0f / float(i + 1u));
        }
    }
    color = postProcess(color, cfg);
    uint idx = outputIndex(gid, cfg);
    out[idx] = uchar4(uchar(clamp(color.r, 0.0f, 1.0f) * 255.0f),
                      uchar(clamp(color.g, 0.0f, 1.0f) * 255.0f),
                      uchar(clamp(color.b, 0.0f, 1.0f) * 255.0f),
                      255);
}

kernel void preview_kernel(device uchar4 *out [[buffer(0)]],
                           constant FptRenderConfig &cfg [[buffer(1)]],
                           uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height) return;
    float2 uv = sdfScreenUv(gid, cfg);
    float3 color = postProcess(viewport(uv, cfg), cfg);
    uint idx = outputIndex(gid, cfg);
    out[idx] = uchar4(uchar(clamp(color.r, 0.0f, 1.0f) * 255.0f),
                      uchar(clamp(color.g, 0.0f, 1.0f) * 255.0f),
                      uchar(clamp(color.b, 0.0f, 1.0f) * 255.0f),
                      255);
}

static bool mandelInteractivePreviewPixel(
    uint2 local_pixel,
    constant FptRenderConfig &cfg,
    thread uint2 &pixel) {
    pixel = local_pixel;
#if defined(FPT_MANDEL_INTERACTIVE_REFINEMENT)
    float refinement_state = cfg.vset_values[131];
    if (refinement_state >= 1.0f) {
        uint tile_mask = min(uint(refinement_state), 0xffffu);
        bool single_tile = (tile_mask & (tile_mask - 1u)) == 0u;
        if (single_tile) {
            uint tile = 0u;
            while ((tile_mask & (1u << tile)) == 0u && tile < 15u) ++tile;
            uint2 tile_coordinate = uint2(tile & 3u, tile >> 2u);
            uint2 origin = uint2(
                (tile_coordinate.x * cfg.width + 3u) / 4u,
                (tile_coordinate.y * cfg.height + 3u) / 4u);
            uint2 end = uint2(
                ((tile_coordinate.x + 1u) * cfg.width + 3u) / 4u,
                ((tile_coordinate.y + 1u) * cfg.height + 3u) / 4u);
            if (any(local_pixel >= end - origin)) return false;
            pixel = local_pixel + origin;
            return true;
        }
        uint2 pixel_tile = uint2(
            min((local_pixel.x * 4u) / max(cfg.width, 1u), 3u),
            min((local_pixel.y * 4u) / max(cfg.height, 1u), 3u));
        uint tile = pixel_tile.x + pixel_tile.y * 4u;
        if ((tile_mask & (1u << tile)) == 0u) return false;
    }
#endif
    return true;
}

kernel void preview_linear_kernel(device float4 *accum [[buffer(0)]],
                                  constant FptRenderConfig &cfg [[buffer(1)]],
                                  uint2 gid [[thread_position_in_grid]]) {
    uint2 pixel;
#if defined(FPT_MANDEL_INTERACTIVE_REFINEMENT)
    int spatial_mode = int(round(cfg.vset_values[132]));
    if (spatial_mode != 0) {
        uint spatial_code = uint(abs(spatial_mode));
        uint spatial_stride = spatial_mode < 0
            ? clamp(spatial_code, 2u, 16u)
            : clamp(spatial_code >> 16u, 2u, 16u);
        uint pass = spatial_mode > 0 ? max(spatial_code & 65535u, 1u) - 1u : 0u;
        uint2 sample_offset = spatial_mode > 0
            ? uint2(pass % spatial_stride, pass / spatial_stride)
            : uint2(0u);
        pixel = gid * spatial_stride + sample_offset;
        if (pixel.x >= cfg.width || pixel.y >= cfg.height) return;
        float2 uv = sdfScreenUv(pixel, cfg);
        float4 sample = float4(viewport(uv, cfg), 1.0f);
        if (spatial_mode < 0) {
            // The moving preview traces one sample per 2x2 block and fills the
            // block. Four exact interlace passes subsequently replace every
            // copied value without changing the persistent buffer layout.
            for (uint y = 0u; y < spatial_stride; ++y) {
                for (uint x = 0u; x < spatial_stride; ++x) {
                    uint2 destination = gid * spatial_stride + uint2(x, y);
                    if (destination.x < cfg.width && destination.y < cfg.height) {
                        accum[destination.y * cfg.width + destination.x] = sample;
                    }
                }
            }
        } else {
            accum[pixel.y * cfg.width + pixel.x] = sample;
        }
        return;
    }
#endif
    if (!mandelInteractivePreviewPixel(gid, cfg, pixel) ||
        pixel.x >= cfg.width || pixel.y >= cfg.height) return;
    float2 uv = sdfScreenUv(pixel, cfg);
    uint index = pixel.y * cfg.width + pixel.x;
    accum[index] = float4(viewport(uv, cfg), 1.0f);
}

kernel void estimate_voxel_focus_distance_kernel(device float *focus_distance [[buffer(0)]],
                                                  constant FptRenderConfig &cfg [[buffer(1)]],
                                                  device const VoxelCell *cells [[buffer(2)]],
                                                  device const uint *page_table [[buffer(3)]],
                                                  uint gid [[thread_position_in_grid]]) {
    if (gid == 0u) focus_distance[0] = estimateVoxelFocusDistance(cfg, cells, page_table);
}

kernel void voxel_preview_linear_kernel(device float4 *accum [[buffer(0)]],
                                        constant FptRenderConfig &cfg [[buffer(1)]],
                                        device const VoxelCell *cells [[buffer(3)]],
                                        device const uint *page_table [[buffer(4)]],
                                        uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height) return;
    float2 suv = (float2(gid) + 0.5f) / float2(float(cfg.width), float(cfg.height));
    float2 uv = suv - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
    uint index = gid.y * cfg.width + gid.x;
    accum[index] = float4(voxelViewport(uv, cfg, cells, page_table), 1.0f);
}
kernel void sdf_profile_kernel(device FptSdfProfileCounts *counts [[buffer(0)]],
                               constant FptRenderConfig &cfg [[buffer(1)]],
                               constant FptSdfProfileConfig &profile [[buffer(2)]],
                               uint2 gid [[thread_position_in_grid]]) {
    uint stride = max(profile.stride, 1u);
    if (gid.x >= cfg.width || gid.y >= cfg.height) return;
    if ((gid.x % stride) != 0u || (gid.y % stride) != 0u) return;
    if (cfg.sdf_id == SDF_GLASS_BALL && cfg.glass_mode == 1u) {
        atomic_fetch_add_explicit(&counts->pixels, 1u, memory_order_relaxed);
        return;
    }
    float2 uv = sdfScreenUv(gid, cfg);
    SdfProfileLocalStats stats = {};
    sdfProfilePath(uv, profile.frame_index, cfg, stats);
    uint pixel_steps = stats.primary_steps + stats.secondary_steps +
                       stats.shadow_steps;
    atomic_fetch_add_explicit(&counts->primary_steps, stats.primary_steps, memory_order_relaxed);
    atomic_fetch_add_explicit(&counts->secondary_steps, stats.secondary_steps, memory_order_relaxed);
    atomic_fetch_add_explicit(&counts->shadow_steps, stats.shadow_steps, memory_order_relaxed);
    atomic_fetch_add_explicit(&counts->normal_evals, stats.normal_evals, memory_order_relaxed);
    atomic_fetch_add_explicit(&counts->bounces, stats.bounces, memory_order_relaxed);
    atomic_fetch_add_explicit(&counts->distance_evals, stats.distance_evals,
                              memory_order_relaxed);
    atomic_fetch_add_explicit(&counts->march_orbit_iterations,
                              stats.march_orbit_iterations,
                              memory_order_relaxed);
    atomic_fetch_add_explicit(&counts->refinement_steps,
                              stats.refinement_steps, memory_order_relaxed);
    atomic_fetch_add_explicit(&counts->normal_field_evals,
                              stats.normal_field_evals,
                              memory_order_relaxed);
    atomic_fetch_add_explicit(&counts->material_evals, stats.material_evals,
                              memory_order_relaxed);
    atomic_fetch_max_explicit(&counts->max_ray_steps, stats.max_ray_steps,
                              memory_order_relaxed);
    atomic_fetch_max_explicit(&counts->max_pixel_steps, pixel_steps,
                              memory_order_relaxed);
    for (uint phase = 0u; phase < 4u; ++phase) {
        atomic_fetch_add_explicit(&counts->distance_evals_by_phase[phase],
                                  stats.distance_evals_by_phase[phase],
                                  memory_order_relaxed);
        atomic_fetch_add_explicit(&counts->orbit_iterations_by_phase[phase],
                                  stats.orbit_iterations_by_phase[phase],
                                  memory_order_relaxed);
    }
    for (uint slot = 0u; slot < 9u; ++slot) {
        atomic_fetch_add_explicit(&counts->formula_slot_iterations[slot],
                                  stats.formula_slot_iterations[slot],
                                  memory_order_relaxed);
    }
    atomic_fetch_add_explicit(&counts->refinement_distance_evals,
                              stats.refinement_distance_evals,
                              memory_order_relaxed);
    atomic_fetch_add_explicit(&counts->pixels, 1u, memory_order_relaxed);
}

kernel void accumulate_kernel(device float4 *accum [[buffer(0)]],
                              constant FptRenderConfig &cfg [[buffer(1)]],
                              constant uint &frame_index [[buffer(2)]],
                              uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height) return;
    float2 uv = sdfScreenUv(gid, cfg);
    float3 sample_color = (cfg.sdf_id == SDF_GLASS_BALL && cfg.glass_mode == 1u)
        ? renderGlassAnalytic(uv, frame_index, cfg)
        : renderPath(uv, frame_index, cfg);
    uint idx = gid.y * cfg.width + gid.x;
    float3 previous = frame_index == 0u ? float3(0.0f) : accum[idx].xyz;
    float a = 1.0f / float(frame_index + 1u);
    accum[idx] = float4(mix(previous, sample_color, a), 1.0f);
}

kernel void accumulate_all_kernel(device float4 *accum [[buffer(0)]],
                                  constant FptRenderConfig &cfg [[buffer(1)]],
                                  uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height) return;
    float2 uv = sdfScreenUv(gid, cfg);
    uint samples = max(cfg.samples, 1u);
    samples = min(samples, 512u);
    float3 color = float3(0.0f);
    for (uint i = 0u; i < samples; ++i) {
        float3 sample_color = (cfg.sdf_id == SDF_GLASS_BALL && cfg.glass_mode == 1u)
            ? renderGlassAnalytic(uv, i, cfg)
            : renderPath(uv, i, cfg);
        color = mix(color, sample_color, 1.0f / float(i + 1u));
    }
    uint idx = gid.y * cfg.width + gid.x;
    accum[idx] = float4(color, 1.0f);
}

kernel void accumulate_all_tile_kernel(
    device float4 *accum [[buffer(0)]],
    constant FptRenderConfig &cfg [[buffer(1)]],
    constant FptAccumulationTile &tile [[buffer(2)]],
    uint2 local_gid [[thread_position_in_grid]]) {
    uint2 gid = local_gid + tile.dispatch_origin;
    if (gid.x >= cfg.width || gid.y >= cfg.height) return;
    float2 uv = sdfScreenUv(gid, cfg);
    uint samples = min(max(cfg.samples, 1u), 512u);
    float3 color = float3(0.0f);
    for (uint sample = 0u; sample < samples; ++sample) {
        float3 sample_color = renderPath(uv, sample, cfg);
        color = mix(color, sample_color, 1.0f / float(sample + 1u));
    }
    uint index = gid.y * cfg.width + gid.x;
    accum[index] = float4(color, 1.0f);
}

kernel void accumulate_chunk_kernel(device float4 *accum [[buffer(0)]],
                                    constant FptRenderConfig &cfg [[buffer(1)]],
                                    constant FptAccumulationChunk &chunk [[buffer(2)]],
                                    uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height || chunk.sample_count == 0u) return;
    float2 uv = sdfScreenUv(gid, cfg);
    uint index = gid.y * cfg.width + gid.x;
    float3 color = chunk.start_sample == 0u ? float3(0.0f) : accum[index].xyz;
    uint end_sample = min(chunk.start_sample + chunk.sample_count, min(max(cfg.samples, 1u), 512u));
    for (uint sample = chunk.start_sample; sample < end_sample; ++sample) {
        float3 sample_color = (cfg.sdf_id == SDF_GLASS_BALL && cfg.glass_mode == 1u)
            ? renderGlassAnalytic(uv, sample, cfg)
            : renderPath(uv, sample, cfg);
        color = mix(color, sample_color, 1.0f / float(sample + 1u));
    }
    accum[index] = float4(color, 1.0f);
}

kernel void regional_accumulate_kernel(
    device float4 *accum [[buffer(0)]],
    constant FptRenderConfig &cfg [[buffer(1)]],
    constant uint &frame_index [[buffer(2)]],
    device const ushort *program_ids [[buffer(3)]],
    device const RegionalProgramHeader *program_headers [[buffer(4)]],
    device const FptSdfInstruction *instruction_pool [[buffer(5)]],
    device const ushort *primitive_pool [[buffer(6)]],
    uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height) return;
    float2 normalized = (float2(gid) + 0.5f) /
                        float2(float(cfg.width), float(cfg.height));
    float2 uv = normalized - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
    float3 sample_color = renderRegionalProgramPath(
        uv, frame_index, cfg, program_ids, program_headers, instruction_pool,
        primitive_pool);
    uint index = gid.y * cfg.width + gid.x;
    float3 previous = frame_index == 0u ? float3(0.0f) : accum[index].xyz;
    accum[index] = float4(mix(previous, sample_color,
                              1.0f / float(frame_index + 1u)), 1.0f);
}

kernel void regional_accumulate_all_kernel(
    device float4 *accum [[buffer(0)]],
    constant FptRenderConfig &cfg [[buffer(1)]],
    device const ushort *program_ids [[buffer(3)]],
    device const RegionalProgramHeader *program_headers [[buffer(4)]],
    device const FptSdfInstruction *instruction_pool [[buffer(5)]],
    device const ushort *primitive_pool [[buffer(6)]],
    uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height) return;
    float2 normalized = (float2(gid) + 0.5f) /
                        float2(float(cfg.width), float(cfg.height));
    float2 uv = normalized - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
    uint samples = min(max(cfg.samples, 1u), 512u);
    float3 color = float3(0.0f);
    for (uint sample = 0u; sample < samples; ++sample) {
        color = mix(color, renderRegionalProgramPath(
                               uv, sample, cfg, program_ids, program_headers,
                               instruction_pool, primitive_pool),
                    1.0f / float(sample + 1u));
    }
    accum[gid.y * cfg.width + gid.x] = float4(color, 1.0f);
}

kernel void regional_accumulate_chunk_kernel(
    device float4 *accum [[buffer(0)]],
    constant FptRenderConfig &cfg [[buffer(1)]],
    constant FptAccumulationChunk &chunk [[buffer(2)]],
    device const ushort *program_ids [[buffer(3)]],
    device const RegionalProgramHeader *program_headers [[buffer(4)]],
    device const FptSdfInstruction *instruction_pool [[buffer(5)]],
    device const ushort *primitive_pool [[buffer(6)]],
    uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height || chunk.sample_count == 0u) {
        return;
    }
    float2 normalized = (float2(gid) + 0.5f) /
                        float2(float(cfg.width), float(cfg.height));
    float2 uv = normalized - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
    uint index = gid.y * cfg.width + gid.x;
    float3 color = chunk.start_sample == 0u ? float3(0.0f) : accum[index].xyz;
    uint end_sample = min(chunk.start_sample + chunk.sample_count,
                          min(max(cfg.samples, 1u), 512u));
    for (uint sample = chunk.start_sample; sample < end_sample; ++sample) {
        color = mix(color, renderRegionalProgramPath(
                               uv, sample, cfg, program_ids, program_headers,
                               instruction_pool, primitive_pool),
                    1.0f / float(sample + 1u));
    }
    accum[index] = float4(color, 1.0f);
}

kernel void regional_program_profile_kernel(
    device RegionalProgramLocalStats *samples [[buffer(0)]],
    constant FptRenderConfig &cfg [[buffer(1)]],
    device const ushort *program_ids [[buffer(2)]],
    device const RegionalProgramHeader *program_headers [[buffer(3)]],
    device const FptSdfInstruction *instruction_pool [[buffer(4)]],
    device const ushort *primitive_pool [[buffer(5)]],
    uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height) return;
    uint index = gid.y * cfg.width + gid.x;
    samples[index] = emptyRegionalProgramLocalStats();
    uint stride = max(cfg.bound_grid_profile_stride, 1u);
    if ((gid.x % stride) != 0u || (gid.y % stride) != 0u) return;
    float2 normalized = (float2(gid) + 0.5f) /
                        float2(float(cfg.width), float(cfg.height));
    float2 uv = normalized - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
    samples[index] = profileRegionalProgramPath(
        uv, cfg, program_ids, program_headers, instruction_pool,
        primitive_pool);
}

kernel void bound_grid_accumulate_kernel(
    device float4 *accum [[buffer(0)]],
    constant FptRenderConfig &cfg [[buffer(1)]],
    constant uint &frame_index [[buffer(2)]],
    texture3d<float, access::sample> bound_grid [[texture(0)]],
    texture3d<float, access::sample> derivative_lower_grid [[texture(1)]],
    texture3d<float, access::sample> derivative_upper_grid [[texture(2)]],
    uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height) return;
    float2 suv = (float2(gid) + 0.5f) / float2(float(cfg.width), float(cfg.height));
    float2 uv = suv - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
    float3 sample_color = renderBoundGridPath(uv, frame_index, cfg, bound_grid,
                                              derivative_lower_grid,
                                              derivative_upper_grid);
    uint index = gid.y * cfg.width + gid.x;
    float3 previous = frame_index == 0u ? float3(0.0f) : accum[index].xyz;
    accum[index] = float4(mix(previous, sample_color,
                              1.0f / float(frame_index + 1u)), 1.0f);
}

kernel void bound_grid_accumulate_all_kernel(
    device float4 *accum [[buffer(0)]],
    constant FptRenderConfig &cfg [[buffer(1)]],
    texture3d<float, access::sample> bound_grid [[texture(0)]],
    texture3d<float, access::sample> derivative_lower_grid [[texture(1)]],
    texture3d<float, access::sample> derivative_upper_grid [[texture(2)]],
    uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height) return;
    float2 suv = (float2(gid) + 0.5f) / float2(float(cfg.width), float(cfg.height));
    float2 uv = suv - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
    uint samples = min(max(cfg.samples, 1u), 512u);
    float3 color = float3(0.0f);
    for (uint sample = 0u; sample < samples; ++sample) {
        color = mix(color, renderBoundGridPath(uv, sample, cfg, bound_grid,
                                              derivative_lower_grid,
                                              derivative_upper_grid),
                    1.0f / float(sample + 1u));
    }
    accum[gid.y * cfg.width + gid.x] = float4(color, 1.0f);
}

kernel void bound_grid_accumulate_chunk_kernel(
    device float4 *accum [[buffer(0)]],
    constant FptRenderConfig &cfg [[buffer(1)]],
    constant FptAccumulationChunk &chunk [[buffer(2)]],
    texture3d<float, access::sample> bound_grid [[texture(0)]],
    texture3d<float, access::sample> derivative_lower_grid [[texture(1)]],
    texture3d<float, access::sample> derivative_upper_grid [[texture(2)]],
    uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height || chunk.sample_count == 0u) return;
    float2 suv = (float2(gid) + 0.5f) / float2(float(cfg.width), float(cfg.height));
    float2 uv = suv - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
    uint index = gid.y * cfg.width + gid.x;
    float3 color = chunk.start_sample == 0u ? float3(0.0f) : accum[index].xyz;
    uint end_sample = min(chunk.start_sample + chunk.sample_count,
                          min(max(cfg.samples, 1u), 512u));
    for (uint sample = chunk.start_sample; sample < end_sample; ++sample) {
        color = mix(color, renderBoundGridPath(uv, sample, cfg, bound_grid,
                                              derivative_lower_grid,
                                              derivative_upper_grid),
                    1.0f / float(sample + 1u));
    }
    accum[index] = float4(color, 1.0f);
}

kernel void bound_grid_profile_kernel(
    device BoundGridLocalStats *samples [[buffer(0)]],
    constant FptRenderConfig &cfg [[buffer(1)]],
    texture3d<float, access::sample> bound_grid [[texture(0)]],
    texture3d<float, access::sample> derivative_lower_grid [[texture(1)]],
    texture3d<float, access::sample> derivative_upper_grid [[texture(2)]],
    uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height) return;
    uint index = gid.y * cfg.width + gid.x;
    samples[index] = emptyBoundGridLocalStats();
    uint stride = max(cfg.bound_grid_profile_stride, 1u);
    if ((gid.x % stride) != 0u || (gid.y % stride) != 0u) return;
    float2 suv = (float2(gid) + 0.5f) / float2(float(cfg.width), float(cfg.height));
    float2 uv = suv - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
    samples[index] = profileBoundGridPath(uv, cfg, bound_grid,
                                          derivative_lower_grid,
                                          derivative_upper_grid);
}

kernel void voxel_accumulate_kernel(device float4 *accum [[buffer(0)]],
                                    constant FptRenderConfig &cfg [[buffer(1)]],
                                    constant uint &frame_index [[buffer(2)]],
                                    device const VoxelCell *cells [[buffer(3)]],
                                    device const uint *page_table [[buffer(4)]],
                                    uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height) return;
    float2 suv = (float2(gid) + 0.5f) / float2(float(cfg.width), float(cfg.height));
    float2 uv = suv - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
    float3 sample_color = renderVoxelPath(uv, frame_index, cfg, cells, page_table);
    uint index = gid.y * cfg.width + gid.x;
    float3 previous = frame_index == 0u ? float3(0.0f) : accum[index].xyz;
    float weight = 1.0f / float(frame_index + 1u);
    accum[index] = float4(mix(previous, sample_color, weight), 1.0f);
}
kernel void voxel_accumulate_all_kernel(device float4 *accum [[buffer(0)]],
                                        constant FptRenderConfig &cfg [[buffer(1)]],
                                        device const VoxelCell *cells [[buffer(3)]],
                                        device const uint *page_table [[buffer(4)]],
                                        uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height) return;
    float2 suv = (float2(gid) + 0.5f) / float2(float(cfg.width), float(cfg.height));
    float2 uv = suv - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
    uint samples = min(max(cfg.samples, 1u), 512u);
    float3 color = float3(0.0f);
    for (uint sample = 0u; sample < samples; ++sample) {
        color = mix(color, renderVoxelPath(uv, sample, cfg, cells, page_table), 1.0f / float(sample + 1u));
    }
    uint index = gid.y * cfg.width + gid.x;
    accum[index] = float4(color, 1.0f);
}
kernel void voxel_accumulate_chunk_kernel(device float4 *accum [[buffer(0)]],
                                          constant FptRenderConfig &cfg [[buffer(1)]],
                                          constant FptAccumulationChunk &chunk [[buffer(2)]],
                                          device const VoxelCell *cells [[buffer(3)]],
                                          device const uint *page_table [[buffer(4)]],
                                          uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height || chunk.sample_count == 0u) return;
    float2 suv = (float2(gid) + 0.5f) / float2(float(cfg.width), float(cfg.height));
    float2 uv = suv - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
    uint index = gid.y * cfg.width + gid.x;
    float3 color = chunk.start_sample == 0u ? float3(0.0f) : accum[index].xyz;
    uint end_sample = min(chunk.start_sample + chunk.sample_count, min(max(cfg.samples, 1u), 512u));
    for (uint sample = chunk.start_sample; sample < end_sample; ++sample) {
        color = mix(color, renderVoxelPath(uv, sample, cfg, cells, page_table), 1.0f / float(sample + 1u));
    }
    accum[index] = float4(color, 1.0f);
}
kernel void present_kernel(device uchar4 *out [[buffer(0)]],
                           device const float4 *accum [[buffer(1)]],
                           constant FptRenderConfig &cfg [[buffer(2)]],
                           uint2 gid [[thread_position_in_grid]]) {
    uint2 pixel;
    if (!mandelInteractivePreviewPixel(gid, cfg, pixel) ||
        pixel.x >= cfg.width || pixel.y >= cfg.height) return;
    uint out_idx = outputIndex(pixel, cfg);
    float3 color = postProcess(chromaticAt(accum, pixel, cfg) + highlightAt(accum, pixel, cfg), cfg);
    out[out_idx] = uchar4(uchar(clamp(color.r, 0.0f, 1.0f) * 255.0f),
                      uchar(clamp(color.g, 0.0f, 1.0f) * 255.0f),
                      uchar(clamp(color.b, 0.0f, 1.0f) * 255.0f),
                      255);
}
