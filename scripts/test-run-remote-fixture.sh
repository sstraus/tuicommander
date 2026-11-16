#!/usr/bin/env bash
# run-remote-fixture.sh must never write under $HOME, must keep the named
# socket's TMPDIR within the sun_path budget, must honour TUIC_FIXTURE_TMPDIR
# and must print the TMPDIR it chose. A fake binary stands in for tuic-remote.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
base="$(. "$root/scripts/test-tmp-lib.sh" && tuic_test_tmp_root "$root")"
host="$(. "$root/scripts/test-tmp-lib.sh" && tuic_test_host_tmpdir)"
# The budget comes from the one definition (tuic-ipc), never restated here; the
# pins below fail if that value or the name shape moves without this test knowing.
socket_max="$(. "$root/scripts/test-tmp-lib.sh" && tuic_unix_socket_path_max "$root")"
test "$socket_max" = 103 || { echo "UNIX_SOCKET_PATH_MAX is $socket_max, expected 103" >&2; exit 1; }
socket_name="tuic-mcp-0000000000000000.sock" # tuic_ipc::named_socket_path's shape
budget=$((socket_max - 1 - ${#socket_name}))
test "$budget" = 72 || { echo "fixture budget is $budget, expected 72" >&2; exit 1; }
fixture="$(mktemp -d "${base%/}/tuic-remote-fixture-test.XXXXXX")"
instance="tuic-fixture-test-$$"
made="$host/tuic-rf-$(printf '%s' "$instance" | cksum | cut -d ' ' -f 1)"
mine=
had_rf_parent=0
[[ -e "$host/tuic-remote-fixture" ]] && had_rf_parent=1
cleanup() {
  chmod -R u+rwX "$fixture"
  rm -rf "$fixture"
  # Only what this test's launches created in the host temp dir.
  for dir in "$made" "$mine"; do
    [[ -n "$dir" && "$dir" != "$host" && -d "$dir" && -O "$dir" ]] && rm -rf -- "$dir"
  done
  [[ -d "$host/tuic-remote-fixture/$instance" ]] && rm -rf -- "$host/tuic-remote-fixture/$instance"
  if (( ! had_rf_parent )); then rmdir "$host/tuic-remote-fixture" 2>/dev/null || true; fi
}
trap cleanup EXIT

home="$fixture/home"
mkdir -p "$home"
chmod 0555 "$home"
fake="$fixture/fake-tuic-remote"
printf '#!/bin/sh\nprintf "TMPDIR=%%s\\nROOT=%%s\\n" "$TMPDIR" "$TUIC_TEST_TMP_ROOT"\n' > "$fake"
chmod +x "$fake"

launch() {
  env -u TUIC_FIXTURE_TMPDIR -u TUIC_FIXTURE_ROOT HOME="$home" "$@" \
    bash "$root/scripts/run-remote-fixture.sh" "$fake" 19999 "$instance"
}
field() { printf '%s\n' "$1" | sed -n "s/^$2=//p"; }
no_tmp_mention() {
  if printf '%s\n' "$1" | grep -Eq '(^|[^a-z])/tmp'; then
    echo "launcher mentions a /tmp fallback: $1" >&2; exit 1
  fi
}

# Catches: a long caller TMPDIR silently falling back to /tmp (or reaching a
# mystery SUN_LEN bind error) instead of failing with the budget.
long_tmp="$fixture/a-deliberately-long-caller-temp-dir-that-cannot-hold-the-socket"
mkdir -p "$long_tmp"
if out="$(launch TMPDIR="$long_tmp/" 2>&1)"; then
  echo "an over-budget caller TMPDIR was accepted: $out" >&2; exit 1
fi
for needle in "at most $budget bytes" "$long_tmp: ${#long_tmp} bytes" "$long_tmp/tuic-rf-" TUIC_FIXTURE_TMPDIR; do
  printf '%s\n' "$out" | grep -Fq -- "$needle" \
    || { echo "budget failure does not name '$needle': $out" >&2; exit 1; }
done
no_tmp_mention "${out//"$long_tmp"/}"

# Catches: not printing the TMPDIR a separately launched client must export,
# or choosing one past the budget, under the real caller temp dir.
if (( ${#host} <= budget )); then
  out="$(launch TMPDIR="$host/")"
  chosen="$(field "$out" TMPDIR)"
  printf '%s\n' "$out" | grep -Fq "Fixture TMPDIR: $chosen" \
    || { echo "launcher did not print its TMPDIR" >&2; printf '%s\n' "$out" >&2; exit 1; }
  case "$chosen" in
    "$made/" | "$host/") ;;
    *) echo "fixture TMPDIR $chosen is neither $made/ nor $host/" >&2; exit 1 ;;
  esac
  (( ${#chosen} - 1 <= budget )) || { echo "fixture TMPDIR $chosen is over $budget bytes" >&2; exit 1; }
  case "$(field "$out" ROOT)" in
    "$host/tuic-remote-fixture/$instance/") ;;
    *) echo "config fallback root left the caller's temp dir: $out" >&2; exit 1 ;;
  esac
else
  echo "SKIP: host temp dir $host is ${#host} bytes, over the $budget-byte fixture budget"
fi

# Catches: clobbering a TMPDIR the caller chose for a separately launched client.
if (( ${#host} + 11 <= budget )); then
  mine="$(mktemp -d "$host/rft.XXXXXX")"
  rmdir "$mine"
  out="$(launch TMPDIR="$long_tmp/" TUIC_FIXTURE_TMPDIR="$mine")"
  test "$(field "$out" TMPDIR)" = "$mine/" || { echo "TUIC_FIXTURE_TMPDIR ignored: $out" >&2; exit 1; }
  test "$(stat -f '%Lp' "$mine" 2>/dev/null || stat -c '%a' "$mine")" = 700 \
    || { echo "fixture TMPDIR is not private" >&2; exit 1; }
elif (( ${#host} <= budget )); then
  # Nothing below a budget-sized host temp dir fits; the override is still used as is.
  out="$(launch TMPDIR="$long_tmp/" TUIC_FIXTURE_TMPDIR="$host")"
  test "$(field "$out" TMPDIR)" = "$host/" || { echo "TUIC_FIXTURE_TMPDIR ignored: $out" >&2; exit 1; }
fi

# Catches: an over-budget override failing later as a mystery bind error.
if launch TUIC_FIXTURE_TMPDIR="$long_tmp/and/even/longer/than/that" >/dev/null 2>&1; then
  echo "an over-budget TUIC_FIXTURE_TMPDIR was accepted" >&2; exit 1
fi

if [ -n "$(find "$home" -mindepth 1 -print -quit)" ]; then
  echo "the fixture launcher wrote below HOME" >&2; exit 1
fi
