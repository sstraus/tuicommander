# Speaker echo investigation, 2026-11-01

Baseline: `86c904c528`, branch `fix/2683-voice-turn-taking`.

Read-only source: `~/Library/Application Support/com.tuic.commander/logs/tuic.log.2026-11-01`.
There are 25 paired capture-triggered pause/resume intervals in the retained log.
After successful synthesis at `17:57:45.167183Z`, five pairs are:

| Pause UTC | Resume UTC | Seconds |
| --- | --- | ---: |
| 17:57:45.544348 | 17:57:47.434993 | 1.890645 |
| 17:57:47.979841 | 17:57:49.867161 | 1.887320 |
| 17:57:50.303461 | 17:57:52.282014 | 1.978553 |
| 17:57:53.215749 | 17:57:55.282979 | 2.067230 |
| 17:57:55.834528 | 17:57:57.531931 | 1.697403 |

No `barge-in hold opened` events or AEC installation failure warnings occur in
this log. The executable predates the hold diagnostic. These lines prove hold
decisions; they do not identify endpoint owner, acoustic playback, per-hold far-end
reference, residual energy, delay estimate, or exact verdict. No endpoint or acoustic
cause attribution is claimed. Source confirms reference registration precedes output
play, and cleaning precedes speech detection. Automatic delay estimation alone does
not prove historical convergence or sample alignment. Regression provenance remains
unknown.

The added `speaker_echo_probe` example uses production native capture/output,
far-end tap and canceller, with the existing local speech bundle. It records bounded
raw/reference/cleaned PCM and sample timelines to a new explicit directory. It checks
microphone permission before opening any device and never requests a prompt, starts
a desktop app, changes configuration, or replaces a binary.

Native mbx/build-slot command:

```
cargo build --manifest-path src-tauri/Cargo.toml -p tuic-dictation --example speaker_echo_probe
```

First build failed on an incorrect default-loudness assumption; corrected to the
existing production defaults. Second build passed. Logs are
`~/Gits/.tmp/sol-voice-probe-build.log` and `~/Gits/.tmp/sol-voice-probe-build-2.log`.

Direct worktree binary execution returned:

```
microphone_permission=not_determined
Error: "microphone permission unavailable; no prompt requested"
```

Log: `~/Gits/.tmp/sol-voice-real-capture-1.log`. The requested evidence directory
`~/Gits/.tmp/sol-voice-real-capture-1` is empty. No capture device was opened, audio
played, PCM recorded, or process left running. No real-loop reproduction or acoustic
RED/GREEN is claimed.

The standalone process needs manual microphone authorization, or an existing real
microphone/reference fixture. Historical per-hold attribution needs measurements
from an instrumented run; old logs cannot recover them. Do not change the accepted
barge-in policy from this evidence. At the initial blocker, parts B and C had no production edits. The coordinator
later authorized B independently. B now has the separate reply-admission fix;
C still requires a proven acoustic cause. Both stories remain open until all
remaining acceptance evidence is available.
