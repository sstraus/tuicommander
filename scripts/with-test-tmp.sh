#!/usr/bin/env bash
# Run a test command with scratch files inside this checkout or its Gits root.
set -euo pipefail

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
"$@"
