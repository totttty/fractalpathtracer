#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/shared-transform-dag/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RESOLUTIONS_TEXT="${RESOLUTIONS:-1920x1080 3840x2160}"
BOUNCES_TEXT="${BOUNCES:-1 8}"
REUSE_TEXT="${TRANSFORM_REUSE:-1 2 4 8}"
MATERIALS_TEXT="${MATERIAL_CLASSES:-opaque translucent refractive}"
SAMPLES="${SAMPLES:-1}"
RUNS="${RUNS:-3}"
LEAVES="${LEAVES:-32}"
read -r -a RESOLUTIONS <<< "$RESOLUTIONS_TEXT"
read -r -a BOUNCES <<< "$BOUNCES_TEXT"
read -r -a REUSE_FACTORS <<< "$REUSE_TEXT"
read -r -a MATERIALS <<< "$MATERIALS_TEXT"
CANDIDATES=(direct program-distance program-surface compact-hybrid shared-dag)

command -v jq >/dev/null || { printf 'jq is required\n' >&2; exit 2; }
if [[ -e "$OUT_DIR" ]]; then
  printf 'refusing to overwrite existing report directory: %s\n' "$OUT_DIR" >&2
  exit 2
fi
cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then cargo build --release; fi
mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/renders" "$OUT_DIR/comparisons"

for reuse in "${REUSE_FACTORS[@]}"; do
  if (( LEAVES % reuse != 0 )); then
    printf 'LEAVES (%s) must be divisible by reuse factor (%s)\n' "$LEAVES" "$reuse" >&2
    exit 2
  fi
  groups=$((LEAVES / reuse))
  jq --argjson leaves "$LEAVES" --argjson reuse "$reuse" --argjson groups "$groups" \
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" '
    def columns: (($groups | sqrt | ceil) | floor);
    def rows: (($groups / columns | ceil) | floor);
    def center($g):
      [((($g % columns) - ((columns - 1) / 2)) * 0.82),
       ((((($g / columns) | floor) - ((rows - 1) / 2))) * 0.82),
       (((($g * 7) % 3) - 1) * 0.18)];
    def leaf($i):
      if ($i % 2) == 0
      then {op:"sphere",radius:(0.28 + (($i % 7) * 0.018))}
      else {op:"box",value:[(0.24 + (($i % 5) * 0.017)),
                             (0.27 + (($i % 3) * 0.021)),
                             (0.22 + (($i % 4) * 0.019))]} end;
    def grouped_program:
      reduce range(0;$groups) as $g
        ({previous:[0,0,0],operations:[]};
         center($g) as $current |
         .operations += [
           {op:"translate",value:[($current[0]-.previous[0]),
                                  ($current[1]-.previous[1]),
                                  ($current[2]-.previous[2])]}
         ] + [range(0;$reuse) as $local | leaf(($g * $reuse) + $local)] |
         .previous = $current) | .operations;
    .preset = $preset |
    .camera.position = [0.0,0.0,-7.2] |
    .camera.focus_distance = 7.2 |
    .camera.fov = 47 |
    .voxel.bounds_min = [-3.25,-3.25,-3.25] |
    .voxel.bounds_max = [3.25,3.25,3.25] |
    .sdf_program.operations = grouped_program
  ' scenes/benchmarks/Exact_Sphere.json > "$OUT_DIR/fixtures/reuse-$reuse-base.json"
  for material in "${MATERIALS[@]}"; do
    jq --argjson reuse "$reuse" --arg material "$material" '
      .output = ("Shared_Transform_" + ($reuse|tostring) + "x_" + $material + ".png") |
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
    ' "$OUT_DIR/fixtures/reuse-$reuse-base.json" > "$OUT_DIR/fixtures/reuse-$reuse-$material.json"
  done
done

: > "$OUT_DIR/jobs.jsonl"
for resolution in "${RESOLUTIONS[@]}"; do
  width="${resolution%x*}"; height="${resolution#*x}"
  for bounces in "${BOUNCES[@]}"; do
    for reuse in "${REUSE_FACTORS[@]}"; do
      for material in "${MATERIALS[@]}"; do
        for candidate in "${CANDIDATES[@]}"; do
          case "$candidate" in
            direct) backend_args=() ;;
            program-distance) backend_args=(--sdf-topology-specialization --no-sdf-generated-surface) ;;
            program-surface) backend_args=(--sdf-topology-specialization) ;;
            compact-hybrid) backend_args=(--sdf-compact-canonical-topology-specialization --no-sdf-generated-surface) ;;
            shared-dag) backend_args=(--sdf-shared-transform-topology-specialization) ;;
          esac
          for ((run=1; run<=RUNS; ++run)); do
            run_dir="$OUT_DIR/renders/$resolution/bounce-$bounces/reuse-$reuse-$material/$candidate/run-$run"
            mkdir -p "$run_dir"
            jq -nc --args '$ARGS.positional' -- \
              "$OUT_DIR/fixtures/reuse-$reuse-$material.json" --out "$run_dir" \
              --renderer sdf --width "$width" --height "$height" --samples "$SAMPLES" \
              --sdf-bounce-cap "$bounces" --sdf-profile "${backend_args[@]}" >> "$OUT_DIR/jobs.jsonl"
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
    for reuse in "${REUSE_FACTORS[@]}"; do
      for material in "${MATERIALS[@]}"; do
        output="Shared_Transform_${reuse}x_${material}.png"
        for candidate in "${CANDIDATES[@]}"; do
          for ((run=1; run<=RUNS; ++run)); do
            metadata="$OUT_DIR/renders/$resolution/bounce-$bounces/reuse-$reuse-$material/$candidate/run-$run/$output.render.json"
            jq -c --arg resolution "$resolution" --argjson bounces "$bounces" \
              --argjson requested_reuse "$reuse" --arg material "$material" \
              --arg candidate "$candidate" --argjson run "$run" '
              {resolution:$resolution,bounces:$bounces,requested_reuse:$requested_reuse,
               material:$material,candidate:$candidate,run:$run,elapsed_ms:.elapsed_ms,
               build_ms:(.sdf_runtime_library_build_ms // 0),evaluator:.sdf_direct_evaluator,
               source_instructions:.sdf_program_source_instruction_count,
               optimized_instructions:.sdf_program_instruction_count,
               canonical_primitives:.sdf_canonical_primitive_count,
               transform_count:.sdf_canonical_transform_count,
               measured_reuse:.sdf_canonical_transform_reuse,
               resources:.sdf_pipeline_resource_stats,profile:.sdf_profile}
            ' "$metadata" >> "$OUT_DIR/records.jsonl"
          done
        done
        if [[ "$resolution" == "1920x1080" && "$bounces" == "8" ]]; then
          direct_image="$OUT_DIR/renders/$resolution/bounce-$bounces/reuse-$reuse-$material/direct/run-1/$output"
          for candidate in "${CANDIDATES[@]:1}"; do
            candidate_image="$OUT_DIR/renders/$resolution/bounce-$bounces/reuse-$reuse-$material/$candidate/run-1/$output"
            "$BIN" compare "$direct_image" "$candidate_image" \
              --report "$OUT_DIR/comparisons/reuse-$reuse-$material-$candidate.json"
          done
        fi
      done
    done
  done
done

jq -s '
  def median: sort | .[length/2|floor];
  group_by([.resolution,.bounces,.requested_reuse,.material,.candidate]) |
  map(.[0] + {elapsed_ms:(map(.elapsed_ms)|median),build_ms:(map(.build_ms)|median)}) |
  group_by([.resolution,.bounces,.requested_reuse,.material]) |
  map((map({key:.candidate,value:.})|from_entries) as $c |
      ($c.direct.elapsed_ms) as $direct |
      {resolution:$c.direct.resolution,bounces:$c.direct.bounces,
       requested_reuse:$c.direct.requested_reuse,material:$c.direct.material,
       transform_count:$c.direct.transform_count,measured_reuse:$c.direct.measured_reuse,
       candidates:$c,
       speedup:{program_distance:($direct/$c["program-distance"].elapsed_ms),
                program_surface:($direct/$c["program-surface"].elapsed_ms),
                compact_hybrid:($direct/$c["compact-hybrid"].elapsed_ms),
                shared_dag:($direct/$c["shared-dag"].elapsed_ms)},
       winner:($c|to_entries|min_by(.value.elapsed_ms)|.key)}) as $rows |
  {settings:{samples:$ARGS.named.samples,runs:$ARGS.named.runs,leaves:$ARGS.named.leaves},
   rows:$rows,
   aggregates:{conditions:($rows|length),
     winner_counts:($rows|group_by(.winner)|map({key:.[0].winner,value:length})|from_entries),
     mean_speedup:{program_distance:($rows|map(.speedup.program_distance)|add/length),
                   program_surface:($rows|map(.speedup.program_surface)|add/length),
                   compact_hybrid:($rows|map(.speedup.compact_hybrid)|add/length),
                   shared_dag:($rows|map(.speedup.shared_dag)|add/length)},
     by_reuse:($rows|group_by(.requested_reuse)|map({key:(.[0].requested_reuse|tostring),
       value:{conditions:length,shared_dag_mean:(map(.speedup.shared_dag)|add/length),
              program_distance_mean:(map(.speedup.program_distance)|add/length),
              shared_dag_wins:(map(select(.winner=="shared-dag"))|length)}})|from_entries)}}
' --argjson samples "$SAMPLES" --argjson runs "$RUNS" --argjson leaves "$LEAVES" \
  "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

if compgen -G "$OUT_DIR/comparisons/*.json" >/dev/null; then
  jq -s '{comparisons:length,maximum_mae:(map(.mean_absolute_error)|max),
          maximum_absolute_error:(map(.max_absolute_error)|max),
          minimum_ssim:(map(.luminance_ssim)|min),
          minimum_lf_ssim:(map(.low_frequency_luminance_ssim)|min)}' \
    "$OUT_DIR"/comparisons/*.json > "$OUT_DIR/quality-summary.json"
fi
jq '{settings,aggregates}' "$OUT_DIR/summary.json"
printf 'Shared-transform DAG experiment: %s\n' "$OUT_DIR"
