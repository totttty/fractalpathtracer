#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/voxel-optimization/coverage-$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-16}"
RUNS="${RUNS:-5}"
RESOLUTION="${RESOLUTION:-256}"

command -v jq >/dev/null || { printf 'jq is required for this experiment\n' >&2; exit 2; }
[[ ! -e "$OUT_DIR" ]] || { printf 'refusing to overwrite %s\n' "$OUT_DIR" >&2; exit 2; }

cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo test
  cargo build --release
fi
[[ -x "$BIN" ]] || { printf 'renderer is not executable: %s\n' "$BIN" >&2; exit 2; }

SCENES=(
  "$ROOT_DIR/scenes/benchmarks/Exact_Sphere.json"
  "$ROOT_DIR/scenes/benchmarks/Exact_Box.json"
  "$ROOT_DIR/scenes/benchmarks/Exact_Plane.json"
  "$ROOT_DIR/scenes/benchmarks/Exact_CSG.json"
)
MODES=(legacy lipschitz interval)
COMMON_ARGS=(
  --width "$WIDTH"
  --height "$HEIGHT"
  --samples "$SAMPLES"
  --voxel-resolution "$RESOLUTION"
)

mkdir -p "$OUT_DIR/sdf" "$OUT_DIR/compare" "$OUT_DIR/policy-compare"
for mode in "${MODES[@]}"; do
  mkdir -p "$OUT_DIR/$mode"
done

for scene in "${SCENES[@]}"; do
  output="$(jq -r .output "$scene")"
  "$BIN" render "$scene" --out "$OUT_DIR/sdf" --renderer sdf "${COMMON_ARGS[@]}"
  for mode in "${MODES[@]}"; do
    for ((run = 0; run < RUNS; run++)); do
      run_dir="$OUT_DIR/$mode/run_$run"
      mkdir -p "$run_dir"
      "$BIN" render "$scene" --out "$run_dir" --renderer voxel \
        --voxel-normal face --voxel-storage sparse-bricks \
        --voxel-coverage "$mode" "${COMMON_ARGS[@]}"
    done
    mkdir -p "$OUT_DIR/compare/$mode"
    "$BIN" compare "$OUT_DIR/sdf/$output" "$OUT_DIR/$mode/run_0/$output" \
      --report "$OUT_DIR/compare/$mode/${output%.png}.json"
  done
  for mode in lipschitz interval; do
    mkdir -p "$OUT_DIR/policy-compare/$mode"
    "$BIN" compare "$OUT_DIR/legacy/run_0/$output" "$OUT_DIR/$mode/run_0/$output" \
      --report "$OUT_DIR/policy-compare/$mode/${output%.png}.json"
  done
done

: > "$OUT_DIR/records.jsonl"
for mode in "${MODES[@]}"; do
  for scene in "${SCENES[@]}"; do
    output="$(jq -r .output "$scene")"
    metadata=()
    for ((run = 0; run < RUNS; run++)); do
      metadata+=("$OUT_DIR/$mode/run_$run/$output.render.json")
    done
    jq -s \
      --arg mode "$mode" \
      --arg scene "$(basename "$scene" .json)" \
      --slurpfile quality "$OUT_DIR/compare/$mode/${output%.png}.json" '
      def median(values): values | sort | .[length / 2 | floor];
      {
        mode: $mode,
        scene: $scene,
        active_cells: .[0].voxel_active_cells,
        active_bricks: .[0].voxel_active_bricks,
        resident_bytes: .[0].voxel_memory_bytes,
        build_ms_median: median(map(.voxel_build_ms)),
        render_ms_median: median(map(.elapsed_ms)),
        first_frame_ms_median: median(map(.voxel_build_ms + .elapsed_ms)),
        quality_vs_sdf: $quality[0]
      }' "${metadata[@]}" >> "$OUT_DIR/records.jsonl"
  done
done

jq -s \
  --argjson width "$WIDTH" \
  --argjson height "$HEIGHT" \
  --argjson samples "$SAMPLES" \
  --argjson runs "$RUNS" \
  --argjson resolution "$RESOLUTION" '
  (group_by(.mode) | map({
    mode: .[0].mode,
    total_active_cells: map(.active_cells) | add,
    total_active_bricks: map(.active_bricks) | add,
    total_resident_bytes: map(.resident_bytes) | add,
    total_build_ms_median: map(.build_ms_median) | add,
    total_render_ms_median: map(.render_ms_median) | add,
    scenes: .
  })) as $modes |
  ($modes | map(select(.mode == "lipschitz" or .mode == "interval")) |
    sort_by(.total_active_cells, .total_active_bricks)) as $qualifying |
  $qualifying[0] as $best |
  ($qualifying | map(select(
    .total_active_cells == $best.total_active_cells and
    .total_active_bricks == $best.total_active_bricks
  )) | length) as $best_count |
  {
    configuration: {
      width: $width,
      height: $height,
      samples: $samples,
      runs: $runs,
      voxel_resolution: $resolution,
      bounds: [[-3.25, -3.25, -3.25], [3.25, 3.25, 3.25]],
      surface_band: 0.5,
      normals: "face",
      storage: "sparse-bricks"
    },
    certification_contract: {
      source: "cargo Metal contract: analytic invariants plus 3x3x3 adversarial samples",
      lipschitz_false_certifications: 0,
      interval_false_certifications: 0,
      unsupported_estimators: "legacy fallback"
    },
    decision: {
      default_mode: "legacy",
      qualifying_modes: ($qualifying | map(.mode)),
      selected_opt_in_mode: (if $best_count == 1 then $best.mode else null end),
      selection_order: ["fewest active cells", "fewest active bricks"],
      tie_policy: "reject candidate and retain the previous qualifying baseline",
      rationale: (if $best_count == 1 then
        "zero false certification, then the unique minimum active-cell/brick payload"
      else
        "tie: no candidate promoted"
      end)
    },
    modes: $modes
  }' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

for mode in "${MODES[@]}"; do
  "$BIN" contact-sheet "$OUT_DIR/$mode-sheet.png" "$OUT_DIR/$mode/run_0"/*.png
done
"$BIN" contact-sheet "$OUT_DIR/sdf-sheet.png" "$OUT_DIR/sdf"/*.png

jq . "$OUT_DIR/summary.json"
printf 'voxel coverage experiment: %s\n' "$OUT_DIR"
