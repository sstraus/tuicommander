# Codex native tool receipts — 2626-f570

Recorded on 2026-11-02 from `/opt/homebrew/bin/codex`, `codex-cli 0.166.0`, on macOS arm64.
`hooks.jsonl` and `interrupt.jsonl` are the original hook stdin JSONL, without synthesized vendor events.

The probe used an isolated `CODEX_HOME` and project under `~/Gits/.tmp/`.
A local deterministic HTTP Responses SSE server supplied one `exec_command`
(`printf tool-capture`) and one plan-mode `request_user_input` call. Actual
`codex app-server` turns produced the hooks. The probe trusted only its scratch
hook definitions through `hooks/list` and `config/batchWrite`. It answered Blue
through the native user-input RPC. A second probe interrupted an open question
with `turn/interrupt`, recording `Interrupt` without a post-tool event.
There were no paid model requests, authentication, or daily configuration edits.
All probe processes and HTTP servers were stopped.

Raw sent/received protocol and probe scripts are retained at
`~/Gits/.tmp/codex-2626-capture/`, `~/Gits/.tmp/codex-2626-interrupt/` and
`~/Gits/.tmp/codex-2626-source/capture{,_interrupt}.py`.
The first capture made four local model requests; the interruption capture made one.
These are hook/RPC receipts. The separate `agent_prompts/codex-native-tools-2626.tcap` records the actual shell emitter output in a harness PTY, with its tty sink explicitly redirected; its provenance documents this boundary. Neither capture proves native rendered controls.
Existing Codex terminal captures cover screen-derived choices and approvals.
Tests substitute only session/turn ownership and paths for their isolated topology.

Native `request_user_input` is exposed as PreToolUse/PostToolUse, with
`tool_name=request_user_input`, `tool_use_id`, `turn_id` and `tool_input.questions`.
TUIC uses the structured question text and matching settlement, retaining native
screen-derived answer controls. It does not derive approval ownership from
PermissionRequest: CLI 0.166.0 invokes that event before choosing a reviewer and
supplies no owner or request id. That separate contract is tracked by 2695-41da.
Source: https://github.com/openai/codex/tree/rust-v0.166.0/codex-rs

SHA-256:

- `hooks.jsonl`: `86c92b618a5372ebd274af7a2da516845f8a54ed2ca9da2767cc375d6073be39`
- `interrupt.jsonl`: `d1c8a1e6a90f506d9841b198ae3dee76fb75b8f44fb03850b578da0b12691f1d`
