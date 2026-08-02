#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/geometry-split/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-5}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-64}"
RESOLUTION="${RESOLUTION:-32}"
INCLUDE_ORBIT="${INCLUDE_ORBIT:-1}"
COUNTS=(8 12 16 20)
VARIANTS=(direct-full direct-split regional-full regional-split)

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
mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/compare"
for variant in "${VARIANTS[@]}"; do mkdir -p "$OUT_DIR/$variant"; done

for primitive_count in "${COUNTS[@]}"; do
  jq --argjson primitive_count "$primitive_count" \
    --argjson include_orbit "$INCLUDE_ORBIT" \
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
    --arg output "Geometry_Split_${primitive_count}.png" '
      def center($index):
        [((($index % 4) - 1.5) * 1.25),
         ((((($index / 4) | floor) % 4) - 1.5) * 1.05),
         (((($index / 16) | floor) - 0.5) * 1.25)];
      (reduce range(0; $primitive_count) as $index
        ({previous:[0,0,0],operations:[]};
         (center($index)) as $current |
         .operations += ([
           {op:"translate",
            value:[($current[0] - .previous[0]),
                   ($current[1] - .previous[1]),
                   ($current[2] - .previous[2])]},
           {op:"sphere",radius:0.46,orbit_weight:0.04}
         ] + (if $include_orbit != 0 then
           [{op:"orbit_add",value:[0.73,0.41,0.89],orbit_weight:0.025}]
         else [] end)) |
         .previous = $current)) as $program |
      .preset = $preset |
      .output = $output |
      .camera.position = [0.0,0.15,-7.2] |
      .camera.focus_distance = 7.2 |
      .sdf_program.operations = $program.operations |
      .sdf_program.material.mode = "gradient" |
      .sdf_program.gradient = [
        {position:0,color:[0.04,0.18,0.72]},
        {position:0.5,color:[0.10,0.82,0.66]},
        {position:1,color:[0.94,0.24,0.08]}
      ]
    ' "$ROOT_DIR/scenes/benchmarks/Exact_Sphere.json" \
    > "$OUT_DIR/fixtures/orbit_${primitive_count}.json"
done

COMMON_ARGS=(--width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
  --sdf-bounce-cap 1 --sdf-program-optimization basic)

render_variant() {
  local variant="$1" fixture="$2" output="$3"
  case "$variant" in
    direct-full)
      "$BIN" render "$fixture" --out "$output" --renderer sdf \
        --no-sdf-geometry-split "${COMMON_ARGS[@]}" ;;
    direct-split)
      "$BIN" render "$fixture" --out "$output" --renderer sdf \
        --sdf-geometry-split "${COMMON_ARGS[@]}" ;;
    regional-full)
      "$BIN" render "$fixture" --out "$output" --renderer regional \
        --regional-program-resolution "$RESOLUTION" --bound-grid-profile \
        --no-sdf-geometry-split "${COMMON_ARGS[@]}" ;;
    regional-split)
      "$BIN" render "$fixture" --out "$output" --renderer regional \
        --regional-program-resolution "$RESOLUTION" --bound-grid-profile \
        --sdf-geometry-split "${COMMON_ARGS[@]}" ;;
  esac
}

for ((run = 0; run < RUNS; run++)); do
  for variant in "${VARIANTS[@]}"; do mkdir -p "$OUT_DIR/$variant/run_$run"; done
  for primitive_count in "${COUNTS[@]}"; do
    fixture="$OUT_DIR/fixtures/orbit_${primitive_count}.json"
    if ((run % 2 == 0)); then
      order=(direct-full direct-split regional-full regional-split)
    else
      order=(regional-split regional-full direct-split direct-full)
    fi
    for variant in "${order[@]}"; do
      render_variant "$variant" "$fixture" "$OUT_DIR/$variant/run_$run"
    done
  done
done

: > "$OUT_DIR/records.jsonl"
for primitive_count in "${COUNTS[@]}"; do
  output="Geometry_Split_${primitive_count}.png"
  for variant in direct-split regional-full regional-split; do
    "$BIN" compare "$OUT_DIR/direct-full/run_0/$output" \
      "$OUT_DIR/$variant/run_0/$output" \
      --report "$OUT_DIR/compare/orbit_${primitive_count}-${variant}.json"
  done
  jq -n --argjson primitive_count "$primitive_count" \
    --slurpfile df <(jq -s . "$OUT_DIR"/direct-full/run_*/"$output.render.json") \
    --slurpfile ds <(jq -s . "$OUT_DIR"/direct-split/run_*/"$output.render.json") \
    --slurpfile rf <(jq -s . "$OUT_DIR"/regional-full/run_*/"$output.render.json") \
    --slurpfile rs <(jq -s . "$OUT_DIR"/regional-split/run_*/"$output.render.json") \
    --slurpfile qds "$OUT_DIR/compare/orbit_${primitive_count}-direct-split.json" \
    --slurpfile qrf "$OUT_DIR/compare/orbit_${primitive_count}-regional-full.json" \
    --slurpfile qrs "$OUT_DIR/compare/orbit_${primitive_count}-regional-split.json" '
      def median(values): values | sort | .[length / 2 | floor];
      {primitive_count:$primitive_count,
       source_instruction_count:$df[0][0].sdf_program_source_instruction_count,
       geometry_instruction_count:$ds[0][0].sdf_program_instruction_count,
       shading_instruction_count:$ds[0][0].sdf_shading_program_instruction_count,
       direct_full:{render_ms:median($df[0]|map(.elapsed_ms))},
       direct_split:{render_ms:median($ds[0]|map(.elapsed_ms)),quality:$qds[0]},
       regional_full:{render_ms:median($rf[0]|map(.elapsed_ms)),
                      build_ms:median($rf[0]|map(.regional_program_build_ms)),
                      quality:$qrf[0]},
       regional_split:{render_ms:median($rs[0]|map(.elapsed_ms)),
                       build_ms:median($rs[0]|map(.regional_program_build_ms)),
                       quality:$qrs[0]}}
    ' >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson width "$WIDTH" --argjson height "$HEIGHT" \
  --argjson samples "$SAMPLES" --argjson runs "$RUNS" \
  --argjson resolution "$RESOLUTION" --argjson include_orbit "$INCLUDE_ORBIT" '
    map(. + {
      direct_speedup:(.direct_full.render_ms/.direct_split.render_ms),
      regional_speedup:(.regional_full.render_ms/.regional_split.render_ms),
      regional_split_vs_direct_split:
        (.direct_split.render_ms/.regional_split.render_ms),
      regional_split_end_to_end_vs_direct_split:
        (.direct_split.render_ms/
         (.regional_split.render_ms + .regional_split.build_ms))
    }) as $rows |
    {settings:{width:$width,height:$height,samples:$samples,runs:$runs,
               regional_resolution:$resolution,
               workload:(if $include_orbit != 0 then
                 "hard-union spheres with interleaved orbit-only ops"
               else "hard-union spheres without removable geometry ops" end)},
     validation:{maximum_mae:
       ([$rows[].direct_split.quality.mean_absolute_error,
         $rows[].regional_full.quality.mean_absolute_error,
         $rows[].regional_split.quality.mean_absolute_error]|max),
       minimum_lf_ssim:
       ([$rows[].direct_split.quality.low_frequency_luminance_ssim,
         $rows[].regional_full.quality.low_frequency_luminance_ssim,
         $rows[].regional_split.quality.low_frequency_luminance_ssim]|min)},
     rows:$rows}' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

jq . "$OUT_DIR/summary.json"
printf 'Geometry-split experiment: %s\n' "$OUT_DIR"
