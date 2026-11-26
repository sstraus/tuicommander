#!/usr/bin/env bash
# Verify the actual native desktop payloads, not only the asset filenames.
# Run on the same Linux host that built the packages, before draft publication.
set -euo pipefail
release=${1:?usage: $0 <release-dir> <Linux-x64|Linux-ARM64>}
label=${2:?usage: $0 <release-dir> <Linux-x64|Linux-ARM64>}
case "$label" in
  Linux-x64) host=x86_64; deb_arch=amd64; rpm_arch=x86_64; machine='Advanced Micro Devices X86-64' ;;
  Linux-ARM64) host=aarch64; deb_arch=arm64; rpm_arch=aarch64; machine=AArch64 ;;
  *) echo "Unsupported Linux package label: $label" >&2; exit 2 ;;
esac
test "$(uname -m)" = "$host"
test "$(rustc --print host-tuple)" = "${host}-unknown-linux-gnu"
release=$(cd "$release" && pwd)
mkdir -p .tmp/tuic-tests
work=$(mktemp -d .tmp/tuic-tests/linux-packages.XXXXXX)
trap 'rm -rf "$work"' EXIT
work=$(cd "$work" && pwd)
check_elf() {
  local binary=$1
  test -x "$binary"
  readelf -h "$binary" | grep -F "Machine:" | grep -F "$machine" >/dev/null
}
for binary in tuicommander tuic tuic-bridge; do
  check_elf "$release/$binary"
done
shopt -s nullglob
debs=("$release"/bundle/deb/*.deb)
rpms=("$release"/bundle/rpm/*.rpm)
images=("$release"/bundle/appimage/*.AppImage)
test "${#debs[@]}" = 1
test "${#rpms[@]}" = 1
test "${#images[@]}" = 1
test "$(dpkg-deb -f "${debs[0]}" Architecture)" = "$deb_arch"
test "$(rpm -qp --queryformat '%{ARCH}' "${rpms[0]}")" = "$rpm_arch"
dpkg-deb -f "${debs[0]}" Depends | grep -F libvulkan1 >/dev/null
rpm -qp --requires "${rpms[0]}" | grep -F libvulkan.so.1 >/dev/null
dpkg-deb -x "${debs[0]}" "$work/deb"
mkdir "$work/rpm"
(cd "$work/rpm" && bsdtar -xf "${rpms[0]}")
chmod +x "${images[0]}"
(cd "$work" && "${images[0]}" --appimage-extract >/dev/null)
for payload in deb rpm squashfs-root; do
  for binary in tuicommander tuic tuic-bridge; do
    found=$(find "$work/$payload" -type f -name "$binary" -print -quit)
    test -n "$found"
    check_elf "$found"
  done
done
# Tauri's updater uses a signed AppImage on Linux for this configuration.
test -s "${images[0]}.sig"
(cd "$release/bundle/deb" && sha256sum -- *.deb; cd ../rpm && sha256sum -- *.rpm; cd ../appimage && sha256sum -- *.AppImage) > "$release/bundle/SHA256SUMS-${label}.txt"
echo "Verified $label desktop, sidecars, deb/rpm metadata, AppImage payload and signature"
cat "$release/bundle/SHA256SUMS-${label}.txt"
