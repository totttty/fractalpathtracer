#!/bin/zsh
set -euo pipefail

ROOT=${0:A:h:h}
FPT_ROOT=${FPT_ROOT:-$ROOT/../FPT}
OUT_DIR=${OUT_DIR:-$ROOT/reports/capabilities}
BIN=${BIN:-$ROOT/target/release/fpt-metal}
cd "$ROOT"
mkdir -p "$OUT_DIR" "$OUT_DIR/batch" "$OUT_DIR/chunked" "$OUT_DIR/preview" \
  "$OUT_DIR/one-spp" "$OUT_DIR/sixteen-spp"

if [[ ${SKIP_BUILD:-0} != 1 ]]; then
  cargo build --release
fi

"$BIN" capability-fixtures "$FPT_ROOT" "$OUT_DIR"

"$BIN" render "$OUT_DIR/typed-program.json" --out "$OUT_DIR/preview" --preview
"$BIN" render "$OUT_DIR/gradient-compat.json" --out "$OUT_DIR/preview" --preview
"$BIN" render "$OUT_DIR/typed-program.json" --out "$OUT_DIR/batch" --samples 64 --sdf-accumulation batch
"$BIN" render "$OUT_DIR/typed-program.json" --out "$OUT_DIR/chunked" --samples 64 --sdf-accumulation chunked --sdf-chunk-samples 4
"$BIN" compare "$OUT_DIR/batch/typed-program.png" "$OUT_DIR/chunked/typed-program.png" --report "$OUT_DIR/batch-vs-chunked.json" --strict
"$BIN" render "$OUT_DIR/typed-program.json" --out "$OUT_DIR/one-spp" --samples 1
"$BIN" render "$OUT_DIR/typed-program.json" --out "$OUT_DIR/sixteen-spp" --samples 16
if cmp -s "$OUT_DIR/one-spp/typed-program.png" "$OUT_DIR/sixteen-spp/typed-program.png"; then
  printf '1 spp and 16 spp outputs are unexpectedly identical\n' >&2
  exit 1
fi

echo "Capability outputs: $OUT_DIR"
