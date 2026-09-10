# Dark-scene lighting: 07, 08 and 40

This fixes missing surface illumination in continuous FPT Metal. It does not
change NAADF traversal, voxel representations, camera framing, exposure or
global path-tracing bounce defaults. The retained implementation is part of the
`release/fractal-library` review branch; rejected candidates remain local reports.

## Diagnosis and retained changes

- **07, clouds 004:** the authored main light is disabled and light2 is a point
  source. Continuous FPT previously skipped it. The new point-light table uses
  native inverse-distance decay choices, the native `100/6` intensity convention,
  finite source-distance shadow queries, and existing penetrating-shadow behavior.
  The newly illuminated floor exposed two further bugs: its normal was computed
  from the fractal alone, and its material inherited the fractal material. Normal
  queries now include the same primitive union used by traversal, and fixed-color
  plane materials are selected when the plane wins the distance comparison.
- **08, pseudo kleinian 002:** the source disables its main light and uses 20
  generated lights. A deterministic MT19937 placement pass samples the actual
  generated Metal distance field, accepts exterior near-surface positions and
  derives distance-scaled intensity. The resulting point lights are compiled into
  the authored scene's table. There is no partial CPU fractal evaluator or
  replacement camera light. Generation is bounded and errors rather than silently
  accepting unplaced lights. Float32 distance queries mean exact native light
  positions/colors are not certified.
- **40, orbitTraps 005:** the source disables its main light and uses three
  orbit-trap light channels. A separate shading-only orbit evaluator implements
  point-trap iteration ranges, inverse-square orbit sums, and a finite-difference
  light direction. It never supplies distances to primary or secondary traversal.
  This initial implementation supports standalone analytic point traps; unsupported
  orbit configurations remain explicit limitations.

For scene 40, treating fake lighting as a physical source at every diffuse bounce
washed out the image. A one-bounce control, compared with native volumes disabled,
showed that the surface calculation was useful but the added indirect propagation
was not appropriate for this source. Fake lights are therefore primary-hit
compatibility shading unless the source explicitly enables `MC_global_illumination`.
This is a limited compatibility policy, not full native reflection/transport parity.

## What remains different

Fog, cloud scattering and visible volumetric light halos are still absent. Native
scene 07 rendered with clouds disabled shows the cube and lit floor without its
cloud glow, confirming that surface lighting alone cannot reproduce the full
reference. Scene 40 similarly lacks the large blue volumetric halos.

Point-light direct specular, exact Monte Carlo finite-source soft shadows and
visible source volumes are not implemented here; the finite-distance shadow path
uses the existing analytic cone approximation. The authored light size is retained
as metadata, not a claim of area-light sampling. Palette-driven primitive materials,
iteration-threshold primitive-material selection and more complex fake-light
topologies need separate work. No exposure boost or per-scene brightness multiplier
was used to conceal these limitations.

## Visual evidence

`reports/mandel-dark-lights-accepted/comparison.png` has columns:
Mandelbulber authored reference / previous FPT / current FPT. All images are
300x169; FPT uses 32 SPP and unchanged scene-default bounces. Native references
are cached and are the same sources used in the preceding 50-scene review.

| Scene | Near-black pixels before | After |
| --- | ---: | ---: |
| 07 | 92.60% | 23.61% |
| 08 | 100.00% | 17.56% |
| 40 | 25.18% | 0.99% |

Near-black means 8-bit grayscale below 8. It measures the reported darkness,
not structural or physical parity. Reference backgrounds and absent volumes make
global image error a poor stand-alone acceptance criterion.

Other evidence:

- `reports/mandel-dark-lights-before`: reproducible small baseline captures.
- `reports/mandel-dark-lights-candidate1`: initial lighting experiment, including
  rejected multi-bounce fake-light propagation and incorrect floor normals.
- `reports/mandel-dark-lights-controls40`: native/FPT one-bounce authored and
  no-volume controls. No-volume RGB MAE was 0.124657 at 160x90; sampling and
  normals still differ, so this is not an exact lighting parity claim.
- `reports/mandel-dark-lights-native7-no-clouds`: native cloud-disabled reference.
- `reports/mandel-dark-lights-reviewed`: floor-normal fix before material correction.
- `reports/mandel-dark-lights-structural`: before/after hit and normal diagnostics.
  All three scenes have zero changed hit positions/depth, zero misses and zero
  extra hits. Scene 07 has 11,920 corrected normals at 160x90; 08/40 normals are
  unchanged.
- `reports/mandel-dark-lights-regression-final`: byte-exact geometry and authored
  outputs for 01, 02, 03, 14 and 42 against the preceding binary, at 160px/8 SPP.

## Verification

The release build passes. Tests cover point-light parser defaults, native coordinate
mapping, attenuation, finite shadow limits and world scaling on Metal; random
generator vectors/replay; fake-light channels and GI policy; plane material emission;
and the actual floor-normal bug on Metal. The floor test failed with a horizontal
fractal normal before the correction and passes with the upward plane normal.

All 234 Rust tests and 55 Python tests pass. The machine's default `rustdoc` is
older than `rustc`; the complete test run uses the matching Rust 1.97.1 `rustdoc`.
No performance claim or complete 50-scene visual sign-off is made by this change.

## Reproduction

Supply the completed ranked-50 audit and an external Mandelbulber checkout:

```sh
python3 scripts/run_mandel_dark_lights.py \
  --binary target/release/fpt-metal \
  --audit reports/mandel-merge-review50-20260910/final/summary.json \
  --mandel-root "$MANDELBULBER_ROOT" \
  --size 300 --samples 32 --output reports/dark-lights-fresh
```

The [ranked-50 gallery](mandel-gallery/README.md) refreshes the complete set after
these fixes. It retains the production precision and unsupported-volume warnings.

The fresh full set has 48/50 byte-identical neutral images against the preceding
50-scene sheet. Only 07 and 49 change; both contain primitive planes. A new
300x169 scene-49 structural control has zero changed hit positions/depth, zero
misses and zero extras, with 4,772 changed normals and 4,738 changed material
colours. Evidence is in `reports/mandel-gallery49-structural`. This distinguishes
the floor shading correction from a traversal or framing change.
