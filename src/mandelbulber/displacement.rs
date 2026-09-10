//! Material displacement specialization. Noise source remains external.
use super::{
    MandelbulberScene, parameter_bool, parameter_number, parameter_u32, parameter_vec3,
    rotation2_matrix,
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{fmt::Write, fs, path::Path};

fn number(v: f64) -> Result<String> {
    ensure!((v as f32).is_finite(), "non-finite Perlin parameter");
    Ok(format!("{:.9e}f", v as f32))
}
fn vector(v: [f64; 3]) -> Result<String> {
    Ok(format!(
        "float3({}, {}, {})",
        number(v[0])?,
        number(v[1])?,
        number(v[2])?
    ))
}

pub(super) fn specialize(base: &str, scene: &MandelbulberScene, root: &Path) -> Result<String> {
    let values = &scene.main_parameters;
    let mut active = Vec::new();
    for id in scene.materials.keys() {
        if parameter_bool(values, &format!("mat{id}_perlin_noise_enable"), false)?
            && parameter_bool(
                values,
                &format!("mat{id}_perlin_noise_displacement_enable"),
                false,
            )?
        {
            active.push(*id);
        }
    }
    if active.is_empty() {
        return Ok(base.to_owned());
    }
    ensure!(
        !scene.boolean_enabled,
        "Perlin displacement with boolean fractals requires per-formula material routing"
    );
    let external = fs::read_to_string(root.join("opencl/engines/perlin_noise.cl"))
        .context("reading external Perlin evaluator")?;
    ensure!(
        external.contains("NormalizedOctavePerlinNoise3D_0_1")
            && external.contains("__global uchar *p"),
        "unrecognized external Perlin evaluator"
    );
    let seed = parameter_u32(values, "clouds_random_seed", 12345)?;
    let mut permutation = [0u8; 512];
    ensure!(
        unsafe {
            crate::ffi::fpt_mandel_perlin_permutation(
                seed,
                permutation.as_mut_ptr(),
                permutation.len(),
            )
        } == 0,
        "Perlin permutation generation failed"
    );
    let mut source = format!(
        "// External Mandelbulber Perlin SHA256 {:x}; native macOS permutation seed {seed}\n#define USE_PERLIN_NOISE 1\n#define FPT_MANDEL_PERLIN 1\n",
        Sha256::digest(external.as_bytes())
    );
    source.push_str(&external.replace("__global uchar *p", "constant uchar *p"));
    writeln!(
        source,
        "\nconstant uchar fptPerlinSeeds[512] = {{ {} }};",
        permutation
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )?;
    source.push_str("static float fptPerlinDisplace(float distance, float3 point, uint material) {\n    switch (material) {\n");
    for id in active {
        let key = |s: &str| format!("mat{id}_perlin_noise_{s}");
        let octaves = parameter_u32(values, &key("iterations"), 5)?;
        ensure!(
            (1..=16).contains(&octaves),
            "mat{id}: Perlin displacement requires 1..16 octaves"
        );
        let period = parameter_vec3(values, &key("period"), [1.0; 3])?;
        ensure!(
            period.iter().all(|x| (*x as f32) > 0.0),
            "mat{id}: Perlin periods must be positive"
        );
        let rotation = rotation2_matrix(
            parameter_vec3(values, &key("rotation"), [0.0; 3])?.map(f64::to_radians),
        );
        let columns: [[f64; 3]; 3] =
            std::array::from_fn(|i| std::array::from_fn(|j| rotation[j][i]));
        let intensity = parameter_number(values, &key("displacement_intensity"), 1.0)?;
        ensure!(
            intensity >= 0.0,
            "mat{id}: negative Perlin displacement intensity"
        );
        writeln!(
            source,
            "    case {id}u: {{ float3 q = (float3x3({}, {}, {}) * point) / {};\n        float noise = NormalizedOctavePerlinNoise3D_0_1(q.x,q.y,q.z,{}, {octaves},fptPerlinSeeds);",
            vector(columns[0])?,
            vector(columns[1])?,
            vector(columns[2])?,
            vector(period)?,
            vector(parameter_vec3(values, &key("position_offset"), [0.0; 3])?)?
        )?;
        // CPU reference applies abs before value offset; upstream OpenCL differs.
        if parameter_bool(values, &key("abs"), false)? {
            source.push_str("        noise = abs(noise - 0.5f) * 2.0f;\n");
        }
        writeln!(
            source,
            "        noise += {};",
            number(parameter_number(values, &key("value_offset"), 0.0)?)?
        )?;
        if parameter_bool(values, &key("displacement_invert"), false)? {
            source.push_str("        noise = 1.0f - noise;\n");
        }
        writeln!(
            source,
            "        return distance - clamp(noise,0.0f,1.0f) * {}; }}",
            number(intensity)?
        )?;
    }
    source.push_str("    default: return distance;\n    }\n}\n");
    writeln!(
        source,
        "static float fptFractalDisplace(float distance, float3 p, constant FptRenderConfig &cfg) {{ float scale = max(setv(cfg,0),1.0f); return fptPerlinDisplace(distance / scale, p.xzy / scale, {}u) * scale; }}",
        scene.formula_material_id
    )?;
    let marker = "#define FPT_MANDEL_GENERATED_FIELD 1";
    ensure!(base.contains(marker), "missing generated insertion marker");
    // Generated material selection must compare the same displaced distances
    // as primary/normal traversal, before individual primitive unions.
    let mut result = base.replacen(marker, &format!("{source}\n{marker}"), 1);
    for variable in ["distance", "selected_distance"] {
        let line = format!(
            "float {variable}=mandelbulberGeneratedFieldSample(mandelbulberGlobalPoint(p,cfg),cfg,1,0).x;"
        );
        result = result.replace(
            &line,
            &format!("{line}\n    {variable} = fptFractalDisplace({variable},p,cfg);"),
        );
        let line = format!(
            "float {variable} = mandelbulberGeneratedFieldSample(mandelbulberGlobalPoint(p,cfg),cfg,1,0).x;"
        );
        result = result.replace(
            &line,
            &format!("{line}\n    {variable} = fptFractalDisplace({variable},p,cfg);"),
        );
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inactive_displacement_preserves_shader_without_external_sources() {
        let scene = MandelbulberScene::parse(
            "# Mandelbulber settings file\n# version 2.33\n[main_parameters]\nformula_1 1;\n",
        )
        .unwrap();
        assert_eq!(
            specialize("unchanged", &scene, Path::new("/nonexistent")).unwrap(),
            "unchanged"
        );
    }
    #[test]
    fn native_permutation_is_deterministic_and_duplicated() {
        let mut a = [0u8; 512];
        let mut b = a;
        unsafe {
            assert_eq!(
                crate::ffi::fpt_mandel_perlin_permutation(12345, a.as_mut_ptr(), 512),
                0
            );
            assert_eq!(
                crate::ffi::fpt_mandel_perlin_permutation(12345, b.as_mut_ptr(), 512),
                0
            );
            assert_ne!(
                crate::ffi::fpt_mandel_perlin_permutation(12345, b.as_mut_ptr(), 511),
                0
            );
        }
        assert_eq!(a, b);
        // Independently reproduced by the external native cPerlinNoiseOctaves.
        assert_eq!(
            &a[..16],
            &[
                214, 107, 150, 213, 89, 244, 227, 8, 31, 41, 37, 199, 49, 59, 57, 102
            ]
        );
        assert_eq!(&a[..256], &a[256..]);
        let mut unique = a[..256].to_vec();
        unique.sort_unstable();
        assert_eq!(unique, (0..=255u8).collect::<Vec<_>>());
    }
    #[test]
    fn generated_displacement_precedes_object_functions_and_rejects_bad_periods() {
        let root = std::env::temp_dir().join(format!("fpt-perlin-unit-{}", std::process::id()));
        fs::create_dir_all(root.join("opencl/engines")).unwrap();
        fs::write(root.join("opencl/engines/perlin_noise.cl"),
                  "float NormalizedOctavePerlinNoise3D_0_1(float x, float y, float z, float3 shift, int n, __global uchar *p) { return 0.5f; }").unwrap();
        let source = "# Mandelbulber settings file\n# version 2.33\n[main_parameters]\nformula_1 1;\nmat1_is_defined true;\nmat1_perlin_noise_enable true;\nmat1_perlin_noise_displacement_enable true;\nmat1_perlin_noise_abs true;\n";
        let scene = MandelbulberScene::parse(source).unwrap();
        let shader = "#define FPT_MANDEL_GENERATED_FIELD 1\nfloat distance = mandelbulberGeneratedFieldSample(mandelbulberGlobalPoint(p,cfg),cfg,1,0).x;";
        let result = specialize(shader, &scene, &root).unwrap();
        assert!(
            result.find("static float fptFractalDisplace").unwrap()
                < result.find("#define FPT_MANDEL_GENERATED_FIELD").unwrap()
        );
        assert!(result.contains("distance = fptFractalDisplace(distance,p,cfg);"));
        assert!(result.find("noise = abs").unwrap() < result.find("noise +=").unwrap());
        let bad = MandelbulberScene::parse(&format!("{source}mat1_perlin_noise_period 0 1 1;\n"))
            .unwrap();
        assert!(specialize(shader, &bad, &root).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
