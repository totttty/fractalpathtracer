#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/regional-flat-union/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-5}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-64}"
RESOLUTION="${RESOLUTION:-32}"
COUNTS=(8 16 24 32)
VARIANTS=(direct-bytecode direct-flat regional-bytecode regional-flat)

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
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
    --arg output "Regional_Flat_Union_${primitive_count}.png" '
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

COMMON_ARGS=(--width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
  --sdf-bounce-cap 1 --sdf-program-optimization basic)

render_variant() {
  local variant="$1"
  local fixture="$2"
  local output="$3"
  case "$variant" in
    direct-bytecode)
      "$BIN" render "$fixture" --out "$output" --renderer sdf \
        "${COMMON_ARGS[@]}"
      ;;
    direct-flat)
      "$BIN" render "$fixture" --out "$output" --renderer sdf \
        --sdf-flat-union "${COMMON_ARGS[@]}"
      ;;
    regional-bytecode)
      "$BIN" render "$fixture" --out "$output" --renderer regional \
        --regional-program-resolution "$RESOLUTION" --bound-grid-profile \
        "${COMMON_ARGS[@]}"
      ;;
    regional-flat)
      "$BIN" render "$fixture" --out "$output" --renderer regional \
        --regional-program-resolution "$RESOLUTION" --bound-grid-profile \
        --sdf-flat-union "${COMMON_ARGS[@]}"
      ;;
  esac
}

for ((run = 0; run < RUNS; run++)); do
  for variant in "${VARIANTS[@]}"; do mkdir -p "$OUT_DIR/$variant/run_$run"; done
  for primitive_count in "${COUNTS[@]}"; do
    fixture="$OUT_DIR/fixtures/union_${primitive_count}.json"
    if ((run % 2 == 0)); then
      order=(direct-bytecode direct-flat regional-bytecode regional-flat)
    else
      order=(regional-flat regional-bytecode direct-flat direct-bytecode)
    fi
    for variant in "${order[@]}"; do
      render_variant "$variant" "$fixture" "$OUT_DIR/$variant/run_$run"
    done
  done
done

: > "$OUT_DIR/records.jsonl"
for primitive_count in "${COUNTS[@]}"; do
  output="Regional_Flat_Union_${primitive_count}.png"
  for variant in direct-flat regional-bytecode regional-flat; do
    "$BIN" compare "$OUT_DIR/direct-bytecode/run_0/$output" \
      "$OUT_DIR/$variant/run_0/$output" \
      --report "$OUT_DIR/compare/union_${primitive_count}-${variant}.json"
  done
  jq -n --argjson primitive_count "$primitive_count" \
    --slurpfile db <(jq -s . "$OUT_DIR"/direct-bytecode/run_*/"$output.render.json") \
    --slurpfile df <(jq -s . "$OUT_DIR"/direct-flat/run_*/"$output.render.json") \
    --slurpfile rb <(jq -s . "$OUT_DIR"/regional-bytecode/run_*/"$output.render.json") \
    --slurpfile rf <(jq -s . "$OUT_DIR"/regional-flat/run_*/"$output.render.json") \
    --slurpfile qdf "$OUT_DIR/compare/union_${primitive_count}-direct-flat.json" \
    --slurpfile qrb "$OUT_DIR/compare/union_${primitive_count}-regional-bytecode.json" \
    --slurpfile qrf "$OUT_DIR/compare/union_${primitive_count}-regional-flat.json" '
      def median(values): values | sort | .[length / 2 | floor];
      {primitive_count:$primitive_count,
       direct_bytecode:{render_ms:median($db[0]|map(.elapsed_ms))},
       direct_flat:{render_ms:median($df[0]|map(.elapsed_ms)),quality:$qdf[0]},
       regional_bytecode:{render_ms:median($rb[0]|map(.elapsed_ms)),
                          build_ms:median($rb[0]|map(.regional_program_build_ms)),
                          memory_bytes:$rb[0][0].regional_program_memory_bytes,
                          profile:$rb[0][0].regional_program_profile,
                          quality:$qrb[0]},
       regional_flat:{render_ms:median($rf[0]|map(.elapsed_ms)),
                      build_ms:median($rf[0]|map(.regional_program_build_ms)),
                      memory_bytes:$rf[0][0].regional_program_memory_bytes,
                      profile:$rf[0][0].regional_program_profile,
                      quality:$qrf[0]}}
    ' >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson width "$WIDTH" --argjson height "$HEIGHT" \
  --argjson samples "$SAMPLES" --argjson runs "$RUNS" \
  --argjson resolution "$RESOLUTION" '
    map(. + {
      flat_direct_speedup:(.direct_bytecode.render_ms/.direct_flat.render_ms),
      regional_bytecode_speedup:
        (.direct_bytecode.render_ms/.regional_bytecode.render_ms),
      regional_flat_vs_direct_flat:
        (.direct_flat.render_ms/.regional_flat.render_ms),
      regional_flat_end_to_end_vs_direct_flat:
        (.direct_flat.render_ms/
          (.regional_flat.render_ms + .regional_flat.build_ms))
    }) as $rows |
    {settings:{width:$width,height:$height,samples:$samples,runs:$runs,
               regional_resolution:$resolution,
               geometry:"spatially distributed hard-union spheres"},
     validation:{maximum_mae:
       ([$rows[].direct_flat.quality.mean_absolute_error,
         $rows[].regional_bytecode.quality.mean_absolute_error,
         $rows[].regional_flat.quality.mean_absolute_error]|max),
       minimum_lf_ssim:
       ([$rows[].direct_flat.quality.low_frequency_luminance_ssim,
         $rows[].regional_bytecode.quality.low_frequency_luminance_ssim,
         $rows[].regional_flat.quality.low_frequency_luminance_ssim]|min),
       sampled_distance_failures:
       ([$rows[].regional_bytecode.profile.sampled_distance_failures,
         $rows[].regional_flat.profile.sampled_distance_failures]|add)},
     rows:$rows}' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

jq . "$OUT_DIR/summary.json"
printf 'Regional flat-union experiment: %s\n' "$OUT_DIR"
