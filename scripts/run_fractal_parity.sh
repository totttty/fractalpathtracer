#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FPT_ROOT="${FPT_ROOT:-$ROOT_DIR/../FPT}"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/fractal-parity}"
SAMPLES="${SAMPLES:-16}"

mkdir -p "$OUT_DIR"
cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo build --release
fi

BIN="$ROOT_DIR/target/release/fpt-metal"
for scene in "$FPT_ROOT"/Beauty/Fractals/*.json; do
  extra=()
  if [[ -n "${SIZE_W:-}" && -n "${SIZE_H:-}" ]]; then
    extra+=(--width "$SIZE_W" --height "$SIZE_H")
  fi
  "$BIN" render "$scene" --out "$OUT_DIR/candidates" "${extra[@]}" --samples "$SAMPLES"
done

for candidate in "$OUT_DIR"/candidates/*.png; do
  name="$(basename "$candidate")"
  "$BIN" compare "$FPT_ROOT/exports-local-fractal-beauty/$name" "$candidate" --report "$OUT_DIR/${name%.png}.json" || true
done

"$ROOT_DIR/target/release/fpt-metal" contact-sheet "$OUT_DIR/contact_sheet.png" "$OUT_DIR"/candidates/*.png
"$ROOT_DIR/target/release/fpt-metal" contact-sheet "$OUT_DIR/comparison_sheet.png" "$OUT_DIR"/*.comparison.png
"$ROOT_DIR/target/release/fpt-metal" report-index "$OUT_DIR"
echo "fractal parity report: $OUT_DIR"
