#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FPT_ROOT="${FPT_ROOT:-$ROOT_DIR/../FPT}"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/sdf-path-cost-diagnostics}"
SIZE_W="${SIZE_W:-512}"
SIZE_H="${SIZE_H:-384}"
SAMPLES="${SAMPLES:-8}"

cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo build --release
fi

BIN="$ROOT_DIR/target/release/fpt-metal"
if [[ "${SCENE_SET:-beauty}" == "readme" ]]; then
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
MODES=(sdf-primary-steps sdf-shadow-steps sdf-normal-evals sdf-bounces)

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

for mode in "${MODES[@]}"; do
  mkdir -p "$OUT_DIR/$mode"
  for scene in "${SCENES[@]}"; do
    "$BIN" diagnostic "$scene" \
      --out "$OUT_DIR/$mode" \
      --mode "$mode" \
      --width "$SIZE_W" \
      --height "$SIZE_H" \
      --samples "$SAMPLES"
  done
  "$ROOT_DIR/target/release/fpt-metal" contact-sheet "$OUT_DIR/${mode}_sheet.png" "$OUT_DIR"/"$mode"/*.png
done

"$BIN" path-cost-summary "$OUT_DIR"
