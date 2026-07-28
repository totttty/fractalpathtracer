#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FPT_ROOT="${FPT_ROOT:-$ROOT_DIR/../FPT}"
SAMPLES="${SAMPLES:-64}"

OUT_DIR="$ROOT_DIR/reports/high-sample-beauty-parity" \
SAMPLES="$SAMPLES" \
"$ROOT_DIR/scripts/run_beauty_parity.sh"

OUT_DIR="$ROOT_DIR/reports/high-sample-fractal-parity" \
SAMPLES="$SAMPLES" \
"$ROOT_DIR/scripts/run_fractal_parity.sh"

"$ROOT_DIR/target/release/fpt-metal" check-parity-reports \
  "$ROOT_DIR/reports/high-sample-beauty-parity" \
  "$ROOT_DIR/reports/high-sample-fractal-parity"

GLASS_PATH_OUT="$ROOT_DIR/reports/high-sample-glass-pathtrace-parity"
mkdir -p "$GLASS_PATH_OUT/candidates"
"$ROOT_DIR/target/release/fpt-metal" render \
  "$FPT_ROOT/Beauty/Glass_Ball.json" \
  --out "$GLASS_PATH_OUT/candidates" \
  --samples "$SAMPLES" \
  --glass-mode pathtrace
"$ROOT_DIR/target/release/fpt-metal" compare \
  "$FPT_ROOT/exports-local-beauty/Glass_Ball.png" \
  "$GLASS_PATH_OUT/candidates/Glass_Ball.png" \
  --report "$GLASS_PATH_OUT/Glass_Ball_pathtrace.json"
"$ROOT_DIR/target/release/fpt-metal" contact-sheet "$GLASS_PATH_OUT/contact_sheet.png" "$GLASS_PATH_OUT"/candidates/*.png
"$ROOT_DIR/target/release/fpt-metal" contact-sheet "$GLASS_PATH_OUT/comparison_sheet.png" "$GLASS_PATH_OUT"/*.comparison.png
"$ROOT_DIR/target/release/fpt-metal" report-index "$GLASS_PATH_OUT"
