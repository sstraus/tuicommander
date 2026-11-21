#!/usr/bin/env bash
# TUIC_TEST_FAKE_HOME=1: scripts/with-test-tmp.sh moves HOME under the test
# root and pins CARGO_HOME/RUSTUP_HOME to the real home (also when HOME is
# unset, from the passwd entry), never touches HOME without the flag, and the
# nextest setup script gives a direct `cargo nextest run` the same HOME.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
base="$(. "$root/scripts/test-tmp-lib.sh" && tuic_test_tmp_root "$root")"
fixture="$(mktemp -d "${base%/}/tuic-fake-home.XXXXXX")"
cleanup() {
  chmod -R u+rwX "$fixture"
  rm -rf "$fixture"
}
trap cleanup EXIT

real_home="$fixture/real-home"
mkdir -p "$real_home"
probe='printf "%s|%s|%s|%s" "${HOME-<unset>}" "${CARGO_HOME-<unset>}" "${RUSTUP_HOME-<unset>}" "$TUIC_TEST_TMP_ROOT"'
run_wrapper() {
  env -u TUIC_TEST_TMP_ROOT -u TUIC_TEST_HOST_TMPDIR -u TUIC_TEST_TMP_BASE -u TUIC_TEST_FAKE_HOME -u CARGO_HOME -u RUSTUP_HOME TMPDIR="$fixture/host/" "$@" \
    "$root/scripts/with-test-tmp.sh" sh -c "$probe"
}
mkdir -p "$fixture/host"

# Catches: the flag not moving HOME, or cargo/rustup following it into the
# throwaway home (a cold toolchain and registry on every run).
IFS='|' read -r seen_home seen_cargo seen_rustup seen_root \
  <<<"$(run_wrapper HOME="$real_home" TUIC_TEST_FAKE_HOME=1)"
test "$seen_home" = "$seen_root/home" || { echo "HOME $seen_home is not <root>/home ($seen_root/home)" >&2; exit 1; }
test "$seen_cargo" = "$real_home/.cargo" || { echo "CARGO_HOME $seen_cargo, expected $real_home/.cargo" >&2; exit 1; }
test "$seen_rustup" = "$real_home/.rustup" || { echo "RUSTUP_HOME $seen_rustup, expected $real_home/.rustup" >&2; exit 1; }

# Catches: a caller's explicit CARGO_HOME/RUSTUP_HOME being overridden.
seen="$(env -u TUIC_TEST_TMP_ROOT -u TUIC_TEST_HOST_TMPDIR -u TUIC_TEST_TMP_BASE TMPDIR="$fixture/host/" HOME="$real_home" TUIC_TEST_FAKE_HOME=1 \
  CARGO_HOME="$fixture/my-cargo" RUSTUP_HOME="$fixture/my-rustup" \
  "$root/scripts/with-test-tmp.sh" sh -c "$probe")"
IFS='|' read -r _ seen_cargo seen_rustup _ <<<"$seen"
test "$seen_cargo" = "$fixture/my-cargo" || { echo "explicit CARGO_HOME replaced by $seen_cargo" >&2; exit 1; }
test "$seen_rustup" = "$fixture/my-rustup" || { echo "explicit RUSTUP_HOME replaced by $seen_rustup" >&2; exit 1; }

# Catches: `set -u` aborting the wrapper when HOME is unset (a launchd job, a
# minimal CI env), and the toolchain homes then following the fake one.
passwd_home="$(env -u HOME bash -c 'printf %s ~')"
status=0
seen="$(env -u HOME -u TUIC_TEST_TMP_ROOT -u TUIC_TEST_HOST_TMPDIR -u TUIC_TEST_TMP_BASE -u CARGO_HOME -u RUSTUP_HOME TMPDIR="$fixture/host/" \
  TUIC_TEST_FAKE_HOME=1 "$root/scripts/with-test-tmp.sh" sh -c "$probe" 2>"$fixture/unset.err")" || status=$?
test "$status" -eq 0 || { echo "wrapper failed ($status) with HOME unset:" >&2; cat "$fixture/unset.err" >&2; exit 1; }
IFS='|' read -r seen_home seen_cargo _ seen_root <<<"$seen"
test "$seen_home" = "$seen_root/home" || { echo "HOME unset: HOME became $seen_home" >&2; exit 1; }
if [[ -n "$passwd_home" && "$passwd_home" != "~" ]]; then
  test "$seen_cargo" = "$passwd_home/.cargo" \
    || { echo "HOME unset: CARGO_HOME $seen_cargo, expected the passwd home's $passwd_home/.cargo" >&2; exit 1; }
fi

# Catches: the wrapper touching HOME without the flag.
IFS='|' read -r seen_home seen_cargo _ _ <<<"$(run_wrapper HOME="$real_home")"
test "$seen_home" = "$real_home" || { echo "without the flag HOME became $seen_home" >&2; exit 1; }
test "$seen_cargo" = "<unset>" || { echo "without the flag CARGO_HOME became $seen_cargo" >&2; exit 1; }

# The nextest setup script: the flag writes HOME (and the pinned toolchain
# homes) into NEXTEST_ENV for the test processes; without it, no HOME line.
nextest_env() {
  local out="$fixture/nextest.env"
  : >"$out"
  # "$@" first: env takes its -u options before any assignment.
  (cd "$root/src-tauri" && env -u CARGO_HOME -u RUSTUP_HOME -u TUIC_TEST_FAKE_HOME "$@" NEXTEST_ENV="$out" \
    TUIC_TEST_TMP_ROOT="$fixture/nextest-root" sh scripts/nextest-test-tmp.sh)
  cat "$out"
}
written="$(nextest_env HOME="$real_home" TUIC_TEST_FAKE_HOME=1)"
grep -qxF "HOME=$fixture/nextest-root/home" <<<"$written" \
  || { echo "nextest setup script wrote no fake HOME:" >&2; printf '%s\n' "$written" >&2; exit 1; }
grep -qxF "CARGO_HOME=$real_home/.cargo" <<<"$written" \
  || { echo "nextest setup script did not pin CARGO_HOME:" >&2; printf '%s\n' "$written" >&2; exit 1; }
test -d "$fixture/nextest-root/home" || { echo "nextest setup script did not create the fake HOME" >&2; exit 1; }
written="$(nextest_env HOME="$real_home")"
if grep -qE '^(HOME|CARGO_HOME|RUSTUP_HOME)=' <<<"$written"; then
  echo "nextest setup script moved HOME without the flag:" >&2
  printf '%s\n' "$written" >&2
  exit 1
fi
# Unset HOME must not abort the POSIX `set -u` setup script either.
written="$(nextest_env -u HOME TUIC_TEST_FAKE_HOME=1)"
grep -qxF "HOME=$fixture/nextest-root/home" <<<"$written" \
  || { echo "nextest setup script with HOME unset wrote no fake HOME" >&2; exit 1; }

# Nothing was written into the "real" home.
if [ -n "$(find "$real_home" -mindepth 1 -print -quit)" ]; then
  echo "something wrote into the real HOME:" >&2
  find "$real_home" -mindepth 1 >&2
  exit 1
fi
