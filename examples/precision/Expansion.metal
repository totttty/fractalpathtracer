// SPDX-License-Identifier: Apache-2.0
// Experimental fixed three-term float expansion, not IEEE binary64.
// Background: https://www.cs.cmu.edu/~quake/robust.html
// Requires safe math, normalized inputs and finite, non-underflowing intermediates.
#include <metal_stdlib>
using namespace metal;
#pragma clang fp contract(off)

namespace fpt_precision {
struct Scalar {
    float hi, mid, lo;
    Scalar(float a = 0) : hi(a), mid(0), lo(0) {}
    Scalar(float a, float b, float c = 0) : hi(a), mid(b), lo(c) {}
};

float2 two_sum(float a, float b) {
    float s = a + b, v = s - a;
    return float2(s, (a - (s - v)) + (b - v));
}

float4 accumulate(float4 a, float b) {
    float2 s = two_sum(a.x, b), t = two_sum(a.y, s.y);
    float2 u = two_sum(a.z, t.y), v = two_sum(a.w, u.y);
    return float4(s.x, t.x, u.x, v.x);
}

Scalar compact4(float4 a) {
    #pragma unroll
    for (int j = 0; j < 3; ++j) {
        float2 p = two_sum(a.z, a.w); a.z = p.x; a.w = p.y;
        p = two_sum(a.y, a.z); a.y = p.x; a.z = p.y;
        p = two_sum(a.x, a.y); a.x = p.x; a.y = p.y;
    }
    return Scalar(a.x, a.y, a.z);
}

Scalar operator+(Scalar a, Scalar b) {
    float4 r = float4(a.hi, a.mid, a.lo, 0);
    r = accumulate(r, b.hi); r = accumulate(r, b.mid); r = accumulate(r, b.lo);
    return compact4(r);
}
Scalar operator-(Scalar a) { return Scalar(-a.hi, -a.mid, -a.lo); }
Scalar operator-(Scalar a, Scalar b) { return a + (-b); }

float4 product(float4 r, float a, float b) {
    float p = a * b, e = fma(a, b, -p);
    return accumulate(accumulate(r, p), e);
}
Scalar operator*(Scalar a, Scalar b) {
    float4 r = float4(0);
    r = product(r, a.hi, b.hi);
    r = product(r, a.hi, b.mid); r = product(r, a.mid, b.hi);
    r = product(r, a.hi, b.lo); r = product(r, a.mid, b.mid); r = product(r, a.lo, b.hi);
    r = product(r, a.mid, b.lo); r = product(r, a.lo, b.mid); r = product(r, a.lo, b.lo);
    return compact4(r);
}
Scalar operator/(Scalar a, Scalar b) {
    Scalar q(a.hi / b.hi);
    #pragma unroll
    for (int j = 0; j < 3; ++j) {
        Scalar r = a - b * q;
        q = q + Scalar(r.hi / b.hi);
    }
    return q;
}
bool operator<(Scalar a, Scalar b) {
    return a.hi < b.hi || (a.hi == b.hi &&
        (a.mid < b.mid || (a.mid == b.mid && a.lo < b.lo)));
}
bool operator>(Scalar a, Scalar b) { return b < a; }

Scalar root(Scalar a) {
    if (!(a > Scalar(0))) return Scalar(0);
    Scalar q(sqrt(a.hi));
    #pragma unroll
    for (int j = 0; j < 2; ++j) q = q + (a - q * q) / (Scalar(2) * q);
    return q;
}
Scalar absolute(Scalar a) { return a < Scalar(0) ? -a : a; }
} // namespace fpt_precision
