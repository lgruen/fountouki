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

scenes=(picker phonics phonics-miss phonics-miss-igloo phonics-done patterns patterns-emoji patterns-unit patterns-hard patterns-levelup patterns-done tracing tracing-watch tracing-two-stroke tracing-grade tracing-done parent-patterns parent-phonics parent-tracing)
# label width height
resolutions=(
  "ipad-landscape 1194 834"
  "ipad-portrait 834 1194"
  "phone-landscape 844 390"
)

# Each capture is its own macroquad process that opens a window against the
# (headless, under CI) X display. That display connection occasionally flakes on
# a busy CI runner ("XOpenDisplay() failed!"), aborting an otherwise-fine render
# mid-matrix — so retry a few times before giving up. A genuinely broken scene
# still fails (all attempts panic); only the transient startup flake is absorbed.
capture() {
  local out=$1 scene=$2 w=$3 h=$4 attempt
  for attempt in 1 2 3; do
    if "$BIN" --capture "$out" "$scene" "$w" "$h" >/dev/null 2>&1; then
      return 0
    fi
    echo "  capture $scene ${w}x${h} attempt $attempt failed; retrying" >&2
    sleep 1
  done
  echo "FAILED: could not capture $scene at ${w}x${h} after 3 attempts" >&2
  return 1
}

for s in "${scenes[@]}"; do
  for r in "${resolutions[@]}"; do
    # shellcheck disable=SC2086
    set -- $r
    capture "$OUT/$s-$1.png" "$s" "$2" "$3"
  done
done

echo "wrote $(ls "$OUT" | wc -l | tr -d ' ') golden screenshots to $OUT"
