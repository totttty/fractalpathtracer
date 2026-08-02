#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/stitch-cache/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
WIDTH="${WIDTH:-160}"
HEIGHT="${HEIGHT:-90}"
SAMPLES="${SAMPLES:-8}"

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

make_fixture() {
  local primitive_count="$1" output="$2" destination="$3"
  jq --argjson primitive_count "$primitive_count" \
    --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
    --arg output "$output" '
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
    ' "$ROOT_DIR/scenes/benchmarks/Exact_Sphere.json" > "$destination"
}

make_fixture 13 Cache_Base.png "$OUT_DIR/fixtures/base.json"
make_fixture 14 Cache_Topology_Edit.png "$OUT_DIR/fixtures/topology-edit.json"
jq '
  .output = "Cache_Numeric_Edit.png" |
  .sdf_program.operations |= map(
    if .op == "sphere" then .radius *= 1.08
    elif .op == "translate" then .value[1] += 0.015
    else . end)
' "$OUT_DIR/fixtures/base.json" > "$OUT_DIR/fixtures/numeric-edit.json"

COMMON_ARGS=(--renderer sdf --sdf-program-optimization basic
  --sdf-function-stitching inline --sdf-bounce-cap 1
  --width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES")
export FPT_STITCH_CACHE_DIR="$OUT_DIR/cache"

for name in base numeric-edit topology-edit base-reuse; do
  fixture="$OUT_DIR/fixtures/$name.json"
  if [[ "$name" == "base-reuse" ]]; then fixture="$OUT_DIR/fixtures/base.json"; fi
  mkdir -p "$OUT_DIR/$name"
  "$BIN" render "$fixture" --out "$OUT_DIR/$name" "${COMMON_ARGS[@]}"
done

for name in base numeric-edit topology-edit; do
  mkdir -p "$OUT_DIR/bytecode-$name"
  "$BIN" render "$OUT_DIR/fixtures/$name.json" --out "$OUT_DIR/bytecode-$name" \
    --renderer sdf --sdf-program-optimization basic --sdf-bounce-cap 1 \
    --width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
  output="$(jq -r .output "$OUT_DIR/fixtures/$name.json")"
  "$BIN" compare "$OUT_DIR/bytecode-$name/$output" "$OUT_DIR/$name/$output" \
    --report "$OUT_DIR/compare/$name.json"
done

base_output="$(jq -r .output "$OUT_DIR/fixtures/base.json")"
numeric_output="$(jq -r .output "$OUT_DIR/fixtures/numeric-edit.json")"
topology_output="$(jq -r .output "$OUT_DIR/fixtures/topology-edit.json")"
jq -n \
  --argjson width "$WIDTH" --argjson height "$HEIGHT" --argjson samples "$SAMPLES" \
  --slurpfile base "$OUT_DIR/base/$base_output.render.json" \
  --slurpfile numeric "$OUT_DIR/numeric-edit/$numeric_output.render.json" \
  --slurpfile topology "$OUT_DIR/topology-edit/$topology_output.render.json" \
  --slurpfile reuse "$OUT_DIR/base-reuse/$base_output.render.json" \
  --slurpfile qb "$OUT_DIR/compare/base.json" \
  --slurpfile qn "$OUT_DIR/compare/numeric-edit.json" \
  --slurpfile qt "$OUT_DIR/compare/topology-edit.json" '
    {settings:{width:$width,height:$height,samples:$samples},
     base:{cache_status:$base[0].sdf_stitch_cache_status,
       cache_key:$base[0].sdf_stitch_cache_key,
       build_ms:$base[0].sdf_stitched_library_build_ms,quality:$qb[0]},
     numeric_edit:{cache_status:$numeric[0].sdf_stitch_cache_status,
       cache_key:$numeric[0].sdf_stitch_cache_key,
       build_ms:$numeric[0].sdf_stitched_library_build_ms,quality:$qn[0]},
     topology_edit:{cache_status:$topology[0].sdf_stitch_cache_status,
       cache_key:$topology[0].sdf_stitch_cache_key,
       build_ms:$topology[0].sdf_stitched_library_build_ms,quality:$qt[0]},
     base_reuse:{cache_status:$reuse[0].sdf_stitch_cache_status,
       cache_key:$reuse[0].sdf_stitch_cache_key,
       build_ms:$reuse[0].sdf_stitched_library_build_ms},
     invariants:{numeric_edit_reuses_topology_key:
       ($base[0].sdf_stitch_cache_key == $numeric[0].sdf_stitch_cache_key),
       topology_edit_changes_key:
       ($base[0].sdf_stitch_cache_key != $topology[0].sdf_stitch_cache_key),
       maximum_mae:([$qb[0].mean_absolute_error,$qn[0].mean_absolute_error,
         $qt[0].mean_absolute_error]|max),
       minimum_lf_ssim:([$qb[0].low_frequency_luminance_ssim,
         $qn[0].low_frequency_luminance_ssim,
         $qt[0].low_frequency_luminance_ssim]|min)}}
  ' > "$OUT_DIR/summary.json"

jq . "$OUT_DIR/summary.json"
printf 'Stitched pipeline cache experiment: %s\n' "$OUT_DIR"
