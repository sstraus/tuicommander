# Live ego follow-up turn read as completed (2924-b811)

Capture recorded on 2026-11-08 from a real ego 0.4.5 PTY (model gpt-6-sol,
yolo) started inside the TUIC orchestrator instance, cwd `~/Gits/.tmp/ego-repro`.
Recording used `POST /diagnostics/capture` for that one session; TUIC_SESSION
and the real OSC 7770 output profile were set by TUIC. No bytes are invented.

SHA-256: `1b11656d9d8f48dfee4d75e400a75e5282425638e8d31bbbc33b4805bdbe4c8b`.
Geometry: the original header held 0x0 (capture began after launch); only those
four header bytes were set to 24 rows by 80 columns, the default of the created PTY
(output wrapped at 80 columns). No record byte changed.

Content: turn 1 (a backgrounded `sleep 45`) ends with
`suggest: [ Another command | Check status | Finish ]`. Turn 2, a follow-up
of five sequential foreground sleeps, runs 1m28s and ends with the same suggest
line. Turn 3 follows. Sampled `session list` showed `idle completed` for all
of turn 2 although ego emitted OSC 7770 `state=busy` at its start.

Used by `ego_followup_turn_is_not_completed_by_previous_turn_suggest`.
