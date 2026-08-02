#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/stitch-vocabulary/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
SCENES=(Exact_Sphere Exact_Box Exact_Plane Exact_CSG Exact_Transforms Exact_Tie)

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
mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/cache" "$OUT_DIR/renders" "$OUT_DIR/comparisons"
export FPT_STITCH_CACHE_DIR="$OUT_DIR/cache"

: > "$OUT_DIR/records.jsonl"
for scene in "${SCENES[@]}"; do
  source_fixture="$ROOT_DIR/scenes/benchmarks/$scene.json"
  base_fixture="$OUT_DIR/fixtures/$scene-base.json"
  variant_fixture="$OUT_DIR/fixtures/$scene-variant.json"
  jq --arg output "$scene-base.png" \
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
    '.output = $output | .preset = $preset' \
    "$source_fixture" > "$base_fixture"
  jq --arg output "$scene-variant.png" '
    .output = $output |
    .sdf_program.operations |= map(
      if .op == "translate" then .value |= map(. + 0.011)
      elif .op == "scale" then .factor *= 0.973
      elif (.op | startswith("rotate_")) then .angle += 0.017
      elif .op == "repeat" then .value |= map(. * 1.013)
      elif .op == "sphere" then .radius *= 1.031
      elif .op == "box" then .value |= map(. * 1.021)
      elif .op == "plane" then .data[3] += 0.013
      else . end)
  ' "$base_fixture" > "$variant_fixture"

  for variant in base variant; do
    fixture="$OUT_DIR/fixtures/$scene-$variant.json"
    bytecode_dir="$OUT_DIR/renders/$scene-$variant-bytecode"
    stitched_dir="$OUT_DIR/renders/$scene-$variant-stitched"
    mkdir -p "$bytecode_dir" "$stitched_dir"
    "$BIN" render "$fixture" --out "$bytecode_dir" --renderer sdf \
      --sdf-program-optimization basic --sdf-bounce-cap 1 \
      --width 64 --height 36 --samples 1
    "$BIN" render "$fixture" --out "$stitched_dir" --renderer sdf \
      --sdf-program-optimization basic --sdf-function-stitching inline \
      --sdf-stitch-validation --sdf-bounce-cap 1 \
      --width 64 --height 36 --samples 1
    output="$(jq -r .output "$fixture")"
    comparison="$OUT_DIR/comparisons/$scene-$variant.json"
    "$BIN" compare "$bytecode_dir/$output" "$stitched_dir/$output" \
      --report "$comparison"
    jq --arg scene "$scene" --arg variant "$variant" \
      --slurpfile comparison "$comparison" '
      {scene:$scene,variant:$variant,
       cache_status:.sdf_stitch_cache_status,
       cache_key:.sdf_stitch_cache_key,
       build_ms:.sdf_stitched_library_build_ms,
       render_ms:.elapsed_ms,
       validation:.sdf_stitch_validation_stats,
       comparison:$comparison[0]}
    ' "$stitched_dir/$output.render.json" >> "$OUT_DIR/records.jsonl"
  done
done

jq -s '
  {settings:{scenes:([.[].scene]|unique),variants:([.[].variant]|unique),
     points_per_case:1048576,
     operations:["abs","translate","scale","rotate_x","rotate_y","rotate_z",
       "repeat","sort_desc","sphere","box","plane","union","intersection",
       "subtract","strict_tie"]},
   totals:{cases:length,
     sample_points:(map(.validation.sample_count)|add),
     distance_failures:(map(.validation.distance_failures)|add),
     gradient_failures:(map(.validation.gradient_failures)|add),
     maximum_distance_error:(map(.validation.max_distance_error)|max),
     maximum_gradient_error:(map(.validation.max_gradient_error)|max),
     maximum_image_mae:(map(.comparison.mean_absolute_error)|max),
     minimum_image_ssim:(map(.comparison.luminance_ssim)|min)},
   cache_invariants:{
     cold_populations:([.[]|select(.variant=="base" and .cache_status=="populated")]|length),
     numeric_cache_hits:([.[]|select(.variant=="variant" and .cache_status=="loaded")]|length),
     numeric_variants_share_topology_key:
       (group_by(.scene)|all(([.[].cache_key]|unique|length)==1))},
   cases:.}
' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

jq -e '
  .totals.distance_failures == 0 and
  .totals.gradient_failures == 0 and
  .totals.maximum_image_mae == 0 and
  .cache_invariants.cold_populations == 6 and
  .cache_invariants.numeric_cache_hits == 6 and
  .cache_invariants.numeric_variants_share_topology_key
' "$OUT_DIR/summary.json" >/dev/null

jq . "$OUT_DIR/summary.json"
printf 'Stitch vocabulary report: %s\n' "$OUT_DIR"
