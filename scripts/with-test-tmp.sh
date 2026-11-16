#!/usr/bin/env bash
# Run a test command with scratch files inside this checkout or its Gits root.
set -euo pipefail

# An exported GIT_DIR (or friends) selects a repository outright and the ceiling never
# restricts it, so neither this script's own git calls nor the command may inherit one.
# Same list as tuic-test-support's GIT_LOCATION_OVERRIDES.
unset GIT_DIR GIT_WORK_TREE GIT_COMMON_DIR GIT_INDEX_FILE GIT_OBJECT_DIRECTORY \
  GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_NAMESPACE GIT_PREFIX
root="$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)"
# A linked worktree must not inherit the parent TUICommander's Cargo target.
# Otherwise its tests can reuse dependency artifacts compiled from another checkout.
if [ -f "$root/.git" ]; then
  unset CARGO_TARGET_DIR
fi
case "${TMPDIR:-}" in
  "$root/"*|"$HOME/Gits/"*) test_tmp_base="${TMPDIR%/}" ;;
  *) test_tmp_base="$root/.tmp/tuic-tests" ;;
esac
# mbx binds its cache socket at "$TMPDIR/mbx-session-XXXXXX/cache-agent.sock",
# and a Unix socket path holds at most 103 bytes on macOS. Keep room for that
# and for this run's "tuic-run.XXXXXX/" directory; a deep checkout falls back
# to the shared Gits scratch root (#2841-774f). A GitHub runner has no ~/Gits
# and a checkout root over the budget, so it uses the runner's own short temp
# directory, which the runner deletes after the job (#3061-4580).
# 103 - 17 ("/tuic-run.XXXXXX/") - 35 ("mbx-session-XXXXXX/cache-agent.sock").
socket_budget=51
if [ "${#test_tmp_base}" -gt "$socket_budget" ] && [ -d "$HOME/Gits" ]; then
  test_tmp_base="$HOME/Gits/.tmp/tuic-tests"
elif [ "${#test_tmp_base}" -gt "$socket_budget" ] && [ -n "${RUNNER_TEMP:-}" ]; then
  test_tmp_base="${RUNNER_TEMP%/}/tuic-tests"
fi
if [ "${#test_tmp_base}" -gt "$socket_budget" ]; then
  echo "with-test-tmp.sh: $test_tmp_base is ${#test_tmp_base} bytes; the mbx socket path limit leaves $socket_budget. Set TMPDIR to a shorter directory under $root or ~/Gits." >&2
  exit 1
fi
mkdir -p "$test_tmp_base"
for stale_root in "$root/.tmp/tuic-tests" "$HOME/Gits/.tmp/tuic-tests"; do
  if [[ -d "$stale_root" ]]; then
    find "$stale_root" -mindepth 1 -maxdepth 1 -type d \
      \( -name 'tuic-run.*' -o -name 'socket-*' \) -mtime +6 -exec rm -rf -- {} +
  fi
done
shared_socket_parent="$HOME/Gits/.tmp"
if [[ -d "$shared_socket_parent" ]]; then
  while IFS= read -r -d '' stale_socket; do
    [[ ${stale_socket##*/} =~ ^s[0-9a-f]{16}$ ]] || continue
    rm -rf -- "$stale_socket"
  done < <(find "$shared_socket_parent" -mindepth 1 -maxdepth 1 -type d \
    -name 's????????????????' -mtime +6 -print0)
fi
test_tmp="$(mktemp -d "$test_tmp_base/tuic-run.XXXXXX")"
# Real permission tests may leave unreadable directories or read-only files.
# This root is disposable; physical traversal never changes symlink targets.
cleanup_test_tmp() {
  local command_status=$?
  chmod -R u+rwX "$test_tmp"
  rm -rf "$test_tmp"
  return "$command_status"
}
trap cleanup_test_tmp EXIT
export TMPDIR="$test_tmp/"
export TMP="$test_tmp/"
export TEMP="$test_tmp/"
export TUIC_TEST_TMP_ROOT="$test_tmp"
# Git hermeticity. An empty template keeps `git init`
# and `git clone` in test fixtures from copying git's hooks/*.sample files into
# every scratch repo: slower, machine-dependent, and an EPERM where `.git/hooks`
# writes are forbidden. A test that needs hooks creates `.git/hooks` itself.
# Fail closed: git under the test root never discovers a repository above it (a
# fixture whose `git init` failed once fell through to this checkout and renamed
# its branches). The ceiling is the root's parent, appended once to any value an
# outer wrapper already set.
# Same function in scripts/with-test-tmp.sh and src-tauri/scripts/nextest-test-tmp.sh
# (each stays one self-contained file: tests copy the wrapper into fixture checkouts).
# Usage: tuic_git_ceiling_with <dir>... Prints $GIT_CEILING_DIRECTORIES with each
# existing <dir> appended in its lexical and its physical spelling (macOS /var
# vs /private/var), each entry once, never an empty one: git skips entries it
# cannot compare and stops resolving symlinks after an empty entry. An outer
# wrapper's value is kept verbatim, so a stricter ceiling survives. Git never
# ascends INTO an entry, so pass the PARENT of the dir the fence protects.
tuic_git_ceiling_with() {
  _tuic_ceiling=${GIT_CEILING_DIRECTORIES:-}
  for _tuic_dir in "$@"; do
    for _tuic_entry in "$(CDPATH='' cd -- "$_tuic_dir" 2>/dev/null && pwd)" \
      "$(CDPATH='' cd -- "$_tuic_dir" 2>/dev/null && pwd -P)"; do
      [ -n "$_tuic_entry" ] || continue
      case ":$_tuic_ceiling:" in
        *":$_tuic_entry:"*) ;;
        *) _tuic_ceiling=${_tuic_ceiling:+$_tuic_ceiling:}$_tuic_entry ;;
      esac
    done
  done
  printf '%s\n' "$_tuic_ceiling"
}
# hooks/ stays an empty directory and info/ holds git's one-line default exclude file:
# tests write .git/hooks/* and read or append .git/info/exclude as after a real `git init`;
# only the sample hooks are gone.
mkdir -p "$test_tmp/git-template-empty/hooks" "$test_tmp/git-template-empty/info"
printf '%s\n' '# git ls-files --others --exclude-from=.git/info/exclude' > "$test_tmp/git-template-empty/info/exclude"
export GIT_TEMPLATE_DIR="$test_tmp/git-template-empty"
# The parent of the PHYSICAL root as well: when the root is (or lives behind) a symlink,
# git compares ceiling entries with where the root really is.
GIT_CEILING_DIRECTORIES="$(tuic_git_ceiling_with "$(dirname "$test_tmp")" "$(dirname "$(cd "$test_tmp" && pwd -P)")")"
export GIT_CEILING_DIRECTORIES
"$@"
