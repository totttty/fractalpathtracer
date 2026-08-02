#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
REFINEMENT="${REFINEMENT:-secant-bisection}"
case "$REFINEMENT" in
  secant-bisection|restricted-trace|fixed-de) ;;
  *) printf 'invalid REFINEMENT: %s\n' "$REFINEMENT" >&2; exit 2 ;;
esac
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/voxel-optimization/leaf-refinement-$REFINEMENT-$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-5}"
WIDTH="${WIDTH:-960}"
HEIGHT="${HEIGHT:-540}"
SAMPLES="${SAMPLES:-112}"

command -v jq >/dev/null || { printf 'jq is required\n' >&2; exit 2; }
[[ ! -e "$OUT_DIR" ]] || { printf 'refusing to overwrite %s\n' "$OUT_DIR" >&2; exit 2; }
cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo test
  cargo build --release
fi

SCENES=(
  "$ROOT_DIR/scenes/benchmarks/Exact_Sphere.json"
  "$ROOT_DIR/scenes/benchmarks/Exact_Box.json"
  "$ROOT_DIR/scenes/benchmarks/Exact_Plane.json"
  "$ROOT_DIR/scenes/benchmarks/Exact_CSG.json"
)
VOXEL_ARGS=(
  --renderer voxel --voxel-resolution 256 --voxel-normal face
  --voxel-storage sparse-bricks --voxel-coverage interval --voxel-build direct
  --voxel-brick-rejection --width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
)
SDF_ARGS=(--renderer sdf --width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES")
DIAGNOSTIC_ARGS=(--mode depth --width "$WIDTH" --height "$HEIGHT" --samples 1)

mkdir -p "$OUT_DIR/off" "$OUT_DIR/on" "$OUT_DIR/sdf" \
  "$OUT_DIR/depth/off" "$OUT_DIR/depth/on" "$OUT_DIR/depth/sdf" "$OUT_DIR/compare"

for ((run = 0; run < RUNS; run++)); do
  mkdir -p "$OUT_DIR/off/run_$run" "$OUT_DIR/on/run_$run"
  for scene in "${SCENES[@]}"; do
    if ((run % 2 == 0)); then
      "$BIN" render "$scene" --out "$OUT_DIR/off/run_$run" \
        --voxel-leaf-refinement none "${VOXEL_ARGS[@]}"
      "$BIN" render "$scene" --out "$OUT_DIR/on/run_$run" \
        --voxel-leaf-refinement "$REFINEMENT" "${VOXEL_ARGS[@]}"
    else
      "$BIN" render "$scene" --out "$OUT_DIR/on/run_$run" \
        --voxel-leaf-refinement "$REFINEMENT" "${VOXEL_ARGS[@]}"
      "$BIN" render "$scene" --out "$OUT_DIR/off/run_$run" \
        --voxel-leaf-refinement none "${VOXEL_ARGS[@]}"
    fi
  done
done

for scene in "${SCENES[@]}"; do
  "$BIN" render "$scene" --out "$OUT_DIR/sdf" "${SDF_ARGS[@]}"
  "$BIN" diagnostic "$scene" --out "$OUT_DIR/depth/sdf" \
    --renderer sdf "${DIAGNOSTIC_ARGS[@]}"
  "$BIN" diagnostic "$scene" --out "$OUT_DIR/depth/off" \
    --voxel-leaf-refinement none "${VOXEL_ARGS[@]:0:13}" "${DIAGNOSTIC_ARGS[@]}"
  "$BIN" diagnostic "$scene" --out "$OUT_DIR/depth/on" \
    --voxel-leaf-refinement "$REFINEMENT" "${VOXEL_ARGS[@]:0:13}" "${DIAGNOSTIC_ARGS[@]}"
done

: > "$OUT_DIR/records.jsonl"
for scene in "${SCENES[@]}"; do
  output="$(jq -r .output "$scene")"
  stem="${output%.png}"
  "$BIN" compare "$OUT_DIR/off/run_0/$output" "$OUT_DIR/on/run_0/$output" \
    --report "$OUT_DIR/compare/$stem-off-on.json"
  "$BIN" compare "$OUT_DIR/sdf/$output" "$OUT_DIR/off/run_0/$output" \
    --report "$OUT_DIR/compare/$stem-sdf-off.json"
  "$BIN" compare "$OUT_DIR/sdf/$output" "$OUT_DIR/on/run_0/$output" \
    --report "$OUT_DIR/compare/$stem-sdf-on.json"
  "$BIN" compare "$OUT_DIR/depth/sdf/$output" "$OUT_DIR/depth/off/$output" \
    --report "$OUT_DIR/compare/$stem-depth-off.json"
  "$BIN" compare "$OUT_DIR/depth/sdf/$output" "$OUT_DIR/depth/on/$output" \
    --report "$OUT_DIR/compare/$stem-depth-on.json"

  off_metadata=()
  on_metadata=()
  for ((run = 0; run < RUNS; run++)); do
    off_metadata+=("$OUT_DIR/off/run_$run/$output.render.json")
    on_metadata+=("$OUT_DIR/on/run_$run/$output.render.json")
  done
  jq -s . "${on_metadata[@]}" > "$OUT_DIR/$stem-on-runs.json"
  jq -s --arg scene "$(basename "$scene" .json)" \
    --slurpfile on "$OUT_DIR/$stem-on-runs.json" \
    --slurpfile off_on "$OUT_DIR/compare/$stem-off-on.json" \
    --slurpfile sdf_off "$OUT_DIR/compare/$stem-sdf-off.json" \
    --slurpfile sdf_on "$OUT_DIR/compare/$stem-sdf-on.json" \
    --slurpfile depth_off "$OUT_DIR/compare/$stem-depth-off.json" \
    --slurpfile depth_on "$OUT_DIR/compare/$stem-depth-on.json" '
      def median(values): values | sort | .[length / 2 | floor];
      . as $off | $on[0] as $on_runs |
      {
        scene:$scene,
        off:{render_ms:median($off|map(.elapsed_ms))},
        on:{render_ms:median($on_runs|map(.elapsed_ms))},
        off_on:$off_on[0],
        sdf_off:$sdf_off[0],
        sdf_on:$sdf_on[0],
        depth_off:$depth_off[0],
        depth_on:$depth_on[0]
      }' "${off_metadata[@]}" >> "$OUT_DIR/records.jsonl"
done

jq -s --arg refinement "$REFINEMENT" '
  (map(.off.render_ms)|add) as $off_time |
  (map(.on.render_ms)|add) as $on_time |
  {
    settings:{refinement:$refinement,resolution:256,bounds_min:[-3.25,-3.25,-3.25],bounds_max:[3.25,3.25,3.25],
      surface_band:0.5,normals:"face",width:'"$WIDTH"',height:'"$HEIGHT"',samples:'"$SAMPLES"',runs:'"$RUNS"'},
    scenes:.,
    aggregate:{
      render_speed_ratio:($off_time/$on_time),
      shaded_mae_before:(map(.sdf_off.mean_absolute_error)|add/length),
      shaded_mae_after:(map(.sdf_on.mean_absolute_error)|add/length),
      shaded_lf_ssim_before:(map(.sdf_off.low_frequency_luminance_ssim)|add/length),
      shaded_lf_ssim_after:(map(.sdf_on.low_frequency_luminance_ssim)|add/length),
      depth_mae_before:(map(.depth_off.mean_absolute_error)|add/length),
      depth_mae_after:(map(.depth_on.mean_absolute_error)|add/length),
      default_images_changed:all(.off_on.mean_absolute_error>0),
      promotion_gate:{minimum_render_speed_ratio:0.95,require_depth_mae_improvement:true,
        maximum_shaded_mae_regression_percent:1.0},
      passes_promotion:(($off_time/$on_time)>=0.95 and
        (map(.depth_on.mean_absolute_error)|add) < (map(.depth_off.mean_absolute_error)|add) and
        (map(.sdf_on.mean_absolute_error)|add) <= (map(.sdf_off.mean_absolute_error)|add)*1.01)
    }
  }' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

"$BIN" contact-sheet "$OUT_DIR/shaded-sdf.png" "$OUT_DIR"/sdf/*.png
"$BIN" contact-sheet "$OUT_DIR/shaded-voxel-off.png" "$OUT_DIR"/off/run_0/*.png
"$BIN" contact-sheet "$OUT_DIR/shaded-voxel-refined.png" "$OUT_DIR"/on/run_0/*.png
"$BIN" contact-sheet "$OUT_DIR/depth-comparison.png" "$OUT_DIR"/compare/*-depth-*.comparison.png

jq . "$OUT_DIR/summary.json"
printf 'leaf refinement experiment: %s\n' "$OUT_DIR"
