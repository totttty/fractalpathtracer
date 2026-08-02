#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/reports/bound-grid-interval-jacobian/$RUN_STAMP}"
BIN="${BIN:-$ROOT_DIR/target/release/fpt-metal}"
RUNS="${RUNS:-5}"
WIDTH="${WIDTH:-320}"
HEIGHT="${HEIGHT:-180}"
SAMPLES="${SAMPLES:-16}"
BOUNCE_CAP="${BOUNCE_CAP:-1}"
PROFILE_STRIDE="${PROFILE_STRIDE:-4}"
read -r -a COUNTS <<< "${COUNTS_STRING:-1 8 16 32 48 60}"
FAMILIES=(abs repeat)

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

mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/sdf" "$OUT_DIR/range32" \
  "$OUT_DIR/directional32" "$OUT_DIR/compare"
for family in "${FAMILIES[@]}"; do
  for count in "${COUNTS[@]}"; do
    if [[ ! "$count" =~ ^[0-9]+$ ]] || ((count < 1 || count > 60)); then
      printf 'primitive counts must be integers in 1..60: %s\n' "$count" >&2
      exit 2
    fi
    output="Interval_Jacobian_${family}_${count}.png"
    jq --arg family "$family" --argjson count "$count" --arg output "$output" \
      --arg preset "$ROOT_DIR/scenes/readme/presets/Cage_Fractal.fpt" \
      '
        .output = $output |
        .preset = $preset |
        .camera.dof = 0 |
        .camera.focus_distance = 4.2 |
        .camera.position = [0.0,0.0,-4.2] |
        .camera.fov = 58 |
        .sdf_program.operations =
          if $family == "abs" then
            ([{op:"rotate_y", angle:0.32},
              {op:"abs"},
              {op:"translate", value:[0.65,0.35,0.2]}] +
             [range(0; $count) | {op:"sphere", radius:0.42}])
          else
            ([{op:"rotate_y", angle:0.18},
              {op:"repeat", value:[1.6,1.6,1.6]}] +
             [range(0; $count) | {op:"sphere", radius:0.32}])
          end
      ' "$ROOT_DIR/scenes/benchmarks/Exact_Sphere.json" \
      > "$OUT_DIR/fixtures/${family}_${count}.json"
  done
done

COMMON_ARGS=(
  --width "$WIDTH" --height "$HEIGHT" --samples "$SAMPLES"
  --sdf-bounce-cap "$BOUNCE_CAP"
)

render_variant() {
  local variant="$1"
  local fixture="$2"
  local output="$3"
  case "$variant" in
    sdf)
      "$BIN" render "$fixture" --out "$output" --renderer sdf "${COMMON_ARGS[@]}"
      ;;
    range32)
      "$BIN" render "$fixture" --out "$output" --renderer bound-grid \
        --bound-grid-resolution 32 "${COMMON_ARGS[@]}"
      ;;
    directional32)
      "$BIN" render "$fixture" --out "$output" --renderer bound-grid \
        --bound-grid-resolution 32 --bound-grid-directional --bound-grid-profile \
        --bound-grid-profile-stride "$PROFILE_STRIDE" "${COMMON_ARGS[@]}"
      ;;
  esac
}

for ((run = 0; run < RUNS; run++)); do
  for variant in sdf range32 directional32; do
    mkdir -p "$OUT_DIR/$variant/run_$run"
  done
  for family in "${FAMILIES[@]}"; do
    for count in "${COUNTS[@]}"; do
      fixture="$OUT_DIR/fixtures/${family}_${count}.json"
      if ((run % 2 == 0)); then
        order=(sdf range32 directional32)
      else
        order=(directional32 range32 sdf)
      fi
      for variant in "${order[@]}"; do
        render_variant "$variant" "$fixture" "$OUT_DIR/$variant/run_$run"
      done
    done
  done
done

: > "$OUT_DIR/records.jsonl"
for family in "${FAMILIES[@]}"; do
  for count in "${COUNTS[@]}"; do
    output="Interval_Jacobian_${family}_${count}.png"
    for variant in range32 directional32; do
      "$BIN" compare "$OUT_DIR/sdf/run_0/$output" \
        "$OUT_DIR/$variant/run_0/$output" \
        --report "$OUT_DIR/compare/${family}_${count}-${variant}.json"
    done
    jq -n --arg family "$family" --argjson primitive_count "$count" \
      --slurpfile sdf <(jq -s . "$OUT_DIR"/sdf/run_*/"$output.render.json") \
      --slurpfile range32 <(jq -s . "$OUT_DIR"/range32/run_*/"$output.render.json") \
      --slurpfile d32 <(jq -s . "$OUT_DIR"/directional32/run_*/"$output.render.json") \
      --slurpfile qr "$OUT_DIR/compare/${family}_${count}-range32.json" \
      --slurpfile qd "$OUT_DIR/compare/${family}_${count}-directional32.json" '
        def median(values): values | sort | .[length / 2 | floor];
        {family:$family,primitive_count:$primitive_count,
         instruction_count:($primitive_count + (if $family == "abs" then 3 else 2 end)),
         sdf:{render_ms:median($sdf[0]|map(.elapsed_ms))},
         range32:{render_ms:median($range32[0]|map(.elapsed_ms)),quality:$qr[0]},
         directional32:{render_ms:median($d32[0]|map(.elapsed_ms)),
                        build_ms:median($d32[0]|map(.bound_grid_build_ms)),
                        quality:$qd[0],profile:$d32[0][0].bound_grid_profile}}' \
      >> "$OUT_DIR/records.jsonl"
  done
done

jq -s --argjson width "$WIDTH" --argjson height "$HEIGHT" \
  --argjson samples "$SAMPLES" --argjson runs "$RUNS" '
    map(. + {sdf_over_directional32:(.sdf.render_ms/.directional32.render_ms),
             range32_over_directional32:(.range32.render_ms/.directional32.render_ms),
             derivative_certified_fraction:
               (.directional32.profile.certified_derivative_cells /
                (.directional32.profile.certified_derivative_cells +
                 .directional32.profile.unknown_derivative_cells))}) as $rows |
    {settings:{width:$width,height:$height,samples:$samples,runs:$runs},
     crossover:{abs:([$rows[]|select(.family=="abs" and
                         .directional32.render_ms + .directional32.build_ms < .sdf.render_ms)|
                         .instruction_count]|first),
                repeat:([$rows[]|select(.family=="repeat" and
                            .directional32.render_ms + .directional32.build_ms < .sdf.render_ms)|
                            .instruction_count]|first)},
     rows:$rows,
     validation:{maximum_mae:($rows|map(.directional32.quality.mean_absolute_error)|max),
                 minimum_lf_ssim:($rows|map(.directional32.quality.low_frequency_luminance_ssim)|min),
                 sampled_bound_failures:($rows|map(.directional32.profile.sampled_bound_failures)|add),
                 sampled_derivative_failures:($rows|map(.directional32.profile.sampled_derivative_failures)|add)}}' \
  "$OUT_DIR/records.jsonl" > "$OUT_DIR/summary.json"

jq . "$OUT_DIR/summary.json"
printf 'Interval-Jacobian experiment: %s\n' "$OUT_DIR"
