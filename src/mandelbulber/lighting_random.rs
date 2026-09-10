//! Deterministic random-light placement against the generated Metal distance field.
//! No CPU approximation of the fractal evaluator is used.
use super::{
    MandelbulberScene, lighting::PointLight, parse_bool, parse_number, parse_rgb16, parse_vec3,
};
use crate::ffi::{FptMandelbulberFieldSample, FptRenderConfig, fpt_mandelbulber_sample_field};
use anyhow::{Result, ensure};
use std::ffi::CStr;

#[derive(Clone)]
struct Mt19937 {
    state: [u32; 624],
    index: usize,
}
impl Mt19937 {
    fn new(seed: u32) -> Self {
        let mut state = [0u32; 624];
        state[0] = if seed == 0 { 4357 } else { seed };
        for i in 1..624 {
            state[i] = 1812433253u32
                .wrapping_mul(state[i - 1] ^ (state[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        Self { state, index: 624 }
    }
    fn next(&mut self) -> u32 {
        if self.index == 624 {
            for i in 0..624 {
                let y = (self.state[i] & 0x80000000) | (self.state[(i + 1) % 624] & 0x7fffffff);
                self.state[i] = self.state[(i + 397) % 624]
                    ^ (y >> 1)
                    ^ if y & 1 != 0 { 0x9908b0df } else { 0 };
            }
            self.index = 0;
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c5680;
        y ^= (y << 15) & 0xefc60000;
        y ^= y >> 18;
        y
    }
    fn unit(&mut self) -> f64 {
        f64::from(self.next()) / 4294967296.0
    }
    fn integer(&mut self, count: u32) -> u32 {
        let scale = u32::MAX / count;
        loop {
            let v = self.next() / scale;
            if v < count {
                return v;
            }
        }
    }
}

pub fn generate(
    scene: &MandelbulberScene,
    source: &str,
    cfg: &FptRenderConfig,
) -> Result<Vec<PointLight>> {
    let p = &scene.main_parameters;
    let flag = |key, default| -> Result<bool> {
        Ok(p.get(key)
            .map(|s| parse_bool(s))
            .transpose()?
            .unwrap_or(default))
    };
    if !flag("random_lights_group", false)? {
        return Ok(vec![]);
    }
    let num = |key, default| -> Result<f64> {
        let n = p
            .get(key)
            .map(|s| parse_number(s))
            .transpose()?
            .unwrap_or(default);
        ensure!(n.is_finite(), "invalid {key}");
        Ok(n)
    };
    let count = num("random_lights_number", 20.0)?;
    let seed = num("random_lights_random_seed", 1234.0)?;
    ensure!(
        (0.0..=256.0).contains(&count)
            && count.fract() == 0.0
            && (0.0..=u32::MAX as f64).contains(&seed)
            && seed.fract() == 0.0,
        "invalid random light count/seed"
    );
    let radius = num("random_lights_distribution_radius", 3.0)?;
    let distance_limit = num("random_lights_max_distance_from_fractal", 0.1)?;
    let intensity = num("random_lights_intensity", 1.0)?;
    let size = num("random_lights_size", 0.1)?;
    let cone = num("random_lights_soft_shadow_cone", 1.0)?;
    ensure!(
        radius > 0.0
            && distance_limit > 0.0
            && intensity >= 0.0
            && size >= 0.0
            && (0.0..=180.0).contains(&cone),
        "invalid random light dimensions/intensity"
    );
    let center = p
        .get("random_lights_distribution_center")
        .map(|s| parse_vec3(s))
        .transpose()?
        .unwrap_or([0.0; 3]);
    let color = |key| -> Result<[f32; 3]> {
        Ok(p.get(key)
            .map(|s| parse_rgb16(s))
            .transpose()?
            .unwrap_or([1.0; 3]))
    };
    let c1 = color("random_lights_color")?;
    let c2 = color("random_lights_color_2")?;
    let mode = p
        .get("random_lights_coloring_type")
        .map(String::as_str)
        .unwrap_or("0");
    ensure!(
        matches!(
            mode,
            "0" | "random" | "1" | "single" | "2" | "two" | "3" | "distance"
        ),
        "unsupported random light coloring"
    );
    let mut shader = super::compiler::retain_metal_kernels(source, &[])?;
    shader.push_str(
        r#"
kernel void mandelbulber_field_sample_kernel(device const float4 *points [[buffer(0)]],
    device float4 *samples [[buffer(1)]], constant FptRenderConfig &cfg [[buffer(2)]],
    uint gid [[thread_position_in_grid]]) {
    samples[gid]=float4(mapSdf(points[gid].xyz,cfg),0,0,0);
}
"#,
    );
    let scale = cfg.set_values[0].max(1.0) as f64;
    let mut rng = Mt19937::new(seed as u32);
    let mut lights = Vec::new();
    for id in 0..count as u32 {
        let mut trials = 0;
        let mut multiplier = 1.0;
        let (position, distance) = loop {
            ensure!(
                trials < 4096,
                "could not place random light {id} outside the fractal after 4096 trials"
            );
            let mut candidates = Vec::new();
            let mut points = Vec::new();
            let mut preview = rng.clone();
            let mut preview_radius = multiplier;
            for i in 0..64 {
                let position =
                    center.map(|v| v + (2.0 * preview.unit() - 1.0) * radius * preview_radius);
                let mapped = super::map_mandel_point(position).map(|v| (v * scale) as f32);
                ensure!(
                    mapped.iter().all(|v| v.is_finite()),
                    "random light coordinate overflow"
                );
                points.push([mapped[0], mapped[1], mapped[2], 0.0]);
                if (trials + i + 1) % 100 == 0 {
                    preview_radius *= 1.01;
                }
                candidates.push((position, preview.clone(), preview_radius));
            }
            let mut samples = vec![FptMandelbulberFieldSample::default(); points.len()];
            let mut error = [0i8; 4096];
            let status = unsafe {
                fpt_mandelbulber_sample_field(
                    c"unused.metallib".as_ptr(),
                    shader.as_ptr().cast(),
                    shader.len(),
                    cfg,
                    points.as_ptr().cast(),
                    points.len(),
                    samples.as_mut_ptr(),
                    error.as_mut_ptr(),
                    error.len(),
                )
            };
            ensure!(
                status == 0,
                "random light distance query: {}",
                unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy()
            );
            let mut accepted = None;
            for ((position, state, radius), sample) in candidates.into_iter().zip(samples) {
                trials += 1;
                rng = state;
                multiplier = radius;
                let distance = f64::from(sample.distance) / scale;
                if distance.is_finite() && distance > 0.0 && distance < distance_limit * multiplier
                {
                    accepted = Some((position, distance));
                    break;
                }
            }
            if let Some(value) = accepted {
                break value;
            }
        };
        let color = match mode {
            "0" | "random" => {
                let mut c = [0.0f32; 3];
                for v in &mut c {
                    *v = (20000 + rng.integer(80001)) as f32;
                }
                let max = c.into_iter().fold(0.0f32, f32::max);
                c.map(|v| v / max)
            }
            "1" | "single" => c1,
            _ => {
                let k = if matches!(mode, "2" | "two") {
                    rng.integer(10001) as f32 / 10000.0
                } else {
                    (distance / distance_limit) as f32
                };
                std::array::from_fn(|i| {
                    if matches!(mode, "2" | "two") {
                        k * c1[i] + (1.0 - k) * c2[i]
                    } else {
                        k * c2[i] + (1.0 - k) * c1[i]
                    }
                })
            }
        };
        let power = intensity * distance.max(0.1 * distance_limit).powi(2);
        ensure!(
            power.is_finite() && power <= f32::MAX as f64,
            "random light intensity overflow"
        );
        lights.push(PointLight {
            id: 10000 + id,
            position: super::map_mandel_point(position).map(|v| v as f32),
            camera_relative: false,
            color,
            intensity: power as f32,
            decay_power: 2,
            cast_shadows: flag("random_lights_cast_shadows", true)?,
            penetrating: flag("random_lights_penetrating", true)?,
            cone_radians: cone.to_radians() as f32,
            size: size as f32,
        });
    }
    Ok(lights)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mt19937_matches_standard_seed_vector() {
        let mut rng = Mt19937::new(5489);
        assert_eq!(
            [rng.next(), rng.next(), rng.next(), rng.next()],
            [3499211612, 581869302, 3890346734, 3586334585]
        );
    }
    #[test]
    fn cloned_rng_replays_unused_candidates() {
        let mut rng = Mt19937::new(1234);
        for _ in 0..30 {
            rng.next();
        }
        let mut cloned = rng.clone();
        for _ in 0..1000 {
            assert_eq!(rng.next(), cloned.next());
        }
    }
}
