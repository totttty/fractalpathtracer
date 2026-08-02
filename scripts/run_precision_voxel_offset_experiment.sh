#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/voxel-optimization/precision-voxel-offset-$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-5}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-16}"

command -v jq >/dev/null || { printf 'jq is required\n' >&2; exit 2; }
[[ ! -e "$OUT_DIR" ]] || { printf 'refusing to overwrite %s\n' "$OUT_DIR" >&2; exit 2; }
cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then cargo test; cargo build --release; fi

SCENES=(
  "$ROOT_DIR/scenes/readme/01-Render005.json"
  "$ROOT_DIR/scenes/readme/08-Render0ad03.json"
  "$ROOT_DIR/scenes/readme/09-Glass.json"
)
VARIANTS=(legacy_face precision_face legacy_normal precision_normal)
COMMON_ARGS=(
  --renderer voxel --voxel-resolution 256 --voxel-storage sparse-bricks
  --voxel-coverage legacy --voxel-build direct --voxel-leaf-refinement none
  --voxel-material stored --width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
)

mkdir -p "$OUT_DIR/sdf" "$OUT_DIR/compare"
for variant in "${VARIANTS[@]}"; do mkdir -p "$OUT_DIR/$variant"; done
for ((run = 0; run < RUNS; run++)); do
  if ((run % 2 == 0)); then
    order=(legacy_face precision_face legacy_normal precision_normal)
  else
    order=(precision_normal legacy_normal precision_face legacy_face)
  fi
  for variant in "${order[@]}"; do
    mkdir -p "$OUT_DIR/$variant/run_$run"
    case "$variant" in
      legacy_face) mode=(--voxel-normal face --voxel-offset legacy) ;;
      precision_face) mode=(--voxel-normal face --voxel-offset precision) ;;
      legacy_normal) mode=(--voxel-normal exact --voxel-offset legacy) ;;
      precision_normal) mode=(--voxel-normal exact --voxel-offset precision) ;;
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
  (total("legacy_face")) as $legacy_face_time |
  (total("precision_face")) as $precision_face_time |
  (total("legacy_normal")) as $legacy_normal_time |
  (total("precision_normal")) as $precision_normal_time |
  {settings:{resolution:256,bounds_min:[-3.25,-3.25,-3.25],bounds_max:[3.25,3.25,3.25],
     surface_band:0.5,width:$width,height:$height,samples:$samples,runs:$runs},scenes:.,aggregate:{
     legacy_face:{render_ms:$legacy_face_time,mae:mae("legacy_face"),lf_ssim:lf("legacy_face")},
     precision_face:{render_ms:$precision_face_time,speed_ratio:($legacy_face_time/$precision_face_time),
       mae:mae("precision_face"),lf_ssim:lf("precision_face")},
     legacy_normal:{render_ms:$legacy_normal_time,mae:mae("legacy_normal"),lf_ssim:lf("legacy_normal")},
     precision_normal:{render_ms:$precision_normal_time,speed_ratio:($legacy_normal_time/$precision_normal_time),
       mae:mae("precision_normal"),lf_ssim:lf("precision_normal")},
     promotion_gate:{minimum_speed_ratio:0.95,require_improvement_with_both_normal_modes:true},
     passes_promotion:(($legacy_face_time/$precision_face_time)>=0.95 and
       ($legacy_normal_time/$precision_normal_time)>=0.95 and mae("precision_face")<mae("legacy_face") and
       mae("precision_normal")<mae("legacy_normal") and lf("precision_face")>lf("legacy_face") and
       lf("precision_normal")>lf("legacy_normal"))}}' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

"$BIN" contact-sheet "$OUT_DIR/sdf-sheet.png" "$OUT_DIR"/sdf/*.png
for variant in "${VARIANTS[@]}"; do
  "$BIN" contact-sheet "$OUT_DIR/$variant-sheet.png" "$OUT_DIR/$variant"/run_0/*.png
done
"$BIN" contact-sheet "$OUT_DIR/comparison-sheet.png" "$OUT_DIR/sdf-sheet.png" \
  "$OUT_DIR/legacy_face-sheet.png" "$OUT_DIR/precision_face-sheet.png" \
  "$OUT_DIR/legacy_normal-sheet.png" "$OUT_DIR/precision_normal-sheet.png"

jq . "$OUT_DIR/summary.json"
printf 'precision voxel offset experiment: %s\n' "$OUT_DIR"
