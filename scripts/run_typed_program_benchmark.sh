#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${CANDIDATE_BIN:-$ROOT_DIR/target/release/fpt-metal}"

export SCENE_SET=typed-program
export BASELINE_BIN="$BIN"
export CANDIDATE_BIN="$BIN"
export BASELINE_EXTRA_RENDER_ARGS="--sdf-normal-mode central"
export CANDIDATE_EXTRA_RENDER_ARGS="--sdf-normal-mode auto"
export OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/typed-program-benchmark}"
export RUNS="${RUNS:-5}"
export SAMPLES="${SAMPLES:-16}"
export SIZE_W="${SIZE_W:-512}"
export SIZE_H="${SIZE_H:-384}"
export MAX_MAE="${MAX_MAE:-0.1}"
export MAX_RMSE="${MAX_RMSE:-1.0}"
export MIN_SSIM="${MIN_SSIM:-0.995}"
export MIN_LOW_FREQUENCY_SSIM="${MIN_LOW_FREQUENCY_SSIM:-0.999}"

exec "$ROOT_DIR/scripts/run_sdf_optimization_regression.sh"
