#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/compact-canonical-codegen/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RESOLUTIONS_TEXT="${RESOLUTIONS:-1920x1080 3840x2160}"
BOUNCES_TEXT="${BOUNCES:-1 8}"
SAMPLES="${SAMPLES:-1}"
RUNS="${RUNS:-1}"
TOPOLOGIES_TEXT="${TOPOLOGY_CLASSES:-union mixed affine}"
MATERIALS_TEXT="${MATERIAL_CLASSES:-opaque translucent refractive}"
read -r -a RESOLUTIONS <<< "$RESOLUTIONS_TEXT"
read -r -a BOUNCES <<< "$BOUNCES_TEXT"
read -r -a TOPOLOGIES <<< "$TOPOLOGIES_TEXT"
read -r -a MATERIALS <<< "$MATERIALS_TEXT"
CANDIDATES=(direct program-distance program-surface wide-distance wide-surface compact-distance compact-surface)

command -v jq >/dev/null || { printf 'jq is required\n' >&2; exit 2; }
if [[ -e "$OUT_DIR" ]]; then
  printf 'refusing to overwrite existing report directory: %s\n' "$OUT_DIR" >&2
  exit 2
fi
cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then cargo build --release; fi
mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/renders" "$OUT_DIR/comparisons"

for topology in "${TOPOLOGIES[@]}"; do
  jq --arg topology "$topology" \
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" '
    def center($i):
      [((($i % 4) - 1.5) * 1.15),
       ((((($i / 4) | floor) % 3) - 1.0) * 1.05),
       (((($i / 12) | floor) - 0.5) * 1.15)];
    def primitive($i; $mixed):
      if $mixed and (($i % 2) == 1)
      then {op:"box",value:[0.34,0.29,0.32]}
      else {op:"sphere",radius:(0.36 + (($i * 7 % 9) / 100))} end;
    def ordinary($count; $mixed):
      reduce range(0;$count) as $i
        ({previous:[0,0,0],operations:[]};
         center($i) as $current |
         .operations += [
           {op:"translate",value:[($current[0]-.previous[0]),
                                  ($current[1]-.previous[1]),
                                  ($current[2]-.previous[2])]},
           primitive($i;$mixed)] | .previous = $current) | .operations;
    def affine($count):
      reduce range(0;$count) as $i
        ({previous:[0,0,0],operations:[]};
         center($i) as $current |
         (($i % 5) * 0.11 - 0.22) as $angle |
         (0.82 + (($i % 4) * 0.11)) as $scale |
         .operations += [
           {op:"translate",value:[($current[0]-.previous[0]),
                                  ($current[1]-.previous[1]),
                                  ($current[2]-.previous[2])]},
           {op:"rotate_z",angle:$angle},{op:"scale",value:$scale},
           primitive($i;true),{op:"scale",value:(1.0/$scale)},
           {op:"rotate_z",angle:(-$angle)}] | .previous = $current) | .operations;
    .preset = $preset |
    .camera.position = [0.0,0.1,-7.0] |
    .camera.focus_distance = 7.0 |
    .sdf_program.operations =
      (if $topology == "union" then ordinary(32;false)
       elif $topology == "mixed" then ordinary(24;true)
       else affine(10) end)
  ' scenes/benchmarks/Exact_Sphere.json > "$OUT_DIR/fixtures/$topology-base.json"
  for material in "${MATERIALS[@]}"; do
    jq --arg topology "$topology" --arg material "$material" '
      .output = ("Compact_" + $topology + "_" + $material + ".png") |
      .sdf_program.material.mode = "solid" |
      .sdf_program.material.color =
        (if $material == "opaque" then [0.16,0.66,0.92]
         elif $material == "translucent" then [0.28,0.82,0.62]
         else [0.72,0.88,0.98] end) |
      .sdf_program.material.roughness =
        (if $material == "opaque" then 0.42
         elif $material == "translucent" then 0.22 else 0.05 end) |
      .sdf_program.material.specular =
        (if $material == "opaque" then 0.3 else 0.9 end) |
      .sdf_program.material.translucency =
        (if $material == "opaque" then 0.0
         elif $material == "translucent" then 0.45 else 1.0 end) |
      .sdf_program.material.ior = 1.45
    ' "$OUT_DIR/fixtures/$topology-base.json" \
      > "$OUT_DIR/fixtures/$topology-$material.json"
  done
done

: > "$OUT_DIR/jobs.jsonl"
for resolution in "${RESOLUTIONS[@]}"; do
  width="${resolution%x*}"; height="${resolution#*x}"
  for bounces in "${BOUNCES[@]}"; do
    for topology in "${TOPOLOGIES[@]}"; do
      for material in "${MATERIALS[@]}"; do
        for candidate in "${CANDIDATES[@]}"; do
          case "$candidate" in
            direct) backend_args=() ;;
            program-distance) backend_args=(--sdf-topology-specialization --no-sdf-generated-surface) ;;
            program-surface) backend_args=(--sdf-topology-specialization) ;;
            wide-distance) backend_args=(--sdf-canonical-topology-specialization --no-sdf-generated-surface) ;;
            wide-surface) backend_args=(--sdf-canonical-topology-specialization) ;;
            compact-distance) backend_args=(--sdf-compact-canonical-topology-specialization --no-sdf-generated-surface) ;;
            compact-surface) backend_args=(--sdf-compact-canonical-topology-specialization) ;;
          esac
          for ((run=1; run<=RUNS; ++run)); do
            run_dir="$OUT_DIR/renders/$resolution/bounce-$bounces/$topology-$material/$candidate/run-$run"
            mkdir -p "$run_dir"
            jq -nc --args '$ARGS.positional' -- \
              "$OUT_DIR/fixtures/$topology-$material.json" --out "$run_dir" \
              --renderer sdf --width "$width" --height "$height" \
              --samples "$SAMPLES" --sdf-bounce-cap "$bounces" --sdf-profile \
              "${backend_args[@]}" >> "$OUT_DIR/jobs.jsonl"
          done
        done
      done
    done
  done
done
jq -s . "$OUT_DIR/jobs.jsonl" > "$OUT_DIR/jobs.json"
"$BIN" render-batch "$OUT_DIR/jobs.json"

: > "$OUT_DIR/records.jsonl"
for resolution in "${RESOLUTIONS[@]}"; do
  for bounces in "${BOUNCES[@]}"; do
    for topology in "${TOPOLOGIES[@]}"; do
      for material in "${MATERIALS[@]}"; do
        output="Compact_${topology}_${material}.png"
        for candidate in "${CANDIDATES[@]}"; do
          for ((run=1; run<=RUNS; ++run)); do
            metadata="$OUT_DIR/renders/$resolution/bounce-$bounces/$topology-$material/$candidate/run-$run/$output.render.json"
            jq -c --arg resolution "$resolution" --argjson bounces "$bounces" \
              --arg topology "$topology" --arg material "$material" \
              --arg candidate "$candidate" --argjson run "$run" '
              {resolution:$resolution,bounces:$bounces,topology:$topology,
               material:$material,candidate:$candidate,run:$run,
               elapsed_ms:.elapsed_ms,
               build_ms:(.sdf_runtime_library_build_ms // 0),
               evaluator:.sdf_direct_evaluator,
               source_instructions:.sdf_program_source_instruction_count,
               optimized_instructions:.sdf_program_instruction_count,
               canonical_source_primitives:.sdf_canonical_source_primitive_count,
               canonical_primitives:.sdf_canonical_primitive_count,
               resources:.sdf_pipeline_resource_stats,profile:.sdf_profile}
            ' "$metadata" >> "$OUT_DIR/records.jsonl"
          done
        done
        if [[ "$resolution" == "1920x1080" && "$bounces" == "8" ]]; then
          direct_image="$OUT_DIR/renders/$resolution/bounce-$bounces/$topology-$material/direct/run-1/$output"
          for candidate in "${CANDIDATES[@]:1}"; do
            candidate_image="$OUT_DIR/renders/$resolution/bounce-$bounces/$topology-$material/$candidate/run-1/$output"
            "$BIN" compare "$direct_image" "$candidate_image" \
              --report "$OUT_DIR/comparisons/$topology-$material-$candidate.json"
          done
        fi
      done
    done
  done
done

jq -s '
  def median: sort | .[length/2|floor];
  group_by([.resolution,.bounces,.topology,.material,.candidate]) |
  map(.[0] + {elapsed_ms:(map(.elapsed_ms)|median),
              build_ms:(map(.build_ms)|median)}) |
  group_by([.resolution,.bounces,.topology,.material]) |
  map((map({key:.candidate,value:.})|from_entries) as $c |
      ($c.direct.elapsed_ms) as $direct |
      {resolution:$c.direct.resolution,bounces:$c.direct.bounces,
       topology:$c.direct.topology,material:$c.direct.material,
       candidates:$c,
       speedup:{
         program_distance:($direct/$c["program-distance"].elapsed_ms),
         program_surface:($direct/$c["program-surface"].elapsed_ms),
         wide_distance:($direct/$c["wide-distance"].elapsed_ms),
         wide_surface:($direct/$c["wide-surface"].elapsed_ms),
         compact_distance:($direct/$c["compact-distance"].elapsed_ms),
         compact_surface:($direct/$c["compact-surface"].elapsed_ms)},
       winner:($c|to_entries|min_by(.value.elapsed_ms)|.key)}) as $rows |
  {settings:{samples:$ARGS.named.samples,runs:$ARGS.named.runs},rows:$rows,
   aggregates:{conditions:($rows|length),
     winner_counts:($rows|group_by(.winner)|map({key:.[0].winner,value:length})|from_entries),
     mean_speedup:{
       program_distance:($rows|map(.speedup.program_distance)|add/length),
       program_surface:($rows|map(.speedup.program_surface)|add/length),
       wide_distance:($rows|map(.speedup.wide_distance)|add/length),
       wide_surface:($rows|map(.speedup.wide_surface)|add/length),
       compact_distance:($rows|map(.speedup.compact_distance)|add/length),
       compact_surface:($rows|map(.speedup.compact_surface)|add/length)},
     compact_surface_wins:($rows|map(select(.winner=="compact-surface"))|length),
     compact_distance_wins:($rows|map(select(.winner=="compact-distance"))|length)}}
' --argjson samples "$SAMPLES" --argjson runs "$RUNS" \
  "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

if compgen -G "$OUT_DIR/comparisons/*.json" >/dev/null; then
  jq -s '{comparisons:length,
          maximum_mae:(map(.mean_absolute_error)|max),
          maximum_absolute_error:(map(.max_absolute_error)|max),
          minimum_ssim:(map(.luminance_ssim)|min),
          minimum_lf_ssim:(map(.low_frequency_luminance_ssim)|min)}' \
    "$OUT_DIR"/comparisons/*.json > "$OUT_DIR/quality-summary.json"
fi
jq '{settings,aggregates}' "$OUT_DIR/summary.json"
printf 'Compact canonical codegen experiment: %s\n' "$OUT_DIR"
