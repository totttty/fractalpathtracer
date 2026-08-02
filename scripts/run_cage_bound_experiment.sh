#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/bound-grid-cage/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-3}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-16}"
BOUNCE_CAP="${BOUNCE_CAP:-1}"
PROFILE_STRIDE="${PROFILE_STRIDE:-4}"

command -v jq >/dev/null || { printf 'jq is required\n' >&2; exit 2; }
if [[ -e "$OUT_DIR" ]]; then
  printf 'refusing to overwrite existing report directory: %s\n' "$OUT_DIR" >&2
  exit 2
fi
cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo test
  cargo build --release
fi

SCENES=(
  "$ROOT_DIR/scenes/readme/01-Render005.json"
  "$ROOT_DIR/scenes/readme/09-Glass.json"
)
COMMON_ARGS=(
  --width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
  --sdf-bounce-cap "$BOUNCE_CAP"
)
mkdir -p "$OUT_DIR/sdf" "$OUT_DIR/bound32" "$OUT_DIR/bound64" "$OUT_DIR/compare"

render_variant() {
  local variant="$1"
  local scene="$2"
  local output="$3"
  case "$variant" in
    sdf)
      "$BIN" render "$scene" --out "$output" --renderer sdf "${COMMON_ARGS[@]}"
      ;;
    bound32)
      "$BIN" render "$scene" --out "$output" --renderer bound-grid \
        --bound-grid-resolution 32 --bound-grid-cage-bounds --bound-grid-profile \
        --bound-grid-profile-stride "$PROFILE_STRIDE" "${COMMON_ARGS[@]}"
      ;;
    bound64)
      "$BIN" render "$scene" --out "$output" --renderer bound-grid \
        --bound-grid-resolution 64 --bound-grid-cage-bounds --bound-grid-profile \
        --bound-grid-profile-stride "$PROFILE_STRIDE" "${COMMON_ARGS[@]}"
      ;;
  esac
}

for ((run = 0; run < RUNS; run++)); do
  mkdir -p "$OUT_DIR/sdf/run_$run" "$OUT_DIR/bound32/run_$run" "$OUT_DIR/bound64/run_$run"
  for scene in "${SCENES[@]}"; do
    if ((run % 2 == 0)); then
      order=(sdf bound32 bound64)
    else
      order=(bound64 bound32 sdf)
    fi
    for variant in "${order[@]}"; do
      render_variant "$variant" "$scene" "$OUT_DIR/$variant/run_$run"
    done
  done
done

: > "$OUT_DIR/records.jsonl"
for scene in "${SCENES[@]}"; do
  output="$(jq -r .output "$scene")"
  stem="${output%.png}"
  "$BIN" compare "$OUT_DIR/sdf/run_0/$output" "$OUT_DIR/bound32/run_0/$output" \
    --report "$OUT_DIR/compare/$stem-bound32.json"
  "$BIN" compare "$OUT_DIR/sdf/run_0/$output" "$OUT_DIR/bound64/run_0/$output" \
    --report "$OUT_DIR/compare/$stem-bound64.json"
  jq -n --arg scene "$(basename "$scene" .json)" \
    --slurpfile sdf <(jq -s . "$OUT_DIR"/sdf/run_*/"$output.render.json") \
    --slurpfile b32 <(jq -s . "$OUT_DIR"/bound32/run_*/"$output.render.json") \
    --slurpfile b64 <(jq -s . "$OUT_DIR"/bound64/run_*/"$output.render.json") \
    --slurpfile q32 "$OUT_DIR/compare/$stem-bound32.json" \
    --slurpfile q64 "$OUT_DIR/compare/$stem-bound64.json" '
      def median(values): values | sort | .[length / 2 | floor];
      {scene:$scene,
       sdf:{render_ms:median($sdf[0]|map(.elapsed_ms))},
       bound32:{render_ms:median($b32[0]|map(.elapsed_ms)),
                build_ms:median($b32[0]|map(.bound_grid_build_ms)),
                quality:$q32[0],profile:$b32[0][0].bound_grid_profile},
       bound64:{render_ms:median($b64[0]|map(.elapsed_ms)),
                build_ms:median($b64[0]|map(.bound_grid_build_ms)),
                quality:$q64[0],profile:$b64[0][0].bound_grid_profile}}' \
    >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson width "$WIDTH" --argjson height "$HEIGHT" \
  --argjson samples "$SAMPLES" --argjson runs "$RUNS" \
  --argjson bounce_cap "$BOUNCE_CAP" '
    (map(.sdf.render_ms)|add) as $sdf |
    (map(.bound32.render_ms)|add) as $b32 |
    (map(.bound64.render_ms)|add) as $b64 |
    {settings:{width:$width,height:$height,samples:$samples,runs:$runs,
               bounce_cap:$bounce_cap,format:"RG32Float",cage_bounds:true},
     scenes:.,
     aggregate:{sdf_render_ms:$sdf,bound32_render_ms:$b32,bound64_render_ms:$b64,
                sdf_over_bound32:($sdf/$b32),sdf_over_bound64:($sdf/$b64),
                bound32_mae:(map(.bound32.quality.mean_absolute_error)|add/length),
                bound64_mae:(map(.bound64.quality.mean_absolute_error)|add/length),
                bound32_lf_ssim:(map(.bound32.quality.low_frequency_luminance_ssim)|add/length),
                bound64_lf_ssim:(map(.bound64.quality.low_frequency_luminance_ssim)|add/length),
                bound32_certified_skips:(map(.bound32.profile.certified_skips|add)|add),
                bound64_certified_skips:(map(.bound64.profile.certified_skips|add)|add),
                sampled_bound_failures:(map(.bound32.profile.sampled_bound_failures +
                                            .bound64.profile.sampled_bound_failures)|add),
                sampled_false_skips:(map(.bound32.profile.sampled_false_skips +
                                         .bound64.profile.sampled_false_skips)|add)}}' \
  "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

"$BIN" contact-sheet "$OUT_DIR/sdf-sheet.png" "$OUT_DIR"/sdf/run_0/*.png
"$BIN" contact-sheet "$OUT_DIR/bound32-sheet.png" "$OUT_DIR"/bound32/run_0/*.png
"$BIN" contact-sheet "$OUT_DIR/bound64-sheet.png" "$OUT_DIR"/bound64/run_0/*.png
"$BIN" contact-sheet "$OUT_DIR/comparison-sheet.png" \
  "$OUT_DIR/sdf-sheet.png" "$OUT_DIR/bound32-sheet.png" "$OUT_DIR/bound64-sheet.png"
jq . "$OUT_DIR/summary.json"
printf 'Cage bound-grid experiment: %s\n' "$OUT_DIR"
