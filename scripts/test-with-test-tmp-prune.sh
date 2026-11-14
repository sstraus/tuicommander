#!/usr/bin/env bash
# with-test-tmp.sh prunes only stale scratch it created, only under the
# caller's temp dir (plus the old in-checkout base; never /tmp), and never fails the
# wrapped command because a prune was denied. Everything here lives under a
# fake host TMPDIR this test owns; nothing touches $HOME.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
base="$(. "$root/scripts/test-tmp-lib.sh" && tuic_test_tmp_root "$root")"
fixture="$(mktemp -d "${base%/}/tuic-prune-test.XXXXXX")"
cleanup() {
  chmod -R u+rwX "$fixture"
  rm -rf "$fixture"
}
trap cleanup EXIT

# A throwaway checkout, so the old in-checkout base can be seeded without
# writing into this repository.
checkout="$fixture/checkout"
git init --quiet "$checkout"
mkdir -p "$checkout/scripts" "$checkout/.tmp/tuic-tests"
cp "$root/scripts/with-test-tmp.sh" "$root/scripts/test-tmp-lib.sh" "$checkout/scripts/"

wrap() {
  env -u TUIC_TEST_TMP_ROOT -u TUIC_TEST_HOST_TMPDIR -u TUIC_TEST_TMP_BASE TMPDIR="$1/" \
    "$checkout/scripts/with-test-tmp.sh" true
}

host="$fixture/host"
legacy_run="$checkout/.tmp/tuic-tests/tuic-run.prune-test-$$"
mkdir -p "$host/tuic-tests"
old_socket="$host/tuic-s$(printf '%016x' "$$")"
recent_socket="$host/tuic-s$(printf '%016x' "$(($$ + 1))")"
lookalike="$host/tuic-s$(printf '%015x' "$$")z"
old_run="$host/tuic-tests/tuic-run.prune-test-$$"
recent_run="$host/tuic-tests/tuic-run.recent-test-$$"
unrelated="$host/tuic-tests/keep-me-$$"
# Socket roots (t.XXXXXX): only a stale one carrying our marker goes.
old_root="$host/t.old$(printf '%03d' "$(($$ % 1000))")"
recent_root="$host/t.new$(printf '%03d' "$(($$ % 1000))")"
unmarked_root="$host/t.unm$(printf '%03d' "$(($$ % 1000))")"
mkdir "$old_socket" "$recent_socket" "$lookalike" "$old_run" "$recent_run" "$unrelated" "$legacy_run" \
  "$old_root" "$recent_root" "$unmarked_root"
: > "$old_root/.tuic-socket-root"
: > "$recent_root/.tuic-socket-root"
touch -t 202001010000 "$old_socket" "$lookalike" "$old_run" "$unrelated" "$legacy_run" \
  "$old_root" "$unmarked_root"

wrap "$host"

test ! -e "$old_socket" || { echo "stale socket scratch was not pruned" >&2; exit 1; }
test ! -e "$old_run" || { echo "stale test run was not pruned" >&2; exit 1; }
test ! -e "$legacy_run" || { echo "stale in-checkout test run was not pruned" >&2; exit 1; }
test -d "$recent_socket" || { echo "recent socket scratch was removed" >&2; exit 1; }
test -d "$recent_run" || { echo "recent test run was removed" >&2; exit 1; }
test -d "$lookalike" || { echo "look-alike socket scratch was pruned" >&2; exit 1; }
test -d "$unrelated" || { echo "unrelated scratch was pruned" >&2; exit 1; }
test ! -e "$old_root" || { echo "stale marked socket root was not pruned" >&2; exit 1; }
test -d "$recent_root" || { echo "recent socket root was removed" >&2; exit 1; }
test -d "$unmarked_root" || { echo "an unmarked t.* dir was pruned" >&2; exit 1; }

# Catches: a denied deletion aborting the wrapped command under `set -e`.
locked="$fixture/locked"
mkdir -p "$locked/tuic-tests"
stuck="$locked/tuic-s$(printf '%016x' "$(($$ + 2))")"
mkdir "$stuck"
touch -t 202001010000 "$stuck"
chmod 0555 "$locked"
if ! output="$(wrap "$locked" 2>&1)"; then
  echo "a denied prune failed the wrapped command:" >&2
  printf '%s\n' "$output" >&2
  exit 1
fi
printf '%s\n' "$output" | grep -Fq "could not prune" \
  || { echo "a denied prune was not reported" >&2; printf '%s\n' "$output" >&2; exit 1; }
test -d "$stuck"
