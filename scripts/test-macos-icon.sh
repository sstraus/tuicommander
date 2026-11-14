#!/bin/bash
# Catches full-bleed square art in the macOS window and development Dock icons.
set -euo pipefail
cd "${1:-$(dirname "$0")/..}"
config=src-tauri/tauri.macos.conf.json
png=$(python3 - "$config" <<'PY'
import json, pathlib, sys
p = pathlib.Path(sys.argv[1])
config = json.loads(p.read_text() if p.exists() else pathlib.Path('src-tauri/tauri.conf.json').read_text())
print(next(icon for icon in config['bundle']['icon'] if icon.endswith('.png')))
PY
)
check_margin() {
  local alpha
  alpha=$(magick "$1" -format '%[fx:p{0,0}.a]' info:)
  [[ "$alpha" == 0 ]] || { echo "FAIL macos_icon_has_transparent_margin: $1 corner alpha=$alpha"; exit 1; }
  # Catch rounded-but-full-bleed art: the outer five percent must stay empty.
  alpha=$(magick "$1" -format '%[fx:p{w/2,floor(h*0.05)}.a]' info:)
  [[ "$alpha" == 0 ]] || { echo "FAIL macos_icon_has_transparent_margin: $1 top margin alpha=$alpha"; exit 1; }
  alpha=$(magick "$1" -format '%[fx:p{w/2,h/2}.a]' info:)
  [[ "$alpha" == 1 ]] || { echo "FAIL macos_icon_has_transparent_margin: $1 body alpha=$alpha"; exit 1; }
}
check_margin "src-tauri/$png"
# Catches a rounded full-bleed icon or an accidentally empty icon.
[[ $(magick identify -format '%wx%h' "src-tauri/$png") == 1024x1024 ]]
[[ $(magick "src-tauri/$png" -format '%[fx:p{50,512}.a]' info:) == 0 ]]
[[ $(magick "src-tauri/$png" -format '%[fx:p{100,512}.a]' info:) == 1 ]]
[[ $(magick "src-tauri/$png" -format '%[fx:p{923,512}.a]' info:) == 1 ]]
[[ $(magick "src-tauri/$png" -format '%[fx:p{974,512}.a]' info:) == 0 ]]
# ICNS is the application icon Tauri embeds for pnpm tauri dev on macOS.
mkdir -p .tmp
work=$(mktemp -d "$PWD/.tmp/macos-icon-test.XXXXXX")
trap 'rm -rf "$work"' EXIT
iconutil -c iconset src-tauri/icons/icon.icns -o "$work/icon.iconset"
for icon in "$work/icon.iconset/"*.png; do
  check_margin "$icon"
done
echo 'PASS macos_icon_has_transparent_margin (macOS PNG and ICNS)'
