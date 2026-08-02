#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/regional-program-hardening/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-5}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-64}"
BOUNCE_CAP="${BOUNCE_CAP:-1}"
FLAT_UNION="${FLAT_UNION:-0}"
GEOMETRY_SPLIT="${GEOMETRY_SPLIT:-1}"
SCENES=(mixed_transforms orbit_material tie_boundaries repeat_seams)

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
mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/sdf" "$OUT_DIR/regional16" \
  "$OUT_DIR/regional32" "$OUT_DIR/compare"

for scene in "${SCENES[@]}"; do
  jq --arg kind "$scene" \
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
    --arg output "Regional_Hardening_${scene}.png" '
      def center($index):
        [((($index % 4) - 1.5) * 1.22),
         ((((($index / 4) | floor) % 3) - 1.0) * 1.15),
         (((($index / 12) | floor) - 0.5) * 1.35)];
      def distributed($count; $mixed; $orbit):
        (reduce range(0; $count) as $index
          ({previous:[0,0,0], operations:[]};
           (center($index)) as $current |
           .operations += [
             {op:"translate",
              value:[($current[0] - .previous[0]),
                     ($current[1] - .previous[1]),
                     ($current[2] - .previous[2])]},
             (if $mixed and ($index % 2 == 1) then
                {op:"box", value:[0.42,0.34,0.38],
                 orbit_weight:(if $orbit then 0.12 + $index * 0.01 else 0 end)}
              else
                {op:"sphere", radius:0.46,
                 orbit_weight:(if $orbit then 0.12 + $index * 0.01 else 0 end)}
              end)
           ] |
           .previous = $current) | .operations);
      (if $kind == "mixed_transforms" then
         [{op:"rotate_y",angle:0.19},{op:"rotate_x",angle:-0.11}] +
         distributed(24; true; false)
       elif $kind == "orbit_material" then
         distributed(16; true; true)
       elif $kind == "tie_boundaries" then
         [{op:"translate",value:[-0.75,0,0]},
          {op:"sphere",radius:0.92,orbit_weight:0.2},
          {op:"translate",value:[1.5,0,0]},
          {op:"sphere",radius:0.92,orbit_weight:0.3},
          {op:"translate",value:[-0.75,-1.25,0]},
          {op:"box",value:[0.72,0.32,0.52],orbit_weight:0.15}]
       else
         [{op:"repeat",value:[2.4,2.4,2.4]},
          {op:"translate",value:[-0.52,0,0]},
          {op:"sphere",radius:0.58,orbit_weight:0.18},
          {op:"translate",value:[1.04,0,0]},
          {op:"box",value:[0.42,0.48,0.36],orbit_weight:0.22}]
       end) as $operations |
      .preset = $preset |
      .output = $output |
      .width = 320 |
      .height = 180 |
      .samples = 64 |
      .camera.position = [0.0,0.2,-7.2] |
      .camera.focus_distance = 7.2 |
      .sdf_program.operations = $operations |
      .sdf_program.material.mode =
        (if $kind == "orbit_material" or $kind == "tie_boundaries" or
            $kind == "repeat_seams" then "gradient" else "constant" end) |
      .sdf_program.material.color = [0.16,0.64,0.92] |
      .sdf_program.gradient = [
        {position:0,color:[0.04,0.18,0.72]},
        {position:0.5,color:[0.10,0.82,0.66]},
        {position:1,color:[0.94,0.24,0.08]}
      ]
    ' "$ROOT_DIR/scenes/benchmarks/Exact_Sphere.json" \
    > "$OUT_DIR/fixtures/$scene.json"
done

COMMON_ARGS=(--width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
  --sdf-bounce-cap "$BOUNCE_CAP" --sdf-program-optimization basic)
if [[ "$FLAT_UNION" == "1" ]]; then COMMON_ARGS+=(--sdf-flat-union); fi
if [[ "$GEOMETRY_SPLIT" == "1" ]]; then COMMON_ARGS+=(--sdf-geometry-split); fi
if [[ "$GEOMETRY_SPLIT" != "1" ]]; then COMMON_ARGS+=(--no-sdf-geometry-split); fi

render_variant() {
  local variant="$1" scene="$2" output="$3"
  case "$variant" in
    sdf)
      "$BIN" render "$scene" --out "$output" --renderer sdf \
        "${COMMON_ARGS[@]}" ;;
    regional16)
      "$BIN" render "$scene" --out "$output" --renderer regional \
        --regional-program-resolution 16 --bound-grid-profile \
        "${COMMON_ARGS[@]}" ;;
    regional32)
      "$BIN" render "$scene" --out "$output" --renderer regional \
        --regional-program-resolution 32 --bound-grid-profile \
        "${COMMON_ARGS[@]}" ;;
  esac
}

for ((run = 0; run < RUNS; run++)); do
  for variant in sdf regional16 regional32; do
    mkdir -p "$OUT_DIR/$variant/run_$run"
  done
  for scene in "${SCENES[@]}"; do
    if ((run % 2 == 0)); then
      order=(sdf regional16 regional32)
    else
      order=(regional32 regional16 sdf)
    fi
    for variant in "${order[@]}"; do
      render_variant "$variant" "$OUT_DIR/fixtures/$scene.json" \
        "$OUT_DIR/$variant/run_$run"
    done
  done
done

: > "$OUT_DIR/records.jsonl"
for scene in "${SCENES[@]}"; do
  output="Regional_Hardening_${scene}.png"
  for variant in regional16 regional32; do
    "$BIN" compare "$OUT_DIR/sdf/run_0/$output" \
      "$OUT_DIR/$variant/run_0/$output" \
      --report "$OUT_DIR/compare/$scene-$variant.json"
  done
  jq -n --arg scene "$scene" \
    --slurpfile sdf <(jq -s . "$OUT_DIR"/sdf/run_*/"$output.render.json") \
    --slurpfile r16 <(jq -s . "$OUT_DIR"/regional16/run_*/"$output.render.json") \
    --slurpfile r32 <(jq -s . "$OUT_DIR"/regional32/run_*/"$output.render.json") \
    --slurpfile q16 "$OUT_DIR/compare/$scene-regional16.json" \
    --slurpfile q32 "$OUT_DIR/compare/$scene-regional32.json" '
      def median(values): values | sort | .[length / 2 | floor];
      {scene:$scene,instruction_count:$sdf[0][0].sdf_program_instruction_count,
       direct_evaluator:$sdf[0][0].sdf_distance_evaluator,
       flat_union_primitive_count:$sdf[0][0].sdf_flat_union_primitive_count,
       regional_lowering:$r32[0][0].regional_program_lowering,
       sdf_ms:median($sdf[0]|map(.elapsed_ms)),
       regional16:{render_ms:median($r16[0]|map(.elapsed_ms)),
                   build_ms:median($r16[0]|map(.regional_program_build_ms)),
                   profile:$r16[0][0].regional_program_profile,quality:$q16[0]},
       regional32:{render_ms:median($r32[0]|map(.elapsed_ms)),
                   build_ms:median($r32[0]|map(.regional_program_build_ms)),
                   profile:$r32[0][0].regional_program_profile,quality:$q32[0]}}' \
    >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson runs "$RUNS" --argjson samples "$SAMPLES" \
  --argjson flat_union "$FLAT_UNION" \
  --argjson geometry_split "$GEOMETRY_SPLIT" '
  map(. + {sdf_over_regional16:(.sdf_ms/.regional16.render_ms),
           sdf_over_regional32:(.sdf_ms/.regional32.render_ms),
           regional16_end_to_end:
             (.sdf_ms/(.regional16.render_ms+.regional16.build_ms)),
           regional32_end_to_end:
             (.sdf_ms/(.regional32.render_ms+.regional32.build_ms))}) as $rows |
  {settings:{runs:$runs,samples:$samples,flat_union:($flat_union != 0),
             geometry_split:($geometry_split != 0)},rows:$rows,
   validation:{sampled_distance_failures:
     ([$rows[].regional16.profile.sampled_distance_failures,
       $rows[].regional32.profile.sampled_distance_failures]|add),
     maximum_mae:
       ([$rows[].regional16.quality.mean_absolute_error,
         $rows[].regional32.quality.mean_absolute_error]|max),
     minimum_lf_ssim:
       ([$rows[].regional16.quality.low_frequency_luminance_ssim,
         $rows[].regional32.quality.low_frequency_luminance_ssim]|min)}}
  ' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

jq . "$OUT_DIR/summary.json"
printf 'Regional hardening experiment: %s\n' "$OUT_DIR"
