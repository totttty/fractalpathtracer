# Rust Migration Validation

The Rust host was validated against the final Zig binary before the Zig source
and build artifacts were removed.

## Image equivalence

Cornell Box, Glass Ball, and Menger Sponge were rendered at `512x384`, 16
samples through both hosts. All three comparisons were byte-equivalent:

| Scene | MAE | RMSE | Changed pixels |
| --- | ---: | ---: | ---: |
| Cornell Box | 0 | 0 | 0% |
| Glass Ball | 0 | 0 | 0% |
| Menger Sponge | 0 | 0 | 0% |

The complete Beauty suite was then rendered through the Rust host. All nine
scenes passed both recognition and strict parity gates.

## Performance

The host benchmark alternated Zig and Rust launch order for 12 warm Cornell Box
renders at `512x384`, 32 samples. Both binaries dispatched the same compiled
Metal shader and reported command-buffer elapsed time.

| Host | Median GPU time | Mean GPU time | Range |
| --- | ---: | ---: | ---: |
| Zig | 215.345 ms | 216.390 ms | 210.944-226.367 ms |
| Rust | 214.825 ms | 215.681 ms | 207.993-221.790 ms |

Measured Rust-to-Zig speedup: `1.0024x`. The migration therefore introduces no
render-performance regression; GPU work remains the dominant cost.

## Runtime dependencies

The CLI, scene compiler, metadata writer, comparison tooling, contact sheets,
report indexes, diagnostic summaries, and regression summaries are implemented
in Rust. The repository has no Zig or Python source/runtime dependency. The
native Objective-C++ bridge and Metal shader remain because they provide the
AppKit/Metal implementation used by the Rust host.
