#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/stitch-fusion/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-7}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-64}"
COUNTS=(31 32)
VARIANTS=(typed-soa fine-inline fused-one fused-pairs fused-double)

command -v jq >/dev/null || { printf 'jq is required\n' >&2; exit 2; }
if [[ -e "$OUT_DIR" ]]; then
  printf 'refusing to overwrite existing report directory: %s\n' "$OUT_DIR" >&2
  exit 2
fi
cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then cargo build --release; fi
mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/cache" "$OUT_DIR/compare"
for variant in "${VARIANTS[@]}"; do mkdir -p "$OUT_DIR/$variant"; done
export FPT_STITCH_CACHE_DIR="$OUT_DIR/cache"

for primitive_count in "${COUNTS[@]}"; do
  jq --argjson primitive_count "$primitive_count" \
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
    --arg output "Stitch_Fusion_${primitive_count}.png" '
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
      [[ "$variant" == fine-inline ]] && extra=(--sdf-function-stitching inline)
      [[ "$variant" == fused-one ]] && extra=(--sdf-function-stitching inline --sdf-stitch-fusion one-pair)
      [[ "$variant" == fused-pairs ]] && extra=(--sdf-function-stitching inline --sdf-stitch-fusion pairs)
      [[ "$variant" == fused-double ]] && extra=(--sdf-function-stitching inline --sdf-stitch-fusion double-pairs)
      "$BIN" render "$fixture" --out "$OUT_DIR/$variant/run_$run" \
        "${COMMON_ARGS[@]}" "${extra[@]}"
    done
  done
done

: > "$OUT_DIR/records.jsonl"
for primitive_count in "${COUNTS[@]}"; do
  output="Stitch_Fusion_${primitive_count}.png"
  for variant in fine-inline fused-one fused-pairs fused-double; do
    "$BIN" compare "$OUT_DIR/typed-soa/run_0/$output" \
      "$OUT_DIR/$variant/run_0/$output" \
      --report "$OUT_DIR/compare/${primitive_count}-${variant}.json"
  done
  jq -n --argjson primitive_count "$primitive_count" \
    --slurpfile soa <(jq -s . "$OUT_DIR"/typed-soa/run_*/"$output.render.json") \
    --slurpfile fine <(jq -s . "$OUT_DIR"/fine-inline/run_*/"$output.render.json") \
    --slurpfile one <(jq -s . "$OUT_DIR"/fused-one/run_*/"$output.render.json") \
    --slurpfile pairs <(jq -s . "$OUT_DIR"/fused-pairs/run_*/"$output.render.json") \
    --slurpfile double <(jq -s . "$OUT_DIR"/fused-double/run_*/"$output.render.json") \
    --slurpfile fine_quality "$OUT_DIR/compare/${primitive_count}-fine-inline.json" \
    --slurpfile one_quality "$OUT_DIR/compare/${primitive_count}-fused-one.json" \
    --slurpfile pairs_quality "$OUT_DIR/compare/${primitive_count}-fused-pairs.json" \
    --slurpfile double_quality "$OUT_DIR/compare/${primitive_count}-fused-double.json" '
      def median(values): values | sort | .[length / 2 | floor];
      def row(values):
        {ms:median(values|map(.elapsed_ms)),
         nodes:values[0].sdf_stitch_pipeline_stats.graph_node_count,
         max_build_ms:(values|map(.sdf_stitched_library_build_ms)|max)};
      {primitive_count:$primitive_count,
       source_instruction_count:$soa[0][0].sdf_program_instruction_count,
       typed_soa_ms:median($soa[0]|map(.elapsed_ms)),
       fine_inline:row($fine[0]),fused_one:row($one[0]),
       fused_pairs:row($pairs[0]),fused_double:row($double[0]),
       quality:{fine_inline:$fine_quality[0],fused_one:$one_quality[0],
                fused_pairs:$pairs_quality[0],fused_double:$double_quality[0]}} |
      . + {fine_vs_typed_soa:(.typed_soa_ms/.fine_inline.ms),
           one_vs_typed_soa:(.typed_soa_ms/.fused_one.ms),
           pairs_vs_typed_soa:(.typed_soa_ms/.fused_pairs.ms),
           double_vs_typed_soa:(.typed_soa_ms/.fused_double.ms)}
    ' >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson runs "$RUNS" --argjson samples "$SAMPLES" \
  --argjson width "$WIDTH" --argjson height "$HEIGHT" '
  . as $rows | ($rows[]|select(.primitive_count==31)) as $p31 |
  ($rows[]|select(.primitive_count==32)) as $p32 |
  {settings:{runs:$runs,width:$width,height:$height,samples:$samples},rows:$rows,
   transition:{fine_32_over_31:($p32.fine_inline.ms/$p31.fine_inline.ms),
     one_32_over_31:($p32.fused_one.ms/$p31.fused_one.ms),
     pairs_32_over_31:($p32.fused_pairs.ms/$p31.fused_pairs.ms),
     double_32_over_31:($p32.fused_double.ms/$p31.fused_double.ms)},
   validation:{maximum_mae:([$rows[]|.quality[]|.mean_absolute_error]|max),
     minimum_ssim:([$rows[]|.quality[]|.luminance_ssim]|min),
     minimum_lf_ssim:([$rows[]|.quality[]|.low_frequency_luminance_ssim]|min)}}
' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"
jq -e '.validation.maximum_mae <= 0.0001 and
       .validation.minimum_ssim >= 0.999999 and
       .validation.minimum_lf_ssim >= 0.999999' \
  "$OUT_DIR/summary.json" >/dev/null
jq . "$OUT_DIR/summary.json"
printf 'Stitch-fusion experiment: %s\n' "$OUT_DIR"
