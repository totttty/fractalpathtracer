#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/voxel-optimization/exact-voxel-material-$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-5}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-16}"

command -v jq >/dev/null || { printf 'jq is required\n' >&2; exit 2; }
[[ ! -e "$OUT_DIR" ]] || { printf 'refusing to overwrite %s\n' "$OUT_DIR" >&2; exit 2; }
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
VARIANTS=(stored_face exact_face stored_normal exact_both)
COMMON_ARGS=(
  --renderer voxel --voxel-resolution 256 --voxel-storage sparse-bricks
  --voxel-coverage legacy --voxel-build direct --voxel-leaf-refinement none
  --width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
)

mkdir -p "$OUT_DIR/sdf" "$OUT_DIR/compare"
for variant in "${VARIANTS[@]}"; do mkdir -p "$OUT_DIR/$variant"; done
for ((run = 0; run < RUNS; run++)); do
  if ((run % 2 == 0)); then
    order=(stored_face exact_face stored_normal exact_both)
  else
    order=(exact_both stored_normal exact_face stored_face)
  fi
  for variant in "${order[@]}"; do
    mkdir -p "$OUT_DIR/$variant/run_$run"
    case "$variant" in
      stored_face) mode=(--voxel-normal face --voxel-material stored) ;;
      exact_face) mode=(--voxel-normal face --voxel-material exact) ;;
      stored_normal) mode=(--voxel-normal exact --voxel-material stored) ;;
      exact_both) mode=(--voxel-normal exact --voxel-material exact) ;;
    esac
    for scene in "${SCENES[@]}"; do
      "$BIN" render "$scene" --out "$OUT_DIR/$variant/run_$run" "${mode[@]}" "${COMMON_ARGS[@]}"
    done
  done
done
for scene in "${SCENES[@]}"; do
  "$BIN" render "$scene" --out "$OUT_DIR/sdf" \
    --renderer sdf --width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
done

: > "$OUT_DIR/records.jsonl"
for scene in "${SCENES[@]}"; do
  output="$(jq -r .output "$scene")"
  stem="${output%.png}"
  record="$OUT_DIR/$stem-record.json"
  jq -n --arg scene "$(basename "$scene" .json)" '{scene:$scene,variants:{}}' > "$record"
  for variant in "${VARIANTS[@]}"; do
    "$BIN" compare "$OUT_DIR/sdf/$output" "$OUT_DIR/$variant/run_0/$output" \
      --report "$OUT_DIR/compare/$stem-$variant.json"
    metadata=()
    for ((run = 0; run < RUNS; run++)); do
      metadata+=("$OUT_DIR/$variant/run_$run/$output.render.json")
    done
    value="$OUT_DIR/$stem-$variant-value.json"
    jq -s --slurpfile quality "$OUT_DIR/compare/$stem-$variant.json" '
      def median(values): values | sort | .[length / 2 | floor];
      {render_ms:median(map(.elapsed_ms)),quality:$quality[0]}' "${metadata[@]}" > "$value"
    next="$record.next"
    jq --arg key "$variant" --slurpfile value "$value" '.variants[$key]=$value[0]' "$record" > "$next"
    mv "$next" "$record"
  done
  jq -c . "$record" >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson width "$WIDTH" --argjson height "$HEIGHT" \
  --argjson samples "$SAMPLES" --argjson runs "$RUNS" '
  def total(key): map(.variants[key].render_ms)|add;
  def mae(key): (map(.variants[key].quality.mean_absolute_error)|add)/length;
  def lf(key): (map(.variants[key].quality.low_frequency_luminance_ssim)|add)/length;
  (total("stored_face")) as $stored_face_time |
  (total("exact_face")) as $exact_face_time |
  (total("stored_normal")) as $stored_normal_time |
  (total("exact_both")) as $exact_both_time |
  {settings:{resolution:256,bounds_min:[-3.25,-3.25,-3.25],bounds_max:[3.25,3.25,3.25],
     surface_band:0.5,width:$width,height:$height,samples:$samples,runs:$runs},scenes:.,
   aggregate:{
     stored_face:{render_ms:$stored_face_time,mae:mae("stored_face"),lf_ssim:lf("stored_face")},
     exact_face:{render_ms:$exact_face_time,speed_ratio:($stored_face_time/$exact_face_time),
       mae:mae("exact_face"),lf_ssim:lf("exact_face")},
     stored_normal:{render_ms:$stored_normal_time,mae:mae("stored_normal"),lf_ssim:lf("stored_normal")},
     exact_both:{render_ms:$exact_both_time,speed_ratio:($stored_normal_time/$exact_both_time),
       mae:mae("exact_both"),lf_ssim:lf("exact_both")},
     promotion_gate:{minimum_speed_ratio:0.95,require_improvement_with_both_normal_modes:true},
     passes_promotion:(($stored_face_time/$exact_face_time)>=0.95 and
       ($stored_normal_time/$exact_both_time)>=0.95 and mae("exact_face")<mae("stored_face") and
       mae("exact_both")<mae("stored_normal") and lf("exact_face")>lf("stored_face") and
       lf("exact_both")>lf("stored_normal"))}}' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

"$BIN" contact-sheet "$OUT_DIR/sdf-sheet.png" "$OUT_DIR"/sdf/*.png
for variant in "${VARIANTS[@]}"; do
  "$BIN" contact-sheet "$OUT_DIR/$variant-sheet.png" "$OUT_DIR/$variant"/run_0/*.png
done
"$BIN" contact-sheet "$OUT_DIR/comparison-sheet.png" "$OUT_DIR/sdf-sheet.png" \
  "$OUT_DIR/stored_face-sheet.png" "$OUT_DIR/exact_face-sheet.png" \
  "$OUT_DIR/stored_normal-sheet.png" "$OUT_DIR/exact_both-sheet.png"

jq . "$OUT_DIR/summary.json"
printf 'exact voxel material experiment: %s\n' "$OUT_DIR"
