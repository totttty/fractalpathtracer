//! Runs the generated WGSL distance on the GPU (wgpu) and compares it with
//! the f64 interpreter and Mandelbulber's own distances.
//!
//! Usage: gpu_check MANDELBULBER_ROOT SCENE.fract POINTS [WGSL_OUT]
//! POINTS: `x y z detail_size native_distance` per line (native coordinates).

#[path = "common/loader.rs"]
mod loader;

use std::{env, fs, path::Path};

use wgpu::util::DeviceExt;

const HARNESS: &str = r#"
@group(0) @binding(0) var<storage, read> fractal_params: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read> points: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> distances: array<f32>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= arrayLength(&points)) { return; }
    let point = points[id.x];
    distances[id.x] = fractal_distance(point.xyz, point.w);
}
"#;

/// GPU_CHECK_ORBIT's kernel (single fractals only: Boolean shaders prefix their orbits).
const ORBIT_HARNESS: &str = r#"
// GPU_CHECK_ORBIT: the orbit's DE, dist, r and iteration count for the first points (single fractals only).
@compute @workgroup_size(64)
fn orbit_main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x * 4u + 3u >= arrayLength(&distances)) { return; }
    let point = points[id.x];
    let orbit = fractal_orbit(point.xyz, ORBIT_FORCED);
    distances[id.x * 4u] = orbit.aux.DE;
    distances[id.x * 4u + 1u] = orbit.aux.dist;
    distances[id.x * 4u + 2u] = length(orbit.z);
    distances[id.x * 4u + 3u] = f32(orbit.formula) * 1000.0 + f32(orbit.iterations);
}

"#;

fn gpu_distances(source: &str, parameters: &[[f32; 4]], points: &[[f32; 4]]) -> Vec<f32> {
    gpu_run(source, parameters, points, "main", points.len())
}

fn gpu_run(
    source: &str,
    parameters: &[[f32; 4]],
    points: &[[f32; 4]],
    entry: &str,
    outputs_len: usize,
) -> Vec<f32> {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("GPU adapter");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .expect("GPU device");
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fractal"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let storage = |contents: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents,
                usage,
            })
        };
        let params = storage(
            bytemuck::cast_slice(parameters),
            wgpu::BufferUsages::STORAGE,
        );
        let inputs = storage(bytemuck::cast_slice(points), wgpu::BufferUsages::STORAGE);
        let size = (outputs_len * 4) as u64;
        let outputs = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &module,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            cache: None,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: inputs.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: outputs.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(points.len().div_ceil(64) as u32, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&outputs, 0, &readback, 0, size);
        queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |result| result.expect("map readback"));
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll");
        let values = bytemuck::cast_slice::<u8, f32>(&slice.get_mapped_range()).to_vec();
        values
    })
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let (root, scene, points) = match args.as_slice() {
        [_, root, scene, points, ..] => (root, scene, points),
        _ => panic!("usage: gpu_check MANDELBULBER_ROOT SCENE.fract POINTS [WGSL_OUT]"),
    };
    let shape = loader::load_shape(Path::new(root), scene);
    let shader =
        fpt_fractal::wgsl::emit_shape(&shape).unwrap_or_else(|e| panic!("WGSL generation: {e}"));
    // GPU_CHECK_SOURCE=file.wgsl replaces the generated WGSL (same parameters), for experiments.
    let generated = env::var("GPU_CHECK_SOURCE")
        .map(|path| fs::read_to_string(path).expect("GPU_CHECK_SOURCE"))
        .unwrap_or_else(|_| shader.source.clone());
    let source = format!("{generated}{HARNESS}");
    let plain = source.clone();
    if let Some(out) = args.get(4) {
        fs::write(out, &source).expect("write WGSL");
    }
    let samples = loader::read_points(points);
    let inputs: Vec<[f32; 4]> = samples
        .iter()
        .map(|s| [s[0] as f32, s[1] as f32, s[2] as f32, s[3] as f32])
        .collect();
    let gpu = gpu_distances(&plain, &shader.parameters, &inputs);
    if env::var_os("GPU_CHECK_ORBIT").is_some() {
        // GPU_CHECK_ORBIT=n stops both orbits after n iterations (no bailout); 0 runs them whole.
        let forced: i64 = env::var("GPU_CHECK_ORBIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let gpu_forced = if forced > 0 { forced } else { -1 };
        let source = format!("{source}{ORBIT_HARNESS}\nconst ORBIT_FORCED: i32 = {gpu_forced};\n");
        let orbit = gpu_run(
            &source,
            &shader.parameters,
            &inputs,
            "orbit_main",
            inputs.len() * 4,
        );
        if let fpt_fractal::boolean::Shape::Single(fractal) = &shape {
            for (s, gpu) in samples.iter().zip(orbit.chunks(4)).take(6) {
                let cpu = fractal
                    .orbit([s[0], s[1], s[2]], (forced > 0).then_some(forced as usize))
                    .unwrap();
                println!(
                    "  gpu DE {:.4e} dist {:.4e} |z| {:.4e} n {} | f64 DE {:.4e} dist {:.4e} |z| {:.4e} n {}",
                    gpu[0], gpu[1], gpu[2], gpu[3], cpu.de, cpu.dist, cpu.r, cpu.iterations
                );
            }
        }
    }
    // GPU_CHECK_SHOW=n prints the first n samples.
    let show = env::var("GPU_CHECK_SHOW")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    for (s, g) in samples.iter().zip(&gpu).take(show) {
        println!(
            "  {:.6} {:.6} {:.6}: gpu {g:.6e} native {:.6e}",
            s[0], s[1], s[2], s[4]
        );
    }
    // GPU_CHECK_NO_REFERENCE skips the f64 pass (native_check already ran it).
    let reference: Vec<f64> = if env::var_os("GPU_CHECK_NO_REFERENCE").is_some() {
        Vec::new()
    } else {
        samples
            .iter()
            .map(|s| shape.distance([s[0], s[1], s[2]], s[3]).unwrap())
            .collect()
    };
    println!(
        "formula {}: {} parameter slots, {} lines of WGSL",
        shape.fractals()[0].primary().meta.internal_name,
        shader.parameters.len(),
        shader.source.lines().count()
    );
    for (label, pairs) in [
        (
            "GPU f32 vs f64 reference",
            gpu.iter()
                .zip(&reference)
                .map(|(g, r)| (f64::from(*g), *r))
                .collect::<Vec<_>>(),
        ),
        (
            "GPU f32 vs native",
            gpu.iter()
                .zip(&samples)
                .map(|(g, s)| (f64::from(*g), s[4]))
                .collect(),
        ),
    ] {
        match loader::relative_errors(pairs) {
            Some((median, p99, max, n)) => {
                println!(
                    "  {label}: {n} samples, median {median:.2e}, p99 {p99:.2e}, max {max:.2e}"
                )
            }
            None => println!("  {label}: no samples"),
        }
    }
}
