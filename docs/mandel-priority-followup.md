# Ranked-50 priority follow-up

This supplements the frozen `2e4917f` audit. It does not overwrite its original
47/50 full-size native-reference result or certify all-scene visual parity.
Production field evaluation, camera rays, shading and traversal are unchanged.
Fog and cloud implementation remains out of scope.

Subsequent implementation: [authored main-light shadow correction](mandel-main-shadow-correction.md).
The diagnosis and prototype counts below describe the pre-fix investigation;
the new document records the retained production change and its separate gates.

## Geometry controls

Fresh native CPU controls use white materials, an unshadowed camera headlight,
no AO/specular effects, and now explicitly disable coloured glow. The first
scene-32 control accidentally retained glow; it is superseded by the `-final`
report. No images are registered, flipped or cropped to improve alignment.

Unjittered depth records provide a separate check from the 32-SPP beauty images.
Native EXR Z is compared with FPT ray distance divided by the renderer's 1024
world scale. Median/p95 errors below use only pixels hit by both renderers;
misses/extras are reported separately, not hidden in the depth denominator.

| Rank | Size | Missing / extra pixels | Median relative depth error | P95 relative depth error |
| --- | --- | ---: | ---: | ---: |
| 11 | 300x225 | 0 / 0 | 0.000761% | 0.002809% |
| 22 | 300x200 | 0 / 0 | 0.000044% | 0.000367% |
| 32 | 300x169 | 0 / 0 | 0.002309% | 0.091362% |
| 46 | 300x169 | 7,470 / 0 | 0.014646% | 0.218027% |
| 09 | 300x169 | 0 / 0 | 0.000489% | 0.005566% |
| 49 | 300x169 | 8 / 5 | 0.000192% | 3.667233% |
| 17 | 96x96 | 5 / 138 | 0.000159% | 0.004226% |

Visual inspection and depth support **coarse geometry alignment for 9, 11, 22
and 32**. This is not exact depth parity: some discontinuities remain, including
514 shared-hit pixels above 1% error on scene 32. Scene 49 retains an appreciable
floor-depth difference despite its tiny median error. Scene 17 needs a larger
silhouette/threshold investigation; its low-resolution extra coverage is not
an appearance-only issue.

### Scene 46: confirmed position stalls

The depth sheet exposes a large missing lower-right region (14.73% of the
frame), not merely absent fog or an unhelpful headlight. A separate 20x20 GPU
probe using the production field and camera found:

- 338 surface hits;
- 62 `next_position == position` float32 stalls;
- zero non-finite, far-range or iteration-budget exits;
- zero found/miss disagreements between the instrumented loop and the original
  primary marcher on those 400 sampled rays.

This establishes lost representable displacement on the sampled missing rays.
Increasing the iteration count will not fix a position that cannot advance.
Scene 48's isolated precise-render example does not support this formula and
must not be presented as a production fix for scene 46.

## Missing references

Native CPU authored references now complete for all three previously timed-out
scenes at **96 maximum-axis pixels**, with Monte Carlo capped at 32 samples and
minimum samples set to 8:

| Rank | Size | Native wall time |
| --- | --- | ---: |
| 09 | 96x54 | 101.312 s |
| 17 | 96x96 | 116.889 s |
| 49 | 96x54 | 34.693 s |

Matching-size FPT neutral/authored images were also captured at 32 SPP.
These are broad-structure references, not replacements for the original
300-pixel authored quality gate. All 50 now have some native reference evidence;
only 47 have completed the original full-size authored reference run.

## Dark authored lighting

`examples/mandel_light_probe.rs` diagnoses the existing generated field without
changing the renderer. A 20x20 central-light probe found valid material colours
but every sampled shadow blocked on scenes 8, 14, 21, 23 and 30. In contrast,
control scene 4 retained 131 lit samples out of 147 surface hits.

Mandelbulber's `main_light_penetrating` and `main_light_cast_shadows` fields are
parsed, but the continuous FPT sunlight path does not consume them. Native
penetrating directional lights use a camera-distance/FOV-derived range and
partial attenuation; FPT currently treats an arbitrary blocker along the full
view range as total occlusion. Native's fog-free shadow step also retains the
surface threshold instead of FPT's changing camera-distance threshold.

A diagnostic-only fixed-threshold, bounded hard-shadow prototype gives:

| Rank | Production nonzero samples | Prototype nonzero samples |
| --- | ---: | ---: |
| 04 | 131 | 146 |
| 08 | 0 | 320 |
| 14 | 0 | 398 |
| 21 | 0 | 158 |
| 23 | 0 | 357 |
| 30 | 0 | 240 |

These counts are **not image improvements or accepted parity results**. The
probe bypasses the main-light enabled switch to isolate possible contribution;
scene 8 actually disables that light and relies on random lights. Scenes 21
and 23 also require additional sources. Soft-shadow cones are forced to zero
in this probe. Main-light shadow semantics, auxiliary/random/fake light sources
and their energy/material treatment need separate production gates.

Native source references: `src/light.cpp::CalculateLightVector`,
`src/render_worker.cpp::CalcDelta`, and `src/shader_aux_shadow.cpp` in the pinned
external Mandelbulber checkout. No external source files were edited.

## Retained tooling fixes

1. Geometry controls explicitly disable coloured glow; a regression test failed
   before this change.
2. Structural diagnostic CLI validation now matches the existing **80-byte**
   GPU/bridge record. The old CLI rejected valid dumps after capture because it
   expected 64 bytes. Manifest version 3 describes all five float4 fields,
   including incoming direction and segment distance. No GPU record changed.
3. The FPTVOX parity-sheet reader accepts validated version-2/64-byte and
   version-3/80-byte records, rejecting mismatched metadata and truncated files.
   Its new current-format regression failed before the reader fix. NumPy is
   included in the release-harness requirements because these tests use the
   existing NumPy-based comparison reader.

The CLI regression exercises actual production diagnostic kernels on Metal,
narrowed to the built-in Mandel field for a bounded compilation footprint.
Two initial generic-pipeline test attempts were stopped during expensive Metal
compilation; they are not counted as successful tests or renderer failures.

Final verification passed: `cargo +1.97.1 build --release --locked`, all 219
Rust tests (134 library, 58 CLI, 27 integration), all 30 Python harness tests,
and `git diff --check`. The corrected CLI reproduces scene 11's previously
captured 80-byte GPU records byte-for-byte and writes the valid v3 manifest.
Same-settings baseline/candidate authored renders for scenes 4 and 11 are
RGB byte-exact. These checks validate the tooling changes, not a new lighting
or precision implementation. Nothing was committed or pushed in this pass.

## Evidence

Generated captures remain ignored local research artifacts:

- `reports/mandel-priority-geometry-controls-final/comparison.png`: corrected
  white controls for 11, 22, 32, 46, 9 and 49.
- `reports/mandel-priority-reference-retries/comparison.png`: newly completed
  96-pixel native references alongside same-size FPT modes.
- `reports/mandel-priority-depth-analysis/comparison.png` and `summary.json`:
  shared-scale depths, miss/extra heat maps, distribution tails and input hashes.
- `reports/mandel-priority-depth/`: original native EXRs and 80-byte GPU dumps;
  the original failed CLI logs remain intact. Analysis explicitly records that
  the missing manifests were recovered using the existing producer layout.
- `reports/mandel-light-probes-bounded/`: raw three-record shadow probes.
- `reports/mandel-priority-exit-probe/46.json`: four-record primary-exit probe,
  source/shader hashes and full sampled data. Check each report's `layouts`
  field rather than assuming a stride across prototype revisions.
- `reports/mandel-priority-cli-fix/verification.json`: preserved baseline binary
  hash, current CLI verification and same-settings authored-output checks.

## Next implementation order

1. Implement native main-light cast/penetrating shadow semantics for authored
   Mandel rendering only. Gate direct-light-only images with AO, specular,
   fog/clouds and auxiliary lights disabled on both sides. Keep ordinary FPT
   scene rendering byte-exact.
2. Add supported auxiliary directional/point sources, then isolate random/fake
   sources separately. Do not compensate for missing sources with exposure.
3. Prototype precision-preserving primary positions for scene 46 with the same
   ray/depth controls. Do not turn stalled points into fabricated surface hits.
4. Investigate scene 17's silhouette excess and scene 49's floor-depth tail at
   larger resolution. Preserve the original small controls for regression.
5. Re-run the 50-scene authored audit after retained renderer fixes. Do not
   broaden library/support claims from capture completion alone.
