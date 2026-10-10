#!/usr/bin/env bash
# The nextest `fixture-bins` setup script must run for tests that spawn a bin
# fixture and for nothing else (#1308-50c9). Each case swaps the real build
# command in .config/nextest.toml for a marker, so the scoping itself is what
# is observed, not a cargo build.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
tauri="$root/src-tauri"
work="$(mktemp -d "${TMPDIR:-$root/.tmp}/nextest-fixture-scope.XXXXXX")"
trap 'rm -rf "$work"' EXIT

marker="$work/fixture-bins-ran"
config="$work/nextest.toml"
sed "s|sh scripts/build-fixture-bins.sh|touch $marker|" "$tauri/.config/nextest.toml" > "$config"
grep -q "touch $marker" "$config" || { echo "nextest.toml no longer names build-fixture-bins.sh" >&2; exit 1; }

# A filter that selects nothing leaves no marker either, so a run must have
# executed tests for its "marker absent" verdict to mean anything. CI colors
# nextest's output, which puts SGR codes around the count the grep matches.
run() {
  (cd "$tauri" && cargo nextest run --color never --config-file "$config" --no-fail-fast "$@" >"$work/run.log" 2>&1) || true
  grep -qE 'Summary .* [1-9][0-9]* tests? run' "$work/run.log" || { echo "run executed no tests: $*" >&2; cat "$work/run.log" >&2; exit 1; }
}

# Catches: a unit test that spawns a bin fixture but is missing from the setup filter.
consumers="$(cd "$tauri" && grep -rlE 'tuic-(acp|mcp)-fixture' src crates/*/src | LC_ALL=C sort | tr '\n' ' ')"
[ "$consumers" = "src/acp_chat.rs src/mcp_proxy/stdio_client.rs src/state.rs " ] || {
  echo "lib unit tests consuming bin fixtures changed: $consumers" >&2
  echo "update the fixture-bins filters in src-tauri/.config/nextest.toml and this list" >&2
  exit 1
}

# Catches: every targeted run paying the fixture build.
run -p tuic-git --lib -E 'test(/./)'
if [ -e "$marker" ]; then
  echo "fixture-bins setup ran for a tuic-git-only run" >&2
  exit 1
fi

# Catches: 1269 regressing, bin fixtures not built before their consumers.
run -p tuicommander --no-default-features --lib -E 'test(=mcp_proxy::stdio_client::tests::is_alive_returns_false_after_process_exits) | test(=state::tests::live_acp_permission_reaches_one_subscribed_push_service)'
[ -e "$marker" ] || { echo "fixture-bins setup did not run for a fixture consumer" >&2; cat "$work/run.log" >&2; exit 1; }
rm -f "$marker"

# Catches: the stdio_client group dropped from the filter while state.rs stays.
run -p tuicommander --no-default-features --lib -E 'test(/^mcp_proxy::stdio_client::tests::/)'
[ -e "$marker" ] || { echo "fixture-bins setup did not run for stdio_client tests" >&2; exit 1; }
rm -f "$marker"

# Catches: ACP chat losing its fixture setup while the other consumers keep it.
run -p tuicommander --no-default-features --lib -E 'test(/^acp_chat::tests::custom_chat_/)'
[ -e "$marker" ] || { echo "fixture-bins setup did not run for ACP chat tests" >&2; exit 1; }
rm -f "$marker"

# Catches: the filter widened to the whole lib binary.
run -p tuicommander --no-default-features --lib -E 'test(=state::worktree_event_payloads::the_created_payload_spells_its_wire_fields)'
[ ! -e "$marker" ] || { echo "fixture-bins setup ran for a lib run with no consumer" >&2; exit 1; }
