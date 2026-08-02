#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/topology-specialization/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-5}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-64}"
RESOLUTION="${RESOLUTION:-32}"
ANALYZE_ONLY="${ANALYZE_ONLY:-0}"
COUNTS=(8 16 24 32)
VARIANTS=(geometry-bytecode topology-msl direct-flat regional-flat)

command -v jq >/dev/null || { printf 'jq is required\n' >&2; exit 2; }
if [[ -e "$OUT_DIR" && "$ANALYZE_ONLY" != "1" ]]; then
  printf 'refusing to overwrite existing report directory: %s\n' "$OUT_DIR" >&2
  exit 2
fi
cd "$ROOT_DIR"
if [[ "$ANALYZE_ONLY" != "1" && "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo test
  cargo build --release
fi
if [[ "$ANALYZE_ONLY" != "1" ]]; then
mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/compare"
for variant in "${VARIANTS[@]}"; do mkdir -p "$OUT_DIR/$variant"; done

for primitive_count in "${COUNTS[@]}"; do
  jq --argjson primitive_count "$primitive_count" \
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
    --arg output "Topology_Union_${primitive_count}.png" '
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

COMMON_ARGS=(--width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
  --sdf-bounce-cap 1 --sdf-program-optimization basic)

render_variant() {
  local variant="$1" fixture="$2" output="$3"
  case "$variant" in
    geometry-bytecode)
      "$BIN" render "$fixture" --out "$output" --renderer sdf \
        "${COMMON_ARGS[@]}" ;;
    topology-msl)
      "$BIN" render "$fixture" --out "$output" --renderer sdf \
        --sdf-topology-specialization "${COMMON_ARGS[@]}" ;;
    direct-flat)
      "$BIN" render "$fixture" --out "$output" --renderer sdf \
        --sdf-flat-union "${COMMON_ARGS[@]}" ;;
    regional-flat)
      "$BIN" render "$fixture" --out "$output" --renderer regional \
        --regional-program-resolution "$RESOLUTION" --sdf-flat-union \
        "${COMMON_ARGS[@]}" ;;
  esac
}

for ((run = 0; run < RUNS; run++)); do
  for variant in "${VARIANTS[@]}"; do mkdir -p "$OUT_DIR/$variant/run_$run"; done
  for primitive_count in "${COUNTS[@]}"; do
    fixture="$OUT_DIR/fixtures/union_${primitive_count}.json"
    if ((run % 2 == 0)); then
      order=(geometry-bytecode topology-msl direct-flat regional-flat)
    else
      order=(regional-flat direct-flat topology-msl geometry-bytecode)
    fi
    for variant in "${order[@]}"; do
      render_variant "$variant" "$fixture" "$OUT_DIR/$variant/run_$run"
    done
  done
done
fi

: > "$OUT_DIR/records.jsonl"
for primitive_count in "${COUNTS[@]}"; do
  output="Topology_Union_${primitive_count}.png"
  for variant in topology-msl direct-flat regional-flat; do
    "$BIN" compare "$OUT_DIR/geometry-bytecode/run_0/$output" \
      "$OUT_DIR/$variant/run_0/$output" \
      --report "$OUT_DIR/compare/union_${primitive_count}-${variant}.json"
  done
  jq -n --argjson primitive_count "$primitive_count" \
    --slurpfile gb <(jq -s . "$OUT_DIR"/geometry-bytecode/run_*/"$output.render.json") \
    --slurpfile tm <(jq -s . "$OUT_DIR"/topology-msl/run_*/"$output.render.json") \
    --slurpfile df <(jq -s . "$OUT_DIR"/direct-flat/run_*/"$output.render.json") \
    --slurpfile rf <(jq -s . "$OUT_DIR"/regional-flat/run_*/"$output.render.json") \
    --slurpfile qtm "$OUT_DIR/compare/union_${primitive_count}-topology-msl.json" \
    --slurpfile qdf "$OUT_DIR/compare/union_${primitive_count}-direct-flat.json" \
    --slurpfile qrf "$OUT_DIR/compare/union_${primitive_count}-regional-flat.json" '
      def median(values): values | sort | .[length / 2 | floor];
      {primitive_count:$primitive_count,
       geometry_instruction_count:$gb[0][0].sdf_program_instruction_count,
       geometry_bytecode:{render_ms:median($gb[0]|map(.elapsed_ms))},
       topology_msl:{render_ms:median($tm[0]|map(.elapsed_ms)),
                     build_ms:median($tm[0]|map(.sdf_topology_build_ms)),
                     cold_build_ms:$tm[0][0].sdf_topology_build_ms,
                     warm_build_ms:median($tm[0][1:]|map(.sdf_topology_build_ms)),
                     quality:$qtm[0]},
       direct_flat:{render_ms:median($df[0]|map(.elapsed_ms)),quality:$qdf[0]},
       regional_flat:{render_ms:median($rf[0]|map(.elapsed_ms)),
                      build_ms:median($rf[0]|map(.regional_program_build_ms)),
                      quality:$qrf[0]}}
    ' >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson width "$WIDTH" --argjson height "$HEIGHT" \
  --argjson samples "$SAMPLES" --argjson runs "$RUNS" \
  --argjson resolution "$RESOLUTION" '
    def break_even($build; $baseline; $specialized):
      if $baseline > $specialized then
        ($build / ($baseline - $specialized))
      else null end;
    map(. + {
      topology_render_speedup:
        (.geometry_bytecode.render_ms/.topology_msl.render_ms),
      topology_vs_direct_flat:
        (.direct_flat.render_ms/.topology_msl.render_ms),
      topology_vs_regional_flat:
        (.regional_flat.render_ms/.topology_msl.render_ms),
      break_even_frames_vs_geometry_bytecode:
        break_even(.topology_msl.build_ms;.geometry_bytecode.render_ms;
                   .topology_msl.render_ms),
      cold_break_even_frames_vs_geometry_bytecode:
        break_even(.topology_msl.cold_build_ms;.geometry_bytecode.render_ms;
                   .topology_msl.render_ms),
      break_even_frames_vs_direct_flat:
        break_even(.topology_msl.build_ms;.direct_flat.render_ms;
                   .topology_msl.render_ms),
      cold_break_even_frames_vs_direct_flat:
        break_even(.topology_msl.cold_build_ms;.direct_flat.render_ms;
                   .topology_msl.render_ms),
      break_even_frames_vs_regional_flat:
        break_even(.topology_msl.build_ms;
                   (.regional_flat.render_ms+.regional_flat.build_ms);
                   .topology_msl.render_ms),
      cold_break_even_frames_vs_regional_flat:
        break_even(.topology_msl.cold_build_ms;
                   (.regional_flat.render_ms+.regional_flat.build_ms);
                   .topology_msl.render_ms)
    }) as $rows |
    {settings:{width:$width,height:$height,samples:$samples,runs:$runs,
               regional_resolution:$resolution,
               workload:"spatially distributed hard-union spheres"},
     validation:{maximum_mae:
       ([$rows[].topology_msl.quality.mean_absolute_error,
         $rows[].direct_flat.quality.mean_absolute_error,
         $rows[].regional_flat.quality.mean_absolute_error]|max),
       minimum_lf_ssim:
       ([$rows[].topology_msl.quality.low_frequency_luminance_ssim,
         $rows[].direct_flat.quality.low_frequency_luminance_ssim,
         $rows[].regional_flat.quality.low_frequency_luminance_ssim]|min)},
     rows:$rows}' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

jq . "$OUT_DIR/summary.json"
printf 'Topology-specialization experiment: %s\n' "$OUT_DIR"
