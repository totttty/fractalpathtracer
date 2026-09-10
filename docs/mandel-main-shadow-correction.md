# Authored Mandel main-light shadow correction

This is a retained working-tree correctness change, not a performance claim or
an all-scene appearance-parity certification. Nothing has been committed or
pushed in this pass.

## Implementation

The continuous authored Mandel path now consumes the already-parsed main-light
cast-shadow and penetrating-light flags. Reserved configuration lanes 6..8
carry penetrating, cast-shadows and the camera/FOV range factor; the C/Metal
configuration layout is unchanged. An explicit CLI camera-FOV override updates
the new range factor as well.

The directional visibility query:

- returns full visibility when shadows are disabled;
- keeps the primary surface threshold fixed along a fog-free shadow ray;
- uses camera distance times Mandelbulber's internal FOV for penetrating
  lights, and the scene view limit otherwise;
- attenuates a penetrating blocker by its distance within that range instead
  of treating it as completely opaque;
- evaluates deterministic soft-cone occlusion when the cone is nonzero and
  iteration-threshold mode is off;
- uses bounded iterations and fails dark on non-finite/stalled/unresolved
  queries rather than claiming an unproven clear ray.

The existing ordinary-FPT sunlight implementation remains the separate
non-Mandel branch. Primary traversal, geometry, palette evaluation, AO, sky,
voxel formats and the native NAADF consumer were not changed by this fix.

The native behavior was checked against `CalculateLightVector`, `CalcDelta`
and `AuxShadow` in the external Mandelbulber source. This implements their
fog-free directional-light subset, **not** fog/cloud/participating-medium or
subsurface shadow transport, interior-mode shading, auxiliary/random/fake
lights, or Mandelbulber's Monte Carlo DOF light-sampling mode. Those authored
scenes currently receive the deterministic fog-free treatment, not a promise
of complete native equivalence. The conservative iteration-exhaustion behavior
is deliberately stricter than native's accumulated-visibility return.

## Matched native controls

`scripts/run_mandel_shadow_controls.py` creates derived control sources without
editing originals. It records source/control/binary hashes, commands, settings,
images and errors. Controls use white diffuse materials, one camera-relative
directional light, no AO/specular/glow/fog/clouds/DOF/other lights, and one FPT
path vertex. They preserve authored camera/fractal parameters. Native is CPU;
FPT uses 32 SPP. Native sampling is not claimed identical to FPT pixel jitter.

| Scene | Control | Before RGB MAE | Corrected RGB MAE | Reduction |
| --- | --- | ---: | ---: | ---: |
| 14 | Hard penetrating | 0.503024 | 0.227120 | 54.85% |
| 14 | Soft penetrating, 5 degrees | 0.489728 | 0.227979 | 53.45% |
| 30 | Hard penetrating | 0.239573 | 0.161890 | 32.43% |
| 30 | Soft penetrating, 5 degrees | 0.142303 | 0.111808 | 21.43% |

Sizes are 200x200 (14) and 200x133 (30). MAE is on normalized RGB in [0,1],
not a percentage or a geometric metric. The non-penetrating hard-shadow controls
are black in all three renders and byte-exact, but that trivial black-image
agreement is not evidence of structural parity.

The additional shadows-disabled controls improve from 0.630694 to 0.204381
(14), and 0.359013 to 0.226123 (30). Their remaining error proves that not all
remaining appearance differences come from shadow visibility. Light direction,
normal response, surface detail and sampling need a separate diagnosis.

Visual inspection confirms the penetrating controls recover actual lit
surfaces from the formerly black images. Important local differences remain,
including native's bright upper-right surfaces in scene 30 and the shading of
the central surfaces in scene 14. Lower global MAE does not certify these as
resolved.

## Original authored captures

Fresh baseline/candidate pairs for ranks 4, 14, 23 and 30 used their original
authored sources at 160 maximum-axis pixels and 32 SPP, with scene-default
bounces. Scenes 14, 23 and 30 recover substantial illumination. Scene 4 remains
visually similar. These before/after images are not native-reference metrics;
the matched white controls above isolate the change more carefully.

## Regression gates

- The parsed-setting regression failed before the fix because the GPU flags
  stayed zero.
- The analytic GPU regression failed before the fix because disabling shadows
  still produced zero direct light. It now covers hard blocked/clear rays,
  disabled shadows, partial penetrating attenuation and soft-cone attenuation
  at thresholds 0.00001, 0.01 and 10 through the production sunlight call.
- The camera-FOV override regression also failed before its consistency fix.
- Four ordinary FPT scenes are RGB byte-exact against the preserved executable:
  `03-Cornell_box`, `04-Glass_Ball`, `07-M4`, `01-Render005`, at 160x90 / 8 SPP.
- Neutral Mandel geometry for ranks 11, 14 and 30 is also RGB byte-exact at
  160 maximum-axis pixels / 8 SPP.
- All 222 Rust tests (136 library, 59 CLI, 27 integration) and 33 Python harness
  tests pass, along with the release build, formatting and whitespace checks.

The reference executable for this pass has SHA-256
`bac2fd66cd251d4df973f7299a68dfa95695c77df6180f6f1d8147cded7a9cbf`.
Capture reports record the candidate executable hashes. The final CLI-FOV
consistency update is covered separately by the configuration regression;
the stored scene pairs do not use that override.

Generated evidence remains in ignored local reports:

- `reports/mandel-main-shadow-controls/comparison.png`: native/before/corrected
  matched hard and soft controls, with raw measurements in `summary.json`.
- `reports/mandel-main-shadow-unshadowed-controls/comparison.png`: native and
  FPT controls with cast-shadows disabled.
- `reports/mandel-main-shadow-regression/comparison.png`: original authored
  before/after images; `summary.json` also contains ordinary-FPT byte hashes.
- `reports/mandel-main-shadow-geometry-regression/summary.json`: separate
  neutral-geometry baseline/candidate captures.

## Next

The subsequent [light-direction diagnosis and correction](mandel-light-direction-correction.md)
confirmed a handedness error in the CPU rotation conversion and substantially
reduced the remaining control errors. Normals and primary depth agree closely
in the two measured scenes. Keep scene 46's float32 position-stall work separate.
The old 50-scene counts remain historical until that larger gate is repeated.
