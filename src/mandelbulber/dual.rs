//! Forward-mode automatic differentiation for Delta-DE formulas.
//!
//! Mandelbulber estimates the radial gradient with double-precision finite
//! differences a tiny fraction of the detail size apart. In fp32 that step is
//! either below the orbit's rounding noise or far coarser than the surface, so
//! FPT carries d(z)/d(point) through a dual-number copy of the formula instead
//! and reads |grad r| exactly. Only formulas whose translated source has been
//! checked against native renders use it; the rest keep finite differences.

use anyhow::{Context, Result};

/// Formula ids whose dual translation compiles and matches native geometry.
const DUAL_FORMULA_IDS: [i32; 14] = [13, 16, 19, 39, 40, 49, 63, 71, 72, 82, 86, 96, 100, 251];

pub(super) fn supports(formula_id: i32) -> bool {
    DUAL_FORMULA_IDS.contains(&formula_id)
}

/// Dual scalar (value plus gradient over the three sample coordinates) and the
/// vector types formula code needs. Members keep the x/y/z/w names so translated
/// component access compiles unchanged.
pub(super) const LIBRARY: &str = r#"
struct MD {
    float v;
    float3 d;
    MD() : v(0.0f), d(0.0f) {}
    MD(float value) : v(value), d(0.0f) {}
    MD(float value, float3 gradient) : v(value), d(gradient) {}
};

static inline MD operator+(MD a, MD b) { return MD(a.v + b.v, a.d + b.d); }
static inline MD operator-(MD a, MD b) { return MD(a.v - b.v, a.d - b.d); }
static inline MD operator*(MD a, MD b) { return MD(a.v * b.v, a.d * b.v + b.d * a.v); }
static inline MD operator/(MD a, MD b) {
    return MD(a.v / b.v, (a.d * b.v - b.d * a.v) / (b.v * b.v));
}
static inline MD operator-(MD a) { return MD(-a.v, -a.d); }
static inline MD operator+(MD a, float b) { return MD(a.v + b, a.d); }
static inline MD operator+(float a, MD b) { return MD(a + b.v, b.d); }
static inline MD operator-(MD a, float b) { return MD(a.v - b, a.d); }
static inline MD operator-(float a, MD b) { return MD(a - b.v, -b.d); }
static inline MD operator*(MD a, float b) { return MD(a.v * b, a.d * b); }
static inline MD operator*(float a, MD b) { return MD(a * b.v, b.d * a); }
static inline MD operator/(MD a, float b) { return MD(a.v / b, a.d / b); }
static inline MD operator/(float a, MD b) { return MD(a / b.v, -b.d * (a / (b.v * b.v))); }
static inline bool operator<(MD a, float b) { return a.v < b; }
static inline bool operator<(float a, MD b) { return a < b.v; }
static inline bool operator>(MD a, float b) { return a.v > b; }
static inline bool operator>(float a, MD b) { return a > b.v; }
static inline bool operator<=(MD a, float b) { return a.v <= b; }
static inline bool operator<=(float a, MD b) { return a <= b.v; }
static inline bool operator>=(MD a, float b) { return a.v >= b; }
static inline bool operator>=(float a, MD b) { return a >= b.v; }
static inline bool operator==(MD a, float b) { return a.v == b; }
static inline bool operator!=(MD a, float b) { return a.v != b; }
static inline thread MD &operator+=(thread MD &a, MD b) { a = a + b; return a; }
static inline thread MD &operator-=(thread MD &a, MD b) { a = a - b; return a; }
static inline thread MD &operator*=(thread MD &a, MD b) { a = a * b; return a; }
static inline thread MD &operator/=(thread MD &a, MD b) { a = a / b; return a; }
static inline thread MD &operator+=(thread MD &a, float b) { a = a + b; return a; }
static inline thread MD &operator-=(thread MD &a, float b) { a = a - b; return a; }
static inline thread MD &operator*=(thread MD &a, float b) { a = a * b; return a; }
static inline thread MD &operator/=(thread MD &a, float b) { a = a / b; return a; }
static inline bool operator<(MD a, MD b) { return a.v < b.v; }
static inline bool operator>(MD a, MD b) { return a.v > b.v; }
static inline bool operator<=(MD a, MD b) { return a.v <= b.v; }
static inline bool operator>=(MD a, MD b) { return a.v >= b.v; }
static inline bool operator==(MD a, MD b) { return a.v == b.v; }
static inline bool operator!=(MD a, MD b) { return a.v != b.v; }

static inline MD sqrt(MD a) {
    float v = sqrt(a.v);
    return MD(v, v > 0.0f ? a.d * (0.5f / v) : float3(0.0f));
}
static inline MD abs(MD a) { return MD(abs(a.v), a.d * sign(a.v)); }
static inline MD sign(MD a) { return MD(sign(a.v)); }
static inline MD floor(MD a) { return MD(floor(a.v)); }
static inline MD ceil(MD a) { return MD(ceil(a.v)); }
static inline MD round(MD a) { return MD(round(a.v)); }
static inline MD trunc(MD a) { return MD(trunc(a.v)); }
static inline MD fmod(MD a, float b) { return MD(fmod(a.v, b), a.d); }
static inline MD min(MD a, MD b) { return a.v <= b.v ? a : b; }
static inline MD max(MD a, MD b) { return a.v >= b.v ? a : b; }
static inline MD min(MD a, float b) { return min(a, MD(b)); }
static inline MD min(float a, MD b) { return min(MD(a), b); }
static inline MD max(MD a, float b) { return max(a, MD(b)); }
static inline MD max(float a, MD b) { return max(MD(a), b); }
static inline MD clamp(MD a, MD lo, MD hi) { return min(max(a, lo), hi); }
static inline MD clamp(MD a, float lo, float hi) { return min(max(a, MD(lo)), MD(hi)); }
static inline MD sin(MD a) { return MD(sin(a.v), a.d * cos(a.v)); }
static inline MD cos(MD a) { return MD(cos(a.v), -a.d * sin(a.v)); }
static inline MD tan(MD a) { float t = tan(a.v); return MD(t, a.d * (1.0f + t * t)); }
static inline MD exp(MD a) { float e = exp(a.v); return MD(e, a.d * e); }
static inline MD log(MD a) { return MD(log(a.v), a.d / a.v); }
static inline MD asin(MD a) {
    float s = 1.0f - a.v * a.v;
    return MD(asin(a.v), s > 0.0f ? a.d * rsqrt(s) : float3(0.0f));
}
static inline MD acos(MD a) {
    float s = 1.0f - a.v * a.v;
    return MD(acos(a.v), s > 0.0f ? -a.d * rsqrt(s) : float3(0.0f));
}
static inline MD atan(MD a) { return MD(atan(a.v), a.d / (1.0f + a.v * a.v)); }
static inline MD atan2(MD y, MD x) {
    float q = x.v * x.v + y.v * y.v;
    return MD(atan2(y.v, x.v), q > 0.0f ? (y.d * x.v - x.d * y.v) / q : float3(0.0f));
}
static inline MD pow(MD a, float p) {
    return MD(pow(a.v, p), a.d * (p * pow(a.v, p - 1.0f)));
}
static inline MD pow(MD a, MD p) {
    float v = pow(a.v, p.v);
    return MD(v, v * (p.d * log(a.v) + a.d * (p.v / a.v)));
}
static inline MD mandelSafeAsin(MD a) {
    float c = clamp(a.v, -1.0f, 1.0f);
    float s = 1.0f - c * c;
    return MD(mandelSafeAsin(a.v), s > 0.0f && abs(a.v) < 1.0f ? a.d * rsqrt(s) : float3(0.0f));
}
static inline MD mandelSafeAcos(MD a) {
    float c = clamp(a.v, -1.0f, 1.0f);
    float s = 1.0f - c * c;
    return MD(mandelSafeAcos(a.v), s > 0.0f && abs(a.v) < 1.0f ? -a.d * rsqrt(s) : float3(0.0f));
}

struct MD3 {
    MD x, y, z;
    MD3() {}
    explicit MD3(MD s) : x(s), y(s), z(s) {}
    MD3(MD a, MD b, MD c) : x(a), y(b), z(c) {}
    MD3(float3 v) : x(v.x), y(v.y), z(v.z) {}
};
static inline MD3 operator+(MD3 a, MD3 b) { return MD3(a.x + b.x, a.y + b.y, a.z + b.z); }
static inline MD3 operator-(MD3 a, MD3 b) { return MD3(a.x - b.x, a.y - b.y, a.z - b.z); }
static inline MD3 operator*(MD3 a, MD3 b) { return MD3(a.x * b.x, a.y * b.y, a.z * b.z); }
static inline MD3 operator/(MD3 a, MD3 b) { return MD3(a.x / b.x, a.y / b.y, a.z / b.z); }
static inline MD3 operator*(MD3 a, MD b) { return MD3(a.x * b, a.y * b, a.z * b); }
static inline MD3 operator*(MD b, MD3 a) { return a * b; }
static inline MD3 operator/(MD3 a, MD b) { return MD3(a.x / b, a.y / b, a.z / b); }
static inline MD3 operator-(MD3 a) { return MD3(-a.x, -a.y, -a.z); }
static inline MD3 operator*(MD3 a, float b) { return a * MD(b); }
static inline MD3 operator*(float b, MD3 a) { return a * MD(b); }
static inline MD3 operator/(MD3 a, float b) { return a / MD(b); }
static inline thread MD3 &operator+=(thread MD3 &a, MD3 b) { a = a + b; return a; }
static inline thread MD3 &operator-=(thread MD3 &a, MD3 b) { a = a - b; return a; }
static inline thread MD3 &operator*=(thread MD3 &a, MD3 b) { a = a * b; return a; }
static inline thread MD3 &operator*=(thread MD3 &a, MD b) { a = a * b; return a; }
static inline thread MD3 &operator*=(thread MD3 &a, float b) { a = a * MD(b); return a; }
static inline thread MD3 &operator*=(thread MD3 &a, float3 b) { a = a * MD3(b); return a; }
static inline thread MD3 &operator+=(thread MD3 &a, float3 b) { a = a + MD3(b); return a; }
static inline thread MD3 &operator-=(thread MD3 &a, float3 b) { a = a - MD3(b); return a; }
static inline MD dot(MD3 a, MD3 b) { return a.x * b.x + a.y * b.y + a.z * b.z; }
// d|a| = (a / |a|) . da. Normalising first keeps an escaped orbit's 2 * v * d
// (|z| ~ 1e17 with gradients ~ 1e22) from overflowing to inf / inf = NaN.
static inline MD length(MD3 a) {
    float3 v = float3(a.x.v, a.y.v, a.z.v);
    float l = length(v);
    float3 u = l > 0.0f ? v / l : float3(0.0f);
    return MD(l, u.x * a.x.d + u.y * a.y.d + u.z * a.z.d);
}
static inline MD3 abs(MD3 a) { return MD3(abs(a.x), abs(a.y), abs(a.z)); }
static inline MD3 cross(MD3 a, MD3 b) {
    return MD3(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x);
}
static inline MD3 normalize(MD3 a) { return a / length(a); }

struct MD4 {
    MD x, y, z, w;
    MD4() {}
    explicit MD4(MD s) : x(s), y(s), z(s), w(s) {}
    MD4(MD a, MD b, MD c, MD d) : x(a), y(b), z(c), w(d) {}
    MD4(float4 v) : x(v.x), y(v.y), z(v.z), w(v.w) {}
};
static inline MD4 operator+(MD4 a, MD4 b) { return MD4(a.x + b.x, a.y + b.y, a.z + b.z, a.w + b.w); }
static inline MD4 operator-(MD4 a, MD4 b) { return MD4(a.x - b.x, a.y - b.y, a.z - b.z, a.w - b.w); }
static inline MD4 operator*(MD4 a, MD4 b) { return MD4(a.x * b.x, a.y * b.y, a.z * b.z, a.w * b.w); }
static inline MD4 operator/(MD4 a, MD4 b) { return MD4(a.x / b.x, a.y / b.y, a.z / b.z, a.w / b.w); }
static inline MD4 operator*(MD4 a, MD b) { return MD4(a.x * b, a.y * b, a.z * b, a.w * b); }
static inline MD4 operator*(MD b, MD4 a) { return a * b; }
static inline MD4 operator/(MD4 a, MD b) { return MD4(a.x / b, a.y / b, a.z / b, a.w / b); }
static inline MD4 operator-(MD4 a) { return MD4(-a.x, -a.y, -a.z, -a.w); }
static inline MD4 operator*(MD4 a, float b) { return a * MD(b); }
static inline MD4 operator*(float b, MD4 a) { return a * MD(b); }
static inline MD4 operator/(MD4 a, float b) { return a / MD(b); }
static inline MD4 operator*(MD4 a, float4 b) { return a * MD4(b); }
static inline MD4 operator+(MD4 a, float4 b) { return a + MD4(b); }
static inline MD4 operator-(MD4 a, float4 b) { return a - MD4(b); }
static inline thread MD4 &operator+=(thread MD4 &a, MD4 b) { a = a + b; return a; }
static inline thread MD4 &operator-=(thread MD4 &a, MD4 b) { a = a - b; return a; }
static inline thread MD4 &operator*=(thread MD4 &a, MD4 b) { a = a * b; return a; }
static inline thread MD4 &operator*=(thread MD4 &a, MD b) { a = a * b; return a; }
static inline thread MD4 &operator/=(thread MD4 &a, MD b) { a = a / b; return a; }
static inline thread MD4 &operator*=(thread MD4 &a, float b) { a = a * MD(b); return a; }
static inline thread MD4 &operator/=(thread MD4 &a, float b) { a = a / MD(b); return a; }
static inline thread MD4 &operator*=(thread MD4 &a, float4 b) { a = a * MD4(b); return a; }
static inline thread MD4 &operator+=(thread MD4 &a, float4 b) { a = a + MD4(b); return a; }
static inline thread MD4 &operator-=(thread MD4 &a, float4 b) { a = a - MD4(b); return a; }
static inline MD dot(MD4 a, MD4 b) { return a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w; }
static inline MD length(MD4 a) {
    float4 v = float4(a.x.v, a.y.v, a.z.v, a.w.v);
    float l = length(v);
    float4 u = l > 0.0f ? v / l : float4(0.0f);
    return MD(l, u.x * a.x.d + u.y * a.y.d + u.z * a.z.d + u.w * a.w.d);
}
static inline MD4 abs(MD4 a) { return MD4(abs(a.x), abs(a.y), abs(a.z), abs(a.w)); }
static inline MD4 normalize(MD4 a) { return a / length(a); }
static inline bool mdIsNaN(MD4 a) {
    uint4 bits = as_type<uint4>(float4(a.x.v, a.y.v, a.z.v, a.w.v));
    return any(((bits & 0x7f800000u) == 0x7f800000u) & ((bits & 0x007fffffu) != 0u));
}
"#;

/// Dual matrix product, emitted only where the formula declares `matrix33`.
const MATRIX: &str = r#"
static inline MD4 Matrix33MulFloat4(matrix33 matrix, MD4 value) {
    MD3 v = MD3(value.x, value.y, value.z);
    return MD4(dot(v, MD3(matrix.m1)), dot(v, MD3(matrix.m2)), dot(v, MD3(matrix.m3)), value.w);
}
"#;

/// Library plus the dual formula, state and gradient driver for one formula.
pub(super) fn delta_source(
    declarations: &str,
    function_name: &str,
    initial_w: &str,
    actual_scale: &str,
    post_iteration: &str,
) -> Result<String> {
    let matrix = if declarations.contains("struct matrix33") {
        MATRIX
    } else {
        ""
    };
    Ok(format!(
        "{LIBRARY}{matrix}{STATE}\n{}\n{}",
        formula_function(declarations, function_name)?,
        gradient_source(function_name, initial_w, actual_scale, post_iteration)
    ))
}

/// Orbit state with every scalar and vector carried as a dual number.
const STATE: &str = r#"
struct MandelDualOrbitState {
    uint i;
    MD4 c;
    MD4 const_c;
    MD4 old_z;
    MD pos_neg;
    MD r;
    MD DE;
    MD DE0;
    MD dist;
    MD pseudoKleinianDE;
    MD actualScale;
    MD actualScaleA;
    MD color;
    MD colorHybrid;
    MD temp1000;
};
"#;

/// Dual copy of the translated `function_name` from `declarations`: scalar and
/// vector `float` types become dual types except behind `const` (constant
/// parameter pointers), and the orbit state becomes the dual state.
pub(super) fn formula_function(declarations: &str, function_name: &str) -> Result<String> {
    let signature = format!("static float4 {function_name}(");
    let start = declarations
        .find(&signature)
        .with_context(|| format!("dual translation: {function_name} not found"))?;
    let open = start
        + declarations[start..]
            .find('{')
            .context("dual translation: formula body has no opening brace")?;
    let mut depth = 0usize;
    let mut end = None;
    for (offset, ch) in declarations[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(open + offset + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end.context("dual translation: unterminated formula body")?;
    let source = &declarations[start..end];
    let mut output = String::with_capacity(source.len() + 64);
    let mut previous_word = "";
    let mut chars = source.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if ch.is_ascii_alphabetic() || ch == '_' {
            let mut stop = index + ch.len_utf8();
            while let Some(&(next, c)) = chars.peek() {
                if c.is_ascii_alphanumeric() || c == '_' {
                    stop = next + c.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            let word = &source[index..stop];
            let replacement = match word {
                _ if previous_word == "const" => word.to_owned(),
                "float" => "MD".to_owned(),
                "float3" => "MD3".to_owned(),
                "float4" => "MD4".to_owned(),
                "MandelOrbitState" => "MandelDualOrbitState".to_owned(),
                _ if word == function_name => format!("{function_name}Dual"),
                _ => word.to_owned(),
            };
            output.push_str(&replacement);
            previous_word = word;
        } else {
            if !ch.is_whitespace() {
                previous_word = "";
            }
            output.push(ch);
        }
    }
    Ok(output)
}

/// Dual orbit replaying `iterations` steps of the base orbit and returning
/// d|z|/d(scaled point). The seed maps the orbit's (x, z, y) start to the
/// sample's axes, so the gradient is in the sample's own coordinates.
pub(super) fn gradient_source(
    function_name: &str,
    initial_w: &str,
    actual_scale: &str,
    post_iteration: &str,
) -> String {
    format!(
        r#"static float3 mandelDeltaDualGradient(float3 scaled,
                                      constant FptRenderConfig &cfg,
                                      int iterations) {{
    MD4 z = MD4(MD(scaled.x, float3(1.0f, 0.0f, 0.0f)),
                MD(scaled.z, float3(0.0f, 0.0f, 1.0f)),
                MD(scaled.y, float3(0.0f, 1.0f, 0.0f)),
                MD({initial_w}));
    MandelDualOrbitState aux;
    aux.i = 0u;
    aux.c = z;
    aux.const_c = z;
    aux.old_z = z;
    aux.pos_neg = 1.0f;
    aux.r = length(z);
    aux.DE = 1.0f;
    aux.dist = 1000.0f;
    aux.pseudoKleinianDE = 1.0f;
    aux.actualScale = {actual_scale};
    aux.color = 1.0f;
    aux.temp1000 = 1000.0f;
    for (int iteration = 0; iteration < iterations; ++iteration) {{
        aux.i = uint(iteration);
        aux.old_z = z;
        z = {function_name}Dual(z, kMandelFormulaParameters, aux);
{post_iteration}        // As in the base orbit (and Mandelbulber), a NaN step keeps the
        // previous point and ends the orbit.
        if (mdIsNaN(z)) {{
            z = aux.old_z;
            break;
        }}
        aux.r = length(z);
    }}
    return length(z).d;
}}
"#
    )
}

/// Radial derivative using the dual gradient, falling back to the finite
/// differences when global folds (not ported to dual numbers) are active.
pub(super) fn radial_derivative_source(delta_source: &str, finite_difference: &str) -> String {
    let fallback = finite_difference.replace(
        "    float3 radial_derivative = ",
        "    radial_derivative = ",
    );
    format!(
        r#"    float3 radial_derivative;
    if (cfg.vset_values[101] > 0.5f || cfg.vset_values[104] > 0.5f) {{
{delta_source}
{fallback}
    }} else {{
        radial_derivative = abs(mandelDeltaDualGradient(scaled, cfg, base.iterations));
    }}"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dual_translation_keeps_constant_pointers_and_renames_state() {
        let declarations = "static float4 FooIteration(float4 z,\n    constant const MandelFractalParameters &fractal,\n    thread MandelOrbitState &aux) {\n    float a = z.x * 2.0f;\n    constant const float3 *p = &fractal.v;\n    if (a > 0.0f) { z.y = a; }\n    return z;\n}\nstatic float other() { return 1.0f; }";
        let dual = formula_function(declarations, "FooIteration").unwrap();
        assert!(dual.starts_with("static MD4 FooIterationDual(MD4 z,"));
        assert!(dual.contains("thread MandelDualOrbitState &aux"));
        assert!(dual.contains("MD a = z.x * 2.0f;"));
        assert!(dual.contains("constant const float3 *p"));
        assert!(dual.ends_with("return z;\n}"));
        assert!(!dual.contains("other"));
    }

    #[test]
    fn only_checked_formulas_use_dual_gradients() {
        assert!(supports(49));
        assert!(supports(86));
        assert!(!supports(1));
    }
}
