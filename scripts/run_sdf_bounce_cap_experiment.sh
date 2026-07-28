#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CAP="${CAP:-5}"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/sdf-bounce-cap-$CAP}"

cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo build --release
fi

BASELINE_BIN="${BASELINE_BIN:-$ROOT_DIR/target/release/fpt-metal}" \
CANDIDATE_BIN="${CANDIDATE_BIN:-$ROOT_DIR/target/release/fpt-metal}" \
CANDIDATE_EXTRA_RENDER_ARGS="--sdf-bounce-cap $CAP ${CANDIDATE_EXTRA_RENDER_ARGS:-}" \
OUT_DIR="$OUT_DIR" \
SKIP_BUILD=1 \
"$ROOT_DIR/scripts/run_sdf_optimization_regression.sh"

echo "sdf bounce cap experiment report: $OUT_DIR"
