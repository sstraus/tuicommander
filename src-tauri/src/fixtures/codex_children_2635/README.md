# Codex native child hooks — 2635-8052

Recorded on 2026-11-05 from `/opt/homebrew/bin/codex`, `codex-cli 0.166.0`, on macOS arm64.
`hooks.jsonl` is the original hook stdin JSONL, in arrival order, without synthesized vendor events.

The probe is the 2626-f570 harness with `SubagentStart` and `SubagentStop` added:
an isolated `CODEX_HOME` and project under `~/Gits/.tmp/`, and a local deterministic
HTTP Responses SSE server. The server answered the lead with one `multi_agent_v1`
`spawn_agent` call, the child with one message, the lead with one `wait_agent` call
and then one message. Actual `codex app-server` turns produced the hooks. The probe
trusted only its scratch hook definitions through `hooks/list` and `config/batchWrite`.
There were four local model requests, no paid model requests, no authentication and
no daily configuration edits. All probe processes and the HTTP server were stopped.

Raw sent/received protocol and the probe script are retained at
`~/Gits/.tmp/codex-2635-capture/` and `~/Gits/.tmp/codex-2635-source/capture.py`.

What the recording shows:

- `SubagentStart` and `SubagentStop` carry the lead's `session_id`, the child's own
  `turn_id`, `agent_id` and `agent_type`; `SubagentStop` adds `agent_transcript_path`
  and `last_assistant_message`, and its `transcript_path` is the lead's rollout.
- The child's own `UserPromptSubmit` arrives with the lead's `session_id`, the child's
  `turn_id`, `agent_id` and the child's rollout as `transcript_path`. It is not a new
  lead turn.
- The child sends no `Stop`; `SubagentStop` is its only settlement. The lead `Stop`
  follows the lead's `wait_agent` `PostToolUse`.

SHA-256: `hooks.jsonl` `9d5253c3aa4d24f23dbc36f024055e04b79fd5bd670283a7098752c9f7907543`
