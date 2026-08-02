#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/stitch-differential-fuzz/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
COUNTS=(4 8 16 32)
VARIANTS=(base expanded offset)

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
mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/cache" "$OUT_DIR/renders"
export FPT_STITCH_CACHE_DIR="$OUT_DIR/cache"

for primitive_count in "${COUNTS[@]}"; do
  jq --argjson primitive_count "$primitive_count" \
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
    --arg output "Stitch_Fuzz_${primitive_count}_base.png" '
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
           {op:"sphere",radius:(0.31 + (($index * 17 % 19) / 100))}
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
    > "$OUT_DIR/fixtures/${primitive_count}-base.json"
  jq --arg output "Stitch_Fuzz_${primitive_count}_expanded.png" '
    .output = $output |
    .sdf_program.operations |= map(
      if .op == "sphere" then .radius *= 1.137
      elif .op == "translate" then .value[2] -= 0.023
      else . end)
  ' "$OUT_DIR/fixtures/${primitive_count}-base.json" \
    > "$OUT_DIR/fixtures/${primitive_count}-expanded.json"
  jq --arg output "Stitch_Fuzz_${primitive_count}_offset.png" '
    .output = $output |
    .sdf_program.operations |= (to_entries | map(
      .value |
      if .op == "sphere" then .radius *= 0.917
      elif .op == "translate" then
        .value[0] += 0.011 | .value[1] -= 0.007
      else . end)
    )
  ' "$OUT_DIR/fixtures/${primitive_count}-base.json" \
    > "$OUT_DIR/fixtures/${primitive_count}-offset.json"
done

: > "$OUT_DIR/records.jsonl"
for primitive_count in "${COUNTS[@]}"; do
  for variant in "${VARIANTS[@]}"; do
    fixture="$OUT_DIR/fixtures/${primitive_count}-${variant}.json"
    destination="$OUT_DIR/renders/${primitive_count}-${variant}"
    mkdir -p "$destination"
    "$BIN" render "$fixture" --out "$destination" --renderer sdf \
      --sdf-program-optimization basic --sdf-function-stitching inline \
      --sdf-stitch-validation --sdf-bounce-cap 1 \
      --width 64 --height 36 --samples 1
    output="$(jq -r .output "$fixture")"
    jq --argjson primitive_count "$primitive_count" --arg variant "$variant" '
      {primitive_count:$primitive_count,variant:$variant,
       cache_status:.sdf_stitch_cache_status,
       cache_key:.sdf_stitch_cache_key,
       build_ms:.sdf_stitched_library_build_ms,
       validation:.sdf_stitch_validation_stats}
    ' "$destination/$output.render.json" >> "$OUT_DIR/records.jsonl"
  done
done

jq -s '
  {settings:{topologies:([.[].primitive_count]|unique),
    numeric_variants:([.[].variant]|unique),points_per_case:1048576,
    distance_tolerance:"2e-5 * (1 + abs(distance))",
    gradient_tolerance:1e-4},
   totals:{cases:length,sample_points:(map(.validation.sample_count)|add),
    distance_failures:(map(.validation.distance_failures)|add),
    gradient_failures:(map(.validation.gradient_failures)|add),
    maximum_distance_error:(map(.validation.max_distance_error)|max),
    maximum_gradient_error:(map(.validation.max_gradient_error)|max)},
   cache_invariants:{base_populations:
      ([.[]|select(.variant=="base" and .cache_status=="populated")]|length),
    numeric_cache_hits:
      ([.[]|select(.variant!="base" and .cache_status=="loaded")]|length),
    numeric_variants_share_topology_key:
      (group_by(.primitive_count)|all(
        ([.[].cache_key]|unique|length)==1))},
   cases:.}
  ' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

jq . "$OUT_DIR/summary.json"
printf 'Stitched differential fuzz report: %s\n' "$OUT_DIR"
