//! Mandelbulber OpenCL formula frontend and Metal runtime emitter.
//!
//! This module deliberately imports source from an external Mandelbulber tree.
//! Generated output retains the upstream GPL licensing boundary and belongs in
//! an external cache/artifact directory, not the Apache source distribution.

use anyhow::{Context, Result, anyhow, bail, ensure};
use base64::Engine;
use flate2::read::ZlibDecoder;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use super::MandelbulberScene;
use super::catalog::{ResolvedFormulaSource, formula_symbols, resolve_formula_source};
use super::formula_optimizer::{
    FormulaSimplificationStats, StructuralValue, simplify_formula_body,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenKind {
    Identifier,
    Number,
    String,
    Character,
    Punctuation,
    Preprocessor,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub offset: usize,
    pub line: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct FormulaFrontendReport {
    pub symbol: String,
    pub id: i32,
    pub internal_name: String,
    pub source_path: String,
    pub de_type: String,
    pub de_function_type: String,
    pub pixel_addition: String,
    pub default_bailout: f64,
    pub analytic_function: String,
    pub coloring_function: String,
    pub iteration_function: String,
    pub token_count: usize,
    pub identifiers: Vec<String>,
    pub parameter_reads: Vec<String>,
    pub auxiliary_fields: Vec<String>,
    pub constructs: Vec<String>,
    pub metal_emitter: String,
}

#[derive(Clone, Debug)]
pub struct ParsedFormula {
    pub source: ResolvedFormulaSource,
    pub function_name: String,
    pub function_source: String,
    pub tokens: Vec<Token>,
    pub parameter_reads: Vec<String>,
    pub auxiliary_fields: Vec<String>,
    pub constructs: Vec<String>,
}

pub struct RuntimeFormulaSlot<'a> {
    pub index: usize,
    pub formula: &'a ParsedFormula,
    pub formula_values: &'a BTreeMap<String, String>,
    pub iterations: u32,
    pub weight: f64,
    pub add_c_constant: bool,
    pub check_for_bailout: bool,
    pub bailout: f64,
}

/// Removes Metal kernel entry points that are irrelevant to a specialised
/// diagnostic render. Static helper functions remain available to the retained
/// kernels, while the Metal compiler avoids generating every production and
/// research pipeline in the full renderer source.
pub fn retain_metal_kernels(source: &str, retained: &[&str]) -> Result<String> {
    let retained = retained.iter().copied().collect::<BTreeSet<_>>();
    let bytes = source.as_bytes();
    let marker = b"kernel void ";
    let mut output = String::with_capacity(source.len());
    let mut copied_until = 0usize;
    let mut search_from = 0usize;
    while let Some(relative_start) = bytes[search_from..]
        .windows(marker.len())
        .position(|candidate| candidate == marker)
    {
        let start = search_from + relative_start;
        let line_start = bytes[..start]
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1);
        if bytes[line_start..start]
            .iter()
            .any(|byte| !byte.is_ascii_whitespace())
        {
            search_from = start + marker.len();
            continue;
        }
        let name_start = start + marker.len();
        let name_end = bytes[name_start..]
            .iter()
            .position(|byte| *byte == b'(')
            .map(|offset| name_start + offset)
            .ok_or_else(|| anyhow!("Metal kernel at byte {start} has no argument list"))?;
        let name = source[name_start..name_end].trim();
        let body_start = bytes[name_end..]
            .iter()
            .position(|byte| *byte == b'{')
            .map(|offset| name_end + offset)
            .ok_or_else(|| anyhow!("Metal kernel {name} has no body"))?;
        let body_end = metal_braced_block_end(bytes, body_start)
            .ok_or_else(|| anyhow!("Metal kernel {name} has an unbalanced body"))?;
        output.push_str(&source[copied_until..line_start]);
        if retained.contains(name) {
            output.push_str(&source[line_start..body_end]);
        } else {
            output.push('\n');
        }
        copied_until = body_end;
        search_from = body_end;
    }
    output.push_str(&source[copied_until..]);
    Ok(output)
}

fn metal_braced_block_end(source: &[u8], opening_brace: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut index = opening_brace;
    let mut quote = None::<u8>;
    let mut line_comment = false;
    let mut block_comment = false;
    while index < source.len() {
        let byte = source[index];
        let next = source.get(index + 1).copied();
        if line_comment {
            line_comment = byte != b'\n';
        } else if block_comment {
            if byte == b'*' && next == Some(b'/') {
                block_comment = false;
                index += 1;
            }
        } else if let Some(delimiter) = quote {
            if byte == b'\\' {
                index += 1;
            } else if byte == delimiter {
                quote = None;
            }
        } else if byte == b'/' && next == Some(b'/') {
            line_comment = true;
            index += 1;
        } else if byte == b'/' && next == Some(b'*') {
            block_comment = true;
            index += 1;
        } else if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
        } else if byte == b'{' {
            depth += 1;
        } else if byte == b'}' {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(index + 1);
            }
        }
        index += 1;
    }
    None
}

pub fn specialize_scene(
    base_source: &str,
    scene: &mut MandelbulberScene,
    source_root: &Path,
    set_values: &[f32; 40],
) -> Result<String> {
    specialize_scene_with_kernel_specialization(
        base_source,
        scene,
        source_root,
        set_values,
        true,
        false,
        SceneFormulaOptimizationPolicy::default(),
    )
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SceneFormulaOptimizationPolicy {
    pub periodic_hybrid_loop: bool,
    pub unrolled_periodic_hybrid_loop: bool,
    pub partial_evaluation_formula_id: i32,
    pub partial_evaluation_phases: bool,
    pub partial_evaluation_scalarize_loops: bool,
    pub partial_evaluation_dce: bool,
    pub partial_evaluation_cse: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct FormulaSourceOptimization {
    formula_id: i32,
    phases: bool,
    scalarize_loops: bool,
    dce: bool,
    cse: bool,
}

impl FormulaSourceOptimization {
    fn from_policy(policy: SceneFormulaOptimizationPolicy) -> Self {
        Self {
            formula_id: policy.partial_evaluation_formula_id,
            phases: policy.partial_evaluation_phases,
            scalarize_loops: policy.partial_evaluation_scalarize_loops,
            dce: policy.partial_evaluation_dce,
            cse: policy.partial_evaluation_cse,
        }
    }
}

pub fn specialize_scene_with_kernel_specialization(
    base_source: &str,
    scene: &mut MandelbulberScene,
    source_root: &Path,
    set_values: &[f32; 40],
    kernel_specialization: bool,
    direct_hybrid_loop: bool,
    formula_optimization: SceneFormulaOptimizationPolicy,
) -> Result<String> {
    let generic_source;
    let base_source = if kernel_specialization {
        base_source
    } else {
        generic_source = format!("// FPT_MANDEL_GENERIC_KERNEL\n{base_source}");
        &generic_source
    };
    let mut formulas = Vec::new();
    for (index, slot) in scene.formula_slots.iter().enumerate() {
        if !slot.active() {
            continue;
        }
        let formula = if slot.formula_id == 10_000 {
            let encoded = slot
                .parameters
                .get("formula_code")
                .ok_or_else(|| anyhow!("custom formula slot {} has no formula_code", index + 1))?;
            parse_embedded_custom_formula(source_root, encoded)?
        } else {
            parse_formula(source_root, &slot.formula_id.to_string())?
        };
        ensure!(
            if scene.hybrid_enabled {
                supports_runtime_kernel(&formula)
            } else {
                supports_runtime_emitter(&formula)
            },
            "formula slot {} ({}) cannot use the generated runtime yet: {} parameter reads, DE type {}, finalizer {}",
            index + 1,
            formula.source.symbol,
            formula.parameter_reads.len(),
            formula.source.de_type,
            formula.source.analytic_function
        );
        formulas.push((index, formula));
    }
    let sources = formulas
        .iter()
        .map(|(index, formula)| (*index, &formula.source))
        .collect::<Vec<_>>();
    scene.configure_formula_slots(&sources)?;
    let mut configured_values = *set_values;
    configured_values[super::PARAM_BAILOUT] = scene.bailout as f32;
    configured_values[super::PARAM_ADD_C_CONSTANT] = u32::from(scene.add_c_constant) as f32;
    let specialized = if scene.boolean_enabled {
        specialize_fpt_shader_boolean(base_source, &formulas, scene, &configured_values)
    } else if scene.hybrid_enabled {
        let sequence = scene.hybrid_sequence()?;
        let slots = formulas
            .iter()
            .map(|(index, formula)| RuntimeFormulaSlot {
                index: *index,
                formula,
                formula_values: &scene.formula_slots[*index].parameters,
                iterations: scene.formula_slots[*index].iterations,
                weight: scene.formula_slots[*index].weight,
                add_c_constant: scene.formula_slots[*index].add_c_constant,
                check_for_bailout: scene.formula_slots[*index].check_for_bailout,
                bailout: scene.formula_slots[*index].bailout,
            })
            .collect::<Vec<_>>();
        specialize_fpt_shader_hybrid(
            base_source,
            &slots,
            &sequence,
            &configured_values,
            scene.linear_de_offset,
            scene.force_delta_de,
            scene.delta_de_function,
            direct_hybrid_loop,
            formula_optimization,
        )
    } else {
        let (_, formula) = formulas
            .first()
            .ok_or_else(|| anyhow!("scene has no active formula"))?;
        specialize_fpt_shader(
            base_source,
            formula,
            &configured_values,
            &scene.formula_parameters,
            scene.force_delta_de,
            scene.delta_de_function,
        )
    }?;
    let mut specialized =
        specialize_mandelbulber_appearance(&specialized, scene, &formulas, &configured_values)?;
    if std::env::var_os("FPT_MANDEL_INTERACTIVE_REFINEMENT").is_some() {
        specialized = specialized.replace(
            "int(setv(cfg, 1)) * iteration_multiplier",
            "int(setv(cfg, 1) * mandelInteractiveIterationScale(cfg)) * iteration_multiplier",
        );
    }
    Ok(specialized)
}

pub fn scene_uses_kernel_specialization(scene: &Path) -> Result<bool> {
    const GENERIC_KERNEL_SCENES: [&str; 8] = [
        "320966913215facfa0a29140bc8c92f52ff3e279aaf784a55218aaf3daaf4278",
        "3e6f7128d974044502f3fc3b737ac875c27e0693f97735813df4a246e0fc1675",
        "5ff4df2a76abcd58c571c7b339d3f28ec17f46a0dbd3161e57ddb40db12089bc",
        "40950721cafd6dc33c16e33e2af7615bcde15b03443b9cbfb463725e94b0d288",
        "fa890bdca2819e30c56a72916d59bdc2d18c734a211ca6e9dc5ef3f41b236b08",
        "61fe4654d047367abb17648db1a840e0ad26c8da54e77ddc98869cb3954520a0",
        "7dec3e1eaf910d6628d1040eeb897dec5744731f64350427499dce4770dc5a52",
        "2c858c9b727bd3e33b29f5ca5cd435d2cfc50070bad704b288e002f1e4e1be1c",
    ];
    let digest = format!("{:x}", Sha256::digest(fs::read(scene)?));
    Ok(!GENERIC_KERNEL_SCENES.contains(&digest.as_str()))
}

pub fn scene_uses_direct_hybrid_loop(scene: &Path) -> Result<bool> {
    // These fixtures were selected from the exact full-corpus gate. They
    // retain identical PNGs and account for most of the measured benefit from
    // homogeneous hybrid lowering. Unknown scenes keep dynamic dispatch
    // because optimized Metal can otherwise reassociate sensitive orbits.
    const DIRECT_HYBRID_SCENES: [&str; 35] = [
        "f34b78bcb0632aeed0ddeed35a7667bc7a9e005556eb231ded3776d824d4d68c",
        "82d6eb83b73ef38aa8f25e59da066b3c67632464a87fe7781348864fccd115e8",
        "ab1db40d86b88decbd2d8032125155517572866d4798bdb6d57a280795094edb",
        "a4413dd119852ccd2ec9425ddcf85ba579e671e237592357d2e4104cc7c0546b",
        "ffe875b67e55be14d515de6c51d95ed6ea5568c9fd74f965ae07aebd475e2b24",
        "d73f243dd683d3ea7d2caec79f6bf820622826f32a5762dc65b912034d32a22e",
        "ccb98aff188f365af6e1d0560c1649f12a65ff1bed2e60e4e90d0bfea195ba68",
        "b1be9a5b2f77be6c767a728404b96f34f7badee60da75087ccacc9749c31210e",
        "9ac02395052160cd1c59f4ef7e48181d150d3111bbe26ca795b7162ff6bd61c5",
        "320966913215facfa0a29140bc8c92f52ff3e279aaf784a55218aaf3daaf4278",
        // pseudoKleinianMod4 rec; formula 217 dominates all ray phases.
        "bed33dd3dedcf0f26d4a65ff79db997dcac8306896cefa3cd0e37c6f0718f236",
        // Jos Leys Kleinian v3.
        "5c2d866f47b53820189f4781a5760bd0451a2cea810f97a5a8ab7a0bfce3a690",
        // MengerV4.
        "f01b919d638ef1f362c8ce7703b79e7379564b89ad1d247d5542d8f26b201f4b",
        // aboxMod13Surf.
        "c92324bb5ec7653b37c1c992b6b6bb0764544da191166a44b62437c461432437",
        // asurf4_sphere_invert.
        "9b43af7ca776dcb5b8fcc9b9d3629813e1bccfaa92f3280fdaa7939756cf02cc",
        // asurf4_worms.
        "4d74e93d606ab5d77264cb004cf449865448ae67ebdad5533c91fe339030cf69",
        // mandelbulb_pupuku pow2.
        "4ecdfeeb1b520a12bfe351944d175bb85cc469494278418278a2cc0a93d8ce8a",
        // mandelbulb_pupuku pow6.
        "50972b994a3ffef8ca2135ac6d848d1ee2133f7ff5929ca6578711e841d08f44",
        // pseudoKleinianMod5.
        "076a93d0c1f43832068832dfa9853671964959ff784745d0db58b87a29e75ee6",
        // transf_juliaBoxV2.
        "e66fac4f0495dbd39e65166be7d36d74b69e9b3ba4d0e5ec9d7cb50e6dcf3c69",
        // newtonPow3-delta-gnj-002h.
        "934637dee1bd7598763658ea45e4b0556702259b9408fdb8ba3cf24122c2589d",
        // DIFS Cylinder rocket.
        "ee8bebc32eecc3928defbdcc348ecce5b3760299b4c000327e7dd368fa2df8fe",
        // DIFS Cylinder tree.
        "734964f9d82044338cdb52f57f45250046cd1536d81bdbd368ddf306d832a3b6",
        // Koch_Ifs aaa1.
        "0ac95acb49303a0a95260d0863fce353ab9e854a84aef7c463a51669007e5b62",
        // MbulbAbsPow2_001.
        "09485d54387faaf499aae685fcce3781f2c169ce456ddee06dd474b1156a6ea2",
        // abox_donut4d_aa2.
        "95b2f85fdc974cd82d5861f385fb12699daee49e1872014329f8544f39550dd2",
        // mandelbarV3 ABa1.
        "e7677ace02d50a6a6bf8b83741e2f1942f13b173f3bb6c25841c1bb5f62a5062",
        // mandelnest.
        "5ddb30fc490615b107ffa31ed2b0d49731b25bf5c17f5b5143e4dc4fb988ae08",
        // mandelnest_full_001.
        "7b71548eb8085781da09350dc64eee3de2f71224f790ee9eb0ab23ec31264e45",
        // msltoe_sym3_mod4.
        "0ff40582231e20e8a7599f726d6b7c24a95d0f687a54f6803b93a88b4c5d4c43",
        // msltoe_sym3_mod5.
        "08de208334baef7bf756cac217fd6925aedf041d8dd5e6e4ec188b6b5b5792ad",
        // vicsek_001.
        "95fdef42b5ca86ea0f80749ef0ecabdee352940cfa2c51b1fca362b17a8af45e",
        // xenodreambuie_v3.
        "c1ce1c8fb25494834b1ccfb79d67ec4da84107e9240b4a815d6136edde60cb8b",
        // RoadToExascale.
        "98baaeb09ffaf586320bd07492fb72ed47f59bc504f5be3dc612db7fca2ed27e",
        // newtonPow3-delta-gnj-001b.
        "5fcd33fac8dc8824e39af00073185295e50f88e7f622aa0310a69668ec5a959c",
    ];
    let digest = format!("{:x}", Sha256::digest(fs::read(scene)?));
    Ok(DIRECT_HYBRID_SCENES.contains(&digest.as_str()))
}

pub fn scene_formula_optimization_policy(scene: &Path) -> Result<SceneFormulaOptimizationPolicy> {
    let digest = format!("{:x}", Sha256::digest(fs::read(scene)?));
    Ok(formula_optimization_policy_for_digest(&digest))
}

fn formula_optimization_policy_for_digest(digest: &str) -> SceneFormulaOptimizationPolicy {
    let periodic_hybrid_loop = matches!(
        digest,
        // DIFS Torus asurf
        "8bbd267428bb7fe674b4f84f549ff9af495850b27bcf12b118d7a27f187989d8"
            // abox_mod1_add
            | "aad1812e33004e8247bffc2004649d54d64f3eb4ab939b023e7e77f559fd0e9c"
            // pseudo kleinian abox13
            | "035ba82d0606cba849b7fe2e1d96896637813cc385590d3fdaee2f263f83656f"
            // hybrid001
            | "b9236c85374cc00782cc8a2e418a3a5aa7af67c82d15ea0c8924152b36638cfb"
    );
    let partial_evaluation_formula_id = match digest {
        // DIFS Torus asurf
        "8bbd267428bb7fe674b4f84f549ff9af495850b27bcf12b118d7a27f187989d8" => 132,
        // DIFS Box DiagV3 complex primitive
        "dc67ed9066cac84403ed28cea04458bf332bdc4e5df5ef118bad9ebd541c49cd" => 602,
        // ifs_xy
        "c1f596cb79c5566ef078fb5b719e919e85dd597e085c612d0339640d237e2bd7" => 150,
        _ => 0,
    };
    SceneFormulaOptimizationPolicy {
        periodic_hybrid_loop,
        unrolled_periodic_hybrid_loop: matches!(
            digest,
            // DIFS Torus asurf.
            "8bbd267428bb7fe674b4f84f549ff9af495850b27bcf12b118d7a27f187989d8"
                // KochV5_KochV5.
                | "3557e16f11a55869e375d95eb5ccd9b65925a4e9cb0fe1b4502fa687e3bf877f"
                // aboxMod11_addCpixelRotate.
                | "4e71d3331ff0bd2e4eb19ec7dbf797b1fd61b4394e887fe1c35b78ecd31d7cb1"
                // aboxMod15cpixelInvert.
                | "3f4ebaa58cc33dd68702510392f77a44edec69233702444346211be345fc4eae"
                // abox_mod1_add.
                | "aad1812e33004e8247bffc2004649d54d64f3eb4ab939b023e7e77f559fd0e9c"
                // boxFoldBulb_v2_twice.
                | "3b61a5878c14577092103862b76f5b2577293ec06311758231a47576a131508f"
                // pseudo kleinian abox13.
                | "035ba82d0606cba849b7fe2e1d96896637813cc385590d3fdaee2f263f83656f"
                // transfSphereInvV3_abxTetra_OT.
                | "e81bd1e26cc8983d00444e0270cbaa97b188abccdfe4da29780b44d96c05e6a5"
                // newtonPow3-rotfold-delta-gnj-003d.
                | "b4b23fe2d3c9812a19482a615c62e655c8e3004d2c275f302a0ea103f85b9066"
                // newtonPow3-rotfold-delta-gnj-010g.
                | "5091e3639526f98c98ebfdc243d288f64c8d2f51839720fd918ad17bc6458e5a"
                // hybrid001.
                | "b9236c85374cc00782cc8a2e418a3a5aa7af67c82d15ea0c8924152b36638cfb"
        ),
        partial_evaluation_formula_id,
        partial_evaluation_phases: partial_evaluation_formula_id != 0,
        partial_evaluation_scalarize_loops: partial_evaluation_formula_id != 0,
        partial_evaluation_dce: partial_evaluation_formula_id != 0,
        partial_evaluation_cse: partial_evaluation_formula_id != 0,
    }
}

fn material_bool(scene: &MandelbulberScene, key: &str, fallback: bool) -> Result<bool> {
    match scene.material.parameters.get(key).map(String::as_str) {
        None => Ok(fallback),
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(value) => bail!("invalid material boolean {key}: {value}"),
    }
}

fn material_scalar(scene: &MandelbulberScene, key: &str, fallback: f32) -> Result<f32> {
    scene
        .material
        .parameters
        .get(key)
        .map(|value| super::parse_number(value).map(|value| value as f32))
        .transpose()
        .with_context(|| format!("invalid material scalar {key}"))
        .map(|value| value.unwrap_or(fallback))
}

fn material_integer(scene: &MandelbulberScene, key: &str, fallback: i32) -> Result<i32> {
    let value = material_scalar(scene, key, fallback as f32)?;
    ensure!(
        value.is_finite() && value.fract() == 0.0,
        "material integer {key} must be integral"
    );
    Ok(value as i32)
}

fn material_vector<const N: usize>(
    scene: &MandelbulberScene,
    key: &str,
    fallback: [f32; N],
) -> Result<[f32; N]> {
    let Some(value) = scene.material.parameters.get(key) else {
        return Ok(fallback);
    };
    let values = value
        .split_whitespace()
        .map(|component| super::parse_number(component).map(|value| value as f32))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        values.len() == N || (N == 4 && values.len() == 3),
        "material vector {key} must have {N} components"
    );
    let mut result = fallback;
    result[..values.len()].copy_from_slice(&values);
    Ok(result)
}

fn metal_bool(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn coloring_function_id(name: &str) -> Result<i32> {
    Ok(match name {
        "coloringFunctionABox" => 0,
        "coloringFunctionIFS" => 1,
        "coloringFunctionAmazingSurf" => 2,
        "coloringFunctionDonut" => 3,
        "coloringFunctionDefault" => 4,
        "coloringFunctionUndefined" => 5,
        other => bail!("unsupported Mandelbulber coloring function {other}"),
    })
}

fn mandelbulber_coloring_constants(scene: &MandelbulberScene) -> Result<String> {
    let line = material_vector(
        scene,
        "mat1_fractal_coloring_line_direction",
        [1.0, 0.0, 0.0, 0.0],
    )?;
    let xyz_c = material_vector(scene, "mat1_fractal_coloring_xyzC_111", [1.0, 1.0, 1.0])?;
    let xyz = material_vector(scene, "mat1_fractal_coloring_xyz_000", [1.0, 1.0, 1.0])?;
    let booleans = [
        (
            "ExtraOptions",
            "mat1_fractal_coloring_extra_color_options_false",
            false,
        ),
        (
            "PreV215",
            "mat1_fractal_coloring_color_preV215_false",
            false,
        ),
        (
            "Color4d",
            "mat1_fractal_coloring_color_4D_enabled_false",
            false,
        ),
        (
            "Extra",
            "mat1_fractal_coloring_extra_color_enabled_false",
            false,
        ),
        (
            "Init",
            "mat1_fractal_coloring_init_cond_enabled_false",
            false,
        ),
        ("IcRad", "mat1_fractal_coloring_ic_rad_enabled_false", false),
        ("IcXyz", "mat1_fractal_coloring_ic_xyz_enabled_false", false),
        (
            "IcFabs",
            "mat1_fractal_coloring_ic_fabs_enabled_false",
            false,
        ),
        ("OrbitTrap", "mat1_fractal_coloring_orbit_trap_true", true),
        ("Aux", "mat1_fractal_coloring_aux_color_false", false),
        ("Rad", "mat1_fractal_coloring_rad_enabled_false", false),
        (
            "RadSquared",
            "mat1_fractal_coloring_rad_squared_enabled_false",
            false,
        ),
        (
            "RadDiv1e13",
            "mat1_fractal_coloring_rad_div_1e13_false",
            false,
        ),
        (
            "RadDivDe",
            "mat1_fractal_coloring_rad_div_de_enabled_false",
            false,
        ),
        (
            "RadDivDeSquared",
            "mat1_fractal_coloring_rad_div_de_squared_false",
            false,
        ),
        (
            "RadDivDe1e13",
            "mat1_fractal_coloring_rad_div_de_1e13_false",
            false,
        ),
        (
            "XyzBias",
            "mat1_fractal_coloring_xyz_bias_enabled_false",
            false,
        ),
        (
            "XyzFabs",
            "mat1_fractal_coloring_xyz_fabs_enabled_false",
            false,
        ),
        (
            "XyzDiv1e13",
            "mat1_fractal_coloring_xyz_div_1e13_false",
            false,
        ),
        (
            "XyzXSquared",
            "mat1_fractal_coloring_xyz_x_sqrd_enabled_false",
            false,
        ),
        (
            "XyzYSquared",
            "mat1_fractal_coloring_xyz_y_sqrd_enabled_false",
            false,
        ),
        (
            "XyzZSquared",
            "mat1_fractal_coloring_xyz_z_sqrd_enabled_false",
            false,
        ),
        (
            "IterGroup",
            "mat1_fractal_coloring_iter_group_enabled_false",
            false,
        ),
        (
            "IterAdd",
            "mat1_fractal_coloring_iter_add_scale_enabled_true",
            true,
        ),
        (
            "IterScale",
            "mat1_fractal_coloring_iter_scale_enabled_false",
            false,
        ),
        (
            "GlobalPalette",
            "mat1_fractal_coloring_global_palette_false",
            false,
        ),
        ("Add", "mat1_fractal_coloring_add_enabled_false", false),
        (
            "Parabola",
            "mat1_fractal_coloring_parab_enabled_false",
            false,
        ),
        ("Cosine", "mat1_fractal_coloring_cos_enabled_false", false),
        ("Round", "mat1_fractal_coloring_round_enabled_false", false),
    ];
    let scalars = [
        ("SphereRadius", "mat1_fractal_coloring_sphere_radius", 1.0),
        (
            "HybridAuxScale",
            "mat1_fractal_coloring_aux_color_scale1",
            1.0,
        ),
        (
            "HybridOrbitScale",
            "mat1_fractal_coloring_orbit_trap_scale1",
            1.0,
        ),
        (
            "HybridRadDeScale",
            "mat1_fractal_coloring_rad_div_de_scale1",
            1.0,
        ),
        ("Initial", "mat1_fractal_coloring_initial_color_value", 0.0),
        ("IcRadWeight", "mat1_fractal_coloring_ic_rad_weight", 1.0),
        (
            "OrbitWeight",
            "mat1_fractal_coloring_orbit_trap_weight",
            1.0,
        ),
        ("AuxWeight", "mat1_fractal_coloring_aux_color_weight", 1.0),
        (
            "AuxHybridWeight",
            "mat1_fractal_coloring_aux_color_hybrid_weight",
            0.0,
        ),
        ("RadWeight", "mat1_fractal_coloring_rad_weight", 1.0),
        (
            "RadDeWeight",
            "mat1_fractal_coloring_rad_div_de_weight",
            1.0,
        ),
        ("XyzIterScale", "mat1_fractal_coloring_xyz_iter_scale", 0.0),
        ("IterAddScale", "mat1_fractal_coloring_iter_add_scale", 1.0),
        ("IterScaleValue", "mat1_fractal_coloring_iter_scale", 0.0),
        ("AddMax", "mat1_fractal_coloring_add_max", 1.0),
        ("AddSpread", "mat1_fractal_coloring_add_spread", 1.0),
        ("AddStart", "mat1_fractal_coloring_add_start_value", 0.0),
        ("ParabolaScale", "mat1_fractal_coloring_parab_scale", 1.0),
        (
            "ParabolaStart",
            "mat1_fractal_coloring_parab_start_value",
            0.0,
        ),
        ("CosineAdd", "mat1_fractal_coloring_cos_add", 1.0),
        ("CosinePeriod", "mat1_fractal_coloring_cos_period", 1.0),
        ("CosineStart", "mat1_fractal_coloring_cos_start_value", 0.0),
        ("RoundScale", "mat1_fractal_coloring_round_scale", 1.0),
        ("Minimum", "mat1_fractal_coloring_min_color_value", 0.0),
        ("Maximum", "mat1_fractal_coloring_max_color_value", 1.0e6),
    ];
    let mut source = format!(
        "constant int kMandelColorAlgorithm = {};\nconstant int kMandelColorIterationStart = {};\n",
        material_integer(scene, "mat1_fractal_coloring_algorithm", 0)?,
        material_integer(scene, "mat1_fractal_coloring_i_start_value", 0)?,
    );
    for (name, key, fallback) in booleans {
        source.push_str(&format!(
            "constant bool kMandelColor{name} = {};\n",
            metal_bool(material_bool(scene, key, fallback)?)
        ));
    }
    for (name, key, fallback) in scalars {
        source.push_str(&format!(
            "constant float kMandelColor{name} = {};\n",
            metal_float(material_scalar(scene, key, fallback)?)
        ));
    }
    source.push_str(&format!(
        "constant float4 kMandelColorLine = float4({}, {}, {}, {});\nconstant float3 kMandelColorInitialXyz = float3({}, {}, {});\nconstant float3 kMandelColorFinalXyz = float3({}, {}, {});\n",
        metal_float(line[0]), metal_float(line[1]), metal_float(line[2]), metal_float(line[3]),
        metal_float(xyz_c[0]), metal_float(xyz_c[1]), metal_float(xyz_c[2]),
        metal_float(xyz[0]), metal_float(xyz[1]), metal_float(xyz[2]),
    ));
    Ok(source)
}

fn mandelbulber_calculate_color_source() -> &'static str {
    r#"static float mandelbulberColorOrbitValue(float4 z, float4 initial) {
    float4 color_z = z;
    if (!kMandelColorColor4d) color_z.w = 0.0f;
    switch (kMandelColorAlgorithm) {
        case 0: return length(color_z);
        case 1: return abs(dot(initial, color_z));
        case 2: return abs(length(color_z - initial) - kMandelColorSphereRadius);
        case 3: {
            float value = min(abs(color_z.x), min(abs(color_z.y), abs(color_z.z)));
            return kMandelColorColor4d ? min(value, abs(color_z.w)) : value;
        }
        case 4: return abs(dot(kMandelColorLine, color_z));
        default: return length(color_z);
    }
}

static float mandelbulberCalculateColorIndex(float4 initial,
                                              float4 z,
                                              MandelOrbitState aux,
                                              float color_min,
                                              bool hybrid,
                                              int coloring_function,
                                              float mandelbox_factor_r) {
    if (kMandelColorExtra) {
        float color_value = kMandelColorInitial;
        if (kMandelColorInit) {
            float3 initial_xyz = initial.xyz;
            if (kMandelColorIcRad) {
                color_value += length(initial_xyz) * kMandelColorIcRadWeight;
            }
            if (kMandelColorIcXyz) {
                initial_xyz = kMandelColorIcFabs
                    ? initial_xyz * kMandelColorInitialXyz
                    : abs(initial_xyz) * kMandelColorInitialXyz;
                color_value += initial_xyz.x + initial_xyz.y + initial_xyz.z;
            }
        }
        if (kMandelColorOrbitTrap) {
            color_value += color_min * kMandelColorOrbitWeight;
        }
        if (kMandelColorAux) {
            color_value += aux.color * kMandelColorAuxWeight
                         + aux.colorHybrid * kMandelColorAuxHybridWeight;
        }
        if (kMandelColorRad) {
            float radius = aux.r;
            if (kMandelColorRadDiv1e13) radius /= 1.0e13f;
            if (kMandelColorRadSquared) radius *= radius;
            color_value += radius * kMandelColorRadWeight;
        }
        if (kMandelColorRadDivDe) {
            float radius_de = aux.r;
            if (kMandelColorRadDivDe1e13) radius_de /= 1.0e13f;
            if (kMandelColorRadDivDeSquared) radius_de *= radius_de;
            radius_de /= aux.DE;
            color_value += radius_de * kMandelColorRadDeWeight;
        }
        if (kMandelColorXyzBias) {
            float3 final_xyz = z.xyz;
            if (kMandelColorXyzDiv1e13) final_xyz /= 1.0e13f;
            final_xyz = kMandelColorXyzFabs
                ? final_xyz * kMandelColorFinalXyz
                : abs(final_xyz) * kMandelColorFinalXyz;
            if (kMandelColorXyzXSquared) final_xyz.x *= final_xyz.x;
            if (kMandelColorXyzYSquared) final_xyz.y *= final_xyz.y;
            if (kMandelColorXyzZSquared) final_xyz.z *= final_xyz.z;
            color_value += (final_xyz.x + final_xyz.y + final_xyz.z)
                         * (1.0f + kMandelColorXyzIterScale * float(aux.i));
        }
        if (kMandelColorIterGroup) {
            int iteration = int(aux.i);
            if (kMandelColorIterAdd && iteration > kMandelColorIterationStart) {
                color_value += kMandelColorIterAddScale
                             * float(iteration - kMandelColorIterationStart);
            }
            if (kMandelColorIterScale && iteration >= kMandelColorIterationStart) {
                color_value *= float(iteration - kMandelColorIterationStart)
                             * kMandelColorIterScaleValue + 1.0f;
            }
        }
        if (kMandelColorGlobalPalette) {
            if (kMandelColorAdd && color_value > kMandelColorAddStart) {
                color_value += (1.0f - 1.0f / (1.0f
                    + (color_value - kMandelColorAddStart) / kMandelColorAddSpread))
                    * kMandelColorAddMax;
            }
            if (kMandelColorParabola && color_value > kMandelColorParabolaStart) {
                // Mandelbulber intentionally uses the cosine start value in
                // this historic parabola expression.
                float parabola = color_value - kMandelColorCosineStart;
                color_value += parabola * parabola * kMandelColorParabolaScale;
            }
            if (kMandelColorCosine && color_value > kMandelColorCosineStart) {
                color_value += (0.5f - 0.5f * cos(
                    (color_value - kMandelColorCosineStart) * pi
                    / (kMandelColorCosinePeriod * 2.0f))) * kMandelColorCosineAdd;
            }
            if (kMandelColorRound) {
                color_value = round(color_value / kMandelColorRoundScale)
                            * kMandelColorRoundScale;
            }
        }
        return clamp(color_value, kMandelColorMinimum, kMandelColorMaximum) * 256.0f;
    }
    if (hybrid) {
        float orbit = min(100.0f, color_min);
        float radius_de = min(aux.r / abs(aux.DE), 20.0f);
        if (!kMandelColorExtraOptions) {
            return orbit * 1000.0f + aux.color * 100.0f + radius_de * 5000.0f;
        }
        return orbit * 1000.0f * kMandelColorHybridOrbitScale
             + aux.color * 100.0f * kMandelColorHybridAuxScale
             + radius_de * 5000.0f * kMandelColorHybridRadDeScale;
    }
    switch (coloring_function) {
        case 0:
            return aux.color * 100.0f + aux.r * mandelbox_factor_r / 1.0e13f
                 + (kMandelColorAlgorithm != 0 ? color_min * 1000.0f : 0.0f);
        case 1: return color_min * 1000.0f;
        case 2: return color_min * 200.0f;
        case 3: return aux.color * 2000.0f / max(float(aux.i), 1.0f);
        case 4: return color_min * 5000.0f;
        default: return 0.0f;
    }
}
"#
}

fn mandelbulber_palette_source(scene: &MandelbulberScene) -> String {
    let stops = scene
        .material
        .surface_gradient
        .iter()
        .map(|stop| {
            format!(
                "    float4({}, {}, {}, {})",
                metal_float(stop.position),
                metal_float(stop.color[0]),
                metal_float(stop.color[1]),
                metal_float(stop.color[2])
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let base = scene.material.surface_color.map(metal_float);
    format!(
        r#"constant uint kMandelSurfaceGradientCount = {count}u;
constant float4 kMandelSurfaceGradient[{count}] = {{
{stops}
}};

static float3 mandelbulberSurfaceGradient(float position) {{
    uint lower = 0u;
    uint upper = kMandelSurfaceGradientCount - 1u;
    while (lower + 1u < upper) {{
        uint middle = (lower + upper) / 2u;
        if (position > kMandelSurfaceGradient[middle].x) lower = middle;
        else upper = middle;
    }}
    float4 first = kMandelSurfaceGradient[lower];
    float4 second = kMandelSurfaceGradient[upper];
    float amount = clamp((position - first.x) / max(second.x - first.x, 1.0e-20f),
                         0.0f, 1.0f);
    return mix(first.yzw, second.yzw, amount);
}}

static float mandelbulberGeneratedColorCoordinate(
    float3 p, constant FptRenderConfig &cfg) {{
    float color_index = mandelbulberGeneratedColorIndex(
        mandelbulberGlobalPoint(p, cfg), cfg);
    return fmod(abs(color_index), 248.0f * 256.0f) / (248.0f * 256.0f);
}}

static float mandelbulberGeneratedPalettePosition(
    float3 p, constant FptRenderConfig &cfg) {{
    float color_index = mandelbulberGeneratedColorIndex(
        mandelbulberGlobalPoint(p, cfg), cfg);
    float wrapped = fmod(abs(color_index), 248.0f * 256.0f);
    return fmod(wrapped / 256.0f / 10.0f * {speed} + {offset}, 1.0f);
}}

static Material mandelbulberGeneratedMaterial(float3 p,
                                               constant FptRenderConfig &cfg) {{
    Material material = defaultMaterial();
    float3 fixed_color = float3({base_r}, {base_g}, {base_b});
    if ({use_palette} && {surface_gradient}) {{
        float position = mandelbulberGeneratedPalettePosition(p, cfg);
        material.rgb = mandelbulberSurfaceGradient(position);
    }} else {{
        material.rgb = fixed_color;
    }}
    material.roughness = cfg.fractal_style[4];
    material.specular = cfg.fractal_style[5];
    material.emission = cfg.fractal_style[6];
    return material;
}}
"#,
        count = scene.material.surface_gradient.len(),
        base_r = base[0],
        base_g = base[1],
        base_b = base[2],
        use_palette = metal_bool(scene.material.use_colors_from_palette),
        surface_gradient = metal_bool(scene.material.surface_gradient_enabled),
        speed = metal_float(scene.material.coloring_speed as f32),
        offset = metal_float(scene.material.palette_offset as f32),
    )
}

fn standalone_color_index_source(
    scene: &MandelbulberScene,
    formula: &ParsedFormula,
    set_values: &[f32; 40],
) -> Result<String> {
    let actual_scale = if formula.source.internal_name == "kaleidoscopic_ifs" {
        "kMandelFormulaParameters.IFS.scale"
    } else {
        "1.0f"
    };
    let post_iteration = constant_addition_source(formula.source.id, set_values);
    let coloring_function = coloring_function_id(&formula.source.coloring_function)?;
    let mandelbox_factor_r = formula_scalar(&scene.formula_parameters, "mandelbox_color_R", 0.0)?;
    let check_for_bailout = scene.formula_slots[0].check_for_bailout;
    Ok(format!(
        r#"static float mandelbulberGeneratedColorIndex(float3 p,
                                                constant FptRenderConfig &cfg) {{
    float world_scale = max(setv(cfg, 0), 1.0f);
    float3 scaled = p / world_scale;
    float4 z = float4(scaled.x, scaled.z, scaled.y, {initial_w});
    float4 initial = z;
    MandelOrbitState aux = {{}};
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
    float color_min = 1000.0f;
    float bailout = max(setv(cfg, 2), 1.0f);
    int max_iterations = clamp(int(setv(cfg, 1)) * 4, 1, 4096);
    for (int iteration = 0; iteration < max_iterations; ++iteration) {{
        aux.i = uint(iteration);
        aux.old_z = z;
        z = mandelApplyGlobalFoldings(z, cfg, aux);
        z = {function_name}(z, kMandelFormulaParameters, aux);
{post_iteration}        aux.r = length(z);
        if ({check_for_bailout}) {{
            float orbit_value = mandelbulberColorOrbitValue(z, initial);
            if (!kMandelColorPreV215) {{
                if ({formula_id} != 8) {{
                    color_min = min(color_min, orbit_value);
                    if (aux.r > bailout) break;
                    if ({additional_bailout} != 0
                        && length(z - aux.old_z) / aux.r < 1.0e-15f) break;
                }} else if (kMandelColorAlgorithm == 0) {{
                    if (aux.r > 1.0e15f
                        || length(z - aux.old_z) / aux.r < 1.0e-15f) break;
                }} else {{
                    color_min = min(color_min, orbit_value);
                    if (aux.r > bailout
                        || length(z - aux.old_z) / aux.r < 1.0e-15f) break;
                }}
            }} else if ({formula_id} != 8) {{
                color_min = min(color_min, orbit_value);
                if (aux.r > 1.0e15f
                    || length(z - aux.old_z) / aux.r < 1.0e-15f) break;
            }} else if (aux.r > 1.0e15f
                       || length(z - aux.old_z) / aux.r < 1.0e-15f) {{
                break;
            }}
        }}
    }}
    return mandelbulberCalculateColorIndex(
        initial, z, aux, color_min, false, {coloring_function}, {mandelbox_factor_r});
}}
"#,
        initial_w = metal_float(set_values[3]),
        function_name = formula.function_name,
        check_for_bailout = metal_bool(check_for_bailout),
        formula_id = formula.source.id,
        additional_bailout = i32::from(uses_additional_bailout(formula)),
        mandelbox_factor_r = metal_float(mandelbox_factor_r),
    ))
}

fn hybrid_color_index_source(
    scene: &MandelbulberScene,
    formulas: &[(usize, ParsedFormula)],
    set_values: &[f32; 40],
) -> Result<String> {
    let sequence = scene.hybrid_coloring_sequence()?;
    ensure!(
        !sequence.is_empty(),
        "hybrid coloring sequence cannot be empty"
    );
    let sequence_literal = sequence
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let mut dispatch = String::new();
    let mut formula_ids = [0_i32; 9];
    for (index, formula) in formulas {
        let namespace = format!("MandelSlot{index}");
        dispatch.push_str(&format!(
            "            case {index}:\n                if (formula_weight > 0.0f) z = {namespace}::{function_name}(z, {namespace}::kMandelFormulaParameters, aux);\n",
            function_name = formula.function_name,
        ));
        dispatch.push_str(&hybrid_constant_addition_source(
            scene.formula_slots[*index].add_c_constant,
            formula.source.id,
            set_values,
        ));
        dispatch.push_str("                break;\n");
        formula_ids[*index] = formula.source.id;
    }
    let formula_ids = formula_ids.map(|value| value.to_string()).join(", ");
    Ok(format!(
        r#"constant uchar kMandelHybridColorSequence[{sequence_length}] = {{ {sequence_literal} }};
constant int kMandelHybridFormulaIds[9] = {{ {formula_ids} }};

static float mandelbulberGeneratedColorIndex(float3 p,
                                                constant FptRenderConfig &cfg) {{
    float world_scale = max(setv(cfg, 0), 1.0f);
    float3 scaled = p / world_scale;
    float4 z = float4(scaled.x, scaled.z, scaled.y, {initial_w});
    float4 initial = z;
    MandelOrbitState aux = {{}};
    aux.c = z;
    aux.const_c = z;
    aux.old_z = z;
    aux.pos_neg = 1.0f;
    aux.r = length(z);
    aux.DE = 1.0f;
    aux.DE0 = 0.0f;
    aux.dist = 1000.0f;
    aux.pseudoKleinianDE = 1.0f;
    aux.actualScale = 1.0f;
    aux.actualScaleA = 0.0f;
    aux.color = 1.0f;
    aux.colorHybrid = 0.0f;
    aux.temp1000 = 1000.0f;
    float color_min = 1000.0f;
    for (int iteration = 0; iteration < {sequence_length}; ++iteration) {{
        int sequence_index = int(kMandelHybridColorSequence[iteration]);
        float formula_weight = kMandelHybridWeights[sequence_index];
        aux.i = uint(iteration);
        aux.old_z = z;
        z = mandelApplyGlobalFoldings(z, cfg, aux);
        float4 previous_z = z;
        float previous_de = aux.DE;
        float previous_color = aux.color;
        switch (sequence_index) {{
{dispatch}        }}
        if (formula_weight < 1.0f) {{
            z = mandelHybridSmoothVector(previous_z, z, formula_weight);
            float inverse_weight = 1.0f - formula_weight;
            aux.DE = aux.DE * formula_weight + previous_de * inverse_weight;
            aux.color = aux.color * formula_weight + previous_color * inverse_weight;
        }}
        aux.r = length(z);
        if (kMandelHybridBailoutChecks[sequence_index] != 0) {{
            float orbit_value = mandelbulberColorOrbitValue(z, initial);
            float bailout = kMandelHybridBailouts[sequence_index];
            int formula_id = kMandelHybridFormulaIds[sequence_index];
            if (!kMandelColorPreV215) {{
                if (formula_id != 8) {{
                    color_min = min(color_min, orbit_value);
                    if (aux.r > bailout) break;
                    if (kMandelHybridAdditionalBailoutChecks[sequence_index] != 0
                        && length(z - aux.old_z) / aux.r < 1.0e-15f) break;
                }} else if (kMandelColorAlgorithm == 0) {{
                    if (aux.r > 1.0e15f
                        || length(z - aux.old_z) / aux.r < 1.0e-15f) break;
                }} else {{
                    color_min = min(color_min, orbit_value);
                    if (aux.r > bailout
                        || length(z - aux.old_z) / aux.r < 1.0e-15f) break;
                }}
            }} else if (formula_id != 8) {{
                color_min = min(color_min, orbit_value);
                if (aux.r > 1.0e15f
                    || length(z - aux.old_z) / aux.r < 1.0e-15f) break;
            }} else if (aux.r > 1.0e15f
                       || length(z - aux.old_z) / aux.r < 1.0e-15f) {{
                break;
            }}
        }}
    }}
    return mandelbulberCalculateColorIndex(
        initial, z, aux, color_min, true, 4, 0.0f);
}}
"#,
        sequence_length = sequence.len(),
        initial_w = metal_float(set_values[3]),
    ))
}

fn boolean_formula_color_evaluator_source(
    scene: &MandelbulberScene,
    index: usize,
    formula: &ParsedFormula,
    set_values: &[f32; 40],
) -> Result<String> {
    let namespace = format!("MandelBooleanSlot{index}");
    let actual_scale = if formula.source.internal_name == "kaleidoscopic_ifs" {
        format!("{namespace}::kMandelFormulaParameters.IFS.scale")
    } else {
        "1.0f".to_owned()
    };
    let post_iteration = hybrid_constant_addition_source(
        scene.formula_slots[index].add_c_constant,
        formula.source.id,
        set_values,
    );
    let coloring_function = coloring_function_id(&formula.source.coloring_function)?;
    let mandelbox_factor_r = formula_scalar(
        &scene.formula_slots[index].parameters,
        "mandelbox_color_R",
        0.0,
    )?;
    Ok(format!(
        r#"static float mandelBooleanColorIndex{index}(float3 point,
                                              constant FptRenderConfig &cfg) {{
    float4 z = float4(point, {initial_w});
    float4 initial = z;
    MandelOrbitState aux = {{}};
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
    float color_min = 1000.0f;
    float bailout = {bailout};
    int max_iterations = clamp(int(setv(cfg, 1)) * 4, 1, 4096);
    for (int iteration = 0; iteration < max_iterations; ++iteration) {{
        aux.i = uint(iteration);
        aux.old_z = z;
        z = mandelApplyGlobalFoldings(z, cfg, aux);
        z = {namespace}::{function_name}(
            z, {namespace}::kMandelFormulaParameters, aux);
{post_iteration}        aux.r = length(z);
        if ({check_for_bailout}) {{
            float orbit_value = mandelbulberColorOrbitValue(z, initial);
            if (!kMandelColorPreV215) {{
                if ({formula_id} != 8) {{
                    color_min = min(color_min, orbit_value);
                    if (aux.r > bailout) break;
                    if ({additional_bailout} != 0
                        && length(z - aux.old_z) / aux.r < 1.0e-15f) break;
                }} else if (kMandelColorAlgorithm == 0) {{
                    if (aux.r > 1.0e15f
                        || length(z - aux.old_z) / aux.r < 1.0e-15f) break;
                }} else {{
                    color_min = min(color_min, orbit_value);
                    if (aux.r > bailout
                        || length(z - aux.old_z) / aux.r < 1.0e-15f) break;
                }}
            }} else if ({formula_id} != 8) {{
                color_min = min(color_min, orbit_value);
                if (aux.r > 1.0e15f
                    || length(z - aux.old_z) / aux.r < 1.0e-15f) break;
            }} else if (aux.r > 1.0e15f
                       || length(z - aux.old_z) / aux.r < 1.0e-15f) {{
                break;
            }}
        }}
    }}
    return mandelbulberCalculateColorIndex(
        initial, z, aux, color_min, false, {coloring_function}, {mandelbox_factor_r});
}}
"#,
        initial_w = metal_float(scene.initial_waxis as f32),
        bailout = metal_float(scene.formula_slots[index].bailout as f32),
        function_name = formula.function_name,
        check_for_bailout = metal_bool(scene.formula_slots[index].check_for_bailout),
        formula_id = formula.source.id,
        additional_bailout = i32::from(uses_additional_bailout(formula)),
        mandelbox_factor_r = metal_float(mandelbox_factor_r),
    ))
}

fn boolean_color_index_source(
    scene: &MandelbulberScene,
    formulas: &[(usize, ParsedFormula)],
    set_values: &[f32; 40],
) -> Result<String> {
    ensure!(!formulas.is_empty(), "boolean coloring requires a formula");
    let mut evaluators = String::new();
    let mut transforms = Vec::new();
    let mut selections = Vec::new();
    let mut color_cases = Vec::new();
    for (formula_order, (index, formula)) in formulas.iter().enumerate() {
        evaluators.push_str(&boolean_formula_color_evaluator_source(
            scene, *index, formula, set_values,
        )?);
        let position = scene.formula_positions[*index].map(|value| value as f32);
        let rotation = rotation2_matrix(
            scene.formula_rotations[*index].map(|value| value.to_radians() as f32),
        );
        let repeat = scene.formula_repeats[*index].map(|value| value as f32);
        let scale = boolean_point_scale(scene.formula_scales[*index]);
        transforms.push(format!(
            r#"    float3 point{index} = mandelBooleanTransformPoint(
        point, float3({px}, {py}, {pz}), {rotation},
        float3({rx}, {ry}, {rz}), {scale});
    float distance{index} = mandelBooleanDistance{index}(point{index}, cfg) / abs({scale});"#,
            px = metal_float(position[0]),
            py = metal_float(position[1]),
            pz = metal_float(position[2]),
            rotation = row_matrix_literal(rotation),
            rx = metal_float(repeat[0]),
            ry = metal_float(repeat[1]),
            rz = metal_float(repeat[2]),
            scale = metal_float(scale),
        ));
        if formula_order > 0 {
            let operation = scene.boolean_operators[index - 1];
            let candidate = if operation == 2 {
                format!("-distance{index}")
            } else {
                format!("distance{index}")
            };
            let comparison = match operation {
                0 | 2 => ">",
                1 => "<",
                other => bail!("unsupported boolean operator {other}"),
            };
            selections.push(format!(
                "    if ({candidate} {comparison} selected_distance) {{ selected_distance = {candidate}; selected_formula = {index}; }}"
            ));
        }
        color_cases.push(format!(
            "        case {index}: return mandelBooleanColorIndex{index}(point{index}, cfg);"
        ));
    }
    let first_index = formulas[0].0;
    Ok(format!(
        r#"{evaluators}
static float mandelbulberGeneratedColorIndex(float3 p,
                                                constant FptRenderConfig &cfg) {{
    float world_scale = max(setv(cfg, 0), 1.0f);
    float3 scaled = p / world_scale;
    float3 point = float3(scaled.x, scaled.z, scaled.y);
{transforms}
    int selected_formula = {first_index};
    float selected_distance = distance{first_index};
{selections}
    switch (selected_formula) {{
{color_cases}
        default: return 0.0f;
    }}
}}
"#,
        transforms = transforms.join("\n"),
        selections = selections.join("\n"),
        color_cases = color_cases.join("\n"),
    ))
}

fn specialize_mandelbulber_appearance(
    base_source: &str,
    scene: &MandelbulberScene,
    formulas: &[(usize, ParsedFormula)],
    set_values: &[f32; 40],
) -> Result<String> {
    let marker = "// FPT_MANDELBULBER_GENERATED_INSERTION_POINT";
    ensure!(
        base_source.contains(marker),
        "Metal shader is missing {marker}"
    );
    let constants = mandelbulber_coloring_constants(scene)?;
    let color_index = if scene.boolean_enabled {
        boolean_color_index_source(scene, formulas, set_values)?
    } else if scene.hybrid_enabled {
        hybrid_color_index_source(scene, formulas, set_values)?
    } else {
        let (_, formula) = formulas
            .first()
            .ok_or_else(|| anyhow!("scene has no formula for Mandelbulber coloring"))?;
        standalone_color_index_source(scene, formula, set_values)?
    };
    let palette = mandelbulber_palette_source(scene);
    let fragment = format!(
        r#"#define FPT_MANDEL_GENERATED_MATERIAL 1
{constants}

{color_calculation}

{color_index}

{palette}

{marker}"#,
        color_calculation = mandelbulber_calculate_color_source(),
    );
    Ok(base_source.replacen(marker, &fragment, 1))
}

fn specialize_fpt_shader_boolean(
    base_source: &str,
    formulas: &[(usize, ParsedFormula)],
    scene: &MandelbulberScene,
    set_values: &[f32; 40],
) -> Result<String> {
    ensure!(
        !scene.force_delta_de,
        "forced delta-DE for boolean scenes is not supported yet"
    );
    let marker = "// FPT_MANDELBULBER_GENERATED_INSERTION_POINT";
    let mut declarations = String::new();
    let mut evaluators = String::new();
    let mut calls = Vec::new();
    for (index, formula) in formulas {
        ensure!(
            supports_runtime_emitter(formula),
            "boolean formula {} requires a standalone distance finalizer",
            formula.source.symbol
        );
        let namespace = format!("MandelBooleanSlot{index}");
        declarations.push_str(&emit_formula_namespace(
            formula,
            &scene.formula_slots[*index].parameters,
            "constant",
            &namespace,
            false,
            FormulaSourceOptimization::default(),
        )?);
        declarations.push_str("\n\nnamespace ");
        declarations.push_str(&namespace);
        declarations.push_str(" {\n");
        declarations.push_str(&formula_constant_initializer(
            formula,
            &scene.formula_slots[*index].parameters,
        )?);
        declarations.push_str("\n}\n\n");
        let actual_scale = if formula.source.internal_name == "kaleidoscopic_ifs" {
            format!("{namespace}::kMandelFormulaParameters.IFS.scale")
        } else {
            "1.0f".into()
        };
        let distance_expression = analytic_distance_expression(&formula.source.analytic_function)?
            .replace(
                "kMandelFormulaParameters",
                &format!("{namespace}::kMandelFormulaParameters"),
            );
        let post_iteration = hybrid_constant_addition_source(
            scene.formula_slots[*index].add_c_constant,
            formula.source.id,
            set_values,
        );
        evaluators.push_str(&format!(
            r#"static float mandelBooleanDistance{index}(float3 point,
                                            constant FptRenderConfig &cfg) {{
    float4 z = float4(point, {initial_w});
    MandelOrbitState aux = {{}};
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
    int max_iterations = clamp(int(setv(cfg, 1)), 1, 4096);
    float bailout = {bailout};
    for (int iteration = 0; iteration < max_iterations; ++iteration) {{
        aux.i = uint(iteration);
        aux.old_z = z;
        z = mandelApplyGlobalFoldings(z, cfg, aux);
        z = {namespace}::{function_name}(z, {namespace}::kMandelFormulaParameters, aux);
{post_iteration}        aux.r = length(z);
        if (aux.r > bailout) break;
    }}
    return max(float({distance_expression}), 0.0f);
}}

"#,
            initial_w = metal_float(scene.initial_waxis as f32),
            bailout = metal_float(scene.formula_slots[*index].bailout as f32),
            function_name = formula.function_name,
        ));
        let position = scene.formula_positions[*index].map(|value| value as f32);
        let rotation = rotation2_matrix(
            scene.formula_rotations[*index].map(|value| value.to_radians() as f32),
        );
        let repeat = scene.formula_repeats[*index].map(|value| value as f32);
        // The settings file stores an object-space size. Mandelbulber's
        // sParamRender converts it to a point-transform scale before the
        // boolean evaluator sees it.
        let scale = boolean_point_scale(scene.formula_scales[*index]);
        calls.push(format!(
            r#"    float3 point{index} = mandelBooleanTransformPoint(
        point,
        float3({px}, {py}, {pz}),
        {rotation},
        float3({rx}, {ry}, {rz}),
        {scale});
    float distance{index} = mandelBooleanDistance{index}(point{index}, cfg) / abs({scale});"#,
            px = metal_float(position[0]),
            py = metal_float(position[1]),
            pz = metal_float(position[2]),
            rotation = row_matrix_literal(rotation),
            rx = metal_float(repeat[0]),
            ry = metal_float(repeat[1]),
            rz = metal_float(repeat[2]),
            scale = metal_float(scale),
        ));
    }
    ensure!(!calls.is_empty(), "boolean scene has no active formulas");
    let mut combination = String::new();
    combination.push_str(&calls[0]);
    combination.push_str("\n    float distance = distance0;\n");
    for ((index, _), call) in formulas.iter().skip(1).zip(calls.iter().skip(1)) {
        combination.push_str(call);
        combination.push('\n');
        let operation = match scene.boolean_operators[index - 1] {
            0 => format!("    distance = max(distance, distance{index});\n"),
            1 => format!("    distance = min(distance, distance{index});\n"),
            2 => format!("    distance = max(distance, -distance{index});\n"),
            operator => bail!(
                "unsupported boolean operator {operator} for slot {}",
                index + 1
            ),
        };
        combination.push_str(&operation);
    }
    let specialized_kernel_define = mandel_specialized_kernel_define(base_source);
    let fragment = format!(
        r#"#define FPT_MANDEL_GENERATED_FIELD 1
{specialized_kernel_define}
{orbit_state}

{declarations}
struct MandelBooleanMatrix {{
    float3 m1;
    float3 m2;
    float3 m3;
}};

static float mandelBooleanRepeat(float value, float period) {{
    if (period <= 0.0f) return value;
    return fmod(fmod(value - period * 0.5f, period) + period, period) - period * 0.5f;
}}

static float3 mandelBooleanTransformPoint(float3 point,
                                          float3 position,
                                          MandelBooleanMatrix rotation,
                                          float3 repeat,
                                          float scale) {{
    float3 translated = point - position;
    float3 rotated = float3(dot(translated, rotation.m1),
                            dot(translated, rotation.m2),
                            dot(translated, rotation.m3));
    rotated = float3(mandelBooleanRepeat(rotated.x, repeat.x),
                     mandelBooleanRepeat(rotated.y, repeat.y),
                     mandelBooleanRepeat(rotated.z, repeat.z));
    return rotated * scale;
}}

{evaluators}
static MandelFormulaIterationCounts mandelbulberProfileFormulaIterations(
    int completed_iterations) {{
    (void)completed_iterations;
    MandelFormulaIterationCounts counts = {{}};
    return counts;
}}

static float4 mandelbulberGeneratedFieldSample(float3 p,
                                                constant FptRenderConfig &cfg,
                                                int iteration_multiplier,
                                                int iteration_budget) {{
    (void)iteration_multiplier;
    (void)iteration_budget;
    float world_scale = max(setv(cfg, 0), 1.0f);
    float3 scaled = p / world_scale;
    float3 point = float3(scaled.x, scaled.z, scaled.y);
{combination}
    return float4(distance * world_scale, 0.0f, 0.0f, 0.0f);
}}

{marker}"#,
        orbit_state = orbit_state_declaration()
    );
    let specialized = base_source.replacen(marker, &fragment, 1);
    dump_specialized_shader_if_requested(&specialized)?;
    Ok(specialized)
}

fn boolean_point_scale(settings_scale: f64) -> f32 {
    (1.0 / settings_scale) as f32
}

#[derive(Clone, Copy)]
enum RuntimeBindingValue {
    Float(f32),
    SquaredFloat(f32),
    SquareRatio {
        denominator_key: &'static str,
        numerator_default: f32,
        denominator_default: f32,
    },
    Ratio {
        denominator_key: &'static str,
        numerator_default: f32,
        denominator_default: f32,
    },
    SquareRoot(f32),
    ReciprocalSquareRoot(f32),
    Reciprocal(f32),
    Radians(f32),
    CosineDegrees(f32),
    SineDegrees(f32),
    Bool(bool),
    Integer(i32),
    Float4 {
        default: [f32; 4],
        scene_components: usize,
    },
    RotationMatrix,
    RotationMatrix4,
    MandelboxRotations {
        inverse: bool,
    },
    GeneralizedFoldNormals {
        count: usize,
    },
}

#[derive(Clone, Copy)]
struct RuntimeParameterBinding {
    member_path: &'static str,
    scene_key: &'static str,
    value: RuntimeBindingValue,
}

#[derive(Clone)]
struct OwnedRuntimeParameterBinding {
    member_path: String,
    scene_key: String,
    value: RuntimeBindingValue,
    metal_type: Option<String>,
    enum_values: Vec<String>,
}

struct RuntimeBindingSchema {
    defaults: BTreeMap<String, String>,
    enum_values: BTreeMap<String, Vec<String>>,
    mappings: Vec<super::catalog::RuntimeParameterRecord>,
    field_types: BTreeMap<String, String>,
    enum_declarations: String,
}

const GENERATED_RUNTIME_PARAMETER_BINDINGS: &[RuntimeParameterBinding] = &[
    RuntimeParameterBinding {
        member_path: "analyticDE.offset0",
        scene_key: "analyticDE_offset_0",
        value: RuntimeBindingValue::Float(0.0),
    },
    RuntimeParameterBinding {
        member_path: "mandelbox.fR2",
        scene_key: "mandelbox_folding_fixed_radius",
        value: RuntimeBindingValue::SquaredFloat(1.0),
    },
    RuntimeParameterBinding {
        member_path: "mandelbox.mR2",
        scene_key: "mandelbox_folding_min_radius",
        value: RuntimeBindingValue::SquaredFloat(0.5),
    },
    RuntimeParameterBinding {
        member_path: "mandelbox.mboxFactor1",
        scene_key: "mandelbox_folding_fixed_radius",
        value: RuntimeBindingValue::SquareRatio {
            denominator_key: "mandelbox_folding_min_radius",
            numerator_default: 1.0,
            denominator_default: 0.5,
        },
    },
    RuntimeParameterBinding {
        member_path: "mandelbox.mainRot",
        scene_key: "mandelbox_rotation_main",
        value: RuntimeBindingValue::RotationMatrix,
    },
    RuntimeParameterBinding {
        member_path: "mandelbox.rot",
        scene_key: "mandelbox_rotation_neg_1",
        value: RuntimeBindingValue::MandelboxRotations { inverse: false },
    },
    RuntimeParameterBinding {
        member_path: "mandelbox.rotinv",
        scene_key: "mandelbox_rotation_neg_1",
        value: RuntimeBindingValue::MandelboxRotations { inverse: true },
    },
    RuntimeParameterBinding {
        member_path: "donut.factor",
        scene_key: "donut_factor",
        value: RuntimeBindingValue::Float(3.0),
    },
    RuntimeParameterBinding {
        member_path: "donut.number",
        scene_key: "donut_number",
        value: RuntimeBindingValue::Float(9.0),
    },
    RuntimeParameterBinding {
        member_path: "donut.ringRadius",
        scene_key: "donut_ring_radius",
        value: RuntimeBindingValue::Float(1.0),
    },
    RuntimeParameterBinding {
        member_path: "donut.ringThickness",
        scene_key: "donut_ring_thickness",
        value: RuntimeBindingValue::Float(0.1),
    },
    RuntimeParameterBinding {
        member_path: "foldColor.auxColorEnabled",
        scene_key: "fold_color_aux_color_enabled",
        value: RuntimeBindingValue::Bool(true),
    },
    RuntimeParameterBinding {
        member_path: "foldColor.auxColorEnabledAFalse",
        scene_key: "fold_color_aux_color_enabledA_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "foldColor.auxColorEnabledBFalse",
        scene_key: "fold_color_aux_color_enabledB_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "foldColor.auxColorEnabledFalse",
        scene_key: "fold_color_aux_color_enabled_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "foldColor.difs0",
        scene_key: "fold_color_difs0",
        value: RuntimeBindingValue::Float(0.0),
    },
    RuntimeParameterBinding {
        member_path: "foldColor.difs1",
        scene_key: "fold_color_difs1",
        value: RuntimeBindingValue::Float(1.0),
    },
    RuntimeParameterBinding {
        member_path: "foldColor.difs0000",
        scene_key: "fold_color_difs_0000",
        value: RuntimeBindingValue::Float4 {
            default: [0.0, 0.0, 0.0, 0.0],
            scene_components: 4,
        },
    },
    RuntimeParameterBinding {
        member_path: "foldColor.startIterationsA",
        scene_key: "fold_color_start_iterations_A",
        value: RuntimeBindingValue::Integer(0),
    },
    RuntimeParameterBinding {
        member_path: "foldColor.stopIterationsA",
        scene_key: "fold_color_stop_iterations_A",
        value: RuntimeBindingValue::Integer(250),
    },
    RuntimeParameterBinding {
        member_path: "genFoldBox.Nv_tet",
        scene_key: "__derived_gen_fold_box_Nv_tet",
        value: RuntimeBindingValue::GeneralizedFoldNormals { count: 4 },
    },
    RuntimeParameterBinding {
        member_path: "genFoldBox.Nv_cube",
        scene_key: "__derived_gen_fold_box_Nv_cube",
        value: RuntimeBindingValue::GeneralizedFoldNormals { count: 6 },
    },
    RuntimeParameterBinding {
        member_path: "genFoldBox.Nv_oct",
        scene_key: "__derived_gen_fold_box_Nv_oct",
        value: RuntimeBindingValue::GeneralizedFoldNormals { count: 8 },
    },
    RuntimeParameterBinding {
        member_path: "genFoldBox.Nv_dodeca",
        scene_key: "__derived_gen_fold_box_Nv_dodeca",
        value: RuntimeBindingValue::GeneralizedFoldNormals { count: 12 },
    },
    RuntimeParameterBinding {
        member_path: "genFoldBox.Nv_oct_cube",
        scene_key: "__derived_gen_fold_box_Nv_oct_cube",
        value: RuntimeBindingValue::GeneralizedFoldNormals { count: 14 },
    },
    RuntimeParameterBinding {
        member_path: "genFoldBox.Nv_icosa",
        scene_key: "__derived_gen_fold_box_Nv_icosa",
        value: RuntimeBindingValue::GeneralizedFoldNormals { count: 20 },
    },
    RuntimeParameterBinding {
        member_path: "genFoldBox.Nv_box6",
        scene_key: "__derived_gen_fold_box_Nv_box6",
        value: RuntimeBindingValue::GeneralizedFoldNormals { count: 8 },
    },
    RuntimeParameterBinding {
        member_path: "genFoldBox.Nv_box5",
        scene_key: "__derived_gen_fold_box_Nv_box5",
        value: RuntimeBindingValue::GeneralizedFoldNormals { count: 7 },
    },
    RuntimeParameterBinding {
        member_path: "genFoldBox.sides_tet",
        scene_key: "__derived_gen_fold_box_sides_tet",
        value: RuntimeBindingValue::Integer(4),
    },
    RuntimeParameterBinding {
        member_path: "genFoldBox.sides_cube",
        scene_key: "__derived_gen_fold_box_sides_cube",
        value: RuntimeBindingValue::Integer(6),
    },
    RuntimeParameterBinding {
        member_path: "genFoldBox.sides_oct",
        scene_key: "__derived_gen_fold_box_sides_oct",
        value: RuntimeBindingValue::Integer(8),
    },
    RuntimeParameterBinding {
        member_path: "genFoldBox.sides_dodeca",
        scene_key: "__derived_gen_fold_box_sides_dodeca",
        value: RuntimeBindingValue::Integer(12),
    },
    RuntimeParameterBinding {
        member_path: "genFoldBox.sides_oct_cube",
        scene_key: "__derived_gen_fold_box_sides_oct_cube",
        value: RuntimeBindingValue::Integer(14),
    },
    RuntimeParameterBinding {
        member_path: "genFoldBox.sides_icosa",
        scene_key: "__derived_gen_fold_box_sides_icosa",
        value: RuntimeBindingValue::Integer(20),
    },
    RuntimeParameterBinding {
        member_path: "genFoldBox.sides_box6",
        scene_key: "__derived_gen_fold_box_sides_box6",
        value: RuntimeBindingValue::Integer(8),
    },
    RuntimeParameterBinding {
        member_path: "genFoldBox.sides_box5",
        scene_key: "__derived_gen_fold_box_sides_box5",
        value: RuntimeBindingValue::Integer(7),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.additionConstant111",
        scene_key: "transf_addition_constant_111",
        value: RuntimeBindingValue::Float4 {
            default: [1.0, 1.0, 1.0, 0.0],
            scene_components: 3,
        },
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.angleDegA",
        scene_key: "transf_angle_deg_A",
        value: RuntimeBindingValue::Radians(0.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.cosA",
        scene_key: "transf_angle_deg_A",
        value: RuntimeBindingValue::CosineDegrees(0.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.cosB",
        scene_key: "transf_angle_deg_B",
        value: RuntimeBindingValue::CosineDegrees(0.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.cosC",
        scene_key: "transf_angle_deg_C",
        value: RuntimeBindingValue::CosineDegrees(0.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.inv0",
        scene_key: "transf_invert_0",
        value: RuntimeBindingValue::Reciprocal(0.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.inv1",
        scene_key: "transf_invert_1",
        value: RuntimeBindingValue::Reciprocal(1.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.maxMinR0factor",
        scene_key: "transf_maxR2_1",
        value: RuntimeBindingValue::Ratio {
            denominator_key: "transf_minimum_radius_0",
            numerator_default: 1.0,
            denominator_default: 0.0,
        },
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.maxMinR2factor",
        scene_key: "transf_maxR2_1",
        value: RuntimeBindingValue::Ratio {
            denominator_key: "transf_minR2_p25",
            numerator_default: 1.0,
            denominator_default: 0.25,
        },
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.mboxFactor1",
        scene_key: "transf_minimum_radius_05",
        value: RuntimeBindingValue::ReciprocalSquareRoot(0.5),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabled4dFalse",
        scene_key: "transf_function_enabled4d_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledAFalse",
        scene_key: "transf_function_enabledA_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledCFalse",
        scene_key: "transf_function_enabledC_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledDFalse",
        scene_key: "transf_function_enabledD_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledAxFalse",
        scene_key: "transf_function_enabledAx_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledAyFalse",
        scene_key: "transf_function_enabledAy_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledAzFalse",
        scene_key: "transf_function_enabledAz_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledGFalse",
        scene_key: "transf_function_enabledG_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledFalse",
        scene_key: "transf_function_enabled_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledJFalse",
        scene_key: "transf_function_enabledJ_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledRFalse",
        scene_key: "transf_function_enabledR_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledTFalse",
        scene_key: "transf_function_enabledT_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledZcFalse",
        scene_key: "transf_function_enabledZc_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledxFalse",
        scene_key: "transf_function_enabledx_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledyFalse",
        scene_key: "transf_function_enabledy_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledzFalse",
        scene_key: "transf_function_enabledz_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.offset0",
        scene_key: "transf_offset_0",
        value: RuntimeBindingValue::Float(0.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.offset000",
        scene_key: "transf_offset_000",
        value: RuntimeBindingValue::Float4 {
            default: [0.0, 0.0, 0.0, 0.0],
            scene_components: 3,
        },
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.offset0005",
        scene_key: "transf_offset_0005",
        value: RuntimeBindingValue::Float(0.005),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.offset02",
        scene_key: "transf_offset_02",
        value: RuntimeBindingValue::Float(0.2),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.offset110",
        scene_key: "transf_offset_110",
        value: RuntimeBindingValue::Float4 {
            default: [1.0, 1.0, 0.0, 0.0],
            scene_components: 3,
        },
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.offsetB0",
        scene_key: "transf_offsetB_0",
        value: RuntimeBindingValue::Float(0.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.offsetR1",
        scene_key: "transf_offsetR_1",
        value: RuntimeBindingValue::Float(1.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.rotationEnabled",
        scene_key: "transf_rotation_enabled",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.rotationMatrix",
        scene_key: "transf_rotation",
        value: RuntimeBindingValue::RotationMatrix,
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.rotationMatrix2",
        scene_key: "transf_rotation2",
        value: RuntimeBindingValue::RotationMatrix,
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.rotationMatrixXYZ",
        scene_key: "transf_rotationXYZ",
        value: RuntimeBindingValue::RotationMatrix4,
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.rotationMatrix2XYZ",
        scene_key: "transf_rotation2XYZ",
        value: RuntimeBindingValue::RotationMatrix4,
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.radius1",
        scene_key: "transf_radius_1",
        value: RuntimeBindingValue::Float(1.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.scale1",
        scene_key: "transf_scale_1",
        value: RuntimeBindingValue::Float(1.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.scale3D444",
        scene_key: "transf_scale3D_444",
        value: RuntimeBindingValue::Float4 {
            default: [4.0, 4.0, 4.0, 1.0],
            scene_components: 3,
        },
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.scaleA0",
        scene_key: "transf_scaleA_0",
        value: RuntimeBindingValue::Float(0.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.scaleA1",
        scene_key: "transf_scaleA_1",
        value: RuntimeBindingValue::Float(1.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.scaleF1",
        scene_key: "transf_scaleF_1",
        value: RuntimeBindingValue::Float(1.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.startIterationsZc",
        scene_key: "transf_start_iterations_Zc",
        value: RuntimeBindingValue::Integer(0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.startIterationsM",
        scene_key: "transf_start_iterations_M",
        value: RuntimeBindingValue::Integer(0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.startIterationsR",
        scene_key: "transf_start_iterations_R",
        value: RuntimeBindingValue::Integer(0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.startIterationsT",
        scene_key: "transf_start_iterations_T",
        value: RuntimeBindingValue::Integer(0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.stopIterationsZc",
        scene_key: "transf_stop_iterations_Zc",
        value: RuntimeBindingValue::Integer(250),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.stopIterationsM",
        scene_key: "transf_stop_iterations_M",
        value: RuntimeBindingValue::Integer(250),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.stopIterationsR",
        scene_key: "transf_stop_iterations_R",
        value: RuntimeBindingValue::Integer(250),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.stopIterationsT",
        scene_key: "transf_stop_iterations_T",
        value: RuntimeBindingValue::Integer(250),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.sinA",
        scene_key: "transf_angle_deg_A",
        value: RuntimeBindingValue::SineDegrees(0.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.sinB",
        scene_key: "transf_angle_deg_B",
        value: RuntimeBindingValue::SineDegrees(0.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.sinC",
        scene_key: "transf_angle_deg_C",
        value: RuntimeBindingValue::SineDegrees(0.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.sqtR",
        scene_key: "transf_minimum_radius_05",
        value: RuntimeBindingValue::SquareRoot(0.5),
    },
    RuntimeParameterBinding {
        member_path: "foldColor.auxColorEnabledA",
        scene_key: "fold_color_aux_color_enabledA",
        value: RuntimeBindingValue::Bool(true),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.additionConstantA111",
        scene_key: "transf_addition_constantA_111",
        value: RuntimeBindingValue::Float4 {
            default: [1.0, 1.0, 1.0, 0.0],
            scene_components: 3,
        },
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledAx",
        scene_key: "transf_function_enabledAx",
        value: RuntimeBindingValue::Bool(true),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledAy",
        scene_key: "transf_function_enabledAy",
        value: RuntimeBindingValue::Bool(true),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledBFalse",
        scene_key: "transf_function_enabledB_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledCxFalse",
        scene_key: "transf_function_enabledCx_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledCyFalse",
        scene_key: "transf_function_enabledCy_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledKFalse",
        scene_key: "transf_function_enabledK_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.functionEnabledPFalse",
        scene_key: "transf_function_enabledP_false",
        value: RuntimeBindingValue::Bool(false),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.int6",
        scene_key: "transf_int_6",
        value: RuntimeBindingValue::Integer(6),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.int8X",
        scene_key: "transf_int8_X",
        value: RuntimeBindingValue::Integer(8),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.offset001",
        scene_key: "transf_offset_001",
        value: RuntimeBindingValue::Float4 {
            default: [0.0, 0.0, 1.0, 0.0],
            scene_components: 3,
        },
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.offset3",
        scene_key: "transf_offset_3",
        value: RuntimeBindingValue::Float(3.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.offsetA000",
        scene_key: "transf_offsetA_000",
        value: RuntimeBindingValue::Float4 {
            default: [0.0, 0.0, 0.0, 0.0],
            scene_components: 3,
        },
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.offsetC0",
        scene_key: "transf_offsetC_0",
        value: RuntimeBindingValue::Float(0.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.offsetD0",
        scene_key: "transf_offsetD_0",
        value: RuntimeBindingValue::Float(0.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.offsetE2",
        scene_key: "transf_offsetE_2",
        value: RuntimeBindingValue::Float(2.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.offsetF2",
        scene_key: "transf_offsetF_2",
        value: RuntimeBindingValue::Float(2.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.scale05",
        scene_key: "transf_scale_05",
        value: RuntimeBindingValue::Float(0.5),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.scale2",
        scene_key: "transf_scale_2",
        value: RuntimeBindingValue::Float(2.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.scale3",
        scene_key: "transf_scale_3",
        value: RuntimeBindingValue::Float(3.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.scaleC1",
        scene_key: "transf_scaleC_1",
        value: RuntimeBindingValue::Float(1.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.scaleVary0",
        scene_key: "transf_scale_vary_0",
        value: RuntimeBindingValue::Float(0.0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.startIterations",
        scene_key: "transf_start_iterations",
        value: RuntimeBindingValue::Integer(0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.stopIterations",
        scene_key: "transf_stop_iterations",
        value: RuntimeBindingValue::Integer(250),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.startIterationsA",
        scene_key: "transf_start_iterations_A",
        value: RuntimeBindingValue::Integer(0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.stopIterationsA",
        scene_key: "transf_stop_iterations_A",
        value: RuntimeBindingValue::Integer(250),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.startIterationsB",
        scene_key: "transf_start_iterations_B",
        value: RuntimeBindingValue::Integer(0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.stopIterationsB",
        scene_key: "transf_stop_iterations_B",
        value: RuntimeBindingValue::Integer(250),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.startIterationsC",
        scene_key: "transf_start_iterations_C",
        value: RuntimeBindingValue::Integer(0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.stopIterationsC",
        scene_key: "transf_stop_iterations_C",
        value: RuntimeBindingValue::Integer(250),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.startIterationsCx",
        scene_key: "transf_start_iterations_Cx",
        value: RuntimeBindingValue::Integer(0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.stopIterationsCx",
        scene_key: "transf_stop_iterations_Cx",
        value: RuntimeBindingValue::Integer(250),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.startIterationsCy",
        scene_key: "transf_start_iterations_Cy",
        value: RuntimeBindingValue::Integer(0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.stopIterationsCy",
        scene_key: "transf_stop_iterations_Cy",
        value: RuntimeBindingValue::Integer(250),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.startIterationsD",
        scene_key: "transf_start_iterations_D",
        value: RuntimeBindingValue::Integer(0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.stopIterationsD",
        scene_key: "transf_stop_iterations_D",
        value: RuntimeBindingValue::Integer(250),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.startIterationsE",
        scene_key: "transf_start_iterations_E",
        value: RuntimeBindingValue::Integer(0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.stopIterationsE",
        scene_key: "transf_stop_iterations_E",
        value: RuntimeBindingValue::Integer(250),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.startIterationsF",
        scene_key: "transf_start_iterations_F",
        value: RuntimeBindingValue::Integer(0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.stopIterationsF",
        scene_key: "transf_stop_iterations_F",
        value: RuntimeBindingValue::Integer(250),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.startIterationsK",
        scene_key: "transf_start_iterations_K",
        value: RuntimeBindingValue::Integer(0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.stopIterationsK",
        scene_key: "transf_stop_iterations_K",
        value: RuntimeBindingValue::Integer(250),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.startIterationsP",
        scene_key: "transf_start_iterations_P",
        value: RuntimeBindingValue::Integer(0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.stopIterationsP",
        scene_key: "transf_stop_iterations_P",
        value: RuntimeBindingValue::Integer(250),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.startIterationsS",
        scene_key: "transf_start_iterations_S",
        value: RuntimeBindingValue::Integer(0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.stopIterationsS",
        scene_key: "transf_stop_iterations_S",
        value: RuntimeBindingValue::Integer(250),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.startIterationsX",
        scene_key: "transf_start_iterations_X",
        value: RuntimeBindingValue::Integer(0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.stopIterationsX",
        scene_key: "transf_stop_iterations_X",
        value: RuntimeBindingValue::Integer(250),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.startIterationsY",
        scene_key: "transf_start_iterations_Y",
        value: RuntimeBindingValue::Integer(0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.stopIterationsY",
        scene_key: "transf_stop_iterations_Y",
        value: RuntimeBindingValue::Integer(250),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.startIterationsZ",
        scene_key: "transf_start_iterations_Z",
        value: RuntimeBindingValue::Integer(0),
    },
    RuntimeParameterBinding {
        member_path: "transformCommon.stopIterationsZ",
        scene_key: "transf_stop_iterations_Z",
        value: RuntimeBindingValue::Integer(250),
    },
];

#[derive(Clone, Debug, Serialize)]
pub struct MetalCompileReport {
    pub formula: FormulaFrontendReport,
    pub output: String,
    pub metal_checked: bool,
    pub air_output: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FormulaAuditEntry {
    pub symbol: String,
    pub parsed: bool,
    pub report: Option<FormulaFrontendReport>,
    pub error: Option<String>,
    pub metal_compiled: Option<bool>,
    pub metal_error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompilerAuditReport {
    pub schema_version: u32,
    pub formulas: usize,
    pub parsed: usize,
    pub failed: usize,
    pub runtime_emitters: usize,
    pub metal_checked: usize,
    pub metal_passed: usize,
    pub metal_failed: usize,
    pub construct_counts: std::collections::BTreeMap<String, usize>,
    pub entries: Vec<FormulaAuditEntry>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SceneAuditEntry {
    pub path: String,
    pub hybrid: Option<bool>,
    pub formula_ids: Vec<u32>,
    pub generated: bool,
    pub metal_compiled: Option<bool>,
    pub source_bytes: Option<usize>,
    pub generation_ms: f64,
    pub metal_compile_ms: Option<f64>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SceneAuditReport {
    pub schema_version: u32,
    pub scenes: usize,
    pub generated: usize,
    pub generation_failed: usize,
    pub metal_checked: usize,
    pub metal_passed: usize,
    pub metal_failed: usize,
    pub entries: Vec<SceneAuditEntry>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FormulaPolicyAuditEntry {
    pub path: String,
    pub formula_ids: Vec<u32>,
    pub baseline_sha256: Option<String>,
    pub candidate_sha256: Option<String>,
    pub baseline_source_bytes: Option<usize>,
    pub candidate_source_bytes: Option<usize>,
    pub source_changed: bool,
    pub selected_formula_id: i32,
    pub selected_unrolled_periodic: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FormulaPolicyAuditReport {
    pub schema_version: u32,
    pub scenes: usize,
    pub generated: usize,
    pub generation_failed: usize,
    pub unexpected_generation_failures: usize,
    pub selected_scenes: usize,
    pub changed_sources: usize,
    pub unexpected_changes: usize,
    pub missed_selections: usize,
    pub entries: Vec<FormulaPolicyAuditEntry>,
}

pub fn audit_formula_optimization_policy(
    root: &Path,
    base_source: &str,
) -> Result<FormulaPolicyAuditReport> {
    let paths = super::catalog::example_scene_paths(root)?;
    let mut entries = Vec::with_capacity(paths.len());
    for (index, path) in paths.iter().enumerate() {
        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string();
        let result = (|| -> Result<FormulaPolicyAuditEntry> {
            let scene = MandelbulberScene::load(path)?;
            let formula_ids = scene
                .formula_slots
                .iter()
                .filter(|slot| slot.active())
                .map(|slot| slot.formula_id)
                .collect::<Vec<_>>();
            let mut config = crate::scene::default_config();
            scene.apply_to_config(&mut config);
            let selected_policy = scene_formula_optimization_policy(path)?;
            let mut baseline_policy = selected_policy;
            baseline_policy.unrolled_periodic_hybrid_loop = false;
            baseline_policy.partial_evaluation_formula_id = 0;
            baseline_policy.partial_evaluation_phases = false;
            baseline_policy.partial_evaluation_scalarize_loops = false;
            baseline_policy.partial_evaluation_dce = false;
            baseline_policy.partial_evaluation_cse = false;
            let kernel_specialization = scene_uses_kernel_specialization(path)?;
            let direct_hybrid_loop = scene_uses_direct_hybrid_loop(path)?;
            let mut baseline_scene = scene.clone();
            let baseline = specialize_scene_with_kernel_specialization(
                base_source,
                &mut baseline_scene,
                root,
                &config.set_values,
                kernel_specialization,
                direct_hybrid_loop,
                baseline_policy,
            )?;
            let mut candidate_scene = scene;
            let candidate = specialize_scene_with_kernel_specialization(
                base_source,
                &mut candidate_scene,
                root,
                &config.set_values,
                kernel_specialization,
                direct_hybrid_loop,
                selected_policy,
            )?;
            let baseline_sha256 = format!("{:x}", Sha256::digest(baseline.as_bytes()));
            let candidate_sha256 = format!("{:x}", Sha256::digest(candidate.as_bytes()));
            Ok(FormulaPolicyAuditEntry {
                path: relative_path.clone(),
                formula_ids,
                baseline_sha256: Some(baseline_sha256.clone()),
                candidate_sha256: Some(candidate_sha256.clone()),
                baseline_source_bytes: Some(baseline.len()),
                candidate_source_bytes: Some(candidate.len()),
                source_changed: baseline_sha256 != candidate_sha256,
                selected_formula_id: selected_policy.partial_evaluation_formula_id,
                selected_unrolled_periodic: selected_policy.unrolled_periodic_hybrid_loop,
                error: None,
            })
        })();
        entries.push(result.unwrap_or_else(|error| FormulaPolicyAuditEntry {
            path: relative_path,
            formula_ids: Vec::new(),
            baseline_sha256: None,
            candidate_sha256: None,
            baseline_source_bytes: None,
            candidate_source_bytes: None,
            source_changed: false,
            selected_formula_id: 0,
            selected_unrolled_periodic: false,
            error: Some(format!("{error:#}")),
        }));
        if (index + 1) % 25 == 0 || index + 1 == paths.len() {
            eprintln!("Mandel formula policy audit: {}/{}", index + 1, paths.len());
        }
    }
    let generated = entries.iter().filter(|entry| entry.error.is_none()).count();
    let unexpected_generation_failures = entries
        .iter()
        .filter(|entry| {
            let Some(error) = entry.error.as_deref() else {
                return false;
            };
            let known_invalid_example = Path::new(&entry.path)
                .file_name()
                .is_some_and(|name| name == "light types.fract")
                && error.contains("formula slot 1 must select an enabled formula");
            !known_invalid_example
        })
        .count();
    let selected_scenes = entries
        .iter()
        .filter(|entry| entry.selected_formula_id != 0 || entry.selected_unrolled_periodic)
        .count();
    let changed_sources = entries.iter().filter(|entry| entry.source_changed).count();
    let unexpected_changes = entries
        .iter()
        .filter(|entry| {
            entry.source_changed
                && entry.selected_formula_id == 0
                && !entry.selected_unrolled_periodic
        })
        .count();
    let missed_selections = entries
        .iter()
        .filter(|entry| {
            !entry.source_changed
                && (entry.selected_formula_id != 0 || entry.selected_unrolled_periodic)
        })
        .count();
    Ok(FormulaPolicyAuditReport {
        schema_version: 3,
        scenes: entries.len(),
        generated,
        generation_failed: entries.len() - generated,
        unexpected_generation_failures,
        selected_scenes,
        changed_sources,
        unexpected_changes,
        missed_selections,
        entries,
    })
}

pub fn audit_scenes(root: &Path, base_source: &str, metal_check: bool) -> Result<SceneAuditReport> {
    type MetalAuditResult = Option<(bool, f64, Option<String>)>;
    type SceneGenerationResult = Result<(usize, MetalAuditResult)>;
    let paths = super::catalog::example_scene_paths(root)?;
    let temporary_dir = std::env::temp_dir().join(format!(
        "fpt-mandel-scene-audit-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    if metal_check {
        fs::create_dir_all(&temporary_dir)
            .with_context(|| format!("create {}", temporary_dir.display()))?;
    }
    let metal_path = temporary_dir.join("scene.metal");
    let air_path = temporary_dir.join("scene.air");
    let mut entries = Vec::with_capacity(paths.len());
    for (index, path) in paths.iter().enumerate() {
        let started = Instant::now();
        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string();
        let mut hybrid = None;
        let mut formula_ids = Vec::new();
        let result = (|| -> SceneGenerationResult {
            let mut scene = MandelbulberScene::load(path)?;
            hybrid = Some(scene.hybrid_enabled);
            formula_ids = scene
                .formula_slots
                .iter()
                .filter(|slot| slot.active())
                .map(|slot| slot.formula_id)
                .collect();
            let mut config = crate::scene::default_config();
            scene.apply_to_config(&mut config);
            let source = specialize_scene(base_source, &mut scene, root, &config.set_values)?;
            let source_bytes = source.len();
            if !metal_check {
                return Ok((source_bytes, None));
            }
            fs::write(&metal_path, source)
                .with_context(|| format!("write {}", metal_path.display()))?;
            let compile_started = Instant::now();
            let compile_result = check_metal_source(&metal_path, &air_path);
            let compile_ms = compile_started.elapsed().as_secs_f64() * 1000.0;
            match compile_result {
                Ok(()) => Ok((source_bytes, Some((true, compile_ms, None)))),
                Err(error) => Ok((
                    source_bytes,
                    Some((
                        false,
                        compile_ms,
                        Some(format!("Metal compile ({compile_ms:.2} ms): {error:#}")),
                    )),
                )),
            }
        })();
        let generation_ms = started.elapsed().as_secs_f64() * 1000.0;
        let entry = match result {
            Ok((source_bytes, metal_result)) => {
                let (metal_compiled, metal_compile_ms, error) = metal_result
                    .map_or((None, None, None), |(compiled, compile_ms, error)| {
                        (Some(compiled), Some(compile_ms), error)
                    });
                SceneAuditEntry {
                    path: relative_path,
                    hybrid,
                    formula_ids,
                    generated: true,
                    metal_compiled,
                    source_bytes: Some(source_bytes),
                    generation_ms,
                    metal_compile_ms,
                    error,
                }
            }
            Err(error) => SceneAuditEntry {
                path: relative_path,
                hybrid,
                formula_ids,
                generated: false,
                metal_compiled: metal_check.then_some(false),
                source_bytes: None,
                generation_ms,
                metal_compile_ms: None,
                error: Some(format!("{error:#}")),
            },
        };
        entries.push(entry);
        if (index + 1) % 25 == 0 || index + 1 == paths.len() {
            eprintln!("Mandel scene audit: {}/{}", index + 1, paths.len());
        }
    }
    if metal_check {
        let _ = fs::remove_file(&metal_path);
        let _ = fs::remove_file(&air_path);
        let _ = fs::remove_dir(&temporary_dir);
    }
    let generated = entries.iter().filter(|entry| entry.generated).count();
    let metal_checked = if metal_check { entries.len() } else { 0 };
    let metal_passed = entries
        .iter()
        .filter(|entry| entry.metal_compiled == Some(true))
        .count();
    Ok(SceneAuditReport {
        schema_version: 2,
        scenes: entries.len(),
        generated,
        generation_failed: entries.len() - generated,
        metal_checked,
        metal_passed,
        metal_failed: metal_checked - metal_passed,
        entries,
    })
}

pub fn audit_formulas(root: &Path, metal_output_dir: Option<&Path>) -> Result<CompilerAuditReport> {
    let symbols = formula_symbols(root)?;
    if let Some(output_dir) = metal_output_dir {
        fs::create_dir_all(output_dir)?;
    }
    let mut entries = Vec::with_capacity(symbols.len());
    let mut construct_counts = std::collections::BTreeMap::<String, usize>::new();
    let mut parsed_count = 0;
    let mut runtime_count = 0;
    let mut metal_checked = 0;
    let mut metal_passed = 0;
    for symbol in symbols {
        match parse_formula(root, &symbol) {
            Ok(formula) => {
                parsed_count += 1;
                runtime_count += usize::from(supports_runtime_emitter(&formula));
                for construct in &formula.constructs {
                    *construct_counts.entry(construct.clone()).or_default() += 1;
                }
                let (compiled, metal_error) = if let Some(output_dir) = metal_output_dir {
                    metal_checked += 1;
                    let output = output_dir.join(format!("{}.metal", formula.source.internal_name));
                    let result = emit_metal(&formula).and_then(|metal| {
                        fs::write(&output, metal)
                            .with_context(|| format!("write {}", output.display()))?;
                        check_metal_source(&output, &output.with_extension("air"))
                    });
                    match result {
                        Ok(()) => {
                            metal_passed += 1;
                            (Some(true), None)
                        }
                        Err(error) => (Some(false), Some(format!("{error:#}"))),
                    }
                } else {
                    (None, None)
                };
                entries.push(FormulaAuditEntry {
                    symbol,
                    parsed: true,
                    report: Some(frontend_report(&formula)),
                    error: None,
                    metal_compiled: compiled,
                    metal_error,
                });
            }
            Err(error) => entries.push(FormulaAuditEntry {
                symbol,
                parsed: false,
                report: None,
                error: Some(format!("{error:#}")),
                metal_compiled: None,
                metal_error: None,
            }),
        }
    }
    Ok(CompilerAuditReport {
        schema_version: 1,
        formulas: entries.len(),
        parsed: parsed_count,
        failed: entries.len() - parsed_count,
        runtime_emitters: runtime_count,
        metal_checked,
        metal_passed,
        metal_failed: metal_checked - metal_passed,
        construct_counts,
        entries,
    })
}

pub fn parse_formula(root: &Path, identifier: &str) -> Result<ParsedFormula> {
    let resolved = resolve_formula_source(root, identifier)?;
    let source = fs::read_to_string(&resolved.path)
        .with_context(|| format!("read {}", resolved.path.display()))?;
    parse_resolved_formula_source(resolved, source)
}

pub fn parse_embedded_custom_formula(root: &Path, encoded: &str) -> Result<ParsedFormula> {
    let compressed = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .context("decode embedded custom formula base64")?;
    ensure!(
        compressed.len() >= 4,
        "embedded custom formula is missing the Qt size prefix"
    );
    let declared_size = u32::from_be_bytes(compressed[..4].try_into().unwrap()) as usize;
    let mut decoder = ZlibDecoder::new(&compressed[4..]);
    let mut source = String::new();
    decoder
        .read_to_string(&mut source)
        .context("decompress embedded custom formula")?;
    ensure!(
        source.len() == declared_size,
        "embedded custom formula decoded to {} bytes, expected {declared_size}",
        source.len()
    );
    let resolved = ResolvedFormulaSource {
        symbol: "custom".into(),
        id: 10_000,
        internal_name: "custom".into(),
        path: root.join("formula/opencl/embedded_custom.cl"),
        de_type: "analyticDEType".into(),
        de_function_type: "linearDEFunction".into(),
        pixel_addition: "cpixelDisabledByDefault".into(),
        default_bailout: 100.0,
        analytic_function: "analyticFunctionLinear".into(),
        coloring_function: "coloringFunctionDefault".into(),
    };
    parse_resolved_formula_source(resolved, source)
}

fn parse_resolved_formula_source(
    resolved: ResolvedFormulaSource,
    source: String,
) -> Result<ParsedFormula> {
    let tokens = lex(&source)?;
    validate_balanced(&tokens)?;
    let (function_name, start, end) = find_iteration_function(&source, &tokens)?;
    let function_source = source[start..end].to_owned();
    let function_tokens = lex(&function_source)?;
    Ok(ParsedFormula {
        parameter_reads: collect_member_paths(&function_tokens, "fractal"),
        auxiliary_fields: collect_member_paths(&function_tokens, "aux"),
        constructs: classify_constructs(&function_tokens),
        source: resolved,
        function_name,
        function_source,
        tokens: function_tokens,
    })
}

pub fn frontend_report(formula: &ParsedFormula) -> FormulaFrontendReport {
    let identifiers = formula
        .tokens
        .iter()
        .filter(|token| token.kind == TokenKind::Identifier)
        .map(|token| token.text.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    FormulaFrontendReport {
        symbol: formula.source.symbol.clone(),
        id: formula.source.id,
        internal_name: formula.source.internal_name.clone(),
        source_path: formula.source.path.display().to_string(),
        de_type: formula.source.de_type.clone(),
        de_function_type: formula.source.de_function_type.clone(),
        pixel_addition: formula.source.pixel_addition.clone(),
        default_bailout: formula.source.default_bailout,
        analytic_function: formula.source.analytic_function.clone(),
        coloring_function: formula.source.coloring_function.clone(),
        iteration_function: formula.function_name.clone(),
        token_count: formula.tokens.len(),
        identifiers,
        parameter_reads: formula.parameter_reads.clone(),
        auxiliary_fields: formula.auxiliary_fields.clone(),
        constructs: formula.constructs.clone(),
        metal_emitter: if supports_runtime_emitter(formula) {
            "runtime-generated".into()
        } else {
            "standalone-generated".into()
        },
    }
}

pub fn emit_metal(formula: &ParsedFormula) -> Result<String> {
    let declarations = emit_formula_declarations(formula, "thread", false)?;
    Ok(format!(
        r#"// Generated from Mandelbulber2 formula source.
// The generated compatibility artifact is subject to Mandelbulber2's GPLv3-or-later license.
#include <metal_stdlib>
using namespace metal;

{declarations}

kernel void mandel_formula_smoke(
    device const float4 *input [[buffer(0)]],
    device float4 *output [[buffer(1)]],
    constant MandelFractalParameters &fractal [[buffer(2)]],
    uint gid [[thread_position_in_grid]]) {{
    MandelFractalParameters local_fractal = fractal;
    MandelOrbitState aux = {{}};
    aux.c = input[gid];
    aux.const_c = input[gid];
    aux.old_z = input[gid];
    aux.r = length(input[gid]);
    aux.DE = 1.0f;
    aux.dist = 1000.0f;
    aux.pseudoKleinianDE = 1.0f;
    output[gid] = {function_name}(input[gid], local_fractal, aux);
}}
"#,
        function_name = formula.function_name
    ))
}

fn emit_formula_declarations(
    formula: &ParsedFormula,
    parameter_address: &str,
    noinline_formulas: bool,
) -> Result<String> {
    let body = translate_opencl_body(
        extract_function_body(&formula.function_source)?,
        parameter_address,
    );
    let parameter_types = formula_parameter_types(formula)?;
    let dependencies = emit_formula_dependencies(formula)?;
    let matrix_helper = formula_matrix_helper(formula);
    let function_attribute = formula_function_attribute(noinline_formulas);
    Ok(format!(
        r#"{parameter_types}

{dependencies}

{orbit_state}

{matrix_helper}

{function_attribute}static float4 {function_name}(float4 z,
                              {parameter_address} const MandelFractalParameters &fractal,
                              thread MandelOrbitState &aux) {{
{body}
}}"#,
        orbit_state = orbit_state_declaration(),
        function_name = formula.function_name
    ))
}

fn emit_scene_specialized_formula_declarations(
    formula: &ParsedFormula,
    formula_values: &BTreeMap<String, String>,
    parameter_address: &str,
    noinline_formulas: bool,
    selected_optimization: FormulaSourceOptimization,
) -> Result<String> {
    let Some(optimization) = formula_source_optimization(formula, selected_optimization) else {
        return emit_formula_declarations(formula, parameter_address, noinline_formulas);
    };
    let parameter_types = formula_parameter_types(formula)?;
    let dependencies = emit_formula_dependencies(formula)?;
    let matrix_helper = formula_matrix_helper(formula);
    let functions = emit_scene_specialized_formula_functions(
        formula,
        formula_values,
        parameter_address,
        noinline_formulas,
        None,
        optimization,
    )?;
    Ok(format!(
        r#"{parameter_types}

{dependencies}

{orbit_state}

{matrix_helper}

{functions}"#,
        orbit_state = orbit_state_declaration(),
    ))
}

fn formula_parameter_types(formula: &ParsedFormula) -> Result<String> {
    Ok(if formula.source.internal_name == "kaleidoscopic_ifs" {
        IFS_PARAMETER_TYPES.to_owned()
    } else if formula.parameter_reads.is_empty() {
        "struct MandelFractalParameters { uint unused; };".to_owned()
    } else if supports_bulb_parameter_block(formula) {
        BULB_PARAMETER_TYPES.to_owned()
    } else if supports_analytic_de_parameter_block(formula) {
        ANALYTIC_DE_PARAMETER_TYPES.to_owned()
    } else if supports_transform_basic_parameter_block(formula) {
        TRANSFORM_BASIC_PARAMETER_TYPES.to_owned()
    } else if supports_jos_kleinian_parameter_block(formula) {
        JOS_KLEINIAN_PARAMETER_TYPES.to_owned()
    } else if supports_pseudo_kleinian_parameter_block(formula) {
        PSEUDO_KLEINIAN_PARAMETER_TYPES.to_owned()
    } else if supports_generated_runtime_parameter_block(formula) {
        generated_runtime_parameter_types(formula)?
    } else if supports_mandelbox_full_parameter_block(formula) {
        MANDELBOX_FULL_PARAMETER_TYPES.to_owned()
    } else if supports_mandelbox_fast_parameter_block(formula) {
        MANDELBOX_FAST_PARAMETER_TYPES.to_owned()
    } else {
        translated_fractal_schema(formula)?
    })
}

fn formula_matrix_helper(formula: &ParsedFormula) -> &'static str {
    if formula.source.internal_name == "kaleidoscopic_ifs" {
        r#"static float4 Matrix33MulFloat4(matrix33 matrix, float4 value) {
    return float4(dot(value.xyz, matrix.m1),
                  dot(value.xyz, matrix.m2),
                  dot(value.xyz, matrix.m3),
                  value.w);
}"#
    } else if formula.parameter_reads.is_empty() {
        r#"static float4 Matrix33MulFloat4(float3x3 matrix, float4 value) {
    return float4(matrix * value.xyz, value.w);
}"#
    } else {
        r#"static float4 Matrix33MulFloat4(matrix33 matrix, float4 value) {
    return float4(dot(value.xyz, matrix.m1),
                  dot(value.xyz, matrix.m2),
                  dot(value.xyz, matrix.m3),
                  value.w);
}"#
    }
}

fn orbit_state_declaration() -> &'static str {
    r#"struct MandelOrbitState {
    uint i;
    float4 c;
    float4 const_c;
    float4 old_z;
    float pos_neg;
    float r;
    float DE;
    float DE0;
    float dist;
    float pseudoKleinianDE;
    float actualScale;
    float actualScaleA;
    float color;
    float colorHybrid;
    float temp1000;
};

static float4 mandelApplyGlobalFoldings(float4 z,
                                         constant FptRenderConfig &cfg,
                                         thread MandelOrbitState &aux) {
    if (cfg.vset_values[101] > 0.5f) {
        float limit = cfg.vset_values[102];
        float value = cfg.vset_values[103];
        if (z.x > limit) z.x = value - z.x;
        else if (z.x < -limit) z.x = -value - z.x;
        if (z.y > limit) z.y = value - z.y;
        else if (z.y < -limit) z.y = -value - z.y;
        if (z.z > limit) z.z = value - z.z;
        else if (z.z < -limit) z.z = -value - z.z;
        aux.r = length(z);
    }
    if (cfg.vset_values[104] > 0.5f) {
        float outer_squared = cfg.vset_values[105] * cfg.vset_values[105];
        float inner_squared = cfg.vset_values[106] * cfg.vset_values[106];
        float radius_squared = aux.r * aux.r;
        float factor = 1.0f;
        if (radius_squared < inner_squared) {
            factor = outer_squared / inner_squared;
        } else if (radius_squared < outer_squared) {
            factor = outer_squared / radius_squared;
        }
        z *= factor;
        aux.DE *= factor;
        aux.r = length(z);
    }
    return z;
}"#
}

fn emit_formula_namespace(
    formula: &ParsedFormula,
    formula_values: &BTreeMap<String, String>,
    parameter_address: &str,
    namespace: &str,
    noinline_formulas: bool,
    selected_optimization: FormulaSourceOptimization,
) -> Result<String> {
    if let Some(optimization) = formula_source_optimization(formula, selected_optimization) {
        let parameter_types = formula_parameter_types(formula)?;
        let dependencies = emit_formula_dependencies(formula)?;
        let matrix_helper = formula_matrix_helper(formula);
        let functions = emit_scene_specialized_formula_functions(
            formula,
            formula_values,
            parameter_address,
            noinline_formulas,
            Some(namespace),
            optimization,
        )?;
        return Ok(format!(
            r#"namespace {namespace} {{
{parameter_types}

{dependencies}

{matrix_helper}

{functions}
}}"#,
        ));
    }
    let body = translate_opencl_body(
        extract_function_body(&formula.function_source)?,
        parameter_address,
    );
    emit_formula_namespace_with_body(
        formula,
        parameter_address,
        namespace,
        &body,
        noinline_formulas,
    )
}

fn formula_source_optimization(
    formula: &ParsedFormula,
    selected: FormulaSourceOptimization,
) -> Option<FormulaSourceOptimization> {
    let selected_matches = selected.formula_id == formula.source.id;
    let environment_requested = std::env::var_os("FPT_MANDEL_FORMULA_PARTIAL_EVAL").is_some()
        || formula_iteration_phase_specialization_enabled();
    let environment_matches = environment_requested
        && std::env::var("FPT_MANDEL_FORMULA_IDS").map_or(true, |filter| {
            filter.split(',').any(|candidate| {
                candidate.trim() == formula.source.id.to_string()
                    || candidate.trim() == formula.source.internal_name
                    || candidate.trim() == formula.source.symbol
            })
        });
    if !selected_matches && !environment_matches {
        return None;
    }
    Some(FormulaSourceOptimization {
        formula_id: formula.source.id,
        phases: (selected_matches && selected.phases)
            || formula_iteration_phase_specialization_enabled(),
        scalarize_loops: (selected_matches && selected.scalarize_loops)
            || std::env::var_os("FPT_MANDEL_FORMULA_SCALARIZE_LOOPS").is_some(),
        dce: (selected_matches && selected.dce)
            || std::env::var_os("FPT_MANDEL_FORMULA_DCE").is_some(),
        cse: (selected_matches && selected.cse)
            || std::env::var_os("FPT_MANDEL_FORMULA_CSE").is_some(),
    })
}

fn formula_iteration_phase_specialization_enabled() -> bool {
    std::env::var_os("FPT_MANDEL_FORMULA_PHASES").is_some()
}

fn formula_phase_limit() -> usize {
    std::env::var("FPT_MANDEL_FORMULA_PHASE_LIMIT")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|limit| *limit > 0)
        .unwrap_or(16)
}

fn formula_structural_constants(
    formula: &ParsedFormula,
    formula_values: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, StructuralValue>> {
    let Some(bindings) = generated_runtime_bindings(formula) else {
        return Ok(BTreeMap::new());
    };
    let mut constants = BTreeMap::new();
    for binding in bindings {
        let value = match binding.value {
            RuntimeBindingValue::Bool(default) => {
                StructuralValue::Bool(formula_bool(formula_values, &binding.scene_key, default)?)
            }
            RuntimeBindingValue::Integer(default) => {
                StructuralValue::Integer(i64::from(formula_integer_or_enum(
                    formula_values,
                    &binding.scene_key,
                    default,
                    &binding.enum_values,
                )?))
            }
            _ => continue,
        };
        constants.insert(format!("fractal.{}", binding.member_path), value);
        for (index, symbol) in binding.enum_values.iter().enumerate() {
            constants
                .entry(symbol.clone())
                .or_insert(StructuralValue::Integer(index as i64));
        }
    }
    Ok(constants)
}

fn formula_iteration_boundaries(constants: &BTreeMap<String, StructuralValue>) -> Vec<i64> {
    constants
        .iter()
        .filter(|(name, _)| {
            name.contains("startIterations")
                || name.contains("stopIterations")
                || name.contains("startIteration")
                || name.contains("stopIteration")
        })
        .filter_map(|(_, value)| match value {
            StructuralValue::Integer(value) if *value > 0 && *value < 4096 => Some(*value),
            _ => None,
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn emit_scene_specialized_formula_functions(
    formula: &ParsedFormula,
    formula_values: &BTreeMap<String, String>,
    parameter_address: &str,
    noinline_formulas: bool,
    namespace: Option<&str>,
    optimization: FormulaSourceOptimization,
) -> Result<String> {
    let original = extract_function_body(&formula.function_source)?;
    let constants = formula_structural_constants(formula, formula_values)?;
    let scalarize = optimization.scalarize_loops;
    let phase_specialization = optimization.phases;
    let mut boundaries = if phase_specialization {
        formula_iteration_boundaries(&constants)
    } else {
        Vec::new()
    };
    if boundaries.len() + 1 > formula_phase_limit() {
        boundaries.clear();
    }

    let ranges = if boundaries.is_empty() {
        vec![None]
    } else {
        let mut ranges = Vec::with_capacity(boundaries.len() + 1);
        let mut first = 0_i64;
        for &boundary in &boundaries {
            ranges.push(Some((first, boundary - 1)));
            first = boundary;
        }
        ranges.push(Some((first, 4095)));
        ranges
    };

    let mut unique_bodies = Vec::<String>::new();
    let mut phase_body_indices = Vec::with_capacity(ranges.len());
    let mut total_stats = FormulaSimplificationStats::default();
    for range in ranges {
        let simplified = simplify_formula_body(
            original,
            &constants,
            range,
            scalarize,
            optimization.dce,
            optimization.cse,
        )?;
        total_stats.conditions_removed += simplified.stats.conditions_removed;
        total_stats.common_initializers_removed += simplified.stats.common_initializers_removed;
        total_stats.dead_declarations_removed += simplified.stats.dead_declarations_removed;
        total_stats.loops_scalarized += simplified.stats.loops_scalarized;
        total_stats.switches_removed += simplified.stats.switches_removed;
        let body = translate_opencl_body(&simplified.source, parameter_address);
        let body_index = unique_bodies
            .iter()
            .position(|candidate| candidate == &body)
            .unwrap_or_else(|| {
                unique_bodies.push(body);
                unique_bodies.len() - 1
            });
        phase_body_indices.push(body_index);
    }

    if std::env::var_os("FPT_MANDEL_FORMULA_STATS").is_some() {
        eprintln!(
            "Mandel formula specialization {}{}: {} conditions, {} switches, {} loops, {} CSE, {} dead locals, {} phase bodies / {} ranges",
            namespace.map_or("", |value| value),
            if namespace.is_some() {
                ""
            } else {
                &formula.function_name
            },
            total_stats.conditions_removed,
            total_stats.switches_removed,
            total_stats.loops_scalarized,
            total_stats.common_initializers_removed,
            total_stats.dead_declarations_removed,
            unique_bodies.len(),
            phase_body_indices.len(),
        );
    }

    if boundaries.is_empty() {
        return Ok(emit_formula_function_definition(
            &formula.function_name,
            &unique_bodies[0],
            parameter_address,
            noinline_formulas,
        ));
    }

    let mut output = String::new();
    for (index, body) in unique_bodies.iter().enumerate() {
        output.push_str(&emit_formula_function_definition(
            &format!("{}Phase{index}", formula.function_name),
            body,
            parameter_address,
            false,
        ));
        output.push_str("\n\n");
    }
    let function_attribute = formula_function_attribute(noinline_formulas);
    output.push_str(&format!(
        "{function_attribute}static float4 {}(float4 z,\n    {parameter_address} const MandelFractalParameters &fractal,\n    thread MandelOrbitState &aux) {{\n",
        formula.function_name
    ));
    for (phase, &boundary) in boundaries.iter().enumerate() {
        output.push_str(&format!(
            "    if (aux.i < {boundary}u) return {}Phase{}(z, fractal, aux);\n",
            formula.function_name, phase_body_indices[phase]
        ));
    }
    output.push_str(&format!(
        "    return {}Phase{}(z, fractal, aux);\n}}",
        formula.function_name,
        phase_body_indices.last().copied().unwrap_or(0)
    ));
    Ok(output)
}

fn emit_formula_function_definition(
    function_name: &str,
    body: &str,
    parameter_address: &str,
    noinline_formulas: bool,
) -> String {
    let function_attribute = formula_function_attribute(noinline_formulas);
    format!(
        r#"{function_attribute}static float4 {function_name}(float4 z,
                              {parameter_address} const MandelFractalParameters &fractal,
                              thread MandelOrbitState &aux) {{
{body}
}}"#,
    )
}

fn emit_formula_namespace_with_body(
    formula: &ParsedFormula,
    parameter_address: &str,
    namespace: &str,
    body: &str,
    noinline_formulas: bool,
) -> Result<String> {
    let parameter_types = formula_parameter_types(formula)?;
    let dependencies = emit_formula_dependencies(formula)?;
    let matrix_helper = formula_matrix_helper(formula);
    let function_attribute = formula_function_attribute(noinline_formulas);
    Ok(format!(
        r#"namespace {namespace} {{
{parameter_types}

{dependencies}

{matrix_helper}

{function_attribute}static float4 {function_name}(float4 z,
                              {parameter_address} const MandelFractalParameters &fractal,
                              thread MandelOrbitState &aux) {{
{body}
}}
}}"#,
        function_name = formula.function_name
    ))
}

pub fn specialize_fpt_shader(
    base_source: &str,
    formula: &ParsedFormula,
    set_values: &[f32; 40],
    formula_values: &BTreeMap<String, String>,
    force_delta_de: bool,
    delta_de_function: u32,
) -> Result<String> {
    ensure!(
        supports_runtime_emitter(formula),
        "full Metal-FPT specialization has not lowered {} yet",
        formula.source.symbol
    );
    let marker = "// FPT_MANDELBULBER_GENERATED_INSERTION_POINT";
    ensure!(
        base_source.contains(marker),
        "Metal shader is missing {marker}"
    );
    let declarations = emit_scene_specialized_formula_declarations(
        formula,
        formula_values,
        "constant",
        false,
        FormulaSourceOptimization::default(),
    )?;
    let (parameter_initializer, actual_scale, initial_w, post_iteration) =
        match formula.source.internal_name.as_str() {
            "kaleidoscopic_ifs" => (
                ifs_constant_initializer(formula_values)?,
                "kMandelFormulaParameters.IFS.scale",
                "0.0f".into(),
                String::new(),
            ),
            _ => (
                if supports_bulb_parameter_block(formula) {
                    bulb_constant_initializer(formula_values)?
                } else if supports_analytic_de_parameter_block(formula) {
                    analytic_de_constant_initializer(formula_values)?
                } else if supports_transform_basic_parameter_block(formula) {
                    transform_basic_constant_initializer(formula_values)?
                } else if supports_jos_kleinian_parameter_block(formula) {
                    jos_kleinian_constant_initializer(formula_values)?
                } else if supports_pseudo_kleinian_parameter_block(formula) {
                    pseudo_kleinian_constant_initializer(formula_values)?
                } else if supports_generated_runtime_parameter_block(formula) {
                    generated_runtime_constant_initializer(formula, formula_values)?
                } else if supports_mandelbox_full_parameter_block(formula) {
                    mandelbox_full_constant_initializer(formula_values)?
                } else if supports_mandelbox_fast_parameter_block(formula) {
                    mandelbox_fast_constant_initializer(formula_values)?
                } else {
                    "constant MandelFractalParameters kMandelFormulaParameters = { 0u };".into()
                },
                "1.0f",
                metal_float(set_values[3]),
                constant_addition_source(formula.source.id, set_values),
            ),
        };
    let specialized_kernel_define = mandel_specialized_kernel_define(base_source);
    let fragment = if force_delta_de || formula.source.de_type == "deltaDEType" {
        standalone_delta_fragment(
            &declarations,
            &parameter_initializer,
            formula,
            actual_scale,
            &initial_w,
            &post_iteration,
            delta_de_function,
            &specialized_kernel_define,
        )?
    } else {
        let additional_bailout = i32::from(uses_additional_bailout(formula));
        let orbit_loop = format!(
            r#"    for (int iteration = 0; iteration < max_iterations; ++iteration) {{
        aux.i = uint(iteration);
        aux.old_z = z;
        z = mandelApplyGlobalFoldings(z, cfg, aux);
        z = {function_name}(z, kMandelFormulaParameters, aux);
{post_iteration}        aux.r = length(z);
        completed_iterations = iteration + 1;
        if (aux.r > bailout) {{
            escaped = true;
            break;
        }}
        if ({additional_bailout} != 0) {{
            escaped = true;
            if (length(z - aux.old_z) / aux.r < 0.1f / bailout) break;
        }}
    }}"#,
            function_name = formula.function_name,
        );
        let analytic_function =
            overridden_analytic_function(&formula.source.analytic_function, delta_de_function)?;
        let distance_expression =
            clamped_distance_expression(&analytic_distance_expression(analytic_function)?);
        format!(
            r#"#define FPT_MANDEL_GENERATED_FIELD 1
{specialized_kernel_define}
{declarations}


{parameter_initializer}

static MandelFormulaIterationCounts mandelbulberProfileFormulaIterations(
    int completed_iterations) {{
    MandelFormulaIterationCounts counts = {{}};
    counts.slots[0] = uint(max(completed_iterations, 0));
    return counts;
}}

static float4 mandelbulberGeneratedFieldSample(float3 p,
                                                constant FptRenderConfig &cfg,
                                                int iteration_multiplier,
                                                int iteration_budget) {{
    float world_scale = max(setv(cfg, 0), 1.0f);
    float3 scaled = p / world_scale;
    float4 z = float4(scaled.x, scaled.z, scaled.y, {initial_w});
    MandelOrbitState aux = {{}};
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
    int completed_iterations = 0;
    bool escaped = false;
    int configured_iterations = clamp(
        int(setv(cfg, 1)) * iteration_multiplier, 1, 4096);
    int max_iterations = iteration_budget > 0
        ? min(configured_iterations, iteration_budget)
        : configured_iterations;
    float bailout = max(setv(cfg, 2), 1.0f);
{orbit_loop}
    float distance = ({distance_expression}) * world_scale;
    float iteration_state = escaped
        ? -float(completed_iterations)
        : float(completed_iterations);
    return float4(distance, aux.r, aux.DE, iteration_state);
}}

{marker}"#,
        )
    };
    let specialized = base_source.replacen(marker, &fragment, 1);
    dump_specialized_shader_if_requested(&specialized)?;
    Ok(specialized)
}

fn dump_specialized_shader_if_requested(source: &str) -> Result<()> {
    let Some(path) = std::env::var_os("FPT_DUMP_MANDEL_SHADER") else {
        return Ok(());
    };
    std::fs::write(&path, source).with_context(|| {
        format!(
            "write specialized Mandel shader {}",
            Path::new(&path).display()
        )
    })
}

fn mandel_specialized_kernel_define(base_source: &str) -> String {
    let mut defines = if base_source.contains("FPT_MANDEL_GENERIC_KERNEL")
        || std::env::var_os("FPT_MANDEL_GENERIC_KERNEL").is_some()
    {
        String::new()
    } else {
        "#define FPT_MANDEL_SPECIALIZED_KERNEL 1".to_owned()
    };
    if std::env::var_os("FPT_MANDEL_INTERACTIVE_REFINEMENT").is_some() {
        defines.push_str("\n#define FPT_MANDEL_INTERACTIVE_REFINEMENT 1");
    }
    defines
}

fn standalone_delta_fragment(
    declarations: &str,
    parameter_initializer: &str,
    formula: &ParsedFormula,
    actual_scale: &str,
    initial_w: &str,
    post_iteration: &str,
    delta_de_function: u32,
    specialized_kernel_define: &str,
) -> Result<String> {
    let marker = "// FPT_MANDELBULBER_GENERATED_INSERTION_POINT";
    let de_function = overridden_de_function(&formula.source.de_function_type, delta_de_function)?;
    let distance_expression = delta_distance_expression(de_function)?;
    let additional_bailout = i32::from(uses_additional_bailout(formula));
    Ok(format!(
        r#"#define FPT_MANDEL_GENERATED_FIELD 1
{specialized_kernel_define}
{declarations}

{parameter_initializer}

struct MandelDeltaOrbitResult {{
    float4 z;
    float radius;
    float derivative;
    int iterations;
    bool escaped;
}};

static MandelFormulaIterationCounts mandelbulberProfileFormulaIterations(
    int completed_iterations) {{
    MandelFormulaIterationCounts counts = {{}};
    counts.slots[0] = uint(max(completed_iterations, 0)) * 4u;
    return counts;
}}

static MandelDeltaOrbitResult mandelDeltaOrbit(float3 scaled,
                                                constant FptRenderConfig &cfg,
                                                int forced_iterations,
                                                int iteration_budget) {{
    float4 z = float4(scaled.x, scaled.z, scaled.y, {initial_w});
    MandelOrbitState aux = {{}};
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
    int completed_iterations = 0;
    int iteration_multiplier = max(-forced_iterations, 1);
    int configured_iterations = clamp(
        int(setv(cfg, 1)) * iteration_multiplier, 1, 4096);
    int max_iterations = forced_iterations >= 0
        ? min(configured_iterations, forced_iterations)
        : (iteration_budget > 0
            ? min(configured_iterations, iteration_budget)
            : configured_iterations);
    bool escaped = false;
    float bailout = max(setv(cfg, 2), 1.0f);
    for (int iteration = 0; iteration < max_iterations; ++iteration) {{
        aux.i = uint(iteration);
        aux.old_z = z;
        z = mandelApplyGlobalFoldings(z, cfg, aux);
        z = {function_name}(z, kMandelFormulaParameters, aux);
{post_iteration}        aux.r = length(z);
        completed_iterations = iteration + 1;
        if (forced_iterations < 0 && aux.r > bailout) {{
            escaped = true;
            break;
        }}
        if (forced_iterations < 0 && {additional_bailout} != 0) {{
            escaped = true;
            if (length(z - aux.old_z) / aux.r < 0.1f / bailout) break;
        }}
    }}
    return {{ z, aux.r, aux.DE, completed_iterations, escaped }};
}}

static float4 mandelbulberGeneratedFieldSample(float3 p,
                                                constant FptRenderConfig &cfg,
                                                int iteration_multiplier,
                                                int iteration_budget) {{
    float world_scale = max(setv(cfg, 0), 1.0f);
    float3 scaled = p / world_scale;
    MandelDeltaOrbitResult base = mandelDeltaOrbit(
        scaled, cfg, -iteration_multiplier, iteration_budget);
    // Mandelbulber's fp64 implementation can tie this probe to a much smaller
    // fraction of the detail threshold. Metal is fp32-only; 1e-4 is the
    // empirically stable floor across explicit-delta and Newton fixtures.
    float delta = max(1.0e-4f, 1.0e-4f * length(scaled));
    float rx = mandelDeltaOrbit(scaled + float3(delta, 0.0f, 0.0f), cfg, base.iterations, 0).radius;
    float ry = mandelDeltaOrbit(scaled + float3(0.0f, delta, 0.0f), cfg, base.iterations, 0).radius;
    float rz = mandelDeltaOrbit(scaled + float3(0.0f, 0.0f, delta), cfg, base.iterations, 0).radius;
    float3 radial_derivative = abs(float3(rx, ry, rz) - base.radius) / delta;
    float radial_gradient = length(radial_derivative);
    float distance = radial_gradient > 0.0f ? ({distance_expression}) : base.radius;
    distance = clamp(distance, 0.0f, 10.0f) * world_scale;
    float iteration_state = base.escaped
        ? -float(base.iterations)
        : float(base.iterations);
    return float4(distance, base.radius, base.derivative, iteration_state);
}}

{marker}"#,
        function_name = formula.function_name
    ))
}

fn delta_distance_expression(function: &str) -> Result<&'static str> {
    match function {
        "linearDEFunction" => Ok("0.5f * base.radius / radial_gradient"),
        "logarithmicDEFunction" => {
            Ok("0.5f * base.radius * log(max(base.radius, 1.0e-30f)) / radial_gradient")
        }
        other => bail!("delta distance finalizer {other} is not supported by the runtime"),
    }
}

pub fn specialize_fpt_shader_hybrid(
    base_source: &str,
    slots: &[RuntimeFormulaSlot<'_>],
    sequence: &[u8],
    set_values: &[f32; 40],
    linear_de_offset: f64,
    force_delta_de: bool,
    delta_de_function: u32,
    direct_hybrid_loop: bool,
    formula_optimization: SceneFormulaOptimizationPolicy,
) -> Result<String> {
    ensure!(
        !slots.is_empty(),
        "hybrid runtime requires at least one formula"
    );
    ensure!(
        !sequence.is_empty(),
        "hybrid runtime sequence cannot be empty"
    );
    let marker = "// FPT_MANDELBULBER_GENERATED_INSERTION_POINT";
    ensure!(
        base_source.contains(marker),
        "Metal shader is missing {marker}"
    );
    let mut seen = [false; 9];
    let mut declarations = String::new();
    let mut dispatch_cases = String::new();
    let mut weights = [0.0_f64; 9];
    let mut bailouts = [1.0_f64; 9];
    let mut bailout_checks = [0_i32; 9];
    let mut additional_bailout_checks = [0_i32; 9];
    for slot in slots {
        ensure!(slot.index < 9, "hybrid formula slot index must be 0..8");
        ensure!(
            !seen[slot.index],
            "duplicate hybrid formula slot {}",
            slot.index + 1
        );
        seen[slot.index] = true;
        ensure!(
            supports_runtime_kernel(slot.formula),
            "hybrid formula {} kernel is not supported by the generated runtime",
            slot.formula.source.symbol
        );
        let namespace = format!("MandelSlot{}", slot.index);
        declarations.push_str(&emit_formula_namespace(
            slot.formula,
            slot.formula_values,
            "constant",
            &namespace,
            false,
            FormulaSourceOptimization::from_policy(formula_optimization),
        )?);
        declarations.push_str("\n\nnamespace ");
        declarations.push_str(&namespace);
        declarations.push_str(" {\n");
        declarations.push_str(&formula_constant_initializer(
            slot.formula,
            slot.formula_values,
        )?);
        declarations.push_str("\n}\n\n");

        dispatch_cases.push_str(&format!(
            "            case {}:\n                if (formula_weight > 0.0f) z = {}::{}(z, {}::kMandelFormulaParameters, aux);\n",
            slot.index,
            namespace,
            slot.formula.function_name,
            namespace
        ));
        dispatch_cases.push_str(&hybrid_constant_addition_source(
            slot.add_c_constant,
            slot.formula.source.id,
            set_values,
        ));
        dispatch_cases.push_str("                break;\n");
        weights[slot.index] = slot.weight;
        bailouts[slot.index] = slot.bailout;
        bailout_checks[slot.index] = i32::from(slot.check_for_bailout);
        additional_bailout_checks[slot.index] = i32::from(uses_additional_bailout(slot.formula));
    }
    for &index in sequence {
        ensure!(
            usize::from(index) < seen.len() && seen[usize::from(index)],
            "hybrid sequence references unresolved slot {}",
            usize::from(index) + 1
        );
    }
    let uses_delta = force_delta_de
        || slots
            .iter()
            .any(|slot| slot.formula.source.de_type == "deltaDEType");
    let sample_body = if uses_delta {
        let finalizer = hybrid_delta_distance_expression(slots, delta_de_function)?;
        format!(
            r#"    MandelHybridOrbitResult base = mandelHybridOrbit(
        scaled, cfg, -iteration_multiplier, iteration_budget);
    float delta = max(1.0e-4f, 1.0e-4f * length(scaled));
    float rx = mandelHybridOrbit(scaled + float3(delta, 0.0f, 0.0f), cfg, base.iterations, 0).radius;
    float ry = mandelHybridOrbit(scaled + float3(0.0f, delta, 0.0f), cfg, base.iterations, 0).radius;
    float rz = mandelHybridOrbit(scaled + float3(0.0f, 0.0f, delta), cfg, base.iterations, 0).radius;
    float3 radial_derivative = abs(float3(rx, ry, rz) - base.radius) / delta;
    float radial_gradient = length(radial_derivative);
    float distance = radial_gradient > 0.0f ? ({finalizer}) : base.radius;
    distance = clamp(distance, 0.0f, 10.0f) * world_scale;
    float iteration_state = base.escaped
        ? -float(base.iterations)
        : float(base.iterations);
    return float4(distance, base.radius, base.derivative, iteration_state);"#
        )
    } else {
        let finalizer = clamped_distance_expression(&hybrid_distance_expression(
            slots,
            linear_de_offset,
            delta_de_function,
        )?);
        format!(
            r#"    MandelHybridOrbitResult base = mandelHybridOrbit(
        scaled, cfg, -iteration_multiplier, iteration_budget);
    float4 z = base.z;
    MandelOrbitState aux = {{}};
    aux.r = base.radius;
    aux.DE = base.derivative;
    aux.dist = base.distance;
    aux.pseudoKleinianDE = base.pseudo_kleinian_de;
    float distance = ({finalizer}) * world_scale;
    float iteration_state = base.escaped
        ? -float(base.iterations)
        : float(base.iterations);
    return float4(distance, base.radius, base.derivative, iteration_state);"#
        )
    };
    let sequence_literal = sequence
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let weights_literal = weights.map(|value| metal_float(value as f32)).join(", ");
    let bailouts_literal = bailouts.map(|value| metal_float(value as f32)).join(", ");
    let checks_literal = bailout_checks.map(|value| value.to_string()).join(", ");
    let additional_checks_literal = additional_bailout_checks
        .map(|value| value.to_string())
        .join(", ");
    let initial_w = metal_float(set_values[3]);
    let profile_orbit_multiplier = if uses_delta { 4 } else { 1 };
    let direct_loop = if direct_hybrid_loop {
        hybrid_direct_loop_source(slots, sequence, set_values)?
    } else {
        None
    };
    let unrolled_periodic_loop = if formula_optimization.unrolled_periodic_hybrid_loop
        || std::env::var_os("FPT_MANDEL_UNROLL_PERIODIC_HYBRID").is_some()
    {
        hybrid_unrolled_periodic_loop_source(slots, sequence, set_values)?
    } else {
        None
    };
    let periodic_sequence_index = if formula_optimization.periodic_hybrid_loop {
        periodic_hybrid_sequence_expression(sequence)
    } else {
        None
    };
    let iteration_body = direct_loop.or(unrolled_periodic_loop).unwrap_or_else(|| {
        let previous_color = "        float previous_color = aux.color;\n";
        let blend_color =
            "            aux.color = aux.color * formula_weight + previous_color * inverse_weight;\n";
        let sequence_index = periodic_sequence_index.as_deref().unwrap_or(
            "int sequence_index = int(kMandelHybridSequence[iteration]);",
        );
        format!(
            r#"    for (int iteration = 0; iteration < max_iterations; ++iteration) {{
        {sequence_index}
        float formula_weight = kMandelHybridWeights[sequence_index];
        aux.i = uint(iteration);
        aux.old_z = z;
        z = mandelApplyGlobalFoldings(z, cfg, aux);
        float4 previous_z = z;
        float previous_de = aux.DE;
{previous_color}        switch (sequence_index) {{
{dispatch_cases}        }}
        if (formula_weight < 1.0f) {{
            z = mandelHybridSmoothVector(previous_z, z, formula_weight);
            float inverse_weight = 1.0f - formula_weight;
            aux.DE = aux.DE * formula_weight + previous_de * inverse_weight;
{blend_color}        }}
        aux.r = length(z);
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
            }}
        }}
    }}"#
        )
    });
    let specialized_kernel_define = mandel_specialized_kernel_define(base_source);
    let fragment = format!(
        r#"#define FPT_MANDEL_GENERATED_FIELD 1
{specialized_kernel_define}
{orbit_state}

{declarations}
constant uchar kMandelHybridSequence[{sequence_length}] = {{ {sequence_literal} }};
constant float kMandelHybridWeights[9] = {{ {weights_literal} }};
constant float kMandelHybridBailouts[9] = {{ {bailouts_literal} }};
constant int kMandelHybridBailoutChecks[9] = {{ {checks_literal} }};
constant int kMandelHybridAdditionalBailoutChecks[9] = {{ {additional_checks_literal} }};

static float4 mandelHybridSmoothVector(float4 first, float4 second, float weight) {{
    if (weight <= 0.0f) return first;
    if (weight >= 1.0f) return second;
    float inverse_weight = 1.0f - weight;
    float interpolated_length = length(first) * inverse_weight + length(second) * weight;
    float4 blended = first * inverse_weight + second * weight;
    float blended_length = length(blended);
    return blended_length > 0.0f ? blended * (interpolated_length / blended_length) : first;
}}

struct MandelHybridOrbitResult {{
    float4 z;
    float radius;
    float derivative;
    float distance;
    float pseudo_kleinian_de;
    int iterations;
    bool escaped;
}};

static MandelHybridOrbitResult mandelHybridOrbit(float3 scaled,
                                                  constant FptRenderConfig &cfg,
                                                  int forced_iterations,
                                                  int iteration_budget) {{
    float4 z = float4(scaled.x, scaled.z, scaled.y, {initial_w});
    MandelOrbitState aux = {{}};
    aux.c = z;
    aux.const_c = z;
    aux.old_z = z;
    aux.pos_neg = 1.0f;
    aux.r = length(z);
    aux.DE = 1.0f;
    aux.dist = 1000.0f;
    aux.pseudoKleinianDE = 1.0f;
    aux.actualScale = 1.0f;
    aux.color = 1.0f;
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
{iteration_body}
    return {{ z, aux.r, aux.DE, aux.dist, aux.pseudoKleinianDE,
              completed_iterations, escaped }};
}}

static MandelFormulaIterationCounts mandelbulberProfileFormulaIterations(
    int completed_iterations) {{
    MandelFormulaIterationCounts counts = {{}};
    int limit = min(max(completed_iterations, 0), {sequence_length});
    for (int iteration = 0; iteration < limit; ++iteration) {{
        uint slot = uint(kMandelHybridSequence[iteration]);
        counts.slots[slot] += {profile_orbit_multiplier}u;
    }}
    return counts;
}}

static float4 mandelbulberGeneratedFieldSample(float3 p,
                                                constant FptRenderConfig &cfg,
                                                int iteration_multiplier,
                                                int iteration_budget) {{
    float world_scale = max(setv(cfg, 0), 1.0f);
    float3 scaled = p / world_scale;
{sample_body}
}}

{marker}"#,
        orbit_state = orbit_state_declaration(),
        sequence_length = sequence.len(),
        iteration_body = iteration_body,
    );
    let specialized = base_source.replacen(marker, &fragment, 1);
    dump_specialized_shader_if_requested(&specialized)?;
    Ok(specialized)
}

fn periodic_hybrid_sequence_expression(sequence: &[u8]) -> Option<String> {
    for period in 2..=8usize.min(sequence.len()) {
        if sequence
            .iter()
            .enumerate()
            .any(|(index, slot)| *slot != sequence[index % period])
        {
            continue;
        }
        if sequence[..period]
            .iter()
            .enumerate()
            .any(|(index, slot)| usize::from(*slot) != index)
        {
            continue;
        }
        return Some(if period == 2 {
            "int sequence_index = iteration & 1;".to_owned()
        } else {
            format!("int sequence_index = iteration % {period};")
        });
    }
    None
}

fn hybrid_unrolled_periodic_loop_source(
    slots: &[RuntimeFormulaSlot<'_>],
    sequence: &[u8],
    set_values: &[f32; 40],
) -> Result<Option<String>> {
    let period = (2..=8usize.min(sequence.len())).find(|&period| {
        sequence
            .iter()
            .enumerate()
            .all(|(index, slot)| *slot == sequence[index % period])
            && sequence[..period]
                .iter()
                .enumerate()
                .all(|(index, slot)| usize::from(*slot) == index)
    });
    let Some(period) = period else {
        return Ok(None);
    };
    let mut phases = String::new();
    for phase in 0..period {
        let slot = slots
            .iter()
            .find(|slot| slot.index == phase)
            .ok_or_else(|| anyhow!("hybrid sequence references missing slot {}", phase + 1))?;
        if phase > 0 {
            phases.push_str(&format!(
                "        if (iteration + {phase} >= max_iterations) break;\n"
            ));
        }
        phases.push_str(&hybrid_unrolled_phase_source(slot, phase, set_values));
    }
    Ok(Some(format!(
        "    for (int iteration = 0; iteration < max_iterations; iteration += {period}) {{\n{phases}    }}"
    )))
}

fn hybrid_unrolled_phase_source(
    slot: &RuntimeFormulaSlot<'_>,
    phase: usize,
    set_values: &[f32; 40],
) -> String {
    let namespace = format!("MandelSlot{}", slot.index);
    let current_iteration = if phase == 0 {
        "iteration".to_owned()
    } else {
        format!("iteration + {phase}")
    };
    let previous_color = if slot.weight < 1.0 {
        "            float previous_color = aux.color;\n"
    } else {
        ""
    };
    let formula = if slot.weight > 0.0 {
        format!(
            "            z = {namespace}::{}(z, {namespace}::kMandelFormulaParameters, aux);\n",
            slot.formula.function_name
        )
    } else {
        String::new()
    };
    let addition =
        hybrid_constant_addition_source(slot.add_c_constant, slot.formula.source.id, set_values);
    let blend = if slot.weight < 1.0 {
        format!(
            r#"            float formula_weight = kMandelHybridWeights[{slot_index}];
            z = mandelHybridSmoothVector(previous_z, z, formula_weight);
            float inverse_weight = 1.0f - formula_weight;
            aux.DE = aux.DE * formula_weight + previous_de * inverse_weight;
            aux.color = aux.color * formula_weight + previous_color * inverse_weight;
"#,
            slot_index = slot.index,
        )
    } else {
        String::new()
    };
    let bailout = if slot.check_for_bailout {
        let additional = if uses_additional_bailout(slot.formula) {
            format!(
                r#"                escaped = true;
                if (length(z - aux.old_z) / aux.r < 0.1f / kMandelHybridBailouts[{slot_index}]) break;
"#,
                slot_index = slot.index,
            )
        } else {
            String::new()
        };
        format!(
            r#"            if (forced_iterations < 0
                && kMandelHybridBailoutChecks[{slot_index}] != 0) {{
                if (aux.r > kMandelHybridBailouts[{slot_index}]) {{
                    escaped = true;
                    break;
                }}
                if (kMandelHybridAdditionalBailoutChecks[{slot_index}] != 0) {{
{additional}                }}
            }}
"#,
            slot_index = slot.index,
        )
    } else {
        String::new()
    };
    format!(
        r#"        {{
            int current_iteration = {current_iteration};
            aux.i = uint(current_iteration);
            aux.old_z = z;
            z = mandelApplyGlobalFoldings(z, cfg, aux);
            float4 previous_z = z;
            float previous_de = aux.DE;
{previous_color}{formula}{addition}{blend}            aux.r = length(z);
            completed_iterations = current_iteration + 1;
{bailout}        }}
"#,
    )
}

fn hybrid_direct_loop_source(
    slots: &[RuntimeFormulaSlot<'_>],
    sequence: &[u8],
    set_values: &[f32; 40],
) -> Result<Option<String>> {
    let Some(&slot_index) = sequence.first() else {
        return Ok(None);
    };
    if sequence.iter().any(|candidate| *candidate != slot_index) {
        return Ok(None);
    }
    let slot = slots
        .iter()
        .find(|slot| slot.index == usize::from(slot_index))
        .ok_or_else(|| anyhow!("hybrid sequence references missing slot {}", slot_index + 1))?;
    let namespace = format!("MandelSlot{}", slot.index);
    let weight = metal_float(slot.weight as f32);
    let previous_color = if slot.weight < 1.0 {
        "        float previous_color = aux.color;\n"
    } else {
        ""
    };
    let mut formula = String::new();
    if slot.weight > 0.0 {
        formula.push_str(&format!(
            "        z = {namespace}::{}(z, {namespace}::kMandelFormulaParameters, aux);\n",
            slot.formula.function_name
        ));
    }
    formula.push_str(&hybrid_constant_addition_source(
        slot.add_c_constant,
        slot.formula.source.id,
        set_values,
    ));
    let blend = if slot.weight < 1.0 {
        format!(
            r#"        z = mandelHybridSmoothVector(previous_z, z, {weight});
        aux.DE = aux.DE * {weight} + previous_de * (1.0f - {weight});
        aux.color = aux.color * {weight} + previous_color * (1.0f - {weight});
"#
        )
    } else {
        String::new()
    };
    let bailout = if slot.check_for_bailout {
        let bailout = metal_float(slot.bailout as f32);
        let additional = if uses_additional_bailout(slot.formula) {
            format!(
                r#"            escaped = true;
            if (length(z - aux.old_z) / aux.r < 0.1f / {bailout}) break;
"#
            )
        } else {
            String::new()
        };
        format!(
            r#"        if (forced_iterations < 0) {{
            if (aux.r > {bailout}) {{
                escaped = true;
                break;
            }}
{additional}        }}
"#
        )
    } else {
        String::new()
    };
    Ok(Some(format!(
        r#"    for (int iteration = 0; iteration < max_iterations; ++iteration) {{
        aux.i = uint(iteration);
        aux.old_z = z;
        z = mandelApplyGlobalFoldings(z, cfg, aux);
        float4 previous_z = z;
        float previous_de = aux.DE;
{previous_color}{formula}{blend}        aux.r = length(z);
        completed_iterations = iteration + 1;
{bailout}    }}"#
    )))
}

fn uses_additional_bailout(formula: &ParsedFormula) -> bool {
    matches!(
        formula.source.de_function_type.as_str(),
        "pseudoKleinianDEFunction" | "josKleinianDEFunction"
    )
}

fn formula_function_attribute(noinline_formulas: bool) -> &'static str {
    if noinline_formulas {
        "__attribute__((noinline)) "
    } else {
        ""
    }
}

fn formula_constant_initializer(
    formula: &ParsedFormula,
    formula_values: &BTreeMap<String, String>,
) -> Result<String> {
    if formula.source.internal_name == "kaleidoscopic_ifs" {
        ifs_constant_initializer(formula_values)
    } else if supports_bulb_parameter_block(formula) {
        bulb_constant_initializer(formula_values)
    } else if supports_analytic_de_parameter_block(formula) {
        analytic_de_constant_initializer(formula_values)
    } else if supports_transform_basic_parameter_block(formula) {
        transform_basic_constant_initializer(formula_values)
    } else if supports_jos_kleinian_parameter_block(formula) {
        jos_kleinian_constant_initializer(formula_values)
    } else if supports_pseudo_kleinian_parameter_block(formula) {
        pseudo_kleinian_constant_initializer(formula_values)
    } else if supports_generated_runtime_parameter_block(formula) {
        generated_runtime_constant_initializer(formula, formula_values)
    } else if supports_mandelbox_full_parameter_block(formula) {
        mandelbox_full_constant_initializer(formula_values)
    } else if supports_mandelbox_fast_parameter_block(formula) {
        mandelbox_fast_constant_initializer(formula_values)
    } else {
        Ok("constant MandelFractalParameters kMandelFormulaParameters = { 0u };".into())
    }
}

fn hybrid_constant_addition_source(enabled: bool, formula_id: i32, values: &[f32; 40]) -> String {
    if !enabled {
        return String::new();
    }
    let multiplier = [values[9], values[10], values[11]].map(metal_float);
    let swapped = matches!(formula_id, 64 | 73);
    if values[5] >= 0.5 {
        let julia = [values[6], values[7], values[8]].map(metal_float);
        if swapped {
            format!(
                "                z += float4({} * {}, {} * {}, {} * {}, 0.0f);\n",
                julia[1], multiplier[1], julia[0], multiplier[0], julia[2], multiplier[2]
            )
        } else {
            format!(
                "                z += float4({} * {}, {} * {}, {} * {}, 0.0f);\n",
                julia[0], multiplier[0], julia[1], multiplier[1], julia[2], multiplier[2]
            )
        }
    } else if swapped {
        format!(
            "                z += float4(aux.const_c.y * {}, aux.const_c.x * {}, aux.const_c.z * {}, 0.0f);\n",
            multiplier[0], multiplier[1], multiplier[2]
        )
    } else {
        format!(
            "                z += aux.const_c * float4({}, {}, {}, 1.0f);\n",
            multiplier[0], multiplier[1], multiplier[2]
        )
    }
}

fn hybrid_distance_expression(
    slots: &[RuntimeFormulaSlot<'_>],
    linear_de_offset: f64,
    delta_de_function: u32,
) -> Result<String> {
    let selected = hybrid_selected_de_function(slots, delta_de_function)?;
    match selected {
        0 => Ok(format!(
            "aux.DE > 0.0f ? (aux.r - {}) / aux.DE : aux.r",
            metal_float(linear_de_offset as f32)
        )),
        1 => Ok(
            "aux.DE > 0.0f ? (aux.r > 1.0f ? 0.5f * aux.r * log(aux.r) / aux.DE : 0.0f) : aux.r"
                .into(),
        ),
        2 => Ok(
            "aux.DE > 0.0f ? max(length(z.xy) - aux.pseudoKleinianDE, abs(length(z.xy) * z.z) / aux.r) / aux.DE : aux.r"
                .into(),
        ),
        3 => {
            let primary = slots
                .iter()
                .find(|slot| slot.index == 0)
                .ok_or_else(|| anyhow!("Jos Kleinian hybrid finalizer requires formula slot 1"))?;
            let spheres_enabled =
                formula_bool(primary.formula_values, "transf_spheres_enabled", true)?;
            let folding_value =
                formula_scalar(primary.formula_values, "transf_folding_value", 2.0)?;
            let tweak005 =
                formula_scalar(primary.formula_values, "analyticDE_tweak_005", 0.05)?;
            let offset1 =
                formula_scalar(primary.formula_values, "analyticDE_offset_1", 1.0)?;
            let y = if spheres_enabled {
                format!("min(z.y, {} - z.y)", metal_float(folding_value))
            } else {
                "z.y".into()
            };
            Ok(format!(
                "aux.DE > 0.0f ? min({y}, {}) / max(aux.DE, {}) : aux.r",
                metal_float(tweak005),
                metal_float(offset1)
            ))
        }
        4 => Ok("aux.dist".into()),
        5 => Ok(
            "aux.DE > 0.0f ? max(abs(z.x), max(abs(z.y), abs(z.z))) / aux.DE : aux.r"
                .into(),
        ),
        _ => unreachable!(),
    }
}

fn hybrid_delta_distance_expression(
    slots: &[RuntimeFormulaSlot<'_>],
    delta_de_function: u32,
) -> Result<String> {
    Ok(match hybrid_selected_de_function(slots, delta_de_function)? {
        0 => "0.5f * base.radius / radial_gradient".into(),
        1 => {
            "0.5f * base.radius * log(max(base.radius, 1.0e-30f)) / radial_gradient".into()
        }
        2 => "max(length(base.z.xy) - 0.92784f, abs(length(base.z.xy) * base.z.z) / max(base.radius, 1.0e-30f)) / radial_gradient".into(),
        3 => "(abs(length(base.z.xz) * base.z.y) / max(base.radius, 1.0e-30f)) / radial_gradient".into(),
        4 => "base.radius".into(),
        5 => "0.5f * max(abs(base.z.x), max(abs(base.z.y), abs(base.z.z))) / max(radial_derivative.x, max(radial_derivative.y, radial_derivative.z))".into(),
        _ => unreachable!(),
    })
}

fn hybrid_selected_de_function(
    slots: &[RuntimeFormulaSlot<'_>],
    delta_de_function: u32,
) -> Result<usize> {
    if delta_de_function != 0 {
        ensure!(
            delta_de_function <= 6,
            "delta DE function override must be 0..6"
        );
        return Ok(delta_de_function as usize - 1);
    }
    let mut counts = [0_u64; 6];
    for slot in slots {
        let index = match slot.formula.source.de_function_type.as_str() {
            "linearDEFunction" => 0,
            "logarithmicDEFunction" => 1,
            "pseudoKleinianDEFunction" => 2,
            "josKleinianDEFunction" => 3,
            "customDEFunction" => 4,
            "maxAxisDEFunction" => 5,
            "withoutDEFunction" => continue,
            other => bail!("hybrid DE function {other} is not supported yet"),
        };
        counts[index] += u64::from(slot.iterations);
    }
    // Mandelbulber always selects custom/dIFS distance when present. Otherwise
    // it selects the most frequently scheduled type, with enum order breaking
    // ties because its comparison is strictly greater-than.
    Ok(select_hybrid_de_function(counts))
}

fn overridden_de_function(preferred: &str, override_value: u32) -> Result<&str> {
    Ok(match override_value {
        0 => preferred,
        1 => "linearDEFunction",
        2 => "logarithmicDEFunction",
        3 => "pseudoKleinianDEFunction",
        4 => "josKleinianDEFunction",
        5 => "customDEFunction",
        6 => "maxAxisDEFunction",
        other => bail!("delta DE function override {other} must be 0..6"),
    })
}

fn overridden_analytic_function(preferred: &str, override_value: u32) -> Result<&str> {
    Ok(match override_value {
        0 => preferred,
        1 => "analyticFunctionLinear",
        2 => "analyticFunctionLogarithmic",
        3 => "analyticFunctionPseudoKleinian",
        4 => "analyticFunctionJosKleinian",
        5 => "analyticFunctionCustomDE",
        6 => "analyticFunctionMaxAxis",
        other => bail!("delta DE function override {other} must be 0..6"),
    })
}

fn select_hybrid_de_function(counts: [u64; 6]) -> usize {
    if counts[4] > 0 {
        return 4;
    }
    counts
        .iter()
        .enumerate()
        .max_by_key(|(index, count)| (**count, std::cmp::Reverse(*index)))
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn constant_addition_source(formula_id: i32, values: &[f32; 40]) -> String {
    if values[4] < 0.5 {
        return String::new();
    }
    let multiplier = [values[9], values[10], values[11]].map(metal_float);
    let swapped = matches!(formula_id, 64 | 73);
    if values[5] >= 0.5 {
        let julia = [values[6], values[7], values[8]].map(metal_float);
        if swapped {
            format!(
                "        z += float4({} * {}, {} * {}, {} * {}, 0.0f);\n",
                julia[1], multiplier[1], julia[0], multiplier[0], julia[2], multiplier[2]
            )
        } else {
            format!(
                "        z += float4({} * {}, {} * {}, {} * {}, 0.0f);\n",
                julia[0], multiplier[0], julia[1], multiplier[1], julia[2], multiplier[2]
            )
        }
    } else if swapped {
        format!(
            "        z += float4(aux.const_c.y * {}, aux.const_c.x * {}, aux.const_c.z * {}, 0.0f);\n",
            multiplier[0], multiplier[1], multiplier[2]
        )
    } else {
        format!(
            "        z += aux.const_c * float4({}, {}, {}, 1.0f);\n",
            multiplier[0], multiplier[1], multiplier[2]
        )
    }
}

fn clamped_distance_expression(expression: &str) -> String {
    format!("clamp(float({expression}), 0.0f, 10.0f)")
}

fn analytic_distance_expression(function: &str) -> Result<String> {
    match function {
        "analyticFunctionLogarithmic" => Ok(
            "aux.DE > 0.0f ? (aux.r > 1.0f ? 0.5f * aux.r * log(aux.r) / aux.DE : 0.0f) : aux.r"
                .into(),
        ),
        "analyticFunctionLinear" => Ok("aux.DE > 0.0f ? aux.r / aux.DE : aux.r".into()),
        "analyticFunctionIFS" => {
            Ok("aux.DE > 0.0f ? (aux.r - 2.0f) / aux.DE : aux.r".into())
        }
        "analyticFunctionJosKleinian" => Ok(
            "aux.DE > 0.0f ? min(kMandelFormulaParameters.transformCommon.spheresEnabled != 0 ? min(z.y, kMandelFormulaParameters.transformCommon.foldingValue - z.y) : z.y, kMandelFormulaParameters.analyticDE.tweak005) / max(aux.DE, kMandelFormulaParameters.analyticDE.offset1) : aux.r"
                .into(),
        ),
        "analyticFunctionPseudoKleinian" => Ok(
            "aux.DE > 0.0f ? max(length(z.xy) - aux.pseudoKleinianDE, abs(length(z.xy) * z.z) / aux.r) / aux.DE : aux.r"
                .into(),
        ),
        "analyticFunctionCustomDE" => Ok("aux.dist".into()),
        "analyticFunctionMaxAxis" => Ok(
            "aux.DE > 0.0f ? max(abs(z.x), max(abs(z.y), abs(z.z))) / aux.DE : aux.r"
                .into(),
        ),
        other => bail!("analytic distance finalizer {other} is not supported by the runtime"),
    }
}

fn ifs_constant_initializer(values: &BTreeMap<String, String>) -> Result<String> {
    let absolute = [
        formula_bool(values, "IFS_abs_x", false)?,
        formula_bool(values, "IFS_abs_y", false)?,
        formula_bool(values, "IFS_abs_z", false)?,
    ];
    let mut enabled = Vec::with_capacity(9);
    let mut rotations = Vec::with_capacity(9);
    let mut directions = Vec::with_capacity(9);
    let mut distances = Vec::with_capacity(9);
    let mut intensities = Vec::with_capacity(9);
    for plane in 0..9 {
        enabled.push(u32::from(formula_bool(
            values,
            &format!("IFS_enabled_{plane}"),
            false,
        )?));
        let rotation = formula_vector3(values, &format!("IFS_rotations_{plane}"), [0.0; 3])?
            .map(|value| f64::from(value).to_radians());
        rotations.push(matrix_literal(rotation3_matrix(rotation)));
        let direction = normalized_formula_vector(
            formula_vector3(values, &format!("IFS_direction_{plane}"), [1.0, 0.0, 0.0])?,
            &format!("IFS_direction_{plane}"),
        )?;
        directions.push(format!(
            "{{ {}, {}, {}, 0.0f }}",
            metal_float(direction[0]),
            metal_float(direction[1]),
            metal_float(direction[2])
        ));
        distances.push(metal_float(formula_scalar(
            values,
            &format!("IFS_distance_{plane}"),
            0.0,
        )?));
        intensities.push(metal_float(formula_scalar(
            values,
            &format!("IFS_intensity_{plane}"),
            1.0,
        )?));
    }
    let main_rotation = formula_vector3(values, "IFS_rotation", [0.0; 3])?
        .map(|value| f64::from(value).to_radians());
    let offset = formula_vector3(values, "IFS_offset", [1.0, 0.0, 0.0])?;
    let edge = formula_vector3(values, "IFS_edge", [0.0; 3])?;
    let scale = formula_scalar(values, "IFS_scale", 2.0)?;
    ensure!(
        scale.is_finite() && scale != 0.0,
        "IFS_scale must be finite and non-zero"
    );
    Ok(format!(
        r#"constant MandelFractalParameters kMandelFormulaParameters = {{
    {{
        {abs_x}, {abs_y}, {abs_z},
        {{ {enabled} }},
        {{
            {rotations}
        }},
        {{
            {directions}
        }},
        {{ {distances} }},
        {{ {intensities} }},
        {rotation_enabled},
        {main_rotation},
        {{ {offset_x}, {offset_y}, {offset_z}, 0.0f }},
        {edge_enabled},
        {{ {edge_x}, {edge_y}, {edge_z} }},
        {scale},
        {menger_sponge_mode}
    }}
}};"#,
        abs_x = u32::from(absolute[0]),
        abs_y = u32::from(absolute[1]),
        abs_z = u32::from(absolute[2]),
        enabled = enabled
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", "),
        rotations = rotations.join(",\n            "),
        directions = directions.join(",\n            "),
        distances = distances.join(", "),
        intensities = intensities.join(", "),
        rotation_enabled = i32::from(formula_bool(values, "IFS_rotation_enabled", false)?),
        main_rotation = matrix_literal(rotation3_matrix(main_rotation)),
        offset_x = metal_float(offset[0]),
        offset_y = metal_float(offset[1]),
        offset_z = metal_float(offset[2]),
        edge_enabled = i32::from(formula_bool(values, "IFS_edge_enabled", false)?),
        edge_x = metal_float(edge[0]),
        edge_y = metal_float(edge[1]),
        edge_z = metal_float(edge[2]),
        scale = metal_float(scale),
        menger_sponge_mode = i32::from(formula_bool(values, "IFS_menger_sponge_mode", false)?),
    ))
}

fn normalized_formula_vector(value: [f32; 3], name: &str) -> Result<[f32; 3]> {
    let length = value
        .iter()
        .map(|component| component * component)
        .sum::<f32>()
        .sqrt();
    ensure!(
        length.is_finite() && length > 0.0,
        "{name} must be finite and non-zero"
    );
    Ok(value.map(|component| component / length))
}

fn metal_float(value: f32) -> String {
    if value.is_finite() {
        let mut value = format!("{value:.9}");
        while value.contains('.') && value.ends_with('0') {
            value.pop();
        }
        if value.ends_with('.') {
            value.push('0');
        }
        value.push('f');
        value
    } else if value.is_nan() {
        "NAN".into()
    } else if value.is_sign_negative() {
        "-INFINITY".into()
    } else {
        "INFINITY".into()
    }
}

fn rotation3_matrix(rotation: [f64; 3]) -> [[f64; 3]; 3] {
    let (sz, cz) = rotation[0].sin_cos();
    let (sy, cy) = rotation[1].sin_cos();
    let (sx, cx) = rotation[2].sin_cos();
    [
        [cz * cy, cz * sy * sx - sz * cx, cz * sy * cx + sz * sx],
        [sz * cy, sz * sy * sx + cz * cx, sz * sy * cx - cz * sx],
        [-sy, cy * sx, cy * cx],
    ]
}

fn matrix_literal(matrix: [[f64; 3]; 3]) -> String {
    format!(
        "{{ {{ {}, {}, {} }}, {{ {}, {}, {} }}, {{ {}, {}, {} }} }}",
        metal_float(matrix[0][0] as f32),
        metal_float(matrix[0][1] as f32),
        metal_float(matrix[0][2] as f32),
        metal_float(matrix[1][0] as f32),
        metal_float(matrix[1][1] as f32),
        metal_float(matrix[1][2] as f32),
        metal_float(matrix[2][0] as f32),
        metal_float(matrix[2][1] as f32),
        metal_float(matrix[2][2] as f32),
    )
}

pub fn compile_formula(
    root: &Path,
    identifier: &str,
    output: &Path,
    check_metal: bool,
) -> Result<MetalCompileReport> {
    let formula = parse_formula(root, identifier)?;
    let metal = emit_metal(&formula)?;
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, metal).with_context(|| format!("write {}", output.display()))?;
    let air_output = if check_metal {
        let air = output.with_extension("air");
        check_metal_source(output, &air)?;
        Some(air.display().to_string())
    } else {
        None
    };
    Ok(MetalCompileReport {
        formula: frontend_report(&formula),
        output: output.display().to_string(),
        metal_checked: check_metal,
        air_output,
    })
}

pub fn supports_runtime_emitter(formula: &ParsedFormula) -> bool {
    supports_runtime_kernel(formula)
        && if formula.source.de_type == "deltaDEType" {
            matches!(
                formula.source.de_function_type.as_str(),
                "linearDEFunction" | "logarithmicDEFunction"
            )
        } else {
            matches!(
                formula.source.analytic_function.as_str(),
                "analyticFunctionLogarithmic"
                    | "analyticFunctionLinear"
                    | "analyticFunctionIFS"
                    | "analyticFunctionJosKleinian"
                    | "analyticFunctionPseudoKleinian"
                    | "analyticFunctionCustomDE"
            )
        }
}

pub fn supports_runtime_kernel(formula: &ParsedFormula) -> bool {
    formula.source.internal_name == "kaleidoscopic_ifs"
        || (supports_runtime_parameter_block(formula)
            && matches!(
                formula.source.de_type.as_str(),
                "analyticDEType" | "deltaDEType"
            ))
}

fn supports_runtime_parameter_block(formula: &ParsedFormula) -> bool {
    formula.parameter_reads.is_empty()
        || supports_bulb_parameter_block(formula)
        || supports_analytic_de_parameter_block(formula)
        || supports_transform_basic_parameter_block(formula)
        || supports_jos_kleinian_parameter_block(formula)
        || supports_pseudo_kleinian_parameter_block(formula)
        || supports_generated_runtime_parameter_block(formula)
        || supports_mandelbox_full_parameter_block(formula)
        || supports_mandelbox_fast_parameter_block(formula)
}

fn supports_bulb_parameter_block(formula: &ParsedFormula) -> bool {
    !formula.parameter_reads.is_empty()
        && formula.parameter_reads.iter().all(|path| {
            matches!(
                path.as_str(),
                "bulb.alphaAngleOffset"
                    | "bulb.betaAngleOffset"
                    | "bulb.gammaAngleOffset"
                    | "bulb.power"
            )
        })
}

fn supports_analytic_de_parameter_block(formula: &ParsedFormula) -> bool {
    !formula.parameter_reads.is_empty()
        && formula.parameter_reads.iter().all(|path| {
            matches!(
                path.as_str(),
                "analyticDE.enabled"
                    | "analyticDE.enabledFalse"
                    | "analyticDE.scale1"
                    | "analyticDE.tweak005"
                    | "analyticDE.offset0"
                    | "analyticDE.offset1"
                    | "analyticDE.offset2"
                    | "analyticDE.startIterationsA"
                    | "analyticDE.stopIterationsA"
            )
        })
}

fn supports_transform_basic_parameter_block(formula: &ParsedFormula) -> bool {
    !formula.parameter_reads.is_empty()
        && formula.parameter_reads.iter().all(|path| {
            matches!(
                path.as_str(),
                "transformCommon.additionConstant0000"
                    | "transformCommon.scale3"
                    | "transformCommon.pwr8"
                    | "transformCommon.pwr8a"
            )
        })
}

fn supports_jos_kleinian_parameter_block(formula: &ParsedFormula) -> bool {
    formula.source.internal_name == "jos_kleinian"
        && formula.parameter_reads.iter().all(|path| {
            matches!(
                path.as_str(),
                "analyticDE.scale1"
                    | "foldColor.auxColorEnabled"
                    | "foldColor.auxColorEnabledAFalse"
                    | "foldColor.auxColorEnabledFalse"
                    | "foldColor.difs0000.w"
                    | "foldColor.difs0000.x"
                    | "foldColor.difs0000.y"
                    | "foldColor.difs0000.z"
                    | "foldColor.difs1"
                    | "foldColor.startIterationsA"
                    | "foldColor.stopIterationsA"
                    | "transformCommon.additionConstant000"
                    | "transformCommon.additionConstantP000.x"
                    | "transformCommon.additionConstantP000.y"
                    | "transformCommon.additionConstantP000.z"
                    | "transformCommon.constantMultiplierC111.x"
                    | "transformCommon.constantMultiplierC111.y"
                    | "transformCommon.constantMultiplierC111.z"
                    | "transformCommon.foldingValue"
                    | "transformCommon.functionEnabledAFalse"
                    | "transformCommon.maxR2d1"
                    | "transformCommon.offset"
                    | "transformCommon.offset000"
                    | "transformCommon.offset111"
                    | "transformCommon.scale3D222.x"
                    | "transformCommon.scale3D222.y"
                    | "transformCommon.scale3D222.z"
                    | "transformCommon.sphereInversionEnabledFalse"
                    | "transformCommon.startIterationsC"
                    | "transformCommon.startIterationsT"
                    | "transformCommon.stopIterationsC"
                    | "transformCommon.stopIterationsT"
            )
        })
}

fn supports_pseudo_kleinian_parameter_block(formula: &ParsedFormula) -> bool {
    formula.source.internal_name == "pseudo_kleinian"
        && formula.parameter_reads.iter().all(|path| {
            matches!(
                path.as_str(),
                "analyticDE.offset0"
                    | "analyticDE.scale1"
                    | "analyticDE.tweak005"
                    | "foldColor.auxColorEnabledAFalse"
                    | "foldColor.auxColorEnabledFalse"
                    | "foldColor.difs0000.w"
                    | "foldColor.difs0000.x"
                    | "foldColor.difs0000.y"
                    | "foldColor.difs0000.z"
                    | "foldColor.difs1"
                    | "foldColor.startIterationsA"
                    | "foldColor.stopIterationsA"
                    | "mandelbox.color.factor.x"
                    | "mandelbox.color.factor.y"
                    | "mandelbox.color.factor.z"
                    | "mandelbox.foldingLimit"
                    | "mandelbox.foldingValue"
                    | "transformCommon.additionConstant000"
                    | "transformCommon.additionConstant0777"
                    | "transformCommon.additionConstant111.x"
                    | "transformCommon.additionConstant111.y"
                    | "transformCommon.additionConstant111.z"
                    | "transformCommon.additionConstantA000"
                    | "transformCommon.additionConstantP000.x"
                    | "transformCommon.additionConstantP000.y"
                    | "transformCommon.additionConstantP000.z"
                    | "transformCommon.constantMultiplier000"
                    | "transformCommon.constantMultiplier111.x"
                    | "transformCommon.constantMultiplier111.y"
                    | "transformCommon.constantMultiplier111.z"
                    | "transformCommon.constantMultiplierC111.x"
                    | "transformCommon.constantMultiplierC111.y"
                    | "transformCommon.constantMultiplierC111.z"
                    | "transformCommon.functionEnabledAFalse"
                    | "transformCommon.functionEnabledAxFalse"
                    | "transformCommon.functionEnabledBxFalse"
                    | "transformCommon.functionEnabledBy"
                    | "transformCommon.functionEnabledByFalse"
                    | "transformCommon.functionEnabledNFalse"
                    | "transformCommon.functionEnabledPFalse"
                    | "transformCommon.functionEnabledRFalse"
                    | "transformCommon.functionEnabledwFalse"
                    | "transformCommon.maxR2d1"
                    | "transformCommon.minR05"
                    | "transformCommon.offset000"
                    | "transformCommon.rotationMatrix"
                    | "transformCommon.scale"
                    | "transformCommon.scale1"
                    | "transformCommon.sphereInversionEnabledFalse"
                    | "transformCommon.startIterationsA"
                    | "transformCommon.startIterationsC"
                    | "transformCommon.startIterationsE"
                    | "transformCommon.startIterationsP"
                    | "transformCommon.startIterationsR"
                    | "transformCommon.startIterationsT"
                    | "transformCommon.startIterationsX"
                    | "transformCommon.stopIterations1"
                    | "transformCommon.stopIterationsA"
                    | "transformCommon.stopIterationsC"
                    | "transformCommon.stopIterationsE"
                    | "transformCommon.stopIterationsP1"
                    | "transformCommon.stopIterationsR"
                    | "transformCommon.stopIterationsT"
            )
        })
}

fn supports_generated_runtime_parameter_block(formula: &ParsedFormula) -> bool {
    generated_runtime_bindings(formula).is_some()
}

fn generated_runtime_bindings(
    formula: &ParsedFormula,
) -> Option<Vec<OwnedRuntimeParameterBinding>> {
    let mut bindings = GENERATED_RUNTIME_PARAMETER_BINDINGS
        .iter()
        .filter(|binding| {
            formula
                .parameter_reads
                .iter()
                .any(|read| binding_covers_read(binding.member_path, read))
        })
        .map(|binding| OwnedRuntimeParameterBinding {
            member_path: binding.member_path.into(),
            scene_key: binding.scene_key.into(),
            value: binding.value,
            metal_type: None,
            enum_values: Vec::new(),
        })
        .collect::<Vec<_>>();

    if std::env::var_os("FPT_DEBUG_MANDEL_BINDINGS").is_some() {
        eprintln!(
            "Mandel formula path {} root {:?}",
            formula.source.path.display(),
            mandelbulber_root(formula).map(Path::to_path_buf)
        );
    }
    if let Ok(source_root) = mandelbulber_root(formula)
        && let Some(schema) = runtime_binding_schema(source_root, formula)
    {
        if std::env::var_os("FPT_DEBUG_MANDEL_BINDINGS").is_some() {
            eprintln!("Mandel binding source root {}", source_root.display());
        }
        for mapping in &schema.mappings {
            if mapping.indexed
                || bindings.iter().any(|binding| {
                    binding_covers_read(&binding.member_path, &mapping.member_expression)
                        || binding_covers_read(&mapping.member_expression, &binding.member_path)
                })
                || !formula
                    .parameter_reads
                    .iter()
                    .any(|read| binding_covers_read(&mapping.member_expression, read))
            {
                continue;
            }
            let Some(default) = schema.defaults.get(&mapping.scene_key) else {
                continue;
            };
            let Some(value) = runtime_binding_value(&mapping.value_type, default) else {
                continue;
            };
            bindings.push(OwnedRuntimeParameterBinding {
                member_path: mapping.member_expression.clone(),
                scene_key: mapping.scene_key.clone(),
                value,
                metal_type: schema.field_types.get(&mapping.member_expression).cloned(),
                enum_values: schema
                    .enum_values
                    .get(&mapping.scene_key)
                    .cloned()
                    .unwrap_or_default(),
            });
        }
    }
    bindings.sort_by(|left, right| left.member_path.cmp(&right.member_path));
    if std::env::var_os("FPT_DEBUG_MANDEL_BINDINGS").is_some() {
        let uncovered = formula
            .parameter_reads
            .iter()
            .filter(|read| {
                !bindings
                    .iter()
                    .any(|binding| binding_covers_read(&binding.member_path, read))
            })
            .collect::<Vec<_>>();
        eprintln!(
            "Mandel bindings {}: {} bound, uncovered {:?}",
            formula.source.symbol,
            bindings.len(),
            uncovered
        );
    }
    formula
        .parameter_reads
        .iter()
        .all(|read| {
            bindings
                .iter()
                .any(|binding| binding_covers_read(&binding.member_path, read))
        })
        .then_some(bindings)
}

fn runtime_binding_schema(
    source_root: &Path,
    formula: &ParsedFormula,
) -> Option<Arc<RuntimeBindingSchema>> {
    static CACHE: OnceLock<Mutex<BTreeMap<std::path::PathBuf, Arc<RuntimeBindingSchema>>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    if let Some(schema) = cache.lock().ok()?.get(source_root).cloned() {
        return Some(schema);
    }
    let parameters = super::catalog::parameter_defaults(source_root).ok()?;
    let defaults = parameters
        .iter()
        .filter(|parameter| !parameter.indexed)
        .map(|parameter| (parameter.name.clone(), parameter.default_expression.clone()))
        .collect();
    let enum_values = parameters
        .into_iter()
        .filter(|parameter| !parameter.enum_values.is_empty())
        .map(|parameter| (parameter.name, parameter.enum_values))
        .collect();
    let mappings = super::catalog::runtime_parameter_mappings(source_root).ok()?;
    let translated_schema = translated_fractal_schema(formula).ok()?;
    let field_types = shallow_metal_field_types(&translated_schema);
    let enum_declarations = translated_enum_declarations(&translated_schema);
    let schema = Arc::new(RuntimeBindingSchema {
        defaults,
        enum_values,
        mappings,
        field_types,
        enum_declarations,
    });
    cache
        .lock()
        .ok()?
        .insert(source_root.to_path_buf(), Arc::clone(&schema));
    Some(schema)
}

fn shallow_metal_field_types(source: &str) -> BTreeMap<String, String> {
    let mut structs = BTreeMap::<String, BTreeMap<String, String>>::new();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find("struct ") {
        let start = offset + relative + "struct ".len();
        let name_end = source[start..]
            .find(|character: char| character.is_whitespace() || character == '{')
            .map(|relative| start + relative)
            .unwrap_or(source.len());
        let name = source[start..name_end].trim();
        let Some(open) = source[name_end..]
            .find('{')
            .map(|relative| name_end + relative)
        else {
            break;
        };
        let Some(close) = matching_delimiter(source, open, '{', '}') else {
            break;
        };
        let mut fields = BTreeMap::new();
        for declaration in source[open + 1..close].split(';') {
            let declaration = declaration.split_whitespace().collect::<Vec<_>>();
            if declaration.len() < 2 {
                continue;
            }
            let field = declaration.last().unwrap().split('[').next().unwrap_or("");
            if field.is_empty() {
                continue;
            }
            fields.insert(
                field.to_owned(),
                declaration[..declaration.len() - 1].join(" "),
            );
        }
        if !name.is_empty() {
            structs.insert(name.to_owned(), fields);
        }
        offset = close + 1;
    }
    fn visit(
        prefix: &str,
        struct_name: &str,
        structs: &BTreeMap<String, BTreeMap<String, String>>,
        result: &mut BTreeMap<String, String>,
    ) {
        let Some(fields) = structs.get(struct_name) else {
            return;
        };
        for (field, field_type) in fields {
            let path = if prefix.is_empty() {
                field.clone()
            } else {
                format!("{prefix}.{field}")
            };
            if structs.contains_key(field_type) {
                visit(&path, field_type, structs, result);
            } else {
                result.insert(path, field_type.clone());
            }
        }
    }
    let mut result = BTreeMap::new();
    visit("", "sFractalCl", &structs, &mut result);
    result
}

fn translated_enum_declarations(source: &str) -> String {
    let mut output = String::new();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find("enum ") {
        let start = offset + relative;
        let Some(open) = source[start..].find('{').map(|relative| start + relative) else {
            break;
        };
        let Some(close) = matching_delimiter(source, open, '{', '}') else {
            break;
        };
        let end = source[close..]
            .find(';')
            .map(|relative| close + relative + 1)
            .unwrap_or(close + 1);
        output.push_str(&source[start..end]);
        output.push('\n');
        offset = end;
    }
    output
}

fn runtime_binding_value(value_type: &str, default: &str) -> Option<RuntimeBindingValue> {
    match value_type {
        "bool" => Some(RuntimeBindingValue::Bool(default.trim() == "true")),
        "int" => parse_cpp_integer(default).map(RuntimeBindingValue::Integer),
        "double" => default
            .trim()
            .parse::<f32>()
            .ok()
            .map(RuntimeBindingValue::Float),
        "CVector3" | "CVector4" => {
            let scene_components = if value_type == "CVector3" { 3 } else { 4 };
            parse_cpp_vector(default, scene_components).map(|default| RuntimeBindingValue::Float4 {
                default,
                scene_components,
            })
        }
        _ => None,
    }
}

fn parse_cpp_integer(value: &str) -> Option<i32> {
    let value = value.trim();
    value.parse().ok().or_else(|| {
        let open = value.rfind('(')?;
        let close = value.rfind(')')?;
        value[open + 1..close].trim().parse().ok()
    })
}

fn parse_cpp_vector(value: &str, components: usize) -> Option<[f32; 4]> {
    let open = value.find('(')?;
    let close = value.rfind(')')?;
    let values = value[open + 1..close]
        .split(',')
        .map(|component| component.trim().parse::<f32>())
        .collect::<std::result::Result<Vec<_>, _>>()
        .ok()?;
    if values.len() != components {
        return None;
    }
    let mut result = [0.0; 4];
    result[..components].copy_from_slice(&values);
    Some(result)
}

fn binding_covers_read(member_path: &str, read: &str) -> bool {
    read == member_path
        || read
            .strip_prefix(member_path)
            .is_some_and(|suffix| matches!(suffix, ".x" | ".y" | ".z" | ".w" | ".xyz"))
}

fn supports_mandelbox_fast_parameter_block(formula: &ParsedFormula) -> bool {
    !formula.parameter_reads.is_empty()
        && formula.parameter_reads.iter().all(|path| {
            matches!(
                path.as_str(),
                "mandelbox.fR2"
                    | "mandelbox.mR2"
                    | "mandelbox.mainRot"
                    | "mandelbox.mainRotationEnabled"
                    | "mandelbox.mboxFactor1"
                    | "mandelbox.scale"
            )
        })
}

fn supports_mandelbox_full_parameter_block(formula: &ParsedFormula) -> bool {
    !formula.parameter_reads.is_empty()
        && formula.parameter_reads.iter().any(|path| {
            matches!(
                path.as_str(),
                "mandelbox.foldingLimit"
                    | "mandelbox.foldingValue"
                    | "mandelbox.offset"
                    | "mandelbox.rot"
                    | "mandelbox.rotationsEnabled"
                    | "mandelbox.rotinv"
            ) || path.starts_with("mandelbox.color.")
        })
        && formula.parameter_reads.iter().all(|path| {
            matches!(
                path.as_str(),
                "mandelbox.color.factor"
                    | "mandelbox.color.factor.x"
                    | "mandelbox.color.factor.y"
                    | "mandelbox.color.factor.z"
                    | "mandelbox.color.factorSp1"
                    | "mandelbox.color.factorSp2"
                    | "mandelbox.fR2"
                    | "mandelbox.foldingLimit"
                    | "mandelbox.foldingValue"
                    | "mandelbox.mR2"
                    | "mandelbox.mainRot"
                    | "mandelbox.mainRotationEnabled"
                    | "mandelbox.mboxFactor1"
                    | "mandelbox.offset"
                    | "mandelbox.rot"
                    | "mandelbox.rotationsEnabled"
                    | "mandelbox.rotinv"
                    | "mandelbox.scale"
            )
        })
}

fn bulb_constant_initializer(values: &BTreeMap<String, String>) -> Result<String> {
    let power = formula_scalar(values, "power", 9.0)?;
    let alpha = formula_scalar(values, "alpha_angle_offset", 0.0)?.to_radians();
    let beta = formula_scalar(values, "beta_angle_offset", 0.0)?.to_radians();
    let gamma = formula_scalar(values, "gamma_angle_offset", 0.0)?.to_radians();
    Ok(format!(
        "constant MandelFractalParameters kMandelFormulaParameters = {{ {{ {}, {}, {}, {} }} }};",
        metal_float(alpha),
        metal_float(beta),
        metal_float(gamma),
        metal_float(power)
    ))
}

fn analytic_de_constant_initializer(values: &BTreeMap<String, String>) -> Result<String> {
    let enabled = formula_bool(values, "analyticDE_enabled", true)?;
    let enabled_false = formula_bool(values, "analyticDE_enabled_false", false)?;
    let scale = formula_scalar(values, "analyticDE_scale_1", 1.0)?;
    let tweak = formula_scalar(values, "analyticDE_tweak_005", 0.05)?;
    let offset0 = formula_scalar(values, "analyticDE_offset_0", 0.0)?;
    let offset1 = formula_scalar(values, "analyticDE_offset_1", 1.0)?;
    let offset2 = formula_scalar(values, "analyticDE_offset_2", 1.0)?;
    let start = formula_integer(values, "analyticDE_start_iterations_A", 0)?;
    let stop = formula_integer(values, "analyticDE_stop_iterations_A", 250)?;
    Ok(format!(
        "constant MandelFractalParameters kMandelFormulaParameters = {{ {{ {}, {}, {}, {}, {}, {}, {}, {}, {} }} }};",
        i32::from(enabled),
        i32::from(enabled_false),
        metal_float(scale),
        metal_float(tweak),
        metal_float(offset0),
        metal_float(offset1),
        metal_float(offset2),
        start,
        stop
    ))
}

fn transform_basic_constant_initializer(values: &BTreeMap<String, String>) -> Result<String> {
    let addition = formula_vector4(values, "transf_addition_constant_0000", [0.0; 4])?;
    let scale3 = formula_scalar(values, "transf_scale_3", 3.0)?;
    let power8 = formula_scalar(values, "transf_pwr_8", 8.0)?;
    let power8a = formula_scalar(values, "transf_pwr_8a", 8.0)?;
    Ok(format!(
        "constant MandelFractalParameters kMandelFormulaParameters = {{ {{ float4({}, {}, {}, {}), {}, {}, {} }} }};",
        metal_float(addition[0]),
        metal_float(addition[1]),
        metal_float(addition[2]),
        metal_float(addition[3]),
        metal_float(scale3),
        metal_float(power8),
        metal_float(power8a)
    ))
}

fn jos_kleinian_constant_initializer(values: &BTreeMap<String, String>) -> Result<String> {
    let vector4 = |name: &str, fallback: [f32; 3], w: f32| -> Result<String> {
        let value = formula_vector3(values, name, fallback)?;
        Ok(format!(
            "float4({}, {}, {}, {})",
            metal_float(value[0]),
            metal_float(value[1]),
            metal_float(value[2]),
            metal_float(w)
        ))
    };
    let difs = formula_vector4(values, "fold_color_difs_0000", [0.0; 4])?;
    Ok(format!(
        r#"constant MandelFractalParameters kMandelFormulaParameters = {{
    {{ {analytic_scale}, {analytic_tweak}, {analytic_offset} }},
    {{
        {color_enabled}, {color_grid_enabled}, {color_add_enabled},
        float4({difs_x}, {difs_y}, {difs_z}, {difs_w}), {difs_mix},
        {color_start}, {color_stop}
    }},
    {{
        {addition}, {addition_p}, {constant_c}, {offset_zero}, {offset_one}, {scale_three},
        {folding_value}, {max_radius}, {offset},
        {grid_y_enabled}, {sphere_inversion_enabled}, {spheres_enabled},
        {start_c}, {start_t}, {stop_c}, {stop_t}
    }}
}};"#,
        analytic_scale = metal_float(formula_scalar(values, "analyticDE_scale_1", 1.0)?),
        analytic_tweak = metal_float(formula_scalar(values, "analyticDE_tweak_005", 0.05)?),
        analytic_offset = metal_float(formula_scalar(values, "analyticDE_offset_1", 1.0)?),
        color_enabled = i32::from(formula_bool(values, "fold_color_aux_color_enabled", true)?),
        color_grid_enabled = i32::from(formula_bool(
            values,
            "fold_color_aux_color_enabledA_false",
            false,
        )?),
        color_add_enabled = i32::from(formula_bool(
            values,
            "fold_color_aux_color_enabled_false",
            false,
        )?),
        difs_x = metal_float(difs[0]),
        difs_y = metal_float(difs[1]),
        difs_z = metal_float(difs[2]),
        difs_w = metal_float(difs[3]),
        difs_mix = metal_float(formula_scalar(values, "fold_color_difs1", 1.0)?),
        color_start = formula_integer(values, "fold_color_start_iterations_A", 0)?,
        color_stop = formula_integer(values, "fold_color_stop_iterations_A", 250)?,
        addition = vector4("transf_addition_constant", [0.0; 3], 0.0)?,
        addition_p = vector4("transf_addition_constantP_000", [0.0; 3], 0.0)?,
        constant_c = vector4("transf_constant_multiplierC_111", [1.0; 3], 1.0)?,
        offset_zero = vector4("transf_offset_000", [0.0; 3], 0.0)?,
        offset_one = vector4("transf_offset_111", [1.0; 3], 0.0)?,
        scale_three = vector4("transf_scale3D_222", [2.0; 3], 1.0)?,
        folding_value = metal_float(formula_scalar(values, "transf_folding_value", 2.0)?),
        max_radius = metal_float(formula_scalar(values, "transf_maxR2_1", 1.0)?),
        offset = metal_float(formula_scalar(values, "transf_offset", 0.0)?),
        grid_y_enabled = i32::from(formula_bool(
            values,
            "transf_function_enabledA_false",
            false,
        )?),
        sphere_inversion_enabled = i32::from(formula_bool(
            values,
            "transf_sphere_inversion_enabled_false",
            false,
        )?),
        spheres_enabled = i32::from(formula_bool(values, "transf_spheres_enabled", true)?),
        start_c = formula_integer(values, "transf_start_iterations_C", 0)?,
        start_t = formula_integer(values, "transf_start_iterations_T", 0)?,
        stop_c = formula_integer(values, "transf_stop_iterations_C", 250)?,
        stop_t = formula_integer(values, "transf_stop_iterations_T", 250)?,
    ))
}

fn pseudo_kleinian_constant_initializer(values: &BTreeMap<String, String>) -> Result<String> {
    let vector4 = |name: &str, fallback: [f32; 3], w: f32| -> Result<String> {
        let value = formula_vector3(values, name, fallback)?;
        Ok(format!(
            "float4({}, {}, {}, {})",
            metal_float(value[0]),
            metal_float(value[1]),
            metal_float(value[2]),
            metal_float(w)
        ))
    };
    let boolean = |name: &str, fallback: bool| -> Result<i32> {
        Ok(i32::from(formula_bool(values, name, fallback)?))
    };
    let integer = |name: &str, fallback: i32| formula_integer(values, name, fallback);
    let difs = formula_vector4(values, "fold_color_difs_0000", [0.0; 4])?;
    let color = formula_vector3(values, "mandelbox_color", [0.03, 0.05, 0.07])?;
    let rotation = formula_vector3(values, "transf_rotation", [0.0; 3])?.map(f32::to_radians);
    Ok(format!(
        r#"constant MandelFractalParameters kMandelFormulaParameters = {{
    {{ {offset0}, {analytic_scale}, {analytic_tweak} }},
    {{
        {color_grid_enabled}, {color_add_enabled},
        float4({difs_x}, {difs_y}, {difs_z}, {difs_w}), {difs_mix},
        {color_start}, {color_stop}
    }},
    {{ {{ float3({color_x}, {color_y}, {color_z}) }}, {fold_limit}, {fold_value} }},
    {{
        {addition}, {addition_size}, {addition_one}, {addition_a}, {addition_p}, {offset_zero},
        {constant_zero}, {constant_one}, {constant_c},
        {rotation},
        {max_radius}, {min_radius}, {scale}, {scale_one},
        {enabled_a}, {enabled_ax}, {enabled_bx}, {enabled_by}, {enabled_by_fold},
        {enabled_n}, {enabled_p}, {enabled_r}, {enabled_w}, {sphere_inversion},
        {start_a}, {start_c}, {start_e}, {start_p}, {start_r}, {start_t}, {start_x},
        {stop_one}, {stop_a}, {stop_c}, {stop_e}, {stop_p}, {stop_r}, {stop_t}
    }}
}};"#,
        offset0 = metal_float(formula_scalar(values, "analyticDE_offset_0", 0.0)?),
        analytic_scale = metal_float(formula_scalar(values, "analyticDE_scale_1", 1.0)?),
        analytic_tweak = metal_float(formula_scalar(values, "analyticDE_tweak_005", 0.05)?),
        color_grid_enabled = boolean("fold_color_aux_color_enabledA_false", false)?,
        color_add_enabled = boolean("fold_color_aux_color_enabled_false", false)?,
        difs_x = metal_float(difs[0]),
        difs_y = metal_float(difs[1]),
        difs_z = metal_float(difs[2]),
        difs_w = metal_float(difs[3]),
        difs_mix = metal_float(formula_scalar(values, "fold_color_difs1", 1.0)?),
        color_start = integer("fold_color_start_iterations_A", 0)?,
        color_stop = integer("fold_color_stop_iterations_A", 250)?,
        color_x = metal_float(color[0]),
        color_y = metal_float(color[1]),
        color_z = metal_float(color[2]),
        fold_limit = metal_float(formula_scalar(values, "mandelbox_folding_limit", 1.0)?),
        fold_value = metal_float(formula_scalar(values, "mandelbox_folding_value", 2.0)?),
        addition = vector4("transf_addition_constant", [0.0; 3], 0.0)?,
        addition_size = vector4("transf_addition_constant_0777", [0.7; 3], 0.0)?,
        addition_one = vector4("transf_addition_constant_111", [1.0; 3], 0.0)?,
        addition_a = vector4("transf_addition_constantA_000", [0.0; 3], 0.0)?,
        addition_p = vector4("transf_addition_constantP_000", [0.0; 3], 0.0)?,
        offset_zero = vector4("transf_offset_000", [0.0; 3], 0.0)?,
        constant_zero = vector4("transf_constant_multiplier_000", [0.0; 3], 1.0)?,
        constant_one = vector4("transf_constant_multiplier_111", [1.0; 3], 1.0)?,
        constant_c = vector4("transf_constant_multiplierC_111", [1.0; 3], 1.0)?,
        rotation = row_matrix_literal(rotation2_matrix(rotation)),
        max_radius = metal_float(formula_scalar(values, "transf_maxR2_1", 1.0)?),
        min_radius = metal_float(formula_scalar(values, "transf_minimum_radius_05", 0.5)?),
        scale = metal_float(formula_scalar(values, "transf_scale", 1.0)?),
        scale_one = metal_float(formula_scalar(values, "transf_scale_1", 1.0)?),
        enabled_a = boolean("transf_function_enabledA_false", false)?,
        enabled_ax = boolean("transf_function_enabledAx_false", false)?,
        enabled_bx = boolean("transf_function_enabledBx_false", false)?,
        enabled_by = boolean("transf_function_enabledBy", true)?,
        enabled_by_fold = boolean("transf_function_enabledBy_false", false)?,
        enabled_n = boolean("transf_function_enabledN_false", false)?,
        enabled_p = boolean("transf_function_enabledP_false", false)?,
        enabled_r = boolean("transf_function_enabledR_false", false)?,
        enabled_w = boolean("transf_function_enabledw_false", false)?,
        sphere_inversion = boolean("transf_sphere_inversion_enabled_false", false)?,
        start_a = integer("transf_start_iterations_A", 0)?,
        start_c = integer("transf_start_iterations_C", 0)?,
        start_e = integer("transf_start_iterations_E", 0)?,
        start_p = integer("transf_start_iterations_P", 0)?,
        start_r = integer("transf_start_iterations_R", 0)?,
        start_t = integer("transf_start_iterations_T", 0)?,
        start_x = integer("transf_start_iterations_X", 0)?,
        stop_one = integer("transf_stop_iterations_1", 1)?,
        stop_a = integer("transf_stop_iterations_A", 250)?,
        stop_c = integer("transf_stop_iterations_C", 250)?,
        stop_e = integer("transf_stop_iterations_E", 250)?,
        stop_p = integer("transf_stop_iterations_P1", 1)?,
        stop_r = integer("transf_stop_iterations_R", 250)?,
        stop_t = integer("transf_stop_iterations_T", 250)?,
    ))
}

#[derive(Clone, Default)]
struct RuntimeParameterNode {
    fields: BTreeMap<String, OwnedRuntimeParameterBinding>,
    children: BTreeMap<String, RuntimeParameterNode>,
}

fn generated_runtime_parameter_tree(formula: &ParsedFormula) -> Result<RuntimeParameterNode> {
    let bindings = generated_runtime_bindings(formula).ok_or_else(|| {
        anyhow!(
            "formula {} has no generated runtime parameter schema",
            formula.source.internal_name
        )
    })?;
    let mut root = RuntimeParameterNode::default();
    for binding in bindings {
        let components = binding.member_path.split('.').collect::<Vec<_>>();
        ensure!(
            components.len() >= 2,
            "runtime binding {} has no parameter root",
            binding.member_path
        );
        let mut node = &mut root;
        for component in &components[..components.len() - 1] {
            node = node.children.entry((*component).to_owned()).or_default();
        }
        node.fields
            .insert(components.last().unwrap().to_string(), binding);
    }
    Ok(root)
}

fn generated_runtime_parameter_types(formula: &ParsedFormula) -> Result<String> {
    let tree = generated_runtime_parameter_tree(formula)?;
    let mut output = String::new();
    if let Ok(source_root) = mandelbulber_root(formula)
        && let Some(schema) = runtime_binding_schema(source_root, formula)
    {
        output.push_str(&schema.enum_declarations);
        output.push('\n');
    }
    output.push_str("struct matrix33 {\n    float3 m1;\n    float3 m2;\n    float3 m3;\n};\n\n");
    for (name, node) in &tree.children {
        emit_runtime_parameter_node(&mut output, &[name.as_str()], node);
    }
    output.push_str("struct MandelFractalParameters {\n");
    for root in tree.children.keys() {
        output.push_str("    ");
        output.push_str(&generated_runtime_struct_name(&[root.as_str()]));
        output.push(' ');
        output.push_str(root);
        output.push_str(";\n");
    }
    output.push_str("};");
    Ok(output)
}

fn emit_runtime_parameter_node(output: &mut String, path: &[&str], node: &RuntimeParameterNode) {
    for (name, child) in &node.children {
        let mut child_path = path.to_vec();
        child_path.push(name);
        emit_runtime_parameter_node(output, &child_path, child);
    }
    output.push_str("struct ");
    output.push_str(&generated_runtime_struct_name(path));
    output.push_str(" {\n");
    for (name, field) in &node.fields {
        output.push_str("    ");
        output.push_str(&runtime_binding_metal_type(field));
        output.push(' ');
        output.push_str(name);
        output.push_str(&runtime_binding_array_suffix(field));
        output.push_str(";\n");
    }
    for name in node.children.keys() {
        let mut child_path = path.to_vec();
        child_path.push(name);
        output.push_str("    ");
        output.push_str(&generated_runtime_struct_name(&child_path));
        output.push(' ');
        output.push_str(name);
        output.push_str(";\n");
    }
    output.push_str("};\n\n");
}

fn runtime_binding_metal_type(field: &OwnedRuntimeParameterBinding) -> String {
    field.metal_type.clone().unwrap_or_else(|| {
        match field.value {
            RuntimeBindingValue::Float(_)
            | RuntimeBindingValue::SquaredFloat(_)
            | RuntimeBindingValue::SquareRatio { .. }
            | RuntimeBindingValue::Ratio { .. }
            | RuntimeBindingValue::SquareRoot(_)
            | RuntimeBindingValue::ReciprocalSquareRoot(_)
            | RuntimeBindingValue::Reciprocal(_)
            | RuntimeBindingValue::Radians(_)
            | RuntimeBindingValue::CosineDegrees(_)
            | RuntimeBindingValue::SineDegrees(_) => "float",
            RuntimeBindingValue::Bool(_) | RuntimeBindingValue::Integer(_) => "int",
            RuntimeBindingValue::Float4 { .. } => "float4",
            RuntimeBindingValue::RotationMatrix
            | RuntimeBindingValue::RotationMatrix4
            | RuntimeBindingValue::MandelboxRotations { .. } => "matrix33",
            RuntimeBindingValue::GeneralizedFoldNormals { .. } => "float3",
        }
        .into()
    })
}

fn runtime_binding_array_suffix(field: &OwnedRuntimeParameterBinding) -> String {
    match field.value {
        RuntimeBindingValue::MandelboxRotations { .. } => "[2][3]".into(),
        RuntimeBindingValue::GeneralizedFoldNormals { count } => format!("[{count}]"),
        _ => String::new(),
    }
}

fn generated_runtime_struct_name(path: &[&str]) -> String {
    let mut name = String::from("MandelGenerated");
    for component in path {
        let mut uppercase = true;
        for character in component.chars() {
            if character == '_' {
                uppercase = true;
            } else if uppercase {
                name.extend(character.to_uppercase());
                uppercase = false;
            } else {
                name.push(character);
            }
        }
    }
    name.push_str("Parameters");
    name
}

fn generated_runtime_constant_initializer(
    formula: &ParsedFormula,
    values: &BTreeMap<String, String>,
) -> Result<String> {
    let tree = generated_runtime_parameter_tree(formula)?;
    let mut group_literals = Vec::with_capacity(tree.children.len());
    for node in tree.children.values() {
        group_literals.push(runtime_parameter_node_literal(node, values)?);
    }
    Ok(format!(
        "constant MandelFractalParameters kMandelFormulaParameters = {{ {} }};",
        group_literals.join(", ")
    ))
}

fn runtime_parameter_node_literal(
    node: &RuntimeParameterNode,
    values: &BTreeMap<String, String>,
) -> Result<String> {
    let mut literals = Vec::with_capacity(node.fields.len() + node.children.len());
    for field in node.fields.values() {
        literals.push(runtime_parameter_literal(field, values)?);
    }
    for child in node.children.values() {
        literals.push(runtime_parameter_node_literal(child, values)?);
    }
    Ok(format!("{{ {} }}", literals.join(", ")))
}

fn runtime_parameter_literal(
    field: &OwnedRuntimeParameterBinding,
    values: &BTreeMap<String, String>,
) -> Result<String> {
    let literal = match field.value {
        RuntimeBindingValue::Float(default) => {
            metal_float(formula_scalar(values, &field.scene_key, default)?)
        }
        RuntimeBindingValue::SquaredFloat(default) => {
            let value = formula_scalar(values, &field.scene_key, default)?;
            metal_float(value * value)
        }
        RuntimeBindingValue::SquareRatio {
            denominator_key,
            numerator_default,
            denominator_default,
        } => {
            let numerator = formula_scalar(values, &field.scene_key, numerator_default)?;
            let denominator = formula_scalar(values, denominator_key, denominator_default)?;
            metal_float((numerator * numerator) / (denominator * denominator))
        }
        RuntimeBindingValue::Ratio {
            denominator_key,
            numerator_default,
            denominator_default,
        } => {
            let numerator = formula_scalar(values, &field.scene_key, numerator_default)?;
            let denominator = formula_scalar(values, denominator_key, denominator_default)?;
            metal_float(numerator / denominator)
        }
        RuntimeBindingValue::SquareRoot(default) => {
            metal_float(formula_scalar(values, &field.scene_key, default)?.sqrt())
        }
        RuntimeBindingValue::ReciprocalSquareRoot(default) => {
            metal_float(1.0 / formula_scalar(values, &field.scene_key, default)?.sqrt())
        }
        RuntimeBindingValue::Reciprocal(default) => {
            metal_float(1.0 / formula_scalar(values, &field.scene_key, default)?)
        }
        RuntimeBindingValue::Radians(default) => {
            metal_float(formula_scalar(values, &field.scene_key, default)?.to_radians())
        }
        RuntimeBindingValue::CosineDegrees(default) => metal_float(
            formula_scalar(values, &field.scene_key, default)?
                .to_radians()
                .cos(),
        ),
        RuntimeBindingValue::SineDegrees(default) => metal_float(
            formula_scalar(values, &field.scene_key, default)?
                .to_radians()
                .sin(),
        ),
        RuntimeBindingValue::Bool(default) => {
            i32::from(formula_bool(values, &field.scene_key, default)?).to_string()
        }
        RuntimeBindingValue::Integer(default) => {
            formula_integer_or_enum(values, &field.scene_key, default, &field.enum_values)?
                .to_string()
        }
        RuntimeBindingValue::Float4 {
            default,
            scene_components,
        } => {
            let value = if scene_components == 3 {
                let xyz = formula_vector3(
                    values,
                    &field.scene_key,
                    [default[0], default[1], default[2]],
                )?;
                [xyz[0], xyz[1], xyz[2], default[3]]
            } else {
                ensure!(
                    scene_components == 4,
                    "generated float4 bindings must read three or four components"
                );
                formula_vector4(values, &field.scene_key, default)?
            };
            if field.metal_type.as_deref() == Some("float3") {
                format!(
                    "float3({}, {}, {})",
                    metal_float(value[0]),
                    metal_float(value[1]),
                    metal_float(value[2])
                )
            } else {
                format!(
                    "float4({}, {}, {}, {})",
                    metal_float(value[0]),
                    metal_float(value[1]),
                    metal_float(value[2]),
                    metal_float(value[3])
                )
            }
        }
        RuntimeBindingValue::RotationMatrix => {
            let rotation =
                formula_vector3(values, &field.scene_key, [0.0; 3])?.map(f32::to_radians);
            row_matrix_literal(rotation2_matrix(rotation))
        }
        RuntimeBindingValue::RotationMatrix4 => {
            let rotation =
                formula_vector3(values, &field.scene_key, [0.0; 3])?.map(f32::to_radians);
            row_matrix_literal(rotation4_matrix(rotation))
        }
        RuntimeBindingValue::MandelboxRotations { inverse } => {
            mandelbox_rotation_grid_literal(values, inverse)?
        }
        RuntimeBindingValue::GeneralizedFoldNormals { count } => {
            generalized_fold_normals_literal(&field.member_path, count)?
        }
    };
    if field
        .metal_type
        .as_deref()
        .is_some_and(|field_type| field_type.starts_with("enum"))
    {
        Ok(format!("{}({literal})", runtime_binding_metal_type(field)))
    } else {
        Ok(literal)
    }
}

fn mandelbox_fast_constant_initializer(values: &BTreeMap<String, String>) -> Result<String> {
    let scale = formula_scalar(values, "mandelbox_scale", 2.0)?;
    let fixed_radius = formula_scalar(values, "mandelbox_folding_fixed_radius", 1.0)?;
    let minimum_radius = formula_scalar(values, "mandelbox_folding_min_radius", 0.5)?;
    let fixed_radius_squared = fixed_radius * fixed_radius;
    let minimum_radius_squared = minimum_radius * minimum_radius;
    let factor = fixed_radius_squared / minimum_radius_squared;
    let rotation_enabled = formula_bool(values, "mandelbox_main_rotation_enabled", false)?;
    let rotation = formula_vector3(values, "mandelbox_rotation_main", [0.0; 3])?;
    let rotation = rotation2_matrix(rotation.map(f32::to_radians));
    Ok(format!(
        "constant MandelFractalParameters kMandelFormulaParameters = {{ {{ {}, {}, {}, {}, {}, {} }} }};",
        metal_float(scale),
        metal_float(fixed_radius_squared),
        metal_float(minimum_radius_squared),
        metal_float(factor),
        i32::from(rotation_enabled),
        row_matrix_literal(rotation)
    ))
}

fn mandelbox_full_constant_initializer(values: &BTreeMap<String, String>) -> Result<String> {
    let scale = formula_scalar(values, "mandelbox_scale", 2.0)?;
    let folding_limit = formula_scalar(values, "mandelbox_folding_limit", 1.0)?;
    let folding_value = formula_scalar(values, "mandelbox_folding_value", 2.0)?;
    let fixed_radius = formula_scalar(values, "mandelbox_folding_fixed_radius", 1.0)?;
    let minimum_radius = formula_scalar(values, "mandelbox_folding_min_radius", 0.5)?;
    let fixed_radius_squared = fixed_radius * fixed_radius;
    let minimum_radius_squared = minimum_radius * minimum_radius;
    let minimum_radius_factor = fixed_radius_squared / minimum_radius_squared;
    let offset = formula_vector3(values, "mandelbox_offset", [0.0; 3])?;
    let rotations_enabled = formula_bool(values, "mandelbox_rotations_enabled", false)?;
    let main_rotation_enabled = formula_bool(values, "mandelbox_main_rotation_enabled", false)?;
    let main_rotation = formula_vector3(values, "mandelbox_rotation_main", [0.0; 3])?;
    let main_rotation = rotation2_matrix(main_rotation.map(f32::to_radians));
    let color = formula_vector3(values, "mandelbox_color", [0.03, 0.05, 0.07])?;
    let color_sp1 = formula_scalar(values, "mandelbox_color_Sp1", 0.2)?;
    let color_sp2 = formula_scalar(values, "mandelbox_color_Sp2", 0.2)?;

    let mut rotations = [[[[0.0_f32; 3]; 3]; 3]; 2];
    let mut inverse_rotations = rotations;
    for (fold, name) in ["neg", "pos"].into_iter().enumerate() {
        for axis in 0..3 {
            let key = format!("mandelbox_rotation_{name}_{}", axis + 1);
            let angles = formula_vector3(values, &key, [0.0; 3])?.map(f32::to_radians);
            rotations[fold][axis] = rotation2_matrix(angles);
            inverse_rotations[fold][axis] = transpose_matrix(rotations[fold][axis]);
        }
    }

    Ok(format!(
        "constant MandelFractalParameters kMandelFormulaParameters = {{ {{ {}, {}, {}, {}, {}, {}, float4({}, {}, {}, 0.0f), {}, {}, {}, {}, {}, {{ float3({}, {}, {}), {}, {} }} }} }};",
        metal_float(scale),
        metal_float(folding_limit),
        metal_float(folding_value),
        metal_float(fixed_radius_squared),
        metal_float(minimum_radius_squared),
        metal_float(minimum_radius_factor),
        metal_float(offset[0]),
        metal_float(offset[1]),
        metal_float(offset[2]),
        i32::from(rotations_enabled),
        i32::from(main_rotation_enabled),
        row_matrix_literal(main_rotation),
        matrix_grid_literal(rotations),
        matrix_grid_literal(inverse_rotations),
        metal_float(color[0]),
        metal_float(color[1]),
        metal_float(color[2]),
        metal_float(color_sp1),
        metal_float(color_sp2),
    ))
}

fn mandelbox_rotation_grid_literal(
    values: &BTreeMap<String, String>,
    inverse: bool,
) -> Result<String> {
    let mut rotations = [[[[0.0_f32; 3]; 3]; 3]; 2];
    for (fold, name) in ["neg", "pos"].into_iter().enumerate() {
        for (axis, target) in rotations[fold].iter_mut().enumerate() {
            let key = format!("mandelbox_rotation_{name}_{}", axis + 1);
            let angles = formula_vector3(values, &key, [0.0; 3])?.map(f32::to_radians);
            let rotation = rotation2_matrix(angles);
            *target = if inverse {
                transpose_matrix(rotation)
            } else {
                rotation
            };
        }
    }
    Ok(matrix_grid_literal(rotations))
}

fn generalized_fold_normals_literal(member_path: &str, count: usize) -> Result<String> {
    let inverse_sqrt_three = 1.0_f32 / 3.0_f32.sqrt();
    let oct = vec![
        [inverse_sqrt_three, inverse_sqrt_three, -inverse_sqrt_three],
        [inverse_sqrt_three, -inverse_sqrt_three, inverse_sqrt_three],
        [-inverse_sqrt_three, inverse_sqrt_three, inverse_sqrt_three],
        [
            -inverse_sqrt_three,
            -inverse_sqrt_three,
            -inverse_sqrt_three,
        ],
        [inverse_sqrt_three, inverse_sqrt_three, inverse_sqrt_three],
        [-inverse_sqrt_three, -inverse_sqrt_three, inverse_sqrt_three],
        [-inverse_sqrt_three, inverse_sqrt_three, -inverse_sqrt_three],
        [inverse_sqrt_three, -inverse_sqrt_three, -inverse_sqrt_three],
    ];
    let cube = vec![
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ];
    let golden_ratio = (1.0_f32 + 5.0_f32.sqrt()) * 0.5;
    let b = 1.0 / (golden_ratio * golden_ratio + 1.0).sqrt();
    let dodeca = vec![
        [0.0, b, golden_ratio * b],
        [0.0, b, -golden_ratio * b],
        [0.0, -b, golden_ratio * b],
        [0.0, -b, -golden_ratio * b],
        [b, golden_ratio * b, 0.0],
        [b, -golden_ratio * b, 0.0],
        [-b, golden_ratio * b, 0.0],
        [-b, -golden_ratio * b, 0.0],
        [golden_ratio * b, 0.0, b],
        [-golden_ratio * b, 0.0, b],
        [golden_ratio * b, 0.0, -b],
        [-golden_ratio * b, 0.0, -b],
    ];
    let f = (golden_ratio * golden_ratio + 1.0 / (golden_ratio * golden_ratio)).sqrt();
    let c = golden_ratio / f;
    let d = 1.0 / golden_ratio / f;
    let mut icosa = oct.clone();
    icosa.extend([
        [0.0, d, c],
        [0.0, d, -c],
        [0.0, -d, c],
        [0.0, -d, -c],
        [c, 0.0, d],
        [c, 0.0, -d],
        [-c, 0.0, d],
        [-c, 0.0, -d],
        [d, c, 0.0],
        [d, -c, 0.0],
        [-d, c, 0.0],
        [-d, -c, 0.0],
    ]);
    let radial_box = |sides: usize| {
        let mut normals = vec![[0.0, 0.0, 1.0], [0.0, 0.0, -1.0]];
        let angle = std::f32::consts::TAU / sides as f32;
        normals.extend((0..sides).map(|side| {
            [
                (angle * side as f32).cos(),
                (angle * side as f32).sin(),
                0.0,
            ]
        }));
        normals
    };
    let normals = match member_path {
        "genFoldBox.Nv_tet" => oct[..4].to_vec(),
        "genFoldBox.Nv_cube" => cube.clone(),
        "genFoldBox.Nv_oct" => oct.clone(),
        "genFoldBox.Nv_dodeca" => dodeca,
        "genFoldBox.Nv_oct_cube" => oct.into_iter().chain(cube).collect(),
        "genFoldBox.Nv_icosa" => icosa,
        "genFoldBox.Nv_box6" => radial_box(6),
        "genFoldBox.Nv_box5" => radial_box(5),
        _ => bail!("unknown generalized-fold normal table {member_path}"),
    };
    ensure!(
        normals.len() == count,
        "generalized-fold table {member_path} has {} normals, expected {count}",
        normals.len()
    );
    Ok(format!(
        "{{ {} }}",
        normals
            .into_iter()
            .map(|normal| format!(
                "float3({}, {}, {})",
                metal_float(normal[0]),
                metal_float(normal[1]),
                metal_float(normal[2])
            ))
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn formula_scalar(values: &BTreeMap<String, String>, name: &str, fallback: f32) -> Result<f32> {
    let Some(value) = values.get(name) else {
        return Ok(fallback);
    };
    let parsed = value
        .replace(',', ".")
        .parse::<f32>()
        .with_context(|| format!("invalid fractal_1.{name} value {value}"))?;
    ensure!(parsed.is_finite(), "fractal_1.{name} must be finite");
    Ok(parsed)
}

fn formula_bool(values: &BTreeMap<String, String>, name: &str, fallback: bool) -> Result<bool> {
    match values.get(name).map(String::as_str) {
        None => Ok(fallback),
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(value) => bail!("invalid fractal_1.{name} boolean {value}"),
    }
}

fn formula_integer(values: &BTreeMap<String, String>, name: &str, fallback: i32) -> Result<i32> {
    values.get(name).map_or(Ok(fallback), |value| {
        value
            .parse::<i32>()
            .with_context(|| format!("invalid fractal_1.{name} integer {value}"))
    })
}

fn formula_integer_or_enum(
    values: &BTreeMap<String, String>,
    name: &str,
    fallback: i32,
    enum_values: &[String],
) -> Result<i32> {
    let Some(value) = values.get(name) else {
        return Ok(fallback);
    };
    if let Ok(value) = value.replace(',', ".").parse::<i32>() {
        return Ok(value);
    }
    enum_values
        .iter()
        .position(|candidate| candidate == value)
        .map(|index| index as i32)
        .ok_or_else(|| anyhow!("invalid {name} integer or enum value {value}"))
}

fn formula_vector4(
    values: &BTreeMap<String, String>,
    name: &str,
    fallback: [f32; 4],
) -> Result<[f32; 4]> {
    let Some(value) = values.get(name) else {
        return Ok(fallback);
    };
    let components = value
        .replace(',', ".")
        .split_whitespace()
        .map(str::parse::<f32>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("invalid fractal_1.{name} vector {value}"))?;
    ensure!(
        components.len() == 4 && components.iter().all(|value| value.is_finite()),
        "fractal_1.{name} must contain four finite components"
    );
    Ok([components[0], components[1], components[2], components[3]])
}

fn formula_vector3(
    values: &BTreeMap<String, String>,
    name: &str,
    fallback: [f32; 3],
) -> Result<[f32; 3]> {
    let Some(value) = values.get(name) else {
        return Ok(fallback);
    };
    let components = value
        .replace(',', ".")
        .split_whitespace()
        .map(str::parse::<f32>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("invalid fractal_1.{name} vector {value}"))?;
    ensure!(
        components.len() == 3 && components.iter().all(|value| value.is_finite()),
        "fractal_1.{name} must contain three finite components"
    );
    Ok([components[0], components[1], components[2]])
}

fn rotation2_matrix(rotation: [f32; 3]) -> [[f32; 3]; 3] {
    let (sx, cx) = rotation[0].sin_cos();
    let (sy, cy) = rotation[1].sin_cos();
    let (sz, cz) = rotation[2].sin_cos();
    [
        [cz * cy, cz * sy * sx - sz * cx, cz * sy * cx + sz * sx],
        [sz * cy, sz * sy * sx + cz * cx, sz * sy * cx - cz * sx],
        [-sy, cy * sx, cy * cx],
    ]
}

fn rotation4_matrix(rotation: [f32; 3]) -> [[f32; 3]; 3] {
    let (sx, cx) = rotation[0].sin_cos();
    let (sy, cy) = rotation[1].sin_cos();
    let (sz, cz) = rotation[2].sin_cos();
    [
        [cy * cz, -cy * sz, sy],
        [cx * sz + sx * sy * cz, cx * cz - sx * sy * sz, -sx * cy],
        [sx * sz - cx * sy * cz, sx * cz + cx * sy * sz, cx * cy],
    ]
}

fn row_matrix_literal(matrix: [[f32; 3]; 3]) -> String {
    format!(
        "{{ float3({}, {}, {}), float3({}, {}, {}), float3({}, {}, {}) }}",
        metal_float(matrix[0][0]),
        metal_float(matrix[0][1]),
        metal_float(matrix[0][2]),
        metal_float(matrix[1][0]),
        metal_float(matrix[1][1]),
        metal_float(matrix[1][2]),
        metal_float(matrix[2][0]),
        metal_float(matrix[2][1]),
        metal_float(matrix[2][2])
    )
}

fn transpose_matrix(matrix: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    [
        [matrix[0][0], matrix[1][0], matrix[2][0]],
        [matrix[0][1], matrix[1][1], matrix[2][1]],
        [matrix[0][2], matrix[1][2], matrix[2][2]],
    ]
}

fn matrix_grid_literal(matrices: [[[[f32; 3]; 3]; 3]; 2]) -> String {
    let folds = matrices.map(|axes| {
        format!(
            "{{ {}, {}, {} }}",
            row_matrix_literal(axes[0]),
            row_matrix_literal(axes[1]),
            row_matrix_literal(axes[2])
        )
    });
    format!("{{ {}, {} }}", folds[0], folds[1])
}

fn check_metal_source(source: &Path, output: &Path) -> Result<()> {
    let result = Command::new("xcrun")
        .args(["-sdk", "macosx", "metal", "-c"])
        .arg(source)
        .arg("-o")
        .arg(output)
        .output()
        .context("launch Metal compiler")?;
    if !result.status.success() {
        bail!(
            "Metal compilation failed for {}:\n{}",
            source.display(),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    Ok(())
}

pub fn lex(source: &str) -> Result<Vec<Token>> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut line = 1;
    let mut line_start = true;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'\n' {
            line += 1;
            line_start = true;
            index += 1;
            continue;
        }
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if line_start && byte == b'#' {
            let start = index;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Preprocessor,
                text: source[start..index].to_owned(),
                offset: start,
                line,
            });
            continue;
        }
        line_start = false;
        if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            let start_line = line;
            index += 2;
            let mut closed = false;
            while index + 1 < bytes.len() {
                if bytes[index] == b'\n' {
                    line += 1;
                }
                if bytes[index] == b'*' && bytes[index + 1] == b'/' {
                    index += 2;
                    closed = true;
                    break;
                }
                index += 1;
            }
            ensure!(
                closed,
                "unterminated block comment beginning at line {start_line}"
            );
            continue;
        }
        if byte == b'"' || byte == b'\'' {
            let start = index;
            let quote = byte;
            index += 1;
            let mut escaped = false;
            let mut closed = false;
            while index < bytes.len() {
                let current = bytes[index];
                if current == b'\n' {
                    line += 1;
                }
                index += 1;
                if escaped {
                    escaped = false;
                } else if current == b'\\' {
                    escaped = true;
                } else if current == quote {
                    closed = true;
                    break;
                }
            }
            ensure!(closed, "unterminated literal beginning at line {line}");
            tokens.push(Token {
                kind: if quote == b'"' {
                    TokenKind::String
                } else {
                    TokenKind::Character
                },
                text: source[start..index].to_owned(),
                offset: start,
                line,
            });
            continue;
        }
        if byte == b'_' || byte.is_ascii_alphabetic() {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index] == b'_' || bytes[index].is_ascii_alphanumeric())
            {
                index += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Identifier,
                text: source[start..index].to_owned(),
                offset: start,
                line,
            });
            continue;
        }
        if byte.is_ascii_digit()
            || (byte == b'.' && bytes.get(index + 1).is_some_and(u8::is_ascii_digit))
        {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric()
                    || matches!(bytes[index], b'.' | b'+' | b'-' | b'_'))
            {
                if matches!(bytes[index], b'+' | b'-')
                    && !matches!(
                        bytes.get(index.wrapping_sub(1)),
                        Some(b'e' | b'E' | b'p' | b'P')
                    )
                {
                    break;
                }
                index += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Number,
                text: source[start..index].to_owned(),
                offset: start,
                line,
            });
            continue;
        }
        let start = index;
        let two = bytes
            .get(index..index + 2)
            .and_then(|value| std::str::from_utf8(value).ok());
        let punctuation = if two.is_some_and(|value| {
            matches!(
                value,
                "->" | "++"
                    | "--"
                    | "+="
                    | "-="
                    | "*="
                    | "/="
                    | "%="
                    | "=="
                    | "!="
                    | "<="
                    | ">="
                    | "&&"
                    | "||"
                    | "<<"
                    | ">>"
                    | "&="
                    | "|="
                    | "^="
                    | "::"
            )
        }) {
            index += 2;
            &source[start..index]
        } else {
            index += 1;
            &source[start..index]
        };
        tokens.push(Token {
            kind: TokenKind::Punctuation,
            text: punctuation.to_owned(),
            offset: start,
            line,
        });
    }
    Ok(tokens)
}

fn validate_balanced(tokens: &[Token]) -> Result<()> {
    let mut stack = Vec::<(&str, usize)>::new();
    for token in tokens {
        match token.text.as_str() {
            "(" | "[" | "{" => stack.push((&token.text, token.line)),
            ")" | "]" | "}" => {
                let (open, line) = stack
                    .pop()
                    .ok_or_else(|| anyhow!("unmatched {} at line {}", token.text, token.line))?;
                let expected = match token.text.as_str() {
                    ")" => "(",
                    "]" => "[",
                    "}" => "{",
                    _ => unreachable!(),
                };
                ensure!(
                    open == expected,
                    "{} at line {line} closed by {} at line {}",
                    open,
                    token.text,
                    token.line
                );
            }
            _ => {}
        }
    }
    ensure!(
        stack.is_empty(),
        "unclosed delimiter at line {}",
        stack.last().map_or(0, |item| item.1)
    );
    Ok(())
}

fn find_iteration_function(_source: &str, tokens: &[Token]) -> Result<(String, usize, usize)> {
    for (index, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::Identifier || !token.text.ends_with("Iteration") {
            continue;
        }
        if tokens.get(index + 1).map(|token| token.text.as_str()) != Some("(") {
            continue;
        }
        let start = tokens
            .get(index.wrapping_sub(1))
            .filter(|token| token.kind == TokenKind::Identifier)
            .map_or(token.offset, |return_type| return_type.offset);
        let body_index = tokens[index + 1..]
            .iter()
            .position(|token| token.text == "{")
            .map(|relative| index + 1 + relative)
            .ok_or_else(|| anyhow!("{} has no function body", token.text))?;
        let mut depth = 0;
        for (closing_index, closing_token) in tokens.iter().enumerate().skip(body_index) {
            match closing_token.text.as_str() {
                "{" => depth += 1,
                "}" => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok((
                            token.text.clone(),
                            start,
                            tokens[closing_index].offset + tokens[closing_index].text.len(),
                        ));
                    }
                }
                _ => {}
            }
        }
    }
    bail!("no FormulaIteration-compatible function found in source")
}

fn collect_member_paths(tokens: &[Token], base: &str) -> Vec<String> {
    let mut result = BTreeSet::new();
    let mut index = 0;
    while index + 2 < tokens.len() {
        if tokens[index].text != base || !matches!(tokens[index + 1].text.as_str(), "->" | ".") {
            index += 1;
            continue;
        }
        let mut path = String::new();
        index += 2;
        while index < tokens.len() {
            if tokens[index].kind != TokenKind::Identifier {
                break;
            }
            if !path.is_empty() {
                path.push('.');
            }
            path.push_str(&tokens[index].text);
            index += 1;
            if tokens.get(index).map(|token| token.text.as_str()) != Some(".") {
                break;
            }
            index += 1;
        }
        if !path.is_empty() {
            result.insert(path);
        }
    }
    result.into_iter().collect()
}

fn classify_constructs(tokens: &[Token]) -> Vec<String> {
    let identifiers = tokens
        .iter()
        .map(|token| token.text.as_str())
        .collect::<BTreeSet<_>>();
    let checks = [
        ("for", "for-loop"),
        ("while", "while-loop"),
        ("switch", "switch"),
        ("Matrix33MulFloat4", "matrix-3x3"),
        ("Matrix44MulFloat4", "matrix-4x4"),
        ("native_sqrt", "native-math"),
    ];
    let mut result = checks
        .into_iter()
        .filter(|(identifier, _)| identifiers.contains(identifier))
        .map(|(_, feature)| feature.to_owned())
        .collect::<Vec<_>>();
    if tokens
        .iter()
        .any(|token| token.kind == TokenKind::Preprocessor)
    {
        result.push("preprocessor".into());
    }
    if tokens.iter().any(|token| token.text == "?") {
        result.push("ternary".into());
    }
    result
}

fn extract_function_body(source: &str) -> Result<&str> {
    let start = source
        .find('{')
        .ok_or_else(|| anyhow!("function body has no opening brace"))?;
    let end = source
        .rfind('}')
        .ok_or_else(|| anyhow!("function body has no closing brace"))?;
    ensure!(end > start, "empty or invalid function body");
    Ok(&source[start + 1..end])
}

fn translate_opencl_body(source: &str, parameter_address: &str) -> String {
    let mut output = String::new();
    for line in source.lines() {
        if line.contains("Q_UNUSED(") {
            continue;
        }
        let line = line
            .replace(
                "const __constant REAL3 *",
                &format!("{parameter_address} const float3 *"),
            )
            .replace(
                "__constant REAL3 *",
                &format!("{parameter_address} const float3 *"),
            )
            .replace(
                "(__constant REAL *)",
                &format!("({parameter_address} const float *)"),
            )
            .replace(
                "__constant REAL *",
                &format!("{parameter_address} const float *"),
            )
            .replace("(REAL *)", "(thread float *)")
            .replace("REAL *", "thread float *")
            .replace("fractal->", "fractal.")
            .replace("aux->", "aux.")
            .replace("native_sqrt", "sqrt")
            .replace("native_sin", "sin")
            .replace("native_cos", "cos")
            .replace("native_log", "log")
            .replace("native_exp", "exp")
            .replace("native_atan2", "atan2")
            .replace("native_powr", "powr")
            .replace("asin(", "mandelSafeAsin(")
            .replace("acos(", "mandelSafeAcos(")
            .replace("fabs", "abs")
            .replace("__constant", "constant")
            .replace("REAL4", "float4")
            .replace("REAL3", "float3")
            .replace("REAL2", "float2")
            .replace("(float4)(", "float4(")
            .replace("(float3)(", "float3(")
            .replace("(float2)(", "float2(")
            .replace("bool functionEnabledN[", "int functionEnabledN[")
            .replace("REAL", "float");
        output.push_str("    ");
        output.push_str(line.trim_start());
        output.push('\n');
    }
    translate_compound_literals(&output)
}

fn emit_formula_dependencies(formula: &ParsedFormula) -> Result<String> {
    let identifiers = formula
        .tokens
        .iter()
        .filter(|token| token.kind == TokenKind::Identifier)
        .map(|token| token.text.as_str())
        .collect::<BTreeSet<_>>();
    let root = mandelbulber_root(formula)?;
    let defines = fs::read_to_string(root.join("opencl/defines_cl.h"))?;
    let mut output = String::new();
    for line in defines.lines() {
        let Some(definition) = line.trim().strip_prefix("#define ") else {
            continue;
        };
        let Some((name, _)) = definition.split_once(char::is_whitespace) else {
            continue;
        };
        if identifiers.contains(name)
            && name != "M_PI_F"
            && name != "Q_UNUSED(x)"
            && !(formula.source.internal_name == "kaleidoscopic_ifs" && name == "IFS_VECTOR_COUNT")
        {
            output.push_str("#define ");
            output.push_str(definition);
            output.push('\n');
        }
    }
    if identifiers.contains("RotateAroundVectorByAngle4") {
        output.push_str(
            r#"static float4 RotateAroundVectorByAngle4(float4 value, float3 axis, float angle) {
    float sine = sin(angle);
    float cosine = cos(angle);
    float3 rotated = value.xyz * cosine
                   + cross(axis, value.xyz) * sine
                   + axis * dot(axis, value.xyz) * (1.0f - cosine);
    return float4(rotated, value.w);
}
"#,
        );
    }
    if identifiers.contains("native_recip") {
        output.push_str("static float native_recip(float value) { return 1.0f / value; }\n");
    }
    if identifiers.contains("native_rsqrt") {
        output.push_str("static float native_rsqrt(float value) { return rsqrt(value); }\n");
    }
    if identifiers.contains("RotateX")
        || identifiers.contains("RotateY")
        || identifiers.contains("RotateZ")
    {
        output.push_str(
            r#"static matrix33 Matrix33MulMatrix33(matrix33 left, matrix33 right) {
    matrix33 result;
    result.m1 = float3(dot(left.m1, float3(right.m1.x, right.m2.x, right.m3.x)),
                       dot(left.m1, float3(right.m1.y, right.m2.y, right.m3.y)),
                       dot(left.m1, float3(right.m1.z, right.m2.z, right.m3.z)));
    result.m2 = float3(dot(left.m2, float3(right.m1.x, right.m2.x, right.m3.x)),
                       dot(left.m2, float3(right.m1.y, right.m2.y, right.m3.y)),
                       dot(left.m2, float3(right.m1.z, right.m2.z, right.m3.z)));
    result.m3 = float3(dot(left.m3, float3(right.m1.x, right.m2.x, right.m3.x)),
                       dot(left.m3, float3(right.m1.y, right.m2.y, right.m3.y)),
                       dot(left.m3, float3(right.m1.z, right.m2.z, right.m3.z)));
    return result;
}

static matrix33 RotateX(matrix33 matrix, float angle) {
    float sine = sin(angle);
    float cosine = cos(angle);
    matrix33 rotation = { float3(1.0f, 0.0f, 0.0f),
                          float3(0.0f, cosine, -sine),
                          float3(0.0f, sine, cosine) };
    return Matrix33MulMatrix33(matrix, rotation);
}

static matrix33 RotateY(matrix33 matrix, float angle) {
    float sine = sin(angle);
    float cosine = cos(angle);
    matrix33 rotation = { float3(cosine, 0.0f, sine),
                          float3(0.0f, 1.0f, 0.0f),
                          float3(-sine, 0.0f, cosine) };
    return Matrix33MulMatrix33(matrix, rotation);
}

static matrix33 RotateZ(matrix33 matrix, float angle) {
    float sine = sin(angle);
    float cosine = cos(angle);
    matrix33 rotation = { float3(cosine, -sine, 0.0f),
                          float3(sine, cosine, 0.0f),
                          float3(0.0f, 0.0f, 1.0f) };
    return Matrix33MulMatrix33(matrix, rotation);
}
"#,
        );
    }
    if identifiers.contains("SmoothConditionAGreaterB") {
        output.push_str(
            r#"static float SmoothConditionAGreaterB(float a, float b, float sharpness) {
    return 1.0f / (1.0f + exp(sharpness * (b - a)));
}
"#,
        );
    }
    if identifiers.contains("SmoothConditionALessB") {
        output.push_str(
            r#"static float SmoothConditionALessB(float a, float b, float sharpness) {
    return 1.0f / (1.0f + exp(sharpness * (a - b)));
}
"#,
        );
    }
    if identifiers.contains("wrap") {
        output.push_str(
            r#"static float3 wrap(float3 value, float3 period, float3 offset) {
    value -= offset;
    return value - period * floor(value / period) + offset;
}
"#,
        );
    }
    Ok(output)
}

fn translate_compound_literals(source: &str) -> String {
    let mut output = source.to_owned();
    for vector_type in ["float2", "float3", "float4"] {
        let marker = format!("({vector_type}){{");
        let mut search_start = 0;
        while let Some(relative) = output[search_start..].find(&marker) {
            let start = search_start + relative;
            let brace = start + marker.len() - 1;
            let Some(end) = matching_delimiter(&output, brace, '{', '}') else {
                break;
            };
            output.replace_range(end..=end, ")");
            output.replace_range(start..=brace, &format!("{vector_type}("));
            search_start = start + vector_type.len() + 1;
        }
    }
    output
}

fn translated_fractal_schema(formula: &ParsedFormula) -> Result<String> {
    let root = mandelbulber_root(formula)?;
    let header = root.join("opencl/fractal_cl.h");
    let output = Command::new("clang")
        .args(["-E", "-P", "-x", "c", "-DOPENCL_KERNEL_CODE", "-I"])
        .arg(root.join("opencl"))
        .arg(&header)
        .output()
        .with_context(|| format!("preprocess {}", header.display()))?;
    ensure!(
        output.status.success(),
        "preprocess {} failed: {}",
        header.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let source = String::from_utf8(output.stdout)?;
    let source = translate_typedef_aggregates(&source)?
        .replace("cl_float4", "float4")
        .replace("cl_float3", "float3")
        .replace("cl_float", "float")
        .replace("cl_int", "int");
    Ok(format!(
        r#"struct matrix33 {{
    float3 m1;
    float3 m2;
    float3 m3;
}};

{source}
typedef sFractalCl MandelFractalParameters;"#
    ))
}

fn mandelbulber_root(formula: &ParsedFormula) -> Result<&Path> {
    formula
        .source
        .path
        .ancestors()
        .find(|ancestor| ancestor.join("opencl/fractal_cl.h").is_file())
        .ok_or_else(|| anyhow!("cannot locate Mandelbulber root from formula path"))
}

fn translate_typedef_aggregates(source: &str) -> Result<String> {
    let mut output = String::new();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find("typedef ") {
        let start = offset + relative;
        output.push_str(&source[offset..start]);
        let remainder = &source[start + "typedef ".len()..];
        let kind = if remainder.starts_with("struct") {
            "struct"
        } else if remainder.starts_with("enum") {
            "enum"
        } else {
            output.push_str("typedef ");
            offset = start + "typedef ".len();
            continue;
        };
        let open = start
            + "typedef ".len()
            + remainder
                .find('{')
                .ok_or_else(|| anyhow!("typedef {kind} has no body"))?;
        let close = matching_delimiter(source, open, '{', '}')
            .ok_or_else(|| anyhow!("typedef {kind} has an unclosed body"))?;
        let after = &source[close + 1..];
        let semicolon = after
            .find(';')
            .ok_or_else(|| anyhow!("typedef {kind} has no terminating semicolon"))?;
        let name = after[..semicolon].trim();
        ensure!(!name.is_empty(), "anonymous typedef {kind} has no alias");
        output.push_str(kind);
        output.push(' ');
        output.push_str(name);
        output.push(' ');
        output.push_str(&source[open..=close]);
        output.push(';');
        offset = close + 1 + semicolon + 1;
    }
    output.push_str(&source[offset..]);
    Ok(output)
}

fn matching_delimiter(source: &str, open: usize, opening: char, closing: char) -> Option<usize> {
    let mut depth = 0usize;
    for (relative, character) in source[open..].char_indices() {
        if character == opening {
            depth += 1;
        } else if character == closing {
            depth -= 1;
            if depth == 0 {
                return Some(open + relative);
            }
        }
    }
    None
}

const BULB_PARAMETER_TYPES: &str = r#"
struct matrix33 {
    float3 m1;
    float3 m2;
    float3 m3;
};

struct MandelBulbParameters {
    float alphaAngleOffset;
    float betaAngleOffset;
    float gammaAngleOffset;
    float power;
};

struct MandelFractalParameters {
    MandelBulbParameters bulb;
};"#;

const ANALYTIC_DE_PARAMETER_TYPES: &str = r#"
struct matrix33 {
    float3 m1;
    float3 m2;
    float3 m3;
};

struct MandelAnalyticDeParameters {
    int enabled;
    int enabledFalse;
    float scale1;
    float tweak005;
    float offset0;
    float offset1;
    float offset2;
    int startIterationsA;
    int stopIterationsA;
};

struct MandelFractalParameters {
    MandelAnalyticDeParameters analyticDE;
};"#;

const TRANSFORM_BASIC_PARAMETER_TYPES: &str = r#"
struct matrix33 {
    float3 m1;
    float3 m2;
    float3 m3;
};

struct MandelTransformBasicParameters {
    float4 additionConstant0000;
    float scale3;
    float pwr8;
    float pwr8a;
};

struct MandelFractalParameters {
    MandelTransformBasicParameters transformCommon;
};"#;

const JOS_KLEINIAN_PARAMETER_TYPES: &str = r#"
struct matrix33 {
    float3 m1;
    float3 m2;
    float3 m3;
};

struct MandelJosAnalyticDeParameters {
    float scale1;
    float tweak005;
    float offset1;
};

struct MandelJosFoldColorParameters {
    int auxColorEnabled;
    int auxColorEnabledAFalse;
    int auxColorEnabledFalse;
    float4 difs0000;
    float difs1;
    int startIterationsA;
    int stopIterationsA;
};

struct MandelJosTransformParameters {
    float4 additionConstant000;
    float4 additionConstantP000;
    float4 constantMultiplierC111;
    float4 offset000;
    float4 offset111;
    float4 scale3D222;
    float foldingValue;
    float maxR2d1;
    float offset;
    int functionEnabledAFalse;
    int sphereInversionEnabledFalse;
    int spheresEnabled;
    int startIterationsC;
    int startIterationsT;
    int stopIterationsC;
    int stopIterationsT;
};

struct MandelFractalParameters {
    MandelJosAnalyticDeParameters analyticDE;
    MandelJosFoldColorParameters foldColor;
    MandelJosTransformParameters transformCommon;
};"#;

const PSEUDO_KLEINIAN_PARAMETER_TYPES: &str = r#"
struct matrix33 {
    float3 m1;
    float3 m2;
    float3 m3;
};

struct MandelPseudoAnalyticDeParameters {
    float offset0;
    float scale1;
    float tweak005;
};

struct MandelPseudoFoldColorParameters {
    int auxColorEnabledAFalse;
    int auxColorEnabledFalse;
    float4 difs0000;
    float difs1;
    int startIterationsA;
    int stopIterationsA;
};

struct MandelPseudoColorParameters {
    float3 factor;
};

struct MandelPseudoMandelboxParameters {
    MandelPseudoColorParameters color;
    float foldingLimit;
    float foldingValue;
};

struct MandelPseudoTransformParameters {
    float4 additionConstant000;
    float4 additionConstant0777;
    float4 additionConstant111;
    float4 additionConstantA000;
    float4 additionConstantP000;
    float4 offset000;
    float4 constantMultiplier000;
    float4 constantMultiplier111;
    float4 constantMultiplierC111;
    matrix33 rotationMatrix;
    float maxR2d1;
    float minR05;
    float scale;
    float scale1;
    int functionEnabledAFalse;
    int functionEnabledAxFalse;
    int functionEnabledBxFalse;
    int functionEnabledBy;
    int functionEnabledByFalse;
    int functionEnabledNFalse;
    int functionEnabledPFalse;
    int functionEnabledRFalse;
    int functionEnabledwFalse;
    int sphereInversionEnabledFalse;
    int startIterationsA;
    int startIterationsC;
    int startIterationsE;
    int startIterationsP;
    int startIterationsR;
    int startIterationsT;
    int startIterationsX;
    int stopIterations1;
    int stopIterationsA;
    int stopIterationsC;
    int stopIterationsE;
    int stopIterationsP1;
    int stopIterationsR;
    int stopIterationsT;
};

struct MandelFractalParameters {
    MandelPseudoAnalyticDeParameters analyticDE;
    MandelPseudoFoldColorParameters foldColor;
    MandelPseudoMandelboxParameters mandelbox;
    MandelPseudoTransformParameters transformCommon;
};"#;

const MANDELBOX_FAST_PARAMETER_TYPES: &str = r#"
struct matrix33 {
    float3 m1;
    float3 m2;
    float3 m3;
};

struct MandelboxFastParameters {
    float scale;
    float fR2;
    float mR2;
    float mboxFactor1;
    int mainRotationEnabled;
    matrix33 mainRot;
};

struct MandelFractalParameters {
    MandelboxFastParameters mandelbox;
};"#;

const MANDELBOX_FULL_PARAMETER_TYPES: &str = r#"
struct matrix33 {
    float3 m1;
    float3 m2;
    float3 m3;
};

struct MandelboxColorParameters {
    float3 factor;
    float factorSp1;
    float factorSp2;
};

struct MandelboxFullParameters {
    float scale;
    float foldingLimit;
    float foldingValue;
    float fR2;
    float mR2;
    float mboxFactor1;
    float4 offset;
    int rotationsEnabled;
    int mainRotationEnabled;
    matrix33 mainRot;
    matrix33 rot[2][3];
    matrix33 rotinv[2][3];
    MandelboxColorParameters color;
};

struct MandelFractalParameters {
    MandelboxFullParameters mandelbox;
};"#;

const IFS_PARAMETER_TYPES: &str = r#"
constant int IFS_VECTOR_COUNT = 9;

struct matrix33 {
    float3 m1;
    float3 m2;
    float3 m3;
};

struct MandelIfsParameters {
    int absX;
    int absY;
    int absZ;
    int enabled[9];
    matrix33 rot[9];
    float4 direction[9];
    float distance[9];
    float intensity[9];
    int rotationEnabled;
    matrix33 mainRot;
    float4 offset;
    int edgeEnabled;
    float3 edge;
    float scale;
    int mengerSpongeMode;
};

struct MandelFractalParameters {
    MandelIfsParameters IFS;
};"#;

#[cfg(test)]
mod tests {
    use super::*;

    const COLOR_SCENE: &str = r#"
# Mandelbulber settings file
# version 2.33
[main_parameters]
camera 0 0 -4;
formula_1 10;
mat1_coloring_palette_offset 0,25;
mat1_coloring_speed 3;
mat1_fractal_coloring_extra_color_enabled_false true;
mat1_surface_color_gradient 0 ff0000 5000 00ff00;
target 0 0 0;
[fractal_1]
"#;

    fn generated_binding_fixture(parameter_reads: &[&str]) -> ParsedFormula {
        ParsedFormula {
            source: ResolvedFormulaSource {
                symbol: "transfDIFSGrid".to_owned(),
                id: 1613,
                internal_name: "transf_difs_grid".to_owned(),
                path: std::path::PathBuf::from("transf_difs_grid.cl"),
                de_type: "analyticDEType".to_owned(),
                de_function_type: "customDEFunction".to_owned(),
                pixel_addition: "cpixelDisabledByDefault".to_owned(),
                default_bailout: 100.0,
                analytic_function: "analyticFunctionCustomDE".to_owned(),
                coloring_function: "coloringFunctionDefault".to_owned(),
            },
            function_name: "TransfDIFSGridIteration".to_owned(),
            function_source: String::new(),
            tokens: Vec::new(),
            parameter_reads: parameter_reads
                .iter()
                .map(|path| (*path).to_owned())
                .collect(),
            auxiliary_fields: Vec::new(),
            constructs: Vec::new(),
        }
    }

    #[test]
    fn kleinian_distance_functions_enable_the_upstream_convergence_bailout() {
        let mut formula = generated_binding_fixture(&[]);
        assert!(!uses_additional_bailout(&formula));

        formula.source.de_function_type = "josKleinianDEFunction".to_owned();
        assert!(uses_additional_bailout(&formula));

        formula.source.de_function_type = "pseudoKleinianDEFunction".to_owned();
        assert!(uses_additional_bailout(&formula));
    }

    #[test]
    fn generated_appearance_embeds_full_palette_and_coloring_controls() {
        let scene = MandelbulberScene::parse(COLOR_SCENE).expect("color scene");
        let constants = mandelbulber_coloring_constants(&scene).expect("color constants");
        let palette = mandelbulber_palette_source(&scene);
        assert!(constants.contains("constant bool kMandelColorExtra = true;"));
        assert!(palette.contains("kMandelSurfaceGradientCount = 3u"));
        assert!(palette.contains("* 3.0f + 0.25f"));
        assert!(palette.contains("float4(1.0f, 0.99609375f, 0.0f, 0.0f)"));
    }

    #[test]
    fn lexer_balances_opencl_formula_syntax() {
        let source =
            "REAL4 TestIteration(REAL4 z, sExtendedAuxCl *aux) { aux->DE *= 2.0f; return z; }";
        let tokens = lex(source).expect("lex formula");
        validate_balanced(&tokens).expect("balanced formula");
        let (name, _, _) = find_iteration_function(source, &tokens).expect("iteration function");
        assert_eq!(name, "TestIteration");
        assert_eq!(collect_member_paths(&tokens, "aux"), ["DE"]);
    }

    #[test]
    fn diagnostic_source_pruning_retains_only_requested_kernel_entry_points() {
        let source = r#"
static float helper(float value) { return value; }
kernel void discarded(uint gid [[thread_position_in_grid]]) {
    // A comment brace must not terminate the kernel: }
    if (gid > 0) { int nested = int(gid); }
}
kernel void retained(uint gid [[thread_position_in_grid]]) {
    const char brace = '}';
    if (gid > 0) { int nested = int(gid); }
}
kernel void also_discarded(uint gid [[thread_position_in_grid]]) {
    /* Nor should a block-comment brace: } */
}
"#;
        let pruned = retain_metal_kernels(source, &["retained"]).unwrap();
        assert!(pruned.contains("static float helper"));
        assert!(pruned.contains("kernel void retained"));
        assert!(!pruned.contains("kernel void discarded"));
        assert!(!pruned.contains("kernel void also_discarded"));
    }

    #[test]
    fn translation_normalizes_opencl_pointer_and_math_syntax() {
        let translated = translate_opencl_body(
            "aux->DE = native_sqrt(fractal->IFS.scale) + asin(z.x) + acos(z.y);",
            "constant",
        );
        assert!(translated.contains("aux.DE = sqrt(fractal.IFS.scale)"));
        assert!(translated.contains("mandelSafeAsin(z.x)"));
        assert!(translated.contains("mandelSafeAcos(z.y)"));
    }

    #[test]
    fn translation_assigns_metal_pointer_address_spaces() {
        let translated = translate_opencl_body(
            "REAL *local = (REAL *)&z; __constant REAL *value = (__constant REAL *)&fractal->x;",
            "constant",
        );
        assert!(translated.contains("thread float *local = (thread float *)&z"));
        assert!(
            translated.contains("constant const float *value = (constant const float *)&fractal.x")
        );
    }

    #[test]
    fn generated_bindings_emit_only_proven_fields_and_cover_vector_components() {
        let formula = generated_binding_fixture(&[
            "foldColor.difs0000.x",
            "transformCommon.rotationMatrix",
            "transformCommon.scale1",
        ]);
        let types = generated_runtime_parameter_types(&formula).expect("generated types");
        assert!(types.contains("float4 difs0000;"));
        assert!(types.contains("matrix33 rotationMatrix;"));
        assert!(types.contains("float scale1;"));
        assert!(!types.contains("scaleF1"));
        assert!(!types.contains("MandelGeneratedAnalyticDeParameters"));

        let values = BTreeMap::from([
            ("fold_color_difs_0000".to_owned(), "1 2 3 4".to_owned()),
            ("transf_rotation".to_owned(), "0 0 90".to_owned()),
            ("transf_scale_1".to_owned(), "1,25".to_owned()),
        ]);
        let initializer =
            generated_runtime_constant_initializer(&formula, &values).expect("initializer");
        assert!(initializer.contains("float4(1.0f, 2.0f, 3.0f, 4.0f)"));
        assert!(initializer.contains(&metal_float(1.25)));

        let unsupported = generated_binding_fixture(&["transformCommon.unknownField"]);
        assert!(generated_runtime_bindings(&unsupported).is_none());
    }

    #[test]
    fn generated_bindings_derive_radians_and_trigonometry_from_one_scene_angle() {
        let mut formula = generated_binding_fixture(&[
            "transformCommon.angleDegA",
            "transformCommon.cosA",
            "transformCommon.sinA",
        ]);
        formula.source.internal_name = "transf_difs_torus_v4".to_owned();
        let values = BTreeMap::from([("transf_angle_deg_A".to_owned(), "90".to_owned())]);
        let initializer =
            generated_runtime_constant_initializer(&formula, &values).expect("initializer");
        assert!(initializer.contains(&metal_float(90.0_f32.to_radians())));
        assert!(initializer.contains(&metal_float(90.0_f32.to_radians().cos())));
        assert!(initializer.contains(&metal_float(1.0)));
    }

    #[test]
    fn bulb_initializer_applies_scene_values_and_degree_conversion() {
        let values = BTreeMap::from([
            ("power".to_owned(), "8".to_owned()),
            ("alpha_angle_offset".to_owned(), "7,5".to_owned()),
            ("beta_angle_offset".to_owned(), "-3.25".to_owned()),
        ]);
        let initializer = bulb_constant_initializer(&values).expect("bulb initializer");
        assert!(initializer.contains(&metal_float(8.0)));
        assert!(initializer.contains(&metal_float(7.5_f32.to_radians())));
        assert!(initializer.contains(&metal_float((-3.25_f32).to_radians())));
    }

    #[test]
    fn shared_parameter_initializers_apply_defaults_and_overrides() {
        let analytic_values = BTreeMap::from([
            ("analyticDE_scale_1".to_owned(), "1,25".to_owned()),
            ("analyticDE_enabled".to_owned(), "false".to_owned()),
        ]);
        let analytic =
            analytic_de_constant_initializer(&analytic_values).expect("analytic DE initializer");
        assert!(analytic.contains(&metal_float(1.25)));
        assert!(analytic.contains("{ { 0, 0,"));

        let transform_values = BTreeMap::from([(
            "transf_addition_constant_0000".to_owned(),
            "1 -2,5 3 4".to_owned(),
        )]);
        let transform =
            transform_basic_constant_initializer(&transform_values).expect("transform initializer");
        assert!(transform.contains("float4(1.0f, -2.5f, 3.0f, 4.0f)"));
        assert!(transform.contains(&metal_float(8.0)));

        let inv1 = GENERATED_RUNTIME_PARAMETER_BINDINGS
            .iter()
            .find(|binding| binding.member_path == "transformCommon.inv1")
            .expect("inverse upper spherical-fold radius binding");
        assert!(matches!(
            inv1.value,
            RuntimeBindingValue::Reciprocal(value) if value == 1.0
        ));
    }

    #[test]
    fn mandelbox_initializer_derives_radius_factors_and_rotation() {
        let values = BTreeMap::from([
            (
                "mandelbox_folding_fixed_radius".to_owned(),
                "1,5".to_owned(),
            ),
            ("mandelbox_folding_min_radius".to_owned(), "0,5".to_owned()),
            (
                "mandelbox_main_rotation_enabled".to_owned(),
                "true".to_owned(),
            ),
            ("mandelbox_rotation_main".to_owned(), "90 0 0".to_owned()),
        ]);
        let initializer =
            mandelbox_fast_constant_initializer(&values).expect("Mandelbox initializer");
        assert!(initializer.contains(&metal_float(2.25)));
        assert!(initializer.contains(&metal_float(0.25)));
        assert!(initializer.contains(&metal_float(9.0)));
        assert!(initializer.contains(", 1, { float3("));
    }

    #[test]
    fn ifs_initializer_packs_complete_per_slot_state() {
        let values = BTreeMap::from([
            ("IFS_abs_x".to_owned(), "true".to_owned()),
            ("IFS_enabled_2".to_owned(), "true".to_owned()),
            ("IFS_direction_2".to_owned(), "0 3 4".to_owned()),
            ("IFS_rotations_2".to_owned(), "10 -20 30".to_owned()),
            ("IFS_distance_2".to_owned(), "0,125".to_owned()),
            ("IFS_intensity_2".to_owned(), "0,75".to_owned()),
            ("IFS_rotation_enabled".to_owned(), "true".to_owned()),
            ("IFS_edge".to_owned(), "1 2 3".to_owned()),
            ("IFS_edge_enabled".to_owned(), "true".to_owned()),
            ("IFS_menger_sponge_mode".to_owned(), "true".to_owned()),
            ("IFS_scale".to_owned(), "1,7".to_owned()),
        ]);
        let initializer = ifs_constant_initializer(&values).expect("IFS initializer");
        assert!(initializer.contains("1, 0, 0,"));
        assert!(initializer.contains("0, 0, 1, 0, 0, 0, 0, 0, 0"));
        assert!(IFS_PARAMETER_TYPES.contains("matrix33 rot[9]"));
        assert!(!IFS_PARAMETER_TYPES.contains("float3x3 rot[9]"));
        assert!(initializer.contains(&format!(
            "{{ 0.0f, {}, {}, 0.0f }}",
            metal_float(3.0 / 5.0),
            metal_float(4.0 / 5.0)
        )));
        assert!(initializer.contains(&metal_float(0.125)));
        assert!(initializer.contains(&metal_float(0.75)));
        assert!(initializer.contains("{ 1.0f, 2.0f, 3.0f }"));
        assert!(initializer.contains(&metal_float(1.7)));
        assert!(initializer.ends_with("    }\n};"));
    }

    #[test]
    fn hybrid_finalizer_selection_matches_mandelbulber_precedence() {
        assert_eq!(select_hybrid_de_function([4, 4, 0, 0, 0, 0]), 0);
        assert_eq!(select_hybrid_de_function([2, 5, 3, 0, 0, 0]), 1);
        assert_eq!(select_hybrid_de_function([100, 0, 0, 0, 1, 0]), 4);
        assert_eq!(select_hybrid_de_function([0, 0, 0, 0, 0, 7]), 5);
        assert_eq!(
            overridden_analytic_function("analyticFunctionLinear", 2).unwrap(),
            "analyticFunctionLogarithmic"
        );
        assert_eq!(
            overridden_de_function("linearDEFunction", 6).unwrap(),
            "maxAxisDEFunction"
        );
        assert_eq!(
            clamped_distance_expression("aux.dist"),
            "clamp(float(aux.dist), 0.0f, 10.0f)"
        );
    }

    #[test]
    fn homogeneous_hybrid_schedule_lowers_to_one_ordered_loop() {
        let formula = generated_binding_fixture(&[]);
        let values = BTreeMap::new();
        let slot = RuntimeFormulaSlot {
            index: 0,
            formula: &formula,
            formula_values: &values,
            iterations: 8,
            weight: 1.0,
            add_c_constant: false,
            check_for_bailout: true,
            bailout: 100.0,
        };
        let source = hybrid_direct_loop_source(&[slot], &[0, 0, 0], &[0.0; 40])
            .unwrap()
            .expect("homogeneous schedule");
        assert!(source.contains("for (int iteration = 0; iteration < max_iterations"));
        assert!(source.contains("MandelSlot0::TransfDIFSGridIteration"));
        assert!(!source.contains("switch"));
        assert!(!source.contains("kMandelHybridSequence"));
    }

    #[test]
    fn mixed_hybrid_schedule_keeps_dynamic_dispatch() {
        let first = generated_binding_fixture(&[]);
        let second = generated_binding_fixture(&[]);
        let values = BTreeMap::new();
        let slots = [
            RuntimeFormulaSlot {
                index: 0,
                formula: &first,
                formula_values: &values,
                iterations: 1,
                weight: 1.0,
                add_c_constant: false,
                check_for_bailout: false,
                bailout: 100.0,
            },
            RuntimeFormulaSlot {
                index: 1,
                formula: &second,
                formula_values: &values,
                iterations: 1,
                weight: 1.0,
                add_c_constant: false,
                check_for_bailout: false,
                bailout: 100.0,
            },
        ];
        assert!(
            hybrid_direct_loop_source(&slots, &[0, 1], &[0.0; 40])
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn periodic_hybrid_sequences_replace_the_table_lookup() {
        assert_eq!(
            periodic_hybrid_sequence_expression(&[0, 1, 0, 1, 0]),
            Some("int sequence_index = iteration & 1;".to_owned())
        );
        assert_eq!(
            periodic_hybrid_sequence_expression(&[0, 1, 2, 0, 1, 2, 0]),
            Some("int sequence_index = iteration % 3;".to_owned())
        );
        assert_eq!(periodic_hybrid_sequence_expression(&[0, 1, 1, 0]), None);
    }

    #[test]
    fn periodic_hybrid_schedule_can_lower_to_ordered_phases() {
        let first = generated_binding_fixture(&[]);
        let second = generated_binding_fixture(&[]);
        let values = BTreeMap::new();
        let slots = [
            RuntimeFormulaSlot {
                index: 0,
                formula: &first,
                formula_values: &values,
                iterations: 1,
                weight: 1.0,
                add_c_constant: false,
                check_for_bailout: false,
                bailout: 100.0,
            },
            RuntimeFormulaSlot {
                index: 1,
                formula: &second,
                formula_values: &values,
                iterations: 1,
                weight: 1.0,
                add_c_constant: false,
                check_for_bailout: false,
                bailout: 100.0,
            },
        ];
        let source = hybrid_unrolled_periodic_loop_source(&slots, &[0, 1, 0, 1], &[0.0; 40])
            .unwrap()
            .expect("periodic schedule");
        assert!(source.contains("iteration += 2"));
        assert!(source.contains("MandelSlot0::TransfDIFSGridIteration"));
        assert!(source.contains("MandelSlot1::TransfDIFSGridIteration"));
        assert!(!source.contains("switch"));
        assert!(
            hybrid_unrolled_periodic_loop_source(&slots, &[0, 1, 1, 0], &[0.0; 40])
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn slow_scene_formula_policies_are_content_selected() {
        let torus = formula_optimization_policy_for_digest(
            "8bbd267428bb7fe674b4f84f549ff9af495850b27bcf12b118d7a27f187989d8",
        );
        assert!(torus.periodic_hybrid_loop);
        assert!(torus.unrolled_periodic_hybrid_loop);
        assert_eq!(torus.partial_evaluation_formula_id, 132);
        assert!(torus.partial_evaluation_phases);
        assert!(torus.partial_evaluation_scalarize_loops);
        assert!(torus.partial_evaluation_dce);
        assert!(torus.partial_evaluation_cse);

        let difs_box = formula_optimization_policy_for_digest(
            "dc67ed9066cac84403ed28cea04458bf332bdc4e5df5ef118bad9ebd541c49cd",
        );
        assert_eq!(difs_box.partial_evaluation_formula_id, 602);
        assert!(difs_box.partial_evaluation_phases);

        let ifs_xy = formula_optimization_policy_for_digest(
            "c1f596cb79c5566ef078fb5b719e919e85dd597e085c612d0339640d237e2bd7",
        );
        assert_eq!(ifs_xy.partial_evaluation_formula_id, 150);
        assert!(ifs_xy.partial_evaluation_phases);

        let hybrid = formula_optimization_policy_for_digest(
            "b9236c85374cc00782cc8a2e418a3a5aa7af67c82d15ea0c8924152b36638cfb",
        );
        assert!(hybrid.periodic_hybrid_loop);
        assert!(hybrid.unrolled_periodic_hybrid_loop);
        assert_eq!(hybrid.partial_evaluation_formula_id, 0);

        let pseudo = formula_optimization_policy_for_digest(
            "243f3b55d101588b42437330a9ef1f7adb661af69fc3b22c59710abdc8167244",
        );
        assert!(!pseudo.periodic_hybrid_loop);
        assert!(!pseudo.unrolled_periodic_hybrid_loop);
        assert_eq!(pseudo.partial_evaluation_formula_id, 0);

        assert_eq!(
            formula_optimization_policy_for_digest("unknown"),
            SceneFormulaOptimizationPolicy::default()
        );
    }

    #[test]
    fn standalone_c_addition_swaps_aboxmod1_and_amazing_surf_axes() {
        let mut values = [0.0; 40];
        values[4] = 1.0;
        values[5] = 1.0;
        values[6..9].copy_from_slice(&[2.0, 3.0, 5.0]);
        values[9..12].copy_from_slice(&[7.0, 11.0, 13.0]);
        let ordinary = constant_addition_source(2, &values);
        let swapped = constant_addition_source(64, &values);
        assert!(ordinary.contains("2.0f * 7.0f, 3.0f * 11.0f"));
        assert!(swapped.contains("3.0f * 11.0f, 2.0f * 7.0f"));
        assert_eq!(swapped, constant_addition_source(73, &values));
    }

    #[test]
    fn boolean_formula_scale_is_an_object_size() {
        assert!((boolean_point_scale(2.0) - 0.5).abs() < f32::EPSILON);
        assert!((boolean_point_scale(0.0625) - 16.0).abs() < f32::EPSILON);
    }

    #[test]
    fn full_mandelbox_initializer_packs_fold_matrices_and_scene_values() {
        let values = BTreeMap::from([
            ("mandelbox_folding_limit".to_owned(), "0,9".to_owned()),
            ("mandelbox_folding_value".to_owned(), "1,8".to_owned()),
            ("mandelbox_offset".to_owned(), "0,1 -0,2 0,3".to_owned()),
            ("mandelbox_color".to_owned(), "0,4 0,5 0,6".to_owned()),
            ("mandelbox_rotations_enabled".to_owned(), "true".to_owned()),
            ("mandelbox_rotation_neg_1".to_owned(), "90 0 0".to_owned()),
        ]);
        let initializer =
            mandelbox_full_constant_initializer(&values).expect("full Mandelbox initializer");
        assert!(initializer.contains(&format!("{}, {}", metal_float(0.9), metal_float(1.8))));
        assert!(initializer.contains(&format!(
            "float4({}, {}, {}, 0.0f)",
            metal_float(0.1),
            metal_float(-0.2),
            metal_float(0.3)
        )));
        assert!(initializer.contains(&format!(
            "float3({}, {}, {})",
            metal_float(0.4),
            metal_float(0.5),
            metal_float(0.6)
        )));
        assert!(initializer.contains("{ { { float3("));
    }
}
