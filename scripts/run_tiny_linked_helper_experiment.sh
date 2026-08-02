#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/tiny-linked-helper/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-64}"
RUNS="${RUNS:-3}"
SHAPE_MODE="${SHAPE_MODE:-spheres}"
COUNTS=(16 32)
VARIANTS=(full-distance full-surface tiny-distance tiny-surface)

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
    --arg shape_mode "$SHAPE_MODE" \
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
    --arg output "Tiny_Link_${primitive_count}.png" '
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
           (if $shape_mode == "mixed" and ($index % 2) == 1
            then {op:"box",value:[0.36,0.32,0.4]}
            else {op:"sphere",radius:(0.37 + (($index * 7 % 11) / 100))}
            end)
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
        full-distance)
          backend_args=(--sdf-topology-specialization --no-sdf-generated-surface) ;;
        full-surface)
          backend_args=(--sdf-topology-specialization) ;;
        tiny-distance)
          backend_args=(--sdf-topology-specialization --no-sdf-generated-surface
                        --sdf-tiny-linked-helper) ;;
        tiny-surface)
          backend_args=(--sdf-topology-specialization --sdf-tiny-linked-helper) ;;
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
FPT_RUNTIME_SOURCE_CACHE_SALT="tiny-link-$RUN_STAMP" \
  "$BIN" render-batch "$OUT_DIR/jobs.json"

: > "$OUT_DIR/records.jsonl"
for primitive_count in "${COUNTS[@]}"; do
  output="Tiny_Link_${primitive_count}.png"
  for variant in "${VARIANTS[@]}"; do
    for ((run=0; run<=RUNS; ++run)); do
      metadata="$OUT_DIR/renders/$primitive_count/$variant/run-$run/$output.render.json"
      jq -c --arg variant "$variant" --argjson count "$primitive_count" \
        --argjson run "$run" '{primitive_count:$count,variant:$variant,run:$run,
          elapsed_ms:.elapsed_ms,build_ms:.sdf_runtime_library_build_ms,
          source_bytes:.sdf_runtime_source_bytes,
          compile_ms:.sdf_runtime_library_compile_ms,
          pipeline_link_ms:.sdf_runtime_pipeline_link_ms,
          library_mode:.sdf_library_mode,library_layout:.sdf_generated_library_layout}' \
        "$metadata" >> "$OUT_DIR/records.jsonl"
    done
  done
  "$BIN" compare \
    "$OUT_DIR/renders/$primitive_count/full-distance/run-1/$output" \
    "$OUT_DIR/renders/$primitive_count/tiny-distance/run-1/$output" \
    --report "$OUT_DIR/comparisons/$primitive_count-distance.json"
  "$BIN" compare \
    "$OUT_DIR/renders/$primitive_count/full-surface/run-1/$output" \
    "$OUT_DIR/renders/$primitive_count/tiny-surface/run-1/$output" \
    --report "$OUT_DIR/comparisons/$primitive_count-surface.json"
done

jq -s --argjson runs "$RUNS" '
  def median: sort | .[length/2|floor];
  group_by(.primitive_count) |
  map(. as $rows | {
    primitive_count:.[0].primitive_count,
    variants:(group_by(.variant)|map({key:.[0].variant,value:{
      cold_build_ms:([.[]|select(.run==0)|.build_ms][0]),
      source_bytes:([.[]|select(.run==0)|.source_bytes][0]),
      cold_compile_ms:([.[]|select(.run==0)|.compile_ms][0]),
      cold_pipeline_link_ms:([.[]|select(.run==0)|.pipeline_link_ms][0]),
      warm_build_ms:([.[]|select(.run>0)|.build_ms]|median),
      median_render_ms:([.[]|select(.run>0)|.elapsed_ms]|median)}})|from_entries)
  } | . + {
    distance_cold_reduction:(.variants["full-distance"].cold_build_ms /
                             .variants["tiny-distance"].cold_build_ms),
    surface_cold_reduction:(.variants["full-surface"].cold_build_ms /
                            .variants["tiny-surface"].cold_build_ms),
    distance_render_ratio:(.variants["tiny-distance"].median_render_ms /
                           .variants["full-distance"].median_render_ms),
    surface_render_ratio:(.variants["tiny-surface"].median_render_ms /
                          .variants["full-surface"].median_render_ms)
  }) as $rows |
  {settings:{width:'"$WIDTH"',height:'"$HEIGHT"',samples:'"$SAMPLES"',runs:$runs,
             shape_mode:"'"$SHAPE_MODE"'"},
   rows:$rows,
   acceptance:{cold_compile_reduction_at_least_2x:
      (all($rows[];.distance_cold_reduction>=2.0 and
                   .surface_cold_reduction>=2.0)),
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
printf 'Tiny linked-helper experiment: %s\n' "$OUT_DIR"
