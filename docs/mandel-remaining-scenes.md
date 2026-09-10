# Remaining Mandel Scene Audit

## Catalogue Integration (2026-09-11)

The [complete experimental catalogue](mandel-catalog/README.md) now exposes
all 746 unique scenes by stable ID, with source-hash-checked loading and
reviewed/experimental/blocked labels. Source paths and hashes were checked
against the external checkout for every row. Scene sources, generated volumes
and raw reports remain external/ignored. The ranked-gallery images are unchanged.

This is metadata integration, not a fresh 696-scene render or NAADF/CVOX gate.
Historical screening failures remain blocked pending a targeted retest, even
where subsequent implementation work may help. Dated sections below retain
their original findings and uncommitted-at-the-time statements as an audit
trail. Renderer fixes and opt-in diagnostic prototypes are checkpointed
separately from the catalogue; rejected experiments are not enabled by it.

Integration checks: release build; 165 library, 62 binary, 27 integration and
19 targeted example tests; 104 Python tests; formatting and diff checks;
deterministic catalogue regeneration; all 746 source hashes; and a fresh
scene-ID launcher smoke at 96px/1 SPP. Higher-SPP checks remain deferred.

The local upstream examples contain 747 scene files with 746 unique file hashes.
Excluding the reviewed ranked-50 leaves 697 paths and **696 unique scenes**.
`tests/fixtures/mandel-release-remaining696.json` records those scenes without
copying upstream `.fract` files, textures, generated shader sources, or volumes.

IDs `051` through `746` identify a deterministic, path-sorted inventory. They
are not quality rankings. The original gallery IDs `01` through `50` are unchanged.
Byte-identical files are represented once, with alternate paths recorded as aliases.

## Completed Screening

The 2026-09-10 pass attempted both FPT modes for all 696 unique remaining scenes
using the unchanged `029c3bc` checkpoint binary
(`cf8b020ae682eef515020dc2dfda3178f963b1e12b39f1df2cec4169aa53cbcb`).

| Screening outcome | Scenes |
| --- | ---: |
| Both modes passed execution/image validation | 675 |
| Neutral geometry passed | 683 |
| Authored path passed | 686 |
| At least one mode passed, establishing compilation | 694 |
| Scenes with automatic dark/flat-output warnings | 20 |
| Both modes passed with no automatic warning | 657 |

There were no Metal compilation failures or process timeouts. The 21 scenes
with a failed mode are not all unsupported fractals:

- Eleven neutral captures were blank/single-colour at this screening resolution.
  They need native controls before deciding whether the cause is framing,
  sampling, geometry, or an intentionally empty view.
- Five authored captures rejected multi-ray AO with iteration-threshold distance
  evaluation: `447`, `456`, `457`, `458`, and `513`.
- Two authored captures need missing author-local lightmaps: `071`
  (`Hydrangeas.jpg`) and `519` (`forest_004.jpg`). No replacement was substituted.
- `659` (`light types`) requires an enabled formula, while its source has none.
- `720` (`primitives001`) uses unsupported sphere limits.
- `729` (`random lights`) requests 2,000 generated lights, exceeding the current
  256-light cap; it is not evidence of a broken fractal formula.

Automatic warnings overlap execution successes and failures; they are not an
additional 20 failed scenes. Source names/IDs above refer to the remaining-scene
manifest, not the original ranked-50 gallery.

Local evidence is retained in
`reports/mandel-remaining696-screen96-20260910/summary.json`, with a triage table,
14 preview sheets, and the collection-balanced next-review manifest under
`reports/mandel-remaining696-screen96-20260910-final/`.

The run paused at the 1-GiB disk guard after 521 complete scenes, then resumed
without repeating completed modes once storage headroom recovered. During that
pause, the generated render cache was relocated to the Ventura report volume,
with its original temporary path retained as a symlink. All 1,742 existing files
(394,929,806 bytes) were verified unchanged with aggregate tree SHA-256
`cf514ce57a3161e70029306c24f50a81fb2d8add68d659a9c19ef22b24fea2f2`.
No old reports, scene sources, or trusted reference captures were deleted.

## Screening Contract

This pass tests **continuous FPT Metal**, not NAADF/CVOX conversion:

- 96 pixels on the longest edge, authored aspect ratio, 1 SPP.
- Separate neutral-geometry and authored-path captures.
- Existing production renderer and scene/config bounce settings, no approximation.
- Serial execution, chunked accumulation, eight-row Metal tiles.
- A 120-second process timeout per mode. Timeouts are inconclusive, not proof of
  unsupported geometry.
- Source, binary, harness, image, and available metadata hashes retained.
- No native reference renders in this inexpensive screening pass.
- Pause if output or temporary storage falls below 1 GiB free.

Process completion, image validation, dark/blank warnings, and visual fidelity
remain separate. A successful colour render can still be black, misframed,
missing structures, or lit differently. Neither the execution counts nor the
automatic luminance flags certify appearance parity. Fog/cloud support remains
deferred. Declared texture filenames are inventory, not proof that all assets
are available or interpreted correctly.

## Reproduce

Supply the external Mandelbulber source root; no author-specific scene files
need to be added to this repository. Set `MANDELBULBER_ROOT` to the directory
containing `deploy/share/mandelbulber2` and `src`.

```sh
python3 scripts/prepare_mandel_remaining.py \
  --scene-root "$MANDELBULBER_ROOT/deploy/share/mandelbulber2/examples" \
  --reviewed tests/fixtures/mandel-release-ranked50.json \
  --output reports/remaining-manifest.json

env FPT_MANDEL_TILED_DISPATCH=1 FPT_MANDEL_TILE_ROWS=8 \
  python3 scripts/run_mandel_support_suite.py \
  --manifest reports/remaining-manifest.json \
  --scene-root "$MANDELBULBER_ROOT/deploy/share/mandelbulber2/examples" \
  --mandelbulber-root "$MANDELBULBER_ROOT" \
  --fpt target/release/fpt-metal \
  --modes geometry authored --max-axis 96 --samples 1 --timeout 120 \
  --pages-every 50 --output reports/remaining-screen96
```

Add `--scene-limit 5` for an initial pilot. Re-run the same command with `--resume`
and without that limit to continue. Inputs/settings must remain unchanged.
Recorded failures are retained rather than silently retried or overwritten;
use a separate follow-up report for changed settings or fixes.

```sh
python3 scripts/summarize_mandel_screening.py \
  --report reports/remaining-screen96/summary.json \
  --output reports/remaining-screen96-review
```

The review directory contains paginated previews, execution/triage tables, and
a candidate next-50 manifest balanced across the source collections. Automatic
selection excludes known execution failures and dark/blank flags. It is a
review queue, not a claim that those scenes are ready to publish.

## Promotion To The Gallery

### First Additional 50: Completed Review

The collection-balanced follow-up completed **50/50 neutral FPT captures,
50/50 authored FPT captures, and 50/50 matching-dimension native controls**.
FPT used 300px max edge and 32 SPP; native CPU renders used authored sampling
and lighting settings, not an equivalent Monte Carlo integrator or SPP contract.
The unchanged production binary was used throughout. All 50 rows have manual,
source-hash-bound visual notes. There were no process timeouts.

Scene `572` is an explicit exception to the native authored settings: its stereo
output was 600x158, while FPT produced a monoscopic 300x158 image. A fresh native
control with `stereo_enabled=false` now matches those dimensions. The original
failure, logs and image are retained; neither eye was cropped or resized.
The derived report records this override and does not certify stereo support.

Local artifacts:

- `reports/mandel-additional50-review300-20260910/summary.json`: original audit.
- `reports/mandel-additional50-mono572-20260910/summary.json`: derived mono control.
- `reports/mandel-additional50-gallery-20260910/README.md`: five detailed sheets,
  overview, attribution, capture hashes and per-scene review notes.

These are **review candidates, not 50 additional parity-certified scenes**.
The accepted ranked-50 gallery is unchanged. No renderer fixes or performance
improvements are claimed by this audit, and capture wall times are not GPU
benchmarks. Some Newton authored captures took approximately 393-438 seconds,
which was accommodated without reducing samples or changing their render path.

### Water, Structure And Lighting Investigation

The follow-up changes target continuous FPT Metal only. The original screening
counts above remain results from the unchanged checkpoint, not updated support
claims for the new implementation. The accepted ranked-50 gallery is unchanged.

Implemented:

- Water distance evaluation imported from the external upstream OpenCL source
  at runtime, including local transforms, wave parameters and fixed material
  selection. No upstream evaluator body is checked into this repository.
  Unsupported object-coupled waves, grouped transforms and non-OR/smooth
  combinations fail explicitly instead of silently changing geometry.
- Fixed-colour sphere material selection, previously missing from a path that
  selected only plane materials. Scene `379` now uses its sphere's own colour.
- Legacy pre-2.25 auxiliary point lights, translated through the existing
  point-light path with native compatibility scaling. Volumetric effects remain
  reported as unsupported; the surface light is retained.

Controlled diagnosis found:

- Native water-off controls for `379` and `602` closely approach the old,
  waterless FPT output. The omitted primitive was a real structural cause.
- `380` also contains a box shell and material displacement that remain missing.
  Restoring water alone does not repair that scene.
- Rounding native camera positions to float precision barely affected `380`
  and `567`; it did not explain their large differences. A legacy DE-setting
  migration trial worsened `567` and was reverted. Its old output is retained.
- `053` remains a palette/colour-orbit outlier; removing AO did not recover the
  authored colours. `377` has material/reflection differences amplified by
  exposure. No scene-specific exposure compensation was retained.
- `372` now includes the legacy orange point light. A native reference without
  its volumetric beam has RGB MAE `0.02555` against corrected FPT; additionally
  disabling glow gives `0.00719`. The bright authored beam is not a missing
  surface-light intensity. Volumes remain deferred.

Review captures use 300px max edge and 32 FPT SPP, unchanged source files and
cameras. Native references retain authored settings, so RGB MAE is an appearance
diagnostic, not a geometry score or proof of equivalent integrators. In
particular, the white-water controls have different normal/lighting responses.

Final verification completed 12/12 neutral and 12/12 authored captures for
`052`, `053`, `055`, `060`, `372`, `377`, `379`, `380`, `567`, `572`, `602`,
and `603`. All 24 match the retained candidate's capture hashes. Against the
pre-fix baseline, the 13 unaffected mode captures are byte-identical; changes
are confined to the five water scenes and `372` authored lighting. This is a
targeted regression gate, not a rerun of all 696 scenes or all 31 water scenes.

| Scene | Authored RGB MAE before | After | Interpretation |
| --- | ---: | ---: | --- |
| 052 | 0.2297 | 0.0992 | Missing water restored; material response still differs |
| 379 | 0.2151 | 0.1336 | Water and fixed sphere colour restored |
| 602 | 0.1296 | 0.1344 | Water restored, but global colour error slightly higher |
| 372 | 0.3626 | 0.3571 | Surface point light restored; dominant native volume absent |
| 380 | 0.2011 | 0.1579 | Partial water fix; substantial geometry mismatch remains |

Release build, formatting and diff checks pass. Tests pass: 151 library,
62 binary, 27 integration and 77 Python tests. The local shell resolved an
older Homebrew `rustdoc`; full tests were rerun successfully with `RUSTDOC`
set to the pinned toolchain's executable. No repository toolchain change was
needed. These changes are not a performance benchmark and are not yet committed.

Local evidence:

- `reports/mandel-outlier-cause-controls-20260910/`: native one-variable controls.
- `reports/mandel-outlier-de-controls-20260910/`: unsuccessful DE/camera probes.
- `reports/mandel-water-material-final-20260910/`: intermediate trial containing
  the rejected DE migration; **not** the retained candidate.
- `reports/mandel-053-appearance-diagnosis-20260910/`,
  `reports/mandel-372-appearance-diagnosis-20260910/`, and
  `reports/mandel-377-appearance-diagnosis-20260910/`: lighting/material controls.
- `reports/mandel-372-native-no-volume-20260910/`: beam/glow isolation references.
- `reports/mandel-water-light-verified-20260910/`: final build regression captures.
- `reports/mandel-water-light-review-20260910/`: hash-checked reference/before/after
  sheets showing both improvements and unresolved outliers.

### Remaining Fixes, In Order

1. Continue lattice geometry in `567` and monoscopic structure in `572`.
   Scene `380` now has a tested displacement implementation (follow-up below),
   but its authored appearance remains incomplete. Validate the other water scenes
   before treating the whole primitive family as visually supported. Keep
   cameras fixed rather than concealing errors through post-render framing.
2. Isolate colour-orbit/material behaviour in `053` and reflection/material
   behaviour in `377`, then water normals/reflection/refraction. Keep deferred
   volume differences separate from surface lighting. Geometry alignment alone
   is not an authored appearance pass.
3. Resolve unsupported input contracts from the screening: iteration-threshold
   AO, primitive-only scenes, sphere limits, and the generated-light count cap.
   Track missing author-local assets separately rather than substitute textures.
4. Recheck the eleven blank neutral screening captures against native controls
   at a higher resolution before classifying them as unsupported geometry.
5. Promote reviewed successes progressively, then take another collection-balanced
   batch. Do not add all remaining scenes to the accepted gallery on the strength
   of execution counts alone. NAADF/CVOX export validation remains a separate gate.

### Box-Shell Follow-Up

Scene `380`'s enabled `primitive_box_2` was silently omitted. The continuous
Metal path now specializes OR-box distance and fixed material selection, with
solid and empty-shell forms, size, position/rotation, thickness, rounding and
local limits. Repetition, smooth/non-OR combinations and grouped transforms
are explicitly rejected. Scenes with no enabled boxes retain their old path.
Perlin displacement was not implemented by the box fix itself; see the
subsequent displacement follow-up below.

A failing enabled-empty-box test was recorded before implementation and now
passes. Native controls hold the camera fixed and disable displacement in both
the box-on and box-off variants, separating this omission from displacement.

| Scene 380 comparison | Before box fix | After |
| --- | ---: | ---: |
| RGB MAE vs white native control, displacement off | 0.112682 | 0.025003 |
| RGB RMSE vs that control | 0.278644 | 0.053291 |
| RGB MAE vs full authored native reference | 0.157851 | 0.134559 |

These are appearance errors, not geometric IoU. Captures are 300px max edge and
32 FPT SPP with no image registration. The 12-scene regression repeated both
FPT modes: **24 passed, and the 22 non-380 captures are byte-identical** to the
previous water/light candidate. No performance improvement is claimed.

The `567` investigation remains diagnostic only. Native shorter steps and
post-load finalizer controls did not repair the visible mismatch. A precision
probe finds that the source camera and target (separation `1.286e-8` fractal
units) round to the same GPU position. At the camera, local double-precision
IFS evaluation gives an orbit radius of `173.8874`; using the rounded input
gives `189.0808`, versus GPU `190.6203`. These values establish input/arithmetic
sensitivity, not the cause of the full image error. The local CPU evaluator
is not an authoritative native Mandelbulber probe, and its preferred distance
finalizer differs from the authored override. No production precision,
camera or finalizer change was retained.

Evidence:

- `reports/mandel-box-structure-regression12-20260910/summary.json`: captures.
- `reports/mandel-box-structure-review-20260910/comparison.png`: neutral and
  authored before/after, with remaining displacement clearly visible.
- `reports/mandel-structure-native-controls-20260910/`: native controls; its
  README records a misnamed pseudo-Kleinian diagnostic, excluded from IFS claims.
- `reports/mandel-567-precision-cpu-20260910.json`: original/rounded-input local
  CPU and GPU probes. The camera is not moved to conceal the discrepancy.

Release build and tests pass (153 library, 62 binary, 27 integration, 77 Python).
Changes remain uncommitted and unpushed. Gallery promotion and NAADF/CVOX
validation are still separate work.

### Perlin Displacement Follow-Up

Scene `380` now applies the authored material displacement to both its fractal
and box shell. The external upstream noise evaluator is imported into the
generated Metal source, with a native macOS permutation and separate material
parameters. Primary distance, normal sampling and material selection all see
the displaced field. No camera, framing, exposure or formula change was made.
Non-boolean fractals and supported primitive paths are covered; boolean-fractal
material routing is explicitly rejected rather than guessed. Other Perlin
material effects and fog/clouds remain outside this implementation.

The initial RGB comparison looked worse: `0.1875 -> 0.2059` against the existing
native white-headlight control. Four object-isolation controls showed the
mismatch was concentrated on the box. Native normal outputs still contained
the black ceiling, and an emissive-white control showed complete coverage.
Disabling texture intensity alone did not remove the black output. An explicit
native zero-intensity Perlin colour branch produced white there without
changing displacement. This localizes the problem to the native colour path,
not missing ceiling geometry; the exact reason for the native black colour is
not yet established. The regular native control is retained, not overwritten.

`scripts/run_mandel_displacement_controls.py --neutral-perlin-color` records
that alternative reference explicitly. It changes only colour enable/intensity
for materials whose noise is already enabled; it never enables noise on an
inactive material or modifies displacement. It is a diagnostic setting, not an
authored scene correction. RGB differences from these controls cannot certify
geometric IoU or equal integrators.

Against that explicitly neutralized reference, scene `380` RGB MAE improved
from `0.223707` before displacement to `0.030739` after displacement (86.3%
lower). The isolated corrected controls have MAE `0.009787` with neither
displacement enabled, `0.017118` with fractal-only, and `0.026576` with box-only.
Normal estimation, water response and pixel sampling still differ.

Independent probes compiled the actual native `cPerlinNoiseOctaves` evaluator
and compared six displacement values with Metal: two materials at three
positions. Maximum absolute distance error was approximately `1.72e-7` fractal
units. Rebuilding the native probe with the application's exact compiler/flags
gave identical CPU results. This validates the tested seed and sample points,
not every noise seed, transformed material or scene.

The 12-scene regression completed **24/24 captures**, with all **22 non-380
captures byte-identical** to the accepted box/water/light candidate. Scene
`380` authored RGB MAE improved from `0.134559` to `0.101628`; authored RMSE
improved from `0.211270` to `0.138040`. Captures retain the original camera,
300px max edge and 32 FPT SPP. These are appearance metrics, not GPU benchmarks.
The 696-scene screening counts above have not been rerun or promoted.

Evidence:

- `reports/mandel-perlin-object-controls-20260910/`: original four native/FPT
  displacement toggles, including the misleading black-colour comparison.
- `reports/mandel-perlin-native-flat-depth-20260910/`: auxiliary native normal
  and depth outputs. The generic single-image capture helper rejects multiple
  PNGs; these are diagnostic artifacts, not a successful suite record.
- `reports/mandel-perlin-native-material-controls-20260910/`: emission/diffuse
  investigation. One attempted depth-auto-range parameter was unrecognized;
  those auxiliary captures are excluded from numerical acceptance metrics.
- `reports/mandel-perlin-white-controls-20260910/`: explicitly labelled white
  colour bypass, with source/binary hashes and all four displacement toggles.
- `reports/mandel-380-perlin-probe-20260910.json`: numerical Metal probes.
- `reports/perlin-native-probe-20260910.cpp`: independent external CPU probe.
- `reports/mandel-perlin-regression12-20260910/summary.json`: final captures.
- `reports/mandel-perlin-review-20260910/`: white-control and authored comparison.

Release build, formatting and diff checks pass. Tests pass: 156 library, 62
binary, 27 integration, and 79 Python tests. Current executable SHA-256:
`2a854178801edcde6c0f615149eafe53fc16556bd9cbc51397949b4851614fbf`.
The run's temporary directory was placed on Ventura after the internal volume
tripped the 1-GiB guard; no old report or source was deleted. Changes remain
uncommitted and unpushed. NAADF/CVOX validation and gallery promotion are separate.

### Scene 567 Ray And Distance Diagnosis

The next structural outlier is `567`, `IFS31_anim.fract` (formula 10,
kaleidoscopic IFS). This investigation changes diagnostic tools only. The
production renderer remains at the Perlin checkpoint above, with the same
executable hash. No original scene, camera, framing, or gallery entry changed.

Three independent controls narrow the cause:

- **Camera direction:** 25 rays from the real native camera/projection code
  agree with mapped Metal rays to within `0.00001149` degrees. This rules out
  a meaningful orientation error in these probes, not all position precision
  effects in this highly zoomed scene.
- **Legacy settings:** native loading migrates this pre-2.20 scene's raw
  `delta_DE_function=2` to preferred IFS (`0`). FPT currently retains `2`.
  On eight identical float-rounded positions, selecting the native setting
  reduces distance errors from roughly 159-196% to 0.82-9.78% (median 1.52%).
  This setting correction alone is not promoted because rendered coverage
  gets worse until the stepping/precision problem is addressed.
- **Float32 stepping:** of 3,225 sampled primary rays, 813 (25.21%) stop because
  adding the next step leaves the position unchanged; 2,156 hit and 256 leave
  the view range. There are no nonfinite or iteration-budget exits. The
  instrumented hit flags match the original loop.

Four diagnostic captures use the same camera, 300x169 dimensions and one
center ray per FPT pixel. These are not 32-SPP beauty renders or timing gates.
The parametric experiment reconstructs primary positions from ray origin plus
accumulated distance; refinement stays unchanged.

| Diagnostic variant | Image hit coverage | Native-visible misses | Extra hits | Median relative depth error |
| --- | ---: | ---: | ---: | ---: |
| Current FPT | 66.25% | 15,851 | 8 | 93.26% |
| Parametric primary positions | 89.72% | 3,975 | 32 | 99.93% |
| Native preferred-IFS setting | 44.13% | 27,078 | 19 | 11.96% |
| Both changes | 97.29% | 181 | 76 | 9.43% |

Miss/extra and depth columns use native EXR depth (49,432 visible pixels).
Depth statistics apply to mutually visible pixels, so missing pixels are not
included in those medians. Native sampling differs from the FPT center ray.
The sheet uses a separate flat-white native PNG mask, differing by one pixel
from EXR coverage. Neither is an exact parity certification.

**All four remain diagnostic, with no candidate promoted.** The combined
trial still renders incorrect strips and has extreme depth outliers: P95
relative depth error is approximately 1,488 as a ratio, not percent. Its good
coverage and lower median do not establish structural correctness. In the
5,814 mutually visible pixels whose relative depth agrees within 0.1%, median
normal error is 0.40 degrees. Wrong intersection depth remains the main visual
failure rather than just normal shading.

Evidence retained in ignored local reports:

- `reports/mandel-567-ray-exits-20260910/`: sampled ray exit classification.
- `reports/mandel-567-native-distance-20260910/`: actual upstream distance
  samples and resolved settings, not a local substitute evaluator.
- `reports/mandel-567-preferred-ifs-fields-20260910.json`: corrected-setting
  Metal field samples at the same test positions.
- `reports/mandel-567-native-ray-check-20260910/`: 25 camera ray comparisons.
- `reports/mandel-567-parametric-20260910/`: four captures, structural dumps,
  hashes and explicitly derived preferred-IFS input; original scene untouched.
- `reports/mandel-567-native-depth-20260910/`: native EXR, decoded channels,
  command provenance and per-variant depth comparisons.
- `reports/mandel-567-march-depth-review-20260910/`: comparison sheet and
  machine-readable summary with hashes and sampling caveats.

The next experiment should preserve source precision through camera inputs,
IFS constants and orbit evaluation, first checking identical-point distances
against native. Only then combine higher-precision evaluation with the legacy
setting correction and parametric ray positions, and repeat image/depth gates.
More camera offsets, hit padding or false-hit acceptance would hide the failure.
This is continuous FPT diagnosis; NAADF/CVOX and gallery promotion remain separate.

### Scene 567 Higher-Precision Follow-Up

The isolated higher-precision experiment now recovers the lattice. The actual
external upstream formula is imported at runtime, with three-term Metal
arithmetic for camera positions, constants, folding, matrix products and orbit
evaluation. A native diagnostic adapter supplies resolved double parameters
and camera rays. The legacy preferred-IFS distance function is used. This is
**not yet a production fix or standalone library implementation**.

Identical-point validation passes with repeat-byte-identical output:

- Original 16 probes: maximum absolute distance error `1.31e-16`, maximum
  relative error `7.08e-10` (ratio).
- Expanded 1,503 surface-near probes: maximum absolute distance error
  `2.91e-16`, median relative error `5.42e-9`, maximum relative error
  `2.38e-4` (ratios). The largest relative errors occur at tiny distances;
  the acceptance bound is `1e-12 + abs(native_distance)*1e-5`.
- The expanded points sample the existing native depth every ten pixels,
  projected onto the center-ray grid at 0.99, 1.00 and 1.01 times that depth.
  They are near-surface samples, not exact native intersection coordinates.

The first 150x85 render visually restored the lattice without stalls. At
300x169, using FPT center-ray sampling, median relative depth error fell from
93.26% to 0.202%, with 84 missing and 70 extra pixels. Inspection of native
`render_worker.cpp` and `render_job.cpp` then confirmed that native's unjittered
CPU image uses integer screen coordinates, not FPT's half-pixel centers.
Matching that convention gives:

| 300x169 diagnostic | Native-visible misses | Extra hits | Median relative depth error | P95 relative depth error |
| --- | ---: | ---: | ---: | ---: |
| Current float32 FPT, center samples | 15,851 | 8 | 93.26% | 1,337,363.85% |
| Three-term precision, center samples | 84 | 70 | 0.202% | 6.807% |
| Three-term precision, native samples | 0 | 2 | 0.000744% | 0.002479% |

The final candidate has 49,434 hit pixels versus 49,432 native. On mutually
visible pixels, 99.719% of depths agree within 1%; this still leaves outliers
and is not exact parity. Neither precision render stalls or exhausts the
10,000-step limit. There is no image flip, alignment transform, camera offset,
hit padding or false-hit acceptance. Native randomized step reduction and
refinement still differ from the deterministic diagnostic; white-headlight
brightness/normal estimation are not claimed identical.

The structural cause is now strongly localized to float32 precision, together
with the previously identified legacy distance-setting migration. Arithmetic
alone at rounded inputs is insufficient: the diagnostic retains source
precision through constants and camera as well. The experiment does not yet
identify the minimum subset of operations requiring higher precision.

This is an offline correctness reference, **not a speed improvement**. The
300x169 center/native variants take approximately 145.98/143.62 seconds of
measured GPU work, respectively, using bounded 256-ray dispatches. These are
one-off diagnostic intervals, not performance-gated benchmarks. Production
binary SHA-256 remains
`2a854178801edcde6c0f615149eafe53fc16556bd9cbc51397949b4851614fbf`.

Evidence:

- `reports/mandel-native-ifs-config-20260910/`: native linked adapter identity,
  resolved constants and original-source settings.
- `reports/mandel-567-expansion-fields-20260910/`: original 16 points and
  150x85 exploratory render.
- `reports/mandel-567-surface-field-points-20260910.tsv`: expanded inputs.
- `reports/mandel-567-expansion-300-20260910/`: center-sampled render/depth.
- `reports/mandel-567-expansion-native-sampling-20260910/`: expanded field gate,
  native-sampled render/depth, commands, shader and artifact hashes.
- `reports/mandel-567-expansion-center-review-20260910/` and
  `reports/mandel-567-expansion-native-review-20260910/`: comparison sheets,
  CSV metrics and hashed JSON provenance.

Next: remove the native parameter/ray dependency by matching its resolved
double values in FPT, then identify a cheaper precision specialization using
these distance/depth gates. Keep it opt-in until structural and authored
appearance checks pass. Do not put the expensive reference in the interactive
loop or claim all IFS scenes supported from this single pinned example.
The ordinary renderer, NAADF/CVOX path and accepted gallery are unchanged.
Python tests (87), formatting and diff checks pass. Nothing committed or pushed.

### Scene 567 Native-Free And Two-Term Follow-Up

The diagnostic no longer needs a native executable to generate render inputs.
`mandel_ifs_precision_inputs` reads the source through FPT's Rust parser,
retains double values and emits constants/rays. All resolved constants match
the previous native dump exactly; maximum ray-component difference is
`7.78e-16`. The resulting three-term image is byte-identical to the earlier
native-input reference and retains zero missing pixels and two extras.
The precision runner can validate against a hash-matched saved native point
report instead of launching the native adapter. External formula source is
still imported at runtime; this is not removal of that source dependency.

A separate compensated two-term Metal arithmetic implementation passed the
4,112-operation numerical gate with repeat-byte-identical output. Maximum
relative arithmetic error was `1.58e-14`. On 1,503 scene probes, maximum
absolute distance error is `1.01e-14`; median relative error is `1.28e-7`,
maximum `0.002851` (ratios). The established mixed absolute/relative field
acceptance bound passes. It remains a finite-range diagnostic, not IEEE
binary64 or a certification for arbitrary scenes/exponents.

| 300x169 native-sampled diagnostic | GPU work | Missing / extra pixels | Median depth error | P95 depth error |
| --- | ---: | ---: | ---: | ---: |
| FPT inputs, three-term | 145.27 s, single run | 0 / 2 | 0.000744% | 0.002479% |
| FPT inputs, two-term | 4.23 s, median of 3 | 0 / 2 | 0.000744% | 0.002480% |
| Two-term, camera origin rounded to float32 | 4.72 s, single run | 34 / 57 | 3.653% | 103.986% |

The first two-term run took 4.95 seconds; fresh repeats took 4.23911, 4.22727
and 4.22762 seconds and produced identical raw render records. This is a large
offline cost reduction, but not an alternating production performance gate.
Both arithmetic variants retain 99.719% of mutually visible depths within 1%
of native and have no stalled or maximum-step exits. Against three-term,
two-term changes 20 pixels by one brightness level (RGB MAE `0.000394` on the
0-255 scale), with an identical hit mask. Maximum relative depth difference
between these variants is `3.97e-5`. Do not describe their images as byte-exact.

Two input-isolation tests establish what cannot be rounded away:

- **Formula constants only rounded:** the distance gate fails before rendering,
  with maximum absolute error `3.98e-8` and median relative error 74.48%.
- **Camera origin only rounded:** unchanged identical-point field queries pass,
  but the render loses depth agreement and introduces missing/extra pixels.
  Directions, other constants and arithmetic remain unchanged in this test.

The retained candidate is therefore two-term arithmetic with high-precision
formula constants, original camera position and precise ray positioning, not
a blanket return to float32. The three-term default remains the correctness
reference. The two-term path is explicit, scene-pinned and offline only.

Evidence:

- `reports/mandel-567-rust-inputs-20260910/`: FPT parser output, input hashes,
  and numerical comparison to native constants/rays.
- `reports/mandel-567-rust-three-20260910/`: native-free three-term capture.
- `reports/mandel-two-term-arithmetic-20260910/`: real Metal arithmetic gate.
- `reports/mandel-567-rust-two-20260910/`: initial two-term field/render gate.
- `reports/mandel-567-rust-two-repeat-20260910/`: three repeat render records,
  GPU intervals, hashes and field validation, with no native process invoked.
- `reports/mandel-567-round-formula-20260910/`: rejected constant-rounding gate.
- `reports/mandel-567-round-camera-20260910/` and its review directory: rejected
  camera-rounding output, including the depth failures despite good coverage.
- `reports/mandel-567-rust-two-review-20260910/`: native/current/prototype sheet.

Next integration should remain opt-in: use this cheaper field/position path
for the pinned IFS scene, then validate authored shading and secondary rays.
Reducing precision in final scalar distance/shading calculations is a separate
experiment; it must not round the orbit, constants or camera prematurely.
The 4-second reference is not suitable as an interactive default. Production
renderer, public CLI, NAADF/CVOX output and accepted gallery remain unchanged.
Checks: 3 Rust input-helper tests, 88 Python tests, formatting and diff checks
pass. Production binary hash remains the prior Perlin checkpoint. No commit
or push was made.

### Scene 567 Full-Path Precision Pilot

Added an explicit experiment to `mandel_derivative_render`, not the production
CLI. `scripts/run_mandel_ifs_path.py` generates the scene-pinned helper from
the FPT double inputs and externally supplied formula source. Both arithmetic
variants now run inside the existing `renderPath` accumulation/presentation
path, compiled with safe math. Ordinary beauty experiments retain fast math.

Positions remain expanded through primary and secondary marching, finite
difference normals, palette orbit queries, shadow walks and bounce offsets.
Only final shading values and scattering directions become float32. The
single-material palette/final shading, random sampling and integrator logic
are reused. AO/fog/clouds/glow are excluded; auxiliary/fake light paths,
camera overrides, DOF and unrelated scene hashes are rejected. This does not
claim native material/lighting equivalence or general IFS support.

**Result: useful integration, but two-term full-path promotion fails.**
Small matched tests use 32x18, 4 SPP and identical settings. The three-term
variant is the numerical reference, not native path-tracing ground truth.

| Control | Changed displayed pixels | RGB MAE / 255 | Max channel change | Three-term render wall | Two-term render wall |
| --- | ---: | ---: | ---: | ---: | ---: |
| White headlight, primary | 0 | 0 | 0 | 2.866 s | 0.107 s |
| Authored, 2 bounces | 0 | 0 | 0 | 6.596 s | 0.240 s |
| Authored, 3 bounces | 12 | 0.007523 | 1 | 9.682 s | 0.357 s |
| Authored, 4 bounces | 89 | 0.203704 | 43 | 12.743 s | 0.467 s |

Even the first two displayed matches have nonzero linear-radiance differences;
they are not byte-exact arithmetic matches. At four bounces, linear RGB MAE
is `0.00081036`, maximum `0.126177`. Repeating the two-term four-bounce capture
produces byte-identical linear records and PNG output. The first *displayed*
divergence is therefore at three bounces; this does not yet locate the first
divergent geometric or shading operation. Do not infer incorrect geometry
versus changed shadow/normal/scattering decisions without per-ray evidence.

The reported times are **single-run renderer wall times**, excluding offline
compile/link but including bridge scheduling/waits; they are not measured
Metal GPU intervals or an alternating performance gate. The earlier standalone
256-ray-per-command timings use a different dispatch and cannot be compared
directly. The diagnostic summary now distinguishes beauty wall time from GPU
diagnostic time instead of labeling both as GPU time.

Larger two-term previews use 160x90, 16 SPP. One-bounce authored rendering took
2.395 seconds wall time; four-bounce took 12.030 seconds. Additional bounces
changed linear radiance at 14,173 of 14,400 pixels with no negative added
radiance (maximum addition 1.638526), confirming the continuation work is
active rather than a primary-only substitution. These previews are not
four-bounce parity-cleared. Fresh native captures use the same 160x90 image
dimensions and omit the listed effects; native and FPT sampling/integrators
still differ. Geometry looks substantially more complete, while illumination
and palette interpretation remain separate open questions.

Evidence:

- `reports/mandel-567-path-review-20260910/`: CSV, hashed JSON,
  `comparison.png` and `four-bounce-gate.png`.
- `reports/mandel-567-path-{two,three}-*-20260910/`: generated source,
  compile/link logs, precise-helper/source/scene hashes, linear records and PNGs.
- `reports/mandel-567-path-native160-20260910/`: fresh native commands and images.

Next: trace the first divergent pixel/sample by bounce, including precise hit
position, normal, material coordinate, shadow result and chosen outgoing ray.
Then test selective three-term arithmetic at the demonstrated source of drift.
Do not silently accept the two-term version for all bounces or put the expensive
reference into interactive defaults. This is a continuous FPT experiment;
NAADF/CVOX, production executable and accepted gallery remain unchanged.
Checks: 91 Python tests, 9 focused Rust example tests, formatting and diff
checks passed. No commit or push.

### Scene 567 Visual Acceptance And Nearby-Camera Review

The user accepted the small full-path differences and chose practical visual
acceptance rather than continuing the byte-exact arithmetic investigation.
The prior failed exact gate remains recorded; it has not been relabeled as
passing. The new decision is to retain **an explicit offline two-term mode
for this source-pinned scene**, not change ordinary rendering defaults.

Completed five matched-camera views at **300x169, 32 FPT SPP**: the authored
camera, then parallel left/right/up/down translations of 0.25 times the
original camera-target separation. Each view includes native and FPT white
headlight controls plus native and FPT authored controls. FPT authored uses
four bounces; the white control stops at the primary surface. In total,
10 FPT captures and 10 native references completed, with finite linear output
throughout and no stalled/exhausted-ray failures. Native and FPT image sizes
match exactly, but their sampling and shading/integrators are not equivalent.
AO/fog/clouds/glow are excluded from both sides of these controls.

Manual inspection found the lattice, foreground pillars and principal openings
consistent across all five pairs, without obvious new holes or a framing jump.
FPT has brighter white geometry shading and substantially more blue/green
indirect illumination in the authored controls. These remain documented
appearance differences, not hidden by exposure adjustment. This is a bounded
translation test, not proof of unrestricted rotation or temporal stability.
The left shift changes 40,482 native and 35,963 FPT headlight pixels out of
50,700, so the test is not merely repeating a camera rounded to the same position.

| View | FPT white render wall | FPT authored render wall |
| --- | ---: | ---: |
| Center | 24.901 s | 131.784 s |
| Left | 24.701 s | 131.909 s |
| Right | 25.090 s | 131.969 s |
| Up | 24.903 s | 131.833 s |
| Down | 24.909 s | 131.846 s |

These are single-run wall times, not GPU timing or an apples-to-apples native
performance benchmark. This path remains for offline output, not interactive
default rendering.

The launcher now accepts `--precision two` (alias of `--arithmetic two`) and
generates FPT's double input files automatically if no cached directory is
supplied. `--camera-shift=RIGHT,UP` records the exact translated camera/target
in `run.json`. It uses the existing tiled-chunk kernel, eight rows and one
sample per command by default; no full-screen precision dispatch is required.
The 32x18 four-bounce tiled smoke is byte-identical to the previous untiled
linear records. A 300x169 direct invocation without tiling is explicitly
rejected. Two-term output is bounded to 320x240/32 SPP in the launcher; the
three-term reference retains its 1,024-pixel image limit.

Evidence: `reports/mandel-567-precision-gallery-20260910/` contains
`center-comparison.png`, the full `comparison.png`, separate mobile-friendly
`geometry.png`/`authored.png`, commands, source/camera/image hashes, linear
captures, `timings.csv`, and the separate `visual-review.json` decision.
`scripts/run_mandel_ifs_gallery.py` reproduces the five-view review.
The ordinary production executable retains SHA256
`2a854178801edcde6c0f615149eafe53fc16556bd9cbc51397949b4851614fbf`.
NAADF/CVOX export and the accepted ranked-50 gallery are unchanged. Nothing
was committed or pushed. Checks: 94 Python tests, 9 Rust example tests,
formatting and diff checks pass.

Next structural target is scene `572` (`hybrid77-stereo`, formula 11).
Its monoscopic native/FPT geometry mismatch remains visible after the stereo
framing control. Do not automatically apply the scene567/formula10 precision
specialization to this different formula. Establish an identical-ray/field
diagnosis first, then revisit material cases `053` and `377` and the remaining
water-family appearance checks. Fog/clouds remain deferred.

### Scene 572 Interior And Clipping Diagnosis

Scene `572` (`hybrid77-stereo`, formula 11) has two independently reproduced
missing rendering features: `limits_enabled` and `interior_mode`. Native CPU
controls at 300x158 explicitly disable stereo, keep the camera unchanged,
and toggle each setting separately. Disabling native limits brings back the
large central object previously present only in FPT. Disabling interior mode
makes the surrounding surfaces substantially more solid. This is not another
scene567 precision specialization or an image-alignment adjustment.

The candidate implements:

- Scene-specialized clipping in original Mandelbulber XYZ coordinates, before
  the fractal's position/rotation/repeat transform, with the native maximum-plane
  bounds distance and an early return outside the current detail threshold.
- Interior distance acceptance, separate normal-distance handling using the
  central surface's detail threshold, and the interior march offset of 0.8
  rather than 0.5. Delta-DE's iteration-limit behavior is selected from the
  actual formula compilation mode, not only the legacy scene flag.
- Compile-time omission for scenes without these settings. Camera-dependent
  rules are guarded to the continuous SDF backend; point-sampled voxel builds
  retain their prior distance behavior. Topology grid sampling is unchanged.
  Boolean-fractal interior combinations fail explicitly until per-object
  handling is implemented.

The four-toggle ablation is reproducible with
`scripts/review_mandel_interior_limits.py`; it records source/binary/control
hashes, complete commands, fresh native/baseline/candidate PNGs and raw image
differences. It also compares the original enabled/enabled authored appearance.
No image flips, camera shifts, exposure compensation or registration are used.

Initial final-review results (normalized RGB MAE; not geometry IoU):

| Control | Previous FPT | Corrected FPT |
| --- | ---: | ---: |
| Neither feature, white diffuse | 0.05951 | 0.05951 |
| Limits only, white diffuse | 0.09892 | 0.03876 |
| Interior only, white diffuse | 0.08343 | 0.08866 |
| Authored limits + interior, white diffuse | 0.11851 | 0.06136 |
| Authored limits + interior, authored shading | 0.08672 | 0.04369 |

The interior-only control does **not** improve aggregate RGB error. This is
recorded rather than hidden: it still differs in fine distance-estimator detail,
sampling and normals. The actual source enables both features and improves
substantially in both neutral and authored views. The correction does not prove
complete interior parity, stereo support or full material/lighting parity.
Native sampling is stochastic, so small rerun differences are expected.

The pre-voxel-guard evidence is in
`reports/mandel-572-final-review-20260910/`; its eleven unaffected scenes produce
22 byte-identical geometry/authored captures in
`reports/mandel-572-final-regression-20260910/`. Guarded final captures and the
point-sampled export check are recorded separately, so these intermediate
measurements are not silently relabeled as a newer binary's output.

Final guarded production binary:
`b16cc7460bb61ef9f1ce455ab99a6ad1f37e97498519c9c5acf0a835a065ca75`.
The fresh five-row review is in
`reports/mandel-572-guarded-final-20260910/`, including
`authored-settings-comparison.png`. With both authored settings enabled,
white-diffuse MAE is **0.11867 -> 0.06166 (-48.0%)** and authored MAE is
**0.08677 -> 0.04369 (-49.7%)**. The disabled-feature row is byte-identical
between baseline and candidate. These remain appearance diagnostics, not
geometric overlap or performance scores.
The final binary also passes all 22 unaffected geometry/authored capture checks
across the other eleven regression scenes; see
`reports/mandel-572-guarded-regression-20260910/summary.json`.

The 24-cubed direct FPTVOX export also remains byte-identical: 58,820 bytes,
SHA256 `72fad555bbaee17497693f39960bd051a07458f9a1c2946d19f4922250bacb7a`.
Commands, binary hashes and outputs are in
`reports/mandel-572-voxel-guard-final-20260910/`. This is a regression control,
not evidence that the exported voxel scene now reproduces the continuous
interior-rendering effect. A camera-independent export contract for these
settings remains separate work. Release build, 162 Rust library tests, 94
Python tests, formatting and whitespace checks pass. Changes are uncommitted
and have not been pushed.

The next fine-geometry investigation should compare identical ray/point
distance and normal samples for this formula's global-fold Delta-DE, particularly
the surviving curved surface bands. The large clipped central-object error is
explained; precision, native sampling and remaining normal differences are not
yet separated. Material cases `053`/`377` and water-family appearance remain
subsequent work. Fog/clouds remain deferred, and no NAADF/CVOX gallery has been
regenerated by this continuous-rendering diagnosis.

### Scene 572 Preferred Estimator Correction

The subsequent identical-point investigation found a parser error rather than
a camera misalignment or a general normal-calculation error. The native 2.x
loader retains `analityc_DE_mode` as a stored parameter, but `cNineFractals`
selects the estimator exclusively from `delta_DE_method`. Scene572 resolves to
method 0 (preferred). FPT incorrectly inferred forced Delta-DE from its stale
`analityc_DE_mode=false` entry. The parser now follows the current native
selection rule; explicit methods 1/2 still select forced delta/analytic.

The parser regression test was observed failing before the correction and
passing afterwards. It covers both legacy flag values, versions 2.18/2.33,
and precedence of explicit current-method settings. This does not implement
the separate pre-2.20 `delta_DE_function` migration implicated in scene567.

Numerical evidence for scene572:

- 1,377 native/Metal camera rays agree to within 0.000023 degrees.
- At the same 128 float-rounded world points, median relative primary-distance
  error decreases from 22.51% to 0.000620%; corrected P95 is 0.01365%.
- Five points still exceed 10% relative error; the maximum is 159.14%.
  The low median is not proof that every orbit or hit is correct.
- At those points, native six-point distance stencils and FPT normals agree to
  0.00183 degrees median and 0.01012 degrees P95, with 128 nondegenerate stencils.
  Native uses the same representable probe positions and central detail size.
- These are separate diagnostic kernels, not a production compiler-parity gate
  or proof that native and FPT select the same first-hit point on every ray.

Raw rays, points, resolved native settings, commands and reports are in
`reports/mandel-572-point-parity-20260910/`,
`reports/mandel-572-preferred-points-20260910/` and
`reports/mandel-572-identical-point-control-20260910/`.
The preferred-points run selects a fresh point set; only the last report reuses
the original 128 points for the direct before/after numerical comparison.
`scripts/run_mandel_point_parity.py` provides the ray and field audit.

Fresh production captures at 300x158 and 32 FPT SPP retain the clipping/interior
fix, camera and materials. White-diffuse image MAE improves from 0.06137 to
0.05709 (7.0%), and authored MAE from 0.04368 to 0.03552 (18.7%). These normalized
RGB measurements are appearance diagnostics, not structural IoU. See
`reports/mandel-572-estimator-final-20260910/comparison.png` and its raw inputs.
Fine speckling and several distance outliers remain, so full scene parity is
not claimed.

The source scan found ten bundled scenes carrying the stale flag without an
explicit current method. None is in the accepted ranked-50 gallery. All ten
received native/baseline/candidate geometry captures in
`reports/mandel-legacy-de10-review-20260910/`:

| Scene | Before RGB MAE | Corrected RGB MAE | Review |
| --- | ---: | ---: | --- |
| hybrid14 | 0.23915 | 0.01391 | Major geometry improvement |
| hybrid25 | n/a | n/a | Native reference is black |
| hybrid31 | 0.15971 | 0.15971 | Candidate byte-identical; existing mismatch |
| hybrid34 | n/a | n/a | Native reference is black |
| hybrid35 | n/a | n/a | Native reference is black |
| hybrid38 | n/a | n/a | Native reference is black |
| hypercomplex 02 | 0.04527 | 0.04619 | Similar silhouette; shading/detail difference remains |
| xenodreambuie2 | 0.15970 | 0.03832 | Major geometry improvement |
| hybrid77-stereo | 0.06137 | 0.05709 | Improved, not exact |
| hybrid77 | 0.13919 | 0.13587 | Small improvement; ring detail remains |

The hybrid14 authored score slightly worsens (0.17699 to 0.18185) despite the
large geometry improvement. The **pre-change** hybrid25 authored renderer hit
Metal's interactivity limit. That workload was not retried; remaining batch
work was deliberately limited to geometry. Neither failed/blank references
nor unrun authored captures count as passing a gallery gate. The retained
change corrects the native parameter contract; it is not a declaration that
all ten scenes are now visually correct or that authored output universally
improves.

No NAADF code or existing CVOX/gallery assets changed. Future exports of scenes
with the stale flag also use the corrected estimator; their voxel content may
therefore intentionally change. The previous 24-cubed byte-exact export check
applied to the earlier clipping/interior guard, not this estimator correction.
No performance, full voxel-parity or merge-readiness claim is made.

Final binary SHA256:
`9db17de60384e89f816e6313b56ebe2a534306760f491048d17445b51010887e`.
All 22 geometry/authored captures across the eleven unaffected regression scenes
are byte-identical (`reports/mandel-legacy-de-unaffected-20260910/`). Directly
rendering the untouched original scene572 file also reproduces both corrected
captures byte-for-byte (`reports/mandel-572-original-source-final-20260910/`),
so the fix does not require a hand-edited scene. Release build, 162 Rust library
tests, 94 Python tests, formatting and whitespace checks pass. No changes were
committed or pushed.

Next: inspect the five remaining numerical outliers and distinguish orbit
precision/escape behavior from interior threshold-band sampling. Separately,
recover usable native references for hybrid25/34/35/38 before their geometry
can be graded. Authored material/lighting work remains separate from geometry.

### Scene572 Numerical Boundaries And Recovered Reference Controls

Follow-up on 2026-09-10, with the production executable unchanged:

- Queried the native analytic orbit directly at the same 128 float-rounded
  world points as the previous control. The adapter calls upstream `Compute`;
  it does not copy a formula implementation. Native raw distances are in
  fractal units; Metal raw distances are in world units (scale 1024).
- Four of the five outliers (12/60/65/124) disagree about escaping after two
  versus three iterations, near the Xenodreambuie bailout radius of 10.
  Interior threshold rules amplify that pre-existing orbit difference.
  Point23 uses two iterations in both implementations but has a different
  radius/distance. One-ULP neighbor probes change its Metal distance by roughly
  10%, while its native neighbors remain stable. These findings implicate
  numerical sensitivity, not a camera transform or bulk normal-direction error.
- Native double-precision results also switch iteration count at some neighboring
  points. Moving the sample or adding a bailout epsilon would not establish a
  generally correct fix.

| Point-kernel math | Median relative DE error | P95 | Points above 1% |
|---|---:|---:|---:|
| Runtime fast / offline fast | 0.000620% | 0.013646% | 5 / 128 |
| Offline conservative | 0.000521% | 0.006809% | 1 / 128 |

The remaining conservative-math outlier is point65 (about 148.86% relative
distance error). Offline-fast point results reproduce runtime-fast exactly.
Full 300x158, 32-SPP mono beauty controls were then rendered with both math
settings. The fast captures are byte-exact to the accepted production captures.
White geometry MAE against native changes only 0.0570853 -> 0.0570667, and
authored MAE 0.0355194 -> 0.0354697. Visual review shows no meaningful recovery
of the remaining structure/speckling. Conservative math is retained only as a
diagnostic option, not enabled in production. These are appearance metrics,
not first-hit completeness or a performance gate.

Evidence:
`reports/mandel-572-raw-field-20260910/`,
`reports/mandel-572-ulp-neighborhood-20260910/`,
`reports/mandel-572-math-control-20260910/`, and
`reports/mandel-572-safe-beauty-20260910/comparison.png`.
Each includes raw inputs, outputs and recorded execution commands; the math
control also retains its generated source, AIR and libraries in ignored reports.

The four previously black native geometry references are now usable with an
explicit **file-loaded headlight control**. Self-lit captures first established
that geometry was present. Reversing the light, forcing delta DE, separating
camera/target, disabling diffuse shading, and normalizing decimal commas did
not fix the tested black controls. Loading full resolved settings worked;
loading only the controlled light settings worked, while loading only the
material controls did not. The discrepancy is isolated to the native capture
light-configuration path, but its internal native root cause is not yet proven.
Do not describe this as a fixed FPT lighting bug.

| Legacy audit row / scene | Recovered native vs accepted FPT RGB MAE |
|---|---:|
| 02 / hybrid25 | 0.041943 |
| 04 / hybrid34 | 0.007132 |
| 05 / hybrid35 | 0.060660 |
| 06 / hybrid38 | 0.037390 |

All four native captures have nonconstant shading. Hybrid34 is visually close;
hybrid25 still differs around its lower/edge spheres, and hybrid35/38 retain
fine-detail/contrast differences. No camera, estimator, resolution or geometry
was changed to recover these references. The derived source files change only
the explicitly recorded geometry-control lights. They are not authored beauty
references and do not establish gallery readiness.

Fresh helper verification also found that replaying duplicate native CLI
overrides could turn this reference black despite identical effective settings
in the linked adapter. The file-loaded helper therefore removes prior geometry
overrides before appending its own, rather than repeating them. The final native
hybrid34 rerender is nonblank and differs from its earlier recovered control by
RGB MAE 0.000240 (native sampling is not byte-exact). This further limits the
root-cause claim: file-versus-CLI behavior is established empirically, but native
override ingestion/initialization still needs upstream investigation.

Evidence: `reports/mandel-native-baked-headlight4-20260910/comparison.png` and
`summary.json`; rejected controls and effective native settings are in
`reports/mandel-blank-reference-probes-20260910/`. The reusable audit flag is
`review_mandel_legacy_de.py --bake-native-lights`; default CLI-light behavior
remains available. Resume rejects a changed light-control strategy or control
hash. `headlight_command(..., bake_lights=True)` preserves the authored source,
uses native file syntax, removes conflicting light CLI overrides, and refuses
to overwrite its derived source. Do not rerun the known watchdog-prone authored
hybrid25 capture just to refresh a geometry control.

The helper's native smoke/provenance check is in
`reports/mandel-native-baked-helper-final-20260910/`. All four generated control
parameter maps match the individually validated controls. Validation passes:
162 Rust library tests, 95 Python tests, both changed diagnostic example builds,
native adapter builds, formatting and whitespace checks. The production binary
retains SHA256 `9db17de60384e89f816e6313b56ebe2a534306760f491048d17445b51010887e`.

The next useful numerical test is a native-first-hit/Metal-first-hit comparison
along identical pixel rays, with frozen thresholds and orbit records at the
first divergent step. The current point set is Metal-hit-selected; good distance
agreement there does not demonstrate complete native surface coverage. Native
capture loading also needs an explicit-settings regression beyond these four
scenes before broadly replacing old reference reports. Existing NAADF/CVOX
assets and the accepted gallery remain untouched. No commit or push was made.

### Scene 572 First-Hit Diagnosis (2026-09-11)

Added diagnostic native-worker and Metal supplied-ray probes, without changing
the production renderer. The native adapter calls the original linked marcher.
A 51x27 grid covers 1,377 rays, including misses, with identical directions and
camera origin after Metal float rounding. Generated versus supplied Metal rays
have zero hit-flag or position differences.

| Native step sampling | Common hits | Native only | FPT only | Neither |
|---|---:|---:|---:|---:|
| Seed 0, no jitter | 282 | 0 | 0 | 1095 |
| Seed 1 | 269 | 22 | 13 | 1073 |
| Seed 2 | 270 | 24 | 12 | 1071 |

Without jitter, raw hits have median position error 0.0000502 native thresholds
and no common hit exceeds one threshold. After refinement, three rays do:
indices 637, 646 and 757, at 1.718, 1.718 and 1.109 thresholds respectively.
Their refinement/threshold policy still needs diagnosis. This is not proof of
full-image parity. Native seeds 1 and 2 themselves disagree on 11 hit flags;
native-only hits must not automatically be labelled true surfaces or FPT-only
hits false surfaces. Step sampling changes the observed visibility in both
directions.

Derived-scene `DE_factor` controls tested 0.98, 0.95 and 0.90, keeping the
production executable and native references unchanged. Factor 0.95 reduces
seed-1/seed-2 visibility disagreements from 35/36 to 10/9 on this grid. A
smaller factor is not monotonically better: 0.90 produces 17/16 disagreements.
Re-querying refined positions is threshold-sensitive and is not a reliable
binary false-hit classification by itself.

| 300x158, FPT 32 SPP | Accepted factor 1.0 MAE | Factor 0.95 MAE |
|---|---:|---:|
| White direct geometry | 0.0570853 | 0.0566432 |
| Authored path | 0.0355194 | 0.0350086 |

The small image improvements do not establish a general correction. Visual
review still shows structural/speckling differences, including the lower curved
surface. No default step factor was changed. Controls remain diagnostic;
timings are not a performance benchmark, and no other scene was gated.

Evidence: `reports/mandel-572-first-hit-full-20260911/summary.json`,
`reports/mandel-572-step-scale-20260911/`, and
`reports/mandel-572-step-scale-beauty-20260911/comparison.png`.
The next focused experiment is a diagnostic native-style step-jitter control,
then refinement traces for the three no-jitter outliers. Broad conservative
math and a blanket smaller step have not resolved the image mismatch.
NAADF/CVOX assets and the published gallery are unchanged; no commit or push.
Verification: native adapter and Metal diagnostic example builds pass, as do
162 Rust library tests, 99 Python tests, `cargo fmt --check` and
`git diff --check`. The production binary hash remains the one recorded above.

### Matched Jitter And Refinement Follow-Up (2026-09-11)

The two targeted hypotheses are confirmed on supplied rays: the native marcher
perturbs each step before clamping, and its refinement recalculates the dynamic
threshold at the updated point. The FPT diagnostic now reproduces each policy
independently without changing production shaders or sampling defaults.

On the original 1,377-ray grid, matching seeds 1/2 removes all 35/36 hit/miss
disagreements. Dynamic refinement removes all three no-jitter outliers, and
the jittered seeds' eight/six refinement outliers. These are errors larger than
one native threshold, not a claim of bit-identical positions.

A separate 64x64 grid provides the denser check:

| Native seed | Rays | Previous hit/miss disagreements | Matched jitter disagreements | Refined hits over one threshold: jitter only -> both |
|---|---:|---:|---:|---:|
| 1 | 4096 | 76 | 0 | 18 -> 0 |
| 2 | 4096 | 68 | 0 | 25 -> 0 |

Median common-hit position error after both controls is about 0.000055 native
thresholds. The no-jitter dense grid also has zero hit/miss disagreements, but
one raw-position outlier remains (index 2270, about 4.82 thresholds). Dynamic
refinement reduces its 27 large refined errors to this one pre-existing primary
outlier (about 4.91 thresholds). Its cause has not been isolated; the result
does not eliminate float32/estimator sensitivity everywhere. Both no-control
and explicit seed-zero controls preserve the previous outputs exactly.

Rendered controls use 300x158, 32 SPP and the unchanged authored `DE_factor`:

| Control | White direct geometry MAE | Authored path MAE |
|---|---:|---:|
| Unchanged | 0.0570853 | 0.0355194 |
| Dynamic refinement only | 0.0573340 | 0.0340870 |
| Fixed-seed jitter only | 0.0565000 | 0.0351806 |
| Both | 0.0565695 | 0.0330003 |

The combined authored MAE improves about 7.1%, but structural/speckling and
shading differences remain visible, particularly on the bottom curved surface.
White-image MAE improves only about 0.9%; dynamic refinement alone slightly
worsens that image metric despite improving first-hit positions. Do not choose
transport correctness from an image MAE in isolation. The fresh unchanged beauty
captures equal the previous accepted captures exactly.

**Retained as diagnostics, not promoted:** the beauty jitter experiment resets
seed 1 per march (including secondary invocations), rather than implementing
native global random sequencing or independent pixel/sample/bounce dimensions.
It is not a production sampling policy. The next gate is cross-scene validation
of dynamic refinement, followed by a properly dimensioned step-jitter sampling
experiment and investigation of the dense no-jitter primary outlier. No broad
math or positional-accumulation change is justified by these results alone.

Evidence: `reports/mandel-572-jitter-refinement-20260911/`,
`reports/mandel-572-first-hit-dense-20260911/`,
`reports/mandel-572-jitter-refinement-dense-20260911/`, and
`reports/mandel-572-jitter-beauty-20260911/comparison.png`. Raw inputs, outputs,
commands and source/binary identities are retained. These are correctness and
appearance experiments, not GPU performance measurements. Production rendering,
NAADF assets and published galleries remain unchanged. No commit or push.
Verification: all 14 dense-grid raw/refined captures repeat with byte-exact
sample arrays in `reports/mandel-572-jitter-refinement-repeat-20260911/`.
162 library tests, 17 diagnostic-example tests, 99 Python tests, both release
example builds, formatting and whitespace checks pass. The production binary
SHA256 is unchanged.

### Cross-Scene Refinement And Dimensioned Sampling (2026-09-11)

Completed ten white direct-geometry scenes plus two existing safe authored
controls, at 300px max edge and 32 FPT samples. The four recovered native
headlight captures replace the known black references. Source and reference
identities are checked; each fresh unchanged FPT capture equals its accepted
displayed output. The watchdog-prone authored hybrid25 is not rerun.

Refinement alone is not a general image win: two geometry MAEs improve, seven
slightly worsen and one stays unchanged. Hybrid77's non-stereo geometry MAE
increases about 0.77%. Its known supplied-ray correctness benefit must not be
confused with a guarantee of better lighting or image MAE across scenes.

Added an **offline dimensioned step-jitter experiment**, separate from fixed
seed-per-march diagnosis. A salted integer hash derives the stream from the
unjittered pixel-plane coordinates, global sample index, bounce and experiment
seed. It does not consume camera/scattering/Fresnel random dimensions. Only the
ordinary path integrator's march call uses the seeded helper; other callers
retain their existing stepping. This is not a claim to match native global
image RNG sequencing, and statistical equivalence is not established.

| White geometry scene | Unchanged MAE | Dimensioned jitter | Jitter + dynamic refinement |
|---|---:|---:|---:|
| hybrid14 | 0.0139122 | 0.0134469 | 0.0134799 |
| hybrid25 | 0.0419434 | 0.0384029 | 0.0384231 |
| hybrid31 | 0.1597080 | 0.1592942 | 0.1592942 |
| hybrid34 | 0.0071323 | 0.0071287 | 0.0071419 |
| hybrid35 | 0.0606599 | 0.0605534 | 0.0605202 |
| hybrid38 | 0.0373901 | 0.0374076 | 0.0374050 |
| hypercomplex 02 | 0.0461900 | 0.0457020 | 0.0457028 |
| xenodreambuie2 | 0.0383220 | 0.0383250 | 0.0383283 |
| hybrid77-stereo (mono control, scene 572) | 0.0570853 | 0.0548378 | 0.0546150 |
| hybrid77 | 0.1358657 | 0.1355701 | 0.1362661 |

Experiment seed 1 improves eight of ten geometry MAEs with jitter alone.
Hybrid25 improves about 8.4%, and scene572 about 3.9% (4.3% with both).
The small hybrid38/xenodreambuie2 regressions are +0.047%/+0.008%; both become
improvements with experiment seed 2, so they are not evidence of a repeatable
regression. Broader sample-count and seed gates are still needed.

For scene572 authored rendering, MAE changes from 0.0355194 to 0.0317518 with
jitter alone (-10.6%) or 0.0305296 with both (-14.0%). Hybrid14 authored output
remains much too dark: jitter-only displayed pixels are unchanged despite
changes in linear radiance; combined MAE is essentially neutral (+0.011%).
Visual review also still finds substantial native/FPT differences on hybrid31
and fine-structure/normal-shading differences elsewhere. None are declared
gallery-correct solely from improved MAE.

Evidence: `reports/mandel-refinement-cross10-20260911/`,
`reports/mandel-sampled-refinement-cross10-20260911/`, and the separate
`reports/mandel-sampled-refinement-seedcheck-20260911/`.
The full sampled suite includes two five-scene geometry sheets and one authored
sheet with native, unchanged, refinement, jitter and combined columns.

Decision: retain the isolated sampling/refinement controls and cross-scene
harness, **do not change production defaults yet**. Next, use higher-SPP
seed/convergence checks on hybrid25, hybrid38, xenodreambuie2 and both hybrid77
views. Evaluate jitter independently before deciding whether to combine it with
dynamic refinement. Remaining large authored-lighting and hybrid31 structural
mismatches need separate diagnosis; jitter is not their general fix.
This work changes no NAADF/CVOX assets, published galleries or production
renderer flags. No GPU performance claim, commit or push.
Verification: 16 follow-up captures cover a second seed on six scene/mode
controls plus four seed-1 repeats; the repeats match both displayed pixels and
linear radiance byte-exactly. Scene572 authored MAE at seed 2 is 0.0317190 with
jitter and 0.0304692 with both, supporting the seed-1 gains. 162 library tests,
11 beauty diagnostic tests, 99 Python tests, the release example build,
formatting and whitespace checks pass. Production executable SHA256 remains
`9db17de60384e89f816e6313b56ebe2a534306760f491048d17445b51010887e`.

### Independent Step Jitter Promotion (2026-09-11)

The user explicitly waived the higher-SPP gate and approved proceeding with
the existing 32-SPP evidence. **Independent step jitter is now the ordinary
continuous Mandel path-rendering default. Dynamic refinement is not promoted.**
The production shader uses the same pixel/sample/bounce stream and experiment
seed 1 as the reviewed jitter-only variant. It preserves the authored DE factor,
step clamps, scalar stepping and existing frozen refinement threshold.

A separate sampled marcher keeps focus, shadow and direct diagnostic queries
deterministic. The profiling path forwards the same sampled seed for its
Mandel primary/secondary walks; shadow profiling keeps seed zero. Regression
tests compare sampled/ordinary loop bodies and clamp logic to prevent future
drift. Camera and scattering random dimensions are unchanged. Non-Mandel path
queries and voxel exporters do not call the new sampled marcher.

All **12 production scene/mode captures** (ten white geometry scenes and two
safe authored controls) match the reviewed jitter-only output with **zero
changed pixels** at the same dimensions and 32 SPP. This is exact parity with
the accepted experiment, not with Mandelbulber. The earlier approximately
8.4% hybrid25 and 10.6% scene572-authored MAE improvements therefore carry into
production; the combined experiment's 14% improvement does **not** describe
this default because dynamic refinement stays off.

Evidence: `reports/mandel-step-sampling-promotion-20260911/production-v2/`.
The first harness attempt stopped on a tuple/list dimension-comparison bug;
`production-v2` is the completed gate after correcting that reporting code.
The pre-promotion executable is retained locally as `fpt-metal-before` in the
same parent report. New production executable SHA256:
`698f2967631d743065e1b9101921a52f70e954560ef0aecb6a88b5e17ec294e1`.

The diagnostic beauty example explicitly restores the deterministic path call
before applying historical A/B controls, and labels that behavior in its
summary. Ordinary `fpt-metal render` uses the promoted path without an extra
flag. Published galleries/timing tables and NAADF/CVOX assets are not refreshed
by this change. Higher-SPP convergence, broad native appearance parity and
performance remain unproven; no commit or push is made.
Validation also passes four non-Mandel byte-exact controls (Cornell, glass,
Tower and typed CSG at 160x90/32 SPP), historical unchanged/sampled diagnostic
replays, and a scene572 profiling smoke with unchanged output. Evidence is in
the promotion report's `non-mandel/`, `diagnostic-controls/` and
`profile-smoke/` subdirectories. The profile smoke verifies execution/output,
not complete equivalence between profiler and beauty-integrator semantics.
165 Rust library tests, 19 diagnostic example tests, 99 Python tests, the
release build, formatting and whitespace checks pass.

### Repeat The Promotion Process

1. Inspect the screening previews and triage missing assets, blank captures,
   unsupported contracts, and slow cases separately.
2. Capture the next review batch at 300px max edge and 32 SPP, with fresh
   Mandelbulber CPU references at matching dimensions. Use the support suite's
   default three modes and supply `--mandelbulber-bin`.
3. Review geometry, framing and visible illumination against references.
   Retain source attribution and label omitted effects or unresolved differences.
4. Publish only the reviewed batch. Do not replace the accepted ranked-50 gallery
   with screening thumbnails or treat smoke-test timings as GPU benchmarks.

The existing ranked-50 publisher still enforces its original contract by
default. For an additional batch, `scripts/publish_mandel_gallery.py` accepts
`--batch-manifest` alongside a fresh support-suite report. It requires matching
source identities, all three capture modes, 300px/32-SPP FPT output, and CPU
references. Incomplete or provisional batches cannot be published. A review
gallery is not itself a supported-scenes or merge-readiness declaration.
