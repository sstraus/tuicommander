#!/usr/bin/env bash
# One resolution order for the test scratch root across the shell entry points
# (scripts/with-test-tmp.sh, src-tauri/scripts/nextest-test-tmp.sh and the
# shell tests' scripts/test-tmp-lib.sh): an inherited TUIC_TEST_TMP_ROOT is the
# caller's and is used as is, else a default under the host temp dir, never in
# the checkout. Everything lives under a fake host TMPDIR this test owns.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
test_tmp="$(. "$root/scripts/test-tmp-lib.sh" && tuic_test_tmp_root "$root")"
fixture="$(mktemp -d "$test_tmp/tuic-roots-test.XXXXXX")"
cleanup() {
  chmod -R u+rwX "$fixture"
  rm -rf "$fixture"
}
trap cleanup EXIT
host="$fixture/host"
mkdir -p "$host"
fail() {
  echo "$*" >&2
  exit 1
}
isolated() {
  env -u TUIC_TEST_TMP_ROOT -u TUIC_TEST_TMP_BASE -u TUIC_TEST_HOST_TMPDIR \
    TMPDIR="$host/" "$@"
}
checkout_real="$(cd "$root" && pwd -P)"
default_root="$host/tuic-tests/tuic-co-$(. "$root/scripts/test-tmp-lib.sh" && tuic_checkout_hash "$checkout_real")"

# (a) Catches: the wrapper replacing (and then deleting) a root its caller chose.
caller_root="$fixture/caller-root"
mkdir -p "$caller_root"
stale_run="$caller_root/tuic-run.stale-$$"
recent_run="$caller_root/tuic-run.recent-$$"
unrelated="$caller_root/keep-me-$$"
mkdir "$stale_run" "$recent_run" "$unrelated"
touch -t 202001010000 "$stale_run" "$unrelated"
seen="$(isolated TUIC_TEST_TMP_ROOT="$caller_root" "$root/scripts/with-test-tmp.sh" \
  sh -c 'printf "%s|%s|%s" "$TUIC_TEST_TMP_ROOT" "$TMPDIR" "$TUIC_TEST_HOST_TMPDIR"')"
IFS='|' read -r seen_root seen_tmpdir seen_host <<<"$seen"
test "$seen_root" = "$caller_root" || fail "inherited root $caller_root was replaced by $seen_root"
test "$seen_tmpdir" = "$caller_root/" || fail "TMPDIR $seen_tmpdir is not the inherited root"
test "$seen_host" = "$host" || fail "TUIC_TEST_HOST_TMPDIR $seen_host != $host"
test -d "$caller_root" || fail "the wrapper deleted the caller's root"
test ! -e "$stale_run" || fail "a stale tuic-run.* child of the inherited root was not pruned"
test -d "$recent_run" || fail "a recent tuic-run.* child of the inherited root was pruned"
test -d "$unrelated" || fail "an unrelated child of the inherited root was pruned"
# A trailing slash names the same root.
seen="$(isolated TUIC_TEST_TMP_ROOT="$caller_root/" "$root/scripts/with-test-tmp.sh" \
  sh -c 'printf "%s" "$TUIC_TEST_TMP_ROOT"')"
test "$seen" = "$caller_root" || fail "inherited root with a trailing slash became $seen"

# (b) Catches: a default outside the host temp dir, or inside the checkout.
seen="$(isolated "$root/scripts/with-test-tmp.sh" sh -c 'printf "%s" "$TUIC_TEST_TMP_ROOT"')"
case "$seen" in
  "$host/tuic-tests/tuic-run."*) ;;
  *) fail "wrapper default $seen is not under $host/tuic-tests" ;;
esac
case "$seen" in "$root"/* | "$checkout_real"/*) fail "wrapper default $seen is inside the checkout" ;; esac
test ! -e "$seen" || fail "the wrapper left its own per-run root $seen behind"

seen="$(isolated sh -c '. "$1/scripts/test-tmp-lib.sh" && tuic_test_tmp_root "$1"' sh "$root")"
test "$seen" = "$default_root" || fail "shell-test default $seen != $default_root"
seen="$(isolated TUIC_TEST_TMP_BASE="$fixture/opt-in" \
  sh -c '. "$1/scripts/test-tmp-lib.sh" && tuic_test_tmp_root "$1"' sh "$root")"
case "$seen" in
  "$fixture/opt-in/tuic-co-"*) ;;
  *) fail "TUIC_TEST_TMP_BASE opt-in ignored: $seen" ;;
esac

# The nextest setup script lands on the same default, and (c) re-running it
# with the environment it wrote, minus the root, still lands there: the host
# temp dir it persisted keeps TMPDIR=<root> from nesting a second default.
nextest_env="$fixture/nextest.env"
run_nextest_setup() {
  : >"$nextest_env"
  (cd "$root/src-tauri" && NEXTEST_ENV="$nextest_env" "$@" sh scripts/nextest-test-tmp.sh)
  sed -n 's/^TUIC_TEST_TMP_ROOT=//p' "$nextest_env"
}
first="$(run_nextest_setup isolated)"
test "$first" = "$default_root" || fail "nextest default $first != $default_root"
chained_host="$(sed -n 's/^TUIC_TEST_HOST_TMPDIR=//p' "$nextest_env")"
chained_tmpdir="$(sed -n 's/^TMPDIR=//p' "$nextest_env")"
second="$(run_nextest_setup env -u TUIC_TEST_TMP_ROOT -u TUIC_TEST_TMP_BASE \
  TUIC_TEST_HOST_TMPDIR="$chained_host" TMPDIR="$chained_tmpdir")"
test "$second" = "$first" || fail "chained nextest root nested: $second (first $first)"
third="$(run_nextest_setup isolated TUIC_TEST_TMP_ROOT="$caller_root")"
test "$third" = "$caller_root" || fail "nextest replaced the inherited root with $third"
