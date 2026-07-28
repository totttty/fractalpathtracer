#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

"$ROOT_DIR/scripts/run_beauty_parity.sh"
"$ROOT_DIR/scripts/run_fractal_parity.sh"

"$ROOT_DIR/target/release/fpt-metal" check-parity-reports \
  "$ROOT_DIR/reports/beauty-parity" \
  "$ROOT_DIR/reports/fractal-parity"
