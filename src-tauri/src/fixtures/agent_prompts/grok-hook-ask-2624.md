# Grok native Ask with hooks (#2624-848f)

Recorded on 2026-11-01 from installed Grok 1.0.70 in an isolated HOME and GROK_HOME under `~/Gits/.tmp/tuic-2624b-ask`, with a disposable project. No user Grok configuration or credential store was read. OpenRouter supplied the model endpoint; its key was loaded only into the process environment from the authorized source.

Command: `grok --trust --model x-ai/grok-4.1-fast --always-approve --no-alt-screen --no-subagents --permission-mode default --disable-web-search --max-turns 1` followed by a request to ask for Red, Green or Blue through the Ask tool. The controller allowed one second for the radio paint to settle, then selected `z` and retained the free-text repaint. Capture stopped two seconds later; only the recorded owned child was killed and reaped. Geometry: 35 rows, 120 columns. Original read/write boundaries, order and monotonic timestamps are retained as TUICCAP2.

Actual hook stdin reports SessionStart, UserPromptSubmit, PreToolUse(`ask_user_question`), then Notification(`elicitation_dialog`, message `User question requested`). Ask is not a `permission_prompt`, even with PreToolUse active. The hook projection is `../grok_hooks_2624/ask.json`; identifiers are consistently redacted there, not in terminal bytes.

SHA-256: `7423d32ca9d6075d9e0b1c55998c57700c7df3f2ec74dbc8c73b7a7310f025aa`
