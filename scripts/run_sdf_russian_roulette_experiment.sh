#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/sdf-russian-roulette}"
RR_START="${RR_START:-3}"
RR_MIN_PROB="${RR_MIN_PROB:-0.2}"

cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo build --release
fi

BASELINE_BIN="${BASELINE_BIN:-$ROOT_DIR/target/release/fpt-metal}" \
CANDIDATE_BIN="${CANDIDATE_BIN:-$ROOT_DIR/target/release/fpt-metal}" \
CANDIDATE_EXTRA_RENDER_ARGS="--sdf-russian-roulette --sdf-rr-start $RR_START --sdf-rr-min-prob $RR_MIN_PROB ${CANDIDATE_EXTRA_RENDER_ARGS:-}" \
OUT_DIR="$OUT_DIR" \
SKIP_BUILD=1 \
"$ROOT_DIR/scripts/run_sdf_optimization_regression.sh"

echo "sdf russian roulette experiment report: $OUT_DIR"
