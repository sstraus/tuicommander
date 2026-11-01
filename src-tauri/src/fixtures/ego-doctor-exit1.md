# Doctor exit 1 receipt

Recorded 2026-10-31 from the installed ego executable with `ego doctor --json`
in the fix/aichat-settings-doctor-failure worktree. Exit 1; stdout remained valid JSON.
Only the credentials and session_store sections are retained; other sections are
omitted, not rewritten. The session-store probe exhausted the shared two-second
budget. Full shell and TUIC PTY receipts are under `~/Gits/.tmp/results/aichat-doctor-*`.
