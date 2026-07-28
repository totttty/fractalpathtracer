#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FPT_ROOT="${FPT_ROOT:-$ROOT_DIR/../FPT}"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/beauty-parity}"
SAMPLES="${SAMPLES:-32}"

mkdir -p "$OUT_DIR"
cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo build --release
fi

BIN="$ROOT_DIR/target/release/fpt-metal"
for scene in "$FPT_ROOT/Beauty/Cornell_Box.json" "$FPT_ROOT/Beauty/Glass_Ball.json"; do
  extra=()
  if [[ -n "${SIZE_W:-}" && -n "${SIZE_H:-}" ]]; then
    extra+=(--width "$SIZE_W" --height "$SIZE_H")
  fi
  if [[ "$(basename "$scene")" == "Glass_Ball.json" ]]; then
    extra+=(--glass-mode "${GLASS_MODE:-pathtrace}")
  fi
  "$BIN" render "$scene" --out "$OUT_DIR/candidates" "${extra[@]}" --samples "$SAMPLES"
done

"$BIN" compare "$FPT_ROOT/exports-local-beauty/Cornell_Box.png" "$OUT_DIR/candidates/Cornell_Box.png" --report "$OUT_DIR/Cornell_Box.json" || true
"$BIN" compare "$FPT_ROOT/exports-local-beauty/Glass_Ball.png" "$OUT_DIR/candidates/Glass_Ball.png" --report "$OUT_DIR/Glass_Ball.json" || true

"$ROOT_DIR/target/release/fpt-metal" contact-sheet "$OUT_DIR/contact_sheet.png" "$OUT_DIR"/candidates/*.png
"$ROOT_DIR/target/release/fpt-metal" contact-sheet "$OUT_DIR/comparison_sheet.png" "$OUT_DIR"/*.comparison.png
"$ROOT_DIR/target/release/fpt-metal" report-index "$OUT_DIR"
echo "beauty parity report: $OUT_DIR"
