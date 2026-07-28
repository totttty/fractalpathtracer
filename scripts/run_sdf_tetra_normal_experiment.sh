#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/sdf-tetra-normal}"

cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo build --release
fi

BASELINE_BIN="${BASELINE_BIN:-$ROOT_DIR/target/release/fpt-metal}" \
CANDIDATE_BIN="${CANDIDATE_BIN:-$ROOT_DIR/target/release/fpt-metal}" \
BASELINE_EXTRA_RENDER_ARGS="--sdf-normal-mode central ${BASELINE_EXTRA_RENDER_ARGS:-}" \
CANDIDATE_EXTRA_RENDER_ARGS="--sdf-normal-mode tetra ${CANDIDATE_EXTRA_RENDER_ARGS:-}" \
OUT_DIR="$OUT_DIR" \
SKIP_BUILD=1 \
"$ROOT_DIR/scripts/run_sdf_optimization_regression.sh"

echo "sdf tetra normal experiment report: $OUT_DIR"
