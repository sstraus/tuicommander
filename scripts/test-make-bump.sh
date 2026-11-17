#!/usr/bin/env bash
# Catches make bump swallowing a failed notes generator and printing Done.
# Exercise the real recipe and notes script in a disposable repository fixture.
set -euo pipefail
project_root="$(cd "$(dirname "$0")/.." && pwd)"
test_tmp="$(. "$project_root/scripts/test-tmp-lib.sh" && tuic_test_tmp_root "$project_root")"
mkdir -p "$test_tmp"
fixture="$(mktemp -d "$test_tmp/make-bump.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/bin" "$fixture/scripts" "$fixture/src-tauri" "$fixture/src/assets"
awk '/^bump:$/ { copying=1 } copying { if ($0 == "") exit; print }' "$project_root/Makefile" > "$fixture/Makefile"
cp "$project_root/scripts/generate-release-notes.sh" "$fixture/scripts/"
# Cargo metadata is outside this shell contract; no build or dependency update.
printf '#!/bin/sh\nexit 0\n' > "$fixture/bin/cargo"
# The recipe uses BSD sed. Adapt only its empty backup suffix on Linux.
cat > "$fixture/bin/sed" <<'SED'
#!/usr/bin/env bash
if [[ $(uname -s) != Darwin && ${1:-} == -i && ${2:-} == '' ]]; then
  shift 2
  exec /usr/bin/sed -i "$@"
fi
exec /usr/bin/sed "$@"
SED
# Deterministic CLI substitute, not a claim about the external AI service.
cat > "$fixture/bin/claude" <<'CLAUDE'
#!/bin/sh
if [ "$NOTES_EXIT" -ne 0 ]; then exit "$NOTES_EXIT"; fi
printf '%s\n' '- Release fixture note'
CLAUDE
chmod +x "$fixture/bin/"* "$fixture/scripts/generate-release-notes.sh"
export PATH="$fixture/bin:$PATH"
reset_fixture() {
  printf '[workspace.package]\nversion = "1.7.7"\n' > "$fixture/src-tauri/Cargo.toml"
  printf '{"version": "1.7.7"}\n' > "$fixture/src-tauri/tauri.conf.json"
  printf '{\n  "version": "1.7.7"\n}\n' > "$fixture/package.json"
  printf '**Version:** 1.7.7\n' > "$fixture/SPEC.md"
  printf '## [Unreleased]\n\n### Fixed\n- Release fixture\n' > "$fixture/CHANGELOG.md"
  printf '{}\n' > "$fixture/src/assets/release-notes.json"
}
for status in 1 7; do
  reset_fixture
  if output="$(cd "$fixture" && NOTES_EXIT="$status" make bump V=1.8.0 </dev/null 2>&1)"; then
    echo "FAIL: make bump swallowed notes exit $status" >&2
    exit 1
  fi
  if [[ $output == *'==> Done.'* ]]; then
    echo 'FAIL: failed bump printed Done' >&2
    exit 1
  fi
  [[ $output == *'Release preparation is incomplete.'* ]]
  [[ $(cat "$fixture/src/assets/release-notes.json") == '{}' ]]
done
reset_fixture
output="$(cd "$fixture" && NOTES_EXIT=0 make bump V=1.8.0 </dev/null 2>&1)"
[[ $output == *'==> Done.'* ]]
python3 - "$fixture" <<'PY'
import json
import pathlib
import sys
root = pathlib.Path(sys.argv[1])
assert 'version = "1.8.0"' in (root / 'src-tauri/Cargo.toml').read_text()
for name in ['package.json', 'src-tauri/tauri.conf.json']:
    assert json.loads((root / name).read_text())['version'] == '1.8.0'
assert '**Version:** 1.8.0' in (root / 'SPEC.md').read_text()
assert '## [1.8.0]' in (root / 'CHANGELOG.md').read_text()
notes = json.loads((root / 'src/assets/release-notes.json').read_text())
assert notes['1.8.0']['highlights'] == ['Release fixture note']
PY
printf '%s\n' 'ok: make bump propagates notes failures and preserves successful preparation'
