#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FPT_ROOT="${FPT_ROOT:-$ROOT_DIR/../FPT}"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/smoke}"
SIZE_W="${SIZE_W:-512}"
SIZE_H="${SIZE_H:-384}"
SAMPLES="${SAMPLES:-16}"

mkdir -p "$OUT_DIR"
cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo build --release
fi

BIN="$ROOT_DIR/target/release/fpt-metal"
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

for scene in "${SCENES[@]}"; do
  "$BIN" render "$scene" --out "$OUT_DIR" --width "$SIZE_W" --height "$SIZE_H" --samples "$SAMPLES"
done

"$ROOT_DIR/target/release/fpt-metal" contact-sheet "$OUT_DIR/contact_sheet.png" "$OUT_DIR"/*.png
echo "smoke renders: $OUT_DIR"
