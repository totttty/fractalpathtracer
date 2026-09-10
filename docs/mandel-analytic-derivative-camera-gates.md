# Analytic derivative camera and beauty gates

Follow-up to [identical-point diagnosis](mandel-identical-point-stencils.md).
The derivative improvement is real, but the full-field candidate is **not
approved for production**: the larger camera sweep reveals new visible misses.
Production shaders, path-tracing defaults, NAADF and voxel export are unchanged.
Nothing was committed or pushed.

## Construction

- Scene 42: original `riemann bulb msltoe mod2 001.fract`.
- Three cameras: original, lateral and forward translations. Camera and target
  move together, preserving orientation, target distance and roll. Translation
  magnitude is 15% of the authored target distance.
- 400x224. FPT authored and white-diffuse captures use 32 SPP and four bounces;
  normal/depth captures use one unjittered sample.
- Native Mandelbulber CPU supplies separate authored, white camera-headlight
  and signed world-normal/depth references.
- No registration, image mirroring, exposure compensation or palette changes.
  Native authored rendering and FPT path tracing retain their own transport;
  they are not identical-integrator comparisons.

The diagnostic example's explicit `beauty` command retains the original
accumulation/present kernels and uses offline Metal 2.4 default optimization
and fast math, as this scene's production path does. Only the opt-in field
replacement differs. Generated shader/compiler artifacts stay in ignored reports.

Two baseline controls pass pixel-byte-exactly: the existing 200x112, 32-SPP,
one-bounce isolated-light capture, and a fresh production 400x224, 32-SPP,
four-bounce authored capture. This validates these test settings, not every
compiler configuration or production performance.

## Results

MAE is normalized RGB absolute error against the corresponding native reference;
it is an appearance metric, not a geometry proof.

| Camera | White diffuse MAE, before -> candidate | Improvement | Authored MAE, before -> candidate | Improvement |
| --- | ---: | ---: | ---: | ---: |
| Original | 0.097723 -> 0.050133 | 48.70% | 0.120298 -> 0.116085 | 3.50% |
| Lateral | 0.094152 -> 0.047738 | 49.30% | 0.117605 -> 0.112904 | 4.00% |
| Forward | 0.101572 -> 0.051692 | 49.11% | 0.122091 -> 0.117567 | 3.71% |

Visual inspection: white surfaces lose much of the baseline's grain and
incorrectly shaded bands. Large contours and smaller circular structures look
closer to native. Authored output is cleaner too, but native retains stronger
bright highlights and different dark-region lighting. This is not a complete
material, specular or transport fix.

| Camera | Median normal error, before -> candidate | Median relative depth error, before -> candidate | Native-visible misses, before -> candidate |
| --- | ---: | ---: | ---: |
| Original | 11.0904 -> 1.3794 degrees | 0.055915% -> 0.014535% | 0 -> 1 |
| Lateral | 10.7732 -> 1.2653 degrees | 0.053953% -> 0.012809% | 2 -> 7 |
| Forward | 11.4735 -> 1.4877 degrees | 0.057963% -> 0.016111% | 0 -> 3 |

All views improve median/tail normal error and median depth error. All fail
the no-additional-native-visible-misses gate. The earlier 200x112 sub-degree
result must not be generalized to this larger sample. There are **ten newly
missing native-visible pixels**; one old miss is repaired, a net increase of nine.

## Missing-ray diagnosis

All ten newly missing pixels are replayed with the original camera UV mapping.
An instrumented clone of the current primary loop reports stop reason, iteration
count and distance/threshold ratio. Results are cross-checked against the
unmodified marcher and the saved hit record; every classification reproduces.

- Baseline finds all ten in 99-209 iterations.
- Candidate reaches the view-distance limit in 313-467 iterations.
- None stops on a non-finite value, float32 position stall or iteration limit.

Increasing the iteration budget or fixing an addition stall cannot address these
observed exits. Stepping across a field/branch discontinuity is the leading
hypothesis, **not yet a proven exact crossing location**. An accurate local
Jacobian does not guarantee a conservative distance estimate across formula
branches, periodic folds or changing bailout iterations.

## Evidence and verification

Ignored local reports:

- `reports/mandel-analytic42-views/summary.json`: complete results; its
  `normal_depth_gate_passed` is deliberately false.
- `reports/mandel-analytic42-views/authored-comparison.png`.
- `reports/mandel-analytic42-views/geometry-comparison.png`.
- `reports/mandel-analytic42-views/normal-comparison.png`.
- `reports/mandel-analytic42-miss-probes/summary.json`: ten classified rays.
- `reports/mandel-analytic42-production-control`: fresh production control.
- `reports/mandel-analytic42-beauty/baseline-control`: isolated-light bridge check.

Captures retain inputs, settings, commands, hashes, logs and output records.
Beauty probes also save finite linear radiance. Single-run timings are not
benchmarks. Project Rust tests and Python tests pass; the visual coverage gate
does not. Production executable identity remains unchanged.

```sh
cargo build --release --locked --example mandel_derivative_render
python3 scripts/run_mandel_derivative_views.py \
  --source /path/to/riemann-scene.fract \
  --probe target/release/examples/mandel_derivative_render \
  --native-command reports/mandel-aux-directional-controls/42/no-shadows/native/command.json \
  --mandel-root /path/to/mandelbulber2 \
  --decoder reports/mandel-priority-depth/channels \
  --output reports/NEW-derivative-views
```

The sweep returns failure after saving all captures when coverage fails.
`probe_mandel_derivative_misses.py` selects the new misses and verifies repeat
classification. Camera tests reject invalid and parallel camera/up vectors.
Reports are not overwritten. These tools are diagnostic examples, not public
renderer options or a release API.

## Next

1. Test analytic derivatives only in the normal-distance helper, leaving the
   traversal field unchanged.
2. Require baseline-identical primary hit/depth output plus improved white and
   authored appearance on these cameras. Check secondary rays separately:
   improved normals legitimately change bounce directions.
3. Keep the full-field candidate diagnostic-only. Trace the first divergent
   steps before designing a conservative boundary-aware step limiter. Do not
   hide the failure with an arbitrary epsilon, brightness change or extra steps.

The analytic mathematics is worth retaining for research. It is not yet a safe
replacement distance estimator or a 50-scene parity fix.
