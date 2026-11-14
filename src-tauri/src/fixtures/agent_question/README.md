# Question tool transcripts — 2633-cbd3

Real transcript rows for question recovery. No row was written by hand.

## Claude Code 2.1.316

Recorded on macOS on 2026-11-03 from the installed `claude` (2.1.316), model
Sonnet with low effort, in an owned Python PTY at 40 rows by 120 columns. The CLI
used `--setting-sources ''`, `--strict-mcp-config`, only the `AskUserQuestion`
tool and a system prompt that asks two fixed questions. Logging hooks copied
native stdin; they made no decision. The recorder removed inherited
`CLAUDE_CODE_*` variables, because an inherited child-session marker turns
transcript saving off. Scratch folders, hook logs and raw PTY streams remain
under `~/Gits/.tmp/tuic-2633/cap/`.

- `claude-answer-then-reopen-2.1.316.jsonl`: Enter answered both questions and
  submitted them; Claude replied `DONE`; a second human prompt opened the same
  two questions again; the recorder then stopped the process while the second
  question was open. The last `tool_use` therefore has no result.
- `claude-escape-2.1.316.jsonl`: Escape cancelled the open question. Claude wrote
  an `is_error` tool result and `[Request interrupted by user for tool use]`.

Projection keeps only `user`, `assistant` and the `api-request-blob` rows that
carry a `tool_use` (Claude's request replay of an already answered call). It
removes `cwd`, `gitBranch` and `slug`. Other values, ids and order are unchanged.
The native PreToolUse `tool_use_id` in the hook log equals the transcript
`tool_use.id`.

## Codex 0.166.0

Projections of the rollouts that the 2626-f570 probe recorded (see
`../codex_tools_2626/README.md`): `codex-answered-0.166.0.jsonl` from
`~/Gits/.tmp/codex-2626-capture/` and `codex-interrupted-0.166.0.jsonl` from
`~/Gits/.tmp/codex-2626-interrupt/`. Projection keeps `session_meta` without
`base_instructions`, every `event_msg`, and the `function_call` /
`function_call_output` response items. It drops message prose and
`turn_context`. The native hook `tool_use_id` equals the rollout `call_id`.
A rollout is append-only, so a prefix of a fixture is a real earlier file state.

| File | SHA-256 |
| --- | --- |
| claude-answer-then-reopen-2.1.316.jsonl | 5f65ce2c3ee40563c1cc553abc8466fb6cc229fbe3b1f922c8c78ab8bfd554d6 |
| claude-escape-2.1.316.jsonl | b5820d2d577c3de36ffca6d612e6a6b159c402a1fe1924614a58fdb5c217053e |
| codex-answered-0.166.0.jsonl | f74ce49610aebb534828ab70bd8628d130a2d7f98ca6200ef86d8e177aa49170 |
| codex-interrupted-0.166.0.jsonl | d9a81bde4489451083df0aae529714ef9c1c3dfb758fa413fd1fa17848a740ad |

Windows was not exercised.
