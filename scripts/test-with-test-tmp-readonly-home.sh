#!/usr/bin/env bash
# The test wrapper, its prune test and the per-run roots must work when HOME
# cannot be written (a sandbox, a CI user, another developer's machine), and
# must put scratch under the caller's TMPDIR, never under HOME.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
base="$(. "$root/scripts/test-tmp-lib.sh" && tuic_test_tmp_root "$root")"
fixture="$(mktemp -d "${base%/}/tuic-ro-home.XXXXXX")"
cleanup() {
  chmod -R u+rwX "$fixture"
  rm -rf "$fixture"
}
trap cleanup EXIT

home="$fixture/home"
host="$fixture/host"
mkdir -p "$home" "$host"
chmod 0555 "$home"

run_isolated() {
  env -u TUIC_TEST_TMP_ROOT -u TUIC_TEST_TMP_BASE -u TUIC_TEST_HOST_TMPDIR \
    -u TUIC_TEST_SOCKET_ROOT HOME="$home" TMPDIR="$host/" "$@"
}

# Catches: the wrapper resolving its base anywhere but the caller's TMPDIR.
seen="$(run_isolated "$root/scripts/with-test-tmp.sh" sh -c \
  'printf "%s|%s|%s" "$TMPDIR" "$TUIC_TEST_TMP_ROOT" "$TUIC_TEST_HOST_TMPDIR"')"
IFS='|' read -r seen_tmpdir seen_root seen_host <<<"$seen"
case "$seen_tmpdir" in
  "$host/tuic-tests/tuic-run."*) ;;
  *) echo "per-run TMPDIR $seen_tmpdir is not under the caller's TMPDIR $host/tuic-tests" >&2; exit 1 ;;
esac
test "${seen_tmpdir%/}" = "$seen_root" || { echo "TUIC_TEST_TMP_ROOT $seen_root != TMPDIR $seen_tmpdir" >&2; exit 1; }
test "$seen_host" = "$host" || { echo "TUIC_TEST_HOST_TMPDIR $seen_host != $host" >&2; exit 1; }

# Catches: a nested wrapper making a second run dir instead of reusing the
# root the outer one exported (it would nest it if TMPDIR were its host).
nested="$(run_isolated "$root/scripts/with-test-tmp.sh" sh -c \
  'printf "%s|" "$TMPDIR"; "$0" sh -c "printf %s \"\$TMPDIR\""' \
  "$root/scripts/with-test-tmp.sh")"
IFS='|' read -r outer_tmpdir inner_tmpdir <<<"$nested"
case "$outer_tmpdir" in
  "$host/tuic-tests/tuic-run."*) ;;
  *) echo "outer per-run TMPDIR $outer_tmpdir left the host base" >&2; exit 1 ;;
esac
test "$inner_tmpdir" = "$outer_tmpdir" \
  || { echo "nested wrapper TMPDIR $inner_tmpdir != outer $outer_tmpdir" >&2; exit 1; }

# Catches: the prune test (part of `make check`) creating fixtures under HOME.
if ! output="$(run_isolated bash "$root/scripts/test-with-test-tmp-prune.sh" 2>&1)"; then
  echo "test-with-test-tmp-prune.sh fails with a read-only HOME:" >&2
  printf '%s\n' "$output" >&2
  exit 1
fi

# Catches: anything at all written below HOME.
if [ -n "$(find "$home" -mindepth 1 -print -quit)" ]; then
  echo "the wrapper wrote below HOME:" >&2
  find "$home" -mindepth 1 >&2
  exit 1
fi
