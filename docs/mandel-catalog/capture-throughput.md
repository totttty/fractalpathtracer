# Gallery Capture Throughput

This is a review workflow optimization, not a renderer GPU benchmark. Existing
gallery images, accepted decisions, renderer binaries and shading are unchanged.

## Pilot Results: 2026-09-11

Scenes 075, 626 and 628, authored aspect/camera, 300px maximum edge, FPT
32 SPP, geometry + authored + native CPU. Same renderer binaries and render
commands; order serial / overlap / overlap / serial. No native sampling changes.

| Capture workflow | Whole three-scene elapsed time |
| --- | ---: |
| Serial, two runs | 126.15 / 135.82 s |
| Overlap, two runs | 73.05 / 77.31 s |
| Median serial -> overlap | 130.99 -> 75.18 s (-42.6%) |
| Native-only cache fill, 300px | 77.25 s |
| Native-only validated cache hits, 300px | 0.55 s |
| Native-only reduced-resolution control, 200px | 29.66 s |

All FPT RGB outputs were byte-exact across the four scheduling runs. Cached
native PNGs were byte-exact to their fresh originals. Fresh native renders
are not deterministic: normalized native MAE versus serial1 was
0.00094/0.00478/0.02271 on serial2 and
0.00089/0.00483/0.02300 on overlap1, respectively for 075/626/628. Visual
inspection found the same broad framing/structure; these are noise-comparable
controls, not proof of byte-exact native rendering. The 200px run preserves
broad framing but loses inspection detail. Its single-pass timing is preliminary.

A separate one-second-budget test deferred all three references, returned a
nonzero suite status, and left no FPT/native renderer processes running. This
exercises timeout handling without spending six minutes waiting on a 120-second
budget. Unit tests additionally cover stale inputs, invalid provenance, corrupt
images and resume/concurrency handling (125 Python tests passed).

Raw runs, commands, logs, PNGs, summary hashes and the comparison sheet are in
`reports/mandel-throughput-pilot-20260911/`; `results.json` records measurements
and hashes. Pilot images are not added to the reviewed gallery. Overlap remains
opt-in rather than claiming these three scenes establish safety on all 746.

## Screening Budget

`scripts/run_mandel_support_suite.py` defaults to a **120-second native CPU
timeout**, independently of the **900-second FPT timeout**. A native timeout
remains an incomplete triplet, not an unsupported scene or an accepted image.
`deferred-native.json` records source hashes and budgets. Supply it to the next
selector with `--deferred <capture-output>/deferred-native.json`; retain it for
the final outlier batch. No automatic slow retry occurs.

Use `--native-timeout 900` only for an intentional later slow-reference run.
Resume validates the complete run identity: do not change the budget or
scheduler under an existing summary; use a new output directory instead.

## Reference Reuse

Opt in with `--reference-cache reports/mandel-native-reference-cache` and reuse
that directory across batches. Entries bind:

- Full source hash, including camera, projection and authored parameters.
- Exact dimensions and normalized native command, including override settings.
- Native executable hash, native preferences and an environment digest.
- Shared/bundled texture and material resource hashes and the resolved lightmap.
- Successful CPU output, image hash/dimensions, command and stdout/stderr hashes.

Timeouts, failed captures, corrupt entries and incomplete contracts are never
reused. Explicit external asset paths with unproven native resolution bypass
the cache. Older capture summaries did not fingerprint enough dependencies;
they cannot be retroactively certified just because their pixels still exist.
This cache starts with verified fresh captures rather than trusting legacy PNGs.

Cache hits copy original evidence locally, report zero render wall time and
retain the original capture duration separately. They still require visual
review. Cache keys exclude output paths and timeout budgets, which do not
change completed rendering. FPT SPP does not affect native reference identity.

## Bounded Overlap

`--overlap-native` starts one native CPU reference while the main lane renders
FPT geometry and authored modes sequentially. No next scene starts until both
lanes finish. Only the main thread writes summary/progress files. Mode failures
remain independent, and resume executes only missing modes.

Overlap remains opt-in: CPU and GPU share memory/bandwidth, and a three-scene
pilot cannot establish safety on every fractal. The existing 1 GiB free-disk
guard applies to output and temporary volumes. Use an external `TMPDIR` with
headroom. This is not a RAM usage cap, nor a guarantee against heavy-scene OOM.

## Lower Resolution And Samples

`--max-axis 200` is already supported and keeps authored aspect ratio/camera.
For a square scene it has 44.4% as many pixels as 300px, but runtime does not
necessarily scale linearly. Use it for preliminary triage. The reviewed-gallery
publisher continues to require 300px/32 FPT SPP; it will reject reduced-size
batches rather than silently mix them into the existing evidence.

Native CPU sampling is not the FPT `--samples` setting. In Mandelbulber's
`src/render_worker.cpp`, `DOF_monte_carlo` uses `DOF_samples`; enabled
antialiasing additionally multiplies repeats by `antialiasing_size` squared.
The native default MC count is 100, with authored scenes sometimes using
200-500. Disabling the MC mode can also change lighting, so it is not a safe
shortcut to the same reference. A separately labelled lower-MC-sample control
can retain the mode and effects but will be noisier. Do not reduce native
sampling invisibly or present it as the unchanged authored reference.

The support suite now provides `--native-mc-samples 16` or
`--native-mc-samples 32` for this labelled experiment. This is an upper cap, not
fixed SPP: it only overrides `DOF_samples` when MC is already enabled and the
authored count is higher. `DOF_min_samples` is lowered only if it would exceed
the new maximum. Authored MC enablement, GI, noise threshold, DOF, antialiasing,
lights, fog, materials and camera remain unchanged. Scenes with MC disabled
receive no overrides, including scenes with unused zero-valued sample fields.

The exact overrides and authored/effective bounds are recorded per scene in
`summary.json`; sheet headers identify the cap. Cache keys include the effective
command, so reduced samples cannot reuse full-sample references or vice versa.
The gallery publisher rejects capped batches and sampling overrides until a
separate reduced-sample evidence policy is deliberately adopted. Defaults remain
300px, 32 FPT SPP and authored native sampling. This option alone does not promote
scenes or replace their accepted reference images.

In batch03, 11 of the 13 references taking more than 120 seconds or timing out
enabled Monte Carlo sampling. Thus a labelled sample-budget experiment is
more targeted than changing FPT shader code. Colours being acceptable does
not waive geometry/framing and illumination review.

## Native Sampling Pilot: 2026-09-11

Three MC-heavy scenes, 300px max edge, authored aspect/camera, CPU renderer,
120-second budget per capture. No FPT rerender and no full-sample retry. Each
completed command was checked against its archived native command: only output
location and `DOF_samples` changed. Source, executable and reference image
hashes were verified. Existing full-sample timings are historical, not fresh
paired baselines; their older asset provenance limitations still apply.

| Scene | Authored MC maximum | Historical reference | Cap 16, fresh | Cap 32, fresh |
| --- | ---: | ---: | ---: | ---: |
| 398 aboxmod15 | 100 | 606.0 s | 38.2 s | Timeout at 120 s |
| 627 difs_tree | 100 | 237.7 s | 67.6 s | 52.0 s |
| 411 bug | 200 | 435.9 s | 31.0 s | 71.8 s |

All three cap-16 captures and two cap-32 captures completed. The cap-32 scene
398 timeout remains recorded, not replaced with a partial image or retried.
These are single captures on a live machine with stochastic/adaptive work;
scene 627 being slower at 16 than 32 illustrates why these results do not prove
linear sample scaling or establish a reliable per-scene speedup percentage.

Visual findings from the unmodified PNG comparison:

- **398:** cap 16 preserves broad framing, but dense noise obscures the finer
  ridges/openings. It is a coarse triage image, not adequate detailed evidence.
- **627:** broad geometry, placement, lighting and sky remain readable. Cap 32
  visibly reduces grain relative to 16 and is the strongest screening result.
- **411:** silhouette and major refracted forms remain, but chromatic/DOF noise
  is conspicuous at 16 and still present at 32. Detailed judgment needs the
  existing higher-sample reference.

Keep authored native sampling as the reviewed-gallery default. Lower caps are
useful optional triage controls, not a universal replacement for references.
The timeout/cache/overlap improvements do not require this quality trade-off.
No gallery decisions or accepted images were changed. Cap parsing was checked
on all 746 source scenes (102 enable MC); 128 Python tests passed, including
sampling controls and rejection of reduced-sample gallery evidence.

Raw commands, summaries, timings, source/reference hashes and the comparison
are in `reports/mandel-native-sampling-pilot-20260911/`. `results.json` includes
pixel differences as noise diagnostics, not automatic geometry acceptance.
