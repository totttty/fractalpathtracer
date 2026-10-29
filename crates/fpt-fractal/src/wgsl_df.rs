//! Double-float WGSL for deep zooms: an unevaluated sum of two f32 (about 48 mantissa bits), after FPT's
//! `double_float.rs`. `DF1 { hi, lo }` is a scalar; `DF2`..`DF4 { hi: vecN, lo: vecN }` are vectors, so component `i`
//! of `v` is `(v.hi[i], v.lo[i])` and dynamic indexing still works. The error-free transforms need IEEE arithmetic:
//! compile without fast-math.

use std::fmt::Write as _;

use super::{AUX_FIELDS, Emitter, Glue, Result, Ty, err};
use crate::ast::{BinaryOp, Expr};
use crate::colouring::Colouring;

/// The scalar core: error-free transforms, arithmetic, comparisons and transcendentals (series on reduced arguments,
/// or one Newton step from the f32 result, which squares its error).
const SCALAR: &str = r#"
struct DF1 { hi: f32, lo: f32, }
struct DF2 { hi: vec2<f32>, lo: vec2<f32>, }
struct DF3 { hi: vec3<f32>, lo: vec3<f32>, }
struct DF4 { hi: vec4<f32>, lo: vec4<f32>, }

fn df1_quick_two_sum(a: f32, b: f32) -> DF1 {
    let s = a + b;
    return DF1(s, b - (s - a));
}
fn df1_two_sum(a: f32, b: f32) -> DF1 {
    let s = a + b;
    let bb = s - a;
    return DF1(s, (a - (s - bb)) + (b - bb));
}
// A sum or product that overflowed, or met an infinity, is f32's `plain` result: its error terms would be NaN.
fn df1_finite(r: DF1, plain: f32) -> DF1 {
    return DF1(select(plain, r.hi, abs(r.hi) <= 3.4028235e38), select(0.0, r.lo, abs(r.hi) <= 3.4028235e38));
}
fn df1_f(x: f32) -> DF1 { return DF1(x, 0.0); }
fn df1_i(x: i32) -> DF1 { return DF1(f32(x), 0.0); }
fn df1_b(x: bool) -> DF1 { return DF1(select(0.0, 1.0, x), 0.0); }
fn df1_to_f(a: DF1) -> f32 { return a.hi + a.lo; }
fn df1_to_i(a: DF1) -> i32 {
    let h = trunc(a.hi);
    if h == a.hi { return i32(h) + i32(trunc(a.lo)); }
    return i32(h);
}
fn df1_nz(a: DF1) -> bool { return a.hi != 0.0 || a.lo != 0.0; }
// Dekker's sum: exact high parts, low parts added in f32 (accurate to 2^-48 of the larger operand).
fn df1_add(a: DF1, b: DF1) -> DF1 {
    let s = df1_two_sum(a.hi, b.hi);
    return df1_finite(df1_quick_two_sum(s.hi, s.lo + (a.lo + b.lo)), a.hi + b.hi);
}
fn df1_neg(a: DF1) -> DF1 { return DF1(-a.hi, -a.lo); }
fn df1_sub(a: DF1, b: DF1) -> DF1 { return df1_add(a, df1_neg(b)); }
fn df1_mul(a: DF1, b: DF1) -> DF1 {
    let p = a.hi * b.hi;
    let e = fma(a.hi, b.hi, -p);
    return df1_finite(df1_quick_two_sum(p, e + (a.hi * b.lo + a.lo * b.hi)), p);
}
fn df1_div(a: DF1, b: DF1) -> DF1 {
    let q1 = a.hi / b.hi;
    let r = df1_sub(a, df1_mul(df1_f(q1), b));
    let q2 = r.hi / b.hi;
    // Division by zero or by infinity, or an overflow: f32's quotient (the refinement would be NaN).
    let finite = abs(q1) <= 3.4028235e38 && abs(b.hi) <= 3.4028235e38;
    return df1_select(df1_f(q1), df1_quick_two_sum(q1, q2), finite);
}
fn df1_lt(a: DF1, b: DF1) -> bool { return a.hi < b.hi || (a.hi == b.hi && a.lo < b.lo); }
fn df1_gt(a: DF1, b: DF1) -> bool { return df1_lt(b, a); }
fn df1_le(a: DF1, b: DF1) -> bool { return !df1_lt(b, a); }
fn df1_ge(a: DF1, b: DF1) -> bool { return !df1_lt(a, b); }
fn df1_eq(a: DF1, b: DF1) -> bool { return a.hi == b.hi && a.lo == b.lo; }
fn df1_ne(a: DF1, b: DF1) -> bool { return !df1_eq(a, b); }
fn df1_select(f: DF1, t: DF1, c: bool) -> DF1 { return DF1(select(f.hi, t.hi, c), select(f.lo, t.lo, c)); }
fn df1_abs(a: DF1) -> DF1 { return df1_select(a, df1_neg(a), df1_lt(a, DF1(0.0, 0.0))); }
fn df1_sign(a: DF1) -> DF1 { return df1_f(select(select(0.0, -1.0, a.hi < 0.0), 1.0, a.hi > 0.0)); }
fn df1_min(a: DF1, b: DF1) -> DF1 { return df1_select(a, b, df1_lt(b, a)); }
fn df1_max(a: DF1, b: DF1) -> DF1 { return df1_select(a, b, df1_lt(a, b)); }
fn df1_clamp(a: DF1, lo: DF1, hi: DF1) -> DF1 { return df1_min(df1_max(a, lo), hi); }
fn df1_sqrt(a: DF1) -> DF1 {
    if a.hi <= 0.0 { return df1_f(sqrt(a.hi)); }
    let q = sqrt(a.hi);
    let p = q * q;
    let e = fma(q, q, -p);
    let r = ((a.hi - p) - e + a.lo) * (0.5 / q);
    return df1_quick_two_sum(q, r);
}
fn df1_rsqrt(a: DF1) -> DF1 { return df1_div(df1_f(1.0), df1_sqrt(a)); }
fn df1_recip(a: DF1) -> DF1 { return df1_div(df1_f(1.0), a); }
fn df1_floor(a: DF1) -> DF1 {
    let h = floor(a.hi);
    if h == a.hi { return df1_quick_two_sum(h, floor(a.lo)); }
    return df1_f(h);
}
fn df1_ceil(a: DF1) -> DF1 { return df1_neg(df1_floor(df1_neg(a))); }
fn df1_trunc(a: DF1) -> DF1 { return df1_select(df1_floor(a), df1_ceil(a), a.hi < 0.0); }
fn df1_round(a: DF1) -> DF1 { return df1_floor(df1_add(a, df1_f(0.5))); }
fn df1_fmod(a: DF1, b: DF1) -> DF1 { return df1_sub(a, df1_mul(b, df1_trunc(df1_div(a, b)))); }
fn df1_copysign(a: DF1, b: DF1) -> DF1 { return df1_select(df1_abs(a), df1_neg(df1_abs(a)), b.hi < 0.0); }
fn df1_mix(a: DF1, b: DF1, t: DF1) -> DF1 { return df1_add(a, df1_mul(df1_sub(b, a), t)); }
fn df1_fma(a: DF1, b: DF1, c: DF1) -> DF1 { return df1_add(df1_mul(a, b), c); }
fn df1_wrap(x: DF1, a: DF1, s: DF1) -> DF1 {
    let shifted = df1_sub(x, s);
    return df1_add(df1_sub(shifted, df1_mul(a, df1_floor(df1_div(shifted, a)))), s);
}
fn df1_sin_series(r: DF1, r2: DF1) -> DF1 {
    var p = DF1(2.811457359e-15, -1.046208474e-22);
    p = df1_sub(DF1(7.647163610e-13, 1.220071047e-20), df1_mul(r2, p));
    p = df1_sub(DF1(1.605904437e-10, -5.352526512e-18), df1_mul(r2, p));
    p = df1_sub(DF1(2.505210794e-08, 4.417623045e-16), df1_mul(r2, p));
    p = df1_sub(DF1(2.755731884e-06, 3.793571224e-14), df1_mul(r2, p));
    p = df1_sub(DF1(1.984127011e-04, -2.725596875e-12), df1_mul(r2, p));
    p = df1_sub(DF1(8.333333768e-03, -4.346172033e-10), df1_mul(r2, p));
    p = df1_sub(DF1(1.666666716e-01, -4.967053879e-09), df1_mul(r2, p));
    return df1_sub(r, df1_mul(df1_mul(r, r2), p));
}
fn df1_cos_series(r2: DF1) -> DF1 {
    var q = DF1(4.779477256e-14, 7.625444044e-22);
    q = df1_sub(DF1(1.147074536e-11, 2.372207689e-19), df1_mul(r2, q));
    q = df1_sub(DF1(2.087675588e-09, 1.108283981e-16), df1_mul(r2, q));
    q = df1_sub(DF1(2.755731998e-07, -7.575112209e-15), df1_mul(r2, q));
    q = df1_sub(DF1(2.480158764e-05, -3.406996094e-13), df1_mul(r2, q));
    q = df1_sub(DF1(1.388888923e-03, -3.363109444e-11), df1_mul(r2, q));
    q = df1_sub(DF1(4.166666791e-02, -1.241763470e-09), df1_mul(r2, q));
    q = df1_sub(DF1(5.000000000e-01, 0.0), df1_mul(r2, q));
    return df1_sub(df1_f(1.0), df1_mul(r2, q));
}
// x less the nearest multiple k of pi/2, and k mod 4. k is a double-float too: past 2^24 an f32 cannot hold
// every integer, and an f32 k left the reduced angle in the thousands, where the series overflow.
struct DFReduced { r: DF1, quadrant: i32, }
fn df1_reduce(x: DF1) -> DFReduced {
    let scaled = df1_mul(x, DF1(6.366197467e-01, 2.568255297e-08));
    let k_hi = round(scaled.hi);
    let k = df1_quick_two_sum(k_hi, round((scaled.hi - k_hi) + scaled.lo));
    let r = df1_sub(x, df1_mul(DF1(1.570796371e+00, -4.371138829e-08), k));
    // Each part's remainder after 4 (exact: k's parts are integers, and dividing by 4 is exact).
    let quadrant = i32(k.hi - 4.0 * floor(k.hi * 0.25)) + i32(k.lo - 4.0 * floor(k.lo * 0.25));
    return DFReduced(r, quadrant & 3);
}
// (sin x, cos x) as DF2's hi.x/lo.x and hi.y/lo.y.
fn df1_sincos(x: DF1) -> DF2 {
    let reduced = df1_reduce(x);
    let r = reduced.r;
    let r2 = df1_mul(r, r);
    let sr = df1_sin_series(r, r2);
    let cr = df1_cos_series(r2);
    let quadrant = reduced.quadrant;
    var s = sr;
    var c = cr;
    if quadrant == 1 { s = cr; c = df1_neg(sr); }
    if quadrant == 2 { s = df1_neg(sr); c = df1_neg(cr); }
    if quadrant == 3 { s = df1_neg(cr); c = sr; }
    return DF2(vec2(s.hi, c.hi), vec2(s.lo, c.lo));
}
// sin and cos alone evaluate only the series their quadrant needs.
fn df1_sin(x: DF1) -> DF1 {
    let reduced = df1_reduce(x);
    let r = reduced.r;
    let r2 = df1_mul(r, r);
    let quadrant = reduced.quadrant;
    var v: DF1;
    if (quadrant & 1) == 0 { v = df1_sin_series(r, r2); } else { v = df1_cos_series(r2); }
    return df1_select(v, df1_neg(v), quadrant >= 2);
}
fn df1_cos(x: DF1) -> DF1 {
    let reduced = df1_reduce(x);
    let r = reduced.r;
    let r2 = df1_mul(r, r);
    let quadrant = reduced.quadrant;
    var v: DF1;
    if (quadrant & 1) == 0 { v = df1_cos_series(r2); } else { v = df1_sin_series(r, r2); }
    return df1_select(v, df1_neg(v), quadrant == 1 || quadrant == 2);
}
fn df1_tan(x: DF1) -> DF1 { let sc = df1_sincos(x); return df1_div(DF1(sc.hi.x, sc.lo.x), DF1(sc.hi.y, sc.lo.y)); }
fn df1_atan2(y: DF1, x: DF1) -> DF1 {
    if x.hi == 0.0 && y.hi == 0.0 { return df1_f(atan2(y.hi, x.hi)); }
    let a0 = atan2(y.hi, x.hi);
    let sc = df1_sincos(df1_f(a0));
    let s = DF1(sc.hi.x, sc.lo.x);
    let c = DF1(sc.hi.y, sc.lo.y);
    return df1_add(df1_f(a0), df1_div(df1_sub(df1_mul(y, c), df1_mul(x, s)), df1_add(df1_mul(x, c), df1_mul(y, s))));
}
fn df1_atan(x: DF1) -> DF1 { return df1_atan2(x, df1_f(1.0)); }
fn df1_asin(v: DF1) -> DF1 {
    if !(abs(v.hi) <= 1.0 + 1.0e-5) { return df1_f(asin(v.hi)); }
    let c = df1_clamp(v, df1_f(-1.0), df1_f(1.0));
    return df1_atan2(c, df1_sqrt(df1_mul(df1_sub(df1_f(1.0), c), df1_add(df1_f(1.0), c))));
}
fn df1_acos(v: DF1) -> DF1 {
    if !(abs(v.hi) <= 1.0 + 1.0e-5) { return df1_f(acos(v.hi)); }
    let c = df1_clamp(v, df1_f(-1.0), df1_f(1.0));
    return df1_atan2(df1_sqrt(df1_mul(df1_sub(df1_f(1.0), c), df1_add(df1_f(1.0), c))), c);
}
fn df1_exp(x: DF1) -> DF1 {
    if !(abs(x.hi) < 87.0) { return df1_f(exp(x.hi)); }
    let k = round(x.hi * 1.442695041);
    // e^r - 1 on r / 512, then squared back nine times as s(s + 2).
    let r = df1_mul(df1_sub(x, df1_mul(DF1(6.931471825e-01, -1.904654212e-09), df1_f(k))), df1_f(1.0 / 512.0));
    var p = DF1(2.480158764e-05, -3.406996094e-13);
    p = df1_add(DF1(1.984127011e-04, -2.725596875e-12), df1_mul(r, p));
    p = df1_add(DF1(1.388888923e-03, -3.363109444e-11), df1_mul(r, p));
    p = df1_add(DF1(8.333333768e-03, -4.346172033e-10), df1_mul(r, p));
    p = df1_add(DF1(4.166666791e-02, -1.241763470e-09), df1_mul(r, p));
    p = df1_add(DF1(1.666666716e-01, -4.967053879e-09), df1_mul(r, p));
    p = df1_add(DF1(5.000000000e-01, 0.0), df1_mul(r, p));
    var s = df1_add(r, df1_mul(df1_mul(r, r), p));
    for (var i = 0; i < 9; i++) { s = df1_mul(s, df1_add(s, df1_f(2.0))); }
    let e = df1_add(s, df1_f(1.0));
    return DF1(ldexp(e.hi, i32(k)), ldexp(e.lo, i32(k)));
}
fn df1_log(a: DF1) -> DF1 {
    if !(a.hi > 0.0) { return df1_f(log(a.hi)); }
    let l = df1_f(log(a.hi));
    return df1_sub(df1_add(l, df1_mul(a, df1_exp(df1_neg(l)))), df1_f(1.0));
}
fn df1_pow(a: DF1, p: DF1) -> DF1 {
    if !(a.hi > 0.0) { return df1_f(pow(a.hi, p.hi)); }
    if p.lo == 0.0 && p.hi == round(p.hi) && abs(p.hi) <= 64.0 {
        var n = i32(abs(p.hi));
        var result = df1_f(1.0);
        var base = a;
        while n > 0 {
            if (n & 1) != 0 { result = df1_mul(result, base); }
            base = df1_mul(base, base);
            n = n >> 1u;
        }
        return df1_select(result, df1_recip(result), p.hi < 0.0);
    }
    return df1_exp(df1_mul(df1_log(a), p));
}
fn df1_exp2(x: DF1) -> DF1 { return df1_exp(df1_mul(x, DF1(6.931471825e-01, -1.904654212e-09))); }
fn df1_log2(x: DF1) -> DF1 { return df1_mul(df1_log(x), DF1(1.442695022e+00, -1.925963033e-08)); }
fn df1_log10(x: DF1) -> DF1 { return df1_mul(df1_log(x), DF1(4.342944920e-01, -1.010305e-08)); }
fn df1_sinh(x: DF1) -> DF1 { let e = df1_exp(x); return df1_mul(df1_sub(e, df1_recip(e)), df1_f(0.5)); }
fn df1_cosh(x: DF1) -> DF1 { let e = df1_exp(x); return df1_mul(df1_add(e, df1_recip(e)), df1_f(0.5)); }
fn df1_tanh(x: DF1) -> DF1 { let e = df1_exp(df1_mul(x, df1_f(2.0))); return df1_div(df1_sub(e, df1_f(1.0)), df1_add(e, df1_f(1.0))); }
"#;

const UNARY: [&str; 24] = [
    "neg", "abs", "sign", "sqrt", "rsqrt", "recip", "floor", "ceil", "trunc", "round", "sin",
    "cos", "tan", "asin", "acos", "atan", "exp", "exp2", "log", "log2", "log10", "sinh", "cosh",
    "tanh",
];
const BINARY: [&str; 10] = [
    "add", "sub", "mul", "div", "min", "max", "atan2", "pow", "fmod", "copysign",
];
const TERNARY: [&str; 4] = ["clamp", "mix", "fma", "wrap"];

/// The double-float library: the scalar core, then `DFn` vectors with the same functions component-wise.
pub(crate) fn library() -> String {
    let mut out = String::from(SCALAR);
    let swz = ["x", "y", "z", "w"];
    for n in 2..=4_usize {
        let t = format!("vec{n}<f32>");
        let d = format!("DF{n}");
        let comps = |f: &dyn Fn(usize) -> String| (0..n).map(f).collect::<Vec<_>>().join(", ");
        let _ = writeln!(
            out,
            "fn df{n}_get(v: {d}, i: i32) -> DF1 {{ return DF1(v.hi[i], v.lo[i]); }}"
        );
        let args = comps(&|i| format!("{}: DF1", swz[i]));
        let his = comps(&|i| format!("{}.hi", swz[i]));
        let los = comps(&|i| format!("{}.lo", swz[i]));
        let _ = writeln!(
            out,
            "fn df{n}_make({args}) -> {d} {{ return {d}({t}({his}), {t}({los})); }}"
        );
        let _ = writeln!(
            out,
            "fn df{n}_splat(s: DF1) -> {d} {{ return {d}({t}(s.hi), {t}(s.lo)); }}"
        );
        let _ = writeln!(
            out,
            "fn df{n}_f(v: {t}) -> {d} {{ return {d}(v, {t}(0.0)); }}"
        );
        let _ = writeln!(
            out,
            "fn df{n}_to_f(v: {d}) -> {t} {{ return v.hi + v.lo; }}"
        );
        let _ = writeln!(
            out,
            "fn df{n}_select(f: {d}, t: {d}, c: bool) -> {d} {{ return {d}(select(f.hi, t.hi, c), select(f.lo, t.lo, c)); }}"
        );
        for name in UNARY {
            let body = comps(&|i| format!("df1_{name}(df{n}_get(a, {i}))"));
            let _ = writeln!(
                out,
                "fn df{n}_{name}(a: {d}) -> {d} {{ return df{n}_make({body}); }}"
            );
        }
        for name in BINARY {
            let body = comps(&|i| format!("df1_{name}(df{n}_get(a, {i}), df{n}_get(b, {i}))"));
            let _ = writeln!(
                out,
                "fn df{n}_{name}(a: {d}, b: {d}) -> {d} {{ return df{n}_make({body}); }}"
            );
        }
        for name in TERNARY {
            let body = comps(&|i| {
                format!("df1_{name}(df{n}_get(a, {i}), df{n}_get(b, {i}), df{n}_get(c, {i}))")
            });
            let _ = writeln!(
                out,
                "fn df{n}_{name}(a: {d}, b: {d}, c: {d}) -> {d} {{ return df{n}_make({body}); }}"
            );
        }
        let terms = (0..n)
            .map(|i| format!("df1_mul(df{n}_get(a, {i}), df{n}_get(b, {i}))"))
            .reduce(|acc, t| format!("df1_add({acc}, {t})"))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "fn df{n}_dot(a: {d}, b: {d}) -> DF1 {{ return {terms}; }}"
        );
        let _ = writeln!(
            out,
            "fn df{n}_length(a: {d}) -> DF1 {{ return df1_sqrt(df{n}_dot(a, a)); }}"
        );
        let _ = writeln!(
            out,
            "fn df{n}_distance(a: {d}, b: {d}) -> DF1 {{ return df{n}_length(df{n}_sub(a, b)); }}"
        );
        let _ = writeln!(
            out,
            "fn df{n}_normalize(a: {d}) -> {d} {{ return df{n}_mul(a, df{n}_splat(df1_recip(df{n}_length(a)))); }}"
        );
        // Narrowing and widening: the first `m` components, or zeros after them.
        for m in 1..=4_usize {
            if m == n {
                continue;
            }
            let target = if m == 1 {
                "DF1".to_owned()
            } else {
                format!("DF{m}")
            };
            let body = if m == 1 {
                format!("df{n}_get(v, 0)")
            } else {
                let parts = (0..m)
                    .map(|i| {
                        if i < n {
                            format!("df{n}_get(v, {i})")
                        } else {
                            "DF1(0.0, 0.0)".into()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("df{m}_make({parts})")
            };
            let _ = writeln!(
                out,
                "fn df{n}_to{m}(v: DF{n}) -> {target} {{ return {body}; }}"
            );
        }
    }
    out.push_str(
        r#"
fn df3_cross(a: DF3, b: DF3) -> DF3 {
    let ax = df3_get(a, 0); let ay = df3_get(a, 1); let az = df3_get(a, 2);
    let bx = df3_get(b, 0); let by = df3_get(b, 1); let bz = df3_get(b, 2);
    return df3_make(df1_sub(df1_mul(ay, bz), df1_mul(az, by)), df1_sub(df1_mul(az, bx), df1_mul(ax, bz)),
        df1_sub(df1_mul(ax, by), df1_mul(ay, bx)));
}
fn df4_cross(a: DF4, b: DF4) -> DF4 { return df3_to4(df3_cross(df4_to3(a), df4_to3(b))); }
struct DFM3 { hi: mat3x3<f32>, lo: mat3x3<f32>, }
// Rows-times-vector of a matrix holding rows as columns (CRotationMatrix::RotateVector).
fn df3_mat_rows(m: DFM3, v: DF3) -> DF3 {
    return df3_make(df3_dot(DF3(m.hi[0], m.lo[0]), v), df3_dot(DF3(m.hi[1], m.lo[1]), v),
        df3_dot(DF3(m.hi[2], m.lo[2]), v));
}
fn df4_mat_rows(m: DFM3, v: DF4) -> DF4 {
    let r = df3_mat_rows(m, df4_to3(v));
    return df4_make(df3_get(r, 0), df3_get(r, 1), df3_get(r, 2), df4_get(v, 3));
}
fn df4_is_nan(v: DF4) -> bool { return any(v.hi != v.hi); }
// Mandelbulber's SmoothCVector: blends direction and length apart.
fn df4_smooth_vector(a: DF4, b: DF4, k: DF1) -> DF4 {
    if k.hi <= 0.0 { return a; }
    if k.hi >= 1.0 { return b; }
    let nk = df1_sub(df1_f(1.0), k);
    let blend = df4_add(df4_mul(a, df4_splat(nk)), df4_mul(b, df4_splat(k)));
    let blend_length = df4_length(blend);
    if blend_length.hi <= 0.0 { return a; }
    let length = df1_add(df1_mul(df4_length(a), nk), df1_mul(df4_length(b), k));
    return df4_mul(blend, df4_splat(df1_div(length, blend_length)));
}
fn df3_rotate_around(o: DF3, k: DF3, angle: DF1) -> DF3 {
    let sc = df1_sincos(angle);
    let s = DF1(sc.hi.x, sc.lo.x);
    let c = DF1(sc.hi.y, sc.lo.y);
    let a = df3_mul(o, df3_splat(c));
    let b = df3_mul(df3_cross(k, o), df3_splat(s));
    let d = df3_mul(k, df3_splat(df1_mul(df3_dot(k, o), df1_sub(df1_f(1.0), c))));
    return df3_add(df3_add(a, b), d);
}
fn df4_rotate_around4(o: DF4, k: DF3, angle: DF1) -> DF4 {
    let r = df3_rotate_around(df4_to3(o), k, angle);
    return df4_make(df3_get(r, 0), df3_get(r, 1), df3_get(r, 2), df4_get(o, 3));
}
fn df1_smooth_greater(a: DF1, b: DF1, k: DF1) -> DF1 {
    return df1_recip(df1_add(df1_f(1.0), df1_exp(df1_mul(k, df1_sub(b, a)))));
}
fn df1_smooth_less(a: DF1, b: DF1, k: DF1) -> DF1 {
    return df1_recip(df1_add(df1_f(1.0), df1_exp(df1_mul(k, df1_sub(a, b)))));
}
"#,
    );
    out
}

/// Declarations every deep module shares: the library, then the orbit's types in double-float.
pub(super) fn prelude(out: &mut String) {
    out.push_str("// Generated by fpt-fractal from Mandelbulber's formula sources (deep zoom: double-float).\n");
    out.push_str(&library());
    // f32 helpers the scene-level code (primitives) shares with f32 modules.
    out.push_str(crate::boolean::WGSL_HELPERS);
    out.push_str("struct FractalAux {\n");
    for (name, ty) in &AUX_FIELDS {
        let ty = match ty {
            Ty::F32 => "DF1".to_owned(),
            Ty::Vec(n) => format!("DF{n}"),
            other => other.wgsl(),
        };
        let _ = writeln!(out, "    {name}: {ty},");
    }
    out.push_str(
        "}\n\n// The fields the distance estimates read, back in f32.\nstruct FractalAuxF { r: f32, DE: f32, pseudoKleinianDE: f32, dist: f32, }\n\n",
    );
    out.push_str(
        "struct FractalOrbit {\n    z: DF4,\n    aux: FractalAux,\n    iterations: i32,\n    formula: i32,\n    empty: bool,\n    escaped: bool,\n}\n\n",
    );
}

/// `value` as a double-float literal.
pub(super) fn literal(value: f64) -> String {
    let hi = value as f32;
    let lo = (value - f64::from(hi)) as f32;
    format!("DF1({}, {})", f32_literal(hi), f32_literal(lo))
}

fn f32_literal(value: f32) -> String {
    if value.is_finite() && value == value.trunc() && value.abs() < 1e7 {
        format!("{value:.1}")
    } else {
        format!("{value:e}")
    }
}

/// Reads a `ty` value from buffer slot `k` and its remainders after it (`Emitter::push_value`).
pub(super) fn buffer_code(k: &str, ty: &Ty) -> String {
    let row = |i: usize| format!("fractal_params[{k} + {i}].xyz");
    match ty {
        Ty::F32 => format!("DF1(fractal_params[{k}].x, fractal_params[{k} + 1].x)"),
        Ty::Vec(2) => format!("DF2(fractal_params[{k}].xy, fractal_params[{k} + 1].xy)"),
        Ty::Vec(3) => format!("DF3(fractal_params[{k}].xyz, fractal_params[{k} + 1].xyz)"),
        Ty::Mat3 => format!(
            "DFM3(mat3x3<f32>({}, {}, {}), mat3x3<f32>({}, {}, {}))",
            row(0),
            row(1),
            row(2),
            row(3),
            row(4),
            row(5)
        ),
        _ => format!("DF4(fractal_params[{k}], fractal_params[{k} + 1])"),
    }
}

/// The library prefix of an F32 or vector type.
fn df(ty: &Ty) -> String {
    match ty {
        Ty::Vec(n) => format!("df{n}"),
        _ => "df1".into(),
    }
}

impl Emitter<'_> {
    /// A `vec{n}` built from `parts` (code, width).
    pub(super) fn deep_vector(&mut self, n: u8, parts: &[(String, u8)]) -> String {
        if let [(scalar, 1)] = parts {
            return format!("df{n}_splat({scalar})");
        }
        if parts.len() == usize::from(n) && parts.iter().all(|(_, width)| *width == 1) {
            let codes: Vec<&str> = parts.iter().map(|(code, _)| code.as_str()).collect();
            return format!("df{n}_make({})", codes.join(", "));
        }
        let signature: Vec<String> = parts.iter().map(|(_, width)| width.to_string()).collect();
        let name = format!("df{n}_cat_{}", signature.join("_"));
        self.helper(&name, || {
            let args: Vec<String> = parts
                .iter()
                .enumerate()
                .map(|(i, (_, width))| format!("a{i}: DF{width}"))
                .collect();
            let half = |field: &str| {
                (0..parts.len())
                    .map(|i| format!("a{i}.{field}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            format!(
                "fn {name}({}) -> DF{n} {{ return DF{n}(vec{n}<f32>({}), vec{n}<f32>({})); }}",
                args.join(", "),
                half("hi"),
                half("lo")
            )
        });
        let codes: Vec<&str> = parts.iter().map(|(code, _)| code.as_str()).collect();
        format!("{name}({})", codes.join(", "))
    }

    /// Arithmetic and comparisons on double-float operands.
    pub(super) fn deep_binary(
        &mut self,
        op: BinaryOp,
        lhs: &Expr,
        rhs: &Expr,
        (lt, rt): (&Ty, &Ty),
        operand: &Ty,
    ) -> Result<(String, Ty)> {
        use BinaryOp::*;
        let name = match op {
            Add => "add",
            Sub => "sub",
            Mul => "mul",
            Div => "div",
            Rem => "fmod",
            Eq => "eq",
            Ne => "ne",
            Lt => "lt",
            Le => "le",
            Gt => "gt",
            Ge => "ge",
            other => return err(format!("{other:?} on floats in a deep zoom")),
        };
        if matches!(op, Eq | Ne | Lt | Le | Gt | Ge) {
            if *operand != Ty::F32 {
                return err("vector comparison in a deep zoom");
            }
            let (a, b) = (self.coerced(lhs, &Ty::F32)?, self.coerced(rhs, &Ty::F32)?);
            return Ok((format!("df1_{name}({a}, {b})"), Ty::Bool));
        }
        let (a, b) = match (lt, rt) {
            (Ty::Vec(n), r) if r.is_scalar() => (
                self.coerced(lhs, lt)?,
                format!("df{n}_splat({})", self.coerced(rhs, &Ty::F32)?),
            ),
            (l, Ty::Vec(n)) if l.is_scalar() => (
                format!("df{n}_splat({})", self.coerced(lhs, &Ty::F32)?),
                self.coerced(rhs, rt)?,
            ),
            _ => (self.coerced(lhs, operand)?, self.coerced(rhs, operand)?),
        };
        Ok((format!("{}_{name}({a}, {b})", df(operand)), operand.clone()))
    }

    /// OpenCL builtins and Mandelbulber helpers on double-floats.
    pub(super) fn deep_call(&mut self, name: &str, args: &[Expr]) -> Result<(String, Ty)> {
        let apply = |emitter: &mut Self, function: &str, count: usize| -> Result<(String, Ty)> {
            let (codes, ty) = emitter.float_args(name, args, count)?;
            Ok((format!("{}_{function}({})", df(&ty), codes.join(", ")), ty))
        };
        Ok(match name {
            "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh" | "tanh" | "exp"
            | "exp2" | "log" | "log2" | "log10" | "sqrt" | "rsqrt" | "recip" | "floor" | "ceil"
            | "round" | "trunc" | "sign" | "abs" | "normalize" => apply(self, name, 1)?,
            "fabs" => apply(self, "abs", 1)?,
            "atan2" | "pow" | "min" | "max" | "fmod" | "copysign" => apply(self, name, 2)?,
            "powr" => apply(self, "pow", 2)?,
            "fmin" => apply(self, "min", 2)?,
            "fmax" => apply(self, "max", 2)?,
            "wrap" | "clamp" => apply(self, name, 3)?,
            "mad" => apply(self, "fma", 3)?,
            "mix" => {
                let (codes, ty) = self.float_args(name, args, 2)?;
                let (_, t_ty) = self.expr(&args[2])?;
                let t = match (&ty, &t_ty) {
                    (Ty::Vec(n), Ty::Vec(_)) => self.coerced(&args[2], &Ty::Vec(*n))?,
                    (Ty::Vec(n), _) => {
                        format!("df{n}_splat({})", self.coerced(&args[2], &Ty::F32)?)
                    }
                    _ => self.coerced(&args[2], &Ty::F32)?,
                };
                (
                    format!("{}_mix({}, {}, {t})", df(&ty), codes[0], codes[1]),
                    ty,
                )
            }
            "dot" | "distance" | "length" => {
                let count = if name == "length" { 1 } else { 2 };
                let (codes, ty) = self.float_args(name, args, count)?;
                let code = match (name, ty.is_scalar()) {
                    ("dot", true) => format!("df1_mul({}, {})", codes[0], codes[1]),
                    ("distance", true) => format!("df1_abs(df1_sub({}, {}))", codes[0], codes[1]),
                    ("length", true) => format!("df1_abs({})", codes[0]),
                    _ => format!("{}_{name}({})", df(&ty), codes.join(", ")),
                };
                (code, Ty::F32)
            }
            "SmoothConditionAGreaterB" | "SmoothConditionALessB" => {
                let a = self.coerced(&args[0], &Ty::F32)?;
                let b = self.coerced(&args[1], &Ty::F32)?;
                let k = self.coerced(&args[2], &Ty::F32)?;
                let function = if name.ends_with("GreaterB") {
                    "greater"
                } else {
                    "less"
                };
                (format!("df1_smooth_{function}({a}, {b}, {k})"), Ty::F32)
            }
            "RotateAroundVectorByAngle" | "RotateAroundVectorByAngle4" => {
                let four = name.ends_with('4');
                let origin = self.coerced(&args[0], &Ty::Vec(if four { 4 } else { 3 }))?;
                let axis = self.coerced(&args[1], &Ty::Vec(3))?;
                let angle = self.coerced(&args[2], &Ty::F32)?;
                if four {
                    (
                        format!("df4_rotate_around4({origin}, {axis}, {angle})"),
                        Ty::Vec(4),
                    )
                } else {
                    (
                        format!("df3_rotate_around({origin}, {axis}, {angle})"),
                        Ty::Vec(3),
                    )
                }
            }
            "cross" => {
                let (codes, ty) = self.float_args(name, args, 2)?;
                let function = if ty == Ty::Vec(4) {
                    "df4_cross"
                } else {
                    "df3_cross"
                };
                (format!("{function}({}, {})", codes[0], codes[1]), ty)
            }
            "Matrix33MulFloat4" | "Matrix33MulFloat3" => {
                let matrix = self.coerced(&args[0], &Ty::Mat3)?;
                let (vector, ty) = self.expr(&args[1])?;
                let function = if ty == Ty::Vec(4) {
                    "df4_mat_rows"
                } else {
                    "df3_mat_rows"
                };
                (format!("{function}({matrix}, {vector})"), ty)
            }
            other => return err(format!("unsupported function {other}")),
        })
    }

    /// The deep orbit and `{prefix}distance`: the f32 glue (`Emitter::distance`) with a double-float orbit from
    /// `origin + p`. The estimate itself is f32: only the orbit needs the precision.
    pub(super) fn deep_distance(&mut self, julia_slot: usize) -> Result<()> {
        let fractal = self.fractal;
        let origin = self.deep.expect("deep zoom");
        self.current = 0;
        if fractal.box_fold.is_some()
            || fractal.spherical_fold.is_some()
            || fractal.repeat != [0.0; 3]
        {
            return err("deep zoom does not support legacy folds or repeats");
        }
        let actual_scale = match self.params().values.get("mandelbox.scale") {
            Some(_) => {
                {
                    let slot = self.slot("mandelbox.scale")?;
                    self.slot_code(&slot)
                }
                .0
            }
            None => "df1_f(2.0)".into(),
        };
        let glue = self.glue_slots();
        let linear_offset = glue.linear_offset;
        let origin_slot = self.push_value([origin[0], origin[1], origin[2], 0.0]);
        self.deep_orbit_function(None, &glue, julia_slot, &actual_scale, origin_slot)?;
        if let Some(colouring) = self.colouring.clone() {
            self.deep_orbit_function(
                Some(&colouring),
                &glue,
                julia_slot,
                &actual_scale,
                origin_slot,
            )?;
        }
        let prefix = self.prefix.clone();
        self.line(format!(
            "fn {prefix}distance(p_camera: vec3<f32>, detail_size: f32) -> f32 {{"
        ));
        self.indent += 1;
        self.line(format!(
            "let p = df3_add(DF3(fractal_params[{origin_slot}].xyz, fractal_params[{}].xyz), df3_f(p_camera));",
            origin_slot + 1
        ));
        self.line(format!("let orbit = {prefix}orbit(p, -1);"));
        self.line("if (orbit.empty) { return 10.0; }");
        self.line("let z = df4_to_f(orbit.z);");
        if fractal.delta {
            self.deep_delta_distance();
        } else {
            self.line(
                "let aux = FractalAuxF(df1_to_f(orbit.aux.r), df1_to_f(orbit.aux.DE), df1_to_f(orbit.aux.pseudoKleinianDE), df1_to_f(orbit.aux.dist));",
            );
            self.analytic_distance(linear_offset)?;
        }
        self.distance_rules();
        Ok(())
    }

    /// The deep orbit function: `{prefix}orbit(p, forced)` on a double-float `p`, or with `colouring` the colouring
    /// orbit `{prefix}colour_index(p_camera)`.
    fn deep_orbit_function(
        &mut self,
        colouring: Option<&Colouring>,
        glue: &Glue,
        julia_slot: usize,
        actual_scale: &str,
        origin_slot: usize,
    ) -> Result<()> {
        let fractal = self.fractal;
        let (prefix, header) = (self.prefix.clone(), self.header);
        let sequence_length = fractal.sequence.len().max(1);
        let Glue {
            sequence_slot,
            ref weights,
            transform,
            ..
        } = *glue;
        if colouring.is_some() {
            // calcModeColouring: four times the iterations, Mandelbulber's own exits and an orbit trap.
            self.line(format!(
                "fn {prefix}colour_index(p_camera: vec3<f32>) -> f32 {{"
            ));
            self.indent += 1;
            self.line(format!(
                "let p = df3_add(DF3(fractal_params[{origin_slot}].xyz, fractal_params[{}].xyz), df3_f(p_camera));",
                origin_slot + 1
            ));
            self.line(format!("let header = fractal_params[{header}];"));
            self.line("let max_iterations = i32(header.x) * 4;");
            self.line("let forced = -1;");
        } else {
            self.line("// `forced` >= 0 runs exactly that many iterations with no bailout,");
            self.line("// as Delta-DE's probes do.");
            self.line(format!(
                "fn {prefix}orbit(p: DF3, forced: i32) -> FractalOrbit {{"
            ));
            self.indent += 1;
            self.line(format!("let header = fractal_params[{header}];"));
            self.line("let max_iterations = select(i32(header.x), forced, forced >= 0);");
        }
        self.line("let bailout = df1_f(header.y);");
        self.line(format!(
            "let position = DF3(fractal_params[{transform}].xyz, fractal_params[{}].xyz);",
            transform + 5
        ));
        let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        if fractal.rotation == identity {
            self.line("let q = df3_sub(p, position);");
        } else {
            self.line(format!(
                "let rotation = DFM3(mat3x3<f32>(fractal_params[{}].xyz, fractal_params[{}].xyz, fractal_params[{}].xyz), mat3x3<f32>());",
                transform + 1,
                transform + 2,
                transform + 3
            ));
            self.line("let q = df3_mat_rows(rotation, df3_sub(p, position));");
        }
        self.line("var z = DF4(vec4<f32>(q.hi, header.z), vec4<f32>(q.lo, 0.0));");
        if colouring.is_some() {
            self.line("let initial = df4_to_f(z);");
            self.line("var colour_min = 1000.0;");
        }
        self.line("var aux: FractalAux;");
        for (field, value) in [
            ("i", "0".to_owned()),
            ("c", "z".into()),
            ("const_c", "z".into()),
            ("old_z", "z".into()),
            ("pos_neg", "df1_f(1.0)".into()),
            ("r", "df4_length(z)".into()),
            ("DE", "df1_f(1.0)".into()),
            ("DE0", "df1_f(0.0)".into()),
            ("dist", "df1_f(1000.0)".into()),
            ("pseudoKleinianDE", "df1_f(1.0)".into()),
            ("actualScale", actual_scale.to_owned()),
            ("actualScaleA", "df1_f(0.0)".into()),
            ("color", "df1_f(1.0)".into()),
            ("colorHybrid", "df1_f(0.0)".into()),
            ("temp1000", "df1_f(1000.0)".into()),
        ] {
            self.line(format!("aux.{field} = {value};"));
        }
        self.line("var iterations = 0;");
        self.line("var formula = 0;");
        self.line("var last_z = z;");
        self.line("var done = false;");
        self.line("var escaped = false;");
        self.line("for (var i = 0; i < max_iterations; i++) {");
        self.indent += 1;
        self.line("iterations = i + 1;");
        self.line("let last_last_z = last_z;");
        self.line("last_z = z;");
        self.line("aux.i = i;");
        if fractal.hybrid {
            self.line("let temp_z = z;");
            self.line(format!(
                "let step = u32(min(i, {}));\n        formula = i32(fractal_params[{sequence_slot}u + step / 4u][step % 4u]);",
                sequence_length - 1
            ));
            if colouring.is_some() {
                self.line("if (formula < 0) { return 0.0; }");
            } else {
                self.line("if (formula < 0) { return FractalOrbit(z, aux, 1, 0, true, false); }");
            }
            self.line("let previous_de = aux.DE;");
            self.line("let previous_color = aux.color;");
            self.line("switch formula {");
        }
        let m = format!("fractal_params[{}].xyz", header + 1);
        let julia = format!(
            "DF3(fractal_params[{julia_slot}].xyz, fractal_params[{}].xyz)",
            julia_slot + 1
        );
        for (k, formula) in fractal.formulas.iter().enumerate() {
            let weight = format!("fractal_params[{}].x", weights[k]);
            if fractal.hybrid {
                let label = if k + 1 == fractal.formulas.len() {
                    "default".to_owned()
                } else {
                    format!("case {k}")
                };
                self.line(format!("{label}: {{"));
                self.indent += 1;
                self.line(format!(
                    "if ({weight} > 0.0) {{ z = {prefix}formula_{k}(z, &aux); }}"
                ));
            } else {
                self.line(format!("z = {prefix}formula_{k}(z, &aux);"));
            }
            if formula.add_c {
                let swapped = matches!(
                    formula.meta.internal_name.as_str(),
                    "abox_mod1" | "amazing_surf"
                );
                let add = match (fractal.julia.is_some(), swapped) {
                    (true, false) => format!("df3_to4(df3_mul({julia}, df3_f({m})))"),
                    (true, true) => format!("df3_to4(df3_mul(df3_s_yxz({julia}), df3_f({m}.yxz)))"),
                    (false, false) => format!("df4_mul(aux.const_c, df4_f(vec4<f32>({m}, 1.0)))"),
                    (false, true) => {
                        format!("df3_to4(df3_mul(df4_s_yxz(aux.const_c), df3_f({m})))")
                    }
                };
                for (name, n) in [("df3_s_yxz", 3), ("df4_s_yxz", 4)] {
                    self.helper(name, || {
                        format!("fn {name}(v: DF{n}) -> DF3 {{ return DF3(v.hi.yxz, v.lo.yxz); }}")
                    });
                }
                self.line(format!("z = df4_add(z, {add});"));
            }
            if fractal.hybrid {
                self.line(format!("if ({weight} < 1.0) {{"));
                self.line(format!(
                    "    z = df4_smooth_vector(temp_z, z, df1_f({weight}));"
                ));
                self.line(format!(
                    "    aux.DE = df1_add(df1_mul(aux.DE, df1_f({weight})), df1_mul(previous_de, df1_f(1.0 - {weight})));"
                ));
                self.line(format!(
                    "    aux.color = df1_add(df1_mul(aux.color, df1_f({weight})), df1_mul(previous_color, df1_f(1.0 - {weight})));"
                ));
                self.line("}");
            }
            let exit = if fractal.hybrid {
                "done = true;"
            } else {
                "break;"
            };
            self.line("aux.r = df4_length(z);");
            self.line("if (df4_is_nan(z)) {");
            self.line("    z = last_z;");
            self.line("    aux.r = df4_length(z);");
            self.line(format!("    {exit}"));
            self.line("}");
            if let (Some(colouring), true) = (colouring, formula.check_bailout) {
                let mandelbox = formula.meta.internal_name == "mandelbox";
                self.colour_trap(colouring, mandelbox, formula.additional_bailout, exit);
            } else if formula.check_bailout {
                self.line("if (!done && forced < 0) {");
                self.line(format!(
                    "    if (df1_gt(aux.r, bailout)) {{ escaped = true; {exit} }}"
                ));
                if formula.additional_bailout {
                    self.line("    escaped = true;");
                    self.line("    let r = df1_to_f(aux.r);");
                    self.line(format!(
                        "    if (df1_to_f(df4_length(df4_sub(z, last_z))) / r < 0.1 / header.y) {{ {exit} }}"
                    ));
                    self.line(format!(
                        "    if (df1_to_f(df4_length(df4_sub(z, last_last_z))) / r < 0.1 / header.y) {{ {exit} }}"
                    ));
                }
                self.line("}");
            }
            if fractal.hybrid {
                self.indent -= 1;
                self.line("}");
            }
        }
        if fractal.hybrid {
            self.line("}");
            self.line("if (done) { break; }");
        }
        self.indent -= 1;
        self.line("}");
        if let Some(colouring) = colouring {
            self.line("let cz = df4_to_f(z);");
            self.line("let cr = df1_to_f(aux.r);");
            self.line("let cde = df1_to_f(aux.DE);");
            self.line("let ccolour = df1_to_f(aux.color);");
            self.line("let chybrid = df1_to_f(aux.colorHybrid);");
            self.line("let ci = f32(aux.i);");
            self.line("let cc = df4_to_f(aux.c);");
            self.colour_index_code(colouring)?;
        } else {
            self.line("return FractalOrbit(z, aux, iterations, formula, false, escaped);");
        }
        self.indent -= 1;
        self.line("}");
        self.line("");
        Ok(())
    }

    /// Delta-DE with double-float probes: their step can follow the detail size down to 1e-13 of |p|.
    fn deep_delta_distance(&mut self) {
        let finaliser = self.delta_finaliser();
        let prefix = self.prefix.clone();
        self.line(
            "let delta = max(1e-5 * detail_size, 1e-13 * max(df1_to_f(df3_length(p)), 1.0));",
        );
        self.line("let r_deep = df3_length(df4_to3(orbit.z));");
        self.line("let r = df1_to_f(r_deep);");
        self.line("var gradient: vec3<f32>;");
        self.line("for (var axis = 0; axis < 3; axis++) {");
        self.line("    let centre = df3_get(p, axis);");
        self.line("    let forward = df1_add(centre, df1_f(delta));");
        self.line("    let backward = df1_sub(centre, df1_f(delta));");
        self.line("    var ahead = p;");
        self.line("    var behind = p;");
        self.line("    ahead.hi[axis] = forward.hi;");
        self.line("    ahead.lo[axis] = forward.lo;");
        self.line("    behind.hi[axis] = backward.hi;");
        self.line("    behind.lo[axis] = backward.lo;");
        self.line("    let step = df1_to_f(df1_sub(forward, backward));");
        self.line(format!(
            "    let r_ahead = df3_length(df4_to3({prefix}orbit(ahead, orbit.iterations).z));"
        ));
        self.line(format!(
            "    let r_behind = df3_length(df4_to3({prefix}orbit(behind, orbit.iterations).z));"
        ));
        self.line("    let one_sided = min(abs(df1_to_f(df1_sub(r_ahead, r_deep))), abs(df1_to_f(df1_sub(r_deep, r_behind)))) / (0.5 * step);");
        self.line("    gradient[axis] = max(abs(df1_to_f(df1_sub(r_ahead, r_behind))) / step, one_sided);");
        self.line("}");
        self.line("let dr = length(gradient);");
        self.line(format!("var distance = select(r, {finaliser}, dr > 0.0);"));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn library_validates() {
        let source = super::library();
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{e:?}"));
    }
}
