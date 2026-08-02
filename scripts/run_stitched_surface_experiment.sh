#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/stitched-surface/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-7}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-64}"
COUNTS=(8 16 24 32)
VARIANTS=(bytecode stitched-distance stitched-surface)

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
mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/cache" "$OUT_DIR/compare"
for variant in "${VARIANTS[@]}"; do mkdir -p "$OUT_DIR/$variant"; done
export FPT_STITCH_CACHE_DIR="$OUT_DIR/cache"

for primitive_count in "${COUNTS[@]}"; do
  jq --argjson primitive_count "$primitive_count" \
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
    --arg output "Stitched_Surface_Union_${primitive_count}.png" '
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
           {op:"sphere",radius:0.46}
         ] |
         .previous = $current)) as $program |
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

COMMON_ARGS=(--renderer sdf --sdf-program-optimization basic
  --sdf-bounce-cap 1 --width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES")
render_variant() {
  local variant="$1" fixture="$2" destination="$3"
  case "$variant" in
    bytecode)
      "$BIN" render "$fixture" --out "$destination" "${COMMON_ARGS[@]}" ;;
    stitched-distance)
      "$BIN" render "$fixture" --out "$destination" \
        --sdf-function-stitching inline --no-sdf-stitched-surface \
        "${COMMON_ARGS[@]}" ;;
    stitched-surface)
      "$BIN" render "$fixture" --out "$destination" \
        --sdf-function-stitching inline "${COMMON_ARGS[@]}" ;;
  esac
}

for ((run = 0; run < RUNS; run++)); do
  case $((run % 3)) in
    0) order=(bytecode stitched-distance stitched-surface) ;;
    1) order=(stitched-distance stitched-surface bytecode) ;;
    *) order=(stitched-surface bytecode stitched-distance) ;;
  esac
  for variant in "${VARIANTS[@]}"; do mkdir -p "$OUT_DIR/$variant/run_$run"; done
  for primitive_count in "${COUNTS[@]}"; do
    fixture="$OUT_DIR/fixtures/union_${primitive_count}.json"
    for variant in "${order[@]}"; do
      render_variant "$variant" "$fixture" "$OUT_DIR/$variant/run_$run"
    done
  done
done

: > "$OUT_DIR/records.jsonl"
for primitive_count in "${COUNTS[@]}"; do
  output="Stitched_Surface_Union_${primitive_count}.png"
  for variant in stitched-distance stitched-surface; do
    "$BIN" compare "$OUT_DIR/bytecode/run_0/$output" \
      "$OUT_DIR/$variant/run_0/$output" \
      --report "$OUT_DIR/compare/union_${primitive_count}-${variant}.json"
  done
  jq -n --argjson primitive_count "$primitive_count" \
    --slurpfile bytecode <(jq -s . "$OUT_DIR"/bytecode/run_*/"$output.render.json") \
    --slurpfile distance <(jq -s . "$OUT_DIR"/stitched-distance/run_*/"$output.render.json") \
    --slurpfile surface <(jq -s . "$OUT_DIR"/stitched-surface/run_*/"$output.render.json") \
    --slurpfile qd "$OUT_DIR/compare/union_${primitive_count}-stitched-distance.json" \
    --slurpfile qs "$OUT_DIR/compare/union_${primitive_count}-stitched-surface.json" '
      def median(values): values | sort | .[length / 2 | floor];
      def stats(records):
        (records|map(.elapsed_ms)) as $values |
        (median($values)) as $median |
        {times_ms:$values,median_ms:$median,min_ms:($values|min),
         max_ms:($values|max),mad_ms:median($values|map((.-$median)|abs))};
      {primitive_count:$primitive_count,
       instruction_count:$bytecode[0][0].sdf_program_instruction_count,
       bytecode:stats($bytecode[0]),
       stitched_distance:(stats($distance[0]) +
         {quality:$qd[0],build_ms:($distance[0]|map(.sdf_stitched_library_build_ms))}),
       stitched_surface:(stats($surface[0]) +
         {quality:$qs[0],build_ms:($surface[0]|map(.sdf_stitched_library_build_ms))})}
    ' >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson width "$WIDTH" --argjson height "$HEIGHT" \
  --argjson samples "$SAMPLES" --argjson runs "$RUNS" '
    map(. + {
      distance_speedup:(.bytecode.median_ms/.stitched_distance.median_ms),
      surface_speedup:(.bytecode.median_ms/.stitched_surface.median_ms),
      surface_vs_distance:(.stitched_distance.median_ms/.stitched_surface.median_ms)
    }) as $rows |
    {settings:{width:$width,height:$height,samples:$samples,runs:$runs},
     validation:{maximum_mae:
       ([$rows[].stitched_distance.quality.mean_absolute_error,
         $rows[].stitched_surface.quality.mean_absolute_error]|max),
       minimum_lf_ssim:
       ([$rows[].stitched_distance.quality.low_frequency_luminance_ssim,
         $rows[].stitched_surface.quality.low_frequency_luminance_ssim]|min)},
     rows:$rows}
  ' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

jq . "$OUT_DIR/summary.json"
printf 'Stitched surface experiment: %s\n' "$OUT_DIR"
