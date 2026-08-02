#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/stitch-split/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-7}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-64}"
COUNTS=(24 28 31 32)
VARIANTS=(typed-soa full-inline lean-inline split-normal split-inline)

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
    --arg output "Stitch_Split_${primitive_count}.png" '
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
      [[ "$variant" == full-inline ]] && extra=(--sdf-function-stitching inline)
      [[ "$variant" == lean-inline ]] && \
        extra=(--sdf-function-stitching inline --sdf-stitch-distance-only)
      [[ "$variant" == split-normal ]] && \
        extra=(--sdf-function-stitching normal --sdf-stitch-split-graph)
      [[ "$variant" == split-inline ]] && \
        extra=(--sdf-function-stitching inline --sdf-stitch-split-graph)
      "$BIN" render "$fixture" --out "$OUT_DIR/$variant/run_$run" \
        "${COMMON_ARGS[@]}" "${extra[@]}"
    done
  done
done

: > "$OUT_DIR/records.jsonl"
for primitive_count in "${COUNTS[@]}"; do
  output="Stitch_Split_${primitive_count}.png"
  for variant in full-inline lean-inline split-normal split-inline; do
    "$BIN" compare "$OUT_DIR/typed-soa/run_0/$output" \
      "$OUT_DIR/$variant/run_0/$output" \
      --report "$OUT_DIR/compare/${primitive_count}-${variant}.json"
  done
  jq -n --argjson primitive_count "$primitive_count" \
    --slurpfile soa <(jq -s . "$OUT_DIR"/typed-soa/run_*/"$output.render.json") \
    --slurpfile full <(jq -s . "$OUT_DIR"/full-inline/run_*/"$output.render.json") \
    --slurpfile lean <(jq -s . "$OUT_DIR"/lean-inline/run_*/"$output.render.json") \
    --slurpfile split_normal <(jq -s . "$OUT_DIR"/split-normal/run_*/"$output.render.json") \
    --slurpfile split_inline <(jq -s . "$OUT_DIR"/split-inline/run_*/"$output.render.json") \
    --slurpfile full_quality "$OUT_DIR/compare/${primitive_count}-full-inline.json" \
    --slurpfile lean_quality "$OUT_DIR/compare/${primitive_count}-lean-inline.json" \
    --slurpfile split_normal_quality "$OUT_DIR/compare/${primitive_count}-split-normal.json" \
    --slurpfile split_inline_quality "$OUT_DIR/compare/${primitive_count}-split-inline.json" '
      def median(values): values | sort | .[length / 2 | floor];
      {primitive_count:$primitive_count,
       source_instruction_count:$soa[0][0].sdf_program_instruction_count,
       typed_soa_ms:median($soa[0]|map(.elapsed_ms)),
       full_inline_ms:median($full[0]|map(.elapsed_ms)),
       lean_inline_ms:median($lean[0]|map(.elapsed_ms)),
       split_normal_ms:median($split_normal[0]|map(.elapsed_ms)),
       split_inline_ms:median($split_inline[0]|map(.elapsed_ms)),
       split_normal_max_build_ms:($split_normal[0]|map(.sdf_stitched_library_build_ms)|max),
       split_inline_max_build_ms:($split_inline[0]|map(.sdf_stitched_library_build_ms)|max),
       quality:{full_inline:$full_quality[0],lean_inline:$lean_quality[0],
                split_normal:$split_normal_quality[0],split_inline:$split_inline_quality[0]}} |
      . + {split_normal_vs_typed_soa:(.typed_soa_ms/.split_normal_ms),
           split_inline_vs_typed_soa:(.typed_soa_ms/.split_inline_ms),
           split_normal_vs_inline:(.split_inline_ms/.split_normal_ms),
           split_inline_vs_monolithic_lean:(.lean_inline_ms/.split_inline_ms),
           split_inline_vs_monolithic_full:(.full_inline_ms/.split_inline_ms)}
    ' >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson runs "$RUNS" --argjson samples "$SAMPLES" \
  --argjson width "$WIDTH" --argjson height "$HEIGHT" '
  . as $rows |
  ($rows[]|select(.primitive_count==31)) as $p31 |
  ($rows[]|select(.primitive_count==32)) as $p32 |
  {settings:{runs:$runs,width:$width,height:$height,samples:$samples},rows:$rows,
   transition:{split_normal_32_over_31:($p32.split_normal_ms/$p31.split_normal_ms),
     split_inline_32_over_31:($p32.split_inline_ms/$p31.split_inline_ms),
     monolithic_full_32_over_31:($p32.full_inline_ms/$p31.full_inline_ms),
     monolithic_lean_32_over_31:($p32.lean_inline_ms/$p31.lean_inline_ms)},
   validation:{maximum_mae:([$rows[]|.quality[]|.mean_absolute_error]|max),
     minimum_ssim:([$rows[]|.quality[]|.luminance_ssim]|min),
     minimum_lf_ssim:([$rows[]|.quality[]|.low_frequency_luminance_ssim]|min)}}
' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"
jq -e '
  .validation.maximum_mae <= 0.0001 and
  .validation.minimum_ssim >= 0.999999 and
  .validation.minimum_lf_ssim >= 0.999999
' "$OUT_DIR/summary.json" >/dev/null
jq . "$OUT_DIR/summary.json"
printf 'Stitch-split experiment: %s\n' "$OUT_DIR"
