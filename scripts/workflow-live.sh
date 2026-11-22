#!/usr/bin/env bash
# Opt-in live suite for the workflow engine. Docs: docs/workflows/testing/live.md
# Writes <checkout>/.tmp/wf-live/<stamp>/evidence.json (WF_LIVE_OUT_ROOT overrides). Absent credentials make a
# test report skipped/unavailable with the missing variable names; nothing is mocked.
# Usage: scripts/workflow-live.sh [--list]
# Env:   WF_LIVE_RUNNER  command prefix, e.g. "build-slot.sh --remote" (default: run here)
#        RB_SECRET_ENV_FILE  env file sourced for credentials (never printed)
#        TUIC_LIVE_BUDGET_FILE  persistent real-model-run counter
#        WF_LIVE_OUT_ROOT  evidence root (default: <checkout>/.tmp/wf-live)
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
export PATH="$HOME/Library/Application Support/mbx/bin:/opt/homebrew/bin:$PATH"

if [ -n "${RB_SECRET_ENV_FILE:-}" ]; then
  set -a; . "$RB_SECRET_ENV_FILE"; set +a
fi

root="${WF_LIVE_OUT_ROOT:-$PWD/.tmp/wf-live}"
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
out="$root/$stamp"
mkdir -p "$out"
export TUIC_LIVE_RESULTS="$out/results.jsonl"
export TUIC_LIVE_BUDGET_FILE="${TUIC_LIVE_BUDGET_FILE:-$root/model-runs.count}"
: > "$TUIC_LIVE_RESULTS"

selector='test(/live_(a|c|jira|gitlab|telegram)_[a-z_]+$/)'
runner="${WF_LIVE_RUNNER:-}"
if [ "${1:-}" = "--list" ]; then
  # shellcheck disable=SC2086
  $runner scripts/with-test-tmp.sh cargo nextest list --manifest-path src-tauri/Cargo.toml --lib --run-ignored ignored-only -E "$selector"
  exit 0
fi

# shellcheck disable=SC2086
set +e
$runner scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib \
  --run-ignored ignored-only --no-capture --no-fail-fast -E "$selector" >"$out/suite.log" 2>&1
suite_exit=$?

# GitHub sandbox: the existing ignored test already cleans up its issue. Gate here.
gh_test='test(github_sandbox_issue_and_pr_sources_execute_installed_authenticated_connector)'
if [ -z "${GITHUB_E2E_TOKEN:-}" ]; then
  printf '%s\n' '{"test":"live_github_issue_pr_trigger_to_completion","result":"unavailable","reason":"unavailable: missing GITHUB_E2E_TOKEN"}' >> "$TUIC_LIVE_RESULTS"
else
  # shellcheck disable=SC2086
  TUIC_PARITY_EVIDENCE="$out/github-run.json" $runner scripts/with-test-tmp.sh cargo nextest run \
    --manifest-path src-tauri/Cargo.toml --lib --run-ignored ignored-only --no-capture -E "$gh_test" >"$out/github.log" 2>&1
  gh_exit=$?
  issue="$(sed -n 's/.*GITHUB_LIVE_ISSUE=\([0-9]*\).*ISSUE_RUN=\([^ ]*\).*/\1 \2/p' "$out/github.log" | head -1)"
  python3 -I - "$gh_exit" "$issue" >> "$TUIC_LIVE_RESULTS" <<'PY'
import json, sys
code, issue = int(sys.argv[1]), sys.argv[2].split()
r = {"test": "live_github_issue_pr_trigger_to_completion", "result": "passed" if code == 0 else "failed",
     "cost": "no model; sandbox sstraus/tuic-e2e-sandbox, 1 issue created and closed"}
if issue:
    r["run_id"], r["sandbox_issue"] = issue[1], issue[0]
print(json.dumps(r))
PY
fi
set -e

python3 -I - "$out" "$suite_exit" "$(git rev-parse HEAD)" "$TUIC_LIVE_BUDGET_FILE" <<'PY'
import json, pathlib, sys
out, code, sha, budget = pathlib.Path(sys.argv[1]), int(sys.argv[2]), sys.argv[3], pathlib.Path(sys.argv[4])
tests = [json.loads(l) for l in (out / "results.jsonl").read_text().splitlines() if l.strip()]
used = int(budget.read_text().strip() or 0) if budget.exists() else 0
evidence = {"head": sha, "suite_exit": code, "model_runs_used_total": used, "tests": tests}
(out / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
for t in tests:
    print(f'{t["result"]:12} {t["test"]} {t.get("reason", "")}')
print(f"evidence: {out / 'evidence.json'}")
PY
exit "$suite_exit"
