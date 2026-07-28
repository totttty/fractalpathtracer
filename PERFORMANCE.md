# Performance

## July 2026 SDF optimization

The three featured README scenes were benchmarked on an Apple M1 Max at each
scene's native resolution with 16 samples. Four runs were alternated between a
preserved baseline binary and the candidate; the table reports median Metal
render wall time around the Metal command buffers.

| Scene | Baseline | Optimized | Speedup |
| --- | ---: | ---: | ---: |
| Render005 (Cage) | 2821.8 ms | 1566.7 ms | 1.80x |
| Render0ad03 (Tower) | 756.0 ms | 736.4 ms | 1.03x |
| Glass (Cage) | 2770.4 ms | 1407.5 ms | 1.97x |
| Aggregate | 6348.2 ms | 3710.5 ms | 1.71x |

Two scene-independent changes produced the improvement:

- A distance-only SDF evaluator is used while marching and estimating normals.
  Full orbit colour and material evaluation now occurs only at a confirmed hit.
- Automatic focus distance is resolved once in a one-thread Metal prepass,
  rather than remarching the centre ray for every pixel sample.

The focus prepass is image-identical. Removing unused material work can change
floating-point instruction scheduling in chaotic distance estimators. At the
featured scenes' committed sample counts, Tower remained byte-identical and
the Cage renders passed strict comparison with MAE 2.73/2.80 and luminance SSIM
0.953/0.962. All nine upstream Beauty comparisons pass their strict gates.

## Bottlenecks

The diagnostic kernels count normalized ray workload; they are indicators, not
independent GPU timer measurements. Across the featured scenes, secondary
bounces dominate, followed by six-sample central normal estimation. Primary and
shadow marches are smaller contributors:

| Scene | Bounces | Normals | Primary march | Shadow march |
| --- | ---: | ---: | ---: | ---: |
| Render0ad03 | 0.851 | 0.424 | 0.082 | 0.044 |
| Render005 | 0.640 | 0.319 | 0.193 | 0.092 |
| Glass | 0.617 | 0.307 | 0.172 | 0.070 |

Tower gains less from distance-only evaluation, which indicates that its
remaining cost is dominated by repeated distance evaluation across bounces and
normal samples rather than material construction. The next plausible changes
are tetrahedral four-sample normals and bounce termination/Russian roulette.
Both deliberately alter sampling or surface reconstruction, so they remain
opt-in experiments until they meet the same image gates.

Both paths were measured on the featured scenes. Tetrahedral normals were
`7-14%` slower because the changed directions caused more downstream march
work, and reduced luminance SSIM as low as 0.514. Russian roulette from bounce
three saved roughly `9-12%`, but Tower's low-frequency SSIM fell to 0.814.
Neither experiment is enabled by default.

## Generated-program optimization

Typed SDF programs now propagate an analytic gradient through their generic
instruction stream. Transforms, repetition, sorting folds, sphere/box/plane
primitives, union/intersection/subtraction, and scale correction are supported.
This replaces six finite-difference distance evaluations with one derivative
evaluation at each surface hit.

On the bundled transformed box/subtractive-sphere benchmark, five alternating
runs at `512x384` and 16 samples improved median render time from 109.3 ms to
84.9 ms (`1.29x`). MAE was 0.025 and luminance SSIM was 0.9986. A simple sphere
fixture improved from 126.0 ms to 69.0 ms (`1.83x`) with MAE 0.0013. Automatic
selection only applies to typed programs; fixed Beauty shaders retain central
finite differences.

Moving material reconstruction behind the terminal-miss test was byte-identical
on all featured renders and provided a smaller aggregate improvement. Generated
programs also skip per-step translucency material checks when their declared
material is opaque.

Two additional generic experiments were rejected. Explicit camera/sun invariant
structures regressed aggregate performance by 0.6%, likely through register
pressure. Function-constant bounce/march specialization improved Tower by about
4% but regressed the complete featured set by 1.5%.

## Reproducing

Preserve the old release binary before changing the shader, then run:

```sh
BASELINE_BIN=/tmp/fpt-metal-baseline \
  RUNS=5 SAMPLES=16 \
  scripts/run_readme_engine_benchmark.sh
```

For workload maps and ranked bottlenecks:

```sh
OUT_DIR=reports/readme-path-cost \
  SCENE_SET=readme SIZE_W=320 SIZE_H=180 SAMPLES=4 \
  scripts/run_sdf_path_cost_diagnostics.sh
```

For an Instruments trace containing Metal command scheduling and GPU activity:

```sh
scripts/run_metal_system_trace.sh
```
