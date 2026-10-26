# Session usage DTO provenance

`ego-v3.json` is copied byte-for-byte from ego `crates/ego-cli/src/usage.rs`,
`the_json_form_is_unchanged_by_the_text_view`, on 2026-10-23.
It is a producer-owned serialization fixture, not a live provider call.
The consumer tests construct session identities around its existing `UsageLines`
shape to test exact-row selection. Token and cost field spellings come from the
producer fixture. No credential or provider response was read.

## Codex profile syntax

On 2026-10-23 critic-2150r1 ran the installed CLI without provider requests:

- `codex app-server --profile work --help` exited 2. Its captured stderr is
  `codex-profile-after.err`.
- `codex --profile work app-server --help` exited 0. Its captured stdout is
  `codex-profile-before-help.txt`.

The process-boundary regression uses this root-option grammar in a temporary
fake CLI. Its quota response is synthetic; it does not represent a live account.
