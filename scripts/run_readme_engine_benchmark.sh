#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ -z "${BASELINE_BIN:-}" ]]; then
  printf 'Set BASELINE_BIN to a preserved pre-change fpt-metal binary.\n' >&2
  printf 'Example: BASELINE_BIN=/tmp/fpt-metal-baseline %s\n' "$0" >&2
  exit 2
fi

export SCENE_SET=readme
export OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/readme-engine-benchmark}"
export RUNS="${RUNS:-5}"
export SAMPLES="${SAMPLES:-16}"
export SIZE_W="${SIZE_W:-512}"
export SIZE_H="${SIZE_H:-384}"
export MAX_MAE="${MAX_MAE:-8.0}"
export MAX_RMSE="${MAX_RMSE:-17.0}"
export MIN_SSIM="${MIN_SSIM:-0.85}"
export MIN_LOW_FREQUENCY_SSIM="${MIN_LOW_FREQUENCY_SSIM:-0.99}"

exec "$ROOT_DIR/scripts/run_sdf_optimization_regression.sh"
