#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/generated-selector-matrix/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
SAMPLES="${SAMPLES:-1}"
RUNS="${RUNS:-3}"
RESOLUTIONS_TEXT="${RESOLUTIONS:-1920x1080 3840x2160}"
BOUNCES_TEXT="${BOUNCES:-1 2 4 8}"
read -r -a RESOLUTIONS <<< "$RESOLUTIONS_TEXT"
read -r -a BOUNCES <<< "$BOUNCES_TEXT"
TOPOLOGIES=(union mixed affine)
MATERIALS=(opaque translucent refractive)
CANDIDATES=(direct generated-distance generated-surface auto)

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
: > "$OUT_DIR/records.jsonl"

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
      else {op:"sphere",radius:(0.36 + (($i * 7 % 9) / 100))}
      end;
    def ordinary($count; $mixed):
      reduce range(0;$count) as $i
        ({previous:[0,0,0],operations:[]};
         center($i) as $current |
         .operations += [
           {op:"translate",value:[($current[0]-.previous[0]),
                                  ($current[1]-.previous[1]),
                                  ($current[2]-.previous[2])]},
           primitive($i;$mixed)] |
         .previous = $current) | .operations;
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
           {op:"rotate_z",angle:$angle},
           {op:"scale",value:$scale},
           primitive($i;true),
           {op:"scale",value:(1.0/$scale)},
           {op:"rotate_z",angle:(-$angle)}] |
         .previous = $current) | .operations;
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
      .output = ("Matrix_" + $topology + "_" + $material + ".png") |
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
      .sdf_program.material.ior =
        (if $material == "refractive" then 1.45 else 1.5 end)
    ' "$OUT_DIR/fixtures/$topology-base.json" \
      > "$OUT_DIR/fixtures/$topology-$material.json"
  done
done

: > "$OUT_DIR/jobs.jsonl"
for resolution in "${RESOLUTIONS[@]}"; do
  width="${resolution%x*}"
  height="${resolution#*x}"
  for bounces in "${BOUNCES[@]}"; do
    for topology in "${TOPOLOGIES[@]}"; do
      for material in "${MATERIALS[@]}"; do
        fixture="$OUT_DIR/fixtures/$topology-$material.json"
        # Run zero is an untimed warmup. Measured rounds rotate all four
        # backends so no candidate owns a persistent thermal/cache position.
        for ((run=0; run<=RUNS; ++run)); do
          for ((slot=0; slot<${#CANDIDATES[@]}; ++slot)); do
            candidate="${CANDIDATES[$(((slot + run) % ${#CANDIDATES[@]}))]}"
            case "$candidate" in
              direct) backend_args=() ;;
              generated-distance)
                backend_args=(--sdf-topology-specialization --no-sdf-generated-surface) ;;
              generated-surface)
                backend_args=(--sdf-topology-specialization) ;;
              auto)
                backend_args=(--sdf-backend probe) ;;
            esac
            run_dir="$OUT_DIR/renders/$resolution/bounce-$bounces/$topology-$material/$candidate/run-$run"
            mkdir -p "$run_dir"
            jq -nc --args '$ARGS.positional' -- \
              "$fixture" --out "$run_dir" --renderer sdf \
              --width "$width" --height "$height" --samples "$SAMPLES" \
              --sdf-bounce-cap "$bounces" --sdf-program-optimization basic \
              --sdf-profile "${backend_args[@]}" >> "$OUT_DIR/jobs.jsonl"
          done
        done
      done
    done
  done
done
jq -s . "$OUT_DIR/jobs.jsonl" > "$OUT_DIR/jobs.json"
"$BIN" render-batch "$OUT_DIR/jobs.json"

for resolution in "${RESOLUTIONS[@]}"; do
  for bounces in "${BOUNCES[@]}"; do
    for topology in "${TOPOLOGIES[@]}"; do
      for material in "${MATERIALS[@]}"; do
        output="Matrix_${topology}_${material}.png"
        for candidate in "${CANDIDATES[@]}"; do
          for ((run=1; run<=RUNS; ++run)); do
            run_dir="$OUT_DIR/renders/$resolution/bounce-$bounces/$topology-$material/$candidate/run-$run"
            jq -c --arg resolution "$resolution" \
              --arg topology "$topology" --arg material "$material" \
              --arg candidate "$candidate" --argjson bounces "$bounces" \
              --argjson run "$run" '
              {resolution:$resolution,bounces:$bounces,topology:$topology,
               material:$material,candidate:$candidate,run:$run,
               elapsed_ms:.elapsed_ms,
               build_ms:.sdf_runtime_library_build_ms,
               evaluator:.sdf_direct_evaluator,
               source_instructions:.sdf_program_source_instruction_count,
               optimized_instructions:.sdf_program_instruction_count,
               canonical_source_primitives:.sdf_canonical_source_primitive_count,
               canonical_primitives:.sdf_canonical_primitive_count,
               selector:(.sdf_backend_selection // null),
               profile:.sdf_profile}
            ' "$run_dir/$output.render.json" >> "$OUT_DIR/records.jsonl"
          done
        done
        if [[ "$resolution" == "1920x1080" && "$bounces" == "4" ]]; then
          direct_image="$OUT_DIR/renders/$resolution/bounce-$bounces/$topology-$material/direct/run-1/$output"
          for candidate in generated-distance generated-surface auto; do
            candidate_image="$OUT_DIR/renders/$resolution/bounce-$bounces/$topology-$material/$candidate/run-1/$output"
            "$BIN" compare "$direct_image" "$candidate_image" \
              --report "$OUT_DIR/comparisons/$topology-$material-$candidate.json"
          done
        fi
      done
    done
  done
done

jq -s --argjson samples "$SAMPLES" --argjson runs "$RUNS" '
  def median: sort | .[length/2|floor];
  group_by([.resolution,.bounces,.topology,.material]) |
  map({resolution:.[0].resolution,bounces:.[0].bounces,
       topology:.[0].topology,material:.[0].material,
       candidates:(group_by(.candidate) | map({
         key:.[0].candidate,
         value:{median_ms:(map(.elapsed_ms)|median),
                median_build_ms:(map(.build_ms // 0)|median),
                evaluator:.[0].evaluator,
                source_instructions:.[0].source_instructions,
                optimized_instructions:.[0].optimized_instructions,
                canonical_source_primitives:.[0].canonical_source_primitives,
                canonical_primitives:.[0].canonical_primitives,
                selector:.[0].selector,
                profile:.[0].profile}}) | from_entries)}) |
  map(. as $row |
      ($row.candidates.direct.median_ms) as $direct |
      ($row.candidates["generated-distance"].median_ms) as $distance |
      ($row.candidates["generated-surface"].median_ms) as $surface |
      ($row.candidates.auto.median_ms) as $auto |
      ([{name:"direct",ms:$direct},
         {name:"generated-distance",ms:$distance},
         {name:"generated-surface",ms:$surface}] | sort_by(.ms) | .[0]) as $best |
      ($row.candidates.auto.selector.selected) as $selected |
      ($row.candidates[$selected].median_ms) as $selected_ms |
      . + {speedup:{
             generated_distance:($direct/$distance),
             generated_surface:($direct/$surface)},
           oracle_fastest:$best.name,
           selector_selected:$selected,
           selector_source:$row.candidates.auto.selector.decision_source,
           selector_render_median_ms:$auto,
           selector_regret_ms:($selected_ms-$best.ms),
           selector_regret_ratio:($selected_ms/$best.ms),
           selector_matches_fastest:
             ($selected == $best.name)}) as $rows |
  {settings:{samples:$samples,runs:$runs},rows:$rows,
   aggregates:{
     conditions:($rows|length),
     selected_counts:($rows|group_by(.selector_selected)|map({key:(.[0].selector_selected // "unknown"),value:length})|from_entries),
     fastest_counts:($rows|group_by(.oracle_fastest)|map({key:(.[0].oracle_fastest // "unknown"),value:length})|from_entries),
     exact_choice_matches:($rows|map(select(.selector_matches_fastest))|length),
     within_five_percent:($rows|map(select(.selector_regret_ratio <= 1.05))|length),
     mean_regret_ratio:($rows|map(.selector_regret_ratio)|add/length),
     maximum_regret_ratio:($rows|map(.selector_regret_ratio)|max),
     mean_generated_distance_speedup:($rows|map(.speedup.generated_distance)|add/length),
     mean_generated_surface_speedup:($rows|map(.speedup.generated_surface)|add/length),
     primary_step_share:($rows|map(
       .candidates.direct.profile as $p |
       ($p.primary_steps / ([$p.primary_steps,$p.secondary_steps,$p.shadow_steps]|add)))|add/length)}}
' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

if compgen -G "$OUT_DIR/comparisons/*.json" >/dev/null; then
  jq -s '{comparisons:length,
          maximum_mae:(map(.mean_absolute_error)|max),
          maximum_absolute_error:(map(.max_absolute_error)|max),
          minimum_ssim:(map(.luminance_ssim)|min),
          minimum_lf_ssim:(map(.low_frequency_luminance_ssim)|min)}' \
    "$OUT_DIR"/comparisons/*.json > "$OUT_DIR/quality-summary.json"
fi
jq '{settings,aggregates}' "$OUT_DIR/summary.json"
printf 'Generated-selector workload matrix: %s\n' "$OUT_DIR"
