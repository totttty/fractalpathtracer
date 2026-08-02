#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/runtime-source-control/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-7}"
RESUME="${RESUME:-0}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-64}"
COUNTS=(8 16 24 32)
VARIANTS=(offline-bytecode runtime-bytecode runtime-topology)

command -v jq >/dev/null || { printf 'jq is required\n' >&2; exit 2; }
if [[ -e "$OUT_DIR" && "$RESUME" != "1" ]]; then
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

macos_version="$(sw_vers -productVersion)"
macos_build="$(sw_vers -buildVersion)"
xcode_version="$(xcodebuild -version | tr '\n' ' ')"
metal_compiler="$(xcrun metal -v 2>&1 | tr '\n' ' ')"
hardware_json="$(system_profiler SPHardwareDataType SPDisplaysDataType -json)"
jq -n \
  --arg timestamp "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --arg macos_version "$macos_version" \
  --arg macos_build "$macos_build" \
  --arg xcode_version "$xcode_version" \
  --arg metal_compiler "$metal_compiler" \
  --arg uname "$(uname -a)" \
  --argjson hardware "$hardware_json" \
  '{timestamp:$timestamp,macos:{version:$macos_version,build:$macos_build},
    xcode:$xcode_version,metal_compiler:$metal_compiler,uname:$uname,
    hardware:$hardware,compile_options:{metal_language:"2.4",math_mode:"fast",
    fp32_functions:"fast"}}' > "$OUT_DIR/environment.json"

for primitive_count in "${COUNTS[@]}"; do
  jq --argjson primitive_count "$primitive_count" \
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
    --arg output "Runtime_Control_Union_${primitive_count}.png" '
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
  --sdf-bounce-cap 1 --sdf-program-optimization basic --renderer sdf)

render_variant() {
  local variant="$1" fixture="$2" output="$3"
  case "$variant" in
    offline-bytecode)
      "$BIN" render "$fixture" --out "$output" "${COMMON_ARGS[@]}" ;;
    runtime-bytecode)
      "$BIN" render "$fixture" --out "$output" \
        --sdf-runtime-source-bytecode "${COMMON_ARGS[@]}" ;;
    runtime-topology)
      "$BIN" render "$fixture" --out "$output" \
        --sdf-topology-specialization "${COMMON_ARGS[@]}" ;;
  esac
}

for ((run = 0; run < RUNS; run++)); do
  case $((run % 6)) in
    0) order=(offline-bytecode runtime-bytecode runtime-topology) ;;
    1) order=(runtime-bytecode runtime-topology offline-bytecode) ;;
    2) order=(runtime-topology offline-bytecode runtime-bytecode) ;;
    3) order=(offline-bytecode runtime-topology runtime-bytecode) ;;
    4) order=(runtime-topology runtime-bytecode offline-bytecode) ;;
    *) order=(runtime-bytecode offline-bytecode runtime-topology) ;;
  esac
  for variant in "${VARIANTS[@]}"; do mkdir -p "$OUT_DIR/$variant/run_$run"; done
  for primitive_count in "${COUNTS[@]}"; do
    fixture="$OUT_DIR/fixtures/union_${primitive_count}.json"
    for variant in "${order[@]}"; do
      variant_output="$OUT_DIR/$variant/run_$run/Runtime_Control_Union_${primitive_count}.png"
      if [[ "$RESUME" == "1" && -e "$variant_output.render.json" ]]; then
        continue
      fi
      render_variant "$variant" "$fixture" "$OUT_DIR/$variant/run_$run"
    done
  done
done

: > "$OUT_DIR/records.jsonl"
for primitive_count in "${COUNTS[@]}"; do
  output="Runtime_Control_Union_${primitive_count}.png"
  for variant in runtime-bytecode runtime-topology; do
    "$BIN" compare "$OUT_DIR/offline-bytecode/run_0/$output" \
      "$OUT_DIR/$variant/run_0/$output" \
      --report "$OUT_DIR/compare/union_${primitive_count}-${variant}.json"
  done
  jq -n --argjson primitive_count "$primitive_count" \
    --slurpfile offline <(jq -s . "$OUT_DIR"/offline-bytecode/run_*/"$output.render.json") \
    --slurpfile bytecode <(jq -s . "$OUT_DIR"/runtime-bytecode/run_*/"$output.render.json") \
    --slurpfile topology <(jq -s . "$OUT_DIR"/runtime-topology/run_*/"$output.render.json") \
    --slurpfile qb "$OUT_DIR/compare/union_${primitive_count}-runtime-bytecode.json" \
    --slurpfile qt "$OUT_DIR/compare/union_${primitive_count}-runtime-topology.json" '
      def median(values): values | sort | .[length / 2 | floor];
      def stats(records):
        (records|map(.elapsed_ms)) as $values |
        (median($values)) as $median |
        {times_ms:$values,median_ms:$median,min_ms:($values|min),
         max_ms:($values|max),mad_ms:median($values|map((.-$median)|abs))};
      def builds(records):
        (records|map(.sdf_runtime_library_build_ms)) as $values |
        {times_ms:$values,first_observed_ms:$values[0],
         subsequent_median_ms:median($values[1:])};
      {primitive_count:$primitive_count,
       geometry_instruction_count:$offline[0][0].sdf_program_instruction_count,
       offline_bytecode:(stats($offline[0])),
       runtime_bytecode:((stats($bytecode[0])) +
         {build:builds($bytecode[0]),quality:$qb[0]}),
       runtime_topology:((stats($topology[0])) +
         {build:builds($topology[0]),quality:$qt[0]})}
    ' >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson width "$WIDTH" --argjson height "$HEIGHT" \
  --argjson samples "$SAMPLES" --argjson runs "$RUNS" \
  --slurpfile environment "$OUT_DIR/environment.json" '
    def break_even_work($build; $baseline; $specialized; $sample_pixels):
      if $baseline > $specialized then
        ($build / ($baseline - $specialized) * $sample_pixels)
      else null end;
    ($width*$height*$samples) as $sample_pixels |
    map(. + {
      runtime_bytecode_vs_offline:
        (.offline_bytecode.median_ms/.runtime_bytecode.median_ms),
      topology_vs_offline:
        (.offline_bytecode.median_ms/.runtime_topology.median_ms),
      topology_vs_runtime_bytecode:
        (.runtime_bytecode.median_ms/.runtime_topology.median_ms),
      first_observed_break_even_sample_pixels_vs_runtime_bytecode:
        break_even_work(.runtime_topology.build.first_observed_ms;
          .runtime_bytecode.median_ms;.runtime_topology.median_ms;$sample_pixels)
    }) as $rows |
    {settings:{width:$width,height:$height,samples:$samples,runs:$runs,
      sample_pixels_per_invocation:$sample_pixels,
      workload:"spatially distributed hard-union spheres",
      variants:["offline-metallib-bytecode","runtime-source-bytecode",
        "runtime-source-topology"]},environment:$environment[0],
     validation:{maximum_mae:
       ([$rows[].runtime_bytecode.quality.mean_absolute_error,
         $rows[].runtime_topology.quality.mean_absolute_error]|max),
       minimum_lf_ssim:
       ([$rows[].runtime_bytecode.quality.low_frequency_luminance_ssim,
         $rows[].runtime_topology.quality.low_frequency_luminance_ssim]|min)},
     rows:$rows}' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

jq . "$OUT_DIR/summary.json"
printf 'Runtime-source control experiment: %s\n' "$OUT_DIR"
