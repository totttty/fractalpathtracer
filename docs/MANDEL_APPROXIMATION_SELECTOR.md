# Mandel approximation selector and profile-guided compiler result

This report records the five-stage follow-up to the Mandel simplification
experiments. Exact procedural rendering remains the default. Approximate
iteration controls are selected only from a native-resolution, device-local
cache, while exact compiler improvements remain content-hash selected.

## Current packaged example

[`mandel-approximation-cache-960x540-1spp.json`](mandel-approximation-cache-960x540-1spp.json)
is the final opt-in example produced after the exact compiler harvest. It is
specific to the Apple M1 Max measurements at 960×540 and one sample per pixel.
The gate required SSIM at least 0.98 and a median speedup of at least 1.10×:

| Upstream scene | Selection | Speedup | SSIM | Decision |
|---|---|---:|---:|---|
| `bristorbrot001.fract` | screen LOD 1.0 | 2.651× | 0.99116 | approximate |
| `mandelbulb powe 6 - circle.fract` | iteration scale 0.70 | 1.624× | 0.99934 | approximate |
| `T_sphInvV4_abxKali_hexGrid2.fract` | iteration scale 0.70 | 1.362× | 0.99980 | approximate |
| `hex grid 002.fract` | iteration scale 0.70 candidate | 1.318× | 0.97496 | exact fallback |

The explicit exact entry demonstrates that a speed win is not sufficient.
Scene hash, width, height, and sample count must all match before the runtime
uses a cached approximation. This file is an example and benchmark artifact,
not a portable promise for other devices or output settings.

## 1. Slow-scene cohort sweep

The 20 slowest scenes in `reports/mandel-optimization/cohort.json` were
screened at 120x68, one path-traced sample per pixel, with deterministic seeds
and production single-command dispatch. Each scene compared:

- exact;
- global iteration scales 0.75 and 0.80;
- screen-space LOD rates 0.5 and 1.0.

A candidate needed both SSIM at least 0.98 and speedup at least 1.05x. Six of
20 scenes cleared that screen:

| Scene | Screen winner | Speedup | SSIM |
|---:|---|---:|---:|
| 494 | screen LOD 0.5 | 1.29x | 1.00000 |
| 469 | iteration scale 0.80 | 3.33x | 1.00000 |
| 637 | iteration scale 0.75 | 1.31x | 0.99224 |
| 500 | screen LOD 1.0 | 8.31x | 0.99989 |
| 675 | screen LOD 0.5 | 1.06x | 0.99998 |
| 101 | screen LOD 1.0 / scale 0.75 | 1.24-1.30x | 0.99974 |

The large low-resolution speedups on scenes 469 and 500 did not survive at
native resolution. This is another warning that a small render is a useful
prefilter, not a selector.

The complete screening report is
`reports/mandel-approximation-selector/slowest-20-production/report.json`.

## 2. Ray-weighted formula profiling

The sweep enables the existing GPU work profiler only on exact candidates.
It records distance evaluations, orbit iterations by ray phase, and executed
iterations for each formula slot. The report adds:

- average orbit iterations per distance evaluation;
- formula-slot iteration share;
- ray-phase iteration share;
- an estimated formula GPU-time share, apportioned by executed iterations.

This is deliberately labelled as an estimate rather than an isolated formula
timer. It is sufficient for prioritisation because it weights formulas by the
points and ray types actually encountered rather than by their static source
size.

The highest estimated costs in the production sweep were:

| Formula ID | Estimated GPU ms | Executed iterations |
|---:|---:|---:|
| 2 | 699.16 | 3,388,273 |
| 132 | 669.10 | 3,800,009 |
| 606 | 582.96 | 3,310,839 |
| 217 | 488.32 | 51,508,585 |
| 127 | 469.46 | 47,400,756 |
| 60 | 447.26 | 4,070,187 |

Formula 2's apparent screen cost did not translate into a native speed win.
Formula 132 already has an accepted partial-evaluation policy. Formula 217 was
therefore selected as the cleanest high-cost exact-only target: it occupies
100% of the formula slots in scene 277 and executes about 34 orbit iterations
per distance evaluation.

## 3. Native confirmation

Screen winners were rerendered at the scene's native resolution and one sample
per pixel. The same SSIM 0.98 and speedup 1.05x gates were applied:

| Scene | Native size | Candidate | Speedup | SSIM | Decision |
|---:|---:|---|---:|---:|---|
| 469 | 1600x1200 | scale 0.80 | 1.001x | 1.00000 | exact |
| 494 | 1920x1080 | LOD 0.5 | 0.951x | 0.98565 | exact |
| 500 | 1600x1600 | LOD 1.0 | 1.032x | 0.99869 | exact |
| 637 | 1920x1080 | scale 0.75 | watchdog / 1.33x tiled | 0.96462 tiled | exact |
| 101 | 1536x1536 | scale 0.75 | 1.215x | 0.99954 | accept |

Scene 675's marginal screen result did not repeat in the focused confirmation
run, so it also remains exact. Only one of the 20 scenes receives an automatic
approximation. That selectivity is intentional.

## 4. Persistent exact-or-approximation cache

The renderer now accepts:

```text
--mandel-optimization exact|auto
--mandel-selection-cache PATH
```

The default is `exact`. In `auto` mode the renderer hashes the complete scene
file and looks up the exact native width, height, and sample count. It applies
only `iteration_scale` or `screen_lod` entries that meet at least SSIM 0.98 and
1.05x speedup. Manual approximation flags override the cache.

Missing files, unknown schemas, malformed entries, changed scene contents,
different output settings, failed native renders, and candidates below the
hard gate all fall back to exact rendering. A cache can alternatively be
provided through `FPT_MANDEL_SELECTION_CACHE`.

The historical production cache from this first run was written to
`reports/mandel-approximation-selector/selection-cache-production.json`. Its
only approximate entry was scene 101 at 1536x1536 and one sample. An automatic
smoke render reported iteration scale 0.75, confirming that the renderer
applied the cached decision. The tracked example above supersedes it as the
current demonstration cache.

## 5. Profile-guided exact compiler lowering

For formula 217 (`PseudoKleinianMod4`) the scene-bound structural compiler was
first tested across five runs:

| Lowering | Median GPU ms | Speedup | Changed pixels |
|---|---:|---:|---:|
| Baseline | 477.25 | 1.000x | 0 |
| Structural partial evaluation | 469.52 | 1.016x | 0 |
| Structural + loop/DCE/CSE | 466.49 | 1.023x | 0 |
| Phase + loop/DCE/CSE | 525.52 | 0.908x | 145 |

Those passes did not meet the 1.10x exact gate. Profiling showed that scene
277 is a homogeneous one-slot hybrid, so the compiler was then tested with its
dynamic hybrid dispatch replaced by one direct ordered formula loop.

At 120x68 across five runs, the direct loop reduced the median from 449.77 ms
to 167.98 ms: 2.68x faster with zero changed pixels. The native 1800x1200
watchdog-safe gate produced:

| Backend | GPU time |
|---|---:|
| Dynamic hybrid | 131,318.11 ms |
| Direct homogeneous loop | 45,496.26 ms |

That is a 2.89x speedup with zero changed pixels, MAE 0, and SSIM 1.0. The
scene-content hash is now included in the production direct-hybrid policy.
Unknown or modified scenes retain dynamic dispatch.

Results are stored in:

- `reports/mandel-approximation-selector/formula-217-exact-autotune/report.json`
- `reports/mandel-approximation-selector/formula-217-direct-hybrid/report.json`
- `reports/mandel-approximation-selector/formula-217-native-tiled/`

The native directory also contains `comparison.jpg`, showing the
pixel-identical dynamic and direct-loop renders side by side.

## Reproduction

```bash
python3 scripts/mandel_iteration_sweep.py /path/to/mandelbulber2 \
  --slowest 20 --cohort-label slow \
  --scales 0.75,0.80,1.0 --screen-lod-rates 0.5,1.0 \
  --width 120 --height 68 --samples 1 \
  --minimum-ssim 0.98 --minimum-speedup 1.05 \
  --native-confirm-count 20 --native-samples 1 \
  --selection-cache reports/mandel-selection-cache.json
```

Do not set tiled-dispatch environment variables when generating an automatic
production cache. Tiling is a watchdog recovery mode and materially changes
the timing overhead. It is valid for parity tests only when both sides use the
same dispatch.
