// SPDX-License-Identifier: Apache-2.0
// Offline white-headlight diagnostic. Formula code is supplied at runtime.
using R = fpt_precision::Scalar;
using fpt_precision::root;
using fpt_precision::absolute;
R maximum(R a, R b) { return a > b ? a : b; }
R minimum(R a, R b) { return a < b ? a : b; }
R max(R a, R b) { return maximum(a, b); }
struct V {
    R x, y, z;
    V(R a, R b, R c, R unused = R(0)) : x(a), y(b), z(c) {}
    R Dot(V b) { return x*b.x + y*b.y + z*b.z; }
    R Length() { return root(Dot(*this)); }
};
V operator+(V a, V b) { return V(a.x+b.x, a.y+b.y, a.z+b.z); }
V operator-(V a, V b) { return V(a.x-b.x, a.y-b.y, a.z-b.z); }
V operator*(V a, R b) { return V(a.x*b, a.y*b, a.z*b); }
V operator*(V a, V b) { return V(a.x*b.x, a.y*b.y, a.z*b.z); }
V fabs(V a) { return V(absolute(a.x), absolute(a.y), absolute(a.z)); }
struct Aux { R DE, pseudoKleinianDE; int i; };
@FORMULA@
R field(V z) {
    Aux aux = {R(1), R(1), 0};
    R radius = z.Length();
    for (int i = 0; i < @ITERATIONS@; ++i) {
        V previous = z;
        aux.i = i;
        formula(z, aux);
        radius = z.Length();
        if (radius > R(100) || (z-previous).Length()/radius < @ORBIT_EPS@) break;
    }
    R xy = root(z.x*z.x + z.y*z.y);
    R distance = aux.DE > R(0)
        ? maximum(xy-aux.pseudoKleinianDE, absolute(xy*z.z)/radius)/aux.DE : radius;
    return minimum(maximum(distance, R(0)), R(10));
}
float2 pack(R a) { return float2(a.hi, a.mid+a.lo); }
kernel void probe(device const float *rays [[buffer(0)]],
                  device float *output [[buffer(1)]], uint i [[thread_position_in_grid]]) {
    V direction(R(rays[i*9], rays[i*9+1], rays[i*9+2]),
                R(rays[i*9+3], rays[i*9+4], rays[i*9+5]),
                R(rays[i*9+6], rays[i*9+7], rays[i*9+8]));
    V origin = @ORIGIN@, pos = origin;
    R step(0), threshold = @MIN_THRESHOLD@, d(0);
    bool hit = false, stall = false;
    int steps = 0;
    for (; steps < @MAX_STEPS@; ++steps) {
        threshold = minimum(maximum((pos-origin).Length()/@THRESHOLD_SCALE@, @MIN_THRESHOLD@), R(1));
        d = field(pos);
        if (d < threshold) { hit = true; break; }
        step = minimum(maximum(d-R(.5f)*threshold, R(0))*@DE_FACTOR@, R(3));
        V next = pos + direction*step;
        if (!((next-pos).Length() > R(0))) { stall = true; break; }
        pos = next;
        if ((pos-origin).Length() > @VIEW_MAX@) break;
    }
    if (hit) {
        step = step*R(.5f);
        for (int j = 0; j < 30; ++j) {
            if (d < threshold && d > threshold*@REFINE_RATIO@) break;
            V next = pos;
            if (d > threshold) next = pos + direction*step;
            else if (d < threshold*@REFINE_RATIO@) next = pos - direction*step;
            if (!((next-pos).Length() > R(0))) break;
            pos = next;
            d = field(pos);
            step = step*R(.5f);
        }
    }
    R shade(0);
    if (hit) {
        R h = threshold*@NORMAL_SCALE@;
        V dx(h,R(0),R(0)), dy(R(0),h,R(0)), dz(R(0),R(0),h);
        V gradient(field(pos+dx)-field(pos-dx), field(pos+dy)-field(pos-dy), field(pos+dz)-field(pos-dz));
        if (gradient.Length() > R(0)) shade = maximum(-gradient.Dot(direction)/gradient.Length(), R(0));
    }
    float2 depth = pack((pos-origin).Length()), brightness = pack(shade);
    output[i*8] = depth.x; output[i*8+1] = depth.y;
    output[i*8+2] = float(hit); output[i*8+3] = float(steps);
    output[i*8+4] = brightness.x; output[i*8+5] = brightness.y;
    output[i*8+6] = float(stall); output[i*8+7] = 0;
}
