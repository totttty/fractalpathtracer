# CI Notes

The parity gates require macOS with Xcode command line tools and Metal shader
compilation available through `xcrun`.

Recommended local gates:

```sh
cargo test --release
FPT_ROOT=../FPT scripts/run_smoke.sh
FPT_ROOT=../FPT scripts/run_all_parity.sh
FPT_ROOT=../FPT scripts/run_high_sample_parity.sh
```

Recommended hosted CI shape:

1. Use a macOS runner with a pinned stable Rust toolchain.
2. Run `cargo test --release` and `scripts/run_smoke.sh` on every push.
3. Run `scripts/run_all_parity.sh` on pull requests that touch `src/`,
   `shaders/`, or `scripts/`.
4. Run `scripts/run_high_sample_parity.sh` manually or nightly because the
   fractal renders are slower.
5. Upload `reports/*/index.html`, contact sheets, comparison sheets, and JSON
   reports as artifacts.

Do not run parity on non-macOS runners; the renderer depends on Metal.
