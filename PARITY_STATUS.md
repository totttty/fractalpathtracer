# FPT Metal Parity Status

Status: strict parity passing for the current Beauty scene set.

Reference inputs:
- `$FPT_ROOT/Beauty/Cornell_Box.json`
- `$FPT_ROOT/Beauty/Glass_Ball.json`
- `$FPT_ROOT/Beauty/Fractals/*.json`

Reference outputs:
- `$FPT_ROOT/exports-local-beauty/*.png`
- `$FPT_ROOT/exports-local-fractal-beauty/*.png`

Passing gates:
- `cargo test --release`
- `scripts/run_all_parity.sh`
- `scripts/run_high_sample_parity.sh`
- `scripts/run_capability_tests.sh` for typed programs, gradients, HDRI, complete
  postprocessing, and batch/chunked equivalence
- `fpt-metal compare ... --strict` for one-off strict report checks

Current strict-passing report groups:
- `reports/beauty-parity`: Cornell Box and Glass Ball
- `reports/fractal-parity`: Ball, Cage, IFS, Mandelbox, Menger, Tower, Tree
- `reports/high-sample-beauty-parity`: Cornell Box and Glass Ball
- `reports/high-sample-fractal-parity`: all seven fractals
- `reports/high-sample-glass-pathtrace-parity`: Glass Ball pathtrace

Important implementation notes:
- Metal PNG export is vertically flipped at write-out to match FPT/OpenGL image orientation.
- The Metal ray marcher follows FPT's advance-then-threshold order.
- Beauty parity defaults to 32 samples so Cornell clears strict parity.
- Glass parity uses `--glass-mode pathtrace` by default; `GLASS_MODE=analytic` remains available for fast deterministic iteration.
- Strict parity uses MAE plus low-frequency luminance SSIM. Local SSIM is still reported for diagnostics.
- Render metadata records the pinned upstream SHA, input scene SHA-256, Metal
  device, and effective accumulation mode.
- Generated fractals can use a bounded typed SDF instruction stream without a
  new shader case. Beauty modules remain specialized compatibility paths.
- Radiance HDRI, chromatic aberration, and highlight processing run through
  the shared linear render/postprocess path.

Known limitations:
- Arbitrary raw GLSL in `.fpt` files is not translated. New procedural scenes
  use typed `sdf_program` data; `Gradient_Example.fpt` has a compatibility
  compiler as the first non-Beauty preset.
- HDRI input currently supports Radiance RGBE `.hdr`; EXR is not decoded.
- The typed program is bounded to 64 operations and 16 gradient stops.
- Live AppKit/Metal preview exists as `fpt-metal preview`; `--pathtrace`
  progressively accumulates one sample per frame. It is not a parity gate.
