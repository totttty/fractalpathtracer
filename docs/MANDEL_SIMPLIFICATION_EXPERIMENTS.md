# Mandel simplification experiments

This report tests seven ways to reduce the cost of path tracing imported
Mandelbulber scenes without changing the exact renderer's default behaviour.
Every approximation is opt-in. The globally optimized procedural evaluator is
the control.

## Experimental design

The screening cohort uses five structurally different scenes from the frozen
Mandel optimization corpus at 160x90 and 4 samples per pixel. A second pass
uses scenes 494, 627, and 637 at 160x90 and 16 samples per pixel. All renders
use deterministic seeds, batch accumulation, and tiled dispatch. Candidate
images are compared with the exact render using RGB MAE, RMSE, and SSIM.

The full machine-readable results and images are in:

- `reports/mandel-simplification/screening/report.json`
- `reports/mandel-simplification/confirmation-16spp/report.json`
- `reports/mandel-simplification/combined-637/`

The retained tuner is `scripts/mandel_iteration_sweep.py`. Rejected runtime
paths were removed after measurement; their frozen reports remain in
`reports/mandel-simplification/` and
`reports/mandel-iteration-shadow-sweep/`.

For example, this sweeps one corpus scene and accepts only candidates at or
above SSIM 0.98:

```bash
python3 scripts/mandel_iteration_sweep.py /path/to/mandelbulber2 \
  --indices 637 \
  --scales 0.75,0.80,0.85,0.90,1.0 \
  --width 854 --height 480 --samples 4 \
  --minimum-ssim 0.98
```

Very expensive Mandel kernels can exceed the macOS Metal watchdog with the
default 32-row tiled dispatch. `FPT_MANDEL_TILE_ROWS=4` (or, for the hardest
cases, `1`) trades command-buffer overhead for shorter individual dispatches.

## Results

### 1. Formula iteration LOD

The first implementation scales the scene's maximum formula iteration count.
It is the scene-wide control against which the later screen-space policy is
measured.

At a 0.75 scale, scenes 19 and 627 were pixel-exact but did not become faster.
Scene 494 changed by only MAE 0.025 but was 5% slower. Scene 637 was the useful
crossover: 21,275 ms became 15,995 ms, a 1.33x speedup, with SSIM 0.99690 and
MAE 0.270.

Conclusion: retain as a scene-selective candidate. Iteration reduction is not
safe or profitable as a global default, but some formulas contain iteration
headroom that can be removed with very little visual effect.

### 2. Formula-specific compiler lowering

The existing structural partial evaluator, loop scalarizer, dead-code
eliminator, and common-subexpression pass were enabled only for each scene's
formula IDs.

The 16-spp confirmation was pixel-exact on all three scenes, but runtime was
essentially unchanged: 1.02x, 1.00x, and 1.00x. Cold compile overhead ranged
from 0.96 to 4.38 seconds. In screening, scene 19 also exposed a non-exact
rewrite and a 2x slowdown.

Conclusion: the already accepted formula policies remain valuable, but the
more aggressive generic lowering is not a broad runtime optimization. New
compiler work should target a measured expensive formula and be parity-gated.

### 3. Cheaper normals / analytic-normal value bound

The tetrahedral normal path reduces the ordinary six distance evaluations to
four. This is not a production analytic derivative; it measures the maximum
value likely available before implementing formula-specific AD across the
large formula catalogue.

It produced no robust speedup (0.88x to 1.02x in confirmation) and materially
changed shading, with SSIM 0.399 to 0.785.

Conclusion: reject the generic four-sample normal. Full analytic/AD normals
should only be implemented for a formula where profiling shows normal
evaluation dominates and where the derivative can be proven to match the
distance estimator.

### 4. Two path bounces plus denoising

Capping paths at two bounces and applying the small edge-aware denoiser gave
the largest non-proxy render reduction: 1.74x to 2.45x. The result did not
preserve the reference lighting, however; confirmation SSIM was 0.790 to
0.863, with MAE 5.42 to 8.05.

Conclusion: useful as an explicit interactive/preview mode, not for a
visual-preserving production selector.

### 5. Capped shadow visibility

Shadow rays were capped at 48 procedural steps while primary and secondary
rays retained their exact limits.

| Scene | Exact ms | Candidate ms | Speedup | SSIM | MAE |
|---|---:|---:|---:|---:|---:|
| 494 | 291.85 | 274.43 | 1.06x | 0.98847 | 0.816 |
| 627 | 13,118.63 | 10,305.89 | 1.27x | 0.97683 | 1.107 |
| 637 | 21,275.09 | 15,179.15 | 1.40x | 0.99933 | 0.034 |

At 160x90, this looked like a real scene-dependent win: scene 637's occluders
resolved well before the original limit. Two stronger checks rejected it.
On related scene 101, cap 48 reached 1.99x but fell to SSIM 0.89231. At native
854x480, combining cap 48 with iteration scale 0.75 reached 2.08x but fell to
SSIM 0.94719. The cap's error was therefore both scene- and resolution-sensitive.

Conclusion: reject and remove the capped-shadow runtime path. A low-resolution
probe did not reliably predict native-resolution quality under the 0.98 gate.

### 6. Per-pixel adaptive sampling

The experimental batch kernel tracked luminance variance using Welford's method and could
stop after four samples when estimated standard error is below 3% of local
luminance.

At 16 spp it saved only 3.6% to 11.9%, while SSIM fell to 0.782 to 0.929. The
per-pixel stopping rule is biased toward early low-variance samples, and the
small sample count gives a weak variance estimate.

Conclusion: reject this rule. A future adaptive sampler needs a less biased
confidence test, sample-count instrumentation, spatial variance guidance, and
probably a higher minimum sample count. The experimental implementation was
removed during cleanup.

### 7. Cached spatial proxy

Mandel-generated Metal was temporarily connected to the sparse direct voxel builder. The
experiment used a 128-cubed sparse proxy with exact procedural material and
normal evaluation at voxel hits. This is a lower-bound experiment for a future
adaptive mesh or octree: it tests whether cached traversal can be fast before
building a more expensive reconstruction system.

Traversal was 16x to 59x faster in the confirmation scenes and proxy builds
took 23 to 44 ms. The geometry was nowhere near acceptable: SSIM was only
0.099 to 0.142 and MAE was 29 to 69. The fixed 128-cubed field discards the
fractal detail responsible for the reference silhouette.

Conclusion: reject the fixed-resolution proxy. Do not invest in an octree or
mesh backend until a narrow-band adaptive reconstruction can demonstrate high
similarity on one hard scene; traversal speed itself is not the problem. The
Mandel voxel connection was removed during cleanup.

## Combined result

Scene 637 is favourable to both iteration scaling and capped shadows. Combining
the two changed 21,275 ms to 11,504 ms, a 1.85x speedup, with SSIM 0.99626,
MAE 0.300, and RMSE 1.150.

The apparent combined win weakened in the 320x180, 16-spp validation:

| Candidate | Time | Speedup | SSIM | MAE |
|---|---:|---:|---:|---:|
| Exact | 54,511 ms | 1.00x | 1.00000 | 0.000 |
| Iteration scale 0.75 | 42,619 ms | 1.28x | 0.99495 | 0.388 |
| Shadow cap 48 | 40,485 ms | 1.35x | 0.98822 | 0.569 |
| Combined | 28,373 ms | 1.92x | 0.98353 | 0.895 |

Using the revised SSIM 0.98 acceptance gate, iteration scaling, shadow capping,
and their combination all qualify at this intermediate size. The combined
candidate is the fastest at 1.92x and SSIM 0.98353, but its small margin makes
the result insufficient for acceptance.

The decisive validation used the scene's native 854x480 resolution and four
deterministic samples per pixel. Four samples were used because the exact
20-spp render exceeded the 20-minute experiment budget. Mandel dispatches were
split into four-row command buffers to stay below the macOS Metal watchdog.

| Candidate | Time | Speedup | SSIM | MAE | Gate |
|---|---:|---:|---:|---:|---:|
| Exact | 298,309 ms | 1.00x | 1.00000 | 0.000 | pass |
| Iteration scale 0.75 | 217,013 ms | 1.37x | 0.98353 | 0.924 | pass |
| Iteration scale 0.80 | 237,963 ms | 1.25x | 0.98592 | 0.801 | pass |
| Scale 0.75 + shadow cap 48 | 143,214 ms | 2.08x | 0.94719 | 2.665 | fail |

The 0.75 iteration scale is the fastest candidate that clears the native
0.98 SSIM gate. The related scene 101 also benefited: scale 0.80 reached
1.26x at SSIM 0.99994, while scale 0.75 reached 1.26x with the same image
metrics. This supports iteration scaling as a formula- or scene-selective
control, although it is not safe as a universal default.

The comparison sheet is ordered exact, iteration scale 0.75, shadow cap 48,
then the combined candidate:

`reports/mandel-simplification/hex-grid-comparison.png`

The native comparison is ordered exact, iteration scale 0.75, iteration scale
0.80, then scale 0.75 plus shadow cap 48 and is stored at
`reports/mandel-iteration-shadow-sweep/scene-637-480p-4spp/comparison.png`.
Its machine-readable metrics are in the adjacent `result.json`.

## Remaining simplification experiments

The four items left open after the first seven-way sweep were implemented and
measured separately. Experimental implementations that failed the visual gate
were then removed; their renders and machine-readable results remain under
`reports/mandel-remaining-experiments/`.

### 8. Screen-space formula-iteration LOD

The retained implementation derives a ray-local iteration budget from the
same camera-distance threshold used by the Mandel marcher. Each doubling of
the projected pixel footprint removes `rate` formula iterations. It never
removes more than 25% of the configured schedule, which prevents short hybrid
programs from collapsing to one iteration. Delta-DE probes use the base
orbit's completed iteration count so their derivative samples remain
internally consistent.

The clean release build produced:

| Scene | Settings | Exact ms | LOD ms | Speedup | SSIM | MAE |
|---|---|---:|---:|---:|---:|---:|
| 637, hex grid 002 | 160x90, 8 spp, rate 1 | 68,359.85 | 59,674.33 | 1.15x | 0.99748 | 0.167 |
| 101, sphInv/hexGrid hybrid | 160x90, 8 spp, rate 1 | 41,699.89 | 32,870.97 | 1.27x | 0.99994 | 0.013 |

An earlier unconstrained sweep on scene 637 ranged from 1.49x at rate 0.5 to
2.87x at rate 4, with SSIM 0.99397 to 0.98652. At native 854x480, rate 1
reached 1.64x at SSIM 0.98191. Those figures motivated the policy but are not
the retained configuration: the final 75% floor intentionally gives up some
speed for a wider safety margin.

Conclusion: retain rate 1 as an opt-in, scene-selective candidate. It clears
the 0.98 gate on both a long 250-iteration formula and a short 14-iteration
hybrid schedule. It remains an approximation and therefore is not enabled by
default.

The final scene-637 metrics are in
`reports/mandel-remaining-experiments/screen-lod/scene-637-final-160x90-8spp/result.json`.
The adjacent `comparison.jpg` shows the exact and retained LOD renders.

### 9. Formula-specific analytic normal

An exact piecewise Jacobian was propagated through the built-in
Kaleidoscopic-IFS evaluator: absolute folds, plane reflections, rotation, and
uniform scale. This removed the finite-difference field samples for that one
formula family and deliberately fell back for generated formula programs,
repeat seams, and global rotations.

| Scene | Exact ms | Analytic ms | Speedup | SSIM | MAE |
|---|---:|---:|---:|---:|---:|
| 556, IFS31 | 127.62 | 128.42 | 0.99x | 0.72716 | 19.96 |
| 440, iter fog 007 | 199.35 | 189.20 | 1.05x | 0.64039 | 22.37 |

The local geometric normals looked plausible, but they did not match the
finite-difference normal contract closely enough at fold discontinuities.
Small normal changes also altered subsequent path decisions, producing large
image differences for negligible speedup.

Conclusion: reject and remove this formula-specific AD path. Analytic normals
are not justified unless a future per-formula implementation is parity-tested
at seams and profiling shows normal evaluation is a dominant cost.

### 10. Sparse two-level narrow-band proxy

The Mandel evaluator was temporarily connected to the direct sparse voxel
builder as a minimal adaptive proxy: a sparse 4-cubed-brick page table formed
the coarse level, occupied fine cells formed the narrow band, and empty bricks
could be skipped as a unit. The experiment swept 64-cubed and 128-cubed fields
and surface bands of one and four cells.

| Proxy | Build ms | Render ms | Speedup | Active cells | SSIM | MAE |
|---|---:|---:|---:|---:|---:|---:|
| 64 cubed, band 1 | 11.94 | 496.78 | 65.3x | 21,632 | 0.13637 | 34.67 |
| 128 cubed, band 1 | 38.41 | 369.82 | 79.5x | 88,736 | 0.13779 | 34.09 |
| 128 cubed, band 4 | 38.83 | 719.87 | 42.8x | 341,528 | 0.13229 | 34.94 |

The proxy traversed quickly, but neither more resolution nor a four-times
wider band repaired the silhouette. A trial coarse construction rejection was
worse: its point-based test rejected every 64-cubed brick. The dominant issue
is not leaf resolution alone. Finite proxy bounds omit repeated and distant
procedural geometry, while an unsigned iterative DE cannot conservatively
prove an entire coarse cell empty from a handful of samples.

Conclusion: reject the sparse proxy and remove its Mandel backend connection.
An adaptive octree is not useful until construction has a conservative
no-false-negative classifier and a camera-visible domain policy for repeats.

### 11. Hybrid proxy traversal plus exact hit refinement

Occupied proxy leaves were then treated only as candidate ray intervals.
Within each interval the original Mandel distance estimator and screen-space
threshold performed a clipped procedural march; a candidate miss resumed DDA.
Accepted hits used the normal procedural shading path.

This achieved the same 42.8x to 79.5x render speedups shown above, but SSIM
remained only 0.132 to 0.138. Exact leaf refinement can correct a false
positive occupied cell or improve hit depth; it cannot recover a surface when
the proxy never emits a candidate interval. The experiment therefore isolates
construction coverage, not refinement quality, as the blocking problem.

Conclusion: reject and remove the hybrid path. Revisit it only after a proxy
builder demonstrates near-reference silhouette coverage by itself. The frozen
results are in
`reports/mandel-remaining-experiments/hybrid-proxy/scene-637-sweep.json`.
`reports/mandel-remaining-experiments/hybrid-proxy/comparison.jpg` makes the
missing candidate coverage visible across the three proxy configurations.

## New controls

```text
--mandel-iteration-scale 0.125..1
--mandel-screen-lod-rate 0..8
```

The default scale is 1.0 and the default screen LOD rate is 0. Both preserve
the exact direct SDF renderer. The approximation controls are intentionally
opt-in; changing either can change geometry.

To sweep the retained controls under an SSIM gate:

```bash
python3 scripts/mandel_iteration_sweep.py /path/to/mandelbulber2 \
  --indices 637 --scales 0.75,1.0 --screen-lod-rates 0.5,1,2 \
  --width 854 --height 480 --samples 4 --minimum-ssim 0.98
```

## Recommendation

Retain scene-wide iteration scaling and screen-space iteration LOD as the two
visual-preserving, opt-in approximations from this round. Validate either at
the intended output resolution under the SSIM 0.98 gate. Keep two-bounce
rendering as an explicitly lower-quality preview mode. Freeze capped shadows,
generic and formula-specific analytic normals, adaptive sampling, broad
formula lowering, sparse/octree proxies, and hybrid proxy refinement. A
production selector should be formula-aware and should not trust a
low-resolution probe alone when silhouette detail changes with output
resolution.
