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
3. Audit authored shadow-ray origins/occlusion, lighting and palette behavior
   independently. The shadow path still uses a fixed world-space offset and a
   position-only march result; do not conflate its fix with primary geometry.
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
