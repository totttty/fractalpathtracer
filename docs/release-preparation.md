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

## Remaining work

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

1. Restore system disk headroom and rerun the three blocked tests plus the full
   gate. Retain the isolated hit/miss correction only with verified checks.
2. Bound rank-17 GPU command duration without reducing resolution, sample count,
   bounce count or formula iterations. Test spatial tiles within sample chunks,
   preserving pixel/sample identities and accumulation order; verify byte-exact
   output on completed controls before retrying the failing full canary. The
   existing batch tile mode evaluates all samples per tile and is not this gate.
3. Audit authored shadow-ray origins/occlusion, lighting and palette behavior
   independently. The shadow path still uses a fixed world-space offset and a
   position-only march result; do not conflate its fix with primary geometry.
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
