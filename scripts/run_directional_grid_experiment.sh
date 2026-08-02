#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/bound-grid-directional/$RUN_STAMP}"
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
  "$ROOT_DIR/scenes/benchmarks/Exact_Sphere.json"
  "$ROOT_DIR/scenes/benchmarks/Exact_Plane.json"
  "$ROOT_DIR/scenes/benchmarks/Exact_Box.json"
  "$ROOT_DIR/scenes/benchmarks/Exact_CSG.json"
)
COMMON_ARGS=(
  --width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
  --sdf-bounce-cap "$BOUNCE_CAP"
)
mkdir -p "$OUT_DIR/sdf" "$OUT_DIR/range32" \
  "$OUT_DIR/directional32" "$OUT_DIR/directional64" "$OUT_DIR/compare"

render_variant() {
  local variant="$1"
  local scene="$2"
  local output="$3"
  case "$variant" in
    sdf)
      "$BIN" render "$scene" --out "$output" --renderer sdf "${COMMON_ARGS[@]}"
      ;;
    range32)
      "$BIN" render "$scene" --out "$output" --renderer bound-grid \
        --bound-grid-resolution 32 "${COMMON_ARGS[@]}"
      ;;
    directional32)
      "$BIN" render "$scene" --out "$output" --renderer bound-grid \
        --bound-grid-resolution 32 --bound-grid-directional --bound-grid-profile \
        --bound-grid-profile-stride "$PROFILE_STRIDE" "${COMMON_ARGS[@]}"
      ;;
    directional64)
      "$BIN" render "$scene" --out "$output" --renderer bound-grid \
        --bound-grid-resolution 64 --bound-grid-directional --bound-grid-profile \
        --bound-grid-profile-stride "$PROFILE_STRIDE" "${COMMON_ARGS[@]}"
      ;;
  esac
}

for ((run = 0; run < RUNS; run++)); do
  for variant in sdf range32 directional32 directional64; do
    mkdir -p "$OUT_DIR/$variant/run_$run"
  done
  for scene in "${SCENES[@]}"; do
    if ((run % 2 == 0)); then
      order=(sdf range32 directional32 directional64)
    else
      order=(directional64 directional32 range32 sdf)
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
  for variant in range32 directional32 directional64; do
    "$BIN" compare "$OUT_DIR/sdf/run_0/$output" \
      "$OUT_DIR/$variant/run_0/$output" \
      --report "$OUT_DIR/compare/$stem-$variant.json"
  done
  jq -n --arg scene "$(basename "$scene" .json)" \
    --slurpfile sdf <(jq -s . "$OUT_DIR"/sdf/run_*/"$output.render.json") \
    --slurpfile range32 <(jq -s . "$OUT_DIR"/range32/run_*/"$output.render.json") \
    --slurpfile d32 <(jq -s . "$OUT_DIR"/directional32/run_*/"$output.render.json") \
    --slurpfile d64 <(jq -s . "$OUT_DIR"/directional64/run_*/"$output.render.json") \
    --slurpfile qr "$OUT_DIR/compare/$stem-range32.json" \
    --slurpfile q32 "$OUT_DIR/compare/$stem-directional32.json" \
    --slurpfile q64 "$OUT_DIR/compare/$stem-directional64.json" '
      def median(values): values | sort | .[length / 2 | floor];
      {scene:$scene,
       sdf:{render_ms:median($sdf[0]|map(.elapsed_ms))},
       range32:{render_ms:median($range32[0]|map(.elapsed_ms)),quality:$qr[0]},
       directional32:{render_ms:median($d32[0]|map(.elapsed_ms)),
                      build_ms:median($d32[0]|map(.bound_grid_build_ms)),
                      memory_bytes:$d32[0][0].bound_grid_memory_bytes,
                      quality:$q32[0],profile:$d32[0][0].bound_grid_profile},
       directional64:{render_ms:median($d64[0]|map(.elapsed_ms)),
                      build_ms:median($d64[0]|map(.bound_grid_build_ms)),
                      memory_bytes:$d64[0][0].bound_grid_memory_bytes,
                      quality:$q64[0],profile:$d64[0][0].bound_grid_profile}}' \
    >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson width "$WIDTH" --argjson height "$HEIGHT" \
  --argjson samples "$SAMPLES" --argjson runs "$RUNS" \
  --argjson bounce_cap "$BOUNCE_CAP" '
    (map(.sdf.render_ms)|add) as $sdf |
    (map(.range32.render_ms)|add) as $range |
    (map(.directional32.render_ms)|add) as $d32 |
    (map(.directional64.render_ms)|add) as $d64 |
    {settings:{width:$width,height:$height,samples:$samples,runs:$runs,
               bounce_cap:$bounce_cap,
               directional_format:"RG32Float + 2xRGBA32Float"},
     scenes:.,
     aggregate:{sdf_render_ms:$sdf,range32_render_ms:$range,
                directional32_render_ms:$d32,directional64_render_ms:$d64,
                sdf_over_directional32:($sdf/$d32),
                sdf_over_directional64:($sdf/$d64),
                range32_over_directional32:($range/$d32),
                directional32_mae:(map(.directional32.quality.mean_absolute_error)|add/length),
                directional64_mae:(map(.directional64.quality.mean_absolute_error)|add/length),
                directional32_lf_ssim:(map(.directional32.quality.low_frequency_luminance_ssim)|add/length),
                directional64_lf_ssim:(map(.directional64.quality.low_frequency_luminance_ssim)|add/length),
                directional_steps:(map(.directional32.profile.directional_steps|add)|add),
                cell_exit_clamps:(map(.directional32.profile.cell_exit_clamps|add)|add),
                unknown_derivative_intervals:(map(.directional32.profile.unknown_derivative_intervals|add)|add),
                sampled_bound_failures:(map(.directional32.profile.sampled_bound_failures +
                                            .directional64.profile.sampled_bound_failures)|add),
                sampled_derivative_failures:(map(.directional32.profile.sampled_derivative_failures +
                                                 .directional64.profile.sampled_derivative_failures)|add)}}' \
  "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

for variant in sdf range32 directional32 directional64; do
  "$BIN" contact-sheet "$OUT_DIR/$variant-sheet.png" "$OUT_DIR"/$variant/run_0/*.png
done
"$BIN" contact-sheet "$OUT_DIR/comparison-sheet.png" \
  "$OUT_DIR/sdf-sheet.png" "$OUT_DIR/range32-sheet.png" \
  "$OUT_DIR/directional32-sheet.png" "$OUT_DIR/directional64-sheet.png"
jq . "$OUT_DIR/summary.json"
printf 'Directional bound-grid experiment: %s\n' "$OUT_DIR"
