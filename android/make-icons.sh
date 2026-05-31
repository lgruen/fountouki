#!/usr/bin/env bash
# Generate Android launcher-icon mipmaps from the existing 512px PNG.
#
# cargo-quad-apk does NOT auto-generate density buckets — it just copies the
# `res` folder you point `icon = "@mipmap/ic_launcher"` at. This script builds
# that folder once: android/res/mipmap-<density>/ic_launcher.png
#
# Needs ImageMagick ('magick' or legacy 'convert'). If you don't have it,
# scale public/icon-512.png to the sizes below by hand and drop them in place.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="${REPO_ROOT}/public/icon-512.png"     # source artwork (square, 512x512)
RES="${REPO_ROOT}/android/res"

if [[ ! -f "${SRC}" ]]; then
  echo "ERROR: source icon not found: ${SRC}" >&2
  exit 1
fi

if command -v magick >/dev/null 2>&1; then
  CONV=(magick)
elif command -v convert >/dev/null 2>&1; then
  CONV=(convert)
else
  echo "ERROR: ImageMagick not found (need 'magick' or 'convert')." >&2
  echo "Resize ${SRC} by hand to these sizes and place them as" >&2
  echo "android/res/mipmap-<density>/ic_launcher.png :" >&2
  echo "  mdpi=48 hdpi=72 xhdpi=96 xxhdpi=144 xxxhdpi=192" >&2
  exit 1
fi

declare -A SIZES=( [mdpi]=48 [hdpi]=72 [xhdpi]=96 [xxhdpi]=144 [xxxhdpi]=192 )
for d in "${!SIZES[@]}"; do
  px="${SIZES[$d]}"
  dir="${RES}/mipmap-${d}"
  mkdir -p "${dir}"
  "${CONV[@]}" "${SRC}" -resize "${px}x${px}" "${dir}/ic_launcher.png"
  echo "wrote ${dir}/ic_launcher.png (${px}x${px})"
done

echo
echo "Done. Point app/Cargo.toml at it:"
echo '  [package.metadata.android]'
echo '  res  = "android/res"'
echo '  icon = "@mipmap/ic_launcher"'
