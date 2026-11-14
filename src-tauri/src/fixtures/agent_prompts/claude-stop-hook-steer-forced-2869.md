# Claude Stop-hook forced steer capture (2869-3e82)

- Source: actual installed Claude Code 2.1.318, macOS PTY, 2026-11-06.
- Geometry: 40 rows, 140 columns; TUICCAP2 preserves original output/input chunks and monotonic timestamps.
- Scenario: isolated local fake API returns one answer, then the real CLI runs a 60-second Stop command. Escape renders the interrupted composer. Owner text and a separate Enter reach the next real client request before the hook's natural completion.
- Capture: `claude-stop-hook-steer-forced-2869.tcap`.
- SHA-256: `d6df3a0eec30f4f15f00efac66b7b5dc98c47bc9525d267b6f65f25ac1d83b07`.
- Details and limits: `docs/evidence/steer-force-2869/README.md`. This is native CLI admission evidence, not a paid model or a worktree TUIC desktop check. The native probe waits two seconds after Escape; the backend waits for confirmed readiness.
