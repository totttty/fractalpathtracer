# Release preparation

This branch prepares FPT Metal for two workflows: continuous Mandelbulber
rendering and reusable generation of voxel assets for native NAADF. It is not
yet a release-certified CVOX library.

## Preserved checkpoint

Research commit `0387d0b` preserves the previously dirty authored-appearance and
surface-diagnostic implementation on `codex/fractal-library-api`. The permanent
local ref `archive/fpt-research-20260909` preserves that checkpoint. Original
reports, experiment directories and caches were left in place; an independently
verified archive and Git bundle were saved outside the repository. Nothing has
been pushed or merged into main.

Release work is on `release/fractal-library`. It starts from the complete
checkpoint to preserve executable output during validation. Consolidate onto
current origin/main only after the release gates are satisfied; do not rewrite
the original research history.

## Reproducible local checks

Use the pinned Rust 1.97.1 toolchain through rustup; ensure both rustc and rustdoc
come from that toolchain. A mixed Homebrew rustdoc and rustup rustc failed the
initial audit despite the ordinary tests passing. Xcode with working Metal
compiler tools is required, not just a C compiler.

```sh
cargo +1.97.1 fmt --all -- --check
cargo +1.97.1 test --release --locked
cargo +1.97.1 doc --no-deps --locked
python3 -m pip install -r scripts/requirements-release.txt
python3 -m unittest discover -s scripts -p test_release_canaries.py
```

The GitHub workflow compiles tests but deliberately does not claim to exercise
Metal rendering on hosted runners. The full test suite and the canary captures
must run on a physical Metal-capable Mac. Publishing still requires dependency,
asset and generated-formula redistribution review.

## Fresh canary captures

The manifest `tests/fixtures/mandel-release-canaries.json` identifies the first
five ranked scenes plus ranks 13 and 17 by source hash and relative path. Scene
files remain in the external Mandelbulber examples tree with their original
attribution. No personal absolute path is embedded in the manifest or runner.

```sh
python3 scripts/run_release_canaries.py \
  --scene-root "$MANDEL_EXAMPLES" \
  --mandelbulber-root "$MANDEL_SOURCE" \
  --mandelbulber-bin "$MANDEL_BINARY" \
  --fpt target/release/fpt-metal \
  --output reports/release-canaries
```

Pass `--mandel-lightmap "$MANDEL_LIGHTMAP"` to pin the AO texture used by native
references. The runner records its SHA-256, includes it in baseline settings,
and verifies it before and after each native capture. This option affects
Mandel references only, not FPT. Without it, the report explicitly marks
external reference textures as unpinned. Other textures remain untracked even
when this AO lightmap is pinned.

The runner requires an empty output directory and captures new original
Mandelbulber CPU renders, FPT neutral geometry views and FPT authored-path views.
The maximum image axis is 300, aspect ratio is authored, and FPT uses 32 SPP.
Mandelbulber's authored integrator is not presented as equivalent to FPT's
32-SPP path integrator. There are no image flips, crops or framing corrections
after rendering. FPT bounce settings remain scene/config defaults and are
available in raw render reports.

One sample per accumulation command avoids imposing eight heavy samples in one
GPU command on interactive macOS. It changes scheduling, not requested sample
count; this is an explicit harness setting, not a new renderer default.

Every command, stderr/stdout log, scene hash, binary hash and RGB image hash is
recorded. The contact sheet marks failed captures. Execution success is not
visual parity. The appearance MAE is descriptive, not an acceptance threshold.
An optional `--baseline previous/summary.json` requires matching scene/settings
and byte-exact FPT RGB captures, intended for the later library extraction.

## Ranked-50 support audit

The broader support gate uses `tests/fixtures/mandel-release-ranked50.json`.
This is the historical ranked-50 ordering with full source-relative paths and
SHA-256 hashes, not filename-only matching. Rank 49 has two different upstream
files with the same basename. This manifest selects the collection scene
corresponding to the historical FPT capture; the old sheet appears to have used
the unrelated root-level file for its Mandel reference. Both new renderers
receive exactly the same pinned source.

```sh
FPT_MANDEL_TILED_DISPATCH=1 FPT_MANDEL_TILE_ROWS=32 \
python3 scripts/run_mandel_support_suite.py \
  --scene-root "$MANDEL_EXAMPLES" \
  --mandelbulber-root "$MANDEL_SOURCE" \
  --mandelbulber-bin "$MANDEL_BINARY" \
  --fpt target/release/fpt-metal \
  --output reports/mandel-release-ranked50 \
  --max-axis 300 --samples 32 --timeout 900
```

The runner is serial, preserves authored aspect, uses one sample per tiled
command, and records each mode independently. A reference failure cannot hide
FPT geometry or authored results. A timeout means the 15-minute command budget
was exhausted, not proof of unsupported geometry. Failed-image diagnostics
remain visible but do not count as successful renders.

Use the same command with `--resume` after interruption. Scene, executable,
lightmap, harness and settings identities must match; saved image/metadata
hashes are checked. Interrupted attempts are preserved in separate directories.
The runner stops below 1 GiB disk headroom. Each completed command updates
`summary.json` and `support.csv`; five ten-scene PNG pages support visual review.

Authored custom AO maps are resolved and pinned independently (including
scene 25's `lightmap2.jpg`), never replaced with the default map merely to make
a render run. Other external textures remain outside this asset audit. The
reference retains authored effects, including volumes/DOF that FPT may not
implement, so appearance MAE is not an isolated geometry metric. Compilation
success is established by a successful production render; timeouts and
unsupported scene contracts are reported separately. Geometry/appearance
fidelity remains explicitly unreviewed until the images are inspected.

### Completed checkpoint audit

Renderer checkpoint `2e4917f`, harness `86063a2`. All 50 scene attempts and
manual reviews of the available images are complete. Detailed per-scene findings, input hashes
and raw-artifact hashes are in
[`mandel-ranked50-review.json`](mandel-ranked50-review.json).

| Check | Result |
| --- | --- |
| FPT compilation and neutral geometry capture | 50/50 |
| FPT authored capture, first pass | 49/50 |
| FPT authored capture, after one identical-settings retry | 50/50 |
| Fresh native CPU reference | 47/50 |
| Coarse structure/framing visually consistent | 40/50 |
| Confirmed structural/composition outliers | 25, 38, 48 |
| Geometry detail still uncertain | 11, 22, 32, 46 |
| Reference unavailable after 900-second timeout | 09, 17, 49 |
| Authored appearance: close / different / major gap / unassessed | 5 / 17 / 25 / 3 |

These are execution and qualitative review counts, **not exact parity gates**.
"Coarse" means large-scale structure at 300 maximum axis; it does not certify
fine details or unseen geometry. Only FPT uses 32 SPP here. Native references
retain their source Monte Carlo/DOF settings, which can be much more expensive.

Scene 31 initially failed at sample 10, row 96 with a Metal GPU-recovery
`InnocentVictim` error. Its identical-settings retry completed and produced a
reviewable image. The original failure remains recorded; one successful retry
does not establish that recovery failures cannot recur.

Additional native white-material controls for 25, 38 and 48 disabled volumes,
AO, specular/reflections, DOF and palette colour without changing camera or
formulas. Their structural/composition mismatch persists. The controls retain
native direct-light directions, so they are not pixel-equivalent lighting
references. Scene 48 is also an extreme-scale diagnostic candidate: its
camera-to-target separation is about `3e-8` at coordinates near `20`. This is
evidence to investigate precision, not a proven diagnosis.

The five final local sheets are
`reports/mandel-release-ranked50/reviewed-pages/page-01.png` through
`page-05.png`; scene 31 is labelled `ok_after_retry`. Original first-pass sheets,
logs, commands, failed outcomes, `followup.json`, `visual-review.json`,
`support-reviewed.csv` and the
white-control sheet `geometry-controls.png` remain alongside them. Generated
images are not shipped in the source package. The follow-up driver and its
hash are retained with those local artifacts.

Next work, in order:

1. Isolate geometry/camera/evaluator differences in 25 and 38, and precision
   limits in 48, using matched neutral depth/normal probes before changing
   appearance. Do not assume every disagreement is a camera flip.
2. Keep the scene-31 recovery case as a repeatability canary for tiled dispatch.
3. Diagnose non-volume appearance failures separately: dark authored scenes
   such as 14/21/30, and palette/material differences such as 36. Fog and clouds
   remain deferred; do not hide their absence by changing reference settings.
4. Add neutral native controls for 11/22/32/46 and finish native references for
   09/17/49 with an explicit larger budget or clearly labelled reduced-quality
   follow-up. Do not substitute old filename-only references.
5. Expand the current-checkpoint execution audit to the larger corpus, then
   continue the typed production library/CVOX work below. This audit did not
   test CVOX or NAADF export and does not certify all Mandel scenes.

## First discovered correctness issue

Mandelbulber's modified-parameters files may omit image width/height. The checked
upstream `src/initparameters.cpp` defaults to 800x600, whereas FPT defaulted to
1920x1080. This changed the source aspect contract for scenes such as DIFS
Cylinder. A new parser regression test fails before the fix and passes after it.
Explicitly specified dimensions remain unchanged.

The fixed parser passes 208 Rust tests. Thirteen completed FPT captures from the
seven-scene run were byte-exact against the checkpoint under explicit capture
dimensions; the only missing capture was rank 17 authored mode. The canary harness
supplies dimensions explicitly, so that comparison proves non-regression, not the
effect of the new omitted-dimension default. The parser regression tests that case.

The five Python harness tests pass. `cargo package --locked` also successfully
built the extracted source package: 70 files, about 2.3 MiB uncompressed / 432 KiB
compressed. This validates build packaging without the research caches, but does
not certify third-party redistribution or the future CVOX library API. The new
GitHub workflow has not run remotely because the branch has not been pushed.

## Mandel hit/miss correction

The next diagnosis found that `marchMandelbulber` correctly returned
`found = false` for exhausted, stalled or non-finite rays, but `renderPath`
discarded that flag through the position-only `march` wrapper. Positions inside
the far-distance limit were then shaded as surfaces. The upstream OpenCL
`engines/ray_recursion.cl` gates surface shading on its explicit `found` flag.

The FPT Mandel integrator now retains that flag for primary and secondary rays.
Misses take the existing background/environment path; native FPT marching,
Mandel step decisions, iteration limits and material calculations are unchanged.
A GPU readback regression forces an exhausted marcher and checks that it returns
the configured background instead of shading the last position. It failed at
that assertion before the fix and passed afterward.

Fresh captures are in `reports/release-canaries-found-fixed`; detailed diagnostic
commands, before/after differences and the rank-17 comparison are in
`reports/march-found-fix`. These generated artifacts are not packaged source.

| Rank | Changed geometry pixels vs checkpoint |
| --- | ---: |
| 01 | 0 |
| 02 | 0 |
| 03 | 2 |
| 04 | 0 |
| 05 | 0 |
| 13 | 3 |
| 17 | 54,392 |

Rank 17 no longer has the large false surface covering its background. The
other geometry differences are small, but the result is an intentional
correctness change, not a byte-exact Mandel gate. Authored captures also change
because secondary misses no longer shade false surfaces. Two native FPT controls
(Exact Box and Cornell, 96x96, 1 SPP) are byte-exact against the saved binary.

The full canary still completes only six of seven scene triplets: rank 17
authored mode fails with `Impacting Interactivity` at 300x300/32 SPP with default
bounces. Separate rank-17 probes completed at 300x300 with 1 SPP/1 bounce,
1 SPP/4 bounces, and 32 SPP/1 bounce. This separates the hit/miss correction from
the remaining long multi-bounce dispatch problem. Reducing bounces is not an
accepted release workaround, and probe wall times are not GPU benchmarks.

Build, formatting, documentation and five Python harness tests pass. Of 209 Rust
tests, 206 passed; three stitched-library tests were blocked by Apple's GPU
archiver reporting `No space left on device` on the system disk. Moving task
temporary files to Ventura allowed ordinary tests to run but did not redirect
the OS-owned GPU archiver cache. After system disk headroom returned, the full
Rust gate was rerun successfully: all 209 tests passed. The hit/miss correction
can therefore be retained independently of the remaining watchdog issue.

## Authored diffuse correction

The next isolated probe found that `sunContributionWithSurface` multiplied
Mandel direct diffuse light by `material.roughness`. The default Mandel
`surface_roughness = 0.01` maps to FPT roughness 0.1, incorrectly removing 90%
of that contribution. Upstream `src/shader_light_shading.cpp` uses the independent
material `shading` parameter: `1 - shading + max(N dot L, 0) * shading`.

The candidate passes the parsed shading value through the previously unused
`mandel_appearance[5]` slot and applies that expression only to authored Mandel
direct light. It preserves shadow traversal, ray offsets, roughness-based bounce
directions, material colors and native FPT lighting. It does not implement the
remaining multi-material graph, specular or full authored-light model.

A one-bounce, zero-environment, zero-emission GPU readback regression failed
before the correction: summed linear RGB was 339.53754 at roughness 1 and
33.953785 at roughness 0.1. Afterward the two captures are exactly equal. The same
test verifies interpolation between flat and Lambert shading; a parser test
checks that shading and roughness remain independent.

All seven geometry/authored pairs complete at 300 maximum axis / authored
aspect / 32 SPP, using one-sample chunks and 32-row tiles. All seven geometry
captures and both native controls (Exact Box and Cornell, 96x96, 1 SPP) are
byte-exact against the accepted checkpoint. Descriptive RGB MAE against the
hash-verified CPU Mandel references is:

| Rank | Before | Corrected |
| --- | ---: | ---: |
| 01 | 0.418471 | 0.130798 |
| 02 | 0.495071 | 0.380897 |
| 03 | 0.201252 | 0.187038 |
| 04 | 0.038186 | 0.031224 |
| 05 | 0.060409 | 0.060409 |
| 13 | 0.102738 | 0.095634 |
| 17 | 0.046700 | 0.046700 |

Scene 05 differs at four pixels by at most one 8-bit channel level; scene 17's
authored capture is byte-exact. The other five show lower reference error.
Visual inspection still finds dark bands on scene 01, absent authored colour
on scene 02, under-lighting on scene 03 and different highlights/bands on scene
13. This is a validated diffuse-weight correction, not full appearance parity.
No performance improvement is claimed: scene 17 completed in 248.25 seconds
wall time, but this resumed capture run is not an alternating GPU benchmark.

Commands, hashes, before/after metrics and the inspected contact sheet are in
`reports/authored-diffuse-fix`. Reference captures are explicitly reused from
the preceding CPU-reference run; the candidate captures are new (scene 01 was
hash-verified and resumed after the initial attempt). Compact results are in
`docs/mandel-release-results.json`.

The initial scene-02 capture and one stitched-archive test were blocked by
Apple's GPU archiver reporting `No space left on device`. With user approval,
`uv cache clean` recovered 1.1 GiB of reproducible cache data without deleting
projects or installed environments. The full rerun passes all 211 Rust tests
and seven Python tests, plus formatting, diff checks, documentation and
extracted-package compilation. Nothing has been pushed.

Next diagnostic targets remain the position-only shadow-ray miss test and fixed
world-space ray offsets. Neither has been changed by this diffuse correction.

## Shadow-origin diagnosis

Starting from `fb0d21e`, seven diagnostic variants were compared on scenes 01,
02 and 03 with the same generated formula, 160x120 camera, 4 SPP and one bounce.
Emission, ambient contribution, secondary environment, DOF and soft-light jitter
were disabled to isolate direct visibility. This deliberately reduced probe is
not an authored beauty reference or a performance benchmark.

| Probe | Result |
| --- | --- |
| Existing fixed normal offset | Reproduces dark rings and under-lighting |
| Use the marcher's `found` flag only | No energy change on any of the three scenes |
| Freeze threshold only | Negligible on 01/03; lowers scene 02 energy by 4.47% |
| Surface-threshold normal offset | Removes the dark rings in the direct-only scene 01 probe |
| Normal offset plus fixed threshold | Similar to normal offset; no clear reason to bundle the second change |
| Surface-threshold light-direction offset | Removes dark rings and follows upstream's initial shadow displacement |
| Light-direction offset plus fixed threshold | Similar; deferred to keep the correction isolated |

The selected correction replaces the initial displacement only for authored
Mandel direct shadows: `point + lightDirection * surfaceThreshold`, after the
existing light jitter. Upstream `src/shader_aux_shadow.cpp` starts at
`input.distThresh` along `lightVector`; the OpenCL equivalent does the same.
Primary marching, later shadow stepping, the position-based miss test, bounce
origins, materials and native FPT offsets are unchanged. Full upstream shadow
parity is not claimed: the ordinary upstream shadow loop also uses a fixed
surface threshold, while this narrow change retains FPT's current loop.

An analytic GPU regression replaces only the distance field and first-hit
input, preserving the real sun function and Mandel marcher. It tests a plane
with/without a nearby blocking slab at three scales (thresholds 0.00001, 0.01
and 10), three light angles (1, 30 and 90 degrees), and positive/negative ray
directions. The old fixed offset skips the small-scale blocker, incorrectly
returning nonzero light. All 36 cases pass with the correction: clear rays
retain the expected cosine response and blocked rays return zero.

Diagnostic source variants, captures and linear-energy measurements are in
`reports/mandel-shadow-diagnosis`. The temporary experiment module is not part
of the source package; only the deterministic GPU regression is retained.

The full authored comparison is deliberately separate from that analytic
correctness gate. Scene 13 loses the false dark bands but becomes too bright
against the CPU reference: RGB MAE increases from 0.095634 to 0.132476. This
change is not an all-scene appearance improvement. Bright rings also remain in
scene 01. Do not compensate by restoring false occlusion or applying a
scene-name exposure multiplier; isolate the remaining light/material response
against a direct-only reference. The current captures do not establish which
part of that response is responsible for scene 13's increased error.

Fresh 32-SPP captures at 300 maximum-axis pixels, authored aspect, one sample
per chunk and 32-row tiles completed for all seven scenes. CPU references were
reused only after hash verification. All seven neutral geometry images remain
byte-exact. The reference-normalized RGB MAE is:

| Rank | Diffuse checkpoint | Shadow-origin correction |
| --- | ---: | ---: |
| 01 | 0.130798 | 0.103347 |
| 02 | 0.380897 | 0.299691 |
| 03 | 0.187038 | 0.184670 |
| 04 | 0.031224 | 0.026587 |
| 05 | 0.060409 | 0.060409 |
| 13 | 0.095634 | 0.132476 |
| 17 | 0.046700 | 0.046700 |

Scene 05 changes one pixel by one 8-bit channel level; scene 17 is byte-exact.
Scene 03's small MAE improvement does not establish appearance parity: its lit
faces are still too bright/yellow. Scene 02's missing authored material colour
also remains. Raw commands, capture/binary hashes and the three-column sheet
are under `reports/mandel-shadow-origin-fix`; the compact tracked results are in
`docs/mandel-release-results.json`. These are correctness/appearance captures,
not an alternating performance benchmark.

Validation: 212 Rust tests and seven Python harness tests pass, as do formatting,
diff checks and the documentation build. Exact Box and Cornell native controls
remain byte-exact with the optional Mandel tile flags present. This validates
native output isolation, not a performance guarantee.
The extracted source package also builds successfully (72 files, 2.3 MiB
uncompressed); ignored diagnostic modules and captures are excluded.

## Secondary-origin diagnosis

Starting from `64251c2`, a fixed-camera 32-SPP bounce-cap sweep separated
first-hit response from later contributions on scenes 01, 03 and 13. Scene
13's reference MAE was 0.076412 at one bounce, 0.115872 at two, 0.129567 at
four and 0.132476 at the full default. Scene 01's bright rings appeared after
the first bounce. This isolates a secondary contribution issue; lowering the
bounce count itself is not the fix, and the one-bounce capture still includes
the configured ambient/emission response.

An 8-SPP, maximum-axis-160 probe retained the production generated fields and
camera aspect. A separate instrumented shader counted secondary march calls
that immediately accepted their starting point as a hit. Counts below are
averaged over all sampled paths, including background, not just active rays:

| Rank | Fixed 0.001 normal offset | One threshold | Two thresholds |
| --- | ---: | ---: | ---: |
| 01 | 0.264160 | 0.000059 | 0.000228 |
| 03 | 1.693374 | 0.000889 | 0.000601 |
| 13 | 0.275537 | 0.005439 | 0.005303 |

The single-threshold probe removes scene 01's bright rings and sharply reduces
immediate re-hits. Doubling it has no consistent advantage, so the smaller
displacement is preferred. Residual immediate hits are not claimed solved.
No scene-name condition, exposure correction, normal formula or light/material
mapping is changed. Native FPT retains its original offset. This is not a full
dielectric boundary or general secondary-ray transport implementation.

The regression fixes the accepted primary input and outgoing direction while
retaining the production primary march, secondary offset and secondary march.
An unsigned plane, with/without another plane four thresholds away, is tested
at three scales, three outgoing angles and both signs. Before the correction,
the small-scale nearby-plane case is skipped. The test therefore checks both
surface escape and preservation of a nearby next hit, rather than only making
an image darker.

Ignored diagnostic scripts, variant sources, captures and counts are under
`reports/mandel-light-response`. Its bounce sweep and offset probe are distinct
experiments with different sample counts; neither is a GPU benchmark.

The full scene 02 result is a material/lighting outlier, not a successful
appearance match: removing secondary re-hits darkens it and raises reference
MAE from 0.299691 to 0.407476. Its authored orange/gold material response is
still absent. Scene 13 improves from 0.132476 to 0.116378 but remains too bright.
Do not describe either as full parity or restore erroneous repeated surface
shading to compensate for the remaining material response. The small scene 05
MAE increase (0.060409 to 0.060477) is also recorded rather than hidden.

For the next scene 02 investigation, distinguish the serialized legacy file
from Mandelbulber's effective loaded configuration. The version-2.14 input
explicitly specifies `mat1_use_colors_from_palette false` and a grey
`mat1_surface_color` (`8800 8800 8800`), despite the orange/gold CPU reference.
Capture the effective native material/light state before treating this as a
missing palette or multiplying FPT colours. The cause of that discrepancy is
not established by this offset experiment.

All fourteen fresh FPT canary captures completed at 32 SPP and 300 maximum-axis
pixels with authored aspect. All seven neutral geometry captures are byte-exact
against `64251c2`; authored changes are intentional and are not byte-exact gates.

| Rank | Shadow checkpoint MAE | Secondary-origin MAE |
| --- | ---: | ---: |
| 01 | 0.103347 | 0.096741 |
| 02 | 0.299691 | 0.407476 |
| 03 | 0.184670 | 0.143237 |
| 04 | 0.026587 | 0.026001 |
| 05 | 0.060409 | 0.060477 |
| 13 | 0.132476 | 0.116378 |
| 17 | 0.046700 | 0.041011 |

The fresh three-column comparison is
`reports/mandel-secondary-origin-fix/contact-sheet.png`; commands, binary/image
hashes and full metrics are in that directory's `summary.json`. Five MAEs
improve and two worsen. This is retained as a scale-dependent origin bug fix,
not certified full appearance parity or a measured speedup.

Validation passes: 213 Rust tests, seven Python harness tests, formatting,
diff checks, documentation build and extracted source-package build. Both
native controls remain byte-exact. The temporary probe module is excluded
from production source and packaging; only the analytic regression is retained.

## Scene 02 coloured ambient diagnosis

The earlier description of scene 02 as a missing orange/gold *material* was
incorrect. Native Mandelbulber 2.35-dev was asked to resave a copy of the
version-2.14 source. The result still disables palette colours and retains grey
`mat1_surface_color 8800 8800 8800`. Its migration adds a legacy specular width
of 1, but does not turn the surface orange. Original scene files were not edited.

Four fresh CPU captures at 300x169 isolate the source of the warm appearance:

| Native variant | Mean RGB channel spread, 8-bit | Observation |
| --- | ---: | --- |
| Original legacy scene | 87.6780 | Orange/gold |
| Native-resaved scene | 87.6698 | Same warm appearance |
| AO disabled | 0.3765 | Dark, near-neutral grey |
| Constant-white AO lightmap | 0.0968 | Near-neutral; deliberately unnormalised and clipped |

Channel spread means `max(R,G,B)-min(R,G,B)` averaged over the image, not MAE.
The original and migrated captures are visually consistent, not byte-exact;
their small pixel variation is retained in raw reports. The white-map control
changes the lightmap alone: its clipping makes it unsuitable for brightness
comparison, but removal of chroma confirms the source of the warm illumination.
The unchanged FPT checkpoint has zero channel spread for this grey scene.

Upstream `src/render_worker.cpp::PrepareAOVectors` samples directions and RGB
colours from `file_lightmap`. `src/shader_ambient_occlusion.cpp` traces their
visibility, and `src/shader_object.cpp` multiplies the resulting colour by AO
strength and AO tint before surface shading. Scene 02 selects multiple-ray AO
with strength 5.5. Its default lightmap is a coloured environment texture, not
a neutral occlusion multiplier. Installed and source-tree `lightmap.jpg` bytes
match, with SHA-256
`44253c6e1fc09ec929c503302608c8208d1f84bc9b4b9cdabd7215524daf8be7`.

The `287d36e` diagnostic checkpoint replaced this with a neutral, clamped `strength * 0.08` term. It
does not read the AO lightmap or perform that directional visibility integral.
Consequently, recolouring the material or restoring false secondary hits would
hide the missing lighting implementation rather than fix it. No shader or
material-parser behaviour is changed in this diagnostic checkpoint.

Raw native commands, original/migrated captures, the numeric white texture
fixture, source hashes and the comparison sheet are in
`reports/mandel-legacy-appearance`. The reference harness now supports explicit
lightmap pinning; ten Python tests pass, including delimiter rejection and
mutation detection. A real pinned scene-02 run at 64x36 / 1 FPT SPP completed
all three capture modes and recorded the texture hash. That small run validates
harness wiring, not appearance quality. Prior unpinned captures have not been
retroactively relabelled as fully reproducible.

### Coloured visibility-aware AO

`src/mandelbulber/ambient.rs` now loads the authored `file_lightmap` (or the
external source root's default `textures/lightmap.jpg`) only for authored-path
scenes with multiple-ray AO enabled. Missing assets fail explicitly. The render
metadata records the resolved path, SHA-256, dimensions, rotation, mode,
quality, strength, tint, direction count and lighting/guard policy. The texture
hash and sampled direction/colour table enter the generated shader source, so
changing an asset invalidates its metallib cache entry. No texture is bundled.

The finite spherical sample lattice follows native azimuth/latitude texel
mapping, including rotation order and the coordinate conversion to FPT space.
There is no cosine hemisphere multiplier. Integer pixels use the native
`2^bit_depth` divisor without an sRGB transfer function; HDR stays floating
point. JPEG decoder differences are still possible. Quality 4 produces 108
directions; quality outside 1..39 is rejected rather than silently truncating
the table. A zero lightmap contributes zero.

Each direction traces visibility from the accepted hit's threshold to the
native camera-distance/FOV range, with distance-estimate steps and the native
distance-to-blocker weighting. Invalid, stalled or 4096-step-exhausted rays
produce a magenta diagnostic rather than being assumed unoccluded. Iteration
fog and iteration-threshold distance evaluation are explicitly unsupported in
this path. Custom `--metallib` overrides are rejected because they would bypass
the asset-specialized implementation. Geometry mode and native FPT do not load
or execute this AO implementation.

This is **primary-hit authored AO compatibility**, not a new physical sky
integrator. It replaces the generic ambient term and secondary environment
illumination for the requesting scenes; the authored background and existing
direct/indirect sun path remain. AO is not added again at each diffuse bounce.
Fast-AO scenes retain their previous approximation. Reflection/refraction AO,
native specular, full authored material graphs and other appearance differences
remain separate work; no palette or exposure adjustment was used here.

Scene 02 at 300x169 / 32 FPT SPP improved RGB MAE against the same accepted CPU
reference from **0.407476 to 0.137054** (66.4% lower), with RMSE from 0.462638 to
0.196194. Warm colour and direction-dependent shading now appear without
recolouring its grey material. This is not exact appearance parity. Neutral
geometry is byte-exact. Raw commands, candidate images, metadata and comparison
sheets are under `reports/mandel-colored-ambient`; reused reference hashes and
the lightmap hash are recorded rather than claiming fresh native captures.

All seven neutral-geometry captures, all six non-multiple-ray authored captures
and both native FPT controls passed byte-exact comparison. A subsequent fresh
300x169 CPU reference explicitly pinned the same lightmap and reproduced the
scene-02 improvement: MAE **0.407500 to 0.137077**. Its small difference from the
stored CPU reference (MAE 0.000514) is retained, not described as byte-exact.
See `reports/mandel-colored-ambient/pinned-reference-02/summary.json` and
`reports/mandel-colored-ambient/scene02-comparison.png`.

Controlled tests cover integer/HDR maps, constant/black/directional colour,
rotation, missing assets, source-cache invalidation, unsupported modes, camera
projection range and scale-dependent clear/blocked/exhausted visibility. A real
Metal probe exercises the production AO visibility and colour sum on analytic
fields. The 64x36 pilot is not used for the headline visual metric.
The full test suite passes: 218 Rust tests and 10 Python harness tests.
Formatting, documentation build, extracted-package build and diff checks pass.

The scene-02 capture's reported render time was 58.45 seconds versus 45.20
seconds for the stored checkpoint capture. These are individual capture
measurements, not a fresh alternating performance benchmark. This change adds
108 visibility queries per primary hit to recover missing illumination; it is
not presented as a speed optimisation. Keep performance measurements separate
from the geometry/appearance gates.

## Experimental chunk tiling

The optional tiled dispatcher now supports sample chunks as well as the older
all-samples batch mode. Each command evaluates a bounded row range and a bounded
sample range, preserving the original global pixel coordinates, sample indices,
camera, formula iterations, bounces and per-pixel accumulation order. The tile
extent check rejects padded threadgroup lanes, including the final partial tile.
Untiled rendering remains the default; no preview, voxel or regional path is
routed through the new kernel.

```sh
FPT_MANDEL_TILED_DISPATCH=1 FPT_MANDEL_TILE_ROWS=32 \
  target/release/fpt-metal render scene.fract \
  --mandelbulber-root "$MANDEL_SOURCE" \
  --mandel-appearance authored-path \
  --width 300 --height 300 --samples 32 \
  --sdf-accumulation chunked --sdf-chunk-samples 1 --out reports/tiled-scene
```

The render metadata identifies this as `chunked-tiled-32-row`. The new
`scripts/run_mandel_tile_gate.py` compares a saved untiled binary, the candidate
with tiling off, and the candidate with 1-, 7- and 32-row tiles. Its defaults are
deliberately irregular: 97x83, 5 SPP, and two samples per chunk. Both geometry and
authored modes are checked on all seven canaries; failures and changed pixels
fail the gate rather than being omitted.

```sh
python3 scripts/run_mandel_tile_gate.py \
  --baseline-fpt /path/to/saved/fpt-metal --fpt target/release/fpt-metal \
  --scene-root "$MANDEL_EXAMPLES" --mandelbulber-root "$MANDEL_SOURCE" \
  --output reports/mandel-tile-gate
```

The initial irregular-size gate passed all 56 candidate comparisons with zero
changed pixels. Very small tiles incur significant dispatch/under-utilization
cost; this is a reliability option, not a claimed performance optimization or a
new global default.

The full-size run in `reports/release-canaries-chunk-tiled` completed all seven
FPT geometry/authored pairs at a maximum axis of 300, 32 SPP, one sample per
chunk and the original bounce settings. Rank 17's authored render completed in
213.14 seconds wall time instead of a watchdog error. All 13 available full-size
FPT captures from the untiled corrected checkpoint were byte-exact. There is no
full-size untiled authored image for rank 17 to compare against; its byte-exact
gate is the irregular-size test, not an invented full-size baseline.

### Reference backend correction

Visual inspection rejected the newly captured Mandelbulber rank-17 reference:
only 173 pixels were nonblack. Its log revealed that the reference runner had
inherited OpenCL from saved Mandelbulber application settings. The existing
`-C` argument controls console colours, not CPU rendering, so prior claims that
this command guaranteed a CPU reference were incorrect. FPT-versus-FPT parity
measurements are unaffected.

The runner now passes `-O opencl_enabled=0` and rejects an OpenCL render log.
A regression test checks the explicit CPU override. Fresh CPU references are
stored in `reports/release-canaries-chunk-tiled-cpu-ref`; its summary explicitly
records reuse of the unchanged FPT captures from the preceding tiled run.
All seven CPU references completed. Rank 17 took 789.93 seconds and has 29,376
nonblack pixels; the complete sheet was visually checked. Its earlier CPU
attempt was interrupted to replace an insufficient 600-second timeout, and
that attempt's logs remain available separately.
The reference runner now allows 1,800 seconds per command, configurable through
`--timeout`, because genuine CPU rendering of rank 17 takes many minutes.
This is a per-command ceiling, not an estimate for the complete suite.
The faulty reference and its logs remain preserved in the original directory.
Execution success still does not establish authored appearance parity.

All 209 Rust tests and seven Python harness tests pass. Formatting, documentation
and extracted-package compilation pass; the package includes the new tile gate
but excludes generated reports and caches.
Compact settings, binary fingerprints and gate counts are retained in
[`mandel-release-results.json`](mandel-release-results.json).

## Earlier failed probes

The initial fresh checkpoint sheet ran six of seven complete scene triplets
with one-sample command chunks. Rank 17 still failed in authored mode with
`Impacting Interactivity`; its geometry view also visibly differs from the
Mandelbulber reference. Rank 2 failed with the previous eight-sample chunks but
completed at 32 SPP with one-sample chunks. This identifies a scheduling risk,
not a proven universal fix. Do not call this gate passed.

Visual inspection found: rank 1 has excessive dark bands in authored mode;
rank 2 differs in detail and becomes much too dark; rank 3 has broadly similar
silhouette but severe authored-lighting mismatch; rank 4 is also too dark;
rank 5 is substantially closer in authored colour; rank 13 retains the broad
shape but differs in palette/illumination. Rank 17 is the first structural and
runtime blocker to isolate before a broader scene-support claim. Timing from
this diagnostic run is not an isolated performance benchmark.

An additional rank-17 probe used the existing batch tiled kernel with
`FPT_MANDEL_TILED_DISPATCH=1 FPT_MANDEL_TILE_ROWS=1`, still at 300x300/32 SPP.
It was stopped after a four-minute diagnostic budget without producing an image.
A sampled host stack was waiting in `MTLCommandBuffer waitUntilCompleted`.
This is an inconclusive probe, not proof of a deadlock or an accepted fix; no
tiled-renderer default or shader change was retained.

## Remaining work

1. Keep the verified hit/miss checkpoint (`9cbff95`); all 209 Rust tests passed
   after disk headroom was restored. Nothing has been pushed.
2. Keep chunk tiling optional despite its successful full-size watchdog gate.
   Automatic scheduling, larger scenes and interactive responsiveness require
   their own tests; the seven-scene offline result is not a universal guarantee.
3. Audit authored shadow stepping/occlusion, lighting and palette behavior
   independently. The authored direct-shadow origin now uses the surface
   threshold, but later stepping still uses FPT's dynamic threshold and a
   position-only march result. Authored secondary bounce offsets now use the
   surface threshold too; native FPT offsets remain fixed. Do not conflate
   these with primary geometry. Scene 01's bright rings are removed by the
   secondary-origin correction. Coloured multiple-ray AO now substantially
   improves scene 02; prioritize its remaining shadow/specular differences and
   scene 13's brightness mismatch before
   claiming appearance parity.
   Authored diffuse shading is now independent of the native FPT roughness
   weight, with a real GPU regression and seven-scene comparison above.
   Expand the gate to the ranked 50 and track the larger corpus separately.
4. Extract the CLI-owned production Metal compiler/render/export orchestration
   into a typed library API, requiring unchanged checkpoint captures.
5. Factor validated occupancy/refinement and CVOX packaging into reusable Rust
   components and pin the required VoxQuant source changes.
6. Expose CVOX plus required palette/local-mask/provenance sidecars as one asset
   bundle. The native viewer can continue loading generated files without a
   runtime dependency on FPT or external NRD.
7. Distinguish view-derived surfaces from camera-independent full volumes;
   validate unseen/secondary geometry before promising complete volumes.
8. Complete clean-clone integration, licensing, API documentation and final
   history consolidation. Generated metallibs, volumes, caches and large research
   captures must remain outside the published source package.

## Library extraction boundary

The production implementation already exists, but its ownership is split:

- `src/scene.rs::load_scene_config` prepares the parsed configuration and generated
  Metal source inside the library, currently through a CLI-shaped argument type.
- `src/main.rs::cached_mandel_render_artifacts` and
  `execute_metal_render_internal` own render compilation/cache/bridge orchestration.
  Move these behind a typed runtime without changing generated source, arithmetic
  or dispatch policy. Keep Metal device/configuration details out of the stable API.
- `src/main.rs::voxel_export_command` and structural capture helpers still own
  production export preparation. `src/fptvox7.rs` and `src/fptvox.rs` already expose
  the lower-level representation and artifact writers.
- The NAADF consumer's `tools/voxel_refinement_probe/src` uses the Rust
  `voxquant_core` crate for validated true occupancy/refinement. Its
  `tools/build_voxel_converter.py` currently creates a temporary Cargo package
  against an explicitly supplied local checkout and fingerprints its sources.
  Extract that converter into a reusable crate only after pinning the required
  external VoxQuant changes; a path dependency on a dirty checkout is not a
  release dependency contract.
- CVOX packing and sidecar handling remain in consumer tools such as
  `run_refinement_probe.py`, `mixed_cube_payload.py` and `colour_voxel_scene.py`.
  The future API must return an asset bundle containing CVOX, local occupancy
  masks, palette, camera and provenance, not just a path to an incomplete CVOX.

Keep authored-view-derived cubes and camera-independent bounded volumes as
distinct request/result kinds. Do not hide a CPU Mandel approximation or relabel
exact triangles as cube occupancy. The existing public `voxelize` API still
implements CPU Menger fixtures only; this audit does not change that limitation.
No consumer or VoxQuant source was modified during this release-runtime fix.
