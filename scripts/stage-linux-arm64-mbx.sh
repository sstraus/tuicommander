#!/usr/bin/env bash
# Keep the ARM64 build tool task-local; do not replace a host installation.
set -euo pipefail
version=1.29.1
archive_sha256=79fb378e26ee06ae3d23b18406d0eff1db196552f65de9af98bc64cf8902f4a8
dest=${1:?Usage: stage-linux-arm64-mbx.sh TASK_TOOLS_DIRECTORY}
test "$(uname -m)" = aarch64
case "$dest" in
  /*) ;;
  *) echo 'Use an absolute task-local tools directory.' >&2; exit 1 ;;
esac
case "$dest" in
  *[[:space:]]*) echo 'The mbx path must not contain whitespace.' >&2; exit 1 ;;
esac
dest="$dest/mbx-v$version"
mkdir -p "$dest"
archive="$dest/mbx-aarch64-unknown-linux-musl.tar.gz"
if ! test -f "$archive"; then
  curl --fail --location --retry 2 \
    "https://github.com/jdx/mr-boxington/releases/download/v$version/mbx-aarch64-unknown-linux-musl.tar.gz" \
    -o "$archive"
fi
printf '%s  %s\n' "$archive_sha256" "$archive" | sha256sum --check
# Extract only this release's executable; preserve any existing tool unchanged.
if ! test -e "$dest/mbx"; then
  tar -xzf "$archive" -C "$dest" mbx
fi
test "$(tar -xOzf "$archive" mbx | sha256sum | cut -d ' ' -f 1)" = \
  "$(sha256sum "$dest/mbx" | cut -d ' ' -f 1)"
test "$("$dest/mbx" --version)" = "mbx $version"
sha256sum "$dest/mbx"
printf 'MBX_BINARY=%s/mbx\n' "$dest"
