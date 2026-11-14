#!/usr/bin/env bash
# Fail when the AppImage does not carry the Vulkan loader (libvulkan.so.1).
# A build with the vulkan feature imports it, and the process does not start
# on a machine without it (exit 127, measured in story 3055-95d9). The .deb and
# .rpm declare the distribution package instead; an AppImage must bring it.
#
# Usage: check-appimage-vulkan-loader.sh <appimage-bundle-dir>
set -euo pipefail

dir=${1:?usage: $0 <appimage-bundle-dir>}
shopt -s nullglob
images=("$dir"/*.AppImage)
if (( ${#images[@]} != 1 )); then
  echo "check-appimage-vulkan-loader: expected one AppImage in $dir, found ${#images[@]}" >&2
  exit 2
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
image=$(cd "$(dirname "${images[0]}")" && pwd)/$(basename "${images[0]}")
chmod +x "$image"
(cd "$work" && "$image" --appimage-extract >/dev/null)

loader=$(find "$work/squashfs-root" -name 'libvulkan.so.1*' -print -quit)
if [[ -z $loader ]]; then
  echo "check-appimage-vulkan-loader: FAIL — ${images[0]} has no libvulkan.so.1" >&2
  exit 1
fi
echo "check-appimage-vulkan-loader: ${loader#"$work/squashfs-root/"}"
