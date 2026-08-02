#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/flat-union/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-5}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-64}"
COUNTS=(8 16 24 32)
SCENES=(union_8 union_16 union_24 union_32 mixed_affine)

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
mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/bytecode" "$OUT_DIR/primitive-list" \
  "$OUT_DIR/compare"

for primitive_count in "${COUNTS[@]}"; do
  jq --argjson primitive_count "$primitive_count" \
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
    --arg output "Flat_Union_${primitive_count}.png" '
      def center($index):
        [((($index % 4) - 1.5) * 1.25),
         ((((($index / 4) | floor) % 4) - 1.5) * 1.05),
         (((($index / 16) | floor) - 0.5) * 1.25)];
      (reduce range(0; $primitive_count) as $index
        ({previous:[0,0,0], operations:[]};
         (center($index)) as $current |
         .operations += [
           {op:"translate",
            value:[($current[0] - .previous[0]),
                   ($current[1] - .previous[1]),
                   ($current[2] - .previous[2])]},
           {op:"sphere",radius:0.46}
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
    > "$OUT_DIR/fixtures/union_${primitive_count}.json"
done

jq --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" '
  def center($index):
    [((($index % 4) - 1.5) * 1.15),
     ((((($index / 4) | floor) % 3) - 1.0) * 1.10),
     (((($index / 12) | floor) - 0.5) * 1.30)];
  (reduce range(0;24) as $index
    ({previous:[0,0,0],operations:[
       {op:"rotate_y",angle:0.19},{op:"rotate_x",angle:-0.11},
       {op:"scale",value:1.07}]};
     (center($index)) as $current |
     .operations += [
       {op:"translate",
        value:[($current[0] - .previous[0]),
               ($current[1] - .previous[1]),
               ($current[2] - .previous[2])]},
       (if ($index % 2) == 0 then {op:"sphere",radius:0.43}
        else {op:"box",value:[0.38,0.32,0.35]} end)
     ] | .previous = $current)) as $program |
  .preset = $preset |
  .output = "Flat_Union_mixed_affine.png" |
  .camera.position = [0.0,0.15,-7.2] |
  .camera.focus_distance = 7.2 |
  .sdf_program.operations = $program.operations |
  .sdf_program.material.mode = "constant" |
  .sdf_program.material.color = [0.18,0.68,0.88]
' "$ROOT_DIR/scenes/benchmarks/Exact_Sphere.json" \
  > "$OUT_DIR/fixtures/mixed_affine.json"

COMMON_ARGS=(--width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
  --sdf-bounce-cap 1 --sdf-program-optimization basic --renderer sdf)

for ((run = 0; run < RUNS; run++)); do
  mkdir -p "$OUT_DIR/bytecode/run_$run" "$OUT_DIR/primitive-list/run_$run"
  for scene in "${SCENES[@]}"; do
    fixture="$OUT_DIR/fixtures/$scene.json"
    if ((run % 2 == 0)); then variants=(bytecode primitive-list)
    else variants=(primitive-list bytecode); fi
    for variant in "${variants[@]}"; do
      extra=()
      [[ "$variant" == "primitive-list" ]] && extra=(--sdf-flat-union)
      "$BIN" render "$fixture" --out "$OUT_DIR/$variant/run_$run" \
        "${COMMON_ARGS[@]}" "${extra[@]}"
    done
  done
done

: > "$OUT_DIR/records.jsonl"
for scene in "${SCENES[@]}"; do
  if [[ "$scene" == mixed_affine ]]; then output="Flat_Union_mixed_affine.png"
  else output="Flat_Union_${scene#union_}.png"; fi
  "$BIN" compare "$OUT_DIR/bytecode/run_0/$output" \
    "$OUT_DIR/primitive-list/run_0/$output" \
    --report "$OUT_DIR/compare/$scene.json"
  jq -n --arg scene "$scene" \
    --slurpfile bytecode <(jq -s . "$OUT_DIR"/bytecode/run_*/"$output.render.json") \
    --slurpfile lowered <(jq -s . "$OUT_DIR"/primitive-list/run_*/"$output.render.json") \
    --slurpfile quality "$OUT_DIR/compare/$scene.json" '
      def median(values): values | sort | .[length / 2 | floor];
      {scene:$scene,
       source_instruction_count:$bytecode[0][0].sdf_program_instruction_count,
       primitive_count:$lowered[0][0].sdf_flat_union_primitive_count,
       bytecode_ms:median($bytecode[0]|map(.elapsed_ms)),
       primitive_list_ms:median($lowered[0]|map(.elapsed_ms)),
       quality:$quality[0]} |
      . + {speedup:(.bytecode_ms/.primitive_list_ms)}
    ' >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson runs "$RUNS" --argjson samples "$SAMPLES" \
  '{settings:{runs:$runs,samples:$samples},rows:.,validation:{
    maximum_mae:([.[].quality.mean_absolute_error]|max),
    minimum_lf_ssim:([.[].quality.low_frequency_luminance_ssim]|min)}}' \
  "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"
jq . "$OUT_DIR/summary.json"
printf 'Flat-union experiment: %s\n' "$OUT_DIR"
