//! Mandelbulber's Perlin-noise surface displacement (`PerlinNoiseDisplacement`,
//! `shader_perlin_noise_for_shaders.cpp`): a material with `perlin_noise_displacement_enable` lowers its objects'
//! distance by octave noise. `emit` writes it as WGSL.

use std::fmt::Write as _;

use crate::params::rotation_matrix;
use crate::scene::FractFile;

/// `cPerlinNoiseOctaves::reseed` as libc++ (macOS, where the references were rendered) runs it:
/// `std::shuffle` of 0..256 with `std::default_random_engine(seed)` (`minstd_rand`) and libc++'s
/// `uniform_int_distribution`. libstdc++ shuffles differently.
#[must_use]
pub fn permutation(seed: u32) -> [u8; 256] {
    const MODULUS: u64 = 2_147_483_647;
    let mut state = u64::from(seed) % MODULUS;
    if state == 0 {
        state = 1;
    }
    let mut next = || {
        state = state * 48_271 % MODULUS;
        state
    };
    // The engine's range is max − min + 1 = MODULUS − 1; a draw of `bits` (at most 8, so one engine call) keeps the
    // low bits of draws below the largest multiple of 2^bits in that range (__independent_bits_engine).
    let engine_range = MODULUS - 1;
    let mut uniform = |range: u64| -> u64 {
        if range == 1 {
            return 0;
        }
        let mut bits = 63 - u64::from(range.leading_zeros());
        if range & (u64::MAX >> (64 - bits)) != 0 {
            bits += 1;
        }
        let limit = (engine_range >> bits) << bits;
        let mask = (1u64 << bits) - 1;
        loop {
            let draw = loop {
                let u = next() - 1;
                if u < limit {
                    break u & mask;
                }
            };
            if draw < range {
                return draw;
            }
        }
    };
    #[allow(clippy::cast_possible_truncation, reason = "0..256")]
    let mut p: [u8; 256] = std::array::from_fn(|i| i as u8);
    for first in 0..255_usize {
        let last = 255 - first;
        #[allow(clippy::cast_possible_truncation, reason = "at most 255")]
        let i = uniform(last as u64 + 1) as usize;
        if i != 0 {
            p.swap(first, first + i);
        }
    }
    p
}

/// One material's displacement settings (Mandelbulber's defaults filled in).
#[derive(Clone, Debug, PartialEq)]
pub struct Perlin {
    pub material: i64,
    /// `SetRotation2` rows applied to the point.
    pub rotation: [[f64; 3]; 3],
    pub period: [f64; 3],
    pub offset: [f64; 3],
    pub octaves: i64,
    pub abs: bool,
    pub value_offset: f64,
    pub invert: bool,
    pub intensity: f64,
}

/// The materials whose Perlin noise displaces their surface.
#[must_use]
pub fn materials(file: &FractFile) -> Vec<Perlin> {
    let Some(main) = file.sections.get("main_parameters") else {
        return Vec::new();
    };
    let mut ids: Vec<i64> = main
        .keys()
        .filter_map(|key| key.strip_prefix("mat")?.split_once('_')?.0.parse().ok())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids.into_iter()
        .filter_map(|id| {
            let key = |name: &str| format!("mat{id}_perlin_noise_{name}");
            (file.boolean(&key("enable"), false)
                && file.boolean(&key("displacement_enable"), false))
            .then(|| Perlin {
                material: id,
                rotation: rotation_matrix(2, file.vector(&key("rotation"), [0.0; 3])),
                period: file.vector(&key("period"), [1.0; 3]),
                offset: file.vector(&key("position_offset"), [0.0; 3]),
                #[allow(clippy::cast_possible_truncation, reason = "an octave count")]
                octaves: file.number(&key("iterations"), 5.0) as i64,
                abs: file.boolean(&key("abs"), false),
                value_offset: file.number(&key("value_offset"), 0.0),
                invert: file.boolean(&key("displacement_invert"), false),
                intensity: file.number(&key("displacement_intensity"), 1.0),
            })
        })
        .collect()
}

#[allow(clippy::cast_possible_truncation, reason = "GPU parameters are f32")]
fn slot(v: [f64; 3], w: f64) -> [f32; 4] {
    [v[0] as f32, v[1] as f32, v[2] as f32, w as f32]
}

/// WGSL for `fractal_displacement(material, p, distance)` (`p` in Mandelbulber's coordinates, the distance lowered
/// by that material's noise; other materials return it unchanged) and `fractal_displace(p, distance)`, the fractal's
/// own (`formula_material_id`) with `p` in the estimate's coordinates (Mandelbulber's, less `origin`).
#[must_use]
pub fn emit(file: &FractFile, origin: [f64; 3], params: &mut Vec<[f32; 4]>) -> String {
    let all = materials(file);
    let mut out = String::new();
    let origin_slot = params.len();
    params.push(slot(origin, 0.0));
    #[allow(clippy::cast_possible_truncation, reason = "a material id")]
    let fractal_material = file.number("formula_material_id", 1.0) as i64;
    let _ = writeln!(
        out,
        "fn fractal_displace(p: vec3<f32>, distance: f32) -> f32 {{\n    return fractal_displacement({fractal_material}, p + fractal_params[{origin_slot}].xyz, distance);\n}}"
    );
    if all.is_empty() {
        out.push_str("fn fractal_displacement(material: i32, p: vec3<f32>, distance: f32) -> f32 {\n    return distance;\n}\n");
        return out;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a seed"
    )]
    let table = permutation(file.number("clouds_random_seed", 12345.0) as u32);
    let first = params.len();
    for chunk in table.chunks(4) {
        params.push([
            f32::from(chunk[0]),
            f32::from(chunk[1]),
            f32::from(chunk[2]),
            f32::from(chunk[3]),
        ]);
    }
    let _ = write!(
        out,
        "fn fractal_perlin_hash(i: i32) -> i32 {{
    let k = u32(i) & 255u;
    return i32(fractal_params[{first}u + k / 4u][k % 4u]);
}}

fn fractal_perlin_gradient(hash: i32, x: f32, y: f32, z: f32) -> f32 {{
    let h = hash & 15;
    let u = select(y, x, h < 8);
    let v = select(select(z, x, h == 12 || h == 14), y, h < 4);
    return select(-u, u, (h & 1) == 0) + select(-v, v, (h & 2) == 0);
}}

// cPerlinNoiseOctaves::noise3D.
fn fractal_perlin_noise(p: vec3<f32>) -> f32 {{
    let cell = floor(p);
    let x = i32(cell.x) & 255;
    let y = i32(cell.y) & 255;
    let z = i32(cell.z) & 255;
    let f = p - cell;
    let fade = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let a = fractal_perlin_hash(x) + y;
    let aa = fractal_perlin_hash(a) + z;
    let ab = fractal_perlin_hash(a + 1) + z;
    let b = fractal_perlin_hash(x + 1) + y;
    let ba = fractal_perlin_hash(b) + z;
    let bb = fractal_perlin_hash(b + 1) + z;
    return mix(
        mix(
            mix(fractal_perlin_gradient(fractal_perlin_hash(aa), f.x, f.y, f.z), fractal_perlin_gradient(fractal_perlin_hash(ba), f.x - 1.0, f.y, f.z), fade.x),
            mix(fractal_perlin_gradient(fractal_perlin_hash(ab), f.x, f.y - 1.0, f.z), fractal_perlin_gradient(fractal_perlin_hash(bb), f.x - 1.0, f.y - 1.0, f.z), fade.x),
            fade.y),
        mix(
            mix(fractal_perlin_gradient(fractal_perlin_hash(aa + 1), f.x, f.y, f.z - 1.0), fractal_perlin_gradient(fractal_perlin_hash(ba + 1), f.x - 1.0, f.y, f.z - 1.0), fade.x),
            mix(fractal_perlin_gradient(fractal_perlin_hash(ab + 1), f.x, f.y - 1.0, f.z - 1.0), fractal_perlin_gradient(fractal_perlin_hash(bb + 1), f.x - 1.0, f.y - 1.0, f.z - 1.0), fade.x),
            fade.y),
        fade.z);
}}

// normalizedOctaveNoise3D_0_1: octaves shifted along `shift`, normalised to 0..1.
fn fractal_perlin_octaves(p: vec3<f32>, shift: vec3<f32>, octaves: i32) -> f32 {{
    var v = p;
    var result = 0.0;
    var amplitude = 1.0;
    var weight = 0.0;
    for (var i = 0; i < octaves; i++) {{
        let s = shift * ((f32(octaves - i) / f32(octaves) + 1.13) * 0.597);
        v -= s * 0.2;
        result += fractal_perlin_noise(v - s) * amplitude;
        weight += amplitude;
        v *= 2.0;
        amplitude *= 0.5;
    }}
    return result / weight * 0.5 + 0.5;
}}

fn fractal_displacement(material: i32, p: vec3<f32>, distance: f32) -> f32 {{
    switch material {{
"
    );
    for perlin in &all {
        let k = params.len();
        for row in perlin.rotation {
            params.push(slot(row, 0.0));
        }
        #[allow(clippy::cast_precision_loss, reason = "an octave count")]
        params.push(slot(perlin.period, perlin.octaves as f64));
        params.push(slot(perlin.offset, perlin.value_offset));
        params.push([perlin.intensity as f32, 0.0, 0.0, 0.0]);
        let abs = if perlin.abs {
            "\n            noise = abs(noise - 0.5) * 2.0;"
        } else {
            ""
        };
        let invert = if perlin.invert {
            "\n            noise = 1.0 - noise;"
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "        case {}: {{
            let r = vec3(dot(fractal_params[{k}].xyz, p), dot(fractal_params[{}].xyz, p), dot(fractal_params[{}].xyz, p));
            let shape = fractal_params[{}];
            var noise = fractal_perlin_octaves(r / shape.xyz, fractal_params[{}].xyz, i32(shape.w));{abs}
            noise += fractal_params[{}].w;{invert}
            return distance - clamp(noise, 0.0, 1.0) * fractal_params[{}].x;
        }}",
            perlin.material,
            k + 1,
            k + 2,
            k + 3,
            k + 4,
            k + 4,
            k + 5
        );
    }
    out.push_str("        default: {}\n    }\n    return distance;\n}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displacement_validates() {
        let file = FractFile::parse(
            "mat1_perlin_noise_enable true;\nmat1_perlin_noise_displacement_enable true;\nmat1_perlin_noise_abs true;\n\
             mat2_perlin_noise_enable true;\n",
        );
        assert_eq!(materials(&file).len(), 1);
        let mut params = Vec::new();
        let source = format!(
            "@group(0) @binding(0) var<storage, read> fractal_params: array<vec4<f32>>;\n{}",
            emit(&file, [0.0; 3], &mut params)
        );
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    }

    #[test]
    fn permutation_is_a_shuffle() {
        let mut p = permutation(12345).to_vec();
        p.sort_unstable();
        assert!(p.iter().enumerate().all(|(i, v)| usize::from(*v) == i));
    }
}

#[cfg(test)]
mod libcxx {
    /// libc++'s shuffle as clang++ on macOS computes it.
    #[test]
    fn permutation_matches_libcxx() {
        for (seed, expected) in [
            (
                12345,
                "214,107,150,213,89,244,227,8,31,41,37,199,49,59,57,102,118,46,237,197,42,56,217,242,193,20,5,77,144,76,223,80,129,66,170,65,103,128,239,132,163,175,55,253,234,124,44,203,60,17,155,202,185,104,226,86,16,121,169,219,115,33,133,153,184,2,53,173,74,198,232,158,208,99,15,114,58,141,52,212,38,47,36,3,201,50,166,159,72,126,62,179,125,211,23,122,171,30,151,29,51,247,88,243,14,7,0,233,96,105,123,79,236,117,35,75,84,12,189,222,230,207,120,240,82,146,205,92,127,220,215,101,186,216,190,228,174,26,91,254,131,43,196,110,106,154,95,108,229,63,177,160,34,152,116,221,245,251,9,45,246,182,18,64,111,39,252,167,165,238,100,248,27,250,94,24,28,187,73,136,149,130,68,200,162,93,140,143,70,4,10,191,225,231,87,176,119,113,83,224,161,172,168,138,71,139,204,98,188,206,97,209,164,142,241,178,19,109,6,255,148,11,145,21,195,54,1,210,61,147,85,235,183,90,134,13,157,181,67,78,137,249,180,156,194,112,218,135,48,69,192,25,81,32,40,22",
            ),
            (
                106,
                "53,157,60,34,51,150,197,10,224,236,144,59,22,67,69,130,101,45,147,46,154,107,28,97,68,17,123,88,104,145,21,119,120,251,212,126,172,49,136,122,94,173,221,13,149,238,27,169,61,55,71,208,254,93,4,166,240,176,43,187,83,3,19,215,168,189,64,76,128,179,134,62,31,89,38,141,39,35,223,75,255,232,135,32,87,245,8,228,195,250,220,229,140,211,65,230,218,105,85,12,129,44,108,127,206,29,200,214,177,180,237,2,217,131,48,112,196,66,253,9,133,40,80,115,142,193,202,100,216,192,73,20,185,52,198,148,171,18,241,203,15,184,160,227,158,188,191,246,16,125,143,47,249,121,56,239,201,58,194,219,164,98,92,222,57,36,182,207,106,110,84,153,91,111,165,96,0,86,1,244,37,151,77,186,248,41,210,54,5,114,23,162,226,24,72,178,63,156,175,174,234,102,161,146,124,167,209,252,74,78,183,243,113,155,26,233,82,25,14,139,70,33,181,116,231,81,118,152,42,199,242,204,103,95,138,247,117,6,213,132,90,170,79,205,137,190,50,7,30,109,99,11,225,159,235,163",
            ),
        ] {
            let expected: Vec<u8> = expected.split(',').map(|v| v.parse().unwrap()).collect();
            assert_eq!(super::permutation(seed).to_vec(), expected, "seed {seed}");
        }
    }
}
