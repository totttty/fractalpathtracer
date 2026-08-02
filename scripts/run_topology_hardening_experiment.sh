#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/topology-hardening/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-3}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-64}"
SCENES=(sphere box plane subtraction intersection repeat_orbit mixed_transform)

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
mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/bytecode" "$OUT_DIR/topology" \
  "$OUT_DIR/compare"

jq --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
  '.preset=$preset | .output="Topology_Sphere.png"' scenes/benchmarks/Exact_Sphere.json \
  > "$OUT_DIR/fixtures/sphere.json"
jq --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
  '.preset=$preset | .output="Topology_Box.png"' scenes/benchmarks/Exact_Box.json \
  > "$OUT_DIR/fixtures/box.json"
jq --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
  '.preset=$preset | .output="Topology_Plane.png"' scenes/benchmarks/Exact_Plane.json \
  > "$OUT_DIR/fixtures/plane.json"
jq --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
  '.preset=$preset | .output="Topology_Subtraction.png"' scenes/benchmarks/Exact_CSG.json \
  > "$OUT_DIR/fixtures/subtraction.json"

jq --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" '
  .preset=$preset |
  .output="Topology_Intersection.png" |
  .sdf_program.operations=[
    {op:"sphere",radius:1.05},
    {op:"translate",value:[0.55,0.0,0.0]},
    {op:"box",value:[0.85,0.7,0.65],combine:"intersection"}
  ]
' scenes/benchmarks/Exact_Sphere.json > "$OUT_DIR/fixtures/intersection.json"

jq --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" '
  .preset=$preset |
  .output="Topology_Repeat_Orbit.png" |
  .sdf_program.operations=[
    {op:"repeat",value:[2.4,2.4,2.4]},
    {op:"translate",value:[-0.52,0.0,0.0]},
    {op:"sphere",radius:0.58,orbit_weight:0.18},
    {op:"orbit_add",value:[0.7,0.4,0.9],orbit_weight:0.05},
    {op:"translate",value:[1.04,0.0,0.0]},
    {op:"box",value:[0.42,0.48,0.36],orbit_weight:0.22}
  ] |
  .sdf_program.material.mode="gradient" |
  .sdf_program.gradient=[
    {position:0,color:[0.04,0.18,0.72]},
    {position:0.5,color:[0.10,0.82,0.66]},
    {position:1,color:[0.94,0.24,0.08]}
  ]
' scenes/benchmarks/Exact_Sphere.json > "$OUT_DIR/fixtures/repeat_orbit.json"

jq --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" '
  .preset=$preset |
  .output="Topology_Mixed_Transform.png" |
  .sdf_program.operations=[
    {op:"rotate_y",angle:0.31},
    {op:"rotate_x",angle:-0.19},
    {op:"abs"},
    {op:"sort_desc"},
    {op:"scale",value:1.17},
    {op:"translate",value:[-0.65,0.2,0.0]},
    {op:"box",value:[0.72,0.54,0.61]},
    {op:"translate",value:[1.3,-0.4,0.0]},
    {op:"sphere",radius:0.66}
  ]
' scenes/benchmarks/Exact_Sphere.json > "$OUT_DIR/fixtures/mixed_transform.json"

COMMON_ARGS=(--width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
  --sdf-bounce-cap 1 --sdf-program-optimization basic --renderer sdf)

for ((run = 0; run < RUNS; run++)); do
  mkdir -p "$OUT_DIR/bytecode/run_$run" "$OUT_DIR/topology/run_$run"
  for scene in "${SCENES[@]}"; do
    fixture="$OUT_DIR/fixtures/$scene.json"
    if ((run % 2 == 0)); then variants=(bytecode topology)
    else variants=(topology bytecode); fi
    for variant in "${variants[@]}"; do
      extra=()
      [[ "$variant" == topology ]] && extra=(--sdf-topology-specialization)
      "$BIN" render "$fixture" --out "$OUT_DIR/$variant/run_$run" \
        "${COMMON_ARGS[@]}" "${extra[@]}"
    done
  done
done

: > "$OUT_DIR/records.jsonl"
for scene in "${SCENES[@]}"; do
  output="$(jq -r .output "$OUT_DIR/fixtures/$scene.json")"
  "$BIN" compare "$OUT_DIR/bytecode/run_0/$output" \
    "$OUT_DIR/topology/run_0/$output" --report "$OUT_DIR/compare/$scene.json"
  jq -n --arg scene "$scene" \
    --slurpfile b <(jq -s . "$OUT_DIR"/bytecode/run_*/"$output.render.json") \
    --slurpfile t <(jq -s . "$OUT_DIR"/topology/run_*/"$output.render.json") \
    --slurpfile quality "$OUT_DIR/compare/$scene.json" '
      def median(values): values | sort | .[length / 2 | floor];
      {scene:$scene,
       geometry_instruction_count:$t[0][0].sdf_program_instruction_count,
       shading_instruction_count:$t[0][0].sdf_shading_program_instruction_count,
       bytecode_ms:median($b[0]|map(.elapsed_ms)),
       topology_ms:median($t[0]|map(.elapsed_ms)),
       cold_build_ms:$t[0][0].sdf_topology_build_ms,
       warm_build_ms:median($t[0][1:]|map(.sdf_topology_build_ms)),
       quality:$quality[0]} |
      . + {render_speedup:(.bytecode_ms/.topology_ms)}
    ' >> "$OUT_DIR/records.jsonl"
done

jq -s --argjson runs "$RUNS" --argjson samples "$SAMPLES" '
  {settings:{runs:$runs,samples:$samples},rows:.,validation:{
    maximum_mae:([.[].quality.mean_absolute_error]|max),
    minimum_lf_ssim:([.[].quality.low_frequency_luminance_ssim]|min)}}
' "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"
jq . "$OUT_DIR/summary.json"
printf 'Topology hardening experiment: %s\n' "$OUT_DIR"
