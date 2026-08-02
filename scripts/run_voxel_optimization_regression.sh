#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/voxel-optimization/$RUN_STAMP}"
BASELINE_BIN="${BASELINE_BIN:?set BASELINE_BIN to a frozen fpt-metal executable}"
CANDIDATE_BIN="${CANDIDATE_BIN:-$ROOT_DIR/target/release/fpt-metal}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-16}"
RUNS="${RUNS:-5}"
VOXEL_RESOLUTION="${VOXEL_RESOLUTION:-256}"
VOXEL_STORAGE="${VOXEL_STORAGE:-sparse-bricks}"
REQUIRE_IDENTICAL="${REQUIRE_IDENTICAL:-1}"

command -v jq >/dev/null || { printf 'jq is required for this harness\n' >&2; exit 2; }
[[ -x "$BASELINE_BIN" ]] || { printf 'baseline is not executable: %s\n' "$BASELINE_BIN" >&2; exit 2; }
if [[ -e "$OUT_DIR" ]]; then
  printf 'refusing to overwrite existing report directory: %s\n' "$OUT_DIR" >&2
  exit 2
fi

cd "$ROOT_DIR"
if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo build --release
fi
[[ -x "$CANDIDATE_BIN" ]] || { printf 'candidate is not executable: %s\n' "$CANDIDATE_BIN" >&2; exit 2; }

SCENES=(
  "$ROOT_DIR/scenes/readme/01-Render005.json"
  "$ROOT_DIR/scenes/readme/08-Render0ad03.json"
  "$ROOT_DIR/scenes/readme/09-Glass.json"
)
for scene in "${SCENES[@]}"; do
  jq -e '.voxel.bounds_min == [-3.25, -3.25, -3.25] and
         .voxel.bounds_max == [3.25, 3.25, 3.25] and
         .voxel.surface_band == 0.5' "$scene" >/dev/null || {
    printf 'scene does not match the fixed voxel bounds/band contract: %s\n' "$scene" >&2
    exit 2
  }
done
COMMON_ARGS=(
  --renderer voxel
  --voxel-resolution "$VOXEL_RESOLUTION"
  --voxel-normal face
  --voxel-storage "$VOXEL_STORAGE"
  --width "$WIDTH"
  --height "$HEIGHT"
  --samples "$SAMPLES"
)

mkdir -p "$OUT_DIR/baseline" "$OUT_DIR/candidate" "$OUT_DIR/compare"
jq -n \
  --arg baseline "$BASELINE_BIN" \
  --arg candidate "$CANDIDATE_BIN" \
  --arg baseline_sha "$(shasum -a 256 "$BASELINE_BIN" | awk '{print $1}')" \
  --arg candidate_sha "$(shasum -a 256 "$CANDIDATE_BIN" | awk '{print $1}')" \
  --arg commit "$(git rev-parse HEAD)" \
  --arg branch "$(git branch --show-current)" \
  --argjson width "$WIDTH" \
  --argjson height "$HEIGHT" \
  --argjson samples "$SAMPLES" \
  --argjson runs "$RUNS" \
  --argjson resolution "$VOXEL_RESOLUTION" \
  --arg storage "$VOXEL_STORAGE" \
  '{baseline_binary:$baseline,candidate_binary:$candidate,baseline_sha256:$baseline_sha,
    candidate_sha256:$candidate_sha,commit:$commit,branch:$branch,width:$width,height:$height,
    samples:$samples,runs:$runs,voxel:{resolution:$resolution,bounds_min:[-3.25,-3.25,-3.25],
    bounds_max:[3.25,3.25,3.25],surface_band:0.5,normals:"face",storage:$storage}}' \
  > "$OUT_DIR/environment.json"

render_scene() {
  local binary="$1"
  local scene="$2"
  local output="$3"
  "$binary" render "$scene" --out "$output" "${COMMON_ARGS[@]}"
}

for ((run = 0; run < RUNS; run++)); do
  baseline_run="$OUT_DIR/baseline/run_$run"
  candidate_run="$OUT_DIR/candidate/run_$run"
  mkdir -p "$baseline_run" "$candidate_run"
  for scene in "${SCENES[@]}"; do
    if ((run % 2 == 0)); then
      render_scene "$BASELINE_BIN" "$scene" "$baseline_run"
      render_scene "$CANDIDATE_BIN" "$scene" "$candidate_run"
    else
      render_scene "$CANDIDATE_BIN" "$scene" "$candidate_run"
      render_scene "$BASELINE_BIN" "$scene" "$baseline_run"
    fi
  done
done

for baseline_image in "$OUT_DIR"/baseline/run_0/*.png; do
  name="$(basename "$baseline_image")"
  "$CANDIDATE_BIN" compare \
    "$baseline_image" \
    "$OUT_DIR/candidate/run_0/$name" \
    --report "$OUT_DIR/compare/${name%.png}.json"
done

"$CANDIDATE_BIN" contact-sheet "$OUT_DIR/baseline_sheet.png" "$OUT_DIR"/baseline/run_0/*.png
"$CANDIDATE_BIN" contact-sheet "$OUT_DIR/candidate_sheet.png" "$OUT_DIR"/candidate/run_0/*.png
"$CANDIDATE_BIN" contact-sheet "$OUT_DIR/comparison_sheet.png" "$OUT_DIR"/compare/*.comparison.png

if [[ "$REQUIRE_IDENTICAL" == "1" ]]; then
  "$CANDIDATE_BIN" optimization-summary "$OUT_DIR" 0 0 1 1 "$RUNS" "${#SCENES[@]}"
else
  "$CANDIDATE_BIN" optimization-summary "$OUT_DIR" 1 3 0.995 0.995 "$RUNS" "${#SCENES[@]}"
fi

printf 'voxel optimization report: %s\n' "$OUT_DIR"
