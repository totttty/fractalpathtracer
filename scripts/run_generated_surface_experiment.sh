#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/generated-surface/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-5}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-64}"
read -r -a COUNTS <<< "${COUNTS:-16 24 31 32}"
VARIANTS=(typed-soa topology-interpreted topology-surface stitch-inline)

command -v jq >/dev/null || { printf 'jq is required\n' >&2; exit 2; }
if [[ -e "$OUT_DIR" ]]; then
  printf 'refusing to overwrite existing report directory: %s\n' "$OUT_DIR" >&2
  exit 2
fi
cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then cargo build --release; fi
mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/compare" "$OUT_DIR/cache"
for variant in "${VARIANTS[@]}"; do mkdir -p "$OUT_DIR/$variant"; done
export FPT_STITCH_CACHE_DIR="$OUT_DIR/cache"

for primitive_count in "${COUNTS[@]}"; do
  jq --argjson primitive_count "$primitive_count" \
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
    --arg output "Generated_Surface_${primitive_count}.png" '
      def center($index):
        [((($index % 4) - 1.5) * 1.25),
         ((((($index / 4) | floor) % 4) - 1.5) * 1.05),
         (((($index / 16) | floor) - 0.5) * 1.25)];
      (reduce range(0; $primitive_count) as $index
        ({previous:[0,0,0],operations:[]};
         (center($index)) as $current |
         .operations += [
           {op:"translate",value:[($current[0] - .previous[0]),
                                  ($current[1] - .previous[1]),
                                  ($current[2] - .previous[2])]},
           {op:"sphere",radius:(0.37 + (($index * 7 % 11) / 100))}
         ] | .previous = $current)) as $program |
      .preset = $preset | .output = $output |
      .camera.position = [0.0,0.15,-7.2] | .camera.focus_distance = 7.2 |
      .sdf_program.operations = $program.operations |
      .sdf_program.material.mode = "constant" |
      .sdf_program.material.color = [0.15,0.62,0.95]
    ' "$ROOT_DIR/scenes/benchmarks/Exact_Sphere.json" \
    > "$OUT_DIR/fixtures/union_${primitive_count}.json"
done

COMMON_ARGS=(--width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
  --sdf-bounce-cap 1 --sdf-program-optimization basic --renderer sdf
  --sdf-normal-mode program-gradient)
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
      [[ "$variant" == topology-interpreted ]] && \
        extra=(--sdf-topology-specialization --no-sdf-generated-surface)
      [[ "$variant" == topology-surface ]] && \
        extra=(--sdf-topology-specialization)
      [[ "$variant" == stitch-inline ]] && \
        extra=(--sdf-function-stitching inline)
      "$BIN" render "$fixture" --out "$OUT_DIR/$variant/run_$run" \
        "${COMMON_ARGS[@]}" "${extra[@]}"
    done
  done
done

validation_dir="$OUT_DIR/validation"
mkdir -p "$validation_dir"
validation_count="${COUNTS[${#COUNTS[@]} - 1]}"
"$BIN" render "$OUT_DIR/fixtures/union_${validation_count}.json" --out "$validation_dir" \
  --width 96 --height 54 --samples 1 --sdf-topology-specialization \
  --sdf-program-validation --sdf-normal-mode program-gradient

: > "$OUT_DIR/records.jsonl"
for primitive_count in "${COUNTS[@]}"; do
  output="Generated_Surface_${primitive_count}.png"
  for variant in topology-interpreted topology-surface stitch-inline; do
    "$BIN" compare "$OUT_DIR/typed-soa/run_0/$output" \
      "$OUT_DIR/$variant/run_0/$output" \
      --report "$OUT_DIR/compare/${primitive_count}-${variant}.json"
  done
  jq -n --argjson primitive_count "$primitive_count" \
    --slurpfile soa <(jq -s . "$OUT_DIR"/typed-soa/run_*/"$output.render.json") \
    --slurpfile interpreted <(jq -s . "$OUT_DIR"/topology-interpreted/run_*/"$output.render.json") \
    --slurpfile surface <(jq -s . "$OUT_DIR"/topology-surface/run_*/"$output.render.json") \
    --slurpfile stitch <(jq -s . "$OUT_DIR"/stitch-inline/run_*/"$output.render.json") \
    --slurpfile qi "$OUT_DIR/compare/${primitive_count}-topology-interpreted.json" \
    --slurpfile qs "$OUT_DIR/compare/${primitive_count}-topology-surface.json" \
    --slurpfile qst "$OUT_DIR/compare/${primitive_count}-stitch-inline.json" '
      def median(values): values | sort | .[length / 2 | floor];
      def generated(values;quality):
        {ms:median(values|map(.elapsed_ms)),
         build_ms:median(values|map(.sdf_topology_build_ms)),
         first_build_ms:values[0].sdf_topology_build_ms,
         maximum_build_ms:(values|map(.sdf_topology_build_ms)|max),
         warm_build_ms:(if (values|length) > 1 then
           median(values[1:]|map(.sdf_topology_build_ms)) else null end),
         quality:quality};
      {primitive_count:$primitive_count,
       instruction_count:$soa[0][0].sdf_program_instruction_count,
       typed_soa_ms:median($soa[0]|map(.elapsed_ms)),
       topology_interpreted:generated($interpreted[0];$qi[0]),
       topology_surface:generated($surface[0];$qs[0]),
       stitch_inline:{ms:median($stitch[0]|map(.elapsed_ms)),quality:$qst[0]}} |
      . + {surface_vs_typed_soa:(.typed_soa_ms/.topology_surface.ms),
           surface_vs_interpreted:(.topology_interpreted.ms/.topology_surface.ms),
           surface_vs_stitch:(.stitch_inline.ms/.topology_surface.ms)}
    ' >> "$OUT_DIR/records.jsonl"
done

validation_output="$validation_dir/Generated_Surface_${validation_count}.png.render.json"
jq -s --argjson runs "$RUNS" --argjson samples "$SAMPLES" \
  --argjson width "$WIDTH" --argjson height "$HEIGHT" \
  --slurpfile validator "$validation_output" '
  {settings:{runs:$runs,width:$width,height:$height,samples:$samples},rows:.,
   differential_validation:$validator[0].sdf_stitch_validation_stats,
   validation:{maximum_mae:([.[]|.topology_interpreted.quality.mean_absolute_error,
     .topology_surface.quality.mean_absolute_error,.stitch_inline.quality.mean_absolute_error]|max),
     minimum_ssim:([.[]|.topology_interpreted.quality.luminance_ssim,
       .topology_surface.quality.luminance_ssim,.stitch_inline.quality.luminance_ssim]|min),
     minimum_lf_ssim:([.[]|.topology_interpreted.quality.low_frequency_luminance_ssim,
       .topology_surface.quality.low_frequency_luminance_ssim,
       .stitch_inline.quality.low_frequency_luminance_ssim]|min)}}
' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"
jq -e '.differential_validation.distance_failures == 0 and
       .differential_validation.gradient_failures == 0 and
       .validation.maximum_mae <= 0.0001 and
       .validation.minimum_ssim >= 0.999999 and
       .validation.minimum_lf_ssim >= 0.999999' \
  "$OUT_DIR/summary.json" >/dev/null
jq . "$OUT_DIR/summary.json"
printf 'Generated-surface experiment: %s\n' "$OUT_DIR"
