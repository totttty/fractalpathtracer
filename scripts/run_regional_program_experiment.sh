#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/regional-program/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-5}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-16}"
BOUNCE_CAP="${BOUNCE_CAP:-1}"
read -r -a PRIMITIVE_COUNTS <<< "${PRIMITIVES_STRING:-8 16 24 32}"

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

mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/sdf" "$OUT_DIR/regional16" \
  "$OUT_DIR/regional32" "$OUT_DIR/compare"
for primitive_count in "${PRIMITIVE_COUNTS[@]}"; do
  if [[ ! "$primitive_count" =~ ^[0-9]+$ ]] ||
     ((primitive_count < 2 || primitive_count > 32)); then
    printf 'primitive counts must be integers in 2..32: %s\n' \
      "$primitive_count" >&2
    exit 2
  fi
  jq --argjson primitive_count "$primitive_count" \
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
    --arg output "Regional_Union_${primitive_count}.png" '
      def center($index):
        [((($index % 4) - 1.5) * 1.25),
         ((((($index / 4) | floor) % 4) - 1.5) * 1.05),
         (((($index / 16) | floor) - 0.5) * 1.25)];
      (reduce range(0; $primitive_count) as $index
        ({previous:[0,0,0], operations:[]};
         (center($index)) as $current |
         .operations += [
           {op:"translate",
            value:[($current[0] - .previous[0]),
                   ($current[1] - .previous[1]),
                   ($current[2] - .previous[2])]},
           {op:"sphere", radius:0.46}
         ] |
         .previous = $current)) as $program |
      .preset = $preset |
      .output = $output |
      .width = 320 |
      .height = 180 |
      .samples = 16 |
      .camera.position = [0.0, 0.15, -7.2] |
      .camera.focus_distance = 7.2 |
      .sdf_program.operations = $program.operations |
      .sdf_program.material.mode = "constant" |
      .sdf_program.material.color = [0.15, 0.62, 0.95]
    ' "$ROOT_DIR/scenes/benchmarks/Exact_Sphere.json" \
    > "$OUT_DIR/fixtures/union_${primitive_count}.json"
done

COMMON_ARGS=(
  --width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
  --sdf-bounce-cap "$BOUNCE_CAP" --sdf-program-optimization basic
)

render_variant() {
  local variant="$1"
  local fixture="$2"
  local output="$3"
  case "$variant" in
    sdf)
      "$BIN" render "$fixture" --out "$output" --renderer sdf \
        "${COMMON_ARGS[@]}"
      ;;
    regional16)
      "$BIN" render "$fixture" --out "$output" --renderer regional \
        --regional-program-resolution 16 --bound-grid-profile \
        "${COMMON_ARGS[@]}"
      ;;
    regional32)
      "$BIN" render "$fixture" --out "$output" --renderer regional \
        --regional-program-resolution 32 --bound-grid-profile \
        "${COMMON_ARGS[@]}"
      ;;
  esac
}

for ((run = 0; run < RUNS; run++)); do
  for variant in sdf regional16 regional32; do
    mkdir -p "$OUT_DIR/$variant/run_$run"
  done
  for primitive_count in "${PRIMITIVE_COUNTS[@]}"; do
    fixture="$OUT_DIR/fixtures/union_${primitive_count}.json"
    if ((run % 2 == 0)); then
      order=(sdf regional16 regional32)
    else
      order=(regional32 regional16 sdf)
    fi
    for variant in "${order[@]}"; do
      render_variant "$variant" "$fixture" "$OUT_DIR/$variant/run_$run"
    done
  done
done

: > "$OUT_DIR/records.jsonl"
for primitive_count in "${PRIMITIVE_COUNTS[@]}"; do
  output="Regional_Union_${primitive_count}.png"
  for variant in regional16 regional32; do
    "$BIN" compare "$OUT_DIR/sdf/run_0/$output" \
      "$OUT_DIR/$variant/run_0/$output" \
      --report "$OUT_DIR/compare/union_${primitive_count}-${variant}.json"
  done
  jq -n --argjson primitive_count "$primitive_count" \
    --slurpfile sdf <(jq -s . "$OUT_DIR"/sdf/run_*/"$output.render.json") \
    --slurpfile r16 <(jq -s . "$OUT_DIR"/regional16/run_*/"$output.render.json") \
    --slurpfile r32 <(jq -s . "$OUT_DIR"/regional32/run_*/"$output.render.json") \
    --slurpfile q16 "$OUT_DIR/compare/union_${primitive_count}-regional16.json" \
    --slurpfile q32 "$OUT_DIR/compare/union_${primitive_count}-regional32.json" '
      def median(values): values | sort | .[length / 2 | floor];
      {primitive_count:$primitive_count,
       source_instruction_count:$sdf[0][0].sdf_program_instruction_count,
       sdf:{render_ms:median($sdf[0]|map(.elapsed_ms))},
       regional16:{render_ms:median($r16[0]|map(.elapsed_ms)),
                   build_ms:median($r16[0]|map(.regional_program_build_ms)),
                   memory_bytes:$r16[0][0].regional_program_memory_bytes,
                   profile:$r16[0][0].regional_program_profile,
                   quality:$q16[0]},
       regional32:{render_ms:median($r32[0]|map(.elapsed_ms)),
                   build_ms:median($r32[0]|map(.regional_program_build_ms)),
                   memory_bytes:$r32[0][0].regional_program_memory_bytes,
                   profile:$r32[0][0].regional_program_profile,
                   quality:$q32[0]}}' >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson width "$WIDTH" --argjson height "$HEIGHT" \
  --argjson samples "$SAMPLES" --argjson runs "$RUNS" '
    map(. + {
      sdf_over_regional16:(.sdf.render_ms/.regional16.render_ms),
      sdf_over_regional32:(.sdf.render_ms/.regional32.render_ms),
      regional16_end_to_end_speedup:
        (.sdf.render_ms/(.regional16.render_ms + .regional16.build_ms)),
      regional32_end_to_end_speedup:
        (.sdf.render_ms/(.regional32.render_ms + .regional32.build_ms))
    }) as $rows |
    {settings:{width:$width,height:$height,samples:$samples,runs:$runs,
               geometry:"spatially distributed hard-union spheres",
               traversal:"unchanged procedural marcher"},
     crossover:{regional16_beats_sdf:
                  ([$rows[]|select(.regional16.render_ms < .sdf.render_ms)|
                    .primitive_count]|first),
                regional32_beats_sdf:
                  ([$rows[]|select(.regional32.render_ms < .sdf.render_ms)|
                    .primitive_count]|first)},
     validation:{maximum_mae:
                   ([$rows[].regional16.quality.mean_absolute_error,
                     $rows[].regional32.quality.mean_absolute_error]|max),
                 minimum_lf_ssim:
                   ([$rows[].regional16.quality.low_frequency_luminance_ssim,
                     $rows[].regional32.quality.low_frequency_luminance_ssim]|min),
                 sampled_distance_failures:
                   ([$rows[].regional16.profile.sampled_distance_failures,
                     $rows[].regional32.profile.sampled_distance_failures]|add)},
     rows:$rows}' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

jq . "$OUT_DIR/summary.json"
printf 'Regional program experiment: %s\n' "$OUT_DIR"
