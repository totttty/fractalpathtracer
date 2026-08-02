# Optimization investigation after voxel PR #1

This document summarizes the implementation and experiment work performed after
merge commit `2233b5e` (`Merge pull request #1 from totttty/voxel-experiment`).
It distinguishes production-facing infrastructure from optional controls and
negative research results. Raw timestamped results remain under the local,
gitignored `reports/` tree; the compact reviewable evidence is in
[`docs/evidence/procedural-jit`](evidence/procedural-jit/README.md).

## Executive conclusion

The investigation began by asking whether voxel-derived spatial data could
accelerate the exact fractal renderer without replacing the fractal with voxel
geometry. The strongest result was a different abstraction:

> Exact procedural rendering benefits most from optimizing and partially
> compiling the procedural program, not from adding spatial traversal over a
> voxel representation.

The durable architecture is therefore split into two systems:

```text
stylized voxel rendering
    direct or staged sparse-brick construction
    + conservative construction controls
    + flat fine-cell DDA

exact typed procedural rendering
    global program optimization
    + geometry/shading separation
    + optimized direct evaluators
    + optional generated MSL distance/surface evaluators
    + measured backend selection and persistent caches
```

Regional atlases, directional fields, function stitching, generated canonical
descriptor variants, shared point DAGs, affine-index code generation, and exact
voxel-leaf refinements remain explicit research controls. They are not
considered by automatic backend selection.

## 1. Voxel renderer hardening

### Implementation

- Generalized host-bridge names from voxel-specific buffer/rebuild terminology
  to acceleration-field terminology.
- Corrected multi-axis DDA boundary and tied-axis traversal behaviour.
- Added direct GPU sparse-brick construction as an alternative to dense staging
  followed by host-side sparse finalization.
- Added conservative whole-brick construction rejection.
- Added legacy, Lipschitz-certified, and interval-certified voxel coverage
  policies.
- Tested hierarchical brick traversal while retaining flat fine-cell DDA as
  the reliable traversal baseline.
- Added occupancy-mask storage as an explicit capacity-oriented mode.
- Added optional exact procedural normals and material evaluation at voxel hits.
- Added precision face/voxel offsets, secant/bisection and restricted-trace leaf
  refinement, and fixed-distance-estimator fractal-leaf controls.
- Added build, memory, occupancy, traversal, ray-class, and parity metadata.

Native quality checks used the agreed `256^3` field, bounds
`[-3.25, 3.25]^3`, surface band `0.50`, `960x540`, and 112 spp where
applicable.

### Results

| Experiment | Result | Decision |
| --- | --- | --- |
| Direct GPU construction | 1.46x aggregate build speedup; 1.20x first-frame speedup; exact occupancy and image parity | Retain as explicit build mode |
| Whole-brick rejection | 1.14x build and 1.07x first-frame speedup; exact parity | Retain as explicit construction control |
| Hierarchical brick DDA | Approximately 0.92-1.01x flat-DDA performance in the final sweep | Do not replace flat DDA |
| Occupancy masks | Roughly 19-35x less payload memory, but about 17-21% slower | Capacity mode only |
| Exact procedural normals | 1.09x aggregate speed; 4.38% lower MAE; higher LF-SSIM | Optional material-aware shading mode |
| Exact material lookup | Approximately 2-4% slower with worse aggregate quality | Rejected as a default |
| Precision offsets | No qualifying quality improvement | Research control only |
| Secant/bisection leaf refinement | 0.85x speed and worse shaded MAE/LF-SSIM | Rejected |
| Fixed-DE fractal leaves | Much slower for only minor image movement | Rejected |

The DDA corrections intentionally favor correctness. Native measurements showed
a performance cost in the initial corrected implementation, but the corrected
multi-axis contract is now covered by the Metal test suite.

## 2. Voxel-derived exact acceleration

### Sparse hierarchy and hybrid leaf tracing

The first exact-preserving design stored conservative spatial intervals in a
sparse hierarchy. Rays used coarse cells to cross empty space, descended near
uncertain regions, and returned to exact procedural marching and shading at
the leaves.

Across 36 scene/resolution/Lipschitz conditions, no candidate cleared the
promotion gates. Exact/no-skip configurations added traversal overhead, while
more aggressive configurations either remained slower or changed visible hit
behaviour. Sparse pointer chasing was a poor match for the tested Apple GPU.

### Dense range and directional textures

A dense single-level 3D texture path replaced sparse pointer chasing with cheap
GPU texture reads. The structure stored conservative distance ranges and,
optionally, derivative bounds used to calculate directional safe steps.

| Backend | Representative aggregate result |
| --- | ---: |
| Direct SDF | 60.03 ms |
| 32-cubed range grid | 102.28 ms (`0.59x` direct) |
| 64-cubed range grid | 110.65 ms (`0.54x` direct) |
| Cage range grid | `0.31-0.35x` direct; no useful certified skips |
| Directional grid | `0.57-0.59x` direct; about 1.20x over scalar range-grid evaluation |

Packing the directional field from 40 to 16 bytes per cell reduced memory by
2.5x and improved FP16 over FP32 by 1.086x. It was still only about
`0.48-0.63x` as fast as direct evaluation on inexpensive exact fixtures.

A deliberately favorable redundant-program crossover showed directional wins
beginning around eight instructions and reaching 1.22x at 64 instructions.
Global optimization then reduced the redundant program to one instruction;
optimized direct evaluation became roughly 1.62x faster than the optimized
directional path. Directional FP16 is therefore a successful representation,
not a successful production backend on this device/compiler.

### Regional residual-program atlas

The regional atlas did not store geometry or control ray traversal. Each cell
selected an exactly equivalent residual program for the current evaluation
point while ordinary procedural sphere tracing continued unchanged.

| Source program | 32-cubed atlas render speedup | End-to-end speedup | Average retained instructions |
| ---: | ---: | ---: | ---: |
| 32 instructions | 1.13x | 1.11x | 4.97 |
| 64 instructions | 1.54x | 1.53x | 4.77 |

Memory was approximately 120-190 KB, sampled distance failures were zero, and
image parity was effectively exact. Ray-weighted instrumentation was added for
distance evaluations, cell entries, same-cell/program reuse, program/header
loads, dynamic residual instructions, and full-program fallbacks across
primary, shadow, and secondary rays.

Caching the last cell, program ID, and header removed about 50-54% of program-ID
loads and 77-79% of header loads, but made rendering 2-3% slower. The reads were
not the dominant bottleneck; persistent cache state likely increased register
or control cost.

### Flat-union lowering and regional primitive lists

Compatible hard unions were lowered from general bytecode into fixed primitive
descriptors. Unsupported and nonlinear programs continued to use bytecode.

Direct primitive lists achieved 2.43x, 2.67x, 2.99x, and 3.06x over bytecode at
8, 16, 24, and 32 primitives, respectively, and 3.16x on a mixed-affine
control.

The regional atlas was then changed to select short primitive-index lists:

| Primitives | Regional end-to-end speedup over direct primitive list |
| ---: | ---: |
| 8 | 0.93x |
| 16 | 1.17x |
| 24 | 1.33x |
| 32 | 1.53x |

Hardening showed why the backend must remain selective: mixed affine geometry
won, orbit/material-heavy scenes were near neutral, tie-heavy programs lost
badly, and repeat seams erased the small render gain once construction was
included.

## 3. Procedural compiler baseline

### Global optimization and geometry/shading separation

A basic global optimizer became part of the direct baseline. A representative
program fell from 12 to 7 instructions and rendered 1.527x faster with SSIM 1.

The typed program is also split into:

- a geometry program used during marching, normal evaluation, bounds,
  specialization, and voxel construction; and
- the complete shading/orbit program evaluated after a hit is accepted.

Fixtures with removable orbit work improved by 1.73-1.88x. Negative controls
with no removable operations stayed around 0.98-1.02x.

### Typed SoA direct evaluator

Translation-only hard unions can be lowered to separate fixed-capacity sphere,
box, and plane arrays. This removes opcode dispatch, primitive type switches,
and dynamic transform matrices without building a new shader.

- 2.61-5.15x faster than optimized bytecode.
- 1.11-1.95x faster than the earlier AoS primitive list.
- Maximum image MAE `0.0000347222`; minimum SSIM 1.

### Canonical affine direct evaluator

The canonical geometry IR composes translation, uniform scale, and axis
rotations into a 3x4 world-to-local transform with a conservative distance
divisor. It flattens supported associative hard CSG, removes identical leaves,
retains source locations and strict winner order, and keeps nonlinear forms on
the bytecode fallback.

Across 12 affine conditions at 1080p/4K, one/eight bounces, and three material
classes, canonical direct evaluation won 12/12 with a 2.16x mean speedup and a
1.57-3.56x range. Minimum SSIM was `0.999994`.

## 4. Generated Metal and JIT experiments

### Topology-specialized generated MSL

The host emits an unrolled Metal distance evaluator from the optimized
geometry topology. Operations and CSG structure become shader structure while
ordinary numeric parameters remain in buffers, so numeric-only edits reuse the
same topology.

On distributed hard unions, generated MSL reached 5.04-8.49x over optimized
bytecode and approximately 2-3x over direct/regional primitive lists. Cold
compilation took roughly 2.5-3.6 seconds, while repeat compilation through
Metal's cache generally fell to 4.5-23 ms.

A runtime-source control kept ordinary bytecode within about 4% of the offline
metallib while generated topology remained 4.41-8.35x faster. The gain is from
specialization rather than selecting a different Metal compiler path.

### Generated analytic surfaces

Generated MSL was extended with an analytic `ProgramSurface` evaluator used
only at accepted hits. The distance loop stays lean; the surface path carries
the transformed point, Jacobian, winning gradient, and strict CSG winner.

At 28 distributed primitives, generated distance plus analytic surface was:

- 2.18x faster than typed SoA;
- 1.10x faster than generated distance plus interpreted surface; and
- 1.20x faster than function stitching.

The crossover occurred around 24-28 primitives, so surface specialization is a
measured candidate rather than a primitive-count default. A 1,048,576-point
validator reported zero distance or gradient failures, maximum distance error
below `1e-6`, and maximum gradient-component error about `4.25e-6`.

### Function stitching

Metal function stitching privately linked operation graphs into a precompiled
renderer host. Early narrow fixtures were 3.77-8.47x faster than bytecode and
warm archive-backed construction was about 2.6-2.9 ms. Numeric edits reused the
same topology key, while cold topology construction remained around two
seconds.

Expanded testing found a catastrophic boundary at 32 translated primitives / 64
source instructions. The 31-to-32 transition slowed stitching by roughly
9-12x. The following controls all failed to move the boundary:

- lean distance-only state;
- removal of Jacobians and gradients;
- normal versus forced-inline functions;
- split prefix/suffix graphs with a real no-inline boundary; and
- fusion into 63, 32, or 16 superinstructions.

The cliff follows final linked semantic/code-generation work, not exposed graph
node count. Function stitching remains available for research and cross-device
testing but is excluded from automatic selection.

### Persistent asynchronous preview and caches

The preview now keeps the Metal device, queue, libraries, fallback pipeline,
and active specialized pipeline alive. A topology change immediately activates
the optimized direct fallback, compiles or loads specialization on a serial JIT
queue, discards stale generations, then installs the successful result at a
sample boundary and resets accumulation.

Cache identity is split into:

```text
TopologyKey = device/compiler/source identity + optimized operation topology
WorkloadKey = TopologyKey + numeric geometry/material/path/camera/render state
```

This lets numeric edits reuse generated topology while preventing backend
decisions measured for one material/path workload from being reused for a
different workload.

## 5. Alternative generated representations

| Experiment | Result | Decision |
| --- | --- | --- |
| Wide canonical-descriptor MSL | 0/72 wins; approximately 0.35x direct performance | Failed control |
| Compact per-leaf canonical MSL | Recovered part of the loss but no stable selector-sized win | Research only |
| Shared-transform point DAG | 1.71x over direct at reuse 8, but 8-11% slower than sequential generated MSL after confirmation | Research only |
| Compact affine indices | Up to 50% less parameter storage; apparent large win did not reproduce | Capacity representation only |
| One source/library with distance and surface | 2.04-2.59x greater cold cost; 9.08-12.03x render regression at 32 primitives | Failed control |
| Tiny privately linked helper | Source compile 14-57x smaller/faster, but total cold cost 1.11-1.45x worse and 16-primitive rendering 3.81-4.97x slower | Failed control |

These experiments demonstrate that compact data, arithmetic sharing, and short
live ranges all matter. The sequential generated operation program already
combines those properties better than the separately materialized descriptor,
point-DAG, and affine-index forms on the tested Metal compiler.

## 6. Measured backend selector

Selector v8 considers only three production candidates derived from the same
globally optimized geometry program:

1. `direct`: typed SoA or canonical direct when eligible, otherwise optimized
   bytecode;
2. `generated-distance`: topology-specialized distance with direct/interpreted
   accepted-hit surface work; and
3. `generated-surface`: topology-specialized distance and analytic surface.

Selection uses:

- one untimed warmup and three Latin-rotated measured runs;
- four additional rotated measurements near a decision gate;
- target-aspect probes up to 1920 pixels wide and up to 64 spp;
- linear accumulation-buffer parity before tone mapping;
- a required 15% reduction in predicted end-to-end time (`1.176x` speedup);
- an additional 1.10x gate before promoting generated analytic surfaces; and
- persistent workload-decision caching.

One-shot `--sdf-backend auto` reuses a cached decision and otherwise renders
direct rather than spending seconds probing a short job. `--sdf-backend probe`
performs explicit measurement and populates the cache. Persistent interactive
preview may probe in the background while direct rendering remains active.

The final five-scene control selected specialization in 5/5, reused cached
decisions in 5/5, and produced pixel-exact final PNG output.

## 7. Correctness, profiling, and tooling

- Deterministic million-point field/gradient validators cover generated,
  stitched, typed-SoA, canonical, compact, shared-transform, and affine-index
  paths where applicable.
- Vocabulary fuzzing covered 12,582,912 field evaluations with zero distance
  or gradient failures.
- Exact fixtures cover sphere, box, plane, CSG, ties, transforms, and translated
  unions.
- Render metadata records linear parity, generated source/pipeline costs,
  backend cache state, and primary/secondary/shadow work estimates.
- `render-batch` keeps one Metal process alive so generated library reuse and
  warm construction are measured honestly.
- Forty-three experiment/regression runners remain indexed on the
  [`optimizations` research branch](https://github.com/totttty/fractalpathtracer/blob/optimizations/scripts/README.md);
  they are not invoked by ordinary builds or automatic backend selection.

## 8. Production and research boundary

### Production-facing infrastructure

- Basic global optimizer.
- Geometry/shading separation.
- Typed SoA and canonical affine direct evaluators.
- Optimized bytecode fallback.
- Sequential generated MSL distance and analytic surface evaluators.
- Selector v8 and its topology/workload caches.
- Persistent asynchronous direct fallback during preview compilation.
- DDA correctness fixes and voxel construction/traversal controls.

### Explicit research-only infrastructure

- Bound-grid and directional-field renderers.
- Regional residual/primitive-list renderer.
- Function stitching, lean state, graph splitting, and fusion.
- Wide/compact canonical generated MSL.
- Shared-transform DAG and affine-index generated paths.
- Dual generated-library and tiny-linked-helper controls.
- Exact voxel material, precision-offset, and leaf-refinement controls.

## 9. Final validation and branch state

The cleanup pass:

- fixed all Clippy findings without broad lint suppression;
- added default-off assertions for research flags;
- refactored render metadata through `RenderMetadataInput`;
- added seven exact benchmark fixtures;
- classified all experiment runners;
- added a compact JSON/PNG evidence package; and
- organized the branch into logical implementation, tooling, and evidence
  commits.

Final local validation completed successfully:

- `cargo fmt --check`;
- `cargo clippy --all-targets -- -D warnings`;
- `cargo test` (39 passed);
- `cargo build --release`;
- `git diff --check`; and
- shell syntax validation for every `scripts/*.sh` runner.

The complete research branch is `optimizations`. Its draft review is
[pull request #2](https://github.com/totttty/fractalpathtracer/pull/2).
