# Real owned-background-child capture — 2930-c02d

Captured on macOS on 2026-11-09 through the isolated `agent-test-2930` instance,
port 19930. The daily app was not restarted. Geometry: 40 rows × 120 columns,
recorded in the TUICCAP2 header. This is real terminal output, not constructed
protocol data.

- Capture: `ego-owned-child-2930.tcap`.
- SHA-256: `1e8dd07b1eda945e231dd95254c9eda3fea101de9035c59f79f3e05e2a208520`.
- TUIC base: `83b91131ecb6ac3724be55bc6235783120b670db`, with the 2930 native
  ownership changes described in the analysis report. The capture predates the
  final scalar-to-enum refinement that restores completion-only notifier fallback.
  Full-native behavior is unchanged by that refinement; the final code replays
  this capture. This is not a claim that the enum binary was live-launched.
- TUIC executable SHA-256:
  `2d713820c0db5d82220b71995704fba5b0e99a5728c7946ec880b5a28fea83ee`.
- Ego source: `626047da2`, branch `fix/2930-background-state`.
- Ego executable SHA-256:
  `42f5220e15452cf2613cd8489c5a7dc9164d77042ae145ef9766b5bac4904d40`.
- PTY: `683991b0-c100-4c0d-829c-36fbb0220971`.
- Native parent session: `01a1c013-97cd-72ef-9c8f-0303373a2eb8`.
- Native child session: `c3ebe1a5-7a33-5cd9-be42-f82b7759994c`.

The parent used a real internal background task. The initial delegation request
was held for coordinator-role activation and did not launch a child. The second
request launched one durable child. That child called foreground `exec` with
`argv=["/bin/sleep","45"]`, then returned `child-finished`. No external terminal
agent or process-descendant inference substitutes for that ownership.

The companion JSON contains selected recorded parent/child journal events,
including the actual exec call, its successful result and ownership settlement.
It excludes generation prompts and credentials. Recorded timestamps:

| Boundary | Unix milliseconds |
|---|---:|
| Child spawned | 1794223879810 |
| Parent completed | 1794223882105 |
| Child sleep started | 1794223883522 |
| Child sleep completed successfully | 1794223928579 |
| Child settled in parent journal | 1794223931901 |
| Automatic collection turn completed | 1794223936870 |

A host snapshot after the parent completed had `native_turn_outcome=completed`,
`shell_state=busy`, `agent_state=working`, and `last_activity_ms=1794223882161`.
The final host snapshot had idle/idle and `last_activity_ms=1794223936971`, after
the child settlement and automatic collection turn. The child outlived the parent
by 49.796 seconds. The screenshot `ego-owned-child-2930.png` was taken at the
first post-parent working snapshot. It shows the isolated global workspace,
its ego PTY row and `Agents · 1 working · 0 idle`; the unregistered scratch
folder is parked outside the repository list, so the terminal canvas is not
shown. It is evidence of the host activity badge, not a terminal-layout check.

## Harness correction and evidence limits

The live harness initially required rich `child_end` before accepting final
idle. This background collection path does not emit that rich event. It reported
`settled_idle=false`, even though the recorded journal proves settlement before
idle. The supplemental verified result reconciles the original capture, samples
and journal; no application change or duplicate live run was used to make this
claim. The replay uses the recorded automatic turn start as the collection
boundary, with this journal as its independent ownership evidence. No new wire
state or consumer was added.

Earlier attempts are not acceptance evidence: one launcher used incompatible
flags and exited before a native session; another obeyed a one-call limit and
stopped after coordinator activation without launching a child. Their artifacts
remain under `~/Gits/.tmp/2930-live/attempt1` and `attempt2`.

Raw samples, original harness result, reconciled result and scripts remain under
`~/Gits/.tmp/2930-live/`. The throwaway PTY was deleted and recording disabled.
This run proves real owned-child lifetime and final settlement on native macOS.
Overlapping child/exec ownership, cancellation, human waits and shutdown have
focused automated evidence; they are not separately live-verified by this capture.
