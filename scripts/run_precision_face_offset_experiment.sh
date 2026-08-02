#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/voxel-optimization/precision-face-offset-$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-5}"
WIDTH="${WIDTH:-960}"
HEIGHT="${HEIGHT:-540}"
SAMPLES="${SAMPLES:-112}"

command -v jq >/dev/null || { printf 'jq is required\n' >&2; exit 2; }
if [[ -e "$OUT_DIR" && "${RESUME:-0}" != "1" ]]; then
  printf 'refusing to overwrite %s (set RESUME=1 to continue)\n' "$OUT_DIR" >&2
  exit 2
fi
cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then cargo test; cargo build --release; fi

SCENES=(
  "$ROOT_DIR/scenes/readme/01-Render005.json"
  "$ROOT_DIR/scenes/readme/08-Render0ad03.json"
  "$ROOT_DIR/scenes/readme/09-Glass.json"
)
COMMON_ARGS=(
  --renderer voxel --voxel-resolution 256 --voxel-normal face --voxel-storage sparse-bricks
  --voxel-coverage legacy --voxel-build direct --voxel-leaf-refinement none --voxel-material stored
  --width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
)

mkdir -p "$OUT_DIR/legacy" "$OUT_DIR/precision" "$OUT_DIR/sdf" "$OUT_DIR/compare"
render_if_missing() {
  local scene="$1" directory="$2"
  shift 2
  local output
  output="$(jq -r .output "$scene")"
  [[ -f "$directory/$output.render.json" ]] || "$BIN" render "$scene" --out "$directory" "$@"
}
for ((run = 0; run < RUNS; run++)); do
  mkdir -p "$OUT_DIR/legacy/run_$run" "$OUT_DIR/precision/run_$run"
  for scene in "${SCENES[@]}"; do
    if ((run % 2 == 0)); then
      render_if_missing "$scene" "$OUT_DIR/legacy/run_$run" --voxel-offset legacy "${COMMON_ARGS[@]}"
      render_if_missing "$scene" "$OUT_DIR/precision/run_$run" --voxel-offset precision "${COMMON_ARGS[@]}"
    else
      render_if_missing "$scene" "$OUT_DIR/precision/run_$run" --voxel-offset precision "${COMMON_ARGS[@]}"
      render_if_missing "$scene" "$OUT_DIR/legacy/run_$run" --voxel-offset legacy "${COMMON_ARGS[@]}"
    fi
  done
done
for scene in "${SCENES[@]}"; do
  render_if_missing "$scene" "$OUT_DIR/sdf" \
    --renderer sdf --width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
done

: > "$OUT_DIR/records.jsonl"
for scene in "${SCENES[@]}"; do
  output="$(jq -r .output "$scene")"
  stem="${output%.png}"
  "$BIN" compare "$OUT_DIR/sdf/$output" "$OUT_DIR/legacy/run_0/$output" \
    --report "$OUT_DIR/compare/$stem-sdf-legacy.json"
  "$BIN" compare "$OUT_DIR/sdf/$output" "$OUT_DIR/precision/run_0/$output" \
    --report "$OUT_DIR/compare/$stem-sdf-precision.json"
  legacy_metadata=()
  precision_metadata=()
  for ((run = 0; run < RUNS; run++)); do
    legacy_metadata+=("$OUT_DIR/legacy/run_$run/$output.render.json")
    precision_metadata+=("$OUT_DIR/precision/run_$run/$output.render.json")
  done
  jq -s . "${precision_metadata[@]}" > "$OUT_DIR/$stem-precision-runs.json"
  jq -s --arg scene "$(basename "$scene" .json)" \
    --slurpfile precision "$OUT_DIR/$stem-precision-runs.json" \
    --slurpfile legacy_quality "$OUT_DIR/compare/$stem-sdf-legacy.json" \
    --slurpfile precision_quality "$OUT_DIR/compare/$stem-sdf-precision.json" '
      def median(values): values | sort | .[length / 2 | floor];
      . as $legacy | $precision[0] as $precision_runs |
      {scene:$scene,legacy:{render_ms:median($legacy|map(.elapsed_ms)),quality:$legacy_quality[0]},
       precision:{render_ms:median($precision_runs|map(.elapsed_ms)),quality:$precision_quality[0]}}' \
    "${legacy_metadata[@]}" >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson width "$WIDTH" --argjson height "$HEIGHT" \
  --argjson samples "$SAMPLES" --argjson runs "$RUNS" '
  (map(.legacy.render_ms)|add) as $legacy_time |
  (map(.precision.render_ms)|add) as $precision_time |
  (map(.legacy.quality.mean_absolute_error)|add/length) as $legacy_mae |
  (map(.precision.quality.mean_absolute_error)|add/length) as $precision_mae |
  (map(.legacy.quality.low_frequency_luminance_ssim)|add/length) as $legacy_lf |
  (map(.precision.quality.low_frequency_luminance_ssim)|add/length) as $precision_lf |
  {settings:{resolution:256,bounds_min:[-3.25,-3.25,-3.25],bounds_max:[3.25,3.25,3.25],
     surface_band:0.5,normals:"face",width:$width,height:$height,samples:$samples,runs:$runs},scenes:.,
   aggregate:{legacy_render_ms:$legacy_time,precision_render_ms:$precision_time,
     speed_ratio:($legacy_time/$precision_time),legacy_mae:$legacy_mae,precision_mae:$precision_mae,
     mae_improvement_percent:(($legacy_mae-$precision_mae)/$legacy_mae*100),
     legacy_lf_ssim:$legacy_lf,precision_lf_ssim:$precision_lf,
     promotion_gate:{minimum_speed_ratio:0.95,minimum_mae_improvement_percent:0.5,
       require_lf_ssim_improvement:true,require_no_scene_quality_regression:true},
     passes_promotion:(($legacy_time/$precision_time)>=0.95 and
       (($legacy_mae-$precision_mae)/$legacy_mae*100)>=0.5 and $precision_lf>$legacy_lf and
       all(.precision.quality.mean_absolute_error<=.legacy.quality.mean_absolute_error and
           .precision.quality.low_frequency_luminance_ssim>=.legacy.quality.low_frequency_luminance_ssim))}}' \
  "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

"$BIN" contact-sheet "$OUT_DIR/sdf-sheet.png" "$OUT_DIR"/sdf/*.png
"$BIN" contact-sheet "$OUT_DIR/legacy-sheet.png" "$OUT_DIR"/legacy/run_0/*.png
"$BIN" contact-sheet "$OUT_DIR/precision-sheet.png" "$OUT_DIR"/precision/run_0/*.png
"$BIN" contact-sheet "$OUT_DIR/comparison-sheet.png" \
  "$OUT_DIR/sdf-sheet.png" "$OUT_DIR/legacy-sheet.png" "$OUT_DIR/precision-sheet.png"
jq . "$OUT_DIR/summary.json"
printf 'precision face offset experiment: %s\n' "$OUT_DIR"
