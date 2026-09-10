# Mandel main-light direction correction

This follow-up retains a CPU configuration correction after the
[main-shadow correction](mandel-main-shadow-correction.md). It is uncommitted
and has not been pushed. It is not an all-50-scene parity or performance claim.

## Cause and correction

Mandelbulber's camera-relative directional light rotates in its native XYZ
coordinate system. FPT previously swapped Y/Z before rotating. That swap has
negative determinant: it changes handedness, so applying the original axial
rotation signs afterward mirrors the horizontal light component.

For native forward +Y, top +Z and authored rotation (-45,45,0), the old FPT
direction was (+0.499988,+0.704645,-0.503475); the expected mapped direction is
(-0.499988,+0.704645,-0.503475). The regression failed on this sign before the
fix. It now covers both yaw signs and a rolled camera.

`MandelbulberScene::main_light_direction` now rotates in native space and maps
the resulting direction once. Mandelbulber's unusual 180.8 angle conversion
and directional pitch inversion are preserved, as verified in native
`src/light.cpp` and `src/algebra.cpp`. No shader, normal estimator, traversal,
material, sky, voxel-format or native NAADF implementation changed in this
follow-up. Shared Mandel appearance metadata receives the corrected direction,
but downstream voxel-export/NAADF captures were not regenerated in this pass.

This remains the existing camera-relative directional-light model. World-space
lights, target-point lights, nonorthogonal camera-top reconstruction and
auxiliary/random/fake sources are not newly implemented or certified here.

## Matched native controls

Native CPU and FPT retain the same scene camera and white direct-diffuse control
settings: no AO/specular/fog/clouds/DOF/other lights, with one FPT path vertex.
FPT uses 32-SPP pixel jitter; native sampling differs. Scene 14 is 200x200 and
scene 30 is 200x133. Errors are normalized RGB MAE, not geometry percentages.

| Scene | Control | Previous MAE | Corrected MAE | Reduction |
| --- | --- | ---: | ---: | ---: |
| 14 | No shadows | 0.204366 | 0.019117 | 90.65% |
| 14 | Hard penetrating | 0.227190 | 0.017310 | 92.38% |
| 14 | Soft penetrating, 5 degrees | 0.228036 | 0.014969 | 93.44% |
| 30 | No shadows | 0.226125 | 0.079766 | 64.72% |
| 30 | Hard penetrating | 0.161890 | 0.035625 | 77.99% |
| 30 | Soft penetrating, 5 degrees | 0.111809 | 0.020462 | 81.70% |

Visual review confirms that scene 14's central surfaces and scene 30's large
upper-right surfaces now light from the native side. Fine-scale differences
remain, particularly in scene 30's unshadowed image. Native jitter/dithering
can produce small run-to-run differences; the reductions above use fresh
baseline/candidate pairs against the same native capture.

## Normal and depth diagnosis

`scripts/run_mandel_normal_controls.py` compares native CPU EXR Z/nW.X/Y/Z
against the production-compiled FPT structural diagnostic's 80-byte records.
Native `render_worker.cpp` stores signed world XYZ directly, not display-mapped
normals. The harness swaps native Y/Z once, without image flips or alignment.
It reports normal angles separately where relative depth error is <=0.1%.
This is a diagnostic single-ray comparison, not the jittered beauty capture.

| Scene | Miss / extra pixels | Median relative depth error | Median normal angle | P95 normal angle |
| --- | ---: | ---: | ---: | ---: |
| 14 | 0 / 0 | 0.005331% | 0.455 degrees | 0.757 degrees |
| 30 | 0 / 0 | 0.000213% | 0.200 degrees | 1.488 degrees |

Normal angles in the table use the depth-matched subset: 39,795/40,000 joint
hits for 14 and 26,566/26,571 for 30. These results argue against a broad normal
orientation error. They do not exclude rare normal outliers or establish
identical subpixel sampling. No normal adjustment was retained.

## Regression and evidence

Four ordinary FPT scenes remain RGB byte-exact at 160x90 / 8 SPP:
Cornell box, Glass Ball, M4 and Render005. Neutral Mandel ranks 11/14/30 remain
RGB byte-exact at maximum axis 160 / 8 SPP. Original authored ranks 4/14 are
also byte-exact; 23/30 change under their authored light settings. Their
before/after sheet is not a native-reference parity score.

All 223 Rust tests (137 library, 59 CLI, 27 integration) and 36 Python harness
tests pass. Release build, formatting and whitespace checks pass. No timing
benchmark was run; this is a visual-correctness change.

The preserved baseline executable SHA-256 is
`3789e6d2fbae53493e82b94a5fbae72c71753efa77edceb67b0abdda1cb28bd8`.
The capture candidate SHA-256 is
`c38ceab163514420e68dff6dc3c70ba3607217b8208ab15b723fc193ce76f3fa`.
Reports contain commands, source hashes and image hashes where applicable.
The final build after test/format cleanup has SHA-256
`d535922bb37eb24ec4e1d628128d0ca179a2d788001c3ed1379e151e1cdd2b7e`;
fresh unshadowed captures for both scenes are RGB byte-exact to the capture
candidate (`reports/mandel-light-direction-final-build/summary.json`).

Local ignored evidence:

- `reports/mandel-light-direction-controls/comparison.png` and `summary.json`:
  native / previous / corrected white-light captures and RGB errors.
- `reports/mandel-light-normal-diagnosis/comparison.png` and `summary.json`:
  world normals, depth-conditioned angle errors, raw EXR/structural records.
- `reports/mandel-light-direction-regression/comparison.png` and `summary.json`:
  original authored changes and ordinary/neutral byte-exact regression checks.

## Next

The [sampling follow-up and light inventory](mandel-sampling-and-light-inventory.md)
now show that matching single-ray sampling removes most of scene 30's residual
control difference. Keep the current normal estimator and production AA.
Auxiliary directional sources in 21/42/50 are now implemented in the
[next follow-up](mandel-auxiliary-directional-lights.md), with mixed full-authored
results and a separate scene-42 normal outlier requiring investigation.
Scene 46's precision failure and fog/cloud transport remain separate. Re-run
the ranked-50 authored audit before updating its historical all-scene status.
