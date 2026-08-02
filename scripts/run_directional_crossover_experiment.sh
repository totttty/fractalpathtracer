#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/bound-grid-directional-crossover/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-5}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-16}"
BOUNCE_CAP="${BOUNCE_CAP:-1}"
PROFILE_STRIDE="${PROFILE_STRIDE:-4}"
read -r -a COUNTS <<< "${COUNTS_STRING:-1 2 4 8 16 32 48 64}"

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

mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/sdf" "$OUT_DIR/sdf_optimized" \
  "$OUT_DIR/range32" "$OUT_DIR/directional32" \
  "$OUT_DIR/directional32_optimized" "$OUT_DIR/compare"
for count in "${COUNTS[@]}"; do
  if [[ ! "$count" =~ ^[0-9]+$ ]] || ((count < 1 || count > 64)); then
    printf 'instruction counts must be integers in 1..64: %s\n' "$count" >&2
    exit 2
  fi
  jq --argjson count "$count" \
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
    --arg output "Directional_Cost_${count}.png" '
      .output = $output |
      .preset = $preset |
      .sdf_program.operations = [range(0; $count) | {op:"sphere", radius:1.0}]
    ' "$ROOT_DIR/scenes/benchmarks/Exact_Sphere.json" \
    > "$OUT_DIR/fixtures/cost_${count}.json"
done

COMMON_ARGS=(
  --width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
  --sdf-bounce-cap "$BOUNCE_CAP"
)

render_variant() {
  local variant="$1"
  local fixture="$2"
  local output="$3"
  case "$variant" in
    sdf)
      "$BIN" render "$fixture" --out "$output" --renderer sdf \
        --sdf-program-optimization off "${COMMON_ARGS[@]}"
      ;;
    sdf_optimized)
      "$BIN" render "$fixture" --out "$output" --renderer sdf \
        --sdf-program-optimization basic "${COMMON_ARGS[@]}"
      ;;
    range32)
      "$BIN" render "$fixture" --out "$output" --renderer bound-grid \
        --bound-grid-resolution 32 --sdf-program-optimization off \
        "${COMMON_ARGS[@]}"
      ;;
    directional32)
      "$BIN" render "$fixture" --out "$output" --renderer bound-grid \
        --bound-grid-resolution 32 --bound-grid-directional --bound-grid-profile \
        --bound-grid-profile-stride "$PROFILE_STRIDE" \
        --sdf-program-optimization off "${COMMON_ARGS[@]}"
      ;;
    directional32_optimized)
      "$BIN" render "$fixture" --out "$output" --renderer bound-grid \
        --bound-grid-resolution 32 --bound-grid-directional --bound-grid-profile \
        --bound-grid-profile-stride "$PROFILE_STRIDE" \
        --sdf-program-optimization basic "${COMMON_ARGS[@]}"
      ;;
  esac
}

for ((run = 0; run < RUNS; run++)); do
  for variant in sdf sdf_optimized range32 directional32 directional32_optimized; do
    mkdir -p "$OUT_DIR/$variant/run_$run"
  done
  for count in "${COUNTS[@]}"; do
    fixture="$OUT_DIR/fixtures/cost_${count}.json"
    if ((run % 2 == 0)); then
      order=(sdf sdf_optimized range32 directional32 directional32_optimized)
    else
      order=(directional32_optimized directional32 range32 sdf_optimized sdf)
    fi
    for variant in "${order[@]}"; do
      render_variant "$variant" "$fixture" "$OUT_DIR/$variant/run_$run"
    done
  done
done

: > "$OUT_DIR/records.jsonl"
for count in "${COUNTS[@]}"; do
  output="Directional_Cost_${count}.png"
  for variant in range32 directional32; do
    "$BIN" compare "$OUT_DIR/sdf/run_0/$output" \
      "$OUT_DIR/$variant/run_0/$output" \
      --report "$OUT_DIR/compare/cost_${count}-${variant}.json"
  done
  "$BIN" compare "$OUT_DIR/sdf/run_0/$output" \
    "$OUT_DIR/sdf_optimized/run_0/$output" \
    --report "$OUT_DIR/compare/cost_${count}-sdf-optimized.json"
  "$BIN" compare "$OUT_DIR/sdf_optimized/run_0/$output" \
    "$OUT_DIR/directional32_optimized/run_0/$output" \
    --report "$OUT_DIR/compare/cost_${count}-directional32-optimized.json"
  jq -n --argjson instruction_count "$count" \
    --slurpfile sdf <(jq -s . "$OUT_DIR"/sdf/run_*/"$output.render.json") \
    --slurpfile sdf_opt <(jq -s . "$OUT_DIR"/sdf_optimized/run_*/"$output.render.json") \
    --slurpfile range32 <(jq -s . "$OUT_DIR"/range32/run_*/"$output.render.json") \
    --slurpfile d32 <(jq -s . "$OUT_DIR"/directional32/run_*/"$output.render.json") \
    --slurpfile d32_opt <(jq -s . "$OUT_DIR"/directional32_optimized/run_*/"$output.render.json") \
    --slurpfile qr "$OUT_DIR/compare/cost_${count}-range32.json" \
    --slurpfile qd "$OUT_DIR/compare/cost_${count}-directional32.json" \
    --slurpfile qo "$OUT_DIR/compare/cost_${count}-sdf-optimized.json" \
    --slurpfile qdo "$OUT_DIR/compare/cost_${count}-directional32-optimized.json" '
      def median(values): values | sort | .[length / 2 | floor];
      {instruction_count:$instruction_count,
       sdf:{render_ms:median($sdf[0]|map(.elapsed_ms))},
       sdf_optimized:{render_ms:median($sdf_opt[0]|map(.elapsed_ms)),
                     instruction_count:$sdf_opt[0][0].sdf_program_instruction_count,
                     quality:$qo[0]},
       range32:{render_ms:median($range32[0]|map(.elapsed_ms)),quality:$qr[0]},
       directional32:{render_ms:median($d32[0]|map(.elapsed_ms)),
                      build_ms:median($d32[0]|map(.bound_grid_build_ms)),
                      quality:$qd[0],profile:$d32[0][0].bound_grid_profile},
       directional32_optimized:{render_ms:median($d32_opt[0]|map(.elapsed_ms)),
                                build_ms:median($d32_opt[0]|map(.bound_grid_build_ms)),
                                instruction_count:$d32_opt[0][0].sdf_program_instruction_count,
                                quality:$qdo[0],profile:$d32_opt[0][0].bound_grid_profile}}' \
    >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson width "$WIDTH" --argjson height "$HEIGHT" \
  --argjson samples "$SAMPLES" --argjson runs "$RUNS" \
  --argjson bounce_cap "$BOUNCE_CAP" '
    map(. + {sdf_over_range32:(.sdf.render_ms/.range32.render_ms),
             sdf_over_directional32:(.sdf.render_ms/.directional32.render_ms),
             sdf_over_sdf_optimized:(.sdf.render_ms/.sdf_optimized.render_ms),
             sdf_optimized_over_directional32_optimized:
               (.sdf_optimized.render_ms/.directional32_optimized.render_ms),
             range32_over_directional32:(.range32.render_ms/.directional32.render_ms)}) as $rows |
    {settings:{width:$width,height:$height,samples:$samples,runs:$runs,
               bounce_cap:$bounce_cap,geometry:"identical redundant sphere union"},
     crossover:{directional_beats_sdf:
                  ([$rows[]|select(.directional32.render_ms < .sdf.render_ms)|.instruction_count]|first),
                range_beats_sdf:
                  ([$rows[]|select(.range32.render_ms < .sdf.render_ms)|.instruction_count]|first),
                optimized_directional_beats_optimized_sdf:
                  ([$rows[]|select(.directional32_optimized.render_ms < .sdf_optimized.render_ms)|.instruction_count]|first)},
     rows:$rows,
     validation:{maximum_directional_mae:
                   ($rows|map(.directional32.quality.mean_absolute_error)|max),
                 minimum_directional_lf_ssim:
                   ($rows|map(.directional32.quality.low_frequency_luminance_ssim)|min),
                 maximum_optimizer_mae:
                   ($rows|map(.sdf_optimized.quality.mean_absolute_error)|max),
                 minimum_optimizer_lf_ssim:
                   ($rows|map(.sdf_optimized.quality.low_frequency_luminance_ssim)|min),
                 sampled_bound_failures:
                   ($rows|map(.directional32.profile.sampled_bound_failures)|add),
                 sampled_derivative_failures:
                   ($rows|map(.directional32.profile.sampled_derivative_failures)|add),
                 unknown_derivative_cells:
                   ($rows|map(.directional32.profile.unknown_derivative_cells)|add)}}' \
  "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

jq . "$OUT_DIR/summary.json"
printf 'Directional crossover experiment: %s\n' "$OUT_DIR"
