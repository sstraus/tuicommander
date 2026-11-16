#!/usr/bin/env bash
# Catches: masking a failed release edit and reporting update-notes as successful.
set -euo pipefail
project_root="$(cd "$(dirname "$0")/.." && pwd)"
test_tmp="$(. "$project_root/scripts/test-tmp-lib.sh" && tuic_test_tmp_root "$project_root")"
mkdir -p "$test_tmp"
fixture="$(mktemp -d "$test_tmp/nightly-notes-error.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT

python3 - "$project_root" "$fixture/step.sh" <<'PY'
from pathlib import Path
import sys

root = Path(sys.argv[1])
workflow = (root / ".github/workflows/nightly.yml").read_text()
step = workflow.split("      - name: Generate changelog and update release notes\n", 1)[1]
block = step.split("        run: |\n", 1)[1].split("\n      - name:", 1)[0]
Path(sys.argv[2]).write_text("\n".join(line[10:] for line in block.splitlines()) + "\n")
PY

# Inject only the external failure; the workflow decides whether to propagate it.
gh() { return 42; }
export -f gh
status=0
(cd "$project_root" && GITHUB_REPOSITORY=sstraus/tuicommander bash -e "$fixture/step.sh") > "$fixture/output" 2>&1 || status=$?
if [[ "$status" -ne 42 ]]; then
  cat "$fixture/output" >&2
  echo "update-notes masked gh exit 42 or failed before release edit: exit $status" >&2
  exit 1
fi
echo "ok: update-notes propagates release edit failure"
