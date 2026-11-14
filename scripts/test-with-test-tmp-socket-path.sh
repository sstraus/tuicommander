#!/usr/bin/env bash
# Catches: the wrapper's TMPDIR in a deep checkout leaves no room for the Unix
# socket mbx binds inside it (`<TMPDIR>mbx-session-XXXXXX/cache-agent.sock`),
# so mbx fails with "path must be shorter than SUN_LEN" and no Rust test runs
# (#2841-774f). Either the scratch root fits the budget, or the wrapper refuses
# up front and names the limit.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
test_tmp_base="${TMPDIR:-$root/.tmp/tuic-tests}"
mkdir -p "$test_tmp_base"
fixture="$(mktemp -d "${test_tmp_base%/}/tuic-socket-test.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT
deep="$fixture/a-checkout-path-long-enough-to-push-the-scratch-root-past-the-socket-limit/repo"
git init --quiet "$deep"
mkdir -p "$deep/scripts"
cp "$root/scripts/with-test-tmp.sh" "$deep/scripts/with-test-tmp.sh"

# What mbx appends to TMPDIR for its session socket, and the macOS sun_path
# capacity without the terminating NUL.
mbx_suffix="mbx-session-XXXXXX/cache-agent.sock"
limit=103

set +e
out="$(env -u TMPDIR -u TMP -u TEMP "$deep/scripts/with-test-tmp.sh" \
  sh -c 'printf "%s\n%s\n" "$TMPDIR" "$TUIC_TEST_TMP_ROOT"' 2>"$fixture/stderr")"
status=$?
set -e
if [ "$status" -eq 0 ]; then
  tmpdir="$(printf '%s\n' "$out" | sed -n 1p)"
  scratch_root="$(printf '%s\n' "$out" | sed -n 2p)"
  socket="$tmpdir$mbx_suffix"
  if [ "${#socket}" -gt "$limit" ]; then
    echo "mbx socket path is ${#socket} bytes, over $limit: $socket" >&2
    exit 1
  fi
  case "$tmpdir" in
    "$scratch_root"/*) ;;
    *) echo "TMPDIR $tmpdir is outside the test temp root $scratch_root" >&2; exit 1 ;;
  esac
else
  grep -q "socket path limit" "$fixture/stderr" || {
    echo "the wrapper failed without naming the socket path limit:" >&2
    cat "$fixture/stderr" >&2
    exit 1
  }
fi

# Catches: on a GitHub runner the checkout root is over the budget and no
# ~/Gits exists, so the wrapper refused every CI test job (#3061-4580). With
# RUNNER_TEMP set it must run inside it. The runner's own temp dir is short;
# a local run uses a short directory under ~/Gits/.tmp instead.
short_parent="${RUNNER_TEMP:-$HOME/Gits/.tmp}"
mkdir -p "$short_parent"
runner_temp="$(mktemp -d "${short_parent%/}/rXXXX")"
trap 'rm -rf "$fixture" "$runner_temp"' EXIT
runner_home="$fixture/runner-home"
mkdir -p "$runner_home"
if ! out="$(env -u TMPDIR -u TMP -u TEMP HOME="$runner_home" RUNNER_TEMP="$runner_temp" \
  "$deep/scripts/with-test-tmp.sh" sh -c 'printf "%s\n" "$TMPDIR"' 2>"$fixture/stderr")"; then
  echo "the wrapper refused a long runner checkout although RUNNER_TEMP was set:" >&2
  cat "$fixture/stderr" >&2
  exit 1
fi
case "$out" in
  "$runner_temp"/*) ;;
  *) echo "runner TMPDIR $out is outside RUNNER_TEMP $runner_temp" >&2; exit 1 ;;
esac
socket="$out$mbx_suffix"
if [ "${#socket}" -gt "$limit" ]; then
  echo "runner mbx socket path is ${#socket} bytes, over $limit: $socket" >&2
  exit 1
fi
