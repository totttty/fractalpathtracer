# Mandelbulber optimization report

This document records the exactness-first performance investigation for the
generated Mandelbulber renderer, including the ideas that shipped and the ones
removed during branch cleanup. Measurements were made on an Apple M1 Max. The
frozen corpus used 746 valid scenes at 120×68 and one sample per pixel; 743
produced comparable path-traced images. Unless stated otherwise, an
optimization was accepted only when its output PNG was pixel-identical.

The enduring result is that formula-specific compiler work outperformed new
spatial traversal structures for difficult procedural fractals. The retained
renderer still evaluates Mandelbulber's distance estimators and shades the
exact procedural hit. It does not replace them with cached geometry.

## What is retained

| Optimization | Status | Result |
| --- | --- | --- |
| Mandel-specific render kernel | Default | Removes generic renderer branches without changing formula operations |
| Representable-position fixed-point stop | Default | 1.116× median incremental corpus speedup; exact |
| Selected homogeneous hybrid schedules | Content-selected | 21 scene hashes; latest ten add 1.995–3.417× native speedups; exact |
| Selected short-period hybrid schedules | Content-selected | Table-index rewrites plus one exact 1.206× native two-phase unroll |
| Scene-bound formula partial evaluation | Three content hashes | 1.415–1.673× in accepted diffuse/path gates; exact |
| Generic and `-O0` safety policies | Content-selected | Preserves numerically sensitive scenes |
| Watchdog retry as 32-row tiles | Automatic fallback | Lets exceptionally slow scenes complete without changing global coordinates or samples |
| Persistent generated-pipeline cache | Default | Avoids repeating multi-second runtime compilation for unchanged generated source |
| Adaptive spatial preview and exact interlace settlement | Opt-in interactive mode | Up to 42.13× moving-preview speedup in the native-resolution test; exact when settled |
| Deduplicated voxel template bricks | Optional capacity mode | 19.51× median resident-memory reduction; exact, not generally faster |

## Production corpus result

The first specialization pass established this exact baseline:

| Metric | Frozen baseline | Specialized renderer | Change |
| --- | ---: | ---: | ---: |
| Successful scenes | 743 / 746 | 743 / 746 | no losses |
| Pixel-exact comparable images | — | 743 / 743 | zero changed pixels |
| Median GPU time | 35.934 ms | 27.346 ms | 1.31× faster |
| Mean GPU time | 141.637 ms | 129.715 ms | 1.09× faster |
| P90 GPU time | 222.016 ms | 171.573 ms | 1.29× faster |
| P95 GPU time | 481.233 ms | 392.256 ms | 1.23× faster |
| Maximum GPU time | 8,994.424 ms | 8,616.995 ms | 1.04× faster |

Adding the fixed-point stop produced the final full-corpus milestone:

| Metric | Previous | Fixed-point stop | Incremental change |
| --- | ---: | ---: | ---: |
| Successful scenes | 743 / 746 | 743 / 746 | no losses |
| Pixel-exact comparable images | — | 743 / 743 | zero changed pixels |
| Median GPU time | 27.346 ms | 23.756 ms | 1.15× faster |
| Mean GPU time | 129.715 ms | 93.205 ms | 1.39× faster |
| P90 GPU time | 171.573 ms | 137.193 ms | 1.25× faster |
| P95 GPU time | 392.256 ms | 265.597 ms | 1.48× faster |
| Maximum GPU time | 8,616.995 ms | 8,495.528 ms | 1.01× faster |
| Watchdog-tiled scenes | 7 | 3 | four fewer |

The stop is safe because a computed fp32 step that cannot change the current
ray position can only cause the next iteration to sample the same field point.
It also applies during boundary refinement. `MbulbAbsPow2_002` was the extreme
case: distance evaluations fell from 12,737,797 to 170,195, formula iterations
from 376,882,208 to 4,403,563, and GPU time from 6,917.647 ms to 532.901 ms
with an identical PNG.

## Accepted compiler specializations

### Mandel-specific renderer kernel

Generated scenes compile a kernel specialized around the procedural program:

- direct Mandel distance and material evaluation;
- direct camera and projection setup;
- Mandel march limits, thresholds, stepping, and refinement;
- the known opaque-surface path instead of generic translucency checks.

The formula body and its floating-point operation order remain unchanged.
Eight sensitive fixtures retain the generic renderer kernel, and five retain
`-O0`; both decisions are keyed by scene-content hash rather than filename.

### Homogeneous and periodic hybrid schedules

A blanket direct lowering of all 92 single-formula hybrids was faster but
changed 22 images under optimized Metal. Production therefore selects only
content hashes that passed exact gates. The original ten-scene cohort had a
2.520× median speedup and saved 2,916 ms in aggregate. On `MbulbAbsPow2_002`, direct lowering
reduced the fixed-point result from 532.901 ms to 180.863 ms—about 38.3× faster
than the original 6,917.647 ms result.

The first profile-guided follow-up added `pseudoKleinianMod4 rec` (formula 217).
Its direct one-formula loop reduced the 1800×1200 watchdog-safe render from
131,318.105 ms to 45,496.258 ms, a 2.886× speedup, with zero changed pixels.

A refreshed 92-scene audit then compared the current production compiler with
a forced-direct compiler at 120×68, 1 spp. Promising unselected scenes had to
remain byte-identical in three alternating production/direct pairs and retain
at least a 1.25× median speedup. The ten candidates with the greatest screen
GPU-time savings were confirmed at their native scene resolutions using
four-row watchdog-safe dispatch. All ten were pixel-identical and exceeded the
1.10× native acceptance gate:

| Scene | Production | Direct loop | Speedup | Changed pixels |
| --- | ---: | ---: | ---: | ---: |
| `Jos Leys Kleinian v3` | 33,118.845 ms | 13,158.115 ms | 2.517× | 0 |
| `MengerV4` | 11,648.840 ms | 4,949.039 ms | 2.354× | 0 |
| `aboxMod13Surf` | 17,327.239 ms | 6,128.406 ms | 2.827× | 0 |
| `asurf4_sphere_invert` | 24,851.279 ms | 7,898.991 ms | 3.146× | 0 |
| `asurf4_worms` | 16,373.586 ms | 4,791.719 ms | 3.417× | 0 |
| `mandelbulb_pupuku pow2` | 25,930.935 ms | 8,824.679 ms | 2.938× | 0 |
| `mandelbulb_pupuku pow6` | 17,475.996 ms | 7,370.413 ms | 2.371× | 0 |
| `pseudoKleinianMod5` | 17,228.411 ms | 8,634.729 ms | 1.995× | 0 |
| `transf_juliaBoxV2` | 16,527.236 ms | 5,540.056 ms | 2.983× | 0 |
| `newtonPow3-delta-gnj-002h` | 42,947.222 ms | 17,709.999 ms | 2.425× | 0 |

The complete repeated and native result is generated by
`scripts/mandel_exact_dispatch_gate.py`. Single-pass exactness is only a
prefilter: five of the 31 repeated candidates fell below the timing gate even
though their images remained exact.

The rebuilt production binary then rendered all 746 generated scenes at
120×68, 1 spp. It retained the established 743 successes and the same three
known blank/single-colour failures. A focused 92-scene homogeneous pass
confirmed that all 21 selected hashes reported `direct-homogeneous` and were
byte-identical to the forced-direct reference. Comparison with the older
exact-compiler corpus found 728/743 byte-identical images; all 15 differences
were outside the ten newly selected hashes, so none was introduced by this
dispatch-table expansion.

Four difficult mixed hybrids retain a narrower optimization. Their sequence
tables repeat every two or three entries, so only the table read is replaced
with an equivalent phase expression. Formula calls, iteration order, bailout,
and colour state remain unchanged.

| Scene | Before | Retained | Speedup | Changed pixels |
| --- | ---: | ---: | ---: | ---: |
| `DIFS Torus asurf.fract` | 3,829.515 ms | 2,290.224 ms | 1.672× | 0 |
| `abox_mod1_add.fract` | 2,539.069 ms | 1,986.394 ms | 1.278× | 0 |
| `pseudo kleinian abox13.fract` | 2,132.605 ms | 1,765.164 ms | 1.208× | 0 |
| `pseudoKleinianMod4.fract` | 2,181.405 ms | 2,286.254 ms | 0.954× | 0 |
| `hybrid001.fract` | 1,024.315 ms | 761.164 ms | 1.346× | 0 |

`pseudoKleinianMod4` is the unchanged control: it has no hybrid sequence table,
so its timing difference is run-to-run variance.

The later formula-cost sweep revisited formulas 606, 127, and 60 in scenes 19,
119, and 101. Scene-bound structural binding of formula 606 was rejected: it
was 1.076× faster but changed 1,486 of 8,160 pixels. Formulas 127 and 60 stayed
pixel-exact under structural binding but measured between 0.994× and 1.000×,
so their apparent profile cost was not reducible by the current source passes.
Phase-specialized variants were also rejected because they were slower or
non-exact.

Scene 19's exact opportunity was instead its alternating `[0, 1]` formula
schedule. Lowering two iterations at a time removes the sequence lookup and
formula switch while retaining the original formula calls, weights, bailout
tests, iteration values, and floating-point operation order. Three 120×68
runs were pixel-identical and measured 1.174× in path tracing and 1.178× in
diffuse-normal. At native 1920×1080, 1 spp, four-row tiled dispatch produced:

| Schedule | GPU ms runs | Median |
| --- | --- | ---: |
| Periodic index + switch | 244,552.630; 245,088.385; 239,491.470 | 244,552.630 ms |
| Two-phase unrolled | 202,723.528; 202,787.154; 202,811.946 | 202,787.154 ms |

All three native image pairs were byte-identical. The retained native speedup
is 1.206×, and the scene-content policy reports `unrolled-periodic-mixed` in
render metadata.

### Scene-bound formula compiler

The compiler binds scene-constant boolean, integer, and enum parameters before
OpenCL-to-Metal translation. It can select constant branches and switches,
scalarize short fixed loops, remove pure dead locals, and eliminate adjacent
immutable duplicate initializers. Surviving floating-point expressions retain
their token order. If no rewrite applies, the original source bytes are
returned, avoiding an otherwise gratuitous Metal code-generation change.

Three exact scene-content policies are enabled:

| Scene / formula | Diffuse baseline | Diffuse retained | Speedup | Path baseline | Path retained | Speedup |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `DIFS Torus asurf` / 132 | 1,767.115 ms | 1,245.945 ms | 1.418× | 1,814.421 ms | 1,282.371 ms | 1.415× |
| `DIFS Box DiagV3 complex primitive` / 602 | 103.858 ms | 66.117 ms | 1.571× | 105.162 ms | 62.850 ms | 1.673× |
| `ifs_xy` / 150 | 127.671 ms | 87.785 ms | 1.454× | 127.543 ms | 87.256 ms | 1.462× |

Five repeated renders in both workloads were pixel-identical for formulas 602
and 150. Formula 132 was also exact in repeated diffuse and path gates. The
full source-policy audit covered 747 examples: 746 generated, the one known
invalid fixture failed both paths, exactly three selected sources changed, and
there were no unexpected changes or missed selections.

One important correctness fix came from rejected formula 64. The expression
evaluator had treated two different unknown symbol names as proof of
inequality. Unknown runtime-symbol equality is now always unknown, and a
regression test protects the rule. Formula 64 became exact after the fix but
measured only 1.001×, so the fix remains while the policy does not.

### Adaptive interactive preview

The retained preview samples a regular pixel lattice while the camera moves
and replicates each result through a 2×2, 4×4, 8×8, or 16×16 block. Once the
camera stops, the remaining lattice offsets are rendered at full procedural
quality into the same accumulation buffer. After all offsets are visited, the
buffer is bit-identical to an ordinary exact preview.

For `DIFS Torus asurf` at 960×540, exact preview took 1,283.51 ms:

| Pixel stride | GPU time | Moving-preview speedup |
| ---: | ---: | ---: |
| 2 | 370.34 ms | 3.47× |
| 4 | 135.65 ms | 9.46× |
| 8 | 37.17 ms | 34.53× |
| 16 | 30.47 ms | 42.13× |

Four stride-2 settlement passes took 1,408.48 ms in total, or 1.10 exact
frames, with no mismatched floats. Enable the controller with
`FPT_MANDEL_INTERACTIVE_REFINEMENT=1`; offline rendering is unaffected.

### Voxel template-brick capacity mode

The successful voxel-side experiment deduplicates each 4³ brick's occupancy
into a 64-bit template and evaluates material procedurally only at the accepted
hit. Across seven README scenes every image was bit-identical. Median resident
memory fell 19.51×, while median render speedup was 0.991× and median build
ratio was 1.048×. This is retained as an explicit capacity option, not a speed
default.

## Rejected or neutral experiments

Rejected implementations and their command-line/environment switches were
removed from the branch. The results remain here so they are not repeated.

| Experiment | Result | Decision |
| --- | --- | --- |
| Procedural bounding sphere | 1.055× median in the narrow diffuse gate, but 0.877× in path tracing and only 1/5 path images exact | Removed |
| Sparse voxel occupancy pyramid | Fast first version changed 6,739 Glass hit-mask pixels; exact version had 0.889× median render ratio | Removed |
| Quaternion reference-orbit certification | fp64 model was sound; conservative fp32 accepted no useful blocks at the render-quality error budget | Removed |
| Cached/precomputed SDF, voxel and NAADF substitutes | Extra builds/storage and altered procedural detail did not beat exact direct evaluation on difficult fractals | Reverted |
| Optimized Metal plus `noinline` on Torus | ~178 ms diffuse, but path image MAE 3.906 and SSIM 0.954 | Removed |
| Stable-suffix hybrid lowering | About 1.103× in its favourable test but path output was not exact | Removed |
| Bailout batching | Neutral where provable; unsafe or visually changed elsewhere | Removed |
| Precise optimized math | Torus MAE 22.057 and luminance SSIM 0.350 versus exact `-O0` | Removed |
| Four-sample tetrahedral normals | 1.27× faster, but MAE 11.174 and SSIM 0.557 | Removed |
| Unreachable helper/source pruning | Compile source shrank 319,140→151,010 bytes, but render slowed 155.663→173.482 ms | Removed |
| Dedicated diffuse kernel | Compiled difficult formulas, but one gate changed 149 pixels | Removed |
| Lean distance-only orbit state | Pixel-exact but no reliable warm-render improvement | Removed |
| Orbit-state member reordering | Approximately 1.002× | Removed |
| Resumable march commands | Could cap command duration, but scene-dependent total overhead was 13–47% at native resolution | Removed |
| Active-ray queue compaction | Exact, but median 1.026× slower than dense resumption | Removed |
| Compile-time resume budgets | Exact but neutral: native ratios 0.972–1.061× | Removed |
| General formula partial evaluation | Mostly neutral under optimized Metal; best follow-up was 1.089×, below the 1.10× gate | Experimental autotuner only |
| Formula 1641 specialization | Changed one pixel | Not selected |

The directional and range-field accelerator work reached the same broad
conclusion: reducing procedural evaluation count through extra field lookups
did not repay lookup, traversal, and certification cost on Apple Silicon. The
direct evaluator should first receive compiler improvements; spatial
acceleration is justified only when it independently beats that stronger
baseline and preserves exact output.

## Benchmark methodology

Two workloads separate first-hit geometry from the complete renderer:

- `pathtrace`: one-sample path tracing, including materials, shadows, secondary
  rays, fog, and post-processing;
- `diffuse-normal`: one primary march plus a six-sample central-difference
  normal and fixed Lambert term, excluding materials and later path stages.

Among 723 scenes successful in both workloads, median diffuse time was 12.913
ms versus 23.455 ms path-traced. The median diffuse/path fraction was 0.568 and
log GPU times had 0.883 Pearson correlation. This confirmed procedural
geometry as a major bottleneck while also exposing scenes whose secondary rays
or shading dominated.

Every accepted source transformation was checked in repeated, alternating
baseline/candidate runs. Timing alone was never sufficient: the PNG comparison
had to report zero changed pixels. Content-hash selection prevents a renamed or
unrelated scene from inheriting a policy.

## Reproducing the retained gates

Build and audit the Mandelbulber source corpus:

```sh
cargo build --release

target/release/fpt-metal mandel-scene-audit /path/to/mandelbulber2 \
  --report reports/mandel-benchmark/full-corpus-scene-audit.json

target/release/fpt-metal mandel-formula-policy-audit /path/to/mandelbulber2 \
  --report reports/mandel-optimization/formula-policy-audit.json
```

Run matching path and geometry-only corpus sweeps:

```sh
python3 scripts/mandel_corpus_benchmark.py /path/to/mandelbulber2 \
  --binary target/release/fpt-metal \
  --audit reports/mandel-benchmark/full-corpus-scene-audit.json \
  --out reports/mandel-optimization/path-corpus \
  --width 120 --height 68 --samples 1

python3 scripts/mandel_diffuse_corpus_benchmark.py /path/to/mandelbulber2 \
  --binary target/release/fpt-metal \
  --audit reports/mandel-benchmark/full-corpus-scene-audit.json \
  --out reports/mandel-optimization/diffuse-corpus \
  --width 120 --height 68
```

Re-run the exact-image formula autotuner for a selected cohort:

```sh
python3 scripts/mandel_formula_optimizer_experiment.py /path/to/mandelbulber2 \
  --binary target/release/fpt-metal \
  --cohort reports/mandel-optimization/cohort.json \
  --indices 19 --runs 5 --workload diffuse-normal \
  --out reports/mandel-optimization/formula-autotune
```

The architecture and formula-coverage details are documented separately in
[`mandelbulber-compatibility.md`](mandelbulber-compatibility.md).

## Final interpretation

The branch now has a narrower architecture:

1. Generate exact Mandelbulber-compatible formula code.
2. Preserve sensitive scenes with generic-kernel or `-O0` content policies.
3. Apply exact global marcher and renderer specialization.
4. Apply scene-bound formula partial evaluation only where repeated image and
   timing gates demonstrate a material win.
5. Use watchdog tiling only as recovery and spatial interlacing only for
   interactive preview.
6. Keep voxel template bricks as an explicit memory-capacity mode, separate
   from exact procedural rendering.

The key lesson is that the difficult fractals were limited by expensive
formula arithmetic, not by a missing hierarchy. Exact, content-selected
partial evaluation reduces that arithmetic directly and is the optimization
direction worth extending.
