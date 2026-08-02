<div align="center">

# FPT Metal

**A native Rust + Metal fractal path tracer with SDF and voxel backends for macOS.**

[![macOS](https://img.shields.io/badge/platform-macOS-111111?style=flat-square&logo=apple)](https://developer.apple.com/metal/)
[![Rust](https://img.shields.io/badge/host-Rust-dea584?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![Metal](https://img.shields.io/badge/GPU-Metal-6e6e73?style=flat-square)](https://developer.apple.com/metal/)

<img src="docs/readme-renders/01-render005-metal.png" alt="Orange Cage fractal rendered by FPT Metal" width="920">

FPT Metal is a native **Metal/Rust port of
[adam-pa/FPT](https://github.com/adam-pa/FPT/)**, preserving its SDF scenes,
camera model, materials, lighting, and postprocessing while replacing the
Python/OpenGL runtime with a standalone macOS renderer.

</div>

## What Changed in This Port

| Upstream FPT | FPT Metal |
| --- | --- |
| Python host and OpenGL/GLSL rendering | Rust CLI and AppKit host with native Metal compute shaders |
| Runtime shader and application dependencies | Embedded `.metallib` and a standalone release binary with no Python or Zig runtime |
| Original Beauty JSON scenes | Compatibility loader plus explicit Metal implementations of all nine Beauty presets |
| Fixed shader-authored fractals | Typed, scene-independent SDF programs for generated fractals, transforms, folds, repetition, CSG, gradients, and materials |
| Procedural SDF traversal only | Optional GPU voxelization with reusable sparse-brick storage and exact grid DDA traversal |
| Six finite-difference samples per normal | Analytic derivative propagation for typed programs, measured up to `1.83x` faster |
| Per-sample automatic focus and full SDF material queries | One-shot GPU focus prepass and distance-only marching, producing a `1.71x` combined featured-scene speedup |
| Original viewport and offline output | Progressive interactive path tracing, responsive camera controls, scene cycling, and deterministic offline accumulation |
| Manual visual inspection | Automated MAE, RMSE, SSIM, blank-image checks, strict parity gates, diff sheets, and contact sheets |
| Basic runtime feedback | Real GPU milliseconds, FPS, workload estimates for marches/normals/bounces, and Metal System Trace capture |
| Environment and post effects | Radiance RGBE HDR input and shared offline/interactive tone mapping, exposure, saturation, aberration, and highlights |

The experimental NAADF backend used during development was removed before
release. The repository now provides shared SDF and voxel geometry paths plus
the generic typed-program route for future procedurally generated fractals.
Arbitrary GLSL translation remains intentionally out of scope.

## Quick Start

Requirements: macOS with a Metal-capable GPU, Xcode command-line tools, and a
current stable Rust toolchain.

```sh
cargo build --release

target/release/fpt-metal render scenes/readme/01-Render005.json \
  --out renders/readme
```

`build.rs` compiles and embeds the Metal library in
`target/release/fpt-metal`, producing a standalone executable.

## Featured Scenes

<table>
  <tr>
    <td width="50%"><img src="docs/readme-renders/08-render0ad03-metal.png" alt="Gold Tower fractal rendered by FPT Metal"></td>
    <td width="50%"><img src="docs/readme-renders/09-glass-metal.png" alt="Teal and violet Cage fractal rendered by FPT Metal"></td>
  </tr>
  <tr>
    <td align="center"><strong>Render0ad03</strong><br>Recursive gold Tower</td>
    <td align="center"><strong>Glass</strong><br>Teal and violet Cage</td>
  </tr>
</table>

The presets and scene files are bundled. Reproduce all three images locally:

```sh
mkdir -p renders/readme

target/release/fpt-metal render scenes/readme/01-Render005.json --out renders/readme
target/release/fpt-metal render scenes/readme/08-Render0ad03.json --out renders/readme
target/release/fpt-metal render scenes/readme/09-Glass.json --out renders/readme
```

Use `--samples 32` for a quicker draft. To render all seven upstream README
re-creations and build a contact sheet:

```sh
FPT_README_SAMPLES=32 scripts/run_readme_reproductions.sh
```

## Performance

Measured on an **Apple M1 Max** at one sample per pixel, using each featured
scene's native resolution. Values are the median of four measured runs after an
uncounted warm-up, timed around the Metal command buffers.

| Scene | Render time (1 spp) | FPS (1 spp) |
| --- | ---: | ---: |
| Render005, Cage (`960x540`) | **120.0 ms** | **8.33** |
| Render0ad03, Tower (`720x720`) | **57.5 ms** | **17.39** |
| Glass, Cage (`960x540`) | **109.0 ms** | **9.18** |

Generated typed SDF normals use analytic derivatives rather than six finite
differences per hit:

| Generated benchmark | Central normals | Analytic normals | Speedup | Image delta |
| --- | ---: | ---: | ---: | ---: |
| Folded box + subtractive sphere | 109.3 ms | **84.9 ms** | **1.29x** | SSIM `0.9986` |
| Sphere fixture | 126.0 ms | **69.0 ms** | **1.83x** | SSIM `0.999995` |

The exact typed-program renderer also includes a procedural compiler and
measured Metal backend selector. These figures come from separate controlled
matrices and should be read as backend-relative results rather than one shared
scene benchmark:

| Compiler result | Measurement | Correctness |
| --- | ---: | --- |
| Basic global optimizer | 12 to 7 instructions; **1.53x** | SSIM `1` |
| Typed SoA direct evaluator | **2.61-5.15x** over optimized bytecode | Maximum MAE `0.0000347` |
| Canonical affine direct evaluator | **2.16x mean**, `1.57-3.56x`; 12/12 wins | Minimum SSIM `0.999994` |
| Generated analytic surface, 28 primitives | **2.18x** over typed SoA; `1.10x` over generated distance | 1,048,576 samples; zero field/gradient failures |
| Selector v8 control | Specialized backend selected and cached in **5/5** scenes | Pixel-exact final PNGs |

See the [optimization investigation summary](docs/optimization-investigation-summary.md)
and [compact evidence package](docs/evidence/procedural-jit/README.md) for the
experiment boundaries, negative controls, and reproduction details.

Re-run the benchmarks:

```sh
scripts/run_typed_program_benchmark.sh

cp target/release/fpt-metal /tmp/fpt-metal-baseline
# Make and build a renderer change, then compare it:
BASELINE_BIN=/tmp/fpt-metal-baseline scripts/run_readme_engine_benchmark.sh
```

## Interactive Path Tracing

```sh
target/release/fpt-metal preview scenes/readme/09-Glass.json \
  --pathtrace --width 960 --height 540 --samples 256 --sdf-profile
```

Omit `--pathtrace` for the fast viewport renderer. Progressive path tracing
resets automatically after camera movement.

| Input | Action |
| --- | --- |
| `W A S D` | Move on the X/Z plane |
| `Q / E` or `Space` | Move vertically |
| Mouse drag or arrow keys | Rotate camera |
| `Shift` | Increase movement speed |
| `[` / `]` | Cycle available scenes |
| `Esc` | Close the viewer |

The `--sdf-profile` HUD samples GPU time every 30 frames and estimates primary
march, shadow march, normal, and bounce contributions. Capture a complete
Instruments trace with:

```sh
scripts/run_metal_system_trace.sh
```

## Voxel Renderer

The voxel backend converts the same procedural SDF programs into a reusable GPU
field, then traces that field instead of evaluating the distance estimator for
every ray step. Camera, lighting, materials, path accumulation, and
postprocessing remain shared with the SDF renderer.

<table>
  <tr>
    <td width="33%"><img src="docs/readme-renders/01-render005-voxel.png" alt="Render005 rendered with the 256 cubed voxel backend"></td>
    <td width="33%"><img src="docs/readme-renders/08-render0ad03-voxel.png" alt="Render0ad03 rendered with the 256 cubed voxel backend"></td>
    <td width="33%"><img src="docs/readme-renders/09-glass-voxel.png" alt="Glass rendered with the 256 cubed voxel backend"></td>
  </tr>
  <tr>
    <td align="center"><strong>Render005</strong></td>
    <td align="center"><strong>Render0ad03</strong></td>
    <td align="center"><strong>Glass</strong></td>
  </tr>
</table>

All three examples use the same release settings: `256^3`, bounds
`[-3.25, 3.25]^3`, surface band `0.50`, face normals, `960x540`, and 112 spp.
Measured on an Apple M1 Max against SDF renders from the same build and sample
count:

| Scene | MAE | SSIM | LF-SSIM | SDF render | Voxel build | Voxel render | Cached speedup |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Render005 | `26.70` | `0.291` | `0.401` | 11,380 ms | 52 ms | 3,456 ms | **3.29x** |
| Render0ad03 | `31.63` | `0.328` | `0.172` | 4,837 ms | 52 ms | 3,547 ms | **1.36x** |
| Glass | `55.75` | `0.202` | `0.493` | 9,760 ms | 60 ms | 3,087 ms | **3.16x** |

MAE is lower-is-better; SSIM and low-frequency luminance SSIM are
higher-is-better. Build time is reported separately because the field is reused
across progressive samples and camera movement.

Reproduce the gallery, comparison JSON, performance summary, and contact sheet:

```sh
scripts/run_voxel_readme.sh
```

The harness defaults to the settings above. Override `WIDTH`, `HEIGHT`,
`SAMPLES`, `VOXEL_RESOLUTION`, or `OUT_DIR` for experiments. Render one scene
directly with:

```sh
target/release/fpt-metal render scenes/readme/09-Glass.json \
  --renderer voxel \
  --voxel-resolution 256 \
  --voxel-surface-band 0.50 \
  --voxel-storage sparse-bricks \
  --voxel-normal face \
  --width 960 --height 540 --samples 112 \
  --out renders/voxel
```

The build kernel samples `distanceSdf` at each cell centre, marks cells within
the configured surface band, evaluates `userSdf` once for occupied-cell
material data, and packs colour, roughness, specular, translucency, IOR, and
emission. The default staging path compacts occupied `4x4x4` bricks behind a
dense page table; `--voxel-build direct` instead constructs sparse bricks on the
GPU and falls back to staging if its capacity is exceeded. Conservative brick
rejection is available through `--voxel-brick-rejection`, and
`--voxel-storage dense` retains the complete field as a correctness reference.
Rays intersect the field bounds and use exact grid DDA to find the first packed
cell before continuing through the shared path-tracing and postprocess stages.

Resolutions up to `512` remain available as an opt-in offline quality mode, but
`256` is the recommended balance. The staging builder creates a dense field
before sparse compaction and can peak near `1.8 GiB` for a `512^3` Tower build.
The direct builder avoids that dense staging field and caps its `512^3` sparse
allocation at approximately 480 MiB.

## Generated SDF Programs

Generated fractals do not require a new scene-specific shader. A bounded typed
instruction stream describes transforms, primitives, CSG, orbit colouring, and
physical material properties:

```json
{
  "sdf_program": {
    "operations": [
      {"op": "repeat", "value": [2, 2, 2]},
      {"op": "sort_desc"},
      {"op": "box", "value": [0.7, 0.5, 0.3], "orbit_weight": 1},
      {"op": "sphere", "radius": 0.35, "combine": "subtract"}
    ],
    "material": {"mode": "gradient", "roughness": 0.65},
    "gradient": [
      {"position": 0, "color": [0.1, 0.6, 1.0]},
      {"position": 1, "color": [1.0, 0.2, 0.1]}
    ]
  }
}
```

Analytic normals are selected automatically for typed programs. Use
`--sdf-normal-mode central` to compare with finite differences. Programs are
bounded to 64 operations and 16 gradient stops.

For stable typed topologies, `--sdf-backend probe` benchmarks the optimized
direct, generated-distance, and generated-analytic-surface candidates with
identical deterministic probes and caches the qualifying decision.
`--sdf-backend auto` reuses that decision; on a one-shot cache miss it renders
directly instead of paying cold JIT discovery cost. Interactive preview keeps
the direct evaluator active while specialization is compiled asynchronously.

Radiance RGBE `.hdr` environments are supported through `world.hdri`, using an
absolute path or a path relative to the scene JSON.

## Verification

```sh
cargo test --release
FPT_ROOT=../FPT scripts/run_smoke.sh
FPT_ROOT=../FPT scripts/run_all_parity.sh
FPT_ROOT=../FPT scripts/run_high_sample_parity.sh
FPT_ROOT=../FPT scripts/run_capability_tests.sh
```

One-off comparisons generate metrics and a three-panel baseline/candidate/diff
sheet:

```sh
target/release/fpt-metal compare baseline.png candidate.png \
  --report reports/comparison.json --strict
```

FPT compatibility behavior and bundled presets are derived from
[adam-pa/FPT](https://github.com/adam-pa/FPT/).
