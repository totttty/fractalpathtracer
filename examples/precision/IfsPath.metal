// SPDX-License-Identifier: Apache-2.0
// Scene-pinned experiment. All positions and orbit arithmetic remain expanded;
// only final shading values and scattering directions become float32.
float value(R a) { return a.hi + (a.mid + a.lo); }
V camera() { return @ORIGIN@; }
V nativeDirection(float3 d) { return V(R(d.x), R(d.z), R(d.y)); }
float3 worldDirection(V d) { return float3(value(d.x), value(d.z), value(d.y)); }
float3 worldPosition(V p) { return worldDirection(p) * 1024.0f; }
R thresholdAt(V p, constant FptRenderConfig &cfg) {
    return minimum(maximum((p-camera()).Length()/R(float(cfg.height)), @MIN_THRESHOLD@), R(1));
}
struct Hit { V position; R threshold; bool found; bool exhausted; };
Hit trace(V origin, V direction, int limit, constant FptRenderConfig &cfg) {
    V p = origin;
    R step(0), threshold(1.0e-12f), d(0);
    bool found = false;
    int i = 0;
    for (; i < limit; ++i) {
        threshold = thresholdAt(p,cfg);
        d = field(p);
        if (d < threshold) { found=true; break; }
        step = minimum(maximum(d-R(.5f)*threshold,R(0))*R(.75f),R(3));
        V next = p + direction*step;
        if (!((next-p).Length()>R(0))) return {p,threshold,false,true};
        p = next;
        if ((p-camera()).Length()>@VIEW_MAX@) return {p,threshold,false,false};
    }
    if (found) {
        step=step*R(.5f);
        for (int j=0;j<30;++j) {
            if (d<threshold && d>threshold*@REFINE_RATIO@) break;
            V next=p;
            if (d>threshold) next=p+direction*step;
            else if (d<threshold*@REFINE_RATIO@) next=p-direction*step;
            if (!((next-p).Length()>R(0))) break;
            p=next;
            d=field(p);
            step=step*R(.5f);
        }
    }
    return {p,threshold,found,i==limit};
}
float3 normal(V p, R threshold) {
    R h=threshold*@NORMAL_SCALE@;
    V dx(h,R(0),R(0)),dy(R(0),h,R(0)),dz(R(0),R(0),h);
    V g(field(p+dx)-field(p-dx),field(p+dy)-field(p-dy),field(p+dz)-field(p-dz));
    R length=g.Length();
    return length>R(0) ? worldDirection(g*(R(1)/length)) : float3(0);
}
R colorIndex(V z) {
    Aux aux={R(1),R(1),0};
    R color_min(1000);
    for (int i=0;i<250;++i) {
        V old=z;
        aux.i=i;
        formula(z,aux);
        R radius=z.Length();
        color_min=minimum(color_min,radius);
        if (kMandelColorPreV215) {
            if (radius>@COLOR_BAILOUT@ || (z-old).Length()/radius<@ORBIT_EPS@) break;
        } else if (radius>R(100)) break;
    }
    return color_min*R(1000);
}
float shadow(V position, float3 world_direction, constant FptRenderConfig &cfg) {
    if (cfg.mandel_appearance[7]==0.0f) return 1.0f;
    V direction=nativeDirection(world_direction);
    R threshold=thresholdAt(position,cfg);
    bool penetrating=cfg.mandel_appearance[6]!=0.0f;
    R limit=penetrating ? (position-camera()).Length()*R(cfg.mandel_appearance[8]) : @VIEW_MAX@;
    R travel=threshold;
    float soft_range=tan(cfg.sun[4]);
    bool soft=soft_range>0.0f && isfinite(soft_range) && cfg.vset_values[115]!=2.0f;
    float occlusion=0.0f;
    for(int i=0;i<int(cfg.render[1]);++i) {
        if(!(travel<limit)) return 1.0f-occlusion;
        R distance=field(position+direction*travel);
        if(soft) {
            float angle=value(maximum(distance-threshold,R(0))/travel);
            float hit=metal::max(1.0f-angle/soft_range,0.0f);
            if(penetrating) hit*=value((limit-travel)/limit);
            occlusion=metal::max(occlusion,hit);
        }
        if(distance<threshold) return soft ? 1.0f-occlusion
            : penetrating ? clamp(value(travel/limit),0.0f,1.0f) : 0.0f;
        R step=maximum(minimum(distance,R(1.0e6f))*R(cfg.vset_values[113]),R(1.0e-15f));
        R next=travel+step;
        if(!(next>travel)) return -1.0f;
        travel=next;
    }
    return -1.0f;
}
float3 sun(V p,float3 n,Material material,constant FptRenderConfig &cfg) {
    float3 direction=rotateCamera(float3(0,0,1),float2(cfg.sun[1],cfg.sun[2])*pi/180.0f);
    float shading=cfg.mandel_appearance[5];
    float diffuse=1.0f-shading+metal::max(dot(n,direction),0.0f)*shading;
    float visibility=shadow(p,direction,cfg);
    if(visibility<0.0f) return float3(NAN);
    return visibility*min(cfg.sun[3]*diffuse,500.0f)*(1.0f-material.translucency)*
        float3(cfg.sun_color[0],cfg.sun_color[1],cfg.sun_color[2]);
}
