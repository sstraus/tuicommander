# 1Password references as a secret provider

Story 2914-b125. Status: evaluation and mock only. Nothing is wired to the `secret` tool. Boss decides.

## Request

Store an `op://vault/item/field` reference (plus account), resolve it with `op read` when a secret is used or an agent is spawned with a scope. The value lives only in the child env: never on disk, in a transcript or in logs.

## What exists today (verified in source)

- The `secret` store (`src-tauri/src/secrets/`) is **in memory only**. Form values and `put` scopes are `Zeroizing<String>` in `SecretStore`; nothing is written to the keychain. The OS keychain holds other credentials (upstream MCP, GitHub), not these secrets. So "keychain-only" is today "memory-only, re-entered after restart".
- `put` stores names and values under a scope with a TTL. `agent spawn scope=...` calls `scope_environment` and `bind_scope` (`mcp_http/agent_routes.rs`), applies the values to the child command, and masks the session output.
- Values are never returned by any tool. An approved command can still exfiltrate; this is an accepted limit.

## Fit

A provider only has to fill the same scope entry that `put` fills. The mock does exactly this: `SecretStore::put_references` resolves every reference, then calls the existing `put`. Spawn, TTL, masking and the child env path stay unchanged. The reference itself is not stored in the scope.

## Security boundary

- Who may resolve: the party that may call `put`, which is any MCP caller of the backend. If the tool accepts references, an agent can name **any** `op://` path the user's `op` session can read. The reference is therefore a capability. It must be user-approved: either a private form where the user types or approves the reference, or an allowlist of references stored in the settings. Recommended: user-approved form only (same card as `request`).
- The agent never sees the value. It sees the reference path, which names vault, item and field. That is metadata, not a secret, but the mock keeps it out of logs anyway.
- `op` runs with stdin closed. Account and reference are validated before spawn (no leading `-`, no control characters), so an agent cannot add a CLI flag.
- Failure never falls back. Each failure is a typed error and stores no scope (tested: failed resolve leaves `scope_environment` missing).
- Out of scope per the threat model: a local attacker or a hostile agent of the same user. Any such agent can already run `op` itself; the reference gate only matters because the agent is *not* meant to see values.

## Biometric prompt when the user is away

`op read` triggers the desktop app or Touch ID prompt. If nobody answers, `op` blocks until its own timeout. The mock bounds the call with a caller-chosen timeout (tests use 300 ms; production value not chosen), kills the child, and returns `Timeout`; a dismissed prompt returns `Locked`. The user-away case therefore fails closed and the spawn fails with a clear reason. Two consequences:

- Resolve at `put` time, not at spawn, if the person who approves is present at `put` (a form they just approved) and absent later. Resolve at spawn only when the prompt should appear at spawn time; then the spawn can stall for the timeout.
- A prompt that appears with no visible cause is confusing. The error text should say which reference and account needed approval, and the Telegram/mobile card could carry that. Not built.

Unknown: how long real `op` blocks, and whether the prompt can be raised when the desktop session is locked. Both need a manual check with a real vault; no test may call it.

## Cost / benefit against keychain-only (memory-only today)

| | Memory store (today) | `op` provider |
|---|---|---|
| Survives restart | no, re-enter | yes, reference persists |
| Value at rest in TUIC | none | none |
| Value at rest elsewhere | none | user's vault (already) |
| Needs user present | at entry | at each resolve, if the vault locks |
| New dependency | none | `op` CLI on the same host; version drift |
| Error surface | few | 5 typed errors plus classification |
| Cost | none | one adapter (~150 lines), settings to store references |

Risk: error classification matches `op` stderr wording, which is **not verified** against a real CLI. A wording change maps to `Failed` (still closed, only a less precise message). A persistent keychain adapter (third adapter) would give restart survival without a new CLI dependency, and is not built.

## Recommendation

Adopt, behind an explicit user approval of each reference, as a second provider. It is the only option that removes re-entry after a restart while storing no value in TUIC, and the failure mode is closed. Before landing:

1. Check the real `op` stderr wording and block time once by hand; adjust `classify`.
2. Decide resolve-at-put (recommended) or resolve-at-spawn.
3. Add the approval form for references and the settings entry; do not let `put` accept a bare `op://` from an agent.
4. Add `provider` actions to the tool schema, with the IPC/HTTP parity and docs in `docs/sync-matrix.md`.

If Boss prefers no new dependency, a keychain-backed adapter on the same port is the alternative.

## Landed (2026-11-08)

Boss reversed the evaluation-only order. `secret put` now accepts `references` through the same port (the `op` adapter, then `put_references` fills the scope store; `op` reads are drained under one deadline). A store adapter and a `stored` reference kind were removed after review: no user, and no keychain adapter exists. Open: recommendation 1 (real `op` wording) and 3 (no approval form yet: any caller of the `secret` tool can name an `op://` reference; the vault's own biometric prompt is the only gate) are NOT done.

## Mock

`src-tauri/src/secrets/provider.rs` (port, errors, store adapter, `put_references`), `provider_op.rs` (op adapter, process runner), `fixtures/fake-op.sh` (fake CLI), `provider_tests.rs` (tests; none calls the real `op`).
