#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FPT_ROOT="${FPT_ROOT:-$ROOT_DIR/../FPT}"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/sdf-optimization}"
SIZE_W="${SIZE_W:-512}"
SIZE_H="${SIZE_H:-384}"
SAMPLES="${SAMPLES:-16}"
RUNS="${RUNS:-3}"
GLASS_MODE="${GLASS_MODE:-analytic}"
MAX_MAE="${MAX_MAE:-1.0}"
MAX_RMSE="${MAX_RMSE:-3.0}"
MIN_SSIM="${MIN_SSIM:-0.995}"
MIN_LOW_FREQUENCY_SSIM="${MIN_LOW_FREQUENCY_SSIM:-0.995}"

cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo build --release
fi

CANDIDATE_BIN="${CANDIDATE_BIN:-$ROOT_DIR/target/release/fpt-metal}"
BASELINE_BIN="${BASELINE_BIN:-$CANDIDATE_BIN}"
read -r -a BASELINE_EXTRA_RENDER_ARGS <<< "${BASELINE_EXTRA_RENDER_ARGS:-}"
read -r -a CANDIDATE_EXTRA_RENDER_ARGS <<< "${CANDIDATE_EXTRA_RENDER_ARGS:-}"

if [[ "${SCENE_SET:-beauty}" == "typed-program" ]]; then
  SCENES=(
    "$ROOT_DIR/scenes/benchmarks/Typed_Program_Fold.json"
  )
elif [[ "${SCENE_SET:-beauty}" == "readme" ]]; then
  SCENES=(
    "$ROOT_DIR/scenes/readme/01-Render005.json"
    "$ROOT_DIR/scenes/readme/08-Render0ad03.json"
    "$ROOT_DIR/scenes/readme/09-Glass.json"
  )
else
  SCENES=(
    "$FPT_ROOT/Beauty/Cornell_Box.json"
    "$FPT_ROOT/Beauty/Glass_Ball.json"
    "$FPT_ROOT/Beauty/Fractals/Ball_Fractal.json"
    "$FPT_ROOT/Beauty/Fractals/Cage_Fractal.json"
    "$FPT_ROOT/Beauty/Fractals/IFS_Fractal.json"
    "$FPT_ROOT/Beauty/Fractals/Mandelbox_Fractal.json"
    "$FPT_ROOT/Beauty/Fractals/Menger_Sponge.json"
    "$FPT_ROOT/Beauty/Fractals/Tower_Fractal.json"
    "$FPT_ROOT/Beauty/Fractals/Tree_Fractal.json"
  )
fi

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR/baseline" "$OUT_DIR/candidate" "$OUT_DIR/compare"

render_scene() {
  local binary="$1"
  local scene="$2"
  local output="$3"
  shift 3
  "$binary" render "$scene" \
    --out "$output" \
    --width "$SIZE_W" \
    --height "$SIZE_H" \
    --samples "$SAMPLES" \
    --glass-mode "$GLASS_MODE" \
    "$@"
}

for ((run = 0; run < RUNS; run++)); do
  mkdir -p "$OUT_DIR/baseline/run_$run" "$OUT_DIR/candidate/run_$run"
  for scene in "${SCENES[@]}"; do
    if ((run % 2 == 0)); then
      render_scene "$BASELINE_BIN" "$scene" "$OUT_DIR/baseline/run_$run" "${BASELINE_EXTRA_RENDER_ARGS[@]}"
      render_scene "$CANDIDATE_BIN" "$scene" "$OUT_DIR/candidate/run_$run" "${CANDIDATE_EXTRA_RENDER_ARGS[@]}"
    else
      render_scene "$CANDIDATE_BIN" "$scene" "$OUT_DIR/candidate/run_$run" "${CANDIDATE_EXTRA_RENDER_ARGS[@]}"
      render_scene "$BASELINE_BIN" "$scene" "$OUT_DIR/baseline/run_$run" "${BASELINE_EXTRA_RENDER_ARGS[@]}"
    fi
  done
done

for baseline_image in "$OUT_DIR"/baseline/run_0/*.png; do
  name="$(basename "$baseline_image")"
  "$CANDIDATE_BIN" compare \
    "$baseline_image" \
    "$OUT_DIR/candidate/run_0/$name" \
    --report "$OUT_DIR/compare/${name%.png}.json" || true
done

"$ROOT_DIR/target/release/fpt-metal" contact-sheet "$OUT_DIR/baseline_sheet.png" "$OUT_DIR"/baseline/run_0/*.png
"$ROOT_DIR/target/release/fpt-metal" contact-sheet "$OUT_DIR/candidate_sheet.png" "$OUT_DIR"/candidate/run_0/*.png
"$ROOT_DIR/target/release/fpt-metal" contact-sheet "$OUT_DIR/comparison_sheet.png" "$OUT_DIR"/compare/*.comparison.png

"$CANDIDATE_BIN" optimization-summary "$OUT_DIR" "$MAX_MAE" "$MAX_RMSE" "$MIN_SSIM" "$MIN_LOW_FREQUENCY_SSIM" "$RUNS" "${#SCENES[@]}"

echo "sdf optimization report: $OUT_DIR"
