#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/sdf-accumulation-modes}"
RUNS="${RUNS:-3}"

cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo build --release
fi

BASELINE_BIN="${BASELINE_BIN:-$ROOT_DIR/target/release/fpt-metal}" \
CANDIDATE_BIN="${CANDIDATE_BIN:-$ROOT_DIR/target/release/fpt-metal}" \
BASELINE_EXTRA_RENDER_ARGS="--sdf-accumulation per-sample" \
CANDIDATE_EXTRA_RENDER_ARGS="--sdf-accumulation batch" \
OUT_DIR="$OUT_DIR" \
RUNS="$RUNS" \
SKIP_BUILD=1 \
"$ROOT_DIR/scripts/run_sdf_optimization_regression.sh"

echo "sdf accumulation mode report: $OUT_DIR"
