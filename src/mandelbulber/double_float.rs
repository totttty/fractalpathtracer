//! Double-float orbits for deep zooms.
//!
//! Some catalogue scenes zoom so far that the surface detail is below one
//! fp32 ulp of the position (Mandelbulber's own notes say they need its
//! double-precision CPU path). Those scenes are rebased so the camera sits at
//! the world origin, which keeps marching positions precise, and the fractal
//! adds the origin back and iterates in double-float: an unevaluated sum of two
//! floats, about 48 mantissa bits.

use anyhow::{Result, ensure};

use super::dual::{FormulaTypes, translate_formula};

const DEEP_TYPES: FormulaTypes = FormulaTypes {
    scalar: "DF",
    vector3: "DF3",
    vector4: "DF4",
    state: "MandelDeepOrbitState",
    suffix: "Deep",
};

/// Marks generated source that needs IEEE arithmetic: the error-free
/// transforms below rely on exact rounding that fast-math reassociation removes.
pub(super) const DEEP_MARKER: &str = "#define FPT_MANDEL_DOUBLE_FLOAT 1";

pub(super) const LIBRARY: &str = r#"
struct DF {
    float hi;
    float lo;
    DF() : hi(0.0f), lo(0.0f) {}
    DF(float value) : hi(value), lo(0.0f) {}
    DF(float high, float low) : hi(high), lo(low) {}
    explicit operator float() const { return hi + lo; }
    explicit operator int() const { return int(hi + lo); }
};

static inline DF dfQuickTwoSum(float a, float b) {
    float s = a + b;
    return DF(s, b - (s - a));
}
static inline DF dfTwoSum(float a, float b) {
    float s = a + b;
    float bb = s - a;
    return DF(s, (a - (s - bb)) + (b - bb));
}
static inline DF dfTwoProd(float a, float b) {
    float p = a * b;
    return DF(p, fma(a, b, -p));
}

static inline DF operator+(DF a, DF b) {
    DF s = dfTwoSum(a.hi, b.hi);
    DF t = dfTwoSum(a.lo, b.lo);
    s.lo += t.hi;
    s = dfQuickTwoSum(s.hi, s.lo);
    s.lo += t.lo;
    return dfQuickTwoSum(s.hi, s.lo);
}
static inline DF operator-(DF a) { return DF(-a.hi, -a.lo); }
static inline DF operator-(DF a, DF b) { return a + (-b); }
static inline DF operator*(DF a, DF b) {
    DF p = dfTwoProd(a.hi, b.hi);
    p.lo += a.hi * b.lo + a.lo * b.hi;
    return dfQuickTwoSum(p.hi, p.lo);
}
static inline DF operator/(DF a, DF b) {
    float q1 = a.hi / b.hi;
    DF r = a - b * DF(q1);
    float q2 = r.hi / b.hi;
    r = r - b * DF(q2);
    float q3 = r.hi / b.hi;
    return dfQuickTwoSum(q1, q2) + DF(q3);
}
static inline DF operator+(DF a, float b) { return a + DF(b); }
static inline DF operator+(float a, DF b) { return DF(a) + b; }
static inline DF operator-(DF a, float b) { return a - DF(b); }
static inline DF operator-(float a, DF b) { return DF(a) - b; }
static inline DF operator*(DF a, float b) { return a * DF(b); }
static inline DF operator*(float a, DF b) { return DF(a) * b; }
static inline DF operator/(DF a, float b) { return a / DF(b); }
static inline DF operator/(float a, DF b) { return DF(a) / b; }
static inline thread DF &operator+=(thread DF &a, DF b) { a = a + b; return a; }
static inline thread DF &operator-=(thread DF &a, DF b) { a = a - b; return a; }
static inline thread DF &operator*=(thread DF &a, DF b) { a = a * b; return a; }
static inline thread DF &operator/=(thread DF &a, DF b) { a = a / b; return a; }
static inline thread DF &operator+=(thread DF &a, float b) { a = a + DF(b); return a; }
static inline thread DF &operator-=(thread DF &a, float b) { a = a - DF(b); return a; }
static inline thread DF &operator*=(thread DF &a, float b) { a = a * DF(b); return a; }
static inline thread DF &operator/=(thread DF &a, float b) { a = a / DF(b); return a; }
static inline bool operator<(DF a, DF b) { return a.hi < b.hi || (a.hi == b.hi && a.lo < b.lo); }
static inline bool operator>(DF a, DF b) { return b < a; }
static inline bool operator<=(DF a, DF b) { return !(b < a); }
static inline bool operator>=(DF a, DF b) { return !(a < b); }
static inline bool operator==(DF a, DF b) { return a.hi == b.hi && a.lo == b.lo; }
static inline bool operator!=(DF a, DF b) { return !(a == b); }
static inline bool operator<(DF a, float b) { return a < DF(b); }
static inline bool operator>(DF a, float b) { return a > DF(b); }
static inline bool operator<=(DF a, float b) { return a <= DF(b); }
static inline bool operator>=(DF a, float b) { return a >= DF(b); }
static inline bool operator<(float a, DF b) { return DF(a) < b; }
static inline bool operator>(float a, DF b) { return DF(a) > b; }
static inline bool operator<=(float a, DF b) { return DF(a) <= b; }
static inline bool operator>=(float a, DF b) { return DF(a) >= b; }

static inline DF abs(DF a) { return a.hi < 0.0f || (a.hi == 0.0f && a.lo < 0.0f) ? -a : a; }
static inline DF sign(DF a) { return DF(a.hi > 0.0f ? 1.0f : (a.hi < 0.0f ? -1.0f : 0.0f)); }
static inline DF min(DF a, DF b) { return b < a ? b : a; }
static inline DF max(DF a, DF b) { return a < b ? b : a; }
static inline DF min(DF a, float b) { return min(a, DF(b)); }
static inline DF max(DF a, float b) { return max(a, DF(b)); }
static inline DF min(float a, DF b) { return min(DF(a), b); }
static inline DF max(float a, DF b) { return max(DF(a), b); }
static inline DF clamp(DF a, DF lo, DF hi) { return min(max(a, lo), hi); }
static inline DF clamp(DF a, float lo, float hi) { return min(max(a, DF(lo)), DF(hi)); }
static inline DF sqrt(DF a) {
    if (a.hi == 0.0f) return DF(0.0f);
    float q = sqrt(a.hi);
    DF qq = dfTwoProd(q, q);
    float r = ((a.hi - qq.hi) - qq.lo + a.lo) * (0.5f / q);
    return dfQuickTwoSum(q, r);
}
static inline DF floor(DF a) {
    float h = floor(a.hi);
    return h == a.hi ? dfQuickTwoSum(h, floor(a.lo)) : DF(h);
}

struct DF3 {
    DF x, y, z;
    DF3() {}
    explicit DF3(DF s) : x(s), y(s), z(s) {}
    DF3(DF a, DF b, DF c) : x(a), y(b), z(c) {}
    DF3(float3 v) : x(v.x), y(v.y), z(v.z) {}
};
static inline DF3 operator+(DF3 a, DF3 b) { return DF3(a.x + b.x, a.y + b.y, a.z + b.z); }
static inline DF3 operator-(DF3 a, DF3 b) { return DF3(a.x - b.x, a.y - b.y, a.z - b.z); }
static inline DF3 operator*(DF3 a, DF3 b) { return DF3(a.x * b.x, a.y * b.y, a.z * b.z); }
static inline DF3 operator/(DF3 a, DF3 b) { return DF3(a.x / b.x, a.y / b.y, a.z / b.z); }
static inline DF3 operator*(DF3 a, DF b) { return DF3(a.x * b, a.y * b, a.z * b); }
static inline DF3 operator*(DF b, DF3 a) { return a * b; }
static inline DF3 operator/(DF3 a, DF b) { return DF3(a.x / b, a.y / b, a.z / b); }
static inline DF3 operator-(DF3 a) { return DF3(-a.x, -a.y, -a.z); }
static inline DF3 operator*(DF3 a, float b) { return a * DF(b); }
static inline DF3 operator*(float b, DF3 a) { return a * DF(b); }
static inline DF3 operator/(DF3 a, float b) { return a / DF(b); }
static inline DF3 operator+(DF3 a, float3 b) { return a + DF3(b); }
static inline DF3 operator-(DF3 a, float3 b) { return a - DF3(b); }
static inline DF3 operator*(DF3 a, float3 b) { return a * DF3(b); }
static inline thread DF3 &operator+=(thread DF3 &a, DF3 b) { a = a + b; return a; }
static inline thread DF3 &operator-=(thread DF3 &a, DF3 b) { a = a - b; return a; }
static inline thread DF3 &operator*=(thread DF3 &a, DF3 b) { a = a * b; return a; }
static inline thread DF3 &operator*=(thread DF3 &a, DF b) { a = a * b; return a; }
static inline thread DF3 &operator/=(thread DF3 &a, DF b) { a = a / b; return a; }
static inline thread DF3 &operator*=(thread DF3 &a, float b) { a = a * DF(b); return a; }
static inline thread DF3 &operator+=(thread DF3 &a, float3 b) { a = a + DF3(b); return a; }
static inline thread DF3 &operator-=(thread DF3 &a, float3 b) { a = a - DF3(b); return a; }
static inline thread DF3 &operator*=(thread DF3 &a, float3 b) { a = a * DF3(b); return a; }
static inline DF dot(DF3 a, DF3 b) { return a.x * b.x + a.y * b.y + a.z * b.z; }
static inline DF length(DF3 a) { return sqrt(dot(a, a)); }
static inline DF3 abs(DF3 a) { return DF3(abs(a.x), abs(a.y), abs(a.z)); }
static inline DF3 normalize(DF3 a) { return a / length(a); }

struct DF4 {
    DF x, y, z, w;
    DF4() {}
    explicit DF4(DF s) : x(s), y(s), z(s), w(s) {}
    DF4(DF a, DF b, DF c, DF d) : x(a), y(b), z(c), w(d) {}
    DF4(float4 v) : x(v.x), y(v.y), z(v.z), w(v.w) {}
    DF3 xyz() const { return DF3(x, y, z); }
};
static inline DF4 operator+(DF4 a, DF4 b) { return DF4(a.x + b.x, a.y + b.y, a.z + b.z, a.w + b.w); }
static inline DF4 operator-(DF4 a, DF4 b) { return DF4(a.x - b.x, a.y - b.y, a.z - b.z, a.w - b.w); }
static inline DF4 operator*(DF4 a, DF4 b) { return DF4(a.x * b.x, a.y * b.y, a.z * b.z, a.w * b.w); }
static inline DF4 operator/(DF4 a, DF4 b) { return DF4(a.x / b.x, a.y / b.y, a.z / b.z, a.w / b.w); }
static inline DF4 operator*(DF4 a, DF b) { return DF4(a.x * b, a.y * b, a.z * b, a.w * b); }
static inline DF4 operator*(DF b, DF4 a) { return a * b; }
static inline DF4 operator/(DF4 a, DF b) { return DF4(a.x / b, a.y / b, a.z / b, a.w / b); }
static inline DF4 operator-(DF4 a) { return DF4(-a.x, -a.y, -a.z, -a.w); }
static inline DF4 operator*(DF4 a, float b) { return a * DF(b); }
static inline DF4 operator*(float b, DF4 a) { return a * DF(b); }
static inline DF4 operator/(DF4 a, float b) { return a / DF(b); }
static inline DF4 operator*(DF4 a, float4 b) { return a * DF4(b); }
static inline DF4 operator+(DF4 a, float4 b) { return a + DF4(b); }
static inline DF4 operator-(DF4 a, float4 b) { return a - DF4(b); }
static inline thread DF4 &operator+=(thread DF4 &a, DF4 b) { a = a + b; return a; }
static inline thread DF4 &operator-=(thread DF4 &a, DF4 b) { a = a - b; return a; }
static inline thread DF4 &operator*=(thread DF4 &a, DF4 b) { a = a * b; return a; }
static inline thread DF4 &operator*=(thread DF4 &a, DF b) { a = a * b; return a; }
static inline thread DF4 &operator/=(thread DF4 &a, DF b) { a = a / b; return a; }
static inline thread DF4 &operator*=(thread DF4 &a, float b) { a = a * DF(b); return a; }
static inline thread DF4 &operator/=(thread DF4 &a, float b) { a = a / DF(b); return a; }
static inline thread DF4 &operator*=(thread DF4 &a, float4 b) { a = a * DF4(b); return a; }
static inline thread DF4 &operator+=(thread DF4 &a, float4 b) { a = a + DF4(b); return a; }
static inline thread DF4 &operator-=(thread DF4 &a, float4 b) { a = a - DF4(b); return a; }
static inline DF dot(DF4 a, DF4 b) { return a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w; }
static inline DF length(DF4 a) { return sqrt(dot(a, a)); }
static inline DF4 abs(DF4 a) { return DF4(abs(a.x), abs(a.y), abs(a.z), abs(a.w)); }
static inline DF4 normalize(DF4 a) { return a / length(a); }
static inline bool dfIsNaN(DF4 a) {
    uint4 bits = as_type<uint4>(float4(a.x.hi, a.y.hi, a.z.hi, a.w.hi));
    return any(((bits & 0x7f800000u) == 0x7f800000u) & ((bits & 0x007fffffu) != 0u));
}

struct MandelDeepOrbitState {
    uint i;
    DF4 c;
    DF4 const_c;
    DF4 old_z;
    DF pos_neg;
    DF r;
    DF DE;
    DF DE0;
    DF dist;
    DF pseudoKleinianDE;
    DF actualScale;
    DF actualScaleA;
    DF color;
    DF colorHybrid;
    DF temp1000;
};
"#;

const MATRIX: &str = r#"
static inline DF4 Matrix33MulFloat4(matrix33 matrix, DF4 value) {
    DF3 v = DF3(value.x, value.y, value.z);
    return DF4(dot(v, DF3(matrix.m1)), dot(v, DF3(matrix.m2)), dot(v, DF3(matrix.m3)), value.w);
}
"#;

/// The origin (native axes) as hi and lo float constants.
pub(super) fn origin_constants(origin: [f64; 3]) -> String {
    let split = |value: f64| {
        let hi = value as f32;
        (hi, (value - f64::from(hi)) as f32)
    };
    let [x, y, z] = origin.map(split);
    format!(
        "constant float3 kMandelDeepOriginHi = float3({:.9e}f, {:.9e}f, {:.9e}f);\n\
         constant float3 kMandelDeepOriginLo = float3({:.9e}f, {:.9e}f, {:.9e}f);\n",
        x.0, y.0, z.0, x.1, y.1, z.1
    )
}

/// Double-float copy of the formula, with the matrix helper when the
/// declarations define `matrix33`.
pub(super) fn formula_source(declarations: &str, function_name: &str) -> Result<String> {
    let matrix = if declarations.contains("struct matrix33") {
        MATRIX
    } else {
        ""
    };
    Ok(format!(
        "{matrix}{}\n",
        translate_formula(declarations, function_name, &DEEP_TYPES)?
    ))
}

/// Field sample for one analytic formula: the point is camera-relative, the
/// orbit runs in double-float from origin + point, and the distance finalizer
/// reads float copies of the orbit's radius and derivative.
pub(super) fn standalone_sample_source(
    function_name: &str,
    initial_w: &str,
    actual_scale: &str,
    post_iteration: &str,
    additional_bailout: i32,
    distance_expression: &str,
) -> Result<String> {
    ensure!(
        !post_iteration.contains("mandelApplyGlobalFoldings"),
        "deep zoom does not support global folds"
    );
    Ok(format!(
        r#"static float4 mandelbulberGeneratedFieldSample(float3 p,
                                                constant FptRenderConfig &cfg,
                                                int iteration_multiplier,
                                                int iteration_budget) {{
    float world_scale = max(setv(cfg, 0), 1.0f);
    float3 scaled = p / world_scale;
    int completed_iterations = 0;
    bool escaped = false;
    int configured_iterations = clamp(
        int(setv(cfg, 1)) * iteration_multiplier, 1, 4096);
    int max_iterations = iteration_budget > 0
        ? min(configured_iterations, iteration_budget)
        : configured_iterations;
    float bailout = max(setv(cfg, 2), 1.0f);
    MandelOrbitState result = {{}};
    {{
        DF4 z = DF4(DF(kMandelDeepOriginHi.x, kMandelDeepOriginLo.x) + DF(scaled.x),
                    DF(kMandelDeepOriginHi.y, kMandelDeepOriginLo.y) + DF(scaled.z),
                    DF(kMandelDeepOriginHi.z, kMandelDeepOriginLo.z) + DF(scaled.y),
                    DF({initial_w}));
        MandelDeepOrbitState aux;
        aux.i = 0u;
        aux.c = z;
        aux.const_c = z;
        aux.old_z = z;
        aux.pos_neg = 1.0f;
        aux.r = length(z);
        aux.DE = 1.0f;
        aux.DE0 = 0.0f;
        aux.dist = 1000.0f;
        aux.pseudoKleinianDE = 1.0f;
        aux.actualScale = {actual_scale};
        aux.actualScaleA = 0.0f;
        aux.color = 1.0f;
        aux.colorHybrid = 0.0f;
        aux.temp1000 = 1000.0f;
        DF4 last_last_z = DF4(DF(0.0f));
        for (int iteration = 0; iteration < max_iterations; ++iteration) {{
            aux.i = uint(iteration);
            last_last_z = aux.old_z;
            aux.old_z = z;
            z = {function_name}Deep(z, kMandelFormulaParameters, aux);
{post_iteration}            aux.r = length(z);
            if (dfIsNaN(z)) {{
                z = aux.old_z;
                aux.r = length(z);
                completed_iterations = iteration + 1;
                break;
            }}
            completed_iterations = iteration + 1;
            if (aux.r > bailout) {{
                escaped = true;
                break;
            }}
            if ({additional_bailout} != 0) {{
                escaped = true;
                if (length(z - aux.old_z) / aux.r < 0.1f / bailout) break;
                if (length(z - last_last_z) / aux.r < 0.1f / bailout) break;
            }}
        }}
        result.r = float(aux.r);
        result.DE = float(aux.DE);
        result.dist = float(aux.dist);
        result.pseudoKleinianDE = float(aux.pseudoKleinianDE);
    }}
    MandelOrbitState aux = result;
    float distance = ({distance_expression}) * world_scale;
    float iteration_state = escaped
        ? -float(completed_iterations)
        : float(completed_iterations);
    return float4(distance, aux.r, aux.DE, iteration_state);
}}
"#
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_constants_carry_the_double_remainder() {
        let source = origin_constants([-2.398_636_884_638_98, 1.0, 0.0]);
        let hi = -2.398_636_884_638_98_f64 as f32;
        assert!(source.contains(&format!("{hi:.9e}f")));
        let lo = (-2.398_636_884_638_98 - f64::from(hi)) as f32;
        assert!(lo != 0.0);
        assert!(source.contains(&format!("{lo:.9e}f")));
    }

    #[test]
    fn deep_formula_uses_double_float_types() {
        let declarations = "static float4 FooIteration(float4 z,\n    constant const MandelFractalParameters &fractal,\n    thread MandelOrbitState &aux) {\n    float r = dot(z, z);\n    return z * r;\n}";
        let source = formula_source(declarations, "FooIteration").unwrap();
        assert!(source.contains("static DF4 FooIterationDeep(DF4 z,"));
        assert!(source.contains("thread MandelDeepOrbitState &aux"));
        assert!(source.contains("DF r = dot(z, z);"));
    }
}
