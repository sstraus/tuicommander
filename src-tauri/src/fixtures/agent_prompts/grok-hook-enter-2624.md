# Grok bare-Enter approval capture (#2624-848f)

Captured on 2026-11-01 from installed Grok 1.0.70 (f50904eaee6c), on a real 35x120 PTY. TUICCAP2 retains original output reads, input writes and monotonic microsecond offsets.

Command: `grok --no-alt-screen --no-subagents --permission-mode default --disable-web-search --max-turns 1 'Use the shell tool to run printf 2624. Then reply only OK.'`

The disposable project's PreToolUse hook returned an ask decision. A physical CR accepted the selected approval. Native receipts: UserPromptSubmit, SessionStart, PreToolUse, Notification(permission_prompt), PostToolUse and Stop. The redacted stdin projection is `../grok_hooks_2624/allow.json`. Case budget: 25 seconds; cleanup budget: one second. Owned PID 63431 was reaped or killed before return. No global hook configuration changes.

SHA-256: `cf1cd2990756299cacf9b4d834cf86308fc35297667a977dcc4a1d04e9afe70f`
