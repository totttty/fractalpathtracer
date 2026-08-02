#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/canonical-ir/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RESOLUTIONS=(1920x1080 3840x2160)
BOUNCES=(1 8)
MATERIALS=(opaque translucent refractive)

command -v jq >/dev/null || { printf 'jq is required\n' >&2; exit 2; }
if [[ -e "$OUT_DIR" ]]; then
  printf 'refusing to overwrite existing report directory: %s\n' "$OUT_DIR" >&2
  exit 2
fi
cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then cargo build --release; fi
mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/renders" "$OUT_DIR/comparisons"

jq --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" '
  def center($i):
    [((($i % 4) - 1.5) * 1.15),
     ((((($i / 4) | floor) % 3) - 1.0) * 1.05),
     (((($i / 12) | floor) - 0.5) * 1.15)];
  def primitive($i):
    if (($i % 2) == 1) then {op:"box",value:[0.34,0.29,0.32]}
    else {op:"sphere",radius:(0.36 + (($i * 7 % 9) / 100))} end;
  (reduce range(0;10) as $i
    ({previous:[0,0,0],operations:[]};
     center($i) as $current |
     (($i % 5) * 0.11 - 0.22) as $angle |
     (0.82 + (($i % 4) * 0.11)) as $scale |
     .operations += [
       {op:"translate",value:[($current[0]-.previous[0]),
                              ($current[1]-.previous[1]),
                              ($current[2]-.previous[2])]},
       {op:"rotate_z",angle:$angle},{op:"scale",value:$scale},primitive($i),
       {op:"scale",value:(1.0/$scale)},{op:"rotate_z",angle:(-$angle)}] |
     .previous = $current)) as $program |
  .preset = $preset |
  .camera.position = [0.0,0.1,-7.0] |
  .camera.focus_distance = 7.0 |
  .sdf_program.operations = $program.operations
' scenes/benchmarks/Exact_Sphere.json > "$OUT_DIR/fixtures/base.json"

for material in "${MATERIALS[@]}"; do
  jq --arg material "$material" '
    .output = ("Canonical_" + $material + ".png") |
    .sdf_program.material.mode = "solid" |
    .sdf_program.material.color = [0.24,0.72,0.92] |
    .sdf_program.material.roughness =
      (if $material == "opaque" then 0.42
       elif $material == "translucent" then 0.22 else 0.05 end) |
    .sdf_program.material.specular =
      (if $material == "opaque" then 0.3 else 0.9 end) |
    .sdf_program.material.translucency =
      (if $material == "opaque" then 0.0
       elif $material == "translucent" then 0.45 else 1.0 end) |
    .sdf_program.material.ior = 1.45
  ' "$OUT_DIR/fixtures/base.json" > "$OUT_DIR/fixtures/$material.json"
done

: > "$OUT_DIR/jobs.jsonl"
for resolution in "${RESOLUTIONS[@]}"; do
  width="${resolution%x*}"; height="${resolution#*x}"
  for bounces in "${BOUNCES[@]}"; do
    for material in "${MATERIALS[@]}"; do
      for evaluator in bytecode canonical; do
        run_dir="$OUT_DIR/renders/$resolution/bounce-$bounces/$material/$evaluator"
        mkdir -p "$run_dir"
        extra=()
        if [[ "$evaluator" == bytecode ]]; then extra=(--no-sdf-canonical-ir); fi
        jq -nc --args '$ARGS.positional' -- \
          "$OUT_DIR/fixtures/$material.json" --out "$run_dir" --renderer sdf \
          --width "$width" --height "$height" --samples 1 \
          --sdf-bounce-cap "$bounces" --sdf-profile "${extra[@]}" \
          >> "$OUT_DIR/jobs.jsonl"
      done
    done
  done
done
jq -s . "$OUT_DIR/jobs.jsonl" > "$OUT_DIR/jobs.json"
"$BIN" render-batch "$OUT_DIR/jobs.json"

: > "$OUT_DIR/records.jsonl"
for resolution in "${RESOLUTIONS[@]}"; do
  for bounces in "${BOUNCES[@]}"; do
    for material in "${MATERIALS[@]}"; do
      output="Canonical_${material}.png"
      for evaluator in bytecode canonical; do
        metadata="$OUT_DIR/renders/$resolution/bounce-$bounces/$material/$evaluator/$output.render.json"
        jq -c --arg resolution "$resolution" --argjson bounces "$bounces" \
          --arg material "$material" --arg evaluator "$evaluator" '
          {resolution:$resolution,bounces:$bounces,material:$material,
           evaluator:$evaluator,elapsed_ms:.elapsed_ms,
           direct_evaluator:.sdf_direct_evaluator,
           canonical_source_primitives:.sdf_canonical_source_primitive_count,
           canonical_primitives:.sdf_canonical_primitive_count,
           profile:.sdf_profile}
        ' "$metadata" >> "$OUT_DIR/records.jsonl"
      done
      if [[ "$resolution" == 1920x1080 && "$bounces" == 8 ]]; then
        "$BIN" compare \
          "$OUT_DIR/renders/$resolution/bounce-$bounces/$material/bytecode/$output" \
          "$OUT_DIR/renders/$resolution/bounce-$bounces/$material/canonical/$output" \
          --report "$OUT_DIR/comparisons/$material.json"
      fi
    done
  done
done

jq -s '
  group_by([.resolution,.bounces,.material]) |
  map({resolution:.[0].resolution,bounces:.[0].bounces,material:.[0].material,
       bytecode_ms:(map(select(.evaluator=="bytecode"))[0].elapsed_ms),
       canonical_ms:(map(select(.evaluator=="canonical"))[0].elapsed_ms)}) |
  map(. + {canonical_speedup:(.bytecode_ms/.canonical_ms)}) as $rows |
  {rows:$rows,aggregates:{conditions:($rows|length),
    wins:($rows|map(select(.canonical_speedup>1.0))|length),
    mean_speedup:($rows|map(.canonical_speedup)|add/length),
    minimum_speedup:($rows|map(.canonical_speedup)|min),
    maximum_speedup:($rows|map(.canonical_speedup)|max)}}
' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"
jq -s '{maximum_mae:(map(.mean_absolute_error)|max),
        minimum_ssim:(map(.luminance_ssim)|min),
        minimum_lf_ssim:(map(.low_frequency_luminance_ssim)|min)}' \
  "$OUT_DIR"/comparisons/*.json > "$OUT_DIR/quality-summary.json"
jq '.aggregates' "$OUT_DIR/summary.json"
printf 'Canonical-IR experiment: %s\n' "$OUT_DIR"
