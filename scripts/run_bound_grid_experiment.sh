#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/bound-grid/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-5}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-16}"
BOUNCE_CAP="${BOUNCE_CAP:-1}"

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
  "$ROOT_DIR/scenes/benchmarks/Exact_Box.json"
  "$ROOT_DIR/scenes/benchmarks/Exact_CSG.json"
  "$ROOT_DIR/scenes/benchmarks/Exact_Plane.json"
)
VARIANTS=(sdf bound32 bound64 voxel)
COMMON_ARGS=(
  --width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
  --sdf-bounce-cap "$BOUNCE_CAP"
)

mkdir -p "$OUT_DIR/compare"
for variant in "${VARIANTS[@]}"; do
  mkdir -p "$OUT_DIR/$variant"
done

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
        --bound-grid-resolution 32 "${COMMON_ARGS[@]}"
      ;;
    bound64)
      "$BIN" render "$scene" --out "$output" --renderer bound-grid \
        --bound-grid-resolution 64 "${COMMON_ARGS[@]}"
      ;;
    voxel)
      "$BIN" render "$scene" --out "$output" --renderer voxel \
        --voxel-resolution 256 --voxel-storage sparse-bricks --voxel-build direct \
        --voxel-normal face "${COMMON_ARGS[@]}"
      ;;
  esac
}

for ((run = 0; run < RUNS; run++)); do
  for variant in "${VARIANTS[@]}"; do
    mkdir -p "$OUT_DIR/$variant/run_$run"
  done
  for scene in "${SCENES[@]}"; do
    if ((run % 2 == 0)); then
      order=(sdf bound32 bound64 voxel)
    else
      order=(voxel bound64 bound32 sdf)
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
  for variant in bound32 bound64 voxel; do
    "$BIN" compare "$OUT_DIR/sdf/run_0/$output" "$OUT_DIR/$variant/run_0/$output" \
      --report "$OUT_DIR/compare/$stem-$variant.json"
  done

  sdf_metadata=()
  bound32_metadata=()
  bound64_metadata=()
  voxel_metadata=()
  for ((run = 0; run < RUNS; run++)); do
    sdf_metadata+=("$OUT_DIR/sdf/run_$run/$output.render.json")
    bound32_metadata+=("$OUT_DIR/bound32/run_$run/$output.render.json")
    bound64_metadata+=("$OUT_DIR/bound64/run_$run/$output.render.json")
    voxel_metadata+=("$OUT_DIR/voxel/run_$run/$output.render.json")
  done
  jq -n --arg scene "$(basename "$scene" .json)" \
    --slurpfile sdf <(jq -s . "${sdf_metadata[@]}") \
    --slurpfile b32 <(jq -s . "${bound32_metadata[@]}") \
    --slurpfile b64 <(jq -s . "${bound64_metadata[@]}") \
    --slurpfile voxel <(jq -s . "${voxel_metadata[@]}") \
    --slurpfile q32 "$OUT_DIR/compare/$stem-bound32.json" \
    --slurpfile q64 "$OUT_DIR/compare/$stem-bound64.json" \
    --slurpfile qvoxel "$OUT_DIR/compare/$stem-voxel.json" '
      def median(values): values | sort | .[length / 2 | floor];
      {scene:$scene,
       sdf:{render_ms:median($sdf[0]|map(.elapsed_ms))},
       bound32:{render_ms:median($b32[0]|map(.elapsed_ms)),
                build_ms:median($b32[0]|map(.bound_grid_build_ms)),quality:$q32[0]},
       bound64:{render_ms:median($b64[0]|map(.elapsed_ms)),
                build_ms:median($b64[0]|map(.bound_grid_build_ms)),quality:$q64[0]},
       voxel:{render_ms:median($voxel[0]|map(.elapsed_ms)),
              build_ms:median($voxel[0]|map(.voxel_build_ms)),quality:$qvoxel[0]}}' \
    >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson width "$WIDTH" --argjson height "$HEIGHT" \
  --argjson samples "$SAMPLES" --argjson runs "$RUNS" \
  --argjson bounce_cap "$BOUNCE_CAP" '
  (map(.sdf.render_ms)|add) as $sdf |
  (map(.bound32.render_ms)|add) as $b32 |
  (map(.bound64.render_ms)|add) as $b64 |
  (map(.voxel.render_ms)|add) as $voxel |
  {settings:{width:$width,height:$height,samples:$samples,runs:$runs,bounce_cap:$bounce_cap,
             bound_grid_resolutions:[32,64],bound_format:"RG32Float"},
   scenes:.,
   aggregate:{sdf_render_ms:$sdf,bound32_render_ms:$b32,bound64_render_ms:$b64,
              voxel_render_ms:$voxel,sdf_over_bound32:($sdf/$b32),
              sdf_over_bound64:($sdf/$b64),sdf_over_voxel:($sdf/$voxel),
              bound32_mae:(map(.bound32.quality.mean_absolute_error)|add/length),
              bound64_mae:(map(.bound64.quality.mean_absolute_error)|add/length),
              bound32_lf_ssim:(map(.bound32.quality.low_frequency_luminance_ssim)|add/length),
              bound64_lf_ssim:(map(.bound64.quality.low_frequency_luminance_ssim)|add/length)}}' \
  "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

"$BIN" contact-sheet "$OUT_DIR/sdf-sheet.png" "$OUT_DIR"/sdf/run_0/*.png
"$BIN" contact-sheet "$OUT_DIR/bound32-sheet.png" "$OUT_DIR"/bound32/run_0/*.png
"$BIN" contact-sheet "$OUT_DIR/bound64-sheet.png" "$OUT_DIR"/bound64/run_0/*.png
"$BIN" contact-sheet "$OUT_DIR/voxel-sheet.png" "$OUT_DIR"/voxel/run_0/*.png
"$BIN" contact-sheet "$OUT_DIR/comparison-sheet.png" \
  "$OUT_DIR/sdf-sheet.png" "$OUT_DIR/bound32-sheet.png" \
  "$OUT_DIR/bound64-sheet.png" "$OUT_DIR/voxel-sheet.png"

jq . "$OUT_DIR/summary.json"
printf 'bound-grid experiment: %s\n' "$OUT_DIR"
