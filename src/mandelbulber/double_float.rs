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

// Transcendentals to about 1e-14: series on reduced arguments, or one Newton
// step from the fp32 result, which squares its ~1e-7 error.
static inline DF dfSinSeries(DF r, DF r2) {
    DF p = DF(2.811457359e-15f, -1.046208474e-22f);
    p = DF(7.647163610e-13f, 1.220071047e-20f) - r2 * p;
    p = DF(1.605904437e-10f, -5.352526512e-18f) - r2 * p;
    p = DF(2.505210794e-08f, 4.417623045e-16f) - r2 * p;
    p = DF(2.755731884e-06f, 3.793571224e-14f) - r2 * p;
    p = DF(1.984127011e-04f, -2.725596875e-12f) - r2 * p;
    p = DF(8.333333768e-03f, -4.346172033e-10f) - r2 * p;
    p = DF(1.666666716e-01f, -4.967053879e-09f) - r2 * p;
    return r - r * r2 * p;
}
static inline DF dfCosSeries(DF r2) {
    DF q = DF(4.779477256e-14f, 7.625444044e-22f);
    q = DF(1.147074536e-11f, 2.372207689e-19f) - r2 * q;
    q = DF(2.087675588e-09f, 1.108283981e-16f) - r2 * q;
    q = DF(2.755731998e-07f, -7.575112209e-15f) - r2 * q;
    q = DF(2.480158764e-05f, -3.406996094e-13f) - r2 * q;
    q = DF(1.388888923e-03f, -3.363109444e-11f) - r2 * q;
    q = DF(4.166666791e-02f, -1.241763470e-09f) - r2 * q;
    q = DF(5.000000000e-01f) - r2 * q;
    return DF(1.0f) - r2 * q;
}
static inline void dfSinCos(DF x, thread DF &s, thread DF &c) {
    float k = rint(x.hi * 0.636619772f);
    DF r = x - DF(1.570796371e+00f, -4.371138829e-08f) * k;
    DF r2 = r * r;
    DF sr = dfSinSeries(r, r2);
    DF cr = dfCosSeries(r2);
    int quadrant = int(k) & 3;
    s = quadrant == 0 ? sr : (quadrant == 1 ? cr : (quadrant == 2 ? -sr : -cr));
    c = quadrant == 0 ? cr : (quadrant == 1 ? -sr : (quadrant == 2 ? -cr : sr));
}
static inline DF sin(DF x) { DF s, c; dfSinCos(x, s, c); return s; }
static inline DF cos(DF x) { DF s, c; dfSinCos(x, s, c); return c; }
static inline DF tan(DF x) { DF s, c; dfSinCos(x, s, c); return s / c; }
static inline DF atan2(DF y, DF x) {
    if (x.hi == 0.0f && y.hi == 0.0f) return DF(atan2(y.hi, x.hi));
    float a0 = atan2(y.hi, x.hi);
    DF s, c;
    dfSinCos(DF(a0), s, c);
    return DF(a0) + (y * c - x * s) / (x * c + y * s);
}
static inline DF atan(DF x) { return atan2(x, DF(1.0f)); }
static inline DF mandelSafeAsin(DF value) {
    if (!(abs(value.hi) <= 1.0f + 1.0e-5f)) return DF(as_type<float>(0x7fc00000u));
    value = clamp(value, -1.0f, 1.0f);
    return atan2(value, sqrt((DF(1.0f) - value) * (DF(1.0f) + value)));
}
static inline DF mandelSafeAcos(DF value) {
    if (!(abs(value.hi) <= 1.0f + 1.0e-5f)) return DF(as_type<float>(0x7fc00000u));
    value = clamp(value, -1.0f, 1.0f);
    return atan2(sqrt((DF(1.0f) - value) * (DF(1.0f) + value)), value);
}
static inline DF asin(DF value) { return mandelSafeAsin(value); }
static inline DF acos(DF value) { return mandelSafeAcos(value); }
static inline DF exp(DF x) {
    float k = rint(x.hi * 1.442695041f);
    // e^r - 1 on r / 512, then squared back nine times as s(s + 2).
    DF r = (x - DF(6.931471825e-01f, -1.904654212e-09f) * k) * (1.0f / 512.0f);
    DF p = DF(2.480158764e-05f, -3.406996094e-13f);
    p = DF(1.984127011e-04f, -2.725596875e-12f) + r * p;
    p = DF(1.388888923e-03f, -3.363109444e-11f) + r * p;
    p = DF(8.333333768e-03f, -4.346172033e-10f) + r * p;
    p = DF(4.166666791e-02f, -1.241763470e-09f) + r * p;
    p = DF(1.666666716e-01f, -4.967053879e-09f) + r * p;
    p = DF(5.000000000e-01f) + r * p;
    DF s = r + r * r * p;
    for (int i = 0; i < 9; ++i) s = s * (s + 2.0f);
    DF e = s + 1.0f;
    return DF(ldexp(e.hi, int(k)), ldexp(e.lo, int(k)));
}
static inline DF log(DF a) {
    if (!(a.hi > 0.0f)) return DF(log(a.hi));
    DF l = DF(log(a.hi));
    return l + a * exp(-l) - 1.0f;
}
static inline DF pow(DF a, float p) {
    if (!(a.hi > 0.0f)) return DF(pow(a.hi, p));
    if (p == rint(p) && abs(p) <= 64.0f) {
        int n = int(abs(p));
        DF result = DF(1.0f);
        DF base = a;
        while (n > 0) {
            if ((n & 1) != 0) result = result * base;
            base = base * base;
            n >>= 1;
        }
        return p < 0.0f ? DF(1.0f) / result : result;
    }
    return exp(log(a) * p);
}
static inline DF pow(DF a, DF p) {
    if (p.lo == 0.0f) return pow(a, p.hi);
    if (!(a.hi > 0.0f)) return DF(pow(a.hi, p.hi));
    return exp(log(a) * p);
}

static inline DF dfComponent(DF4 v, int c) {
    return c == 0 ? v.x : (c == 1 ? v.y : (c == 2 ? v.z : v.w));
}
static inline DF dfComponentRest(DF4 v, int c) {
    if (c == 0) v.x = 0.0f;
    else if (c == 1) v.y = 0.0f;
    else if (c == 2) v.z = 0.0f;
    else v.w = 0.0f;
    return length(v);
}
static inline bool dfComponentRadius(DF4 v, DF r) {
    return r.hi > 0.0f && abs(length(v) - r).hi <= 1.0e-6f * r.hi;
}
static inline DF mandelComponentAsin(DF4 v, int c, DF r) {
    return dfComponentRadius(v, r) ? atan2(dfComponent(v, c), dfComponentRest(v, c))
                                   : mandelSafeAsin(dfComponent(v, c) / r);
}
static inline DF mandelComponentAcos(DF4 v, int c, DF r) {
    return dfComponentRadius(v, r) ? atan2(dfComponentRest(v, c), dfComponent(v, c))
                                   : mandelSafeAcos(dfComponent(v, c) / r);
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

/// Reads of `fractal.<path>` in a double-float formula become `DF(value,
/// remainder)` for parameters whose f32 constant drops a nonzero remainder.
pub(super) fn with_parameter_remainders(source: &str, remainders: &[(&str, f32)]) -> String {
    remainders
        .iter()
        .filter(|(_, remainder)| *remainder != 0.0)
        .fold(source.to_owned(), |source, (path, remainder)| {
            source.replace(path, &format!("DF({path}, {remainder:.9e}f)"))
        })
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

/// One hybrid slot to translate into double-float.
pub(super) struct HybridDeepSlot<'a> {
    pub index: usize,
    pub namespace: &'a str,
    /// The slot's own emitted namespace source (parameters, helpers, formula).
    pub declarations: &'a str,
    pub function_name: &'a str,
    /// The slot's float constant-addition statement, if any.
    pub constant_addition: &'a str,
}

/// Double-float twin of `mandelHybridOrbit`: the same sequence, weights,
/// smoothing, constant additions, per-slot bailouts and NaN rule, iterated from
/// origin + the camera-relative point, returning the float orbit result.
pub(super) fn hybrid_orbit_source(
    slots: &[HybridDeepSlot<'_>],
    initial_w: &str,
    actual_scale: &str,
    sequence_length: usize,
) -> Result<String> {
    let mut namespaces = String::new();
    let mut dispatch_cases = String::new();
    for slot in slots {
        namespaces.push_str(&format!(
            "namespace {} {{\n{}}}\n\n",
            slot.namespace,
            formula_source(slot.declarations, slot.function_name)?
        ));
        dispatch_cases.push_str(&format!(
            "            case {}:\n                if (formula_weight > 0.0f) z = {}::{}Deep(z, {}::kMandelFormulaParameters, aux);\n{}                break;\n",
            slot.index,
            slot.namespace,
            slot.function_name,
            slot.namespace,
            slot.constant_addition.replace("float4(", "DF4(")
        ));
    }
    Ok(format!(
        r#"{namespaces}static DF4 dfHybridSmoothVector(DF4 first, DF4 second, float weight) {{
    if (weight <= 0.0f) return first;
    if (weight >= 1.0f) return second;
    float inverse_weight = 1.0f - weight;
    DF interpolated_length = length(first) * inverse_weight + length(second) * weight;
    DF4 blended = first * inverse_weight + second * weight;
    DF blended_length = length(blended);
    return blended_length.hi > 0.0f ? blended * (interpolated_length / blended_length) : first;
}}

static MandelHybridOrbitResult mandelHybridDeepOrbit(float3 scaled,
                                                      constant FptRenderConfig &cfg,
                                                      int forced_iterations,
                                                      int iteration_budget) {{
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
    int completed_iterations = 0;
    int iteration_multiplier = max(-forced_iterations, 1);
    int configured_iterations = min(
        clamp(int(setv(cfg, 1)) * iteration_multiplier, 1, 4096),
        {sequence_length});
    int max_iterations = forced_iterations >= 0
        ? min(configured_iterations, forced_iterations)
        : (iteration_budget > 0
            ? min(configured_iterations, iteration_budget)
            : configured_iterations);
    bool escaped = false;
    DF4 last_last_z = DF4(DF(0.0f));
    for (int iteration = 0; iteration < max_iterations; ++iteration) {{
        int sequence_index = int(kMandelHybridSequence[iteration]);
        float formula_weight = kMandelHybridWeights[sequence_index];
        aux.i = uint(iteration);
        last_last_z = aux.old_z;
        aux.old_z = z;
        DF4 previous_z = z;
        DF previous_de = aux.DE;
        DF previous_color = aux.color;
        switch (sequence_index) {{
{dispatch_cases}        }}
        if (formula_weight < 1.0f) {{
            z = dfHybridSmoothVector(previous_z, z, formula_weight);
            float inverse_weight = 1.0f - formula_weight;
            aux.DE = aux.DE * formula_weight + previous_de * inverse_weight;
            aux.color = aux.color * formula_weight + previous_color * inverse_weight;
        }}
        aux.r = length(z);
        if (dfIsNaN(z)) {{
            z = aux.old_z;
            aux.r = length(z);
            completed_iterations = iteration + 1;
            break;
        }}
        completed_iterations = iteration + 1;
        if (forced_iterations < 0
            && kMandelHybridBailoutChecks[sequence_index] != 0) {{
            float bailout = kMandelHybridBailouts[sequence_index];
            if (aux.r > bailout) {{
                escaped = true;
                break;
            }}
            if (kMandelHybridAdditionalBailoutChecks[sequence_index] != 0) {{
                escaped = true;
                if (length(z - aux.old_z) / aux.r < 0.1f / bailout) break;
                if (length(z - last_last_z) / aux.r < 0.1f / bailout) break;
            }}
        }}
    }}
    float4 float_z = float4(float(z.x), float(z.y), float(z.z), float(z.w));
    return {{ float_z, float(aux.r), float(aux.DE), float(aux.dist),
              float(aux.pseudoKleinianDE), completed_iterations, escaped }};
}}
"#
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
    fn parameter_remainders_wrap_only_inexact_reads() {
        let source = "DF a = fractal.bulb.power - 1.0f; DF b = fractal.bulb.alphaAngleOffset;";
        let wrapped = with_parameter_remainders(
            source,
            &[
                ("fractal.bulb.power", 0.0),
                ("fractal.bulb.alphaAngleOffset", 1.5e-8),
            ],
        );
        assert!(wrapped.contains("DF a = fractal.bulb.power - 1.0f;"));
        assert!(wrapped.contains(&format!(
            "DF(fractal.bulb.alphaAngleOffset, {:.9e}f)",
            1.5e-8_f32
        )));
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
