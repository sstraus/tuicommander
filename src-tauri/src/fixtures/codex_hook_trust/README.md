# Codex native trust evidence

Captured on 2026-11-01 from `/opt/homebrew/bin/codex`, `codex-cli 0.166.0`,
macOS arm64. All processes used the isolated
`/Users/stefano.straus/Gits/.tmp/codex-hook-trust-2625/home` as `CODEX_HOME`.
No daily config, credentials or hook files were read or changed.

`codex-0.166.0.jsonl` is the actual sent/received JSONL from a bounded Python
stdio client. It initialized with experimental API support and sent:

1. `hooks/list` for the scratch home: one probe hook and one unrelated
   user hook, both untrusted. The command probe carries the TUIC sentinel but
   does not execute or reproduce production terminal behavior.
2. `config/read` with layers: native user config version and path.
3. `config/batchWrite`: one quoted hook key's `trusted_hash`, copied unchanged
   from `hooks/list`, with the user-layer `expectedVersion` from `config/read`.
   No local hash calculation was used.
4. `hooks/list`: the probe is trusted; the unrelated hook remains untrusted.
5. After changing only the probe command on disk, `hooks/list`: the probe is
   modified with a different native hash. Its old trust does not authorize it.
6. `config/batchWrite` with a stale expected version: exact native
   `configVersionConflict` response, no write.

The scratch config retained `model = "preserve-user-model"` and the unrelated
`hooks.state.unrelated.trusted_hash = "sha256:preserve-user-trust"` after the
successful write and the rejected write. The probe process was killed and
reaped in a `finally` block; no model turn or authentication was needed.

`profile-unsupported.txt` records the exact stderr from
`CODEX_HOME=<scratch> codex app-server --profile trust-probe` (exit 2).
The implementation verifies the base user configuration and does not claim
trust verification for profile-specific definitions.

SHA-256:

- `codex-0.166.0.jsonl`: `ffcd24eb938eddaf43fa0fab4ba739b7cd9efb5ae9312fc53aa2cc0dc2473945`
- `profile-unsupported.txt`: `8bc3aad7c32c9d76d6972cd1ad422734e6502d399d162731c0bb13f6fea3ee62`

These are protocol fixtures, not PTY captures. This change adds no awaiting
source and does not change ego PTY or ACP lifecycle ownership. Windows execution
is not claimed by this macOS capture.
