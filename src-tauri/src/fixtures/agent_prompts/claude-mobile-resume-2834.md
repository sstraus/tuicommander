# Claude resume picker taller than the screen, 2834-dcdd

Recorded on 2026-11-05 from Claude Code 2.1.316 in an isolated native
`tuic-remote` built from this worktree (isolated HOME, throwaway project
directory, no production TUIC session). The PTY was created at 30 rows by 50
columns, a phone-like size. The `/diagnostics/capture` tap recorded it from
before PTY creation; the fixture is the unmodified capture, SHA-256
`a22fc5869074731b40ed518f41523cef4e2717c7e4b283d7f6ae58cf8dd022f0`.

Two own conversations were seeded (`alpha`, `beta`), then `/clear` and
`/resume` were typed. At this size the picker lists more sessions than the
screen holds, so its footer (`Esc to cancel`) is below the last visible row.
The log screen then stopped at the Search box: the trim treated the box border
as composer chrome. At 110 columns the footer is visible and the 1971 rule
already keeps the list.

`captured_claude_resume_taller_than_screen_keeps_session_rows_2834` replays the
capture and checks both the HTTP screen trim and the log-WebSocket screen.
