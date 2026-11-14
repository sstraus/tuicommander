# Claude Stop-hook deferred steer capture (2869-3e82)

- Source: actual installed Claude Code 2.1.318, macOS PTY, 2026-11-06.
- Geometry: 40 rows, 140 columns; TUICCAP2 preserves original output/input chunks and monotonic timestamps.
- Scenario: isolated local fake API returns one answer, then the real CLI runs a 60-second Stop command. Ctrl+B plus owner text and Enter does not reach its next API request until Escape interrupts the hook.
- Capture: `claude-stop-hook-steer-old-2869.tcap`.
- SHA-256: `768d79c38da101715665b6ddf6f96ddde23c7f07dba33fe803e359693071930a`.
- Details and limits: `docs/evidence/steer-force-2869/README.md`. The API key displayed in onboarding is a local dummy, not a credential. No live TUIC session or paid model was used.
