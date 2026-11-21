#!/usr/bin/env bash
# Exercise the release-body file boundary without a GitHub publication or token.
set -euo pipefail
project_root="$(cd "$(dirname "$0")/.." && pwd)"
test_tmp="$(. "$project_root/scripts/test-tmp-lib.sh" && tuic_test_tmp_root "$project_root")"
mkdir -p "$test_tmp"
fixture="$(mktemp -d "$test_tmp/release-notes-limit.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT

python3 - "$project_root" "$fixture" <<'PY'
from pathlib import Path
import subprocess
import sys

root, fixture = map(Path, sys.argv[1:])
compare = "https://github.com/sstraus/tuicommander/compare/v1.7.6...main"
notes = fixture / "notes.md"

def cap(body):
    notes.write_bytes(body)
    result = subprocess.run(
        [sys.executable, str(root / "scripts/cap-release-notes.py"), str(notes), compare],
        capture_output=True, text=True, check=True,
    )
    assert not result.stdout and not result.stderr
    return notes.read_bytes()

# Catches: adding a truncation footer to small or exactly-budgeted notes.
for body in [b"", b"### Features\n- feat: small release\n", b"x" * 120000]:
    assert cap(body) == body

# Catches: publishing unlimited history, dropping the compare link, or chopping subjects.
line = "- fix: complete commit subject\n"
body = ("## Changes since v1.7.6\n\n### Fixes\n" + line * 6000).encode()
result = cap(body)
assert len(result) <= 120000 and len(result.decode("utf-8")) < 125000
text = result.decode("utf-8")
assert text.startswith("## Changes since v1.7.6\n\n### Fixes\n")
assert f"[View all changes]({compare})" in text
assert "Notes truncated." in text
prefix = text.split("\n\n---\n\n")[0]
assert prefix.splitlines()[-1] == line.rstrip("\n")
assert 0 < prefix.count(line.rstrip("\n")) < 6000
assert cap(result) == result

# Catches: measuring characters then cutting bytes inside a multibyte subject.
body = ("### Features\n" + "- feat: café 🐧 漢字\n" * 10000).encode()
result = cap(body)
assert len(result) <= 120000 and len(result.decode("utf-8")) < 125000
assert f"[View all changes]({compare})" in result.decode("utf-8")
assert result.decode("utf-8").split("\n\n---\n\n")[0].splitlines()[-1] == "- feat: café 🐧 漢字"

# Catches: one oversized line escaping the bound when no newline fits.
result = cap(b"x" * 125001)
assert len(result) < 125000
assert f"[View all changes]({compare})" in result.decode("utf-8")
assert b"x" * 100 not in result
print("ok: release notes preserve small bodies and bound history, Unicode, and oversized lines")
PY
