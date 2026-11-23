# agy 1.5.10 captures (story 3103-eb8e)

Recorded 2026-11-22 from `/opt/homebrew/bin/agy` 1.5.10 with `scripts/agent_capture/run.py record`:
an isolated debug `tuic-remote` daemon, the `/diagnostics/capture` tap, one throwaway session
per file, 41 rows by 128 columns. The daemon launched agy with
`--settings={"altScreenMode":"never"}`; no capture holds `?1049h`.

| File | agy arguments | Recorded sequence |
|---|---|---|
| `agy-1.5.10-trust-turn.tcap` | `--model gpt-oss-120b-medium` | Workspace-trust picker, Enter, empty composer, one prompt, working, answer `READY`, empty composer |
| `agy-1.5.10-command-approval.tcap` | `--model gpt-oss-120b-medium --mode accept-edits` | Trust picker, Enter, prompt, `Run this command?` approval for `date -u`, Enter on option 1, command output, empty composer |
| `agy-1.5.10-question.tcap` | `--model gpt-oss-120b-medium` | Trust picker, Enter, prompt, two rejected `AskQuestion` calls, native question form, Down, Enter (green), answer, empty composer |
| `agy-1.5.10-model-unavailable.tcap` | none (default model) | Trust picker, Enter, prompt, `Model unavailable (attempt N/9), retrying` for four minutes; the session was deleted while the turn still ran |

Each file ends with the rows that agy prints when the driver deletes the session
(`CLI error: program was killed`, `Resume with -c`).

The files are not promoted scenario fixtures. The trust picker prints the absolute workspace
path, which the promotion check refuses, and the daemon that recorded them still had the old
screen adapter, so its observed states are not evidence. The tests in
`src/pty/agy_capture_tests.rs` replay the bytes through the current pipeline.

Content note: the banner prints the signed-in account address and the plan name.

## Shapes these captures prove

- Composer: a `>` row between two full-width `─` rules. It stays on the screen during a turn.
- Hint row below the composer: `esc to cancel` during a turn and during a picker;
  `? for shortcuts` with an empty ready composer; no hint text with a draft.
- Working rows above the composer: a braille spinner and a phrase (`Generating...`,
  `Working...`, `Running command...`, `Model unavailable (attempt 6/9), retrying in 22s...`).
- Echo of a submitted prompt: a 60-column rule, then `> <prompt>`, with no rule below it.
- Pickers: option rows start with `> ` (cursor) or two spaces; the footer starts at column 2:
  `↑/↓ Navigate · enter Confirm` (trust), `↑/↓ Navigate · tab Amend · ctrl+g edit/expand command`
  (approval), `↑/↓ Navigate · enter Select · esc Skip` (question).

## Not recorded

- A user status-line command (the binary has one), a wrapped option label, a multi-question
  form, the write-in row, Escape on a picker, number keys on a picker.
- Hook frames: the capture daemon runs with `--no-agent-configs`.
