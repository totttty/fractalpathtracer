// SPDX-License-Identifier: Apache-2.0
// Pinned Menger/Tglad experiment. Native formula bodies are imported at runtime.
using R = fpt_precision::Scalar;
using fpt_precision::absolute;
using fpt_precision::root;
R maximum(R a,R b) { return a>b?a:b; }
R minimum(R a,R b) { return a<b?a:b; }
R fabs(R a) { return absolute(a); }
bool operator!=(R a,R b) { return a<b || a>b; }
void operator+=(thread R &a,R b) { a=a+b; }
void operator-=(thread R &a,R b) { a=a-b; }
void operator*=(thread R &a,R b) { a=a*b; }
struct V {
    R x,y,z,w;
    V(R a,R b,R c,R d=R(0)):x(a),y(b),z(c),w(d) {}
    R Dot(V b) { return x*b.x+y*b.y+z*b.z+w*b.w; }
    R Length() { return root(Dot(*this)); }
};
V operator+(V a,V b) { return V(a.x+b.x,a.y+b.y,a.z+b.z,a.w+b.w); }
V operator-(V a,V b) { return V(a.x-b.x,a.y-b.y,a.z-b.z,a.w-b.w); }
V operator*(V a,R b) { return V(a.x*b,a.y*b,a.z*b,a.w*b); }
void operator*=(thread V &a,R b) { a=a*b; }
V fabs(V a) { return V(absolute(a.x),absolute(a.y),absolute(a.z),absolute(a.w)); }
struct Aux { R DE,color; int i; };
@FORMULAS@
void iterate(thread V &z, thread Aux &aux) {
    if(aux.i<2 || aux.i%2==1) formula0(z,aux); else formula1(z,aux);
}
R field(V z) {
    Aux aux={R(1),R(1),0};
    R radius=z.Length();
    for(int i=0;i<250;++i) {
        aux.i=i;
        iterate(z,aux);
        radius=z.Length();
        if(radius>R(100)) break;
    }
    return minimum(maximum(radius/aux.DE,R(0)),R(10));
}
float value(R a) { return a.hi+(a.mid+a.lo); }
bool finite(R a) { return isfinite(a.hi) && isfinite(a.mid); }
V camera() { return @ORIGIN@; }
V nativeDirection(float3 d) { return V(R(d.x),R(d.z),R(d.y)); }
float3 worldDirection(V d) { return float3(value(d.x),value(d.z),value(d.y)); }
float3 worldPosition(V p) { return worldDirection(p)*1024.0f; }
R thresholdAt(V p,constant FptRenderConfig &cfg) {
    return minimum(maximum((p-camera()).Length()/R(float(cfg.height)),@MIN_THRESHOLD@),R(1));
}
struct Hit { V position; bool found; bool invalid; };
Hit trace(V origin,V direction,int requested,constant FptRenderConfig &cfg,uint seed) {
    V p=origin;
    R step(0),threshold(0),d(0);
    bool found=false;
    int limit=sdfMarchIterationLimit(requested,cfg),i=0;
    for(;i<limit;++i) {
        threshold=thresholdAt(p,cfg);
        d=field(p);
        if(!finite(d)) return {p,false,true};
        if(d<threshold) { found=true; break; }
        step=minimum(maximum(d-R(.5f)*threshold,R(0))*R(mandelStepMultiplier(seed)),R(3));
        V next=p+direction*step;
        if(!((next-p).Length()>R(0))) return {p,false,true};
        p=next;
        if(!((p-origin).Length()<@VIEW_MAX@)) return {p,false,false};
    }
    if(!found) return {p,false,true};
    step=step*R(.5f);
    for(int j=0;j<30;++j) {
        if(d<threshold && d>threshold*@REFINE_RATIO@) break;
        V next=p;
        if(d>threshold) next=p+direction*step;
        else if(d<threshold*@REFINE_RATIO@) next=p-direction*step;
        if(!((next-p).Length()>R(0))) break;
        p=next;
        d=field(p);
        if(!finite(d)) return {p,false,true};
        step=step*R(.5f);
    }
    return {p,true,false};
}
float3 normal(V p,R h) {
    V dx(h,R(0),R(0)),dy(R(0),h,R(0)),dz(R(0),R(0),h);
    V g(field(p+dx)-field(p-dx),field(p+dy)-field(p-dy),field(p+dz)-field(p-dz));
    R length=g.Length();
    return finite(length) && length>R(0) ? worldDirection(g*(R(1)/length)) : float3(NAN);
}
R colorIndex(V z) {
    Aux aux={R(1),R(1),0};
    R color_min(1000),radius=z.Length();
    for(int i=0;i<250;++i) {
        aux.i=i;
        iterate(z,aux);
        radius=z.Length();
        color_min=minimum(color_min,radius);
        // Preserve the effective generated color mode, not the file's version label.
        if(radius>R(100)) break;
    }
    return minimum(R(100),color_min)*R(1000)+aux.color*R(100)
        +minimum(radius/absolute(aux.DE),R(20))*R(5000);
}
float shadow(V point,float3 direction,constant FptRenderConfig &cfg) {
    if(cfg.mandel_appearance[7]==0.0f) return 1.0f;
    V dr=nativeDirection(direction);
    R threshold=thresholdAt(point,cfg);
    bool penetrating=cfg.mandel_appearance[6]!=0.0f;
    R end=penetrating ? (point-camera()).Length()*R(cfg.mandel_appearance[8]) : @VIEW_MAX@;
    R travel=threshold;
    float soft_range=tan(cfg.sun[4]);
    bool soft=soft_range>0.0f && isfinite(soft_range) && cfg.vset_values[115]!=2.0f;
    float occlusion=0.0f;
    for(int i=0;i<int(cfg.render[1]);++i) {
        if(!(travel<end)) return 1.0f-occlusion;
        R d=field(point+dr*travel);
        if(!finite(d)) return -1.0f;
        if(soft) {
            float angle=value(maximum(d-threshold,R(0))/travel);
            float hit=metal::max(1.0f-angle/soft_range,0.0f);
            if(penetrating) hit*=value((end-travel)/end);
            occlusion=metal::max(occlusion,hit);
        }
        if(d<threshold) return soft ? 1.0f-occlusion
            : penetrating ? clamp(value(travel/end),0.0f,1.0f) : 0.0f;
        R next=travel+maximum(minimum(d,R(1.0e6f)),@ORBIT_EPS@);
        if(!finite(next) || !(next>travel)) return -1.0f;
        travel=next;
    }
    return -1.0f;
}
float3 sun(V point,float3 normal,Material material,constant FptRenderConfig &cfg) {
    float3 direction=rotateCamera(float3(0,0,1),float2(cfg.sun[1],cfg.sun[2])*pi/180.0f);
    float shading=cfg.mandel_appearance[5];
    float diffuse=1.0f-shading+metal::max(dot(normal,direction),0.0f)*shading;
    float visibility=shadow(point,direction,cfg);
    if(visibility<0.0f) return float3(NAN);
    return visibility*min(cfg.sun[3]*diffuse,500.0f)*(1.0f-material.translucency)
        *float3(cfg.sun_color[0],cfg.sun_color[1],cfg.sun_color[2]);
}
