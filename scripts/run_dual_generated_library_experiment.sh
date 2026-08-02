#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/dual-generated-library/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-64}"
RUNS="${RUNS:-3}"
COUNTS=(16 32)
VARIANTS=(single-distance single-surface dual-distance dual-surface)

command -v jq >/dev/null || { printf 'jq is required\n' >&2; exit 2; }
if [[ -e "$OUT_DIR" ]]; then
  printf 'refusing to overwrite existing report directory: %s\n' "$OUT_DIR" >&2
  exit 2
fi
cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo build --release
fi
mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/renders" "$OUT_DIR/comparisons"

for primitive_count in "${COUNTS[@]}"; do
  jq --argjson primitive_count "$primitive_count" \
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
    --arg output "Dual_Library_${primitive_count}.png" '
      def center($index):
        [((($index % 4) - 1.5) * 1.25),
         ((((($index / 4) | floor) % 4) - 1.5) * 1.05),
         (((($index / 16) | floor) - 0.5) * 1.25)];
      (reduce range(0; $primitive_count) as $index
        ({previous:[0,0,0],operations:[]};
         (center($index)) as $current |
         .operations += [
           {op:"translate",value:[($current[0]-.previous[0]),
                                   ($current[1]-.previous[1]),
                                   ($current[2]-.previous[2])]},
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
    > "$OUT_DIR/fixtures/union-$primitive_count.json"
done

: > "$OUT_DIR/jobs.jsonl"
for primitive_count in "${COUNTS[@]}"; do
  fixture="$OUT_DIR/fixtures/union-$primitive_count.json"
  for ((run=0; run<=RUNS; ++run)); do
    for ((slot=0; slot<${#VARIANTS[@]}; ++slot)); do
      variant="${VARIANTS[$(((slot + run) % ${#VARIANTS[@]}))]}"
      case "$variant" in
        single-distance)
          backend_args=(--sdf-topology-specialization --no-sdf-generated-surface) ;;
        single-surface)
          backend_args=(--sdf-topology-specialization) ;;
        dual-distance)
          backend_args=(--sdf-topology-specialization --no-sdf-generated-surface
                        --sdf-dual-generated-library) ;;
        dual-surface)
          backend_args=(--sdf-topology-specialization --sdf-dual-generated-library) ;;
      esac
      run_dir="$OUT_DIR/renders/$primitive_count/$variant/run-$run"
      mkdir -p "$run_dir"
      jq -nc --args '$ARGS.positional' -- \
        "$fixture" --out "$run_dir" --renderer sdf \
        --width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES" \
        --sdf-bounce-cap 1 --sdf-program-optimization basic \
        "${backend_args[@]}" >> "$OUT_DIR/jobs.jsonl"
    done
  done
done
jq -s . "$OUT_DIR/jobs.jsonl" > "$OUT_DIR/jobs.json"
"$BIN" render-batch "$OUT_DIR/jobs.json"

: > "$OUT_DIR/records.jsonl"
for primitive_count in "${COUNTS[@]}"; do
  output="Dual_Library_${primitive_count}.png"
  for variant in "${VARIANTS[@]}"; do
    for ((run=0; run<=RUNS; ++run)); do
      metadata="$OUT_DIR/renders/$primitive_count/$variant/run-$run/$output.render.json"
      jq -c --arg variant "$variant" --argjson count "$primitive_count" \
        --argjson run "$run" '{primitive_count:$count,variant:$variant,run:$run,
          elapsed_ms:.elapsed_ms,build_ms:.sdf_runtime_library_build_ms}' \
        "$metadata" >> "$OUT_DIR/records.jsonl"
    done
  done
  "$BIN" compare \
    "$OUT_DIR/renders/$primitive_count/single-distance/run-1/$output" \
    "$OUT_DIR/renders/$primitive_count/dual-distance/run-1/$output" \
    --report "$OUT_DIR/comparisons/$primitive_count-distance.json"
  "$BIN" compare \
    "$OUT_DIR/renders/$primitive_count/single-surface/run-1/$output" \
    "$OUT_DIR/renders/$primitive_count/dual-surface/run-1/$output" \
    --report "$OUT_DIR/comparisons/$primitive_count-surface.json"
done

jq -s --argjson runs "$RUNS" '
  def median: sort | .[length/2|floor];
  group_by(.primitive_count) |
  map(. as $rows | {
    primitive_count:.[0].primitive_count,
    variants:(group_by(.variant)|map({key:.[0].variant,value:{
      cold_build_ms:([.[]|select(.run==0)|.build_ms][0]),
      warm_build_ms:([.[]|select(.run>0)|.build_ms]|median),
      median_render_ms:([.[]|select(.run>0)|.elapsed_ms]|median)}})|from_entries)
  } | . + {
    cold_compile_reduction:(
      (.variants["single-distance"].cold_build_ms +
       .variants["single-surface"].cold_build_ms) /
      (.variants["dual-distance"].cold_build_ms +
       .variants["dual-surface"].cold_build_ms)),
    distance_render_ratio:(.variants["dual-distance"].median_render_ms /
                           .variants["single-distance"].median_render_ms),
    surface_render_ratio:(.variants["dual-surface"].median_render_ms /
                          .variants["single-surface"].median_render_ms)
  }) as $rows |
  {settings:{runs:$runs},rows:$rows,
   acceptance:{cold_compile_reduction_at_least_2x:
      (all($rows[];.cold_compile_reduction>=2.0)),
    render_within_five_percent:
      (all($rows[];.distance_render_ratio<=1.05 and
                   .surface_render_ratio<=1.05))}}
' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"
jq -s '{maximum_mae:(map(.mean_absolute_error)|max),
        maximum_absolute_error:(map(.max_absolute_error)|max),
        minimum_ssim:(map(.luminance_ssim)|min)}' \
  "$OUT_DIR"/comparisons/*.json > "$OUT_DIR/quality-summary.json"
jq . "$OUT_DIR/summary.json"
jq . "$OUT_DIR/quality-summary.json"
printf 'Dual generated-library experiment: %s\n' "$OUT_DIR"
