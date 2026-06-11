#!/usr/bin/env bash
# Visual regression gate: compare the freshly rendered screenshots/golden/
# matrix against the committed baselines in tests/golden-baseline/.
#
# Uses ImageMagick `compare -metric AE -fuzz $FUZZ` (per-channel tolerance
# absorbs software-GL/AA jitter between mesa versions); an image fails when
# more than $MAX_DIFF_FRAC of its pixels differ. Missing or extra scenes fail
# the set, so adding a scene means adding its baseline.
#
# Intentional visual change? Refresh the baselines in the same PR:
#   bash tools/goldens.sh && cp screenshots/golden/*.png tests/golden-baseline/
# (CI renders under xvfb + LIBGL_ALWAYS_SOFTWARE=1; regenerate baselines in
# that environment — or copy them from the CI "screenshots" artifact — so the
# committed pixels match what CI produces.)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FRESH="$ROOT/screenshots/golden"
BASE="$ROOT/tests/golden-baseline"
DIFF_OUT="$ROOT/screenshots/golden-diff"
FUZZ="${FUZZ:-3%}"
MAX_DIFF_FRAC="${MAX_DIFF_FRAC:-0.002}" # ≤0.2% of pixels may differ

if ! command -v compare >/dev/null; then
  echo "ImageMagick 'compare' not found" >&2
  exit 2
fi
[ -d "$BASE" ] || { echo "no baselines at $BASE" >&2; exit 2; }
mkdir -p "$DIFF_OUT"

fails=0
checked=0

# Every baseline must have a fresh render, and vice versa.
for b in "$BASE"/*.png; do
  name="$(basename "$b")"
  if [ ! -f "$FRESH/$name" ]; then
    echo "FAIL $name: baseline exists but no fresh render (scene removed?)"
    fails=$((fails + 1))
  fi
done
for f in "$FRESH"/*.png; do
  name="$(basename "$f")"
  b="$BASE/$name"
  if [ ! -f "$b" ]; then
    echo "FAIL $name: new scene without a committed baseline"
    fails=$((fails + 1))
    continue
  fi
  checked=$((checked + 1))
  # `compare` exits 1 on any difference; the AE pixel count goes to stderr.
  ae=$(compare -metric AE -fuzz "$FUZZ" "$b" "$f" "$DIFF_OUT/$name" 2>&1 || true)
  if ! [[ "$ae" =~ ^[0-9.e+]+$ ]]; then
    echo "FAIL $name: compare error: $ae"
    fails=$((fails + 1))
    continue
  fi
  dims=$(identify -format "%w %h" "$f")
  total=$(( ${dims% *} * ${dims#* } ))
  # Integer-compare AE (may print scientific notation) via awk.
  if awk -v ae="$ae" -v tot="$total" -v frac="$MAX_DIFF_FRAC" \
      'BEGIN { exit !(ae + 0 > tot * frac) }'; then
    echo "FAIL $name: $ae of $total px differ (> ${MAX_DIFF_FRAC} frac)"
    fails=$((fails + 1))
  else
    rm -f "$DIFF_OUT/$name"
  fi
done

echo "golden-diff: $checked compared, $fails failure(s)"
if [ "$fails" -gt 0 ]; then
  echo "diff images (red = changed pixels): $DIFF_OUT"
  exit 1
fi
