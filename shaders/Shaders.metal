#include <metal_stdlib>
using namespace metal;

constant float pi = 3.14159265359f;
constant float inf = 1.0e20f;

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
};

constant uint FPT_SDF_PROGRAM_MAX_OPS = 64u;
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
};

enum {
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
};

struct FptDiagnosticConfig {
    uint mode;
    uint _pad0;
    float max_distance;
    float normal_mix;
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

struct FptSdfProfileCounts {
    atomic_uint primary_steps;
    atomic_uint shadow_steps;
    atomic_uint normal_evals;
    atomic_uint bounces;
    atomic_uint pixels;
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

static DeResult deProgram(float3 source, constant FptRenderConfig &cfg) {
    float3 p = source;
    float distance_scale = 1.0f;
    float distance = inf;
    float orbit = 0.0f;
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

static ProgramSurface programSurface(float3 source, constant FptRenderConfig &cfg) {
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
    if (cfg.sdf_id == SDF_PROGRAM && cfg.sdf_program_count > 0u) {
        DeResult program = deProgram(p, cfg);
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
        default: de.d = 1000.0f; de.orbit = 0.0f; break;
    }
    r.distance = de.d;
    r.material = fractalMaterial(de.orbit, cfg, menger);
    return r;
}

static float distanceSdf(float3 p, constant FptRenderConfig &cfg) {
    if (cfg.sdf_id == SDF_PROGRAM && cfg.sdf_program_count > 0u) return deProgram(p, cfg).d;
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
        default: return 1000.0f;
    }
}

static float mapSdf(float3 p, constant FptRenderConfig &cfg) {
    float d = distanceSdf(p, cfg);
    return mix(d, d / 2.0f, cfg.render[6]);
}

static bool sdfHasTranslucentSurfaces(constant FptRenderConfig &cfg) {
    return cfg.sdf_id == SDF_GLASS_BALL || cfg.sdf_id == SDF_README_GLASS ||
           (cfg.sdf_id == SDF_PROGRAM && cfg.program_material[5] > 0.0f);
}

static float3 march(float3 dr, float3 rp, int ni, float min_dist, float lod_falloff, constant FptRenderConfig &cfg) {
    float3 cam_pos = rp;
    int max_iter = min(ni, 360);
    bool check_translucency = sdfHasTranslucentSurfaces(cfg);
    for (int i = 0; i < max_iter; i++) {
        float d = mapSdf(rp, cfg);
        if (!isfinite(d)) break;
        float o = abs(d) * 0.99f;
        rp += dr * o;
        float lod = min_dist;
        if (lod_falloff > 0.00001f) {
            float fog_lod = dot(cam_pos - rp, cam_pos - rp);
            lod = mix(lod, 0.1f, fog_lod * lod_falloff);
            lod = mix(min_dist, lod, cfg.render[5]);
        }
        if (check_translucency && userSdf(rp, cfg).material.translucency > 0.0f) lod = 0.0001f;
        if (o < lod) break;
        if (cfg.render[4] < o) break;
    }
    return rp;
}

static float3 normalAt(float3 p, constant FptRenderConfig &cfg) {
    float e = max(cfg.render[2], 0.0002f);
    if ((cfg.sdf_normal_mode == 0u || cfg.sdf_normal_mode == 2u) && cfg.sdf_id == SDF_PROGRAM && cfg.sdf_program_count > 0u) {
        float3 gradient = programSurface(p, cfg).gradient;
        if (dot(gradient, gradient) >= 1.0e-12f && isfinite(gradient.x) && isfinite(gradient.y) && isfinite(gradient.z)) {
            return normalize(gradient);
        }
    }
    if (cfg.sdf_normal_mode == 1u) {
        float3 k0 = float3(1.0f, -1.0f, -1.0f);
        float3 k1 = float3(-1.0f, -1.0f, 1.0f);
        float3 k2 = float3(-1.0f, 1.0f, -1.0f);
        float3 k3 = float3(1.0f, 1.0f, 1.0f);
        float3 n4 = k0 * mapSdf(p + k0 * e, cfg)
                  + k1 * mapSdf(p + k1 * e, cfg)
                  + k2 * mapSdf(p + k2 * e, cfg)
                  + k3 * mapSdf(p + k3 * e, cfg);
        if (dot(n4, n4) < 1.0e-12f || !isfinite(n4.x) || !isfinite(n4.y) || !isfinite(n4.z)) return float3(0.0f, 1.0f, 0.0f);
        return normalize(n4);
    }
    float3 n = float3(
        mapSdf(p + float3(e, 0.0f, 0.0f), cfg) - mapSdf(p - float3(e, 0.0f, 0.0f), cfg),
        mapSdf(p + float3(0.0f, e, 0.0f), cfg) - mapSdf(p - float3(0.0f, e, 0.0f), cfg),
        mapSdf(p + float3(0.0f, 0.0f, e), cfg) - mapSdf(p - float3(0.0f, 0.0f, e), cfg)
    );
    if (dot(n, n) < 1.0e-12f || !isfinite(n.x) || !isfinite(n.y) || !isfinite(n.z)) return float3(0.0f, 1.0f, 0.0f);
    return normalize(n);
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
    float3 dr = rotateCamera(normalize(float3(0.0f, 0.0f, focal_length)), cameraYawPitch(cfg));
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

static float3 renderPath(float2 xy, uint sample_idx, constant FptRenderConfig &cfg) {
    float frame = float(sample_idx);
    float3 cam_pos = cameraPos(cfg);
    float3 rp = cam_pos;
    float focal_length = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
    float aa_strength = 0.3f / max(float(cfg.width), float(cfg.height));
    float3 dr = normalize(float3(xy + randomPoint(aa_strength, xy, frame), focal_length));
    dr = rotateCamera(dr, cameraYawPitch(cfg));
    float focus = cfg.focus_distance > 0.0f ? cfg.focus_distance : estimateFocusDistance(cfg);
    float3 fp = rp + dr * focus;
    float2 lens = randomPoint(cfg.camera_dof, xy, frame);
    rp += rotateCamera(float3(lens.x, lens.y, 0.0f), cameraYawPitch(cfg));
    dr = normalize(fp - rp);

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
    float3 rd = rotateCamera(normalize(float3(xy + jitter, focal_length)), cameraYawPitch(cfg));

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

static float3 viewport(float2 xy, constant FptRenderConfig &cfg) {
    float3 rp = cameraPos(cfg);
    float f = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
    float3 dr = rotateCamera(normalize(float3(xy, f)), cameraYawPitch(cfg));
    float3 cam_pos = rp;
    float min_dist = 0.001f;
    float lod_falloff = 0.0002f;
    bool hit = false;
    float travel = 0.0f;
    float hit_lod = min_dist;
    for (int i = 0; i < 160; i++) {
        float d = mapSdf(rp, cfg);
        if (!isfinite(d)) break;
        float o = abs(d) * 0.99f;
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

static uint marchStepCount(float3 dr, float3 rp, int ni, float min_dist, float lod_falloff, constant FptRenderConfig &cfg) {
    float3 cam_pos = rp;
    int max_iter = min(ni, 360);
    uint count = 0u;
    bool check_translucency = sdfHasTranslucentSurfaces(cfg);
    for (int i = 0; i < max_iter; i++) {
        count++;
        float d = mapSdf(rp, cfg);
        if (!isfinite(d)) break;
        float o = abs(d) * 0.99f;
        rp += dr * o;
        float lod = min_dist;
        if (lod_falloff > 0.00001f) {
            float fog_lod = dot(cam_pos - rp, cam_pos - rp);
            lod = mix(lod, 0.1f, fog_lod * lod_falloff);
            lod = mix(min_dist, lod, cfg.render[5]);
        }
        if (check_translucency && userSdf(rp, cfg).material.translucency > 0.0f) lod = 0.0001f;
        if (o < lod) break;
        if (cfg.render[4] < o) break;
    }
    return count;
}

static float3 marchCounted(float3 dr, float3 rp, int ni, float min_dist, float lod_falloff, constant FptRenderConfig &cfg, thread uint &steps) {
    float3 cam_pos = rp;
    int max_iter = min(ni, 360);
    bool check_translucency = sdfHasTranslucentSurfaces(cfg);
    for (int i = 0; i < max_iter; i++) {
        steps++;
        float d = mapSdf(rp, cfg);
        if (!isfinite(d)) break;
        float o = abs(d) * 0.99f;
        rp += dr * o;
        float lod = min_dist;
        if (lod_falloff > 0.00001f) {
            float fog_lod = dot(cam_pos - rp, cam_pos - rp);
            lod = mix(lod, 0.1f, fog_lod * lod_falloff);
            lod = mix(min_dist, lod, cfg.render[5]);
        }
        if (check_translucency && userSdf(rp, cfg).material.translucency > 0.0f) lod = 0.0001f;
        if (o < lod) break;
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
        dr = rotateCamera(dr, cameraYawPitch(cfg));
        float focus = cfg.focus_distance > 0.0f ? cfg.focus_distance : estimateFocusDistance(cfg);
        float3 fp = rp + dr * focus;
        float2 lens = randomPoint(cfg.camera_dof, xy, frame);
        rp += rotateCamera(float3(lens.x, lens.y, 0.0f), cameraYawPitch(cfg));
        dr = normalize(fp - rp);

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

static void sdfProfilePath(float2 xy,
                           uint sample_idx,
                           constant FptRenderConfig &cfg,
                           thread uint &primary_steps,
                           thread uint &shadow_steps,
                           thread uint &normal_evals,
                           thread uint &bounces) {
    float frame = float(sample_idx);
    float3 cam_pos = cameraPos(cfg);
    float3 rp = cam_pos;
    float focal_length = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
    float aa_strength = 0.3f / max(float(cfg.width), float(cfg.height));
    float3 dr = normalize(float3(xy + randomPoint(aa_strength, xy, frame), focal_length));
    dr = rotateCamera(dr, cameraYawPitch(cfg));
    float focus = cfg.focus_distance > 0.0f ? cfg.focus_distance : estimateFocusDistance(cfg);
    float3 fp = rp + dr * focus;
    float2 lens = randomPoint(cfg.camera_dof, xy, frame);
    rp += rotateCamera(float3(lens.x, lens.y, 0.0f), cameraYawPitch(cfg));
    dr = normalize(fp - rp);

    int local_ni = int(cfg.render[1]);
    int bounce_limit = min(int(cfg.render[0]), 8);
    if (cfg.sdf_bounce_cap > 0u) bounce_limit = min(bounce_limit, int(cfg.sdf_bounce_cap));
    float far_dist = cfg.render[4] * 0.99f;
    float far_dist_sq = far_dist * far_dist;
    for (int i = 0; i < bounce_limit; i++) {
        uint local_primary = 0u;
        rp = marchCounted(dr, rp, local_ni, cfg.render[3], 0.0002f, cfg, local_primary);
        primary_steps += local_primary;
        local_ni = int(cfg.render[1] / (cfg.render[5] * 2.0f + 1.0f));
        if (dot(rp - cam_pos, rp - cam_pos) > far_dist_sq) break;
        Material material = userSdf(rp, cfg).material;
        bounces += 1u;
        normal_evals += 1u;
        float3 n = normalAt(rp, cfg);
        if (cfg.sun[0] == 1.0f) shadow_steps += sunStepCountWithNormal(rp, xy, frame, n, cfg);
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
        dr = rotateCamera(dr, cameraYawPitch(cfg));
        float focus = cfg.focus_distance > 0.0f ? cfg.focus_distance : estimateFocusDistance(cfg);
        float3 fp = rp + dr * focus;
        float2 lens = randomPoint(cfg.camera_dof, xy, frame);
        rp += rotateCamera(float3(lens.x, lens.y, 0.0f), cameraYawPitch(cfg));
        dr = normalize(fp - rp);

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
    float focal_length = 1.0f / tan(cfg.camera_fov / 2.0f * pi / 180.0f);
    float3 rd = rotateCamera(normalize(float3(xy, focal_length)), cameraYawPitch(cfg));
    int steps = min(int(max(cfg.render[1], 32.0f)), 420);
    bool hit = false;
    for (int i = 0; i < steps; ++i) {
        float d = mapSdf(rp, cfg);
        if (!isfinite(d)) break;
        if (d < max(cfg.render[3], 0.0005f) && length(rp - ro) > 0.001f) {
            hit = true;
            break;
        }
        rp += rd * clamp(d, max(cfg.render[3], 0.0005f), 0.25f);
        if (length(rp - ro) > cfg.render[4]) break;
    }
    if (diag.mode == FPT_DIAGNOSTIC_DEPTH) return diagnosticEncodeDepth(length(rp - ro), hit, cfg, diag);
    if (!hit) return float3(0.0f);
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
    if (gid.x >= cfg.width || gid.y >= cfg.height) return;
    float2 suv = (float2(gid) + 0.5f) / float2(float(cfg.width), float(cfg.height));
    float2 uv = suv - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
    float3 color = sdfDiagnostic(uv, cfg, diag);
    uint idx = (cfg.height - 1u - gid.y) * cfg.width + gid.x;
    out[idx] = uchar4(uchar(clamp(color.r, 0.0f, 1.0f) * 255.0f),
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
    float2 suv = (float2(gid) + 0.5f) / float2(float(cfg.width), float(cfg.height));
    float2 uv = suv - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);

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
    float2 suv = (float2(gid) + 0.5f) / float2(float(cfg.width), float(cfg.height));
    float2 uv = suv - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
    float3 color = postProcess(viewport(uv, cfg), cfg);
    uint idx = outputIndex(gid, cfg);
    out[idx] = uchar4(uchar(clamp(color.r, 0.0f, 1.0f) * 255.0f),
                      uchar(clamp(color.g, 0.0f, 1.0f) * 255.0f),
                      uchar(clamp(color.b, 0.0f, 1.0f) * 255.0f),
                      255);
}

kernel void preview_linear_kernel(device float4 *accum [[buffer(0)]],
                                  constant FptRenderConfig &cfg [[buffer(1)]],
                                  uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height) return;
    float2 suv = (float2(gid) + 0.5f) / float2(float(cfg.width), float(cfg.height));
    float2 uv = suv - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
    uint index = gid.y * cfg.width + gid.x;
    accum[index] = float4(viewport(uv, cfg), 1.0f);
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
    float2 suv = (float2(gid) + 0.5f) / float2(float(cfg.width), float(cfg.height));
    float2 uv = suv - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
    uint primary_steps = 0u;
    uint shadow_steps = 0u;
    uint normal_evals = 0u;
    uint bounces = 0u;
    sdfProfilePath(uv, profile.frame_index, cfg, primary_steps, shadow_steps, normal_evals, bounces);
    atomic_fetch_add_explicit(&counts->primary_steps, primary_steps, memory_order_relaxed);
    atomic_fetch_add_explicit(&counts->shadow_steps, shadow_steps, memory_order_relaxed);
    atomic_fetch_add_explicit(&counts->normal_evals, normal_evals, memory_order_relaxed);
    atomic_fetch_add_explicit(&counts->bounces, bounces, memory_order_relaxed);
    atomic_fetch_add_explicit(&counts->pixels, 1u, memory_order_relaxed);
}

kernel void accumulate_kernel(device float4 *accum [[buffer(0)]],
                              constant FptRenderConfig &cfg [[buffer(1)]],
                              constant uint &frame_index [[buffer(2)]],
                              uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height) return;
    float2 suv = (float2(gid) + 0.5f) / float2(float(cfg.width), float(cfg.height));
    float2 uv = suv - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
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
    float2 suv = (float2(gid) + 0.5f) / float2(float(cfg.width), float(cfg.height));
    float2 uv = suv - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
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

kernel void accumulate_chunk_kernel(device float4 *accum [[buffer(0)]],
                                    constant FptRenderConfig &cfg [[buffer(1)]],
                                    constant FptAccumulationChunk &chunk [[buffer(2)]],
                                    uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height || chunk.sample_count == 0u) return;
    float2 suv = (float2(gid) + 0.5f) / float2(float(cfg.width), float(cfg.height));
    float2 uv = suv - 0.5f;
    uv.x *= float(cfg.width) / float(cfg.height);
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

kernel void present_kernel(device uchar4 *out [[buffer(0)]],
                           device const float4 *accum [[buffer(1)]],
                           constant FptRenderConfig &cfg [[buffer(2)]],
                           uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= cfg.width || gid.y >= cfg.height) return;
    uint out_idx = outputIndex(gid, cfg);
    float3 color = postProcess(chromaticAt(accum, gid, cfg) + highlightAt(accum, gid, cfg), cfg);
    out[out_idx] = uchar4(uchar(clamp(color.r, 0.0f, 1.0f) * 255.0f),
                      uchar(clamp(color.g, 0.0f, 1.0f) * 255.0f),
                      uchar(clamp(color.b, 0.0f, 1.0f) * 255.0f),
                      255);
}
