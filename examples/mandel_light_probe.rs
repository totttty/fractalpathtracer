//! Read-only lighting diagnosis using the production generated field and shadow marcher.
use anyhow::{Context, Result, ensure};
use fpt_metal::{MandelbulberScene, ffi::*, scene};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{ffi::CStr, fs, path::Path};

const PROBE: &str = r#"
// Stop reasons mirror the production march loop, before hit refinement.
float4 primaryExit(float3 direction,constant FptRenderConfig &cfg) {
    float3 position=cameraPos(cfg),start=position;
    int limit=sdfMarchIterationLimit(int(cfg.render[1]),cfg);
    float distance=0,threshold=0;
    for (int i=0;i<limit;++i) {
        threshold=mandelbulberMarchThreshold(position,cfg);
        distance=mapSdf(position,cfg);
        if (!isfinite(distance)) return float4(2,i,0,threshold);
        if (distance<threshold) return float4(1,i,distance/max(threshold,1e-30f),threshold);
        float3 next=position+direction*sdfMarchStep(distance,threshold,cfg);
        if (all(next==position)) return float4(3,i,distance/max(threshold,1e-30f),threshold);
        position=next;
        if (length(position-start)>=cfg.render[4]) return float4(4,i,distance/max(threshold,1e-30f),threshold);
    }
    return float4(5,limit,distance/max(threshold,1e-30f),threshold);
}
// Diagnostic hard-shadow version of the native penetrating-light contract.
float4 boundedShadow(float3 p,float3 light,constant FptRenderConfig &cfg) {
    float threshold=mandelbulberMarchThreshold(p,cfg);
    bool penetrating=cfg.mandel_appearance[6]>0;
    float limit=penetrating ? length(p-cameraPos(cfg))*cfg.mandel_appearance[8] : cfg.render[4];
    float travel=threshold;
    if (cfg.mandel_appearance[7]==0) return float4(1,limit/cfg.render[4],0,0);
    for (int i=0;i<int(cfg.render[1]);++i) {
        if (travel>=limit) return float4(1,limit/cfg.render[4],i,0);
        float distance=mapSdf(p+light*travel,cfg);
        if (!isfinite(distance)) return float4(0,limit/cfg.render[4],i,1);
        if (distance<threshold) return float4(penetrating ? clamp(travel/limit,0.0f,1.0f):0.0f,limit/cfg.render[4],i,0);
        float next=travel+max(distance*cfg.vset_values[113],1e-15f*cfg.set_values[0]);
        if (!(next>travel)) return float4(0,limit/cfg.render[4],i,1);
        travel=next;
    }
    return float4(0,limit/cfg.render[4],cfg.render[1],1);
}
kernel void mandelbulber_field_sample_kernel(
    device const float4 *points [[buffer(0)]], device float4 *samples [[buffer(1)]],
    constant FptRenderConfig &cfg [[buffer(2)]], uint gid [[thread_position_in_grid]]) {
    float2 xy = points[gid].xy;
    float3 dr = mandelbulberCameraRay(xy, cfg);
    if (points[gid].z==3) { samples[gid]=primaryExit(dr,cfg); return; }
    MandelbulberMarchResult primary = marchMandelbulber(dr, cameraPos(cfg), int(cfg.render[1]), cfg);
    if (!primary.found) { samples[gid] = float4(0); return; }
    float3 p = primary.position, n = normalAt(p, cfg);
    Material m = userSdf(p, cfg).material;
    float3 light = rotateCamera(float3(0,0,1),float2(cfg.sun[1]*pi/180.0f,cfg.sun[2]*pi/180.0f));
    if (points[gid].z==2) { samples[gid]=boundedShadow(p,light,cfg); return; }
    float ndotl = max(dot(n,light),0.0f);
    float diffuse = (1-cfg.mandel_appearance[5])+ndotl*cfg.mandel_appearance[5];
    float3 potential = min(cfg.sun[3]*diffuse,500.0f)*(1-m.translucency)*float3(cfg.sun_color[0],cfg.sun_color[1],cfg.sun_color[2])*m.rgb;
    float3 direct = sunContributionWithSurface(p,xy,0,m,n,cfg)*m.rgb;
    float threshold = mandelbulberMarchThreshold(p,cfg);
    MandelbulberMarchResult shadow = marchMandelbulber(light,p+light*threshold,int(cfg.render[1]),cfg);
    float travel = length(shadow.position-p)/cfg.render[4];
    // 1: surface blocker, 2: range reached, 3: stopped early without a hit.
    float state = shadow.found ? 1.0f : travel>.99f ? 2.0f : 3.0f;
    if (points[gid].z==0) samples[gid]=float4(1,dot(potential,float3(.2126,.7152,.0722)),dot(direct,float3(.2126,.7152,.0722)),state);
    else samples[gid]=float4(dot(m.rgb,float3(.2126,.7152,.0722)),ndotl,travel,threshold);
}
"#;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 5,
        "usage: mandel_light_probe scene.fract mandel-root width height NEW-report.json"
    );
    let out = Path::new(&args[4]);
    ensure!(!out.exists(), "refusing to overwrite a report");
    let source = MandelbulberScene::load(Path::new(&args[0]))?;
    let loaded = scene::load_scene_config(&scene::parse_render_args(&[
        args[0].clone(),
        "--mandelbulber-root".into(),
        args[1].clone(),
        "--width".into(),
        args[2].clone(),
        "--height".into(),
        args[3].clone(),
        "--samples".into(),
        "1".into(),
        "--mandel-appearance".into(),
        "authored-path".into(),
    ])?)?;
    let mut cfg = loaded.config;
    // Isolate central hard-shadow rays; retain source values in the report.
    let original_sun = cfg.sun;
    cfg.sun[4] = 0.0;
    cfg.mandel_appearance[6] = u32::from(source.main_light_penetrating) as f32;
    cfg.mandel_appearance[7] = u32::from(source.main_light_cast_shadows) as f32;
    cfg.mandel_appearance[8] = if source.camera_projection == 0 {
        (2.0 * (source.fov_degrees.to_radians() * 0.5).tan()) as f32
    } else if source.camera_projection == 2 {
        (source.fov_degrees.to_radians() * 0.5) as f32
    } else {
        source.fov_degrees.to_radians() as f32
    };
    let mut shader = String::from_utf8(
        loaded
            .runtime_metal_source
            .context("missing generated shader")?,
    )?;
    let marker = "kernel void mandelbulber_field_sample_kernel(";
    ensure!(
        shader.matches(marker).count() == 1,
        "field probe entrypoint changed"
    );
    shader = shader.replace(marker, "kernel void original_field_sample_kernel(");
    shader.push_str(PROBE);
    let mut points: Vec<[f32; 4]> = Vec::new();
    let aspect = cfg.width as f32 / cfg.height as f32;
    for y in 0..20 {
        for x in 0..20 {
            for mode in 0..4 {
                points.push([
                    ((x as f32 + 0.5) / 20.0 - 0.5) * aspect,
                    0.5 - (y as f32 + 0.5) / 20.0,
                    mode as f32,
                    0.0,
                ]);
            }
        }
    }
    let mut samples = vec![FptMandelbulberFieldSample::default(); points.len()];
    let mut error = [0i8; 4096];
    let status = unsafe {
        fpt_mandelbulber_sample_field(
            c"unused.metallib".as_ptr(),
            shader.as_ptr().cast(),
            shader.len(),
            &cfg,
            points.as_ptr().cast(),
            points.len(),
            samples.as_mut_ptr(),
            error.as_mut_ptr(),
            error.len(),
        )
    };
    ensure!(
        status == 0,
        "{}",
        unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy()
    );
    let raw: Vec<[f32; 4]> = samples
        .iter()
        .map(|s| [s.distance, s.radius, s.derivative, s.iterations])
        .collect();
    ensure!(
        raw.iter().flatten().all(|v| v.is_finite()),
        "non-finite lighting probe"
    );
    let primary: Vec<_> = raw.iter().step_by(4).collect();
    let candidate: Vec<_> = raw.iter().skip(2).step_by(4).collect();
    let exits: Vec<_> = raw.iter().skip(3).step_by(4).collect();
    let hits = primary.iter().filter(|r| r[0] > 0.0).count();
    let source_parameters = source.main_parameters;
    let report = json!({"scene":args[0],"scene_sha256":format!("{:x}",Sha256::digest(fs::read(&args[0])?)),
        "shader_sha256":format!("{:x}",Sha256::digest(shader.as_bytes())),"render_size":[cfg.width,cfg.height],"grid":[20,20],
        "scope":"Central hard-shadow light1 probe; soft shadow cone forced to zero. Potential/direct bypass the light enabled switch to expose unsupported-source routing separately. Diagnostic bounded shadow uses fixed surface threshold and native penetrating-light range/attenuation. No renderer changes.",
        "source_sun":original_sun,"source_sun_color":cfg.sun_color,"ambient_enabled":source.ambient_occlusion_enabled,"ambient_mode":source.ambient_occlusion_mode,
        "source_main_cast_shadows":source.main_light_cast_shadows,"source_main_penetrating":source.main_light_penetrating,
        "source_aux_enabled":source.auxiliary_light_enabled,"source_parameters":source_parameters,
        "hits":hits,"lit_normal_hits":primary.iter().filter(|r|r[0]>0.0&&r[1]>1e-5).count(),
        "direct_nonzero_hits":primary.iter().filter(|r|r[0]>0.0&&r[2]>1e-5).count(),
        "shadow_hit":primary.iter().filter(|r|r[0]>0.0&&r[3]==1.0).count(),
        "shadow_clear":primary.iter().filter(|r|r[0]>0.0&&r[3]==2.0).count(),
        "shadow_unresolved":primary.iter().filter(|r|r[0]>0.0&&r[3]==3.0).count(),
        "candidate_lit_hits":primary.iter().zip(&candidate).filter(|(r,c)|r[0]>0.0 && r[1]*c[0]>1e-5).count(),
        "candidate_unresolved":candidate.iter().filter(|c|c[3]>0.0).count(),
        "primary_exit_counts":{
            "hit":exits.iter().filter(|r|r[0]==1.0).count(),
            "nonfinite":exits.iter().filter(|r|r[0]==2.0).count(),
            "fp32_position_stall":exits.iter().filter(|r|r[0]==3.0).count(),
            "view_range":exits.iter().filter(|r|r[0]==4.0).count(),
            "iteration_budget":exits.iter().filter(|r|r[0]==5.0).count()
        },
        "primary_probe_found_disagreements":primary.iter().zip(&exits).filter(|(p,e)|(p[0]>0.0)!=(e[0]==1.0)).count(),
        "layouts":[["primary_found","unshadowed_material_light_luminance","production_direct_luminance","shadow_class"],
        ["material_luminance","normal_dot_light","shadow_travel_fraction","surface_threshold"],
        ["candidate_visibility","candidate_range_fraction","candidate_steps","candidate_unresolved"],
        ["primary_exit_class","primary_iterations","distance_over_threshold","threshold"]],"samples":raw});
    fs::create_dir_all(out.parent().unwrap_or(Path::new(".")))?;
    fs::write(out, serde_json::to_vec_pretty(&report)?)?;
    println!("hits={hits}; report={}", out.display());
    Ok(())
}
