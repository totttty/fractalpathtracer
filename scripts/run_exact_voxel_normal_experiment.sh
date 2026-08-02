#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/voxel-optimization/exact-voxel-normal-$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-5}"
WIDTH="${WIDTH:-960}"
HEIGHT="${HEIGHT:-540}"
SAMPLES="${SAMPLES:-112}"

command -v jq >/dev/null || { printf 'jq is required\n' >&2; exit 2; }
if [[ -e "$OUT_DIR" && "${RESUME:-0}" != "1" ]]; then
  printf 'refusing to overwrite %s (set RESUME=1 to continue a partial run)\n' "$OUT_DIR" >&2
  exit 2
fi
cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo test
  cargo build --release
fi

SCENES=(
  "$ROOT_DIR/scenes/readme/01-Render005.json"
  "$ROOT_DIR/scenes/readme/08-Render0ad03.json"
  "$ROOT_DIR/scenes/readme/09-Glass.json"
)
COMMON_ARGS=(
  --renderer voxel --voxel-resolution 256 --voxel-storage sparse-bricks
  --voxel-coverage legacy --voxel-build direct --voxel-leaf-refinement none
  --width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
)

mkdir -p "$OUT_DIR/face" "$OUT_DIR/exact" "$OUT_DIR/sdf" "$OUT_DIR/compare"
render_if_missing() {
  local scene="$1"
  local directory="$2"
  shift 2
  local output
  output="$(jq -r .output "$scene")"
  if [[ -f "$directory/$output.render.json" ]]; then
    return
  fi
  "$BIN" render "$scene" --out "$directory" "$@"
}

for ((run = 0; run < RUNS; run++)); do
  mkdir -p "$OUT_DIR/face/run_$run" "$OUT_DIR/exact/run_$run"
  for scene in "${SCENES[@]}"; do
    if ((run % 2 == 0)); then
      render_if_missing "$scene" "$OUT_DIR/face/run_$run" \
        --voxel-normal face "${COMMON_ARGS[@]}"
      render_if_missing "$scene" "$OUT_DIR/exact/run_$run" \
        --voxel-normal exact "${COMMON_ARGS[@]}"
    else
      render_if_missing "$scene" "$OUT_DIR/exact/run_$run" \
        --voxel-normal exact "${COMMON_ARGS[@]}"
      render_if_missing "$scene" "$OUT_DIR/face/run_$run" \
        --voxel-normal face "${COMMON_ARGS[@]}"
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
  "$BIN" compare "$OUT_DIR/sdf/$output" "$OUT_DIR/face/run_0/$output" \
    --report "$OUT_DIR/compare/$stem-sdf-face.json"
  "$BIN" compare "$OUT_DIR/sdf/$output" "$OUT_DIR/exact/run_0/$output" \
    --report "$OUT_DIR/compare/$stem-sdf-exact.json"
  "$BIN" compare "$OUT_DIR/face/run_0/$output" "$OUT_DIR/exact/run_0/$output" \
    --report "$OUT_DIR/compare/$stem-face-exact.json"

  face_metadata=()
  exact_metadata=()
  for ((run = 0; run < RUNS; run++)); do
    face_metadata+=("$OUT_DIR/face/run_$run/$output.render.json")
    exact_metadata+=("$OUT_DIR/exact/run_$run/$output.render.json")
  done
  jq -s . "${exact_metadata[@]}" > "$OUT_DIR/$stem-exact-runs.json"
  jq -s --arg scene "$(basename "$scene" .json)" \
    --slurpfile exact "$OUT_DIR/$stem-exact-runs.json" \
    --slurpfile face_quality "$OUT_DIR/compare/$stem-sdf-face.json" \
    --slurpfile exact_quality "$OUT_DIR/compare/$stem-sdf-exact.json" \
    --slurpfile delta "$OUT_DIR/compare/$stem-face-exact.json" '
      def median(values): values | sort | .[length / 2 | floor];
      . as $face | $exact[0] as $exact_runs |
      {scene:$scene,
       face:{render_ms:median($face|map(.elapsed_ms)),quality:$face_quality[0]},
       exact:{render_ms:median($exact_runs|map(.elapsed_ms)),quality:$exact_quality[0]},
       delta:$delta[0]}' "${face_metadata[@]}" >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson width "$WIDTH" --argjson height "$HEIGHT" \
  --argjson samples "$SAMPLES" --argjson runs "$RUNS" '
  (map(.face.render_ms)|add) as $face_time |
  (map(.exact.render_ms)|add) as $exact_time |
  (map(.face.quality.mean_absolute_error)|add/length) as $face_mae |
  (map(.exact.quality.mean_absolute_error)|add/length) as $exact_mae |
  (map(.face.quality.low_frequency_luminance_ssim)|add/length) as $face_lf |
  (map(.exact.quality.low_frequency_luminance_ssim)|add/length) as $exact_lf |
  {settings:{resolution:256,bounds_min:[-3.25,-3.25,-3.25],bounds_max:[3.25,3.25,3.25],
     surface_band:0.5,width:$width,height:$height,samples:$samples,runs:$runs},
   scenes:.,
   aggregate:{face_render_ms:$face_time,exact_render_ms:$exact_time,speed_ratio:($face_time/$exact_time),
     face_mae:$face_mae,exact_mae:$exact_mae,mae_improvement_percent:(($face_mae-$exact_mae)/$face_mae*100),
     face_lf_ssim:$face_lf,exact_lf_ssim:$exact_lf,
     promotion_gate:{minimum_speed_ratio:0.95,require_mae_improvement:true,require_lf_ssim_improvement:true},
     passes_promotion:(($face_time/$exact_time)>=0.95 and $exact_mae<$face_mae and $exact_lf>$face_lf)}}' \
  "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

"$BIN" contact-sheet "$OUT_DIR/sdf-sheet.png" "$OUT_DIR"/sdf/*.png
"$BIN" contact-sheet "$OUT_DIR/face-sheet.png" "$OUT_DIR"/face/run_0/*.png
"$BIN" contact-sheet "$OUT_DIR/exact-sheet.png" "$OUT_DIR"/exact/run_0/*.png
"$BIN" contact-sheet "$OUT_DIR/comparison-sheet.png" \
  "$OUT_DIR/sdf-sheet.png" "$OUT_DIR/face-sheet.png" "$OUT_DIR/exact-sheet.png"

jq . "$OUT_DIR/summary.json"
printf 'exact voxel normal experiment: %s\n' "$OUT_DIR"
