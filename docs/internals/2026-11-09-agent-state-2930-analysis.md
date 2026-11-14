# Agent state analysis and design for 2930-c02d

Scope: initial source inspection at TUICommander `5c9d751ab5d208d7ca7608bc17617435d2fc4cbc` and ego `b169ecea2edce07ee4847161dd205e780b0318ae`. Writer inventory and initial design below describe that source pin. The implementation section records the selected simplification, corrected validation claims and native macOS evidence.

## Evidence

- The prior audit is `orchestrator/reports/tuicommander/2026-11-06-state-detection-audit.md`. It found that native hook configuration was not proof that the hook was active: the inspected restored Claude process had no launch-scoped state settings. It also documented false-busy and false-green histories, with evidence limits. Its line numbers refer to the earlier source pin, not this checkout.
- In the current ego source, `crates/ego-cli/src/run/interactive_driver.rs:860` emits busy for `TurnStarted`; `:888–905` emits idle for `TurnCompleted`. This path does not examine active background work before sending idle. That observation alone does not establish which source owns every background exec or subagent lifecycle.
- In current TUICommander, `src-tauri/src/state.rs:4761–4820` derives the returned shell and agent state. The ego idle special case deliberately excludes process-derived background work from the agent-state decision; completion can also normalize a busy shell snapshot to idle. This was intended to avoid interpreting arbitrary descendants as active turns, so removing that exception without a reliable owned-work signal would regress the opposite case.
- `src-tauri/src/agent_stage.rs:172–183` separately considers background work and working agent state for the stage projection. Stage is not simply another name for agent state.

## Ownership observations

The durable-child display surface is not a complete owned-work source. In ego, `crates/ego-cli/src/repl/seat/background.rs:155–175` registers child rows by invocation, and `:280–295` marks the attached input settled and removes the row. Separately, `crates/ego-core/src/state.rs:2260–2284` stores detached executions until a settlement event with an outcome arrives. `crates/ego-core/src/public/mod.rs:605–607` suppresses those lifecycle events from the public projection. `crates/ego-core/src/turn/mod.rs:6184` refreshes detached executions from the process port, while `crates/ego-cli/src/run/driver_runtime.rs:3452–3465` checks the actual process status on an idle-seat wake. The two ownership paths need a shared publication point or an explicitly combined projection; a count of display rows alone misses detached executions. The timing of publication after a wake and after session replacement remains to be established.

## Design constraints and candidate direction

Ego should publish the lifecycle of *owned* running subagents and background executions, including their final settlement, independently of the parent turn. A TUIC projection can then show working until both the foreground turn and owned work settle. The design needs explicit semantics for awaiting input, overlapping work, cancellation, session shutdown, and out-of-order or duplicate events. Do not use process descendants as a substitute for ownership.

Native-only detection for a hook-capable agent must distinguish an actually active native source from a configuration assumption. The previous audit showed that restored Claude launches can omit the hook. Suppressing all fallback solely because an agent supports hooks can silently produce false states. The deployment and failure behavior of hooks must be established before removing fallback paths. ACP has a separate activity owner and must not be routed through PTY detection.

## Writer inventory

All paths in this section are relative to TUICommander unless prefixed `ego:`. The shell atomic is initialized to `SHELL_NULL` by the reader at `src-tauri/src/pty.rs:4222`, changed by the compare-and-exchange at `src-tauri/src/pty/lifecycle.rs:2453–2466`, and removed on lifecycle cleanup at `:3370`. The following producers reach that common transition:

| Producer | Source | Effect |
|---|---|---|
| Native hook/OSC 7770 | `src-tauri/src/pty.rs:1647–1660`, `src-tauri/src/pty/lifecycle.rs:2872–3016` | Explicit busy or idle; hook state receives protocol handling. |
| Shell OSC 133 | `src-tauri/src/pty.rs:1680–1683` | Shell command start/end, not agent turn boundaries. |
| Submitted input | `src-tauri/src/pty/lifecycle.rs:2762–2834` | Advances the turn and may set busy. |
| Queue/explicit composer claim or rollback | `src-tauri/src/pty/injection.rs:698–714, 749–797` | Takes idle to busy for delivery; rolls back a failed claim. |
| Changed output/working screen | `src-tauri/src/pty.rs:3836–3869`, `src-tauri/src/pty/lifecycle.rs:2679–2755` | Records evidence and may set busy. |
| Silence timer, ready screen and process observation | `src-tauri/src/pty.rs:405–591` | Can set idle after its decision and epoch checks. |
| Claude cancellation recovery | `src-tauri/src/pty.rs:3583` | Explicit idle after the recognized cancellation. |
| Reader exit | `src-tauri/src/pty.rs:4744` | Attempts busy to idle when the reader ends. |

`session_state_with_shell` reads that atomic and may normalize the returned `shell_state`; it derives `agent_state` in `src-tauri/src/state.rs:4761–4820`. Its precedence is no agent, foreground input blocked, awaiting or choice, eligible background work, completion, busy, idle, then starting. The ego idle exception affects the local background-work decision, not the stored `background_work` field. Thus the stage projection can still see stored process work even when the returned agent state does not. The HTTP creation path sets the requested `agent_type` and sets `agent_state` to `starting` only when that type is present (`src-tauri/src/mcp_http/session.rs:1051–1055`). `background_work` is reconciled by the process snapshot at `src-tauri/src/pty/screen.rs:680–735`, with turn and snapshot-generation checks. `stage` is calculated, not independently persisted: the ordered projection in `src-tauri/src/agent_stage.rs:130–190` handles failed or uncertain input, pending prompt, reports, native question, queue, idle, then working. Test-only assignments in source files are not production writers.

## Selected per-kind design

| Agent | Proposed authority |
|---|---|
| ego PTY | Native `state` only. Ego itself keeps busy while any owned work remains. Human awaiting takes precedence. Do not use descendants to infer owned work. |
| Claude and Codex | Native hooks only after the current process has actually sent an activation/identity receipt; before that, or where hooks are unsupported, use the existing non-native fallback. Install/inject hooks on every managed launch and restore; configuration intent is not an activation receipt. A missing receipt must not be reported as proof of an idle turn. |
| Gemini, Grok, OpenCode, Pi | Use native lifecycle when an observed current-session native source is active; otherwise the existing fallback. Do not equate the presence of an optional adapter with a delivered signal. |
| Goose, Amp, Cursor, Droid | No managed native turn source was identified by the prior audit. Retain their existing screen/process/silence path. |
| ego ACP | Keep the existing separate ACP activity projection: `src-tauri/src/acp/activity.rs:10` defines human wait, running turn, live tool, live child, idle precedence; `:149–164` keeps durable children across turn end. No PTY source or agent-kind heuristic applies. |

Boss selected the simpler contract at 11:41: **no new wire state, metadata frame, or TUIC consumer**. Ego owns its children and detached execs, so its existing OSC `busy` remains true until the last owned job settles. Only then may it emit `idle`. Work delegated to another TUIC agent is not owned work. Awaiting a person still takes precedence. Journal subagent/exec lifecycle is the authoritative ownership source, not descendant processes or the disposable child display registry.

### Selected simplification

Once an agent has actually emitted native busy, rendered Working/Ready rows, generic output, process descendants and silence do not decide its turn state. Before a native receipt, existing heuristics remain available for restored/uninstrumented processes. A real foreground exit still withdraws the agent identity; this is lifecycle, not a turn guess. Submitted input still claims the composer to serialize delivery, but cannot turn a completed native turn into a new model turn without native acknowledgement.

Remove these competing native paths: screen reopen after native idle; restoration of native idle after that screen reopen (including its two bookkeeping fields); five-minute screen recovery of native busy; silence/process confirmation of native busy; descendant-based agent-state promotion for a native agent; completion overriding a newer native busy. Keep the screen adapters and evidence reducer for agents that have never delivered native state, and keep native modal parsing separate from turn activity.

### Inventory completeness notes

`SessionState` defaults initialize the optional snapshots and `background_work=false`; `src-tauri/src/state.rs:691–707` declares these fields. The only production atomic shell writer is `try_shell_transition_locked` (`pty/lifecycle.rs:2453–2466`); injection claim/rollback and reader shutdown use that same writer. `stage` has a single production owner, `AgentStage::view` (`agent_stage.rs:130–190`), whose report/queue precedence is intentionally not a model activity detector. `mcp_http/launch_transaction*.rs` also has a field named stage, but it describes launch transaction recovery, not agent activity. Assignments in `sleep_prevention.rs`, `idle_close.rs`, and the later `state.rs` blocks are test setup, not additional production state writers.

`background_work` has one production process writer (`pty/screen.rs:729–734`), initialized by the session default and exposed by the snapshot. It must not be substituted for native owned work. ACP activity is independently derived in `acp/activity.rs`; it has no shell atomic and is not a hidden PTY writer. The native producer owners are `ego:run/interactive_driver.rs:850–905` (foreground state), `ego:run/native_events.rs` (session/turn/tool/child metadata), `ego:state.rs:2260–2284` (exec settlement), and `ego:state.rs:2694–2764` (subagent spawn/settlement). Background task collection in `ego:run/port_adapters.rs:1566–1572` journals settlement; the driver wakes for that work at `ego:run/driver_runtime.rs:3452–3485`.


The expected deletion is the **native-session branches** that reopen or settle native turns from screen or silence, including the native stale-screen recovery in `src-tauri/src/pty.rs:405–591` and screen-reopen arbitration in `src-tauri/src/pty/lifecycle.rs`, once activation and native coverage are proven. The common timer, screen classifiers and process reconciliation cannot simply be removed: non-native agents and suspend safety still use them. Exact branch boundaries and before/after line counts are not yet established; claiming a count before selecting and verifying a diff would be misleading. The `state.rs:4790–4795` ego descendant exception can be replaced by the owned-work projection, but removing it without that signal creates false busy.

## Known cases checked against the prior audit

- **Ego resumed-turn false green:** the November 6 audit observed work after an idle frame from an older running ego process and associated it with the missing busy-on-resume fix in ego 900-242b. The audit did not prove a fresh fixed process reproduced that defect. This is distinct from the November 9 report, where the parent has ended but owned child work remains.
- **Ego process-descendant false busy:** story 2018-1885 is cited in the audit as the reason not to promote an idle turn from generic descendants. The story is not available through the current story index; this status is historical audit evidence, not a fresh story verification.
- **Claude stale spinner reopens completed turn:** 2837-df6a is now `complete` in the story index; the earlier audit called it pending. Verify its implementation against the current checkout before deleting its recovery path.
- **Claude AskUserQuestion Escape leaves busy/tool stale:** 2839-9095 is now `complete`; its story records a captured replay and GREEN on tree `427785f749`. This is a change from the earlier audit's pending classification.
- **Claude /resume Escape remains working:** 2840-b06c is `wontfix`, tagged `validate-later`; its capture and RED are unchecked. The claimed cause is not established by a replay.
- **Codex internal continuation false completed, frozen spinner false busy, tool-progress false idle:** the November 6 audit cites historical memories and states that relevant source behavior existed at its pin. They remain prior evidence, not newly reproduced cases in this checkout.

Additional MDKB problem records, all historical unless separately verified on this tree:

| Case | Recorded cause and outcome |
|---|---|
| Frozen spinner false busy (`frozen-spinner-glyph-pins-agent-busy`) | A retained Claude completion summary was classified as a live spinner. The recorded remedy distinguished animation from a static glyph; Codex had a separate presence rule. |
| Codex frozen TUI false idle (`codex-frozen-tui-presence-keepalive`) | A foreground subprocess could freeze the screen while the working row remained present. Change-only evidence timed out; the recorded remedy used working-row presence as a keepalive. |
| Codex working row could not reopen idle (`codex-false-idle-working-not-busy-entry`) | Working was classified as chrome and used only as a keepalive, so a false idle could persist until other output. The memory labels its then-proposed remedy as not yet applied; the later audit reports a semantic working-row reopen in its source pin. |
| Retry false idle (`agent-false-idle-during-api-retry`) | A frozen retry screen allowed ready/silence to close a still-running turn; the recorded remedy held busy on recognized retry lines. |
| Tool-spinner false idle (`tool-progress-spinner-false-idle`) | A quarter-circle progress row was not recognized before cutoff. The old remedy checked spinner rows before cutoff. This historical remedy conflicts with the later HUD-flap fix below; do not reinstate it. |
| HUD repaint false busy (`false-busy-flap-cc-hud-repaint`) | Pre-cutoff spinner detection interpreted arbitrary user HUD glyphs below the input as activity. The later positional remedy removed the pre-cutoff bypass. |
| Protocol idle erased (`protocol-idle-erased-before-rank-check`) | Screen activity cleared idle metadata before rank arbitration, so decorative animation overrode native idle. The record cites a captured replay and fix. |
| Codex stale completion (`tuic-status-line-dedup-stale-completion-2026-07-25`) | Lifetime task-name dedup suppressed a later turn's Working event, leaving old completion metadata. The recorded fix keyed dedup by turn epoch and task name. |

These are the cases surfaced by the audit and targeted memory searches, not a claim that all historical defects have been enumerated.

## Owned-work sources discovered

Ego keeps detached-exec IDs and their settlement snapshots in `SessionState.detached_execs`: `ego:crates/ego-core/src/state.rs:1181, 1427–1432, 2260–2284`. Its public projection explicitly suppresses detached-exec lifecycle events (`ego:crates/ego-core/src/public/mod.rs:605–607`); merely reading existing public `agent-event` frames cannot observe those transitions. The durable-child surface registers by invocation and removes the row when settled (`ego:crates/ego-cli/src/repl/seat/background.rs:155–175, 280–295`). That surface is an observer, not yet proven to cover every exec or every owned child. The event owner and delivery after parent settlement must be traced before choosing an emitter.

## Implementation and remaining validation

Ego change: `fix/2930-background-state`, `626047da2`. The existing emitter keeps busy while journal-owned subagents or detached execs remain unsettled. No new frame, state, TUIC consumer or display ownership registry was added. RED on `b169ecea2`: `parent_turn_end_with_running_subagent_stays_working` failed with idle while an owned child runs. Ten focused ego tests passed on rb; native Mac debug binary built for the live check only. Full evidence: ego `docs/internals/2026-11-09-owned-background-state-993.md`.

TUIC uses one native ownership enum instead of a received flag, latest-hook value and screen-reopen flag. A delivered busy establishes the complete turn source. An idle-only receipt does not: the committed `codex-0.154-runtime-hook-idle.tcap` has zero busy or prompt markers. Treating that legacy notifier as a complete protocol would create a new false-green on every later turn. It retains the working adapter and its delivered idle still suppresses stale prose questions.

Enter delivery is a separate transport transaction, not model truth. A native local command can consume Enter without starting a model turn. Native snapshots keep the delivered state during that claim. A stable empty composer can release a transport claim only while native state remains idle; it cannot complete native busy and does not send parent lifecycle mail. Foreground identity retirement uses the existing lifecycle-to-session lock order and clears native authority before the next process.

TUIC RED: `native_idle_is_not_reopened_by_working_screen` failed on `83b91131ec`; rb log `~/Gits/.tmp/2930-tuic-red-force-2.log`. The suggestion test in that same run omitted a terminal buffer, so its native frame never reached the lifecycle. Its failure is not evidence of received native busy and is withdrawn from the RED claim. The test now includes the real terminal parser setup, with unchanged expected busy/working snapshots; its corrected pre-fix check belongs to coordinator verification.

The first focused aggregate ran 161 of 336 selected tests: 160 passed and the suggestion test failed for the missing terminal buffer. The remaining 175 did not run because Nextest stopped at the failure. Log: `~/Gits/.tmp/2930-tuic-green.log`. Final focused validation passed; see the final result below. The rebuilt isolated app recorded a real ego-owned background child and its final idle; see the evidence below. No daily app was restarted. The earlier isolated instance's readiness receipt was not used as proof of the latest source.

### Measured simplification

The lifecycle ownership record changes from three fields (`hook_state_seen`, `last_hook_state`, `screen_reopened_hook_idle`) to one `native_state` ownership enum. Two methods and their decision paths are removed: screen-reopen tracking and hook-idle restoration after a reopen. Stale unacknowledged submission recovery remains only for completion-only notifiers; full native sources cannot use it. Screen-based reopen and five-minute protocol recovery remain only for fallback sessions; they cannot override a delivered native busy.

Against the implementation base, production file line totals are: `pty.rs` 5178 → 5219; `pty/lifecycle.rs` 4440 → 4447; `pty/screen.rs` 2287 → 2304; `state.rs` 11831 → 11848. The production diff adds 231 and removes 149 lines (net +82). This is fewer competing native owners, not a net line-count reduction: explicit transport-claim handling, foreground retirement serialization and authoritative snapshot projection add code. These counts exclude tests, documentation and hook configuration.

## Native macOS acceptance evidence

The rebuilt `agent-test-2930` instance on port 19930 ran the ego binary from
`626047da2`, with no installed-binary replacement or daily-app restart. Its real
internal background child executed foreground `/bin/sleep 45`. The parent ended
at Unix ms 1794223882105; the child remained owned until journal settlement at
1794223931901, 49.796 seconds later. A post-parent host snapshot showed
`native_turn_outcome=completed`, `shell_state=busy`, `agent_state=working`.
Final idle was stamped 1794223936971, after child settlement and the automatic
collection turn. The screenshot shows the isolated host's ego PTY and
`Agents · 1 working · 0 idle`; the scratch directory is an unregistered global
workspace, so this is badge evidence rather than terminal-canvas evidence.

Committed evidence: `src-tauri/src/fixtures/agent_prompts/ego-owned-child-2930`
with `.tcap`, selected recorded journal `.json`, provenance `.md` and screenshot
`.png` companions. Capture SHA-256:
`1e8dd07b1eda945e231dd95254c9eda3fea101de9035c59f79f3e05e2a208520`.
The named replay is
`recorded_ego_owned_child_keeps_native_working_until_settlement`.

The original live harness reported a false failure at final idle because it
expected rich `child_end`. This background collection path does not emit that
frame. The recorded parent and child journals prove actual settlement and the
successful foreground exec; the supplemental verified result reconciles those
with the same capture and host snapshots. No new consumer, protocol frame or
duplicate live run was added to satisfy that mistaken assumption. Earlier
launcher/role-activation attempts are explicitly excluded in the provenance.
The throwaway sessions were deleted, capture disabled, and the isolated instance
was stopped through `make agent-test-stop`.

Overlapping child/exec work, cancellation, human waits and shutdown retain
focused automated coverage but are not separate live-capture claims here.
ACP and the daily application were not used as substitutes for the worktree build.

## Final focused run and pending correction

The replacement rb run selected 237 tests after retaining 100 unchanged
recorded-green results. It ran all 237 in 12.441 seconds: 235 passed, two failed.
The corrected native suggestion test and real owned-child capture replay passed.
Log: `~/Gits/.tmp/2930-tuic-final-green.log`. This is not final GREEN.

- The partial `claude-ready-idle-notify.tcap` replay contains no native busy
  receipt. Its original fallback working/busy assertion was correct; the changed
  native-completion expectation was wrong and has been restored.
- Removing the latest-hook scalar also lost the completion-only notifier's
  stale unconsumed-submission recovery in an unreadable transcript. Full native
  sources must never use that timer, but the legacy notifier must retain it.

The proposed correction uses one explicit internal ownership enum with four
valid states: unseen, completion-only, native busy and native idle. It preserves
the one-field design, unlike reintroducing an independent flag. Native authority
still starts at busy; only completion-only sessions can use the legacy stale
submission recovery. The coordinator approved this refinement. It is implemented and the final scoped run passed. The unchanged hidden-transcript regression failure above
is its pre-refinement RED. No new protocol state or consumer is introduced.

## Final validation and handoff

The approved enum refinement passed 149 focused tests, zero failures, on rb
Linux aarch64 in 12.409 seconds. The command was:

```sh
RB_FORCE=1 build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run \
  --manifest-path src-tauri/Cargo.toml --lib --no-fail-fast \
  -E "$(cat ~/Gits/.tmp/2930-refinement-filter.txt)"
```

Log: `~/Gits/.tmp/2930-tuic-refinement-green.log`; remote result:
`~/Gits/.tmp/rb/20261109T114757Z-23429`. `RB_BOX_DIRTY=no`. Peak recorded
RSS was 16125956 KiB. The final filter runs the failed cases, touched core
tests and adjacent stale-submission cases. It retains unchanged recorded-green
results from the first two runs rather than repeating unaffected consumers.
The initial selection was native/hook/idle/busy/completion/working/foreground/
suggest/ready/queued/injection PTY cases; shell/agent/completion/background
state cases; stage and sleep-prevention cases; and agent/background/busy
idle-close cases. This is impact-scoped evidence, not a full-suite pass.

The unchanged hidden-transcript stale-submit failure in the previous run is
the pre-refinement behavioral RED. The original native idle-screen regression
failed on `83b91131ec`. Ego has its independent pre-fix owned-child RED. The
corrected suggestion regression, real owned-child capture replay, native
stale-Ready/Unknown protection, completion-only fallback, native process
metadata exclusion and foreground retirement are green. The recorded
no-native-busy Stop-hook test retains its original working/busy expectation.

SHA-256 verification confirmed the final run's production and core/injection
test sources were unchanged after execution. Rust formatting, diff whitespace,
shell syntax and instruction-link checks passed. The staged framed capture has
matching source/hash provenance. The isolated native capture predates the
scalar-to-enum correction, which changes legacy completion-only recovery only;
its native behavior is replayed through the final code. No second live launch
was performed merely to repeat that native result.

Risk is medium/high: the change crosses shared PTY activity, delivery claims,
foreground retirement and snapshot boundaries. Independent critic, batch
integration, post-fix mutation/coverage and the corrected suggestion test's
after-the-fact base RED remain coordinator responsibilities. No full native
suite, Windows runtime or daily-instance restart is claimed. The daily QA item
stays in `to-test/agents-pty-2.md`; it is not used as a substitute for this
isolated check.


## Landing replay-oracle follow-up

The landing wrapper reported a clippy refusal, but its log shows the actual
failure was `replay_oracle_all_committed_tcap_preserves_chunk_decisions`.
At event 99 / record 88 in `claude-ask-multiselect-2634.tcap`, the old golden
expected working after the producer had delivered native idle. No production
correction is required: the recorded expectation conflicts with this story's
native-authority contract.

### Capture evidence

The capture SHA-256 is
`d54286ccdf9aa32d3a99fd043bda848ae12e5304486c374418e22f0a6653a28c`.
It uses TUICCAP2 with 40 rows and 120 columns. Record indices are zero-based.

- Record 68, 14,863,027 us: native `state=busy`.
- Record 84: the answer is `DONE`, while a working row is still retained.
- Record 85: that row says `running Stop hooks`.
- Record 88, 15,825,677 us: exactly `ESC ] 7770;state=idle ESC \`.
- Record 89, 15,837,753 us: only a cursor-restoring Stop-hook repaint; no busy
  frame. The old golden incorrectly reopened shell busy from that screen row.
- Record 92: completed `Brewed` row and ready composer.

Idle at record 88 is correct. A retained working row after delivered native
idle cannot override the producer. The golden now remains idle across the
following repaint instead of changing the production code to preserve the
incorrect working expectation.

### Complete regeneration audit

`scripts/replay-oracle.sh regenerate` updated 15 existing traces and added the
missing oracle for the already committed `ego-owned-child-2930.tcap` fixture.
All 720 changed existing state rows alter only `agent` or `shell`. Question,
choice, confidence, fullscreen and ring-length fields are unchanged. The 19
changed event records alter only shell-state transitions, not question events.
Every corpus entry's event count matches its golden file.

| Capture suffix | Reviewed evidence and expected change |
| --- | --- |
| ask-multiselect-2634 | Native idle at 88 outranks retained Stop-hook repaint. |
| askuser-esc-20260929 | Cancellation text and ready composer at 188; idle from 189 replaces retained working. |
| askuser-esc-21318-2839 | Cancellation text and completed/ready composer at 81; idle replaces retained working. |
| askuser-multi-question-20261012 | Idle at 39 and 178 remains idle; prompt at 64 still starts the next shell-busy transition. |
| await-exit-before-input-1985 | Native idle at 682 replaces retained working. |
| plan-approve-2634 | Native idle at 68 outranks the next repaint. |
| preview-chat-2536 | Native idle at 131 outranks the next repaint. |
| question-exit-2091 | Native idle at 144 is not reopened by retained question or later screen history. |
| stop-hook-reopen-2837 | Idle at 215 and 346 survives retained Stop-hook rows; prompt at 331 still begins the next submission. |
| suggest-consumed-2029 | Native idle at 104 and 150 permits completed presentation; prompt at 124 starts the next submission. |
| suggest-stop-hooks-2029 | Native idle at 76 permits completed presentation despite retained working rows. |
| suggest-two-turns-2029 | Native idle at 93 and 139 permits completed presentation; prompt at 112 starts the next submission. |
| codex-native-tools-2626 | Native idle at 4 and 9 replaces retained working. |
| ego-followup-stale-suggest-2924 | Delivered busy at 83, 119 and 248 outranks stale completed labels until the matching native idle. |
| ego-native-questions-2028 | Busy at 341 and 358 outranks stale completed labels; awaiting remains intact and idle arrives at 414. |

The new owned-child golden has 123 events. Its agent is working through record
100 and idle from record 101. The original fixture's journal companion remains
the independent ownership oracle; generated replay output is not new live
ownership evidence. The corpus now covers 113 captures.

### Validation and remaining scope

- Landing RED: `~/Gits/.tmp/coordinator/land-2930-coordinator.log`, rb nextest
  `af44125d-522f-4d08-ad99-066f5709b749`, failure at event 99 / record 88.
- Regeneration: 4/4 oracle tests passed in 31.037 s on native macOS, nextest
  `3eebc9f3-933d-4ec8-85b9-4e436bc276e2`; compile 5m53s through mbx and
  build-slot. Log: `~/Gits/.tmp/2930-tuic-oracle-regenerate.log`.
- Strict verification, with `TUIC_REGENERATE_ORACLES` unset: the all-captures
  oracle test passed in 26.150 s (test 26.044 s), nextest
  `80cfea92-0a91-4619-8ea2-1ae9879f1c75`, native macOS. Command:
  `scripts/with-test-tmp.sh build-slot.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(replay_oracle_all_committed_tcap_preserves_chunk_decisions)'`.
  Log: `~/Gits/.tmp/2930-tuic-oracle-verify.log`.
- The native test binary reports a linker warning: `__eh_frame section too
  large (max 16MB) to encode dwarf unwind offsets in compact unwind table`.
  Verification exits 0; no warning-free build is claimed.
- Full drift inventory: `~/Gits/.tmp/2930-oracle-drift-audit.json`. No production
  code, capture bytes or independent scenario expectations changed.

This is a test-oracle correction, not a new behavior change. No new desktop
instance, full-suite run, landing, push or story closure is included. The
coordinator owns the next landing attempt and integration gates.
