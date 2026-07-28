#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCENE="${1:-$ROOT_DIR/scenes/benchmarks/Typed_Program_Fold.json}"
TRACE_DIR="${2:-$ROOT_DIR/reports/metal-traces}"
STAMP="$(date +%Y%m%d-%H%M%S)"
TRACE_PATH="$TRACE_DIR/fpt-metal-$STAMP.trace"

mkdir -p "$TRACE_DIR" "$TRACE_DIR/render"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo build --release --manifest-path "$ROOT_DIR/Cargo.toml"
fi

xcrun xctrace record \
  --template "Metal System Trace" \
  --output "$TRACE_PATH" \
  --launch -- \
  "$ROOT_DIR/target/release/fpt-metal" render "$SCENE" \
  --out "$TRACE_DIR/render" \
  --width "${SIZE_W:-512}" \
  --height "${SIZE_H:-384}" \
  --samples "${SAMPLES:-16}"

printf 'Metal System Trace: %s\n' "$TRACE_PATH"
