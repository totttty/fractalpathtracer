# Experimental Metal precision arithmetic

The separate `native_distance_probe.cpp` adapter links an existing external
Mandelbulber build for [identical-point stencil diagnosis](../../docs/mandel-identical-point-stencils.md).
It contains no formula implementation and is not part of the production build.
Its linked executable remains external GPL-bound diagnostic output.
It supports default distance, `metal85-delta`, and `orbit85` modes. The orbit
mode emits radius, native iteration count/exhaustion flag and final XYZ; the
Python orbit report accounts for native's exhausted-loop bookkeeping.

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
