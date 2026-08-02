# Procedural JIT experiments

The exact typed-SDF renderer is now best treated as a procedural compiler with
multiple execution backends. Spatial structures remain useful for stylised
voxel rendering and selected regional workloads, but the strongest exact
renderer exposes the optimized operation topology directly to Metal.

Raw paths below identify the timestamped local runs used during the
investigation. The complete `reports/` tree is intentionally gitignored. The
committed review results and representative comparison sheets are in the
[compact evidence package](evidence/procedural-jit/README.md).

## Backend status

| Surface | Status |
| --- | --- |
| Global optimizer and geometry/shading split | Production default |
| Typed SoA and canonical affine direct evaluators | Production direct candidates |
| Sequential generated MSL distance and analytic surface | Selective production candidates |
| Selector v8, async direct fallback, and persistent caches | Production policy |
| Function stitching and its state/fusion variants | Explicit research only |
| Bound-grid, directional, and regional fields | Explicit research only |
| Canonical-descriptor, compact-DAG, and affine-index codegen | Explicit research only |
| Dual full-source and tiny private-linked libraries | Failed controls retained explicitly |

Automatic selection is intentionally limited to direct,
`generated-distance`, and `generated-surface`.

## Implemented pipeline

The current experimental pipeline is:

```text
typed SDF program
    -> basic global optimization
    -> geometry/shading separation
    -> topology hash
    -> optimized bytecode fallback
    -> function-stitched distance and surface graphs
    -> explicit MTLBinaryArchive
    -> privately AIR-linked compute pipelines
```

Numeric parameters remain in `FptRenderConfig`. Changing positions, radii, or
other values supported by an existing topology updates only the parameter
buffer. Changing the opcode/CSG graph produces a different topology key and a
new archive.

The stitched backend currently accepts the full typed geometry vocabulary:
absolute folds, translation, signed scale, X/Y/Z rotation, repeat, descending
coordinate sort, spheres, boxes, planes, and hard union/intersection/
subtraction. Unsupported operations still fail explicitly instead of silently
falling back inside the stitched backend.

## Runtime compiler-path control

`scripts/run_runtime_source_control_experiment.sh` compares:

1. Offline-metallib bytecode.
2. Unmodified bytecode compiled through `newLibraryWithSource`.
3. Generated topology source compiled through the same runtime path.

All paths use Metal 2.4 and fast math explicitly. At 320x180, 64 spp, seven
runs, the runtime-source bytecode control remained within approximately 4% of
the offline metallib. Generated topology was 4.41-8.35x faster than the
runtime-source bytecode control. This rules out the offline-versus-runtime
compiler path as the cause of the topology result.

Evidence:
`reports/runtime-source-control/20260801-215127/summary.json`.

## Function stitching

The stitch host is packaged as a loadable metallib with unresolved
`[[visible]]` distance and surface declarations. At pipeline creation, Metal
privately AIR-links the runtime graphs into the render and focus kernels. This
avoids rerunning the Metal source front end for topology composition.

At 320x180, 64 spp, seven runs:

| Primitives | Bytecode | Source topology | Stitched normal | Stitched inline |
| ---: | ---: | ---: | ---: | ---: |
| 8 | 119.75 ms | 33.57 ms | 31.74 ms | 31.19 ms |
| 16 | 340.98 ms | 61.80 ms | 56.99 ms | 50.41 ms |
| 24 | 541.92 ms | 83.37 ms | 71.14 ms | 70.67 ms |
| 32 | 775.68 ms | 95.62 ms | 91.55 ms | 91.77 ms |

The stitched variants were 3.77-8.47x faster than bytecode and matched or beat
generated source. First-observed construction was 1.89-2.26 seconds, while
warm stitched construction was typically 2.6-2.9 ms. Image parity had maximum
MAE 0.0000405093 and minimum LF-SSIM 1.

Evidence: `reports/function-stitching/20260801-221213/summary.json`.

## Explicit topology and pipeline cache

Stitched libraries and their linked compute pipelines are stored in an
`MTLBinaryArchive`. The default cache is below the process temporary directory;
set `FPT_STITCH_CACHE_DIR` to use a controlled location.

The cache key contains the stitch-host implementation digest, optimized opcode
and CSG topology, stitching mode, surface specialization mode, effective entry
point, focus-pipeline requirement, and validation mode. Ordinary instruction
data is deliberately excluded.

The controlled cache test produced:

- New 13-primitive topology: populated in 1,987.99 ms.
- Numeric-only edit: reused the same key and loaded in 4.10 ms.
- 14-primitive topology edit: produced a new key and populated in 1,994.32 ms.
- Reuse of the original topology: loaded in 4.48 ms.
- All three stitched-versus-bytecode comparisons were pixel-exact.

Evidence: `reports/stitch-cache/20260801-221934/summary.json`.

## Persistent asynchronous preview context

The interactive preview now keeps the Metal device, command queue, offline
library, stitch-host library, fallback pipeline, and active pipeline alive for
the lifetime of the window. A topology request follows this state transition:

```text
scene/topology change
    -> activate optimized-bytecode fallback immediately
    -> compile or load the stitched pipeline on a serial JIT queue
    -> discard the result if a newer topology superseded it
    -> install the result on the main/sample boundary
    -> reset accumulation and continue with the stitched pipeline
```

An unsupported topology or compilation/archive failure leaves the fallback
pipeline active and reports the reason in the preview HUD. Camera and numeric
parameter changes do not rebuild the pipeline because those values remain in
`FptRenderConfig`.

A headless lifecycle regression exercises the same cold/warm pipeline path. On
the 96x54, one-sample exact-sphere fixture used by the test:

| Path | Render/build time | Result |
| --- | ---: | --- |
| Bytecode fallback during cold JIT | 0.316 ms | Completed before JIT |
| Cold stitched build | 1,147.31 ms | Archive populated |
| Cold stitched render | 0.192 ms | Max absolute error 0 |
| Bytecode fallback during warm load | 0.318 ms | Remained available |
| Warm stitched load | 0.791 ms | Archive reused |
| Warm stitched render | 0.186 ms | Max absolute error 0 |

These figures validate lifecycle and parity rather than establish a crossover;
the dedicated performance sweeps above remain the performance evidence. The
test is deterministic and creates a fresh archive for its cold pass.

## Generated analytic surface

The stitched state also carries the winning analytic gradient. This removes
the remaining geometry interpreter at accepted hits for the supported union
topology.

| Primitives | Distance only | Distance + surface | Surface change |
| ---: | ---: | ---: | ---: |
| 8 | 28.90 ms | 30.34 ms | 0.95x |
| 16 | 54.15 ms | 53.23 ms | 1.02x |
| 24 | 73.76 ms | 63.65 ms | 1.16x |
| 32 | 91.53 ms | 75.38 ms | 1.21x |

Surface specialization has a small fixed cost and should remain selective for
small programs. It becomes a material win from the tested 16-24 primitive
range onward. The 32-primitive stitched distance-and-surface renderer was
10.23x faster than optimized bytecode. Quality remained effectively exact.

Evidence: `reports/stitched-surface/20260801-222509/summary.json`.

## Field-level differential validation

`--sdf-stitch-validation` dispatches 1,048,576 deterministic field samples
before rendering and compares interpreted and stitched distance and gradient
results. A render fails if the distance error exceeds
`2e-5 * (1 + abs(distance))` or the maximum gradient-component error exceeds
`1e-4`.

The fuzz sweep covered 4, 8, 16, and 32 primitive topologies with three numeric
parameter variants each:

- 12,582,912 evaluated points.
- Zero distance failures.
- Zero gradient failures.
- Maximum distance error: 0.00000333786.
- Maximum gradient error: 0.0000388324.
- All eight numeric variants reused their topology archive.

The gradient tolerance was chosen from evidence: an initial `2e-5` threshold
found one point at `3.883e-5`, consistent with fast-math normalization order
rather than a branch or sign mismatch.

Evidence:
`reports/stitch-differential-fuzz/20260801-223100/summary.json`.

## Expanded typed vocabulary

Each stitched state now carries the transformed position, distance scale,
distance winner, analytic gradient, and the three rows of the position
Jacobian. Transform nodes update position and Jacobian using the same operation
order as `programSurfaceInterpreted`; primitive nodes transform their local
analytic gradient back into source space before applying CSG.

CSG selection deliberately uses the interpreter's strict comparisons:

```text
union:         candidate < current
intersection:  candidate > current
subtraction:  -candidate > current
tie:           retain the previous winner and gradient
```

The validator reserves deterministic origin/bounds samples in addition to its
random field sweep. `Exact_Tie.json` uses opposing plane gradients at an exact
intersection tie, making an accidental `>=` selection observable.

The reproducible vocabulary sweep covered six topologies and a numeric-only
variant of each:

- 12,582,912 field evaluations.
- Zero distance failures and zero gradient failures.
- Maximum distance error: 0.000000596046.
- Maximum gradient-component error: 0.00000533089.
- All 12 stitched images were pixel-identical to bytecode (MAE 0, SSIM 1).
- All six cold topologies populated an archive.
- All six numeric variants reused their topology key and loaded the archive.
- Cold construction was approximately 1.87-2.11 seconds; warm loads were
  approximately 2.19-2.82 ms.

Evidence: `reports/stitch-vocabulary/20260801-231816/summary.json`.

## Translation-only typed SoA baseline

The next direct baseline lowers translation-only hard unions into separate,
fixed-capacity sphere, box, and plane arrays. The distance loop therefore
avoids bytecode dispatch, transform matrices, per-primitive type tags, and the
AoS primitive switch. Numeric primitive data remains in `FptRenderConfig`, so
the representation has no shader-build or pipeline-build cost.

The lowering is deliberately narrow and explicit:

- cumulative translations are baked into primitive centres;
- spheres, boxes, and planes are stored in independent typed arrays;
- each entry retains its source instruction index so equal-distance winners
  remain identical after grouping primitives by type;
- planes retain the source normal and centre so normalization and offset
  arithmetic occur in the same GPU precision and order as the interpreter;
- nonlinear transforms and non-union CSG fail lowering rather than silently
  changing semantics.

An offline Metal differential kernel compared typed-SoA and interpreted
distance and analytic gradient at 1,048,576 deterministic points. It reported
zero failures, a maximum distance error of 0.000000715256, and a maximum
gradient-component error of 0.00000387430.

At 320x180, 64 spp, one bounce, and seven alternating-order runs:

| Scene | Source instructions | Bytecode | AoS flat union | Typed SoA | Stitched inline |
| --- | ---: | ---: | ---: | ---: | ---: |
| 8 spheres | 16 | 122.11 ms | 51.78 ms | 46.73 ms | 36.97 ms |
| 16 spheres | 32 | 346.76 ms | 132.08 ms | 83.27 ms | 57.49 ms |
| 24 spheres | 48 | 564.92 ms | 196.04 ms | 112.59 ms | 71.08 ms |
| 32 spheres | 64 | 738.36 ms | 272.96 ms | 150.94 ms | 828.13 ms |
| 12 spheres + 12 boxes | 48 | 519.57 ms | 197.37 ms | 100.98 ms | 321.96 ms |

Typed SoA was 2.61-5.15x faster than optimized bytecode and 1.11-1.95x
faster than the AoS flat-union evaluator. It is the strongest zero-build
direct evaluator in the tested domain. Stitched inline remained faster for
the 8-, 16-, and 24-sphere fixtures, but crossed a severe performance cliff at
64 source instructions and also lost on the mixed-type fixture. The cliff is
stable across the seven runs. Its coincidence with the expanded stitched
position/Jacobian/gradient state suggests code-size, register-pressure, or
spill costs, but GPU counter evidence is still needed before assigning a
cause.

All image comparisons retained SSIM and LF-SSIM 1. The maximum image MAE was
0.0000347222; at worst, two 8-bit levels changed on 0.00521% of pixels. This is
consistent with the bounded floating-point reordering measured by the
field-level validator, rather than a geometry or winner-selection mismatch.

This changes backend selection in two ways. Translation-only unions should use
typed SoA as their direct baseline, and stitching must demonstrate a measured
margin over typed SoA rather than over bytecode alone. Instruction count by
itself is not a safe stitching selector because the 48-instruction homogeneous
and mixed fixtures choose different winners, while the 64-instruction fixture
regresses below bytecode.

Evidence: `reports/typed-soa/20260802-095804/summary.json`.

## Measured backend selection

Selector v8 separates two identities that had previously been conflated:

```text
TopologyKey: device/compiler/source identity + optimized operation topology
WorkloadKey: TopologyKey + geometry parameters + material/path/camera/render state
```

Generated libraries and explicit preview pipelines use `TopologyKey`, so
numeric geometry and material edits reuse topology code. Backend decisions use
`WorkloadKey`, which includes material mode and parameters, bounce cap,
Russian-roulette controls, camera, lighting, resolution, samples, and the
selector policy constants. This fixes the version-7 bug where a material edit
could reuse a decision measured for a materially different path workload.

The selector compares three evaluators from the same globally optimized
geometry program:

```text
direct:                 typed SoA when lowering succeeds, otherwise bytecode
generated-distance:     topology-specialized distance + interpreted surface
generated-surface:      topology-specialized distance + analytic surface
```

Persistent selection performs one untimed warmup per backend, then three timed
runs in a three-way Latin rotation. A candidate within 5% of either decision
gate triggers four more rotated measurements. The probe preserves the requested
aspect ratio, path state, accumulation kernel, and up to 64 spp; its width is
capped at 1920 so GPU-occupancy crossovers seen at 1080p and 4K are represented.

Parity is measured on the linear `float4` accumulation buffer before tone
mapping and 8-bit quantization. A candidate must keep mean absolute error at or
below `1e-6`, and no more than `2e-5` of components may exceed `1e-3` absolute
error. Maximum error remains recorded as a diagnostic rather than allowing one
surface-boundary component to veto an otherwise numerically equivalent frame.
Independent field/gradient validators remain the semantic correctness proof.

A specialized candidate must use at least 15% less predicted end-to-end time
than direct, equivalent to a reported speedup of `1 / 0.85 = 1.176x` or more.
Generated analytic surfaces must additionally be at least 1.10x faster than the
best qualifying non-surface evaluator.

Function stitching is no longer an automatic candidate. It remains available
through explicit research flags, but including it added roughly 2.4 seconds of
cold discovery work at 24 primitives to distinguish backends whose render
times differed by only about 0.5%. Removing it also eliminates the known
32-primitive and mixed-vocabulary stitching cliffs from production selection.

Offline one-shot execution now has an explicit policy split. `--sdf-backend
auto` reuses a cached `WorkloadKey` decision and otherwise renders direct; it
does not spend seconds discovering a backend for a single short render.
`--sdf-backend probe` explicitly performs measurement and populates the cache.
The older `--sdf-function-stitching auto` spelling remains a probing research
alias. Interactive preview is persistent and therefore continues to probe on a
cache miss while direct rendering remains active.

The version-8 five-scene control selected a specialized backend in all cases,
used adaptive seven-run measurement in boundary cases, reused every cached
decision, and retained pixel-exact final PNGs. Linear parity maxima ranged from
`1.19e-7` to `7.90e-5` in that control.

The target-workload regret experiment also exposed two important selector
limits. A fixed-order, single-run matrix was invalid and has been replaced by
one warmup plus three rotated measured rounds. In the corrected 1080p,
bounce-four focus, the 960-wide intermediate selector had 10.8% mean decision
regret and 23.9% worst regret across nine topology/material conditions. Raising
the persistent probe to the target-representative 1920 cap recovered the clear
opaque union and mixed wins. An affine opaque boundary case measured a 1.90x
probe speedup and now qualifies with float MAE `4.55e-7` and outlier fraction
`1.04e-5`; a subsequent `auto` invocation reused the same decision from cache.
Material-heavy cases remain deliberately conservative where repeated probe
medians do not clear the 1.176x gate.

Evidence:

- `reports/backend-selection/20260802-170548/summary.json`
- `reports/generated-selector-matrix/20260802-selector-v8-fresh-controlled-focus/summary-v8-decision-regret.json`
- `reports/backend-selection/20260802-selector-v8-1920-probes/summary.json`
- `reports/backend-selection/20260802-selector-v8-policy-verify/summary.json`

## Lean stitched distance state

The cliff-isolation backend adds `--sdf-stitch-distance-only`. Its stitched
distance state contains only:

```text
position
distance scale
winning distance
has-primitive flag
```

It does not carry the three position-Jacobian rows or the winning analytic
gradient. Every transform, primitive, and CSG mode has a separate lean
stitchable function family. The stitched surface symbol is a one-node wrapper
around `programSurfaceInterpreted`, and the runtime surface flag is disabled,
so the full state chain is absent from the distance graph rather than merely
unused.

This creates three causal controls:

```text
full surface:    full distance state + stitched analytic accepted-hit surface
full distance:   full distance state + interpreted accepted-hit surface
lean distance:   lean distance state + interpreted accepted-hit surface
```

A 1,048,576-point differential validation on the expanded transform/CSG
fixture reported zero distance and gradient failures. Maximum distance error
was 0.000000476837 and maximum gradient error was zero.

At 320x180, 64 spp, one bounce, and seven alternating-order runs:

| Primitives | Instructions | Bytecode | Typed SoA | Full surface | Full distance | Lean distance |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 24 | 48 | 566.10 ms | 113.17 ms | 63.55 ms | 87.33 ms | 77.47 ms |
| 28 | 56 | 649.40 ms | 134.09 ms | 72.34 ms | 101.50 ms | 93.21 ms |
| 32 | 64 | 815.39 ms | 156.72 ms | 867.07 ms | 867.63 ms | 868.93 ms |

Below the cliff, the lean state improved the surface-off control by 1.13x at
24 primitives and 1.09x at 28. This confirms that carrying Jacobians and the
winning gradient has a measurable distance-loop cost. It does not justify
disabling stitched analytic surfaces in those cases: accepted-hit interpreted
surface work made lean distance 22-29% slower than the full-surface backend.

At 32 primitives the causal controls converge within 0.2%. Removing analytic
surface evaluation does not change the cliff, and removing the Jacobian and
gradient state does not change it either. Lean distance is 5.54x slower than
typed SoA and even 1.07x slower than optimized bytecode. Cold lean-library
construction also remains approximately 2.59-2.99 seconds, similar to the full
graph.

This refutes the working state-pressure explanation for the discontinuity.
The remaining evidence points to a compiler or function-stitching graph
boundary reached between 56 and 64 source instructions: generated-code size,
an internal node limit, inlining behaviour, or a private spill/code-placement
threshold. Public pipeline limits remain unchanged across all variants.

All render comparisons retained SSIM and LF-SSIM 1. Maximum image MAE was
0.00000578704.

Evidence: `reports/stitch-state/20260802-104201/summary.json`.

## Exact stitch threshold

The follow-up sweep brackets the discontinuity at exactly 32 translated-union
primitives, or 64 source instructions. It compares the typed-SoA baseline with
normal and always-inline function stitching using both the full and lean
distance states. Each result is the median of seven alternating-order runs at
320x180, 64 spp, and one bounce.

| Primitives | Instructions | Typed SoA | Full normal | Full inline | Lean normal | Lean inline |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 29 | 58 | 136.08 ms | 70.88 ms | 70.51 ms | 91.79 ms | 88.15 ms |
| 30 | 60 | 141.39 ms | 72.16 ms | 72.89 ms | 92.89 ms | 92.13 ms |
| 31 | 62 | 146.31 ms | 75.48 ms | 76.29 ms | 100.07 ms | 98.31 ms |
| 32 | 64 | 157.00 ms | 920.79 ms | 926.73 ms | 910.13 ms | 928.40 ms |

The 31-to-32 transition slows the full graph by 12.20x with normal stitching
and 12.15x with always-inline stitching. The lean graph slows by 9.09x and
9.44x respectively. Normal and always-inline performance remains within 2.1%
at every size, so forced inlining is not the cause. Full and lean state also
converge after the transition, confirming that the state payload is not the
cause. This isolates a Metal compiler or function-stitching graph boundary at
64 nodes/instructions for this graph family.

At 31 primitives, full stitching remains 1.92-1.94x faster than typed SoA. At
32 primitives it is 5.86-5.90x slower. The automatic selector therefore keeps
its measured static cutoff at 32 typed primitives; a boundary regression test
now proves that 31 remains eligible while 32 is rejected. All 16 stitched
comparisons were pixel-identical to typed SoA (MAE 0, SSIM and LF-SSIM 1).

Evidence: `reports/stitch-threshold/20260802-104918/summary.json`.

## Split stitched graphs

The next experiment tests whether the 64-instruction cliff belongs to an
individual stitching graph or to the linked function generated for the final
pipeline. `--sdf-stitch-split-graph` divides the lean state chain in half and
builds it in two stages:

```text
segment library: prefix(source, config) -> state
                 suffix(state, config) -> state

outer library:   finish(suffix(prefix(source, config), config)) -> distance
```

Each segment is compiled as an independent stitched graph. The outer graph has
only prefix, suffix, and finish calls. The prefix ends with an explicit
`__attribute__((noinline))` stitchable identity function, so the join is a real
optimizer barrier rather than merely the absence of `AlwaysInline`.

At 320x180, 64 spp, one bounce, and seven alternating-order runs:

| Primitives | Instructions | Typed SoA | Full inline | Lean inline | Split normal | Split inline |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 24 | 48 | 113.05 ms | 59.86 ms | 74.17 ms | 86.59 ms | 85.70 ms |
| 28 | 56 | 131.03 ms | 68.69 ms | 86.02 ms | 92.01 ms | 95.03 ms |
| 31 | 62 | 142.99 ms | 74.02 ms | 95.16 ms | 108.36 ms | 106.73 ms |
| 32 | 64 | 154.87 ms | 907.03 ms | 925.00 ms | 897.04 ms | 915.92 ms |

The split graph still slows by 8.28x with normal stitching and 8.58x with
always-inline stitching at the 31-to-32 transition. At 32 primitives it is
5.79-5.91x slower than typed SoA and statistically equivalent to the
monolithic lean graph. Below the cliff, the split boundary costs 10-16% versus
the monolithic lean graph and 28-44% versus the full stitched surface graph.

This refutes the individual-DAG-size explanation. Metal accepts generated
stitched functions as inputs to another stitched library and honors the
explicit call boundary, but the final linked pipeline still exhibits the same
64-instruction discontinuity. The remaining explanation is a compiler/codegen
threshold applied after linked code is combined, such as total generated code,
register allocation, or code placement. Splitting is therefore retained only
as an explicit research backend and is not added to automatic selection.

The 1,048,576-point transform/CSG differential validator reported zero
failures, maximum distance error 0.000000476837, and zero gradient error. Every
render comparison was pixel-identical to typed SoA.

Evidence: `reports/stitch-split/20260802-113013/summary.json`.

## Stitched superinstructions

The final stitching experiment reduces total stitched operation nodes while
preserving exactly the same 32 translated-sphere union computation. Three
full-surface superinstruction modes were added as explicit research controls:

```text
one-pair:     fuse one translate + sphere + union pair       64 -> 63 nodes
pairs:        fuse every translate + sphere + union pair     64 -> 32 nodes
double-pairs: fuse two translated sphere unions per function 64 -> 16 nodes
```

This differs from split graphs: fusion genuinely removes graph nodes and
linked call boundaries rather than partitioning the same fine-grained chain.

Seven-run alternating-order medians at 320x180, 64 spp, and one bounce:

| Primitives | Typed SoA | Fine nodes/time | One-pair nodes/time | Pair nodes/time | Double-pair nodes/time |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 31 | 144.69 ms | 62 / 81.65 ms | 61 / 86.28 ms | 31 / 86.00 ms | 17 / 87.76 ms |
| 32 | 147.85 ms | 64 / 796.92 ms | 63 / 777.94 ms | 32 / 795.56 ms | 16 / 789.82 ms |

Every fused representation remains in the catastrophic regime at 32
primitives. The 31-to-32 transition is 9.76x for fine operations, 9.02x after
fusing one pair, 9.25x with 32 fused nodes, and 9.00x with only 16 fused nodes.
At 32 primitives all stitched variants are 5.26-5.39x slower than typed SoA.

This rules out both a 63/64 stitched-node threshold and total stitching-node
count as the trigger. The cliff follows the 32nd primitive's combined semantic
work or the final code/resources generated for it. Stitched superinstructions
cannot extend the safe envelope on this toolchain and remain research-only.
There is no remaining renderer-level stitching restructuring experiment with
a credible path to avoiding the cliff.

The 1,048,576-point fused differential validator reported zero failures;
maximum distance error was 0.000000834465 and maximum gradient error was
0.00000424683. All rendered comparisons were pixel-identical to typed SoA.

Evidence: `reports/stitch-fusion/20260802-115542/summary.json`.

Matching Xcode 26.2 Metal System Trace captures are stored at
`reports/stitch-fusion/20260802-115542/captures/31.trace` and `32.trace`. The
headless template captured the command-buffer timelines, but reported no GPU
counter set and disabled shader-timeline sampling. The traces are therefore
ready for side-by-side inspection in Instruments/Xcode, but occupancy and
shader-cost counters require enabling those facilities interactively.

## Generated MSL with analytic surfaces

The next backend moved topology specialization beyond a distance-only source
generator. For each typed program it now emits two topology-specialized MSL
functions:

- a lean distance evaluator used by every sphere-marching step;
- an analytic `ProgramSurface` evaluator used only at accepted hits.

The surface evaluator carries the transformed point and its three Jacobian
rows through translate, scale, rotation, repeat, absolute-value, and sort
operations. Sphere, box, and plane leaves produce analytic gradients, while
hard union, intersection, and subtraction select the same strict winner and
gradient sign as the interpreted evaluator. Material parameters remain in the
configuration buffer, so material-only changes do not alter generated source.

An initial version exposed an important compiler interaction: merely retaining
typed-SoA, stitching, and interpreted branches in the runtime-compiled shader
made the 32-primitive generated-distance control regress from about 119 ms to
about 970 ms. The generated shader now defines its backend at compile time.
Its distance loop contains only the topology-specialized evaluator, and its
accepted-hit path contains either the generated or interpreted surface
evaluator selected when source is produced. This removed the apparent
32-primitive cliff; it was branch/resource contamination in the monolithic
shader, not a limit of generated MSL.

Five-run medians at 320x180, 64 spp, and one bounce were:

| Primitives | Source instructions | Typed SoA | Generated distance + interpreted surface | Generated distance + surface | Stitch inline | Surface vs SoA | Surface vs interpreted |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 16 | 32 | 72.38 ms | 36.10 ms | 45.27 ms | 53.49 ms | 1.60x | 0.80x |
| 24 | 48 | 100.87 ms | 51.11 ms | 53.68 ms | 60.92 ms | 1.88x | 0.95x |
| 31 | 62 | 129.64 ms | 65.21 ms | 58.96 ms | 77.96 ms | 2.20x | 1.11x |
| 32 | 64 | 131.99 ms | 70.99 ms | 61.86 ms | 744.72 ms | 2.13x | 1.15x |

Evidence: `reports/generated-surface/20260802-122029/summary.json`.

A separate seven-run 28-primitive point located the crossover more closely:
the interpreted-surface variant took 62.04 ms and the combined generated
variant 56.41 ms. The combined backend was therefore 1.10x faster than
generated distance alone, 2.18x faster than typed SoA, and 1.20x faster than
stitching. The crossover for this distributed-union fixture lies between 24
and 28 primitives. That is evidence for a cost/probe-based selector, not a
universal primitive-count rule.

Evidence: `reports/generated-surface/20260802-122459/summary.json`.

All image comparisons were exact (MAE 0, SSIM 1, LF-SSIM 1). The generated
surface validator evaluates 1,048,576 deterministic random and boundary
points against interpreted distance and analytic gradients. It reported zero
distance and gradient failures; the largest observed errors were below
0.000001 for distance and 0.000005 for gradient. This validator is also a
permanent Rust test.

The compiler cost remains meaningful. A new topology/source combination took
about 2.1-2.6 seconds to compile in the cold crossover run, while repeated
compilation through Metal's internal cache took about 5 ms. The backend should
therefore be cached by topology and shader/compiler identity, used while a
scene topology remains stable, and selected on amortized render work. The
`--no-sdf-generated-surface` control retains generated distance with the
interpreted accepted-hit surface for small or surface-heavy programs.

## Canonical geometry IR and direct evaluation

The next compiler milestone adds a canonical affine geometry representation.
Each supported sphere, box, or plane is stored as a primitive descriptor with
one composed 3x4 world-to-local transform, a conservative distance divisor,
primitive data, source location, and hard-CSG combine mode. Translate, uniform
scale, and axis rotations therefore disappear from the dynamic operation
stream. Hard unions and intersections are flattened, and identical leaves in
an associative chain are removed even when they were not adjacent in source.
Nonlinear repeat, absolute-fold, and coordinate-sort operations conservatively
keep the existing bytecode evaluator.

The shading program remains separate and complete. Orbit/material operations
are removed only from the distance program and are still evaluated after an
accepted hit. Material-only edits are therefore independent from canonical
geometry and generated-source cache identity.

A 12-condition affine sweep compared canonical direct evaluation with the
optimized bytecode evaluator at 1920x1080 and 3840x2160, one and eight
bounces, and opaque, translucent, and refractive materials. Canonical direct
evaluation won every condition:

| Result | Value |
| --- | ---: |
| Mean speedup | 2.16x |
| Minimum speedup | 1.57x |
| Maximum speedup | 3.56x |
| Winning conditions | 12 / 12 |

At representative extremes, 1920x1080 opaque one-bounce rendering fell from
75.06 ms to 21.09 ms (3.56x), while 3840x2160 opaque eight-bounce rendering
fell from 404.10 ms to 124.33 ms (3.25x). The three 1080p, eight-bounce
comparisons had maximum MAE 0.000317, minimum SSIM 0.999994, and LF-SSIM 1.
The small differences are strict-winner/normal precision at isolated
boundaries, not a geometric approximation.

Evidence: `reports/canonical-ir/20260802-142539/summary.json`.

## Generated MSL from canonical descriptors: negative result

The same IR was then used as input to a generated-MSL experiment. The source
generator unrolled primitive topology and CSG selection while leaving matrices
and primitive parameters in the configuration buffer. This preserves cheap
parameter updates, but the 80-byte descriptors made the generated function
much larger without eliminating its runtime data loads.

The sweep covered 72 conditions: 1920x1080 and 3840x2160; one, two, four, and
eight bounces; distributed sphere unions, mixed sphere/box unions, and affine
mixed primitives; and opaque, translucent, and refractive materials. Each
condition compared direct evaluation, generated distance with an interpreted
accepted-hit surface, and generated distance with analytic surface output.

| Result | Generated distance | Generated analytic surface |
| --- | ---: | ---: |
| Mean speedup versus direct | 0.350x | 0.354x |
| Best observed speedup | 0.653x | 0.666x |
| Worst observed speedup | 0.102x | 0.103x |
| Selector wins | 0 / 72 | 0 / 72 |

Even affine scenes, where canonical direct evaluation is strong, lost in the
generated form. The evidence points to register/instruction pressure and
repeated wide-descriptor loads, not to an invalid IR. Descriptor-unrolled MSL
is retained only behind `--sdf-canonical-topology-specialization` and is
deliberately excluded from automatic backend selection.

The 18 representative 1080p comparisons had maximum MAE 0.000130, minimum
SSIM 0.999998, and LF-SSIM 1. The maximum channel error occurred on an isolated
boundary pixel; aggregate parity remained effectively exact.

Evidence: `reports/generated-selector-matrix/20260802-141426/summary.json` and
`quality-summary.json` in the same directory.

## Compact canonical source generation

The next source generator tested whether the wide-descriptor failure came from
the canonical representation itself or from how it was lowered. The compact
mode keeps matrices and primitive parameters indirect in the configuration
buffer, but emits scoped, primitive-specific MSL:

- only the matrix rows and sphere/box/plane parameters required by that leaf
  are loaded;
- primitive opcode dispatch and whole `FptPrimitiveInstance` copies disappear;
- hard-CSG selection is static;
- each leaf is scoped so point, gradient, and box temporaries leave their live
  range before the next leaf.

This is exposed as `--sdf-compact-canonical-topology-specialization`. The
distance-only form initially retained the interpreted accepted-hit surface;
the follow-up hybrid instead uses the canonical loop's analytic surface at
accepted hits, avoiding both bytecode replay and a second large unrolled
function.

The main matrix covered 36 conditions: union, mixed, and affine topologies;
opaque, translucent, and refractive materials; 1920x1080 and 3840x2160; and
one and eight bounces. It compared direct evaluation with production generated
distance/surface, wide canonical distance/surface, and compact canonical
distance/surface.

| Backend | Mean speedup versus direct | Matrix wins |
| --- | ---: | ---: |
| Production generated distance | 1.56x | 8 / 36 |
| Production generated surface | 1.61x | 28 / 36 |
| Wide canonical distance | 0.37x | 0 / 36 |
| Wide canonical surface | 0.38x | 0 / 36 |
| Compact canonical distance | 0.62x | 0 / 36 |
| Compact canonical surface | 0.48x | 0 / 36 |

Compact lowering therefore recovered a substantial part of the wide format's
loss but did not challenge the production generator. Topology explains the
remaining gap. Compact distance averaged 1.12x over direct on affine scenes
and reached 1.74x in its best single run, but averaged only 0.39x on mixed
translation and 0.36x on distributed unions. The optimized-program generator
already expresses those transforms more compactly than twelve matrix
coefficients per primitive.

The analytic-surface hybrid improved compact affine performance to 1.18x on
average in a 12-condition follow-up and produced two single-run wins. Neither
was robust enough for selection. The strongest apparent result, 1.89x at
1920x1080 with one opaque bounce, became 1.09x over direct under seven-run
medians; production generated distance reached 1.19x on the same fixture. The
other compact win was only 1.04x. Both are below the project's 10-15% backend
margin once compared with the strongest candidate.

All 54 main-matrix comparisons remained effectively exact: maximum MAE
0.000292, minimum SSIM 0.999993, and LF-SSIM 1. The million-point canonical
distance/gradient validator also covers both wide and compact generated modes
and reports zero failures.

Metal's public pipeline properties did not expose the suspected resource
difference. Every generated variant reported SIMD width 32, maximum 384
threads per threadgroup, and zero static threadgroup memory; direct reported
448 maximum threads. Register count and spills remain unavailable in the
headless API, so the register-pressure explanation remains an inference from
timing and generated structure rather than a directly measured counter.

Evidence:

- `reports/compact-canonical-codegen/20260802-144439/summary.json`
- `reports/compact-canonical-codegen/20260802-145253/summary.json`
- `reports/compact-canonical-codegen/20260802-145439/summary.json`

The compact backend remains explicit research infrastructure and is not added
to automatic selection. The compact SSA/DAG direction is now closed in its
current matrix-per-leaf form. A future revisit would need a genuinely smaller
parameter representation—such as shared transform nodes or compact affine
indices—not another source-level spelling of the same twelve coefficients.

## Shared-transform DAG: representation crossover, not backend crossover

The next experiment implemented the suggested smaller parameter
representation. `--sdf-shared-transform-topology-specialization` groups
canonical leaves whose 3x4 transform and conservative distance divisor are
bitwise identical. Generated MSL computes one transformed point per unique
group, then reuses it for every sphere, box, or plane leaf in that group. Leaf
opcode and hard-CSG selection remain static, and combine order is unchanged.
Accepted-hit shading stays in the canonical analytic loop, so the experiment
isolates generated distance code rather than duplicating the large generated
surface function.

The controlled fixture held the geometry workload at 32 primitive leaves and
varied only transform reuse: 32, 16, 8, or 4 unique transforms, corresponding
to reuse factors 1, 2, 4, and 8. The matrix covered opaque, translucent, and
refractive materials; 1920x1080 and 3840x2160; one and eight bounces; five
backends; and three timing repetitions, for 720 renders and 48 median
conditions.

| Leaves per transform | Unique transforms | Shared-DAG speedup vs direct | Production program-distance speedup | Shared-DAG wins |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 32 | 0.35x | 0.60x | 0 / 12 |
| 2 | 16 | 0.50x | 1.77x | 0 / 12 |
| 4 | 8 | 1.18x | 1.74x | 0 / 12 |
| 8 | 4 | 1.71x | 1.79x | 1 / 12 |

This establishes a real arithmetic crossover: once four leaves share a
transform, hoisting the matrix evaluation becomes faster than canonical direct
evaluation; at eight leaves, it is a strong 1.71x accelerator. It also exposes
a generated-state cliff. Keeping 16 or 32 transformed points live is very
slow, and Metal's public pipeline metadata cannot show the presumed register
allocation or spills. All generated modes still report SIMD width 32, maximum
384 threads per threadgroup, and zero static threadgroup memory.

The important comparison is against the production generated-program backend.
That evaluator naturally applies a transform once and immediately consumes all
following leaves without retaining a table of transformed points. At reuse 8,
the shared DAG averaged 96.8% of production program-distance performance. Its
only initial win was the 1920x1080 opaque eight-bounce condition, where the
three-run median reported 18.16 ms against 24.05 ms. A seven-run confirmation
reversed it:

| Resolution | Direct | Production program distance | Shared-transform DAG | DAG relative to production |
| --- | ---: | ---: | ---: | ---: |
| 1920x1080 | 48.36 ms | 24.77 ms | 27.56 ms | 0.90x |
| 3840x2160 | 168.61 ms | 46.59 ms | 50.56 ms | 0.92x |

Production program distance therefore wins both confirmed conditions and
retains an 8-11% render-time advantage. The DAG is exact enough for the
renderer parity contract: the 48 main comparisons had maximum MAE 0.000141,
minimum SSIM 0.99998, and LF-SSIM 1. The seven-run confirmation tightened that
to maximum MAE 0.00000788, SSIM 1, and LF-SSIM 1. The topology validator now
also supports hybrid generated-distance/canonical-surface modes; the shared
DAG passes one million randomized field and gradient evaluations with zero
failures.

Evidence:

- `reports/shared-transform-dag/20260802-151739/summary.json`
- `reports/shared-transform-dag/20260802-151739/quality-summary.json`
- `reports/shared-transform-dag/20260802-153135/summary.json`
- `reports/shared-transform-dag/20260802-153135/quality-summary.json`
- `reports/shared-transform-dag/20260802-153135/shared-transform-reuse8-opaque-comparison.png`

The mode remains explicit research infrastructure and is excluded from
automatic selection. The useful result is that shared affine nodes can remove
substantial duplicate arithmetic, but a separately materialized point DAG is
not better than the existing sequential generated program on this Metal
compiler. Future compiler work should express transform sharing through
ordinary program dataflow and short live ranges, not a live array of affine
results.

## Compact affine indices and sequential transform runs

The follow-up implemented that short-live-range representation explicitly.
Canonical compilation now emits two additive tables alongside the existing
wide descriptors:

- a 64-byte `FptAffineTransform` containing one 3x4 matrix and conservative
  distance divisor;
- a 32-byte `FptIndexedPrimitive` containing primitive data, static opcode,
  source instruction, combine mode, and transform index.

`--sdf-affine-index-topology-specialization` lowers consecutive leaves with the
same transform index as one lexical run. It computes one transformed point,
immediately consumes every leaf in that run, and ends the scope before loading
the next transform. Unlike the previous DAG, it never keeps unrelated points
live. Unlike the production generated program, it applies a precomposed affine
matrix instead of replaying translate/rotate operations. Accepted-hit shading
continues through the canonical analytic surface evaluator.

The controlled matrix used 16 leaves, translation-only and translated-plus-
rotated topology classes, reuse factors 1, 2, 4, and 8, 1920x1080 and
3840x2160, one and eight bounces, five backends, and three timing runs. This
produced 480 renders and 32 median conditions. Sixteen leaves keep the rotated
unique-transform control within the 64-instruction source limit.

The indexed representation has an explicit memory crossover:

| Leaves per transform | Unique transforms | Wide descriptors | Indexed representation | Change |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 16 | 1,280 B | 1,536 B | +20% |
| 2 | 8 | 1,280 B | 1,024 B | -20% |
| 4 | 4 | 1,280 B | 768 B | -40% |
| 8 | 2 | 1,280 B | 640 B | -50% |

It also fixes the previous DAG's broad live-state cliff. At reuse 2-8, indexed
runs accelerated direct evaluation by roughly 1.52-1.98x across topology
groups. Unique-transform programs still suffered a severe generated-code
cliff: 0.27x for translations and 0.28x for rotations. Compact storage cannot
offset sixteen matrix evaluations with no reuse.

Across the complete matrix, production program distance averaged 2.04x over
direct, while affine-index runs averaged 1.43x and won 3 of 32 conditions.
The promising subset was rotated geometry:

| Reuse | Indexed speedup vs direct | Program-distance speedup | Indexed relative to program | Initial indexed wins |
| ---: | ---: | ---: | ---: | ---: |
| 2 | 1.75x | 1.60x | 1.08x | 1 / 4 |
| 4 | 1.52x | 2.07x | 0.76x | 0 / 4 |
| 8 | 1.81x | 2.02x | 0.93x | 1 / 4 |

A seven-run confirmation covered rotated reuse 2 and 8 at both resolutions and
bounce caps. Affine-index, shared-DAG, and production program distance were
effectively tied in aggregate: 1.743x, 1.743x, and 1.746x over direct,
respectively. Indexed runs won 3 of 8 individual conditions, but averaged only
95.7% of the strongest alternative at reuse 2 and 97.7% at reuse 8.

One confirmation row appeared selectable: rotated reuse 8 at 1920x1080 and one
bounce reported 14.95 ms for affine index, 20.15 ms for program distance, and
21.27 ms for the shared DAG. An independent seven-run material sweep did not
reproduce it. The opaque medians became 20.14 ms, 19.81 ms, and 19.43 ms,
respectively; translucent and refractive also selected program distance. The
repeat therefore rejects the apparent 26% advantage as a stable backend
crossover. The remaining confirmed indexed win at 4K/reuse 2/eight bounces was
only about 2% over the shared DAG and is below the 10-15% selector margin.

Parity remains effectively exact. The 32 main comparisons had maximum MAE
0.000646, minimum SSIM 0.999947, and LF-SSIM 1. The eight-condition seven-run
confirmation had maximum MAE 0.000277, minimum SSIM 0.999974, and LF-SSIM 1.
The indexed backend also passes the million-point distance/gradient validator
with zero failures.

Evidence:

- `reports/affine-index-runs/20260802-155440/summary.json`
- `reports/affine-index-runs/20260802-155440/quality-summary.json`
- `reports/affine-index-runs/20260802-155855/summary.json`
- `reports/affine-index-runs/20260802-155855/quality-summary.json`
- `reports/affine-index-runs/20260802-160038/summary.json`
- `reports/affine-index-runs/20260802-155855/affine-index-rotated-reuse8-comparison.png`

Compact affine indices are therefore a successful capacity representation and
a useful research backend, but not a stable production selector candidate on
this GPU/compiler. The sequential generated program remains the best general
form because it combines compact operation parameters, transform arithmetic
sharing, and short live ranges without matrix expansion.

## Ray-class profiling and batch execution

Offline `--sdf-profile` renders now record primary, secondary, and shadow
march steps separately, plus normal evaluations, accepted bounces, and sampled
pixels. Metadata also includes an explicitly labelled time estimate that
apportions measured render time by counted work units. These estimates are not
isolated GPU timers, but they make Amdahl effects visible on the same
deterministic path sample. Across the 72-condition matrix, primary marches
accounted for 52.4% of counted march steps on average; higher-bounce glass and
translucent cases shifted substantial work into secondary rays.

`render-batch` accepts a JSON array of ordinary render argument arrays and
keeps one Metal process alive. The exact-source/device runtime library cache
can consequently reuse generated libraries across resolution, bounce-cap, and
material changes. Workload scripts use this path so warm build times measure
the cache contract rather than process startup and repeated source compilation.

After isolating the failed descriptor generator, the production selector was
rerun with its compact optimized-program generator. It selected a specialized
backend in all five controls, with measured probe speedups from 1.39x to 2.29x,
pixel-exact automatic outputs, and successful persistent selection-cache reuse.
The canonical experiment therefore does not regress the shipping selector.

The final selector-version-7 rerun tightened that range to 1.82-2.24x and kept
MAE 0, SSIM 1, LF-SSIM 1, and complete cold/warm cache validation.

Evidence: `reports/backend-selection/20260802-143157/summary.json`.

## One-library distance/surface experiment

The first cold-JIT experiment compiled one full generated source containing
both the sequential distance function and analytic surface function. A Metal
function constant statically specialized the distance/interpreted-surface and
distance/generated-surface pipelines; there was no per-evaluation runtime
backend branch. This was intended to replace two source-library compilations
with one library plus two specialized entry pipelines.

It failed both acceptance criteria:

| Primitives | Single cold total | Dual cold total | Dual/single cold cost | Distance render ratio | Surface render ratio |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 16 | 1,844 ms | 3,758 ms | 2.04x | 0.94x | 1.15x |
| 32 | 1,494 ms | 3,863 ms | 2.59x | 9.08x | 12.03x |

The build interval includes source compilation and the specialized compute
pipeline, which is why the second function-constant variant still records
substantial cold work even when the source library can be reused. Warm lookup
cost was about 0.60-0.64 ms for the dual form versus 0.17-0.24 ms for the
single forms.

The 32-primitive result is the decisive failure: putting both semantic paths in
one source/library contaminates both statically specialized pipelines and
recreates a compiler cliff. Image parity remained exact (MAE 0, SSIM 1), so
this is purely a compiler/performance result. The full-source dual-library form
is therefore stopped and remains available only through
`--sdf-dual-generated-library` for toolchain regression testing.

This does not yet test the materially different tiny-helper arrangement. The
next cold-JIT experiment should compile only two monolithic generated helper
functions and link them across one boundary to a precompiled host kernel.

Evidence: `reports/dual-generated-library/20260802-173758/summary.json`.

## Tiny private-linked helper experiment

The follow-up compiled only the generated distance and analytic-surface
functions, then supplied those two `[[visible]]` functions as private linked
functions to the precompiled stitch-host kernel. The runtime unit contains the
Metal ABI declarations and evaluator bodies, but no renderer kernels,
integrator, materials, lighting, voxel code, or presentation code. It is
available only through `--sdf-tiny-linked-helper`.

The source-size half of the hypothesis was strongly confirmed. Across the
16/32-primitive controls, full generated sources were 324-359 KB while the
tiny sources were 14-49 KB. On a fresh mixed sphere/box topology, compiling the
tiny library itself took 14-55 ms versus 699-803 ms for the full source.

The complete cold-discovery result failed, however. Pipeline creation moved
the saved work into private-function linking and optimization:

| Primitives | Variant | Source | Source compile | Pipeline link | Cold total | Median render |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 16 | Full distance | 324 KB | 803 ms | 1,545 ms | 2,349 ms | 41.34 ms |
| 16 | Tiny distance | 14 KB | 14 ms | 2,583 ms | 2,597 ms | 157.67 ms |
| 16 | Full surface | 338 KB | 747 ms | 1,369 ms | 2,116 ms | 37.78 ms |
| 16 | Tiny surface | 28 KB | 29 ms | 2,711 ms | 2,740 ms | 187.89 ms |
| 32 | Full distance | 331 KB | 700 ms | 1,656 ms | 2,356 ms | 652.30 ms |
| 32 | Tiny distance | 21 KB | 20 ms | 3,237 ms | 3,257 ms | 395.02 ms |
| 32 | Full surface | 359 KB | 770 ms | 1,718 ms | 2,488 ms | 794.96 ms |
| 32 | Tiny surface | 49 KB | 55 ms | 3,563 ms | 3,618 ms | 424.40 ms |

Thus the tiny form reduced source compilation by roughly 14-57x, but total
cold discovery was still 1.11-1.45x slower. At 16 primitives the linked call
was 3.81-4.97x slower in steady-state rendering. The 32-primitive mixed control
reversed that result and made the tiny form 1.65-1.87x faster because the full
generated kernel crossed a separate whole-program compiler cliff. That
crossover does not rescue the backend: it is topology-specific, total cold
latency is still worse, and the 16-primitive control fails badly.

Parity remains effectively exact. The four image comparisons had maximum MAE
0.00000579 and minimum SSIM 1. A one-million-point distance/analytic-gradient
validation recorded zero failures, maximum distance error 7.15e-7, and maximum
gradient error 3.90e-6.

The key result is that source compilation and pipeline discovery are separate
costs. Shrinking the source solves the first, while a private evaluator call
inside every march step both increases final link optimization and removes the
whole-program optimization that makes the normal generated backend fast. This
backend is therefore frozen as explicit research infrastructure. Production
continues to use full generated MSL behind asynchronous fallback, persistent
caching, and the measured selector.

Evidence:

- `reports/tiny-linked-helper/20260802-175640/summary.json`
- `reports/tiny-linked-helper/20260802-175640/quality-summary.json`
- `reports/tiny-linked-helper/20260802-175640/comparisons/16-surface.comparison.png`
- `reports/tiny-linked-helper/20260802-175640/comparisons/32-surface.comparison.png`
- `reports/tiny-linked-helper/20260802-175422/summary.json`

## Reproducing the experiments

These commands are research tooling, not production startup paths. See
[`scripts/README.md`](../scripts/README.md) for classification and runtime
expectations.

```bash
scripts/run_runtime_source_control_experiment.sh
scripts/run_function_stitching_experiment.sh
scripts/run_stitch_cache_experiment.sh
scripts/run_stitched_surface_experiment.sh
scripts/run_stitch_differential_fuzz.sh
scripts/run_stitch_vocabulary_experiment.sh
scripts/run_typed_soa_experiment.sh
scripts/run_backend_selection_experiment.sh
scripts/run_stitch_state_experiment.sh
scripts/run_stitch_threshold_experiment.sh
scripts/run_stitch_split_experiment.sh
scripts/run_stitch_fusion_experiment.sh
scripts/run_generated_surface_experiment.sh
scripts/run_canonical_ir_experiment.sh
scripts/run_generated_selector_workload_matrix.sh
scripts/run_compact_canonical_codegen_experiment.sh
scripts/run_shared_transform_dag_experiment.sh
scripts/run_affine_index_experiment.sh
scripts/run_dual_generated_library_experiment.sh
scripts/run_tiny_linked_helper_experiment.sh
cargo test persistent_async_jit_renders_fallback_then_reuses_archive -- --nocapture
cargo test topology_generated_surface_matches_interpreted_field_and_gradient -- --nocapture
cargo test canonical_generated_surface_matches_interpreted_field_and_gradient -- --nocapture
```

Each script builds by default, records individual timings and quality reports,
and writes a timestamped `summary.json` below `reports/`.

## Next work

1. Keep full generated MSL as the selective exact backend and rely on its
   asynchronous direct fallback plus persistent topology/workload cache to hide
   cold discovery. Do not select either generated-library packing experiment.
2. Replace broad configuration-dependent generated parameters with a sequential
   native parameter ABI and measure whether it reduces warm pipeline overhead
   without recreating the failed wide-descriptor register cliff.
3. Extend interval-safe negative/non-uniform scale handling and canonical
   CSE/DCE beyond identical associative leaves.
4. Extend field fuzzing to negative and near-zero scales, explicit repeat seams,
   CSG ties, and one-ULP perturbations around transform and cell boundaries.
5. Keep stitching, directional fields, shared-transform DAG, and compact
   affine-index generation frozen as explicit research infrastructure on this
   Apple/Metal signature unless an independent workload demonstrates a robust
   margin.

The durable division is now clear: the voxel renderer retains direct sparse
construction and flat DDA, while exact typed procedural geometry uses compiler
specialization when its topology and render workload justify the build.
