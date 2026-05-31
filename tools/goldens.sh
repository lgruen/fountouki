#!/usr/bin/env bash
# Golden screenshot matrix: capture every scene at several device resolutions /
# aspect ratios into screenshots/golden/. The SAME macroquad renderer that ships
# native produces these (offscreen render-target → PNG), so they reflect the
# device, not an emulator. Eyeball them; CI uploads them as artifacts.
#
# Local (macOS): real GL. CI (Linux): run under `xvfb-run` with software GL.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/screenshots/golden"
BIN="$ROOT/target/debug/fountouki"

mkdir -p "$OUT"
( cd "$ROOT" && cargo build -q -p fountouki )

scenes=(picker phonics phonics-miss phonics-done patterns patterns-emoji patterns-unit parent-patterns parent-phonics)
# label width height
resolutions=(
  "ipad-landscape 1194 834"
  "ipad-portrait 834 1194"
  "phone-landscape 844 390"
)

for s in "${scenes[@]}"; do
  for r in "${resolutions[@]}"; do
    # shellcheck disable=SC2086
    set -- $r
    "$BIN" --capture "$OUT/$s-$1.png" "$s" "$2" "$3" >/dev/null
  done
done

echo "wrote $(ls "$OUT" | wc -l | tr -d ' ') golden screenshots to $OUT"
