# Experimental Metal precision arithmetic

The separate `native_distance_probe.cpp` adapter links an existing external
Mandelbulber build for [identical-point stencil diagnosis](../../docs/mandel-identical-point-stencils.md).
It contains no formula implementation and is not part of the production build.
Its linked executable remains external GPL-bound diagnostic output.
It supports default normal-mode distance, `primary`, `metal85-delta`, `orbit85`, `rays`, and
`ifs10-config` modes. The orbit
mode emits radius, native iteration count/exhaustion flag and final XYZ; the
Python orbit report accounts for native's exhausted-loop bookkeeping.
The `rays` mode accepts normalized image-plane XY pairs and emits native XYZ
directions using the upstream camera and projection implementation. Consumers
must apply the native-to-Metal coordinate mapping before comparing rays.
The `ifs10-config` mode emits the upstream resolved formula-10 parameters,
including normalized directions, rotation matrices and render controls. This
is a diagnostic bridge, not the public library's parameter-loading API.
The distance modes support authored clipping bounds but not primitive/object-tree
or Boolean scene data. `mandel_normal_probe` also accepts negative point modes:
`-1` primary distance/threshold, `-2` raw field, `-3` camera ray from normalized
image-plane XY, and `-4` primary march position/hit flag. Existing normal/orbit
modes are unchanged. These entry points are diagnostics, not an assertion of
byte-identical compilation with the complete production path kernel.

`analytic85.rs` is a separate diagnostic Jacobian experiment used only by
`mandel_normal_probe` and `mandel_derivative_render`. Enable it with
`FPT_NORMAL_PROBE_ANALYTIC85=1`; unsupported formula/transform combinations
are rejected. It passes scene 42's identical-point and fresh-ray normal gates,
but is not a production mode or a path-tracing parity claim. The normal probe
also accepts `FPT_NORMAL_PROBE_SOURCE` to save its input shader to a new file;
generated source belongs in ignored reports, with the external-source licensing
boundary preserved. Run its source-guard tests with
`cargo test --release --example mandel_normal_probe`.

`mandel_derivative_render beauty ...` uses the original offline path-tracing
kernels for an isolated authored A/B. It requires chunked accumulation and
keeps generated sources/metallibs under its new report directory. The larger
camera sweep improves appearance but fails strict coverage: see the
[camera gates](../../docs/mandel-analytic-derivative-camera-gates.md).
`FPT_DERIVATIVE_RAY_PIXELS` is an internal diagnostic override for replaying
selected output-pixel coordinates and checking primary exit reasons. It does
not enable production behavior.

`FPT_MARCH_PROBE_PARAMETRIC=1` enables a separate primary ray-position trial in
`mandel_derivative_render`: accumulate a scalar travel distance and reconstruct
positions from the original ray. It leaves refinement unchanged and refuses
combination with analytic-85 or exit instrumentation. The default is off; it is
not a production CLI feature. Scene 567 still fails the depth and visual gates,
even when paired with the native legacy IFS distance-setting migration. See
[remaining-scene diagnosis](../../docs/mandel-remaining-scenes.md).
Run its source-guard tests with
`cargo test --release --example mandel_derivative_render`.

This directory contains a reusable, **diagnostic-only** three-term float
expansion and a standalone safe-math Metal runner. It is not included in the
normal renderer's shader library and exposes no stable Rust API or production
render CLI mode. It contains no Mandelbulber formula bodies.

From the repository root on macOS with Apple clang and Metal:

```sh
python3 scripts/run_metal_precision_gate.py --out reports/precision-arithmetic
python3 -m unittest discover -s scripts -p 'test_metal_precision_gate.py'
```

The output directory must not exist. The GPU test checks 4,112 arithmetic
results from 1,028 deterministic input pairs against host double precision,
then checks repeat output bytes. This includes cancellation and source
coordinates that collapse in float32. Raw inputs, outputs, compile commands,
source hashes and pipeline measurements remain in the report directory.

`Expansion.metal` provides `fpt_precision::Scalar`, addition, subtraction,
multiplication, division, comparison, `root` and `absolute`. Split source
float64 values into three normalized float32 terms before upload; never build
them from an already rounded float32 scene configuration. The runner accepts
nine floats per input record, dispatches at most 256 records per command and
expects eight floats per output record. It is a generic test driver, not an
application rendering loop.

## Offline reference command

The opt-in `mandel_precise_render` Cargo example renders the validated
`RoadToExascale.fract` using the original parsed float64 camera and formula
parameters. All field, marching and normal queries use three-term arithmetic.
It produces a white camera-headlight image, not authored beauty lighting,
materials, path tracing or a voxel export.

```sh
MANDELBULBER_ROOT=/path/to/mandelbulber2
SCENE="$MANDELBULBER_ROOT/deploy/share/mandelbulber2/examples/Robert Pancoast collection - license Creative Commons (CC-BY 4.0)/RoadToExascale.fract"
cargo run --release --example mandel_precise_render -- \
  "$SCENE" "$MANDELBULBER_ROOT" reports/precise-road 160x120
```

The output directory must not exist. `reference.png`, `depth.f64` (row-major
little-endian depth per pixel), `samples.bin`, original float64 rays,
`summary.json`, and the generated source are retained for diagnosis. Use the
hit field in each eight-float `samples.bin` record to distinguish depth from
miss travel distance: depth high/low, hit, march steps, shade high/low, stall,
reserved. GPU intervals and process wall time are reported separately.

This first command deliberately pins **the exact scene and native formula
SHA-256**, not just formula ID 117. Changed scenes, other formulas and source
revisions are rejected before creating output or compiling Metal. This avoids
silently omitting unsupported branches. Even a lighting-only scene edit is
currently rejected. The default is 160x120, one deterministic center ray per
pixel; sizes up to 320x240 are experimental, not a high-resolution parity claim.
Expect seconds or tens of seconds per small image, not interactive speed.

Formula branches are imported from the external native C++ source at runtime;
the original notice is carried into generated code. Generated GPL-derived
files belong in ignored `reports/` or external storage and must not be bundled
as Apache source. Apple clang and a Metal device are required, but this example
does not need Python, Qt or an installed Mandelbulber executable.

Run its input/arithmetic helper tests with:

```sh
cargo test --release --example mandel_precise_render
```

## Limitations

### Scene 567 IFS Probe

`scripts/run_mandel_ifs_precision.py` is a separate, scene/formula-hash-pinned
diagnostic for `IFS31_anim.fract`. It imports the external IFS formula body at
runtime, supplies native resolved double constants to three-term Metal
arithmetic, and checks original/float-rounded points against the real native
distance evaluator. Input points are TSV rows of native XYZ plus a positive
distance threshold. The field gate requires repeat byte identity and absolute
error at most `1e-12 + abs(native_distance)*1e-5`; rendering is skipped on failure.

```sh
python3 scripts/build_native_distance_probe.py \
  --native-build /path/to/mandelbulber-build --output reports/ifs-native-adapter
python3 scripts/run_mandel_ifs_precision.py \
  --scene /path/to/IFS31_anim.fract \
  --mandelbulber-root /path/to/mandelbulber2 \
  --native-probe reports/ifs-native-adapter/native-distance-probe \
  --points /path/to/points.tsv --out reports/ifs-precision \
  --size 300x169 --pixel-sampling native
```

Omit `--size` for distance probes only. `--pixel-sampling center` is the default
FPT sample convention; `native` uses integer image coordinates matching the
native CPU's unjittered screen mapping. No image registration or camera edits
are performed. The diagnostic uses native camera rays and constants, white
headlight shading, and no authored material transport. It is not a standalone
production FPT or NAADF path. At 300x169 the tested three-term implementation
uses about 144 seconds of GPU work. Do not enable it in the interactive loop.

Use `scripts/review_mandel_ifs_precision.py` to compare structural dumps with
decoded native depth, and `python3 -m unittest discover -s scripts -p
'test_mandel_ifs_precision.py'` for the helper tests. Reports and generated
GPL-derived shader sources remain ignored, with input/output hashes recorded.

### Native-Free Inputs And Two-Term Experiment

The scene-pinned `mandel_ifs_precision_inputs` Cargo example uses the public
FPT Rust parser to generate double camera/formula values and rays. It does not
launch or link Mandelbulber. For the validated scene, all emitted constants
match the native adapter exactly, and maximum ray component difference is
`7.78e-16`. This example supplies the legacy preferred-IFS behavior explicitly;
it does not change production parsing or claim general formula support.

```sh
cargo run --release --example mandel_ifs_precision_inputs -- \
  /path/to/IFS31_anim.fract reports/ifs-rust-inputs 300x169 native
python3 scripts/run_mandel_ifs_precision.py \
  --scene /path/to/IFS31_anim.fract \
  --mandelbulber-root /path/to/mandelbulber2 \
  --fpt-inputs reports/ifs-rust-inputs \
  --reference-report /path/to/previous-native-validated-point-report \
  --points /path/to/the-same-points.tsv --out reports/ifs-two-term \
  --size 300x169 --pixel-sampling native --arithmetic two --render-repeats 3
```

`--reference-report` reuses the previously captured native distances, checking
scene and input-point hashes before use. No native binary is needed for this
run, but the external formula source remains required. Output is still an
offline white-headlight diagnostic, not a production render CLI or public
precision API. The existing default arithmetic remains `three`.

`TwoTerm.metal` provides compensated float-pair operations with the same
three-float input ABI (the third component is folded into the pair on load).
The experiment keeps source constants, camera, orbit and position arithmetic
above float32 precision. Its finite-range arithmetic gate passes 4,112
operations, and the 1,503-point scene gate passes. At 300x169, three repeat
renders are byte-identical with median GPU work 4.23 seconds. Versus the
three-term reference, 20 pixels change by one brightness level; hit masks
match. This is not exact image parity between arithmetic variants.

Run the arithmetic gate independently:

```sh
python3 scripts/run_metal_precision_gate.py --arithmetic two --out reports/two-term-arithmetic
cargo test --release --example mandel_ifs_precision_inputs
```

`--round-data formula|camera` is diagnostic only and defaults to `none`.
Both rounding trials are rejected: formula rounding fails the field gate;
camera-origin rounding passes identical-point queries but fails image depth.
The input hashes and effective configuration are recorded independently.

### Arithmetic Limits

- This is truncated expansion arithmetic, not exact arithmetic or an IEEE
  binary64 emulator. It has float32's exponent constraints.
- Inputs must be normalized, finite and within the supported exponent range.
  The GPU gate covers ordinary finite arithmetic, cancellation, zero addition
  and square root, and moderate magnitudes. It does not certify arbitrary
  exponents, overflow, subnormal products or exceptional values.
- Division requires a nonzero denominator. `root` is supported for nonnegative
  inputs only. Unsupported domains do not have a general error-reporting API.
- Safe math is mandatory. The runner selects `MTLMathModeSafe` on macOS 15+,
  otherwise disables fast math. Implicit contraction is disabled; explicit
  `fma` is used to recover multiplication residuals.
- Full scene-48 numerical captures using this arithmetic approach are much
  slower than the two-term prototype. They are an offline correctness gate,
  not evidence of a performance improvement.

The error-free-sum and expansion-arithmetic background is described in
[Shewchuk's work](https://www.cs.cmu.edu/~quake/robust.html). The fixed-size
truncation used here needs its own application-level gates; it is not the
exact-predicate algorithm certified by that work.
## Scene 567 Path Integration Experiment

`run_mandel_ifs_path.py` is a scene-pinned offline experiment using the existing
path integrator and presentation code. It does not change the public renderer
or NAADF. Original camera/positions, orbit constants and arithmetic stay
expanded through normals, material orbit, shadows and secondary traversal.
Scattering directions and final material/light values remain float32.

```sh
cargo build --release --example mandel_derivative_render --example mandel_ifs_precision_inputs
python3 scripts/run_mandel_ifs_path.py \
  --scene /path/to/IFS31_anim.fract \
  --mandelbulber-root /path/to/mandelbulber2 \
  --precision two --control authored --size 300x169 --samples 32 --bounces 4 \
  --out reports/new-ifs-path-experiment
```

Use `--control headlight` for white primary geometry. `--precision` is an alias
for the existing `--arithmetic` option. The default remains `three`, limited to
1,024 image pixels in the runner. Two-term runs support up to 320x240 and 32
samples. The runner now uses the existing tiled-chunk kernel: eight rows and
one sample per command by default (`--tile-rows 1..32`). Larger images cannot
run untiled. The 32x18 tiled smoke produced byte-identical linear output to the
previous untiled four-bounce run.

FPT double inputs are generated automatically unless `--fpt-inputs DIR` names
a validated cached directory. No native executable is needed. A bounded
`--camera-shift=RIGHT,UP` moves the eye and target together in units of their
original separation (each component at most 0.5 in magnitude). For example,
`--camera-shift=0.25,0` tests a nearby parallel view. The helper retains the
precise translated origin; the normal float32 camera position is not used
for geometry. Native references must use the recorded `camera` and `target`
from `run.json`. These are translation checks, not rotation or general camera
support; arbitrary render CLI camera overrides remain rejected.

AO/fog/clouds/glow, DOF, other source scenes and unsupported auxiliary/fake
lights remain outside this mode. Generated external
formula bodies stay in ignored output directories. No native executable is
needed for FPT generation or rendering.

The internal `FPT_IFS_PRECISION_SOURCE` environment hook is only exposed by the
diagnostic example. Do not point production scripts at it. Safe math is
mandatory and enabled automatically. Exhausted/stalled path queries and
shadow queries produce non-finite linear output and fail the render gate
rather than being silently reported as sky. Beauty timings are renderer wall
time, not GPU counters.

The original exact arithmetic gate: displayed output matches three-term at one/two
bounces in the small gate, but differs at three/four bounces. At four bounces,
89 of 576 pixels change, RGB MAE 0.203704/255, maximum channel difference 43.
Repeated two-term output is byte-exact. The user subsequently accepted this
visual difference, so the follow-up uses structural/visual acceptance rather
than pretending it passes byte equality. It remains an explicit offline mode
for scene 567, not a production default or support claim for all IFS scenes.
See `docs/mandel-remaining-scenes.md` for the fresh gallery/camera evidence.

### Point Math And Native Capture Controls

`mandel_normal_probe` supports isolated offline math comparisons without changing
the production compiler. `FPT_NORMAL_PROBE_SOURCE=NEW.metal` writes the complete
diagnostic source including its entry point. Compile that source with the Metal
2.4 toolchain using `-ffast-math` or `-fno-fast-math`, link the AIR to a library,
then set `FPT_NORMAL_PROBE_LIBRARY=path.metallib` to query it. The result records
the library hash. Keep compile commands and the dumped source with the report:
the probe cannot prove that an arbitrary supplied library came from that source.
`FPT_BEAUTY_PROBE_SAFE_MATH=1` selects conservative math only in the diagnostic
`mandel_derivative_render beauty` example; it does not alter ordinary rendering.

The native adapter's `analytic-field` mode reports raw upstream analytic
distance, orbit radius, iterations and max-iteration state before clipping and
interior policy. It accepts only analytic fields; inputs must already be in
native fractal coordinates, without scene transforms. The separate
`FPT_NATIVE_PROBE_OVERRIDES_FILE` hook reads main-container `key=value` lines
through the native CLI-style QString setter. Its `.settings` sidecar records
effective values for capture diagnosis. It does not support fractal-slot CLI
prefixes. Generated external code and linked native binaries remain ignored
diagnostic artifacts, subject to their upstream license.

### Identical-Ray First-Hit Diagnosis

`run_mandel_first_hit_parity.py` compares the original native `RayMarching`
worker with FPT primary traversal on the same float32 rays and rounded camera
origin. It samples both hits and misses rather than selecting only FPT hits.
The adapter links existing native objects; `-fno-access-control` exposes the
private worker only in this diagnostic translation unit. No native traversal
implementation is copied into the renderer.

```sh
python3 scripts/run_mandel_first_hit_parity.py \
  --scene /path/to/hybrid77-stereo.fract \
  --source-root /path/to/mandelbulber2 \
  --metal-probe target/release/examples/mandel_normal_probe \
  --native-probe reports/native-probe/native-distance-probe \
  --grid-width 51 --grid-height 27 --seeds 0 1 2 \
  --output reports/new-first-hit-comparison
```

The native `march` mode accepts `dx dy dz seed` rows and emits two eight-value
records per ray (unrefined, refined): found, XYZ position, depth, last distance,
threshold and primary step-buffer count. Native coordinates are fractal XYZ;
the harness converts once to Metal XZY/world scale. Seed zero disables native
step jitter; positive seeds retain it and are reset for each raw/refined call.
This does not reproduce the native image renderer's global random schedule.
The adapter currently requires nonhybrid formula 11, no primitives, and no
displacement/fractalized texture queries. It is not a general scene oracle.

Metal modes `w=-5` and `w=-6` take a supplied direction in XYZ and return refined
and unrefined hits respectively. The raw helper is extracted from the current
production primary loop before refinement; the harness checks identical hit
flags and also compares supplied rays against internally generated rays.
Native and Metal thresholds remain each implementation's own dynamic policy,
not artificially frozen. Reports retain commands, binaries' hashes, inputs and
outputs. These isolated probes are not a production image or timing gate.

`run_mandel_march_controls.py --baseline REPORT --source-root MANDEL_ROOT
--metal-probe PROBE --output NEW_REPORT` replays these rays with diagnostic
step jitter and dynamic refinement thresholds separately and together. It
requires native seed-0/1/2 TSVs from the first-hit harness. No-control and
seed-zero outputs must equal the accepted probe output; refinement must not
change hit flags. Use a new directory on every run.

`FPT_NORMAL_PROBE_STEP_SEED=0..2147483646` selects a Park-Miller step sequence
inside each march. The multiplier is applied before existing step clamps,
not after them. `FPT_NORMAL_PROBE_DYNAMIC_THRESHOLD=1` refreshes the threshold
at each refinement point. Neither option changes ordinary renderer code;
source rewriting is guarded against changed signatures/call sites, and the
unmodified step helper remains available to all other callers. These controls
cannot be combined with an arbitrary offline probe library.

The beauty example exposes the same experiments through
`FPT_BEAUTY_PROBE_STEP_SEED` and `FPT_BEAUTY_PROBE_DYNAMIC_THRESHOLD`.
They require ordinary isolated beauty mode, not analytic/parametric/IFS
experiments. Jitter restarts the same seed at **every invocation of the march
function**, including secondary calls. That correlated sampling is only a
diagnosis control, not the native image RNG schedule or a production sampling
design. Render reports include both settings. Never infer image parity from
identical supplied-ray hit flags.

`FPT_BEAUTY_PROBE_SAMPLING_SEED=UINT32` is the separate dimensioned-jitter
experiment. It hashes unjittered pixel-plane coordinates, the global sample
index, bounce index and an experiment seed with a march-event salt, then uses
that nonzero seed for the step stream. It does not consume or alter camera,
scattering or Fresnel random dimensions. Only the ordinary `renderPath` march
call is redirected; focus, shadow and non-path callers keep their existing
sampling. Dynamic refinement, when also requested, retains its separate scope.
Fixed-seed and dimensioned-seed controls are mutually exclusive. This remains
an offline experiment, not a public render flag or proven native RNG match.

`scripts/run_mandel_refinement_suite.py` consumes the legacy-DE audit,
recovered headlight references and scene-572 authored report. It verifies
source/reference hashes and dimensions, rejects flat references, and requires
fresh unchanged beauty output to equal accepted captures. It tests ten white
geometry scenes and the two existing safe authored controls at 300px max edge,
32 SPP, without rerunning authored hybrid25. `--sampling-seed N` adds sampled
and sampled-plus-refinement variants; output directories must be new.
The report remains incomplete until all captures and contact sheets finish.

After production step-jitter promotion, `mandel_derivative_render` deliberately
restores the deterministic path call before applying experimental controls.
Its summary marks `production_step_sampling_disabled_for_control: true`.
This preserves the historical unchanged/refinement/fixed/dimensioned A/B modes;
use ordinary `fpt-metal render` to test the production default. The normal probe
also continues to query the deterministic marcher directly for identical-ray
diagnosis. These probes must not be presented as the new default renderer.

`scripts/verify_mandel_step_sampling.py --reference SAMPLED_SUITE --binary FPT
--source-root MANDEL_ROOT --output NEW_REPORT` renders production at the same
32 SPP and requires zero changed pixels against each reviewed jitter-only
capture. It checks source/image identities and leaves a partial report on
failure. Higher-SPP checks were explicitly waived for this promotion.
