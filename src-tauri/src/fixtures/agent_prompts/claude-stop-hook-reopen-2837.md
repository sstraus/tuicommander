# Claude Stop-hook reopen capture (2837-df6a)

- Source: real Claude Code v2.1.316 (Sonnet 5.5) in a throwaway PTY of the daily TUICommander instance, recorded with `POST /diagnostics/capture` for that session only on 2026-11-05. This is NOT a managed worker. The PTY came from `session create`, and `claude --model sonnet "…"` was typed into it, not spawned through the TUIC agent path. TUIC hooks were active (the log shows `hook-idle` for it), but it had no peer identity, mailbox or worker prompt. The managed-worker evidence is the daily log of `totest-s3` and `tuic-2528` (below).
- Geometry: 40 rows, 140 columns. Capture was enabled after the PTY existed and its first record is input, so the recorder wrote 0x0; the header carries the geometry the session was created with (`session create rows=40 cols=140`).
- Capture: `claude-stop-hook-reopen-2837.tcap` (records 0-493 of session `a13c923c-f2b7-40f5-ad50-973ded6d8455`).
- SHA-256: `3c07e98ca344298fb8f81cb529dece2252bb639d58b49a12639316cfa6a806a4`.

Two turns: a launch prompt and a submitted `Reply with the single word TWO`. Status line repaints follow every 10 s, then the next prompt is typed into the composer (records 491-493, not submitted in the slice).

TUIC's Claude hooks reach the backend through the socket side channel, so their markers are not in the bytes. The daily log has them: the prompt hook with the submitted line (record 322), and `hook-idle` at 09:08:31.031, 3 ms before `working-screen` at Protocol rank. Record 447 is that first chunk after the Stop hook: it only retitles the window, so the screen still shows the `Cultivating… (10s · ↓ 1 tokens)` spinner row. Every Claude turn on the daily instance that day shows the same pair of log lines (1-25 ms apart), and Claude 2.1.316 shows `(running Stop hooks… 6/7 …)` in that spinner row while it runs the remaining Stop hooks.

The two stuck workers (`totest-s3` 129b48a3, `tuic-2528` 0a1fb926) never reached `agent-ready-screen` after that reopen: the timer fell back to `silence-timeout-agent` every 10 s, between `real-activity` from status line repaints, for 4 and 83 minutes. Both prove that the Ready adapter could not read their composer. That screen was not retained (the ring had rolled past it). The replay stands in for it with records 491-493, a composer that holds text TUIC did not type.
