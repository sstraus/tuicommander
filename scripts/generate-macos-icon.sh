#!/bin/bash
# Preserve the shared source art; generate only macOS assets.
set -euo pipefail
cd "$(dirname "$0")/.."
out=src-tauri/icons/macos
mkdir -p .tmp "$out"
work=$(mktemp -d "$PWD/.tmp/macos-icon.XXXXXX")
trap 'rm -rf "$work"' EXIT
# The body occupies 824 of 1024 pixels, centered with a 100-pixel margin.
# Render the rounded mask at 4x resolution for smooth small representations.
magick -size 4096x4096 xc:none -fill white \
  -draw 'roundrectangle 400,400 3695,3695 736,736' \
  -resize 1024x1024 "$work/mask.png"
magick src-tauri/icons/icon.png -resize 824x824 \
  -background none -gravity center -extent 1024x1024 \
  "$work/mask.png" -alpha off -compose CopyOpacity -composite "$out/icon.png"
mkdir "$work/icon.iconset"
for size in 16 32 128 256 512; do
  magick "$out/icon.png" -resize "${size}x${size}" "$work/icon.iconset/icon_${size}x${size}.png"
  double=$((size * 2))
  magick "$out/icon.png" -resize "${double}x${double}" "$work/icon.iconset/icon_${size}x${size}@2x.png"
done
iconutil -c icns "$work/icon.iconset" -o src-tauri/icons/icon.icns
