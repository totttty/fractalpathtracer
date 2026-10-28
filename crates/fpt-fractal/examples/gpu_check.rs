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

fn gpu_distances(source: &str, parameters: &[[f32; 4]], points: &[[f32; 4]]) -> Vec<f32> {
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
        let size = (points.len() * 4) as u64;
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
            entry_point: Some("main"),
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
    let fractal = loader::load_fractal(Path::new(root), scene);
    let shader =
        fpt_fractal::wgsl::emit(&fractal).unwrap_or_else(|e| panic!("WGSL generation: {e}"));
    let source = format!("{}{HARNESS}", shader.source);
    if let Some(out) = args.get(4) {
        fs::write(out, &source).expect("write WGSL");
    }
    let samples = loader::read_points(points);
    let inputs: Vec<[f32; 4]> = samples
        .iter()
        .map(|s| [s[0] as f32, s[1] as f32, s[2] as f32, s[3] as f32])
        .collect();
    let gpu = gpu_distances(&source, &shader.parameters, &inputs);
    let reference: Vec<f64> = samples
        .iter()
        .map(|s| fractal.distance([s[0], s[1], s[2]], s[3]).unwrap())
        .collect();
    println!(
        "formula {}: {} parameter slots, {} lines of WGSL",
        fractal.primary().meta.internal_name,
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
