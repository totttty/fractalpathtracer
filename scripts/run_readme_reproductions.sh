#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ORIGINALS_DIR="${FPT_README_ORIGINALS:-$ROOT_DIR/renders/upstream-readme}"
OUT_DIR="${1:-$ROOT_DIR/renders/readme}"
SAMPLES="${FPT_README_SAMPLES:-}"

SCENES=(
  scenes/readme/01-Render005.json
  scenes/readme/02-Render_14.json
  scenes/readme/03-Cornell_box.json
  scenes/readme/04-Glass_Ball.json
  scenes/readme/07-M4.json
  scenes/readme/08-Render0ad03.json
  scenes/readme/09-Glass.json
)

mkdir -p "$OUT_DIR"

if [[ ! -x "$ROOT_DIR/target/release/fpt-metal" ]]; then
  cargo build --release --manifest-path "$ROOT_DIR/Cargo.toml"
fi

for scene in "${SCENES[@]}"; do
  args=(render "$ROOT_DIR/$scene" --out "$OUT_DIR")
  if [[ -n "$SAMPLES" ]]; then
    args+=(--samples "$SAMPLES")
  fi
  "$ROOT_DIR/target/release/fpt-metal" "${args[@]}"
done

"$ROOT_DIR/target/release/fpt-metal" contact-sheet \
  "$OUT_DIR/metal-contact-sheet.png" \
  "$OUT_DIR"/*-metal.png

if compgen -G "$ORIGINALS_DIR/01-*" >/dev/null; then
  "$ROOT_DIR/target/release/fpt-metal" readme-comparison \
    "$ORIGINALS_DIR" \
    "$OUT_DIR" \
    "$OUT_DIR/upstream-vs-metal.png"
else
  printf 'Skipping upstream comparison; no source images found in %s\n' "$ORIGINALS_DIR"
fi

printf 'README Metal reproductions: %s\n' "$OUT_DIR"
