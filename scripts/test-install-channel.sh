#!/usr/bin/env bash
# Real plist discovery; fake the network and extraction process boundaries.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$root/.tmp/tuic-tests"
work=$(mktemp -d "$root/.tmp/tuic-tests/install-channel.XXXXXX")
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/bin"
export INSTALL_TEST_WORK="$work" INSTALL_TEST_ROOT="$root"
export TUICOMMANDER_HOME="$work/Applications"
export PATH="$work/bin:$PATH"
installer="$root/website/public/install.sh"
# Ubuntu runs the Darwin installer cases too, but has no Apple PlistBuddy.
# Adapt only that process boundary in a disposable copy; still parse the real
# installed plist, and keep using Apple's binary when it is available.
if [[ ! -x /usr/libexec/PlistBuddy ]]; then
  cat > "$work/bin/PlistBuddy" <<'MOCK'
#!/usr/bin/env python3
import plistlib
import sys

if sys.argv[1:3] != ['-c', 'Print :CFBundleVersion'] or len(sys.argv) != 4:
    raise SystemExit('unexpected PlistBuddy invocation')
with open(sys.argv[3], 'rb') as source:
    print(plistlib.load(source)['CFBundleVersion'])
MOCK
  sed "s|/usr/libexec/PlistBuddy|$work/bin/PlistBuddy|g" "$installer" > "$work/install.sh"
  installer="$work/install.sh"
fi
cat > "$work/bin/uname" <<'MOCK'
#!/bin/sh
printf 'Darwin\n'
MOCK
cat > "$work/bin/curl" <<'MOCK'
#!/bin/sh
printf '%s\n' "$*" >> "$INSTALL_TEST_WORK/requests"
case "$*" in
  *releases/download/beta/latest.json*) cat "$INSTALL_TEST_WORK/beta-manifest.json" ;;
  *releases/download/nightly/latest.json*) cat "$INSTALL_TEST_ROOT/src-tauri/test-fixtures/updater/nightly-manifest.json" ;;
  *api.github.com*releases/latest*) cat "$INSTALL_TEST_ROOT/src-tauri/test-fixtures/updater/github-stable-release.json" ;;
  *releases/download/*) printf 'fixture archive\n' ;;
  *) echo "Unexpected network request: $*" >&2; exit 2 ;;
esac
MOCK
cat > "$work/bin/tar" <<'MOCK'
#!/bin/sh
cat >/dev/null
MOCK
chmod +x "$work/bin/"*
jq '.version = "2.0.0-beta.2"' "$root/src-tauri/test-fixtures/updater/stable-manifest.json" > "$work/beta-manifest.json"
installed() {
  mkdir -p "$TUICOMMANDER_HOME/TUICommander.app/Contents"
  python3 - "$TUICOMMANDER_HOME/TUICommander.app/Contents/Info.plist" "$1" <<'PY'
import plistlib, sys
with open(sys.argv[1], 'wb') as out:
    plistlib.dump({'CFBundleVersion': sys.argv[2]}, out)
PY
}
run() {
  : > "$work/requests"
  local rc=0
  sh "$installer" "$@" > "$work/output" 2>&1 || rc=$?
  if [[ $rc != 0 ]]; then cat "$work/output" >&2; echo "installer exited $rc" >&2; return "$rc"; fi
}
# Catches: an installed beta aborts during nightly detection or downloads stable.
installed 2.0.0-beta.1
run
grep -q '/releases/download/beta/latest.json' "$work/requests"
grep -q '/releases/download/v2.0.0-beta.2/TUICommander_aarch64.app.tar.gz' "$work/requests"
! grep -q 'releases/latest' "$work/requests"
# Catches: stable/no-version downloads have a duplicate v in their release tag.
rm -rf "$TUICOMMANDER_HOME"
mkdir -p "$TUICOMMANDER_HOME/TUICommander.app"
run
grep -q '/releases/download/v1.7.6/TUICommander_aarch64.app.tar.gz' "$work/requests"
! grep -q '/releases/download/vv' "$work/requests"
# Catches: explicit versions lose their v, or add one twice.
for version in 1.7.6 v1.7.6; do
  run "$version"
  grep -q '/releases/download/v1.7.6/TUICommander_aarch64.app.tar.gz' "$work/requests"
  ! grep -q 'releases/latest' "$work/requests"
done
# Catches: a stable installation cannot update because nightly detection exits 1.
installed 1.7.4
run
grep -q '/releases/download/v1.7.6/TUICommander_aarch64.app.tar.gz' "$work/requests"
# Catches: a beta already at the feed version downloads needlessly.
installed 2.0.0-beta.2
run
grep -q 'No need to update' "$work/output"
! grep -q 'app.tar.gz' "$work/requests"
# Catches: auto-detecting beta accidentally replaces the existing nightly path.
installed 1.7.0-nightly.20261010.t0000
run
grep -q '/releases/download/nightly/latest.json' "$work/requests"
grep -q '/releases/download/nightly/TUICommander_aarch64.app.tar.gz' "$work/requests"
# Catches: selecting Beta explicitly is treated as a literal version named beta.
installed 2.0.0-beta.1
run beta
grep -q '/releases/download/v2.0.0-beta.2/TUICommander_aarch64.app.tar.gz' "$work/requests"
echo 'install-channel ok (8 cases)'
