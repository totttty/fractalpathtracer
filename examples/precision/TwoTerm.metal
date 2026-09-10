// SPDX-License-Identifier: Apache-2.0
// Diagnostic compensated float pair. Same finite-exponent limits as float32.
#include <metal_stdlib>
using namespace metal;
#pragma clang fp contract(off)
namespace fpt_precision {
float2 two_sum(float a, float b) {
    float s=a+b, v=s-a;
    return float2(s,(a-(s-v))+(b-v));
}
struct Scalar {
    float hi, mid, lo;
    Scalar(float a=0):hi(a),mid(0),lo(0) {}
    // Retain the existing three-float input ABI; arithmetic carries only two.
    Scalar(float a,float b,float c=0) {
        float2 s=two_sum(a,b+c);
        hi=s.x; mid=s.y; lo=0;
    }
};
Scalar operator+(Scalar a,Scalar b) {
    float2 s=two_sum(a.hi,b.hi), t=two_sum(a.mid,b.mid);
    s=two_sum(s.x,s.y+t.x);
    return Scalar(s.x,s.y+t.y);
}
Scalar operator-(Scalar a) { return Scalar(-a.hi,-a.mid); }
Scalar operator-(Scalar a,Scalar b) { return a+(-b); }
Scalar operator*(Scalar a,Scalar b) {
    float p=a.hi*b.hi;
    float e=fma(a.hi,b.hi,-p);
    e=e+a.hi*b.mid;
    e=e+a.mid*b.hi;
    e=e+a.mid*b.mid;
    return Scalar(p,e);
}
Scalar operator/(Scalar a,Scalar b) {
    Scalar q(a.hi/b.hi);
    for(int j=0;j<2;++j) {
        Scalar r=a-b*q;
        q=q+Scalar(r.hi/b.hi);
    }
    return q;
}
bool operator<(Scalar a,Scalar b) { return a.hi<b.hi || (a.hi==b.hi && a.mid<b.mid); }
bool operator>(Scalar a,Scalar b) { return b<a; }
Scalar root(Scalar a) {
    if (!(a>Scalar(0))) return Scalar(0);
    Scalar q(sqrt(a.hi));
    for(int j=0;j<2;++j) q=q+(a-q*q)/(Scalar(2)*q);
    return q;
}
Scalar absolute(Scalar a) { return a<Scalar(0) ? -a : a; }
}
