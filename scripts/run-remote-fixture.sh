#!/usr/bin/env bash
# Fixture only: binary MUST be built with --no-default-features --features tuic-core/test-support.
# This uses the existing test config fallback, so no production config directory is written.
# Share this launcher for remote MCP, detection and browser-grid verification.
set -euo pipefail
fixture_binary="${1:?usage: run-remote-fixture.sh <test-support binary> [port] [instance]}"
fixture_port="${2:-19877}"
fixture_instance="${3:-tuic-remote-fixture}"
if [[ ! "$fixture_instance" =~ ^[a-z0-9][a-z0-9-]*$ ]]; then
  echo "Invalid fixture instance" >&2; exit 2
fi
# Scratch lives under the caller's temp dir (or TUIC_FIXTURE_ROOT), never $HOME.
fixture_root="${TUIC_FIXTURE_ROOT:-${TMPDIR:-/tmp}}"
fixture_root="${fixture_root%/}"
# A named instance binds <TMPDIR>/tuic-mcp-<16 hex>.sock (30 bytes after the
# '/'), and a separately launched `tuic` client must export the same TMPDIR to
# find it. The longest socket path is tuic-ipc's UNIX_SOCKET_PATH_MAX (read from
# its source: one definition), so the fixture TMPDIR may be at most that minus
# '/' minus the socket name. First of: TUIC_FIXTURE_TMPDIR; a private
# per-instance <root>/tuic-rf-<hash>; the caller's temp dir itself.
# Nothing falls back to /tmp: over budget, this fails naming every candidate.
# shellcheck source=test-tmp-lib.sh
. "$(dirname "$0")/test-tmp-lib.sh"
socket_max="$(tuic_unix_socket_path_max "$(cd "$(dirname "$0")/.." && pwd)")"
socket_name="tuic-mcp-0000000000000000.sock" # tuic_ipc::named_socket_path's shape
socket_budget=$((socket_max - 1 - ${#socket_name}))
instance_hash="$(printf '%s' "$fixture_instance" | cksum | cut -d ' ' -f 1)"
if [[ -n "${TUIC_FIXTURE_TMPDIR:-}" ]]; then
  candidates=("${TUIC_FIXTURE_TMPDIR%/}")
else
  candidates=("$fixture_root/tuic-rf-$instance_hash" "$fixture_root")
fi
fixture_tmpdir=
for candidate in "${candidates[@]}"; do
  if (( ${#candidate} <= socket_budget )); then fixture_tmpdir="$candidate"; break; fi
done
if [[ -z "$fixture_tmpdir" ]]; then
  {
    echo "No fixture TMPDIR fits the named socket: it needs at most $socket_budget bytes (${socket_max}-byte socket path minus /tuic-mcp-<16 hex>.sock)."
    for candidate in "${candidates[@]}"; do echo "  $candidate: ${#candidate} bytes"; done
    echo "Set TUIC_FIXTURE_TMPDIR to a private dir of at most $socket_budget bytes (or use a shorter TMPDIR)."
  } >&2
  exit 2
fi
# Only a directory we create here is made private; the caller's own temp dir
# is used as it is.
[[ -d "$fixture_tmpdir" ]] || mkdir -p -m 0700 "$fixture_tmpdir"
if [[ -L "$fixture_tmpdir" || ! -O "$fixture_tmpdir" ]]; then
  echo "Fixture TMPDIR $fixture_tmpdir is not a directory owned by you" >&2; exit 2
fi
if (( ${#fixture_tmpdir} > 61 )); then
  echo "Note: fixture TMPDIR is ${#fixture_tmpdir} bytes; a second fixture of the same instance could not bind its tuic-mcp-<hash>-<pid>.sock alternate (needs at most 61)." >&2
fi
export TMPDIR="$fixture_tmpdir/"
export TUIC_TEST_TMP_ROOT="$fixture_root/tuic-remote-fixture/$fixture_instance/"
export PAGER=cat GIT_PAGER=cat
mkdir -p "$TUIC_TEST_TMP_ROOT"
echo "Fixture TMPDIR: $TMPDIR (export TMPDIR=$TMPDIR for a separately launched tuic client)"
export TUIC_PORT="$fixture_port"
export TUIC_PAIRING_TOKEN="${TUIC_FIXTURE_TOKEN:-tuic-1421-fixture}"
echo "Fixture URL: http://127.0.0.1:$fixture_port (instance $fixture_instance)"
exec "$fixture_binary" --instance "$fixture_instance" --bind 127.0.0.1 --no-agent-configs
