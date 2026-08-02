#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/bound-grid-directional-fp16/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-5}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-64}"
BOUNCE_CAP="${BOUNCE_CAP:-1}"

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
SCENES=(Exact_Sphere Exact_Plane Exact_Box Exact_CSG)
mkdir -p "$OUT_DIR/sdf" "$OUT_DIR/fp32" "$OUT_DIR/fp16" "$OUT_DIR/compare"
COMMON=(--width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
  --sdf-bounce-cap "$BOUNCE_CAP")

render_variant() {
  local variant="$1" scene="$2" output="$3"
  case "$variant" in
    sdf)
      "$BIN" render "$scene" --out "$output" --renderer sdf "${COMMON[@]}" ;;
    fp32)
      "$BIN" render "$scene" --out "$output" --renderer bound-grid \
        --bound-grid-resolution 32 --bound-grid-directional \
        --bound-grid-profile "${COMMON[@]}" ;;
    fp16)
      "$BIN" render "$scene" --out "$output" --renderer bound-grid \
        --bound-grid-resolution 32 --bound-grid-directional --bound-grid-fp16 \
        --bound-grid-profile "${COMMON[@]}" ;;
  esac
}

for ((run=0; run<RUNS; run++)); do
  for variant in sdf fp32 fp16; do mkdir -p "$OUT_DIR/$variant/run_$run"; done
  for name in "${SCENES[@]}"; do
    scene="$ROOT_DIR/scenes/benchmarks/$name.json"
    if ((run % 2 == 0)); then order=(sdf fp32 fp16); else order=(fp16 fp32 sdf); fi
    for variant in "${order[@]}"; do
      render_variant "$variant" "$scene" "$OUT_DIR/$variant/run_$run"
    done
  done
done

: > "$OUT_DIR/records.jsonl"
for name in "${SCENES[@]}"; do
  output="$name.png"
  "$BIN" compare "$OUT_DIR/sdf/run_0/$output" "$OUT_DIR/fp32/run_0/$output" \
    --report "$OUT_DIR/compare/$name-sdf-fp32.json"
  "$BIN" compare "$OUT_DIR/sdf/run_0/$output" "$OUT_DIR/fp16/run_0/$output" \
    --report "$OUT_DIR/compare/$name-sdf-fp16.json"
  "$BIN" compare "$OUT_DIR/fp32/run_0/$output" "$OUT_DIR/fp16/run_0/$output" \
    --report "$OUT_DIR/compare/$name-fp32-fp16.json"
  jq -n --arg scene "$name" \
    --slurpfile sdf <(jq -s . "$OUT_DIR"/sdf/run_*/"$output.render.json") \
    --slurpfile fp32 <(jq -s . "$OUT_DIR"/fp32/run_*/"$output.render.json") \
    --slurpfile fp16 <(jq -s . "$OUT_DIR"/fp16/run_*/"$output.render.json") \
    --slurpfile q32 "$OUT_DIR/compare/$name-sdf-fp32.json" \
    --slurpfile q16 "$OUT_DIR/compare/$name-sdf-fp16.json" \
    --slurpfile qp "$OUT_DIR/compare/$name-fp32-fp16.json" '
      def median(values): values|sort|.[length/2|floor];
      {scene:$scene,sdf_ms:median($sdf[0]|map(.elapsed_ms)),
       fp32:{render_ms:median($fp32[0]|map(.elapsed_ms)),
             build_ms:median($fp32[0]|map(.bound_grid_build_ms)),
             memory_bytes:$fp32[0][0].bound_grid_memory_bytes,
             profile:$fp32[0][0].bound_grid_profile,quality:$q32[0]},
       fp16:{render_ms:median($fp16[0]|map(.elapsed_ms)),
             build_ms:median($fp16[0]|map(.bound_grid_build_ms)),
             memory_bytes:$fp16[0][0].bound_grid_memory_bytes,
             profile:$fp16[0][0].bound_grid_profile,quality:$q16[0],
             versus_fp32:$qp[0]}}' >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson runs "$RUNS" --argjson samples "$SAMPLES" '
  map(. + {fp32_over_fp16:(.fp32.render_ms/.fp16.render_ms),
           sdf_over_fp16:(.sdf_ms/.fp16.render_ms)}) as $rows |
  {settings:{runs:$runs,samples:$samples,resolution:32},rows:$rows,
   aggregate:{fp32_render_ms:($rows|map(.fp32.render_ms)|add),
              fp16_render_ms:($rows|map(.fp16.render_ms)|add),
              fp32_over_fp16:
                (($rows|map(.fp32.render_ms)|add)/($rows|map(.fp16.render_ms)|add)),
              memory_reduction:
                (($rows[0].fp32.memory_bytes)/($rows[0].fp16.memory_bytes))},
   validation:{sampled_bound_failures:
     ([$rows[].fp16.profile.sampled_bound_failures]|add),
     sampled_derivative_failures:
     ([$rows[].fp16.profile.sampled_derivative_failures]|add),
     maximum_fp16_vs_fp32_mae:
     ([$rows[].fp16.versus_fp32.mean_absolute_error]|max),
     minimum_fp16_vs_fp32_lf_ssim:
     ([$rows[].fp16.versus_fp32.low_frequency_luminance_ssim]|min)}}
  ' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"
jq . "$OUT_DIR/summary.json"
printf 'Directional FP16 experiment: %s\n' "$OUT_DIR"
