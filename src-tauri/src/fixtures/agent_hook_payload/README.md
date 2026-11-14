# Native question payload fixtures, 2634-ea5b

Each `.jsonl` file holds the native hook stdin, one JSON object per line, that
Claude Code 2.1.316 gave to the hooks of one recorded session on macOS,
2026-11-03. The matching PTY captures are in `../agent_prompts/` (TUICCAP2,
40 rows by 120 columns, output and input with real chunk boundaries and timing).
The TUIC session id of the recording is `rec-2634-owner`.

`record_claude_2634.py <ask|plan_yes|plan_no>` made both. It installs TUIC's
generated Claude hook commands (a port of `agent_hook.rs::claude_hook_map`)
plus a logger hook, runs `claude` (sonnet, effort low, no setting sources, no
MCP servers, only the tool under test) in a PTY with a controlling terminal,
and types the answer.

| File | SHA-256 | Native input |
|---|---|---|
| `claude-ask-multiselect-2.1.316.jsonl` | `5dbf769e5a8389ef1764a0b5e58b770778e980bb7812735e47e40ab5788610f2` | Enter (question 1), Enter (toggle Small), Down x3, Enter (Submit), Enter (Submit answers) |
| `claude-plan-approve-2.1.316.jsonl` | `7761620e9d2ab1b44cc7459fcad15abd9155b14a6946a383c22d87e0d389e891` | Enter (approve) |
| `claude-plan-reject-2.1.316.jsonl` | `5c5bc65f64cd795b8f7cdc1919bda6aec692a3038b62e0269500fee7ef17b24e` | Escape (refuse) |
| `../agent_prompts/claude-ask-multiselect-2634.tcap` | `d54286ccdf9aa32d3a99fd043bda848ae12e5304486c374418e22f0a6653a28c` | as above |
| `../agent_prompts/claude-plan-approve-2634.tcap` | `9d18971544c7a41d4f03d4bc871851a00dd02bc9d37705945591d6abfccd2656` | as above |
| `../agent_prompts/claude-plan-reject-2634.tcap` | `1ce0f3d91e0d6780ec4cae521884d82b10dd61cbeafc298264b08ebd65b655ec` | as above |
| `../agent_prompts/ego-question-answer-2634.tcap` | `56830371ee4e575aae41470dd52983e8da1d39ddedfb7a38aa6a46d8c15e0583` | Enter (answer), `/quit` |

Recorded facts:

- AskUserQuestion fires PreToolUse, PermissionRequest (no `tool_use_id`),
  Notification `permission_prompt`, and PostToolUse after the answer.
- ExitPlanMode has `tool_input: {}`. The plan text is not in the payload.
- Escape on a plan sends no PostToolUse and no Stop. The screen shows
  `User rejected Claude's plan:` above a ready composer.

The ego capture comes from ego 0.4.3 (`--tools ask_question`, local fixture
model) at 32 rows by 110 columns. Its source log kept output bytes only
(SHA-256 `c40fd63aaa84cfb75256c3d0d741913bad09c2522eb4482424655e544a67f8a3`).
`ego_raw_to_tcap_2634.py` reconstructed the chunk boundaries (one record ends
after each OSC 7770 frame), put the answer Enter before the `modal_end` frame
and `/quit` after `turn_end`, and dropped the cursor-position replies. The
timing is synthetic. Windows was not recorded.
