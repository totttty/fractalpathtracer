#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/backend-selection/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-64}"
COUNTS=(8 16 24 32)
SCENES=(union_8 union_16 union_24 union_32 mixed_translation)

command -v jq >/dev/null || { printf 'jq is required\n' >&2; exit 2; }
if [[ -e "$OUT_DIR" ]]; then
  printf 'refusing to overwrite existing report directory: %s\n' "$OUT_DIR" >&2
  exit 2
fi
cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo build --release
fi
mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/backend-cache" "$OUT_DIR/stitch-cache" \
  "$OUT_DIR/compare" "$OUT_DIR/bytecode" "$OUT_DIR/auto-cold" \
  "$OUT_DIR/auto-warm" "$OUT_DIR/stitched"
export FPT_SDF_BACKEND_CACHE_DIR="$OUT_DIR/backend-cache"
export FPT_STITCH_CACHE_DIR="$OUT_DIR/stitch-cache"

for primitive_count in "${COUNTS[@]}"; do
  jq --argjson primitive_count "$primitive_count" \
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
    --arg output "Backend_Union_${primitive_count}.png" '
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
    > "$OUT_DIR/fixtures/union_${primitive_count}.json"
done

jq --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" '
  def center($index):
    [((($index % 4) - 1.5) * 1.20),
     ((((($index / 4) | floor) % 3) - 1.0) * 1.10),
     (((($index / 12) | floor) - 0.5) * 1.30)];
  (reduce range(0;24) as $index
    ({previous:[0,0,0],operations:[]};
     (center($index)) as $current |
     .operations += [
       {op:"translate",
        value:[($current[0] - .previous[0]),
               ($current[1] - .previous[1]),
               ($current[2] - .previous[2])]},
       (if ($index % 2) == 0 then {op:"sphere",radius:0.41}
        else {op:"box",value:[0.36,0.31,0.34]} end)
     ] | .previous = $current)) as $program |
  .preset = $preset |
  .output = "Backend_mixed_translation.png" |
  .camera.position = [0.0,0.15,-7.2] |
  .camera.focus_distance = 7.2 |
  .sdf_program.operations = $program.operations |
  .sdf_program.material.mode = "constant" |
  .sdf_program.material.color = [0.18,0.68,0.88]
' "$ROOT_DIR/scenes/benchmarks/Exact_Sphere.json" \
  > "$OUT_DIR/fixtures/mixed_translation.json"

COMMON_ARGS=(--width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
  --sdf-bounce-cap 1 --sdf-program-optimization basic --renderer sdf)
: > "$OUT_DIR/records.jsonl"
for scene in "${SCENES[@]}"; do
  fixture="$OUT_DIR/fixtures/$scene.json"
  if [[ "$scene" == mixed_translation ]]; then
    output="Backend_mixed_translation.png"
  else
    output="Backend_Union_${scene#union_}.png"
  fi
  "$BIN" render "$fixture" --out "$OUT_DIR/bytecode" "${COMMON_ARGS[@]}"
  "$BIN" render "$fixture" --out "$OUT_DIR/auto-cold" "${COMMON_ARGS[@]}" \
    --sdf-backend auto
  "$BIN" render "$fixture" --out "$OUT_DIR/auto-warm" "${COMMON_ARGS[@]}" \
    --sdf-backend auto
  "$BIN" render "$fixture" --out "$OUT_DIR/stitched" "${COMMON_ARGS[@]}" \
    --sdf-function-stitching inline
  for variant in auto-cold auto-warm stitched; do
    "$BIN" compare "$OUT_DIR/bytecode/$output" "$OUT_DIR/$variant/$output" \
      --report "$OUT_DIR/compare/$scene-$variant.json"
  done
  jq -n --arg scene "$scene" \
    --slurpfile bytecode "$OUT_DIR/bytecode/$output.render.json" \
    --slurpfile cold "$OUT_DIR/auto-cold/$output.render.json" \
    --slurpfile warm "$OUT_DIR/auto-warm/$output.render.json" \
    --slurpfile stitched "$OUT_DIR/stitched/$output.render.json" \
    --slurpfile cold_quality "$OUT_DIR/compare/$scene-auto-cold.json" \
    --slurpfile warm_quality "$OUT_DIR/compare/$scene-auto-warm.json" \
    --slurpfile stitched_quality "$OUT_DIR/compare/$scene-stitched.json" '
      {scene:$scene,
       source_instruction_count:$bytecode[0].sdf_program_instruction_count,
       selection:$cold[0].sdf_backend_selection,
       cached_selection:$warm[0].sdf_backend_selection,
       bytecode_ms:$bytecode[0].elapsed_ms,
       auto_cold_ms:$cold[0].elapsed_ms,
       auto_warm_ms:$warm[0].elapsed_ms,
       stitched_ms:$stitched[0].elapsed_ms,
       stitch_pipeline:$stitched[0].sdf_stitch_pipeline_stats,
       quality:{cold:$cold_quality[0],warm:$warm_quality[0],
                stitched:$stitched_quality[0]}}
    ' >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson width "$WIDTH" --argjson height "$HEIGHT" \
  --argjson samples "$SAMPLES" '
  {settings:{width:$width,height:$height,samples:$samples},rows:.,
   validation:{
     cache_reuse:(all(.[];
       .cached_selection.decision_source == "cache" and
       .cached_selection.selected == .selection.selected)),
     candidate_coverage:(all(.[];
       (.selection.candidates | has("generated-distance")) and
       (.selection.candidates | has("generated-surface")))),
     valid_decisions:(all(.[];
       .selection.decision_source == "probe" and
       if .selection.selected == "direct" then true
       elif .selection.selected == "generated-surface" then
         .selection.candidates["generated-surface"].qualified and
         ((if .selection.candidates["generated-distance"].qualified
           then .selection.candidates["generated-distance"].predicted_amortized_ms
           else .selection.predicted_direct_ms end) /
          .selection.candidates["generated-surface"].predicted_amortized_ms >=
          .selection.generated_surface_required_speedup)
       else .selection.selected == "generated-distance" and
            .selection.candidates["generated-distance"].qualified end)),
     maximum_mae:([.[]|.quality[]|.mean_absolute_error]|max),
     minimum_ssim:([.[]|.quality[]|.luminance_ssim]|min),
     minimum_lf_ssim:([.[]|.quality[]|.low_frequency_luminance_ssim]|min)}}
' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"
jq -e '
  .validation.cache_reuse and .validation.candidate_coverage and
  .validation.valid_decisions and
  .validation.maximum_mae <= 0.0001 and
  .validation.minimum_ssim >= 0.999999 and
  .validation.minimum_lf_ssim >= 0.999999
' "$OUT_DIR/summary.json" >/dev/null
jq . "$OUT_DIR/summary.json"
printf 'Backend-selection experiment: %s\n' "$OUT_DIR"
