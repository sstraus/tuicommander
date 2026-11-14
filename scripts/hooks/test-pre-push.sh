#!/usr/bin/env bash
set -euo pipefail
project_root="$(git rev-parse --show-toplevel)"
scratch="$(mktemp -d "${TUIC_TEST_TMP_ROOT:?run through scripts/with-test-tmp.sh}/pre-push.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
hook="${PRE_PUSH_HOOK:-$project_root/scripts/hooks/pre-push}"
real_git="$(command -v git)"
export REAL_GIT="$real_git"
mkdir -p "$scratch/bin"
# Stub only the external issue service; Git state and hook execution are real.
cat > "$scratch/bin/gh" <<'EOF'
#!/bin/sh
case "$1" in
  auth) exit 0 ;;
  issue) echo OPEN ;;
  *) exit 1 ;;
esac
EOF
cat > "$scratch/bin/git" <<'EOF'
#!/bin/sh
if [ "${FAIL_LOG:-0}" = 1 ] && [ "$1" = log ]; then
  exit 42
fi
exec "$REAL_GIT" "$@"
EOF
chmod +x "$scratch/bin/gh" "$scratch/bin/git"
export PATH="$scratch/bin:$PATH"
export TUIC_SKIP_ISSUE_CHECK=0
repo="$scratch/repo"
git init -q "$repo"
cd "$repo"
git -c core.hooksPath=/dev/null -c user.name=Test -c user.email=test@example.com commit -q --allow-empty -m initial
base="$(git rev-parse HEAD)"
git -c core.hooksPath=/dev/null -c user.name=Test -c user.email=test@example.com commit -q --allow-empty -m 'fix: repair (#123)'
bare="$(git rev-parse HEAD)"
git -c core.hooksPath=/dev/null -c user.name=Test -c user.email=test@example.com commit -q --allow-empty -m 'fix: repair (Closes #123)'
closed="$(git rev-parse HEAD)"
failures=0
run_case() {
  local name="$1" sha="$2" target="$3" previous="$4" expected="$5" diagnostic="$6" status=0
  case "$target" in refs/*) ;; *) target="refs/heads/$target" ;; esac
  printf 'refs/heads/main %s %s %s\n' "$sha" "$target" "$previous" > "$scratch/input"
  bash "$hook" private < "$scratch/input" > "$scratch/out" 2> "$scratch/err" || status=$?
  if [ "$status" -ne "$expected" ] || { [ -n "$diagnostic" ] && ! grep -qF "$diagnostic" "$scratch/err"; }; then
    printf 'FAIL %s: expected exit=%s reason=%s; got exit=%s stderr=%s\n' "$name" "$expected" "$diagnostic" "$status" "$(cat "$scratch/err")" >&2
    failures=$((failures + 1))
  else
    printf 'PASS %s\n' "$name"
  fi
}
# Catches: set -e aborts before the main fallback, even for a clean commit.
run_case missing_remote_head_allows_clean_main "$base" main "$base" 0 ''
# Catches: fixing the abort by bypassing checks accepts bare open references.
run_case missing_remote_head_checks_main "$bare" main "$base" 1 'open issue(s) referenced without a closing keyword'
# Catches: valid closing keywords become refusals when HEAD is absent.
run_case missing_remote_head_allows_closing_keyword "$closed" main "$bare" 0 ''
# Catches: fallback enforcement leaks onto non-default branches.
run_case missing_remote_head_ignores_feature "$bare" feature "$base" 0 ''
git update-ref refs/remotes/private/trunk "$base"
git symbolic-ref refs/remotes/private/HEAD refs/remotes/private/trunk
# Catches: main fallback replaces a valid remote default branch.
run_case remote_head_checks_trunk "$bare" trunk "$base" 1 'open issue(s) referenced without a closing keyword'
# Agent capabilities (story 2713): TUIC_CAPABILITIES is set only for a capped agent.
# Catches: a capped agent with no capabilities pushes a branch.
TUIC_CAPABILITIES='' run_case capped_agent_cannot_push_branch "$base" feature "$base" 1 'lacks the push capability'
# Catches: the push capability also lets a tag through.
TUIC_CAPABILITIES='push' run_case push_capability_does_not_push_tags "$base" refs/tags/v1 "$base" 1 'lacks the tag capability'
# Catches: a granted capability is not matched inside a list.
TUIC_CAPABILITIES='progress,push' run_case push_capability_allows_branch "$base" feature "$base" 0 ''
TUIC_CAPABILITIES='land,tag' run_case tag_capability_allows_tag "$base" refs/tags/v1 "$base" 0 ''
# Catches: the issue-check bypass also skips the capability rule.
TUIC_SKIP_ISSUE_CHECK=1 TUIC_CAPABILITIES='' run_case issue_bypass_keeps_capability_rule "$base" feature "$base" 1 'lacks the push capability'
# Catches: unexpected Git failure exits silently instead of reporting a reason.
export FAIL_LOG=1
run_case git_failure_reports_reason "$bare" trunk "$base" 42 'pre-push failed'
[ "$failures" -eq 0 ]
