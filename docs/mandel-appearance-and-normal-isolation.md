# Scene 50 appearance and scene 42 normal isolation

This follows the [auxiliary directional-light implementation](mandel-auxiliary-directional-lights.md).
It adds diagnostic controls, not a production shading or traversal adjustment.
No changes have been committed or pushed. Native NAADF and voxel export are
unchanged.

## Scene 50: different light transport is the dominant brightness cause

The original authored mismatch was reproduced with the unchanged production
executable. The controlled sweep keeps the scene, camera, palette and authored
light settings fixed. Each comparison identifies its parent configuration.
Native is the real CPU renderer; FPT uses 32 SPP at 160x160.

| Configuration | Parent | Normalized RGB MAE vs native |
| --- | --- | ---: |
| Original authored | None | 0.255422 |
| Chromatic post-effect disabled | Original | 0.254989 |
| DOF disabled | Original | 0.292759 |
| Both post-effects disabled | No chromatic | 0.293052 |
| FPT capped to one bounce | No post-effects | 0.073916 |
| Direct diffuse: specular/reflection disabled | One bounce | 0.057174 |
| Brightness/contrast/gamma set to identity | Direct diffuse | 0.045452 |

With post-effects disabled, changing only FPT's bounce cap reduces error by
74.8%. Visual inspection confirms that excessive interior illumination drops
and the large surfaces acquire shading much closer to native. The current
FPT authored path adds indirect illumination; the native scene is not using
the same diffuse global-illumination integrator. Adding a previously missing
auxiliary source made this transport difference more visible.

This is **not evidence that indirect illumination should be disabled in the
path tracer**. No global brightness compensation, material dimming or bounce
default change was retained. Use one bounce for direct-light parity tests,
and compare path tracing against a separately configured native GI reference.

Other isolated observations:

- Disabling native chromatic effects changes native RGB MAE by 0.041155;
  FPT is byte-exact. It does not reproduce this post-effect.
- Disabling native DOF changes native RGB MAE by 0.090197; FPT is byte-exact.
  These native post-DOF settings are not implemented by this FPT path.
- Disabling specular/reflections changes the native one-bounce control by
  0.021357; FPT is byte-exact. Its stochastic reflective continuation is not
  equivalent to native direct specular shading, especially with one bounce.
- Repeating the identical native no-post scene for the bounce-only row changes
  native RGB MAE by 0.002632. Native captures are not byte-deterministic; small
  differences must not be overinterpreted. The transport difference is much
  larger than this observed repeat variation.

## Corrected white-light control

The earlier geometry-control generator disabled DOF but omitted
`post_chromatic_aberration_enabled`. A new test reproduced that omission,
then passed after the override was added. This fixes the test reference, not
the renderer. Historical control reports remain immutable.

Fresh scene-50 light2-only controls at 200x200 still pass:

| Shadows | Previous FPT vs native | Current FPT vs native |
| --- | ---: | ---: |
| Disabled | 0.442337 | 0.076532 |
| Hard penetrating | 0.162477 | 0.029117 |
| Soft penetrating | 0.137360 | 0.023105 |

With post-effects truly disabled, matching the native single-ray sampling
reduces the unshadowed probe error from 0.076076 to 0.008131. Unjittered
1/32-sample probe outputs are byte-exact. Probe/production MAE is 0.000725;
localized compiler/sampling differences remain (maximum 0.556863). This is
not an exact production-backend parity claim.

## Scene 42: location explains part, not all, of the normal mismatch

`mandel_normal_probe` calls the production `normalAt` at explicit points,
without marching. The existing 200x112 native EXR and production structural
dump supply 22,397 joint-hit pixels. A separate probe kernel is used, and its
same-point comparison against production normals is measured first.

| Evaluation position | Median normal error vs native | P95 |
| --- | ---: | ---: |
| Actual FPT hit | 9.212 degrees | 98.257 degrees |
| Native depth along FPT ray, rounded FPT camera | 7.933 degrees | 35.978 degrees |
| Native depth along FPT ray, source camera | 7.871 degrees | 35.418 degrees |

At actual FPT hits, the probe matches production normals to a median angle of
zero and P95 0.00000121 degrees. Repositioning reduces the large-error tail,
but leaves a substantial median discrepancy. These are **approximate native
positions**: they use native depth but FPT directions and float32 point input,
not an authoritative native point/evaluator comparison.

None of the tested normal offsets is smaller than the largest coordinate ULP.
The median ULP/normal-offset ratio is 0.017806, P95 0.031031. A collapsed
finite-difference stencil is therefore not supported as the primary cause.
This does not rule out float32 error inside the fractal evaluator.

No normal smoothing, epsilon multiplier, camera registration or traversal
patch was retained. The next useful experiment is to compare native and Metal
distance samples at identical six-point normal stencils, with explicit
float64-to-float32 rounding controls. That separates formula arithmetic from
hit location before changing either algorithm.

## Reproduction and evidence

```sh
python3 scripts/run_mandel_appearance_ablation.py \
  --audit reports/mandel-release-ranked50 --binary target/release/fpt-metal \
  --scene 50 --output reports/NEW-appearance50
cargo build --release --locked --example mandel_normal_probe
python3 scripts/run_mandel_normal_position_probe.py \
  --diagnostic reports/mandel-aux-directional-normal42/42 \
  --probe target/release/examples/mandel_normal_probe \
  --output reports/NEW-normal-position42
```

Local evidence, including command logs, source/binary hashes and metrics:

- `reports/mandel-appearance-ablation50/{summary.json,comparison.png}`
- `reports/mandel-aux-controls-clean-post50/{summary.json,comparison.png}`
- `reports/mandel-aux-clean-post-sampling50/{summary.json,comparison.png}`
- `reports/mandel-normal-position42/{summary.json,samples.json,points.json}`

The production executable is unchanged at SHA-256
`c5e7dcc1be2023341adb6ee43131d7fc41fa9bb8178d1efdc5d966f5e19785e0`.
Its prior ten exact-output gates remain applicable; no production code was
edited in this follow-up. The new normal diagnostic example builds, all 227
Rust tests and 47 Python harness tests pass, and whitespace checks pass.
No timing benchmark was performed.

## Next priorities

The [identical-point follow-up](mandel-identical-point-stencils.md) now reproduces
scene 42's evaluator discrepancy without camera rays. Its derivative-step sweep
does not produce a reliable fix; production remains unchanged.

1. Keep direct-diffuse parity and path-traced beauty as separate labelled
   comparisons. Match sampling and post-effects explicitly in new sheets.
2. Compare scene 42's native/Metal normal-distance stencils at identical
   positions before attempting a numerical fix.
3. Implement native direct specular and remaining point lights as independent
   features with isolated controls. Do not use them to compensate for different
   indirect transport. Fog/clouds and deep-zoom precision stay separate.
