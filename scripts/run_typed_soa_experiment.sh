#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/typed-soa/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-7}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-64}"
COUNTS=(8 16 24 32)
SCENES=(union_8 union_16 union_24 union_32 mixed_translation)
VARIANTS=(bytecode aos typed-soa stitched)

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
mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/cache" "$OUT_DIR/compare"
for variant in "${VARIANTS[@]}"; do mkdir -p "$OUT_DIR/$variant"; done
export FPT_STITCH_CACHE_DIR="$OUT_DIR/cache"

for primitive_count in "${COUNTS[@]}"; do
  jq --argjson primitive_count "$primitive_count" \
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
    --arg output "Typed_SoA_Union_${primitive_count}.png" '
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
  .output = "Typed_SoA_mixed_translation.png" |
  .camera.position = [0.0,0.15,-7.2] |
  .camera.focus_distance = 7.2 |
  .sdf_program.operations = $program.operations |
  .sdf_program.material.mode = "constant" |
  .sdf_program.material.color = [0.18,0.68,0.88]
' "$ROOT_DIR/scenes/benchmarks/Exact_Sphere.json" \
  > "$OUT_DIR/fixtures/mixed_translation.json"

COMMON_ARGS=(--width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
  --sdf-bounce-cap 1 --sdf-program-optimization basic --renderer sdf)

for ((run = 0; run < RUNS; run++)); do
  case $((run % 4)) in
    0) order=(bytecode aos typed-soa stitched) ;;
    1) order=(stitched typed-soa aos bytecode) ;;
    2) order=(aos bytecode stitched typed-soa) ;;
    *) order=(typed-soa stitched bytecode aos) ;;
  esac
  for variant in "${VARIANTS[@]}"; do mkdir -p "$OUT_DIR/$variant/run_$run"; done
  for scene in "${SCENES[@]}"; do
    fixture="$OUT_DIR/fixtures/$scene.json"
    for variant in "${order[@]}"; do
      extra=()
      [[ "$variant" == aos ]] && extra=(--sdf-flat-union)
      [[ "$variant" == typed-soa ]] && extra=(--sdf-typed-soa)
      [[ "$variant" == stitched ]] && extra=(--sdf-function-stitching inline)
      "$BIN" render "$fixture" --out "$OUT_DIR/$variant/run_$run" \
        "${COMMON_ARGS[@]}" "${extra[@]}"
    done
  done
done

: > "$OUT_DIR/records.jsonl"
for scene in "${SCENES[@]}"; do
  if [[ "$scene" == mixed_translation ]]; then
    output="Typed_SoA_mixed_translation.png"
  else
    output="Typed_SoA_Union_${scene#union_}.png"
  fi
  for variant in aos typed-soa stitched; do
    "$BIN" compare "$OUT_DIR/bytecode/run_0/$output" \
      "$OUT_DIR/$variant/run_0/$output" \
      --report "$OUT_DIR/compare/$scene-$variant.json"
  done
  jq -n --arg scene "$scene" \
    --slurpfile bytecode <(jq -s . "$OUT_DIR"/bytecode/run_*/"$output.render.json") \
    --slurpfile aos <(jq -s . "$OUT_DIR"/aos/run_*/"$output.render.json") \
    --slurpfile soa <(jq -s . "$OUT_DIR"/typed-soa/run_*/"$output.render.json") \
    --slurpfile stitched <(jq -s . "$OUT_DIR"/stitched/run_*/"$output.render.json") \
    --slurpfile aos_quality "$OUT_DIR/compare/$scene-aos.json" \
    --slurpfile soa_quality "$OUT_DIR/compare/$scene-typed-soa.json" \
    --slurpfile stitched_quality "$OUT_DIR/compare/$scene-stitched.json" '
      def median(values): values | sort | .[length / 2 | floor];
      {scene:$scene,
       source_instruction_count:$bytecode[0][0].sdf_program_instruction_count,
       primitive_count:$soa[0][0].sdf_typed_soa_primitive_count,
       bytecode_ms:median($bytecode[0]|map(.elapsed_ms)),
       aos_ms:median($aos[0]|map(.elapsed_ms)),
       typed_soa_ms:median($soa[0]|map(.elapsed_ms)),
       stitched_ms:median($stitched[0]|map(.elapsed_ms)),
       stitched_setup_ms:median($stitched[0]|map(.sdf_stitched_library_build_ms)),
       quality:{aos:$aos_quality[0],typed_soa:$soa_quality[0],
                stitched:$stitched_quality[0]}} |
      . + {speedup_vs_bytecode:{aos:(.bytecode_ms/.aos_ms),
            typed_soa:(.bytecode_ms/.typed_soa_ms),
            stitched:(.bytecode_ms/.stitched_ms)},
           typed_soa_vs_aos:(.aos_ms/.typed_soa_ms),
           stitched_vs_typed_soa:(.typed_soa_ms/.stitched_ms)}
    ' >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson runs "$RUNS" --argjson samples "$SAMPLES" \
  --argjson width "$WIDTH" --argjson height "$HEIGHT" '
  {settings:{runs:$runs,width:$width,height:$height,samples:$samples},rows:.,
   validation:{maximum_mae:([.[]|.quality[]|.mean_absolute_error]|max),
     minimum_ssim:([.[]|.quality[]|.luminance_ssim]|min),
     minimum_lf_ssim:([.[]|.quality[]|.low_frequency_luminance_ssim]|min)}}
' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

jq -e '
  .validation.maximum_mae <= 0.0001 and
  .validation.minimum_ssim >= 0.999999 and
  .validation.minimum_lf_ssim >= 0.999999
' \
  "$OUT_DIR/summary.json" >/dev/null
jq . "$OUT_DIR/summary.json"
printf 'Typed-SoA experiment: %s\n' "$OUT_DIR"
