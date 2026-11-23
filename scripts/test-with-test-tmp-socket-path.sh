#!/usr/bin/env bash
# Catches: the wrapper's TMPDIR leaves no room for the Unix socket mbx binds inside
# it (`<TMPDIR>/mbx-session-XXXXXX/cache-agent.sock`), so mbx fails with "path must be
# shorter than SUN_LEN" and no Rust test runs (#2841-774f). Either the per-run root
# fits the budget, or (when mbx is installed) the wrapper refuses up front and names
# the limit; where no mbx is installed a long temp dir is accepted.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
scratch="$(mktemp -d "${TMPDIR:?}/tuic-socket-test.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
mbx_suffix="mbx-session-XXXXXX/cache-agent.sock"
limit=103
fail() { echo "socket-path: $*" >&2; exit 1; }

# A stand-in mbx on PATH, and a PATH with no mbx at all.
stub="$scratch/stub-bin"
mkdir -p "$stub"
printf '#!/bin/sh\nexit 0\n' > "$stub/mbx"
chmod +x "$stub/mbx"
without_mbx="$(. "$root/scripts/test-tmp-lib.sh" && tuic_path_without_mbx "$PATH")"

run_wrapper() { # <path> <host tmpdir> -- prints TMPDIR; stderr to $scratch/stderr
  env -u TUIC_TEST_TMP_ROOT -u TUIC_TEST_TMP_BASE PATH="$1" TUIC_TEST_HOST_TMPDIR="$2" \
    "$root/scripts/with-test-tmp.sh" sh -c 'printf "%s\n" "$TMPDIR"' 2>"$scratch/stderr"
}

# 1. The current temp dir, mbx installed: the socket fits, or the wrapper says why not.
if out="$(run_wrapper "$stub:$without_mbx" "$(cd "${TMPDIR%/}" && pwd -P)")"; then
  socket="$out$mbx_suffix"
  (( ${#socket} <= limit )) || fail "mbx socket path is ${#socket} bytes, over $limit: $socket"
else
  grep -q "socket path limit" "$scratch/stderr" || { cat "$scratch/stderr" >&2; fail "refused without naming the socket path limit"; }
fi

# 2. A long temp dir and mbx installed: the run dir moves to the short private per-user
#    fallback (mode 0700, owned by the caller), so the socket fits and the command runs.
long="$scratch/$(printf 'a%.0s' $(seq 1 70))"
mkdir -p "$long"
out="$(run_wrapper "$stub:$without_mbx" "$long")" || { cat "$scratch/stderr" >&2; fail "the wrapper refused a ${#long}-byte temp dir with mbx installed"; }
case "$out" in
  */tuic-"$(id -u)"/tuic-run.*) ;;
  *) fail "TMPDIR $out is not under the per-user fallback directory" ;;
esac
socket="$out$mbx_suffix"
(( ${#socket} <= limit )) || fail "fallback: mbx socket path is ${#socket} bytes: $socket"

# 2b. Even the fallback too long (parent override): refused up front, naming the limit.
if TUIC_TEST_FALLBACK_PARENT="$long" run_wrapper "$stub:$without_mbx" "$long" >/dev/null; then
  fail "the wrapper accepted a temp dir and a fallback that both exceed the budget"
fi
grep -q "socket path limit" "$scratch/stderr" || { cat "$scratch/stderr" >&2; fail "the refusal does not name the socket path limit"; }

# 2c. A fallback directory owned by someone else, or a symlink, is refused, never used.
fb_parent="$scratch/fb"
mkdir -p "$fb_parent"
ln -s "$scratch" "$fb_parent/tuic-$(id -u)"
if TUIC_TEST_FALLBACK_PARENT="$fb_parent" run_wrapper "$stub:$without_mbx" "$long" >/dev/null; then
  fail "the wrapper used a symlinked fallback directory"
fi

# 3. The same long temp dir with no mbx anywhere: nothing binds that socket, so it runs.
out="$(run_wrapper "$without_mbx" "$long")" || { cat "$scratch/stderr" >&2; fail "the wrapper refused a long temp dir although mbx is not installed"; }
case "$out" in
  "$long"/*|"$(cd "$long" && pwd -P)"/*) ;;
  *) fail "TMPDIR $out is not under the host temp dir $long" ;;
esac

# 4. A short temp dir: the run dir sits directly in it (no tuic-tests level) when the
#    usual <host>/tuic-tests/tuic-run.XXXXXX would not fit, and the socket fits.
short="$scratch/s"
mkdir -p "$short"
physical_short="$(cd "$short" && pwd -P)"
if (( ${#physical_short} + 1 + 15 <= 67 )); then
  out="$(run_wrapper "$stub:$without_mbx" "$physical_short")" || { cat "$scratch/stderr" >&2; fail "refused a short temp dir"; }
  socket="$out$mbx_suffix"
  (( ${#socket} <= limit )) || fail "short temp dir: mbx socket path is ${#socket} bytes: $socket"
else
  echo "socket-path: note: ${#physical_short} bytes is already over the budget here; the short-dir case is UNCHECKED" >&2
fi
