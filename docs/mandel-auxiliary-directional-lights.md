# Auxiliary directional lights

Follow-up to the [sampling diagnosis and light inventory](mandel-sampling-and-light-inventory.md).
This implements missing camera-relative auxiliary directional sources in
continuous authored FPT. It does not change voxel export or native NAADF.
Changes remain uncommitted and unpushed.

## Implementation and scope

- Parse modern enabled directional lights, preserving native rotation,
  per-light colour/intensity defaults, global intensity, shadow enable,
  penetrating mode and soft-shadow cone.
- Embed a deterministic, scene-specialized Metal light table. Light settings
  enter the generated source/cache identity; directions follow the live camera.
- Reuse the tested fog-free directional shadow walker. Sum contributions at
  the current path vertex using the existing material and throughput.
- Include supported and unsupported auxiliary settings in render metadata.
  Report unsupported point, legacy, world-space and target-point placements
  instead of silently treating them as supported directional sources.
- Reject an explicit metallib override and `--sdf-profile` for this new path:
  the existing profiling implementation does not count its shadow work.

Neutral Mandel and ordinary FPT paths do not acquire the light table. No
sampling, normal estimation, primary traversal or AA default was changed.
Per-bounce diagnostic contribution images also remain main-light-only and
must not be treated as a decomposition of the new authored production output.

## Isolated native controls

Light2-only white-material captures disable other sources, AO, specular and
fog. Native uses its CPU renderer; production FPT uses 32 SPP and one bounce.
Scenes 21/42 are 200x112; scene 50 is 200x200. Errors below are normalized
RGB MAE, not geometry errors or percentages of missing surfaces.

| Scene | Shadows | Previous vs native | Candidate vs native |
| --- | --- | ---: | ---: |
| 21 | Disabled | 0.187811 | 0.047967 |
| 21 | Hard penetrating | 0.064787 | 0.021281 |
| 21 | Soft penetrating | 0.041343 | 0.011640 |
| 42 | Disabled | 0.361395 | 0.070789 |
| 42 | Hard penetrating | 0.275796 | 0.046818 |
| 42 | Soft penetrating | 0.263257 | 0.044516 |
| 50 | Disabled | 0.442969 | 0.076680 |
| 50 | Hard penetrating | 0.161873 | 0.029086 |
| 50 | Soft penetrating | 0.137156 | 0.023100 |

All nine native/baseline/candidate captures completed. The old light2-only
images were black; the candidate restores the expected lit surfaces.
Analytic GPU tests additionally cover additive red/blue lights, independent
shadow flags, penetrating attenuation, clear/blocked fields, scale changes
and camera roll.

## Original authored output is still mixed

These captures retain the original scene settings, at maximum axis 160 and
32 SPP. They are not the isolated white-light controls above.

| Scene | Previous vs native MAE | Candidate vs native MAE | Interpretation |
| --- | ---: | ---: | --- |
| 21 | 0.564443 | 0.445980 | Improved, still far too dark overall |
| 42 | 0.212535 | 0.122091 | Missing blue directional illumination restored |
| 50 | 0.188835 | 0.255540 | Overall error worsens; brightness/material mismatch unresolved |

Visual inspection confirms scene 50 gains the source tint but is too bright
overall. Passing the isolated light controls does not establish full authored
parity, and no compensating exposure multiplier was introduced. Material,
environment, exposure and sampling/DOF must be isolated next before claiming
an all-scene appearance improvement.

## Sampling and scene 42 normals

The diagnostic sampling probe uses production `renderPath`, with its own
compiled entry point. Unshadowed probe/native MAE changes as follows:

| Scene | Jittered | Unjittered | Probe vs production MAE |
| --- | ---: | ---: | ---: |
| 21 | 0.047208 | 0.002704 | 0.000883 |
| 42 | 0.070649 | 0.091090 | 0.000427 |
| 50 | 0.076225 | 0.008991 | 0.000725 |

Unjittered 1/32-sample outputs are byte-exact. The probe passes the existing
0.001 MAE sanity gate, but is not byte-identical to production; localized
differences remain. Sampling explains much of 21/50's control error, not 42's.

Scene 42's signed-normal/depth diagnostic has zero misses and three extra
hits at 200x112. Median relative depth error is 0.092262%; P95 is 1.266133%.
Median normal disagreement is 9.212 degrees across joint hits. Restricting to
the 11,880 pixels with relative depth error <=0.1% still gives 7.620 degrees
median and 41.450 degrees P95. This warrants a separate normal/geometry
investigation, not an arbitrary light-direction correction.

## Regression and evidence

All ten unchanged-path captures remain RGB byte-exact: ordinary Cornell box,
Glass Ball, M4 and Render005; neutral Mandel 11/21/42/50; authored main-light
controls 14/30. The three newly lit authored scenes intentionally change.

Verification: 227 Rust tests (140 library, 60 CLI, 27 integration), 43 Python
tests, release build and whitespace checks pass. These are correctness runs,
not a performance benchmark. No speed or full-50-scene parity claim is made.

Baseline executable SHA-256:
`d535922bb37eb24ec4e1d628128d0ca179a2d788001c3ed1379e151e1cdd2b7e`.
Capture candidate SHA-256:
`c5e7dcc1be2023341adb6ee43131d7fc41fa9bb8178d1efdc5d966f5e19785e0`.

Local ignored reports contain commands, hashes, raw captures and JSON metrics:

- `reports/mandel-aux-directional-controls`: nine isolated controls and sheet.
- `reports/mandel-aux-directional-regression`: exact gates and original authored sheet.
- `reports/mandel-aux-directional-sampling`: matched-sampling diagnostics.
- `reports/mandel-aux-directional-normal42`: native EXR/FPT normal-depth comparison.

Reproduce the capture gates with `scripts/run_mandel_aux_controls.py` and
`scripts/run_mandel_aux_regression.py`, passing `--audit`, `--baseline`,
`--candidate` and a fresh `--output` directory. Sampling/normal harnesses retain
their existing interfaces. Do not overwrite historical evidence.

## Next

The [appearance/normal isolation follow-up](mandel-appearance-and-normal-isolation.md)
now separates scene 50's indirect-transport mismatch from missing post/specular
effects, fixes a chromatic-effect omission in the control harness, and measures
scene 42 normals at fixed points. It makes no production renderer change.

1. Isolate scene 50's remaining material/exposure/environment mismatch and
   scene 42's depth-conditioned normal errors; preserve the isolated light tests.
2. Add point sources separately, starting with 7/23: native attenuation,
   finite shadow distance, camera-relative positions and legacy migration.
3. Then address random/fake lights and rerun the ranked-50 authored suite.
   Scene 46 precision and deferred fog/cloud transport remain separate.
