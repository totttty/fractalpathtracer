#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/voxel-optimization/fractal-leaf-$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-3}"
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
VARIANTS=(baseline fixed_de exact_normal fixed_de_exact)
VOXEL_ARGS=(
  --renderer voxel --voxel-resolution 256 --voxel-storage sparse-bricks
  --voxel-coverage legacy --voxel-build direct
  --width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
)
SDF_ARGS=(--renderer sdf --width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES")

mkdir -p "$OUT_DIR/sdf" "$OUT_DIR/compare"
for variant in "${VARIANTS[@]}"; do
  mkdir -p "$OUT_DIR/$variant"
done

for ((run = 0; run < RUNS; run++)); do
  if ((run % 2 == 0)); then
    order=(baseline fixed_de exact_normal fixed_de_exact)
  else
    order=(fixed_de_exact exact_normal fixed_de baseline)
  fi
  for variant in "${order[@]}"; do
    mkdir -p "$OUT_DIR/$variant/run_$run"
    case "$variant" in
      baseline) mode_args=(--voxel-normal face --voxel-leaf-refinement none) ;;
      fixed_de) mode_args=(--voxel-normal face --voxel-leaf-refinement fixed-de) ;;
      exact_normal) mode_args=(--voxel-normal exact --voxel-leaf-refinement none) ;;
      fixed_de_exact) mode_args=(--voxel-normal exact --voxel-leaf-refinement fixed-de) ;;
      *) exit 2 ;;
    esac
    for scene in "${SCENES[@]}"; do
      "$BIN" render "$scene" --out "$OUT_DIR/$variant/run_$run" \
        "${mode_args[@]}" "${VOXEL_ARGS[@]}"
    done
  done
done

for scene in "${SCENES[@]}"; do
  "$BIN" render "$scene" --out "$OUT_DIR/sdf" "${SDF_ARGS[@]}"
done

: > "$OUT_DIR/records.jsonl"
for scene in "${SCENES[@]}"; do
  output="$(jq -r .output "$scene")"
  stem="${output%.png}"
  for variant in "${VARIANTS[@]}"; do
    "$BIN" compare "$OUT_DIR/sdf/$output" "$OUT_DIR/$variant/run_0/$output" \
      --report "$OUT_DIR/compare/$stem-sdf-$variant.json"
    if [[ "$variant" != baseline ]]; then
      "$BIN" compare "$OUT_DIR/baseline/run_0/$output" "$OUT_DIR/$variant/run_0/$output" \
        --report "$OUT_DIR/compare/$stem-baseline-$variant.json"
    fi
  done

  jq -n --arg scene "$(basename "$scene" .json)" '{scene:$scene}' > "$OUT_DIR/$stem-record.json"
  for variant in "${VARIANTS[@]}"; do
    metadata=()
    for ((run = 0; run < RUNS; run++)); do
      metadata+=("$OUT_DIR/$variant/run_$run/$output.render.json")
    done
    jq -s --arg variant "$variant" --slurpfile quality "$OUT_DIR/compare/$stem-sdf-$variant.json" '
      def median(values): values | sort | .[length / 2 | floor];
      {key:$variant,value:{render_ms:median(map(.elapsed_ms)),quality:$quality[0]}}' \
      "${metadata[@]}" > "$OUT_DIR/$stem-$variant.json"
  done
  jq -n --slurpfile base "$OUT_DIR/$stem-record.json" \
    --slurpfile baseline "$OUT_DIR/$stem-baseline.json" \
    --slurpfile fixed_de "$OUT_DIR/$stem-fixed_de.json" \
    --slurpfile exact_normal "$OUT_DIR/$stem-exact_normal.json" \
    --slurpfile fixed_de_exact "$OUT_DIR/$stem-fixed_de_exact.json" '
      $base[0] + {variants:([$baseline[0],$fixed_de[0],$exact_normal[0],$fixed_de_exact[0]]|from_entries)}' \
      >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson width "$WIDTH" --argjson height "$HEIGHT" \
  --argjson samples "$SAMPLES" --argjson runs "$RUNS" '
  def total_time(key): map(.variants[key].render_ms)|add;
  def mean_mae(key): (map(.variants[key].quality.mean_absolute_error)|add)/length;
  def mean_lf_ssim(key): (map(.variants[key].quality.low_frequency_luminance_ssim)|add)/length;
  (total_time("baseline")) as $base_time |
  {
    settings:{resolution:256,bounds_min:[-3.25,-3.25,-3.25],bounds_max:[3.25,3.25,3.25],
      surface_band:0.5,width:$width,height:$height,samples:$samples,runs:$runs},
    scenes:.,
    aggregate:{
      baseline:{render_ms:$base_time,mae:mean_mae("baseline"),lf_ssim:mean_lf_ssim("baseline")},
      fixed_de:{render_ms:total_time("fixed_de"),speed_ratio:($base_time/total_time("fixed_de")),
        mae:mean_mae("fixed_de"),lf_ssim:mean_lf_ssim("fixed_de")},
      exact_normal:{render_ms:total_time("exact_normal"),speed_ratio:($base_time/total_time("exact_normal")),
        mae:mean_mae("exact_normal"),lf_ssim:mean_lf_ssim("exact_normal")},
      fixed_de_exact:{render_ms:total_time("fixed_de_exact"),speed_ratio:($base_time/total_time("fixed_de_exact")),
        mae:mean_mae("fixed_de_exact"),lf_ssim:mean_lf_ssim("fixed_de_exact")}
    }
  }' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

"$BIN" contact-sheet "$OUT_DIR/sdf-sheet.png" "$OUT_DIR"/sdf/*.png
for variant in "${VARIANTS[@]}"; do
  "$BIN" contact-sheet "$OUT_DIR/$variant-sheet.png" "$OUT_DIR/$variant"/run_0/*.png
done
"$BIN" contact-sheet "$OUT_DIR/ablation-sheet.png" \
  "$OUT_DIR/sdf-sheet.png" "$OUT_DIR/baseline-sheet.png" "$OUT_DIR/fixed_de-sheet.png" \
  "$OUT_DIR/exact_normal-sheet.png" "$OUT_DIR/fixed_de_exact-sheet.png"

jq . "$OUT_DIR/summary.json"
printf 'fractal leaf experiment: %s\n' "$OUT_DIR"
