# Sampling diagnosis and remaining authored lights

This follows the [main-light direction correction](mandel-light-direction-correction.md).
The subsequent [auxiliary directional implementation](mandel-auxiliary-directional-lights.md)
completes steps 1-3 below, with isolated-control improvements but mixed original
authored results. The inventory and executable hash here describe the earlier pass.
No production renderer, sampling default, geometry, voxel export, or NAADF
consumer was changed in this pass. Nothing was committed or pushed.

## Sampling explains most of the remaining control difference

The native white direct-light references use a single pixel ray, while FPT
averages jittered rays. Scene 30 contains many subpixel features. Comparing
these two sampling modes exaggerated the remaining shading/detail error.

The isolated `mandel_sampling_probe` example calls the existing `renderPath`
and post-processing functions. It uses the production GPU `sdfScreenUv`
calculation, one path vertex, no DOF, and either the original jitter strength
or zero jitter. The derived scenes disable AO, shadows, specular and other
lighting sources. Production source and sampling remain unchanged.

| Scene | Size | Jittered probe vs native MAE | Unjittered probe vs native MAE | Reduction |
| --- | --- | ---: | ---: | ---: |
| 14 | 200x200 | 0.018790 | 0.004822 | 74.34% |
| 30 | 200x133 | 0.079625 | 0.003680 | 95.38% |

Errors are normalized RGB MAE, not geometry percentages. The unjittered
1-sample and 32-sample images are RGB byte-exact for both controls. Visual
inspection confirms that scene 30's high-frequency, single-pixel detail
reappears when sampling matches native; production's smoother image is not
evidence that those surfaces were removed.

The probe is **not byte-identical to the production backend**. The jittered
sanity comparison has MAE 0.000461 on 14 and 0.000241 on 30. Respectively
216/40,000 and 83/26,600 pixels differ, with localized maximum errors of 0.5255
and 0.3412. This passes the declared 0.001 global-MAE probe gate but does not
rule out local compilation/accumulation differences. It supports the sampling
diagnosis, not an exact backend-parity claim.

Two preliminary probes were excluded: one omitted the CLI's one-bounce
configuration application; another calculated camera UVs on the CPU, changing
float rounding and jitter seeds. The final example applies the same camera
and optimization arguments as the CLI, asserts one bounce, and constructs
UVs with `sdfScreenUv` on the GPU.

Do not disable anti-aliasing to improve a reference score. Future visual gates
should use matched pixel sampling, or native anti-aliased references with a
clearly specified filter. Neither arbitrary image registration nor geometry
changes are warranted by this result.

## Authored light inventory

`scripts/audit_mandel_lights.py` inventories all 50 original source files,
recording source hashes and raw enabled-light settings. Code inspection shows
that continuous FPT `renderPath` adds the main sun and ambient lighting, but
does not add auxiliary direct sources. Parsed auxiliary fields and FPTVOX
appearance metadata do not constitute continuous-renderer support.

| Missing source category | Scenes | Explicit enabled sources |
| --- | --- | ---: |
| Camera-relative auxiliary directional | 21, 42, 50 | 3 |
| Auxiliary point | 7, 23, 29, 46 | 6 |
| Random lights | 8 | Separate generated-light system |
| Fake lights | 21, 40 | Separate orbit-based lighting |

There are seven scenes with nine explicitly enabled auxiliary sources.
Scene 4 has legacy auxiliary settings without an explicit enable flag; it is
flagged for native migration/default verification, not counted as enabled.
No enabled world-space or target-point **main** light was found in this set.
The inventory is a source/code audit, not a native-settings migration test or
a fresh all-50-scene visual certificate. Scene 46 also retains its independent
float32 position-stall failure.

## Next implementation order

1. Add a scene-specialized auxiliary directional-light table for authored
   continuous FPT. Parse light type, enable, color/intensity, camera-relative
   rotation and shadow flags; preserve native camera-basis and rotation rules.
   Do not interpret a directional light's stored position as a point source.
2. Reuse the validated directional shadow behavior. Gate light2-only controls
   for 21/42/50 with white materials, disabled AO/specular/fog/fake lights,
   first without shadows and then with hard/soft penetrating shadows. Separate
   matched-sampling diagnostics from 32-SPP appearance renders.
3. Gate original authored 21/42/50 and ordinary FPT/neutral Mandel captures.
   Keep ordinary and neutral output byte-exact. Report any intentional authored
   difference against native references before retaining it.
4. Add point lights separately: native distance attenuation, finite source
   distance, cast/penetrating flags, relative-position transforms, and legacy
   intensity migration. Start with 7/23 before deep-zoom 29/46. Do not use an
   arbitrary brightness multiplier to hide missing sources.
5. Address random/fake lights separately and rerun the ranked-50 authored
   suite. Fog/cloud transport remains deferred.

## Reproduction

```sh
cargo build --release --locked --example mandel_sampling_probe
python3 scripts/run_mandel_sampling_controls.py \
  --controls reports/mandel-light-direction-controls \
  --probe target/release/examples/mandel_sampling_probe \
  --output reports/NEW-sampling-controls
python3 scripts/audit_mandel_lights.py \
  --audit reports/mandel-release-ranked50 \
  --output reports/NEW-light-inventory/summary.json
```

Local evidence: `reports/mandel-sampling-final/comparison.png` and `summary.json`,
plus `reports/mandel-light-inventory/summary.json`. The sampling report includes
command logs, source/shader/binary hashes, image hashes, native/production
diffs and the single-sample consistency gate. These are diagnostic captures,
not performance measurements.

Verification: 223 Rust tests and 41 Python tests pass, as do the probe release
build, formatting and whitespace checks. The production executable remains
unchanged at SHA-256
`d535922bb37eb24ec4e1d628128d0ca179a2d788001c3ed1379e151e1cdd2b7e`.
