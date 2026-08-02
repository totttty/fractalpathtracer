#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/voxel-optimization/direct-build-$RUN_STAMP}"
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
  "$ROOT_DIR/scenes/readme/01-Render005.json"
  "$ROOT_DIR/scenes/readme/08-Render0ad03.json"
  "$ROOT_DIR/scenes/readme/09-Glass.json"
)
COMMON_ARGS=(
  --renderer voxel
  --voxel-resolution 256
  --voxel-normal face
  --voxel-storage sparse-bricks
  --width 320
  --height 180
  --samples 16
)

mkdir -p "$OUT_DIR/staging" "$OUT_DIR/direct" "$OUT_DIR/compare"
for ((run = 0; run < RUNS; run++)); do
  mkdir -p "$OUT_DIR/staging/run_$run" "$OUT_DIR/direct/run_$run"
  for scene in "${SCENES[@]}"; do
    if ((run % 2 == 0)); then
      "$BIN" render "$scene" --out "$OUT_DIR/staging/run_$run" \
        --voxel-build staging "${COMMON_ARGS[@]}"
      "$BIN" render "$scene" --out "$OUT_DIR/direct/run_$run" \
        --voxel-build direct "${COMMON_ARGS[@]}"
    else
      "$BIN" render "$scene" --out "$OUT_DIR/direct/run_$run" \
        --voxel-build direct "${COMMON_ARGS[@]}"
      "$BIN" render "$scene" --out "$OUT_DIR/staging/run_$run" \
        --voxel-build staging "${COMMON_ARGS[@]}"
    fi
  done
done

: > "$OUT_DIR/records.jsonl"
for scene in "${SCENES[@]}"; do
  output="$(jq -r .output "$scene")"
  "$BIN" compare "$OUT_DIR/staging/run_0/$output" "$OUT_DIR/direct/run_0/$output" \
    --report "$OUT_DIR/compare/${output%.png}.json"
  staging_metadata=()
  direct_metadata=()
  for ((run = 0; run < RUNS; run++)); do
    staging_metadata+=("$OUT_DIR/staging/run_$run/$output.render.json")
    direct_metadata+=("$OUT_DIR/direct/run_$run/$output.render.json")
  done
  direct_runs_file="$OUT_DIR/direct-${output%.png}.json"
  jq -s . "${direct_metadata[@]}" > "$direct_runs_file"
  jq -s \
    --arg scene "$(basename "$scene" .json)" \
    --slurpfile direct "$direct_runs_file" \
    --slurpfile quality "$OUT_DIR/compare/${output%.png}.json" '
      def median(values): values | sort | .[length / 2 | floor];
      . as $staging |
      $direct[0] as $direct_runs |
      {
        scene: $scene,
        staging: {
          active_cells: $staging[0].voxel_active_cells,
          active_bricks: $staging[0].voxel_active_bricks,
          resident_bytes: $staging[0].voxel_memory_bytes,
          build_ms: median($staging | map(.voxel_build_ms)),
          render_ms: median($staging | map(.elapsed_ms)),
          first_frame_ms: median($staging | map(.voxel_build_ms + .elapsed_ms))
        },
        direct: {
          active_cells: $direct_runs[0].voxel_active_cells,
          active_bricks: $direct_runs[0].voxel_active_bricks,
          resident_bytes: $direct_runs[0].voxel_memory_bytes,
          build_ms: median($direct_runs | map(.voxel_build_ms)),
          render_ms: median($direct_runs | map(.elapsed_ms)),
          first_frame_ms: median($direct_runs | map(.voxel_build_ms + .elapsed_ms))
        },
        parity: $quality[0]
      }' "${staging_metadata[@]}" >> "$OUT_DIR/records.jsonl"
done

mkdir -p "$OUT_DIR/resolution-512"
"$BIN" render "$ROOT_DIR/scenes/readme/09-Glass.json" \
  --out "$OUT_DIR/resolution-512" \
  --renderer voxel --voxel-resolution 512 --voxel-normal face \
  --voxel-storage sparse-bricks --voxel-build direct \
  --width 80 --height 45 --samples 1

jq -s \
  --slurpfile memory512 "$OUT_DIR/resolution-512/09-Glass-metal.png.render.json" '
  {
    configuration: {width:320,height:180,samples:16,runs:5,resolution:256},
    parity_contract: "cargo GPU test compares canonical staging/direct VoxelCell bytes",
    scenes: .,
    aggregate: {
      staging_build_ms: map(.staging.build_ms) | add,
      direct_build_ms: map(.direct.build_ms) | add,
      staging_first_frame_ms: map(.staging.first_frame_ms) | add,
      direct_first_frame_ms: map(.direct.first_frame_ms) | add,
      build_speedup: ((map(.staging.build_ms) | add) / (map(.direct.build_ms) | add)),
      first_frame_speedup: ((map(.staging.first_frame_ms) | add) / (map(.direct.first_frame_ms) | add)),
      exact_image_parity: all(.parity.mean_absolute_error == 0 and .parity.root_mean_square_error == 0),
      exact_occupancy_totals: all(
        .staging.active_cells == .direct.active_cells and
        .staging.active_bricks == .direct.active_bricks
      )
    },
    resolution_512: {
      resident_bytes: $memory512[0].voxel_memory_bytes,
      resident_mib: ($memory512[0].voxel_memory_bytes / 1048576),
      build_ms: $memory512[0].voxel_build_ms,
      active_cells: $memory512[0].voxel_active_cells,
      active_bricks: $memory512[0].voxel_active_bricks,
      cap_mib: 512,
      passes_cap: ($memory512[0].voxel_memory_bytes <= 512 * 1048576)
    }
  }' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

jq . "$OUT_DIR/summary.json"
printf 'direct voxel build experiment: %s\n' "$OUT_DIR"
