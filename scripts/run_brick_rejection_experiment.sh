#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/voxel-optimization/brick-rejection-$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-5}"

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
COMMON_ARGS=(
  --renderer voxel --voxel-resolution 256 --voxel-normal face
  --voxel-storage sparse-bricks --voxel-coverage interval --voxel-build direct
  --width 320 --height 180 --samples 16
)

mkdir -p "$OUT_DIR/off" "$OUT_DIR/on" "$OUT_DIR/compare"
for ((run = 0; run < RUNS; run++)); do
  mkdir -p "$OUT_DIR/off/run_$run" "$OUT_DIR/on/run_$run"
  for scene in "${SCENES[@]}"; do
    if ((run % 2 == 0)); then
      "$BIN" render "$scene" --out "$OUT_DIR/off/run_$run" "${COMMON_ARGS[@]}"
      "$BIN" render "$scene" --out "$OUT_DIR/on/run_$run" \
        --voxel-brick-rejection "${COMMON_ARGS[@]}"
    else
      "$BIN" render "$scene" --out "$OUT_DIR/on/run_$run" \
        --voxel-brick-rejection "${COMMON_ARGS[@]}"
      "$BIN" render "$scene" --out "$OUT_DIR/off/run_$run" "${COMMON_ARGS[@]}"
    fi
  done
done

: > "$OUT_DIR/records.jsonl"
for scene in "${SCENES[@]}"; do
  output="$(jq -r .output "$scene")"
  "$BIN" compare "$OUT_DIR/off/run_0/$output" "$OUT_DIR/on/run_0/$output" \
    --report "$OUT_DIR/compare/${output%.png}.json"
  off_metadata=()
  on_metadata=()
  for ((run = 0; run < RUNS; run++)); do
    off_metadata+=("$OUT_DIR/off/run_$run/$output.render.json")
    on_metadata+=("$OUT_DIR/on/run_$run/$output.render.json")
  done
  on_file="$OUT_DIR/on-${output%.png}.json"
  jq -s . "${on_metadata[@]}" > "$on_file"
  jq -s --arg scene "$(basename "$scene" .json)" \
    --slurpfile on "$on_file" \
    --slurpfile parity "$OUT_DIR/compare/${output%.png}.json" '
      def median(values): values | sort | .[length / 2 | floor];
      . as $off | $on[0] as $on_runs |
      {
        scene:$scene,
        off:{build_ms:median($off|map(.voxel_build_ms)),first_frame_ms:median($off|map(.voxel_build_ms+.elapsed_ms)),active_cells:$off[0].voxel_active_cells,active_bricks:$off[0].voxel_active_bricks},
        on:{build_ms:median($on_runs|map(.voxel_build_ms)),first_frame_ms:median($on_runs|map(.voxel_build_ms+.elapsed_ms)),active_cells:$on_runs[0].voxel_active_cells,active_bricks:$on_runs[0].voxel_active_bricks,rejected_bricks:$on_runs[0].voxel_rejected_bricks},
        parity:$parity[0]
      }' "${off_metadata[@]}" >> "$OUT_DIR/records.jsonl"
done

jq -s '
  (map(.off.build_ms)|add) as $off_build |
  (map(.on.build_ms)|add) as $on_build |
  (map(.off.first_frame_ms)|add) as $off_first |
  (map(.on.first_frame_ms)|add) as $on_first |
  {
    scenes:.,
    aggregate:{
      rejected_bricks:(map(.on.rejected_bricks)|add),
      build_speedup:($off_build/$on_build),
      first_frame_speedup:($off_first/$on_first),
      exact_images:all(.parity.mean_absolute_error==0 and .parity.root_mean_square_error==0),
      exact_occupancy:all(.off.active_cells==.on.active_cells and .off.active_bricks==.on.active_bricks),
      promotion_gate:{minimum_speedup:1.10,no_scene_below:0.95},
      passes_promotion:(($off_build/$on_build)>=1.10 and all((.off.build_ms/.on.build_ms)>=0.95))
    }
  }' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

jq . "$OUT_DIR/summary.json"
printf 'brick rejection experiment: %s\n' "$OUT_DIR"
