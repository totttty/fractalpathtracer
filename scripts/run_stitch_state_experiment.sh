#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/stitch-state/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-7}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-64}"
COUNTS=(24 28 32)
VARIANTS=(bytecode typed-soa full-surface full-distance lean-distance)

command -v jq >/dev/null || { printf 'jq is required\n' >&2; exit 2; }
if [[ -e "$OUT_DIR" ]]; then
  printf 'refusing to overwrite existing report directory: %s\n' "$OUT_DIR" >&2
  exit 2
fi
cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo build --release
fi
mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/cache" "$OUT_DIR/compare"
for variant in "${VARIANTS[@]}"; do mkdir -p "$OUT_DIR/$variant"; done
export FPT_STITCH_CACHE_DIR="$OUT_DIR/cache"

for primitive_count in "${COUNTS[@]}"; do
  jq --argjson primitive_count "$primitive_count" \
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
    --arg output "Stitch_State_${primitive_count}.png" '
      def center($index):
        [((($index % 4) - 1.5) * 1.25),
         ((((($index / 4) | floor) % 4) - 1.5) * 1.05),
         (((($index / 16) | floor) - 0.5) * 1.25)];
      (reduce range(0; $primitive_count) as $index
        ({previous:[0,0,0],operations:[]};
         (center($index)) as $current |
         .operations += [
           {op:"translate",
            value:[($current[0] - .previous[0]),
                   ($current[1] - .previous[1]),
                   ($current[2] - .previous[2])]},
           {op:"sphere",radius:(0.37 + (($index * 7 % 11) / 100))}
         ] | .previous = $current)) as $program |
      .preset = $preset |
      .output = $output |
      .camera.position = [0.0,0.15,-7.2] |
      .camera.focus_distance = 7.2 |
      .sdf_program.operations = $program.operations |
      .sdf_program.material.mode = "constant" |
      .sdf_program.material.color = [0.15,0.62,0.95]
    ' "$ROOT_DIR/scenes/benchmarks/Exact_Sphere.json" \
    > "$OUT_DIR/fixtures/union_${primitive_count}.json"
done

COMMON_ARGS=(--width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
  --sdf-bounce-cap 1 --sdf-program-optimization basic --renderer sdf)
for ((run = 0; run < RUNS; run++)); do
  offset=$((run % ${#VARIANTS[@]}))
  order=()
  for ((index = 0; index < ${#VARIANTS[@]}; index++)); do
    order+=("${VARIANTS[$(((index + offset) % ${#VARIANTS[@]}))]}")
  done
  for variant in "${VARIANTS[@]}"; do mkdir -p "$OUT_DIR/$variant/run_$run"; done
  for primitive_count in "${COUNTS[@]}"; do
    fixture="$OUT_DIR/fixtures/union_${primitive_count}.json"
    for variant in "${order[@]}"; do
      extra=()
      [[ "$variant" == typed-soa ]] && extra=(--sdf-typed-soa)
      [[ "$variant" == full-surface ]] && extra=(--sdf-function-stitching inline)
      [[ "$variant" == full-distance ]] && \
        extra=(--sdf-function-stitching inline --no-sdf-stitched-surface)
      [[ "$variant" == lean-distance ]] && \
        extra=(--sdf-function-stitching inline --sdf-stitch-distance-only)
      "$BIN" render "$fixture" --out "$OUT_DIR/$variant/run_$run" \
        "${COMMON_ARGS[@]}" "${extra[@]}"
    done
  done
done

: > "$OUT_DIR/records.jsonl"
for primitive_count in "${COUNTS[@]}"; do
  output="Stitch_State_${primitive_count}.png"
  for variant in typed-soa full-surface full-distance lean-distance; do
    "$BIN" compare "$OUT_DIR/bytecode/run_0/$output" \
      "$OUT_DIR/$variant/run_0/$output" \
      --report "$OUT_DIR/compare/${primitive_count}-${variant}.json"
  done
  jq -n --argjson primitive_count "$primitive_count" \
    --slurpfile bytecode <(jq -s . "$OUT_DIR"/bytecode/run_*/"$output.render.json") \
    --slurpfile soa <(jq -s . "$OUT_DIR"/typed-soa/run_*/"$output.render.json") \
    --slurpfile full_surface <(jq -s . "$OUT_DIR"/full-surface/run_*/"$output.render.json") \
    --slurpfile full_distance <(jq -s . "$OUT_DIR"/full-distance/run_*/"$output.render.json") \
    --slurpfile lean <(jq -s . "$OUT_DIR"/lean-distance/run_*/"$output.render.json") \
    --slurpfile soa_quality "$OUT_DIR/compare/${primitive_count}-typed-soa.json" \
    --slurpfile full_surface_quality "$OUT_DIR/compare/${primitive_count}-full-surface.json" \
    --slurpfile full_distance_quality "$OUT_DIR/compare/${primitive_count}-full-distance.json" \
    --slurpfile lean_quality "$OUT_DIR/compare/${primitive_count}-lean-distance.json" '
      def median(values): values | sort | .[length / 2 | floor];
      {primitive_count:$primitive_count,
       source_instruction_count:$bytecode[0][0].sdf_program_instruction_count,
       bytecode_ms:median($bytecode[0]|map(.elapsed_ms)),
       typed_soa_ms:median($soa[0]|map(.elapsed_ms)),
       full_surface_ms:median($full_surface[0]|map(.elapsed_ms)),
       full_distance_ms:median($full_distance[0]|map(.elapsed_ms)),
       lean_distance_ms:median($lean[0]|map(.elapsed_ms)),
       full_surface_max_build_ms:($full_surface[0]|map(.sdf_stitched_library_build_ms)|max),
       full_distance_max_build_ms:($full_distance[0]|map(.sdf_stitched_library_build_ms)|max),
       lean_distance_max_build_ms:($lean[0]|map(.sdf_stitched_library_build_ms)|max),
       full_pipeline:$full_surface[0][0].sdf_stitch_pipeline_stats,
       lean_pipeline:$lean[0][0].sdf_stitch_pipeline_stats,
       quality:{typed_soa:$soa_quality[0],full_surface:$full_surface_quality[0],
                full_distance:$full_distance_quality[0],lean_distance:$lean_quality[0]}} |
      . + {lean_vs_full_surface:(.full_surface_ms/.lean_distance_ms),
           lean_vs_full_distance:(.full_distance_ms/.lean_distance_ms),
           lean_vs_typed_soa:(.typed_soa_ms/.lean_distance_ms),
           full_distance_vs_full_surface:(.full_surface_ms/.full_distance_ms)}
    ' >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson runs "$RUNS" --argjson samples "$SAMPLES" \
  --argjson width "$WIDTH" --argjson height "$HEIGHT" '
  {settings:{runs:$runs,width:$width,height:$height,samples:$samples},rows:.,
   validation:{maximum_mae:([.[]|.quality[]|.mean_absolute_error]|max),
     minimum_ssim:([.[]|.quality[]|.luminance_ssim]|min),
     minimum_lf_ssim:([.[]|.quality[]|.low_frequency_luminance_ssim]|min)}}
' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"
jq -e '
  .validation.maximum_mae <= 0.0001 and
  .validation.minimum_ssim >= 0.999999 and
  .validation.minimum_lf_ssim >= 0.999999
' "$OUT_DIR/summary.json" >/dev/null
jq . "$OUT_DIR/summary.json"
printf 'Stitch-state experiment: %s\n' "$OUT_DIR"
