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

FPT Metal also contains a generated Mandelbulber2 formula frontend with 458
fixed formula implementations and 747-scene dependency coverage. It supports
analytic/delta estimators, hybrids, Boolean combinations, embedded custom
formulas, and Mandelbulber palette colouring through FPT Metal's material
system. The retained exact optimization stack improved full-corpus median GPU
time from 35.934 ms to 23.756 ms while all 743 comparable images remained
pixel-identical.

## Mandelbulber2 Scenes

The importer reads `.fract` scenes and formula sources from an external
[Mandelbulber2](https://github.com/buddhi1980/mandelbulber2) checkout, generates
a scene-specialized Metal distance estimator, and renders the exact procedural
surface through FPT Metal's own camera, materials, lighting, and path tracer.
It does not bake the fractal into voxels or a cached SDF.

<img src="docs/mandel-renders/production-quality-720p-50spp-contact-sheet.jpg" alt="Three Mandelbulber2 scenes path traced by FPT Metal at 1280 by 720 and 50 samples per pixel" width="1200">

These three production examples span the measured fast, median, and slow
cohort at `1280x720`, 50 spp on an Apple M1 Max. Their GPU times were 120.910
ms, 4,082.461 ms, and 127,142.222 ms respectively; shader compilation is not
included.

<table>
  <tr>
    <td width="50%"><img src="docs/mandel-renders/production-quality-720p-50spp-median.png" alt="T sphInvV4 menger3 Mandelbulber scene rendered by FPT Metal"></td>
    <td width="50%"><img src="docs/mandel-renders/production-quality-720p-50spp-slow.png" alt="transfSphereInvV3 abxTetra OT Mandelbulber scene rendered by FPT Metal"></td>
  </tr>
  <tr>
    <td align="center"><strong>T_sphInvV4_menger3</strong><br>4,082.461 ms GPU</td>
    <td align="center"><strong>transfSphereInvV3_abxTetra_OT</strong><br>127,142.222 ms GPU</td>
  </tr>
  <tr>
    <td colspan="2"><img src="docs/mandel-renders/production-quality-720p-50spp-fast.png" alt="menger coastn Mandelbulber scene rendered by FPT Metal"></td>
  </tr>
  <tr>
    <td colspan="2" align="center"><strong>menger-coastn</strong><br>120.910 ms GPU</td>
  </tr>
</table>

### How the Mandelbulber frontend works

1. The Rust importer parses a `.fract` scene, its active formula slots,
   transforms, iteration schedule, camera, lights, and supported material
   controls.
2. Formula IDs are resolved against an external Mandelbulber2 checkout. The
   source frontend translates the selected OpenCL formula bodies and shared
   helpers into scene-specialized Metal.
3. Generated shaders preserve the procedural distance estimator. FPT Metal
   sphere-traces that exact field and uses its own path tracer for final
   lighting rather than voxelising or meshing the fractal.
4. Generated libraries and Metal pipeline archives are cached by source and
   topology hash. Scene-content policies enable exact compiler
   specializations only where native-resolution image gates passed.

The exact renderer retains a Mandel-specific kernel, representable-position
march termination, 35 selected homogeneous schedules, 11 selected short-period
hybrid schedules, persistent pipeline caching, and watchdog-safe tiled retry.
Cached SDF, voxel, NAADF, sparse traversal, adaptive sampling, and approximate
normal experiments were not retained because they either lost detail, changed
images, or failed to improve end-to-end time.

Render an upstream scene after building the release binary:

```sh
MANDELBULBER_ROOT=/path/to/mandelbulber2
SCENE="$MANDELBULBER_ROOT/deploy/share/mandelbulber2/examples/mandelbulb001.fract"

target/release/fpt-metal render "$SCENE" \
  --out renders/mandelbulb001 --renderer sdf \
  --mandelbulber-root "$MANDELBULBER_ROOT" \
  --width 1280 --height 720 --samples 50 \
  --sdf-accumulation per-sample
```

Exact rendering is the default. A deliberately opt-in, device- and
output-specific approximation example is provided in
[`docs/mandel-approximation-cache-960x540-1spp.json`](docs/mandel-approximation-cache-960x540-1spp.json):

```sh
target/release/fpt-metal render "$SCENE" \
  --out renders/mandel-auto --renderer sdf \
  --mandelbulber-root "$MANDELBULBER_ROOT" \
  --width 960 --height 540 --samples 1 \
  --mandel-optimization auto \
  --mandel-selection-cache docs/mandel-approximation-cache-960x540-1spp.json
```

Cache decisions apply only when the complete scene hash, width, height, and
sample count match. Every miss or failed quality gate falls back to exact.
The packaged example accepts three candidates at SSIM at least `0.98` and
speedup at least `1.10x`, and records one explicit rejection.

| Example cache scene | Selection | Speedup | SSIM |
| --- | --- | ---: | ---: |
| `bristorbrot001` | Screen LOD 1.0 | 2.651x | 0.99116 |
| `mandelbulb powe 6 - circle` | Iteration scale 0.70 | 1.624x | 0.99934 |
| `T_sphInvV4_abxKali_hexGrid2` | Iteration scale 0.70 | 1.362x | 0.99980 |
| `hex grid 002` | Exact fallback | 1.318x candidate | 0.97496 |

Formula geometry is broadly covered, but Mandelbulber's complete appearance
system is not: volumetric fog and clouds, visible light geometry, advanced
reflection/transparency gradients, textures, and some material graphs can
still make an otherwise correct fractal look different. Three of 746 valid
corpus scenes currently reach visible diffuse-normal geometry but fail the
full path-traced appearance gate. Generated formula artifacts retain
Mandelbulber2's GPLv3-or-later boundary and stay in ignored runtime caches; the
Apache-2.0 repository does not vendor the upstream generated formula corpus.

### Rust library and portable voxel export

The crate exposes serializable fractal requests, the exact 12-byte Metal
`VoxelCell`, sparse `VoxelGrid` volumes, a deterministic built-in CPU reference
voxelizer, and a greedy-meshed GLB encoder. Real `.fract` scenes retain their
authoritative generated evaluator by dispatching the existing Metal
`voxel_build_kernel` and reading its cells back:

```sh
target/release/fpt-metal voxel-export "$SCENE" \
  --out renders/mandelbulb001.fptvox \
  --voxel-resolution 256 \
  --mandelbulber-root "$MANDELBULBER_ROOT"
```

For an authoritative original-Mandelbulber surface and image pair, select
`--surface-source mandelbulber-mesh`, preserve the raw exporter output with
`--mandel-mesh-ply-out`, and capture the original renderer with
`--mandel-reference-out`. These artifacts use the external Mandelbulber process directly;
FPT's generated Metal evaluator is not used as the reference. See
[Mandelbulber mesh voxelization](docs/mandelbulber-mesh-voxelization.md).
Thin, isolated meshes can opt into the gated two-pass
`--mandel-mesh-auto-bounds` policy; it retains tighter bounds only when both
passes prove zero occupied boundary contact and the candidate remains below
the conservative multi-plane surface-complexity ceiling.

The lossless little-endian `.fptvox` path preserves every occupied cell's
packed material tuple for direct native volume construction. The intended
direct pipeline is FPT -> `.fptvox` -> the native Metal NAADF path tracer.
Mandelbulber exports also embed independently versioned appearance, camera,
128x64 environment, and authored-material trailers. Geometry-only consumers can
ignore them; the native NAADF viewer consumes them only with
`--gpu-naadf-appearance mandel-compat`. Indexed FPTVOX8/FPTVOX11 exact-surface
exports append `FPTCOL2`, with three packed RGB8 values per triangle for
barycentric first-hit color, followed by `FPTMID1`, with the authoritative
nonzero `matN` identifier for each triangle, and `FPTNRM1`, with an optional
10-bit-per-axis shading normal for each triangle. Camera-facing splats retain
their sampled normal. Connected triangles emit the normalized average of their
source vertex normals only when it differs from the final quantized geometric
normal by more than five degrees; a zero record keeps the geometric fast path.
Readers remain compatible with flat `FPTCOL1` and geometry-only artifacts.

The latest retained FPTVOX11/NAADF authored-view gate completed all 50 ranked
scenes at a maximum 300-pixel capture axis, 384-cell output axis, 32 samples,
and four bounces. Median visibility-mask IoU was `0.99141`, median continuous-
normal mean error was `23.18` degrees, and median NAADF GPU time was
`203.24 ms`. Ranks 13 and 17 remain the two material coverage outliers. A
larger automatic splat footprint repaired those views, but was rejected after
regressing already-correct scenes by up to `10.6%`, growing artifacts by
`15-35%`, and costing rank 17 `13.8%`. The larger footprint remains available
only through explicit diagnostic flags.

Continuous Mandel renders default to the retained neutral geometry diagnostic.
Use `--mandel-appearance authored-path` to opt into the generated Mandel palette,
parsed material properties, authored background and image adjustments, and the
ordinary FPT path integrator. This mode is intended for controlled appearance
comparisons and does not alter default render or voxel-export behavior.

Experimental `--surface-normals` (`FPTVOX2`) and `--surface-planes`
(`FPTVOX3`) exports add structural surface data for continuous-FPT parity
work while leaving the version-1 default unchanged. The library also exposes
experimental `FPTVOX5` two-plane cells for multi-surface validation; the CLI
does not generate them until camera-independent plane clustering is proven.
Experimental `--surface-patches` writes `FPTVOX6` cells with the V3 primary
plane plus an optional dominant-axis bounded secondary patch, so a consumer can
recover another local surface without changing the V1 default. V6 also probes
cells rejected by the primary fit. Probe-derived cells are retained only when
the resulting surface candidate covers at least 90% of the export grid; sparse
scenes deterministically fall back to the established V6 cell set.

`--surface-triangles` writes the new `FPTVOX7` exact-surface contract. Metal
samples the specialized FPT distance field on a regular lattice, Rust runs
marching cubes and clips every triangle into the requested NAADF cells, and
Metal samples the packed material tuple only for occupied cells. Each cell
points into a compact stream of three 30-bit, cell-local triangle vertices;
NAADF intersects those triangles for primary, shadow, secondary, and
transmissive rays. This bypasses Mandelbulber, PLY, GLB, and MagicaVoxel:

```sh
fpt-metal voxel-export scene.fract --out scene.fptvox \
  --voxel-resolution 96 --surface-triangles \
  --surface-triangle-resolution 192
```

Finite surfaces whose requested volume clips occupied cells can opt into one
conservative retry:

```sh
fpt-metal voxel-export scene.fract --out scene.fptvox \
  --voxel-resolution 96 --surface-triangles \
  --surface-triangle-resolution 192 \
  --surface-triangle-auto-bounds \
  --surface-triangle-auto-bounds-margin 0.10
```

The retry expands all bounds and grid dimensions together, preserving voxel
size. It is accepted only when the expanded result has zero occupied boundary
cells; otherwise the original export is retained byte-for-byte. The path is
explicitly opt-in because intentionally clipped and unbounded fractals should
keep their authored framing.

`--surface-triangle-threshold-scale` is an explicit experimental control for
matching a fixed continuous-render camera whose distance-dependent DE
acceptance band cannot be represented by one view-independent mesh. The
default `1.0` remains compatible with Mandelbulber's mesh threshold; changing
the scale changes the extracted surface and must not be used for PLY parity
claims.

The command reports separate Metal topology, Rust marching-cubes, clipping,
and Metal material times. It also reports `topology_policy` and
`topology_threshold_scale`. The normal policy
is `mandelbulber-distance-threshold`; `distance-floor-fallback` identifies a
scene whose current generated FPT estimator did not cross Mandelbulber's mesh
threshold, so structural equivalence still needs evaluator work.

Current differential validation at `96^3` output / `192^3` topology sampling
matches Mandelbulber PLY occupancy exactly on Menger FabsAddConditional4D,
Mix Pinski 4D, and Menger IterationWeight4D. Christmas Ornaments reaches
`0.963922` cell IoU. PBR and emission tuples match on common cells; hybrid
palette colour remains the main material-parity limitation. A separate
`48^3` / `96^3` interoperability sweep exported and dispatched all 50 ranked
scenes through native NAADF. The former failure, `asurfKlein_difsGreek`, now
produces a valid but very small eight-cell surface at this resolution, so this
sweep validates the producer/consumer seam rather than claiming fine-detail
structural parity for every scene.

Reference provenance is enforced separately from interoperability. A review of
the earlier ranked-50 high-resolution report found that 49 of its 50 stored PLY
references used Mandelbulber OpenCL even though the report was later described
as CPU/double. That report remains useful as historical OpenCL comparison data,
but it is no longer accepted as the authoritative structural baseline. The
current parity harness rejects any reference whose export report does not say
`opencl: false`. A fresh CPU/double cell-only sweep at `48^3` output / `96^3`
topology completed 49 of 50 scenes with mean/median cell IoU
`0.9488 / 0.9785`; 35 of 49 reached at least `0.95`. One thin scene was empty
in the CPU reference at this intentionally coarse sampling rate. High-resolution
CPU/double visual reports must be regenerated before freezing a new release
baseline.

The exact triangle path currently trades speed for structure. At `854x480`,
4 SPP and 4 bounces, representative V7 scenes measured 1.56x to 3.46x the GPU
time of their fitted PLY/V6 counterparts. Runtime triangle-group AABBs were
byte-exact but rejected: they improved Menger by 7.9% and IterationWeight by
5.3%, while regressing Christmas by 23.4% and Mix Pinski by 3.6%.

For authored-view surfaces, `--surface-view-indexed-triangles` writes the
experimental `FPTVOX8` layout. V8 stores each globally quantized source
triangle once and gives each occupied cell a compact list of triangle indices,
instead of storing a separately clipped triangle fan in every intersected V7
cell. The native NAADF consumer uses a dedicated function-constant path, so V7
and ordinary voxel kernels do not carry the indexed traversal branch.

```sh
fpt-metal voxel-export scene.fract --out scene.fptvox \
  --voxel-resolution 192 --surface-view-indexed-triangles \
  --surface-view-splats --surface-triangle-resolution 300 \
  --surface-triangle-auto-bounds \
  --mandelbulber-root "$MANDELBULBER_ROOT"
```

V8 remains opt-in and authored-view-only. The producer and native loader reject
any cell requiring more than 1,024 triangle references; use the default
`--surface-view-triangles` V7 path for those scenes. The guarded alternative
`--surface-view-indexed-triangles-auto` builds the same candidate statistics and
writes V8 only when fan-out is safe and references reduce V7 triangle tests by
at least 28%; otherwise it writes ordinary V7.

The ranked-50 gate found 25 fan-out-safe scenes. The conservative auto rule
selected 17 and fell back to V7 for 33; all 50 outputs were byte-identical to
the corresponding explicit layout. Every selected scene improved in alternating
paired NAADF timing: `-9.1%` to `-40.8%`, median `-20.4%`. Their median
intersection reduction was 31.7%, median payload reduction was 14.9%, and
median authored-capture IoU changed from `0.91763` to `0.91774`. V7 remains the
default; auto and explicit V8 are deliberate authored-view optimization modes.

For scenes whose high-frequency surface falls between the regular topology
lattice, `--surface-view-triangles` is an explicit camera-visible experiment:

```sh
fpt-metal voxel-export scene.fract --out scene.fptvox \
  --voxel-resolution 192 --surface-view-triangles \
  --surface-view-splats \
  --surface-triangle-resolution 300 \
  --surface-triangle-auto-bounds \
  --mandelbulber-root "$MANDELBULBER_ROOT"
```

It traces the authored continuous view in Metal, connects only neighboring hit
samples that pass depth, normal, and maximum-edge discontinuity guards, clips
those triangles directly into FPTVOX7 cells in Rust, and writes no intermediate
PLY. `--surface-triangle-resolution` specifies the maximum capture axis; the
other axis is derived from the authored Mandelbulber image aspect, so a 16:9
scene requested at `192` captures `192x108` rather than a distorted square.
The export report identifies `view_dependent: true`, capture resolution,
accepted/rejected triangle counts, and diagnostic GPU time. This mode is useful
for structural diagnosis and camera-matched cached assets; it is not a complete
all-view replacement for lattice V7. On the current `300x300` pilots it exactly
reproduced the temporary PLY prototype on ranks 26 and 34, while avoiding the
large false planes produced by dense V6 patches on rank 34.

FPTVOX11 can optionally retain deterministic secondary-path surfaces:

```sh
fpt-metal voxel-export scene.fract --out scene.fptvox \
  --voxel-resolution 192 --surface-view-indexed-triangle-bvh \
  --surface-view-splats --surface-triangle-resolution 300 \
  --surface-triangle-auto-bounds --surface-view-path-bounces 2 \
  --mandelbulber-root "$MANDELBULBER_ROOT"
```

The added triangles are excluded from primary visibility and are available only
to shadow and secondary rays. This remains opt-in: the ranked-scene gate
improved continuous-FPT parity in 45 of 48 supported scenes, but increased
artifact cost and generally did not improve direct Mandelbulber image parity.

`--surface-view-splats` fills continuous hit samples that cannot participate
in a connected depth-grid triangle, plus accepted samples adjoining a rejected
depth/normal edge, with a camera-facing micro-quad. `FPTNRM1` preserves the
sampled source normal so the billboard geometry does not become the shading
normal. Connected triangles use the same contract selectively when their
source-average and quantized geometric normals differ by more than five
degrees. Each half-width defaults to `0.85x` the projected capture-pixel
footprint with a `0.45x` output-cell safety cap. Disconnected, connected-
dominant, rejected-boundary, and grazing captures raise the cap to `0.75x`;
splat-dominant views keep the compact cap to control cell references. The
option remains explicit because the result is a camera-matched surface rather
than a closed asset.

`--surface-view-ray-consistent-splats` replaces those oversized free-facing
quads for primary and auxiliary camera captures with patches clipped to the
four source-pixel boundary rays. The exporter replays the authored projection,
camera rotation, image-Y convention, and Mandel diagnostic pixel origin, then
intersects the corner rays with the sample's camera-facing depth plane. This
keeps each fallback inside its source pixel frustum, so it cannot cover a
connected triangle at a neighboring pixel center. Secondary-bounce captures
retain the established splats by default.
`--surface-view-path-ray-consistent-splats` records the selected path segment's
incoming direction and orients fallback patches perpendicular to that ray
while preserving the validated total-path footprint. It requires both the
primary ray-consistent mode and path-bounce captures. Keeping it separate
preserves the prior artifact contract for callers that prefer the established
secondary representation.

On the 49 renderable ranked scenes at `300x300`, 32 SPP, four path bounces,
and two 300-pixel secondary-surface captures, the mode improved 46 scenes
against continuous FPT and reduced median MAE from `0.33881` to `0.10771`.
The three small continuous-FPT regressions all moved closer to their authored
Mandelbulber references. On structural ranks 09/21/39/49, primary normal mean
error fell from `29.10/38.29/29.05/40.16` degrees to
`3.24/16.64/7.07/11.95`; first-bounce hit IoU rose from
`0.632/0.437/0.574/0.493` to `0.891/0.706/0.832/0.716`.

The alternating performance gate also passes. Ray-consistent patches reduce
cell-triangle references by `9.6-16.3%` and FPTVOX size by `2.9-5.5%` on the
same hard ranks. Four-pair Metal counter medians move monolithic path time by
`-3.4/-9.2/-0.2/-9.7%`; presentation stays at approximately
`0.02-0.03 ms`. The longer rank-39 canary is neutral/slightly faster at
`-0.37%`. Native build medians are mixed within a `-3.1%` to `+4.5%` band.

The secondary incoming-ray experiment improves bounce-one hit IoU on hard
ranks 09/21/39/49 from `0.891/0.706/0.832/0.716` to
`0.925/0.789/0.936/0.751`. Across all 49 renderable ranked scenes it improves
45 continuous-FPT beauty comparisons, regresses three slightly, and lowers
median MAE from `0.10771` to `0.09272`. It remains a separate opt-in because
authored Mandelbulber image error is essentially neutral and includes lighting
and material differences outside the structural capture contract. Alternating
NAADF timing is mixed: rank 09 regresses `+5.84%`, ranks 21/49 improve about
`4.5%`, and rank 39 is noisy. The option is therefore a quality experiment,
not a default performance optimization.

Additional camera sectors can be generated as independent V11 artifacts with
explicit capture controls:

```sh
fpt-metal voxel-export scene.fract --out side-sector.fptvox \
  --voxel-resolution 192 --surface-view-indexed-triangle-bvh \
  --surface-view-splats --surface-triangle-resolution 300 \
  --surface-view-camera-position 4.7,0,-0.7 \
  --surface-view-camera-yaw-pitch=-1.570796,0 \
  --surface-view-camera-fov 60 --surface-view-clip-bounds
```

Camera positions use export coordinates; yaw, pitch, and roll use radians;
FOV uses the FPT/Mandelbulber degree convention. `--surface-view-clip-bounds`
limits the structural diagnostic to the requested export AABB, which is needed
when an unbounded fractal has nearer structure outside that domain. These
controls do not merge views. They are intended for producing separate camera-
sector assets until a validated runtime view-set selector exists. With none of
the controls supplied, V11 output remains byte-identical to the established
authored-camera path.

For a production capture, set `--surface-triangle-resolution` to the maximum
axis of the intended NAADF render. The exporter now preserves the authored
aspect, so `320` produces `320x240`, `320x180`, or `320x320` as appropriate.
This matters for discontinuous fractals: view splats reconstruct the sampled
pixel grid, but no finite point sample can exactly answer a different set of
subpixel rays. The final first-five authored-camera gate at a `320` maximum
axis reaches mask IoU `0.99027 / 0.99394 / 0.99998 / 1.00000 / 0.99346`.
The splat-dominated scene 3 uses a `12.54 MiB` artifact and measured about
`0.83 ms` for the one-bounce NAADF path in the long alternating timing gate.

On the first-five authored-camera normal gate, selective connected-triangle
overrides changed zero hit pixels and reduced mean angular error from
`2.33/34.01/29.82/46.22/22.79` degrees to
`1.42/13.19/13.19/1.95/15.15`. Overrides were emitted for
`5.5/88.1/34.9/99.6/44.5%` of triangles. Three alternating 64-frame Metal
batches measured `+2.76/+1.75/+2.70/+2.34/-12.07%` GPU time versus geometric
normals. The cost is isolated to artifacts containing `FPTNRM1`; ordinary
NAADF and existing geometry-only FPTVOX artifacts retain their established
kernel path. A per-vertex barycentric `FPTNRM2` prototype improved interpolation
further but was rejected after regressing four required scenes by `3-16%`.

`--surface-view-splat-cell-cap` accepts explicit values through `4.0` for
diagnosis or output grids that cannot be matched. A `2.0` cap recovered
`0.982` IoU for the mismatched scene-3 grid, but grew the artifact to
`29.25 MB` and GPU time to `1.217 ms`; it is therefore not the default.

`--surface-view-triangle-dilation 0..1` is an additional conservative
authored-view repair. It expands only accepted triangles that share a sampled
vertex with a rejected depth/normal discontinuity; clean interior triangles
remain unchanged. A value of `0.75` raised the iridescence pilot's authored
camera coverage from `95.079%` to `99.630%` with no lost baseline hits, while
increasing clipped triangles by `6.6%`. The option defaults to zero pending a
wider scene gate. Export JSON records the requested value and the number of
triangles actually dilated.

The triangulator continues to reject adjacent triangles whose sampled normals
disagree. It tags the affected vertices and, when callers explicitly request
smaller splats than the defaults, expands only their fallback splats to at
least `1.0x` / `0.49x`. Reports expose samples actually changed by this rule as
`expanded_low_normal_splats`.

Captures whose finite-hit occupancy is at least `99.9%` retain a dense-view
minimum of `1.5x / 0.49x` for callers that explicitly request smaller splats.
Reports expose samples actually changed by this compatibility rule as
`expanded_dense_view_splats`.

The corresponding NAADF comparison camera now preserves explicit pole poses
instead of applying the interactive mouse-look pitch clamp. A fresh 50-scene
rerun changed only the two authored cameras at exactly `-90` degrees: rank 20
improved from `0.9283` to `0.9361` mask IoU and rank 36 from `0.7690` to
`0.8266`; the other 48 masks and every FPTVOX payload were unchanged. Two
follow-up reconstruction experiments remain rejected. Replacing fallback
splats with low-normal connected triangles raised rank 12 from `0.7874` to
`0.8490`, but regressed its exact-surface paired GPU median by `7.48%`.
Packing the same captures into a `96^3` rather than `192^3` surface grid cut
median payload size by `27.7%` and moved median IoU from `0.9221` to `0.9243`,
but materially regressed ranks 42 (`-0.130` IoU) and 40 (`-0.089`), so the
requested `192^3` grid remains the parity checkpoint.

The initial ranked-50 census completed 49 scenes; rank 48 produced no finite
hit in the ordinary unbounded structural march. Across those successful scenes,
median mask IoU rose
from `0.730` to `0.889`, median extra coverage was `0.036%`, and no scene lost
IoU. Ranks 12, 17, 31, and 36 exceeded `2%` extra coverage. Lowering the cell
cap from `0.45` to `0.30`, or the footprint scale from `0.85` to `0.65`, reduced
their extra coverage but lowered IoU in all four cases, so the original values
remain the explicit-mode defaults. Alternating 60-frame timing canaries measured
paired GPU changes of `+3.24%`, `-6.88%`, and `-17.90%` on ranks 24, 26, and
34 respectively. The mode is therefore a structural reconstruction tradeoff,
not a universal runtime optimization.

Camera-matched exports can additionally use `--surface-view-auto-fit-bounds`
with `--surface-triangle-auto-bounds`. The guarded mode fits the output grid to
captured hits only when their largest extent is at most `1%` of the requested
extent; otherwise it keeps the existing expand-only policy. Across the same
ranked-50 gate it selected only rank 46 (`iter fog 005`), whose mask IoU
increased from `0.0065` to `0.8635`. All other 48 successful scene metrics were
exactly unchanged. Forcing
fitted bounds on every scene was rejected because median IoU fell from `0.889`
to `0.784`.

If the ordinary authored-view march has zero true hits, the exporter now
retries only the requested finite voxel AABB. That exceptional retry uses at
least a `1024x1024` structural capture and retains edge-bounded
low-normal-agreement triangles additively alongside fallback splats. The cache
manifest records the requested and effective resolutions, export bounds, and
whether the bounded retry was selected. This recovered rank 48
(`RoadToExascale`): the requested 300px NAADF view reaches `0.9878` mask IoU,
with `0.91%` miss and `0.32%` extra coverage. Ranks 12, 24, and 44 remained
byte-identical to their previous accepted FPTVOX files. The older continuous
beauty image for rank 48 was not a valid surface oracle because its legacy
path kernel shaded a stalled, non-converged march position; the explicit hit
diagnostic correctly reports no unbounded hit.

An independent geometry gate regenerated Mandelbulber CPU/double PLY surfaces
at `48^3` output / `96^3` mesh sampling (rank 26 required mesh 192), retained
the exact FPTVOX7 references, and rendered both paths through the same NAADF
visibility pipeline. Automatic visible-bound expansion was rejected for this
comparison because it measured geometry outside the fixed Mandel volume: it
reported `13.88%` median extra coverage. With identical fixed bounds, 47 scenes
completed and median Mandel mask IoU changed from `0.600` for connected
triangles to `0.735` with splats; median extra coverage was `1.315%`. Ranks 11
and 40 had no reconstructable captured surface inside the fixed volume, and
rank 48 had no unbounded authored-camera hits at that checkpoint. Splats
regressed ranks 17, 30, 32, 36,
and 38, so they are not a general Mandel-parity solution. Rank 24 was also
checked against a retained `192^3 / mesh 384` CPU/double reference and measured
only `0.0036` IoU. The authored Mandelbulber render and continuous FPT render
both contain its large recursive spherical forms, while the CPU mesh and
lattice are sparse in that view. The mesh is therefore not a valid structural
oracle for this formula; this result must not be used to remove the continuous
surfaces. Camera-matched capture remains the applicable rank-24 reference.

Repeated camera-matched experiments can add
`--surface-view-capture-cache capture.bin`. A missing cache is populated; an
existing cache is reused only when its JSON sidecar exactly matches the
generated Metal source hash, camera bit patterns, FOV, requested/effective
sampling resolution, fallback selection, export bounds, world scale, and
structural-record contract. A rank-26 smoke test reduced the
same export from `3.84 s` to `0.12 s` and produced a byte-identical FPTVOX7
artifact. The cache currently applies only to the authored view, not auxiliary
multi-view captures.

`--surface-view-auxiliary-views 4|6|12` additionally captures small parallax
views. Fusion keeps authored-view cells and adds only cells absent from the
primary surface. This removed the naive five-view payload explosion in the
rank-26 pilot (`7.1 MB` down to `1.56 MB`), but did not improve the authored
view enough to offset its extra cells (`7.03` versus `8.88 ms` in the noisy
one-frame gate). It is therefore a research option, not a recommended default.

The optional automatic-bounds flag expands, but never shrinks, the requested
volume to contain all finite hits in the authored view plus the configured
margin and uses an aspect-matched grid so voxel size remains isotropic. This
matters for repeated/unbounded fractals whose original finite
bounds clip visible layers; omitting it intentionally retains the fixed-volume
contract.

`--surface-complex-patches` is an opt-in camera-independent completion mode
that still writes ordinary FPTVOX6. It evaluates a bounded primary fit for
probe-derived cells, admits the first weaker support tier only when it has one
coherent patch and at least ten stronger neighbors in its 3x3x3 neighborhood,
then encodes that cell as a bounded-only V6 record. Previously accepted cells
retain their existing V6 payload. At `192^3`, this improved the held-out Greek
view across silhouette IoU (`0.7327` to `0.7345`), mean/median/p95 hit-position
error (`5.29/1.71/21.39` to `5.18/1.61/21.27` voxels), and normal agreement
(`0.579` to `0.586`). The BoxFold control was metric-identical. Because the
consumer receives V6, NAADF has no new shader, buffer, or traversal branch.
Four alternating 160-frame pairs measured Greek at `1.930 -> 1.885 ms`
(`-2.3%`) and BoxFold at `1.987 -> 2.018 ms`; BoxFold's paired median was
`+0.57%`, within the `1% / 0.05 ms` noise gate. Added cells can still change
the existing acceleration structure, so this remains an explicit quality mode.
For offline structural matching, add `--surface-local-parallax`. The exporter
captures twelve nearby continuous-SDF views, fits bounded secondary patches,
and fills only one-cell gaps adjacent to the accepted V6 surface. This is an
opt-in quality mode: it can add minutes to complex Mandelbulber exports, but it
does not change default export bytes or NAADF render-time traversal. On held-out
`192^3` views, the same bounded policy improved silhouette IoU for Greek
(`0.7327` to `0.7426`), BoxFold (`0.4659` to `0.4672`), and AmazIfs Torus
(`0.9662` to `0.9688`) while also improving mean, median, and p95 hit-position
error and surface-normal agreement.
The validated quality defaults are 12 views, 384x384 samples, and two parallax
rings. Offline experiments can reduce export cost with
`--surface-local-parallax-views 4|6|12`,
`--surface-local-parallax-resolution 192|256|384`, and
`--surface-local-parallax-rings 1|2`; the export report records all three
effective values.
The first `6/256/1` sweep was 5.8x cheaper than the quality default on Greek
and improved that scene's held-out metrics, but did not improve every BoxFold
metric. Reduced settings therefore remain explicit experiments rather than an
automatic scene policy.
As a separate offline experiment, `--surface-source mandelbulber-mesh` invokes
Mandelbulber's authoritative marching-cubes PLY exporter and conservatively
converts its triangles directly to FPTVOX6 cells in the requested fixed world
bounds. This can preserve thin topology that point sampling loses, and it does
not pass through GLB or MagicaVoxel. It requires an external Mandelbulber binary
and remains opt-in because some generated scenes differ between Mandelbulber's
evaluator and FPT's generated Metal evaluator. See
[`docs/mandelbulber-mesh-voxelization.md`](docs/mandelbulber-mesh-voxelization.md)
for the exact command, format contract, and current limitations.
Selecting a `.glb` output remains supported: it deduplicates packed materials,
carries glTF specular, transmission, IOR, and emissive extensions, and embeds
the versioned marker `asset.extras.fpt_voxel_contract`. See
[`docs/fractal-library-api.md`](docs/fractal-library-api.md) for public API
signatures, exact binary offsets, payload layout, bounds controls, consumer
integration, and the Mandelbulber licensing boundary.

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

Measured on an **Apple M1 Max** at one sample per pixel and each featured
scene's native `960x540` resolution. Values are the median of four interleaved
measured runs after an uncounted warm-up, timed around the Metal command
buffers. The baseline is merge commit `2233b5e`; both paths use the default
six-sample central-difference normals.

| Scene | Baseline | Specialized | Speedup | FPS |
| --- | ---: | ---: | ---: | ---: |
| Render005, Cage | 114.5 ms | **102.4 ms** | **1.12x** | **9.77** |
| Render0ad03, Tower | 66.0 ms | **51.6 ms** | **1.28x** | **19.40** |
| Glass, Cage | 114.5 ms | **97.7 ms** | **1.17x** | **10.24** |

Built-in SDF scenes automatically use compact precompiled Metal libraries that
omit the large typed-program payload. Cage and Tower additionally use
scene-family-specialized distance entry points, while accepting all ordinary
scene parameters from the JSON file. At the README's native 112 spp,
Render005 and Glass were pixel-exact against the baseline. Render0ad03 passed
the strict image gate with MAE `1.180`, SSIM `0.9717`, and low-frequency SSIM
`0.99993`; its small delta comes from specialized fast-math code generation.

Central differences remain the default. `--sdf-normal-mode tetra` is an
explicit faster-quality option that evaluates four normalized tetrahedral
offsets instead of six axis offsets. In the controlled 32 spp matrix it added
another `1.20-1.35x` over the generic central-normal path and passed the strict
image gate, but it is not selected automatically.

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
