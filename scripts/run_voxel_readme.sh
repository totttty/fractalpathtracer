#!/bin/zsh
set -euo pipefail

ROOT=${0:A:h:h}
OUT_DIR=${OUT_DIR:-$ROOT/reports/voxel-readme}
BIN=${BIN:-$ROOT/target/release/fpt-metal}
WIDTH=${WIDTH:-960}
HEIGHT=${HEIGHT:-540}
SAMPLES=${SAMPLES:-112}
VOXEL_RESOLUTION=${VOXEL_RESOLUTION:-}

cd "$ROOT"
mkdir -p "$OUT_DIR/sdf" "$OUT_DIR/voxel" "$OUT_DIR/compare"
command -v jq >/dev/null || { printf 'jq is required for this harness\n' >&2; exit 2; }

if [[ ${SKIP_BUILD:-0} != 1 ]]; then
  cargo build --release
fi

scenes=(
  scenes/readme/01-Render005.json
  scenes/readme/08-Render0ad03.json
  scenes/readme/09-Glass.json
)

sdf_metadata=()
voxel_metadata=()
sheet_images=()
for scene in $scenes; do
  scene_width=${WIDTH:-$(jq -r .width "$scene")}
  scene_height=${HEIGHT:-$(jq -r .height "$scene")}
  scene_samples=${SAMPLES:-$(jq -r .samples "$scene")}
  common_args=(
    --width "$scene_width"
    --height "$scene_height"
    --samples "$scene_samples"
  )
  "$BIN" render "$scene" \
    --renderer sdf \
    "${common_args[@]}" \
    --out "$OUT_DIR/sdf"
  voxel_args=(
    --renderer voxel
    --voxel-normal face
    "${common_args[@]}"
  )
  if [[ -n "$VOXEL_RESOLUTION" ]]; then
    voxel_args+=(--voxel-resolution "$VOXEL_RESOLUTION")
  fi
  "$BIN" render "$scene" \
    "${voxel_args[@]}" \
    --out "$OUT_DIR/voxel"
  output=$(basename "$(jq -r .output "$scene")")
  stem=${output%.png}
  "$BIN" compare \
    "$OUT_DIR/sdf/$output" \
    "$OUT_DIR/voxel/$output" \
    --report "$OUT_DIR/compare/$stem.json"
  jq -e '.width > 0 and .height > 0 and .unique_colours.candidate >= 16' \
    "$OUT_DIR/compare/$stem.json" >/dev/null || {
      printf 'invalid or low-detail voxel output: %s\n' "$output" >&2
      exit 1
    }
  sdf_metadata+=("$OUT_DIR/sdf/$output.render.json")
  voxel_metadata+=("$OUT_DIR/voxel/$output.render.json")
  sheet_images+=("$OUT_DIR/sdf/$output" "$OUT_DIR/voxel/$output")
done

summary_args=()
for ((index = 1; index <= ${#sdf_metadata}; index++)); do
  summary_args+=("${sdf_metadata[$index]}" "${voxel_metadata[$index]}")
done
"$BIN" voxel-summary "$OUT_DIR/performance.json" $summary_args
"$BIN" contact-sheet "$OUT_DIR/sdf-vs-voxel.png" $sheet_images

printf 'Voxel README report: %s\n' "$OUT_DIR"
