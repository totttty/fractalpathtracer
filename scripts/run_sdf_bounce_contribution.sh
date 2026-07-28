#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FPT_ROOT="${FPT_ROOT:-$ROOT_DIR/../FPT}"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/sdf-bounce-contribution}"
SIZE_W="${SIZE_W:-512}"
SIZE_H="${SIZE_H:-384}"
SAMPLES="${SAMPLES:-8}"
BOUNCES="${BOUNCES:-0 1 2 3 4 5}"

cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo build --release
fi

BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
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

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

for bounce in $BOUNCES; do
  bounce_dir="$OUT_DIR/bounce_$bounce"
  mkdir -p "$bounce_dir"
  for scene in "${SCENES[@]}"; do
    "$BIN" diagnostic "$scene" \
      --out "$bounce_dir" \
      --mode sdf-bounce-contribution \
      --sdf-bounce-index "$bounce" \
      --width "$SIZE_W" \
      --height "$SIZE_H" \
      --samples "$SAMPLES"
  done
  "$ROOT_DIR/target/release/fpt-metal" contact-sheet "$OUT_DIR/bounce_${bounce}_sheet.png" "$bounce_dir"/*.png
done

"$BIN" bounce-summary "$OUT_DIR" $BOUNCES

echo "sdf bounce contribution report: $OUT_DIR"
