---
status: in_progress
approved_at: "2026-10-09T16:14:58.202Z"
updated: "2026-10-09T16:15:32.401Z"
started_at: "2026-10-09T16:15:32.401Z"
---
# Plan: Automations scheduler

**Created:** 2026-10-09 | **Status:** Approved for phased implementation | **Effort:** XL | **Branch:** feat/automations-core

## Summary

- Add scheduled agent runs with friendly cadence controls, durable history and clear needs-you states.
- This is a development assignment. Implement phase 1, then phase 2. Phase 3 is listed but not started; this peer stops after Step 1.

## Architecture Context

- Definitions → zone-aware due occurrence → SQLite reservation → workspace/precheck → configured agent → task/event evidence → durable final state.
- `config.rs::ConfigFile<T>` provides atomic JSON writes and locked strict read-modify-write; use it for `automations.json`.
- `workflows/run/{store,runtime}.rs` provides the owner-lock, restart reconciliation and reserve-before-effect patterns, not an automation implementation.
- `mcp_http/mcp_transport.rs::launch_daemon_workflow_agent` requires a real workflow activation. Share its underlying agent launch assembly; preserve its guard.
- `create_daemon_workflow_worktree` delegates to the common worktree routes. An automation needs its configured base branch passed through that path.
- `lib.rs` starts WorkflowRuntime in desktop and remote boot. The scheduler is another in-process runtime with no new daemon or plugin host.
- `tasks.rs`, `event_bus` and progress supply evidence; idle is not universal proof. Final unknown is required when evidence is absent.
- Fixed decisions: no session reuse; run-config permissions; one IANA zone per definition default local; overlap skip; global configurable limit default 2; max duration.
- ADR: reserve durably before dispatch and interrupt all open runs at boot without retry — prevents duplicate agent execution after a crash — transparent automatic retry rejected.

## Research Findings

- Orca source `89ba9682`: `src/shared/automations-types.ts`, `automation-cron-occurrence.ts`, `src/main/automations/{service,headless-dispatch}.ts`; see [source analysis](../ideas/automations-scheduler.md).

- No Orca implementation code is incorporated. RRULE, multi-host ownership, external managers and estimated token spend are out of scope.
- [croner](https://docs.rs/croner/latest/croner/) offers next/previous zone-aware occurrence APIs; [chrono-tz](https://docs.rs/chrono-tz/latest/chrono_tz/) supplies IANA TimeZone implementations. Verify pinned APIs in Step 2.
- Strict ConfigFile loading preserves malformed JSON by moving it aside. Semantic/schema failures must abort writes and leave the original available for recovery.

## OpenClaw reference

- Checked 2026-10-09: [source commit 476ce032](https://github.com/openclaw/openclaw/tree/476ce032f4f01e2d95c2316a219c8fea8f5f09ba), [cron schedules](https://docs.openclaw.ai/automation/cron-jobs/schedules), [heartbeat/wake](https://docs.openclaw.ai/gateway/heartbeat), [result delivery](https://docs.openclaw.ai/automation/cron-jobs/delivery). Read `src/cron/service/{timer-scheduler,timer-job-runner}.ts` and `src/cron/isolated-agent/run.ts`; no code copied.
- Better single-user fit than Orca (product judgment): explicit result destinations/inspection links, separate execution and delivery outcomes, quiet no-change reports and bounded wakes.
- **Adopt, Steps 3/10/11:** saved output is the canonical report; notices link the run/session. Persist notification attempted/confirmed/unknown separately; deduplicate by run/transition/channel. Notification failures cannot change execution success. Do not replay ambiguous sends or historical notices on boot.
- **Adopt, Steps 4/7:** bounded wake reconciliation, reserve before execution, finite duration and completion provenance. Keep TUIC's simple 30s tick.
- **Adopt, Step 11:** precheck skips remain quiet; notify failure/needs-you through existing native/push/Telegram preferences. Completed output remains in history.
- **Reject:** model heartbeats (cost/context coupling); session reuse/result injection into replacement conversations (fixed exclusion); stream watchers, interval/pacing and webhook/channel routing (YAGNI); automatic execution retries/restart dispatch (fixed no-retry decision); implicit last-conversation recipient (use run/session ownership).

## Security Considerations

- Automations use the professional user's own repo, prompt, shell and run config; do not invent per-automation unattended permissions.
- Reject invalid definitions before execution; preserve malformed/unsupported files, validate destination again before spawning, and terminate only the run's owned child/session.
- Run output may contain private project text. Follow existing config-directory ownership and authenticated HTTP transport; never log prompts or secrets routinely.

## Performance Considerations

- One 30s scheduler tick; query indexed recent/open runs, not entire history. Sleep/wake evaluates at most one latest occurrence per definition.
- Configurable concurrency default 2, overlap skip, finite max duration; no queued backlog. Capacity refusal is recorded as skipped_concurrency.
- Defaults: grace 12h; max duration 1h; both user-configurable. Output/precheck buffers bounded to 256 KiB per stream; history age retention default 90d, final runs only.
- Routine tests are targeted and use test temp isolation, mbx and build-slot. Coordinator owns full suites/mutations and headless cross-feature builds.

## Execution contract

- All `automations/` paths below are relative to `src-tauri/src/`; full acceptance criteria live in each linked story.
- DST: skip nonexistent fixed wall times; run fixed wall times once at the earlier fold instant. Validate impossible cron patterns with bounded search.
- Run Now works when paused, bypasses precheck, obeys overlap/cap and leaves scheduled cursors unchanged. Full capacity records skipped_concurrency, never queues.
- Open needs_you runs count toward overlap/cap and max duration. Boot interrupts every open state. Final transitions are immutable/idempotent; lost evidence becomes unknown.
- History survives deletion; final output/precheck streams truncate at 256 KiB. Age retention defaults to 90d and never deletes open runs.
- Every command shares one Rust implementation. API work includes preview/presets and aggregates, regenerated COMMAND_TABLE snapshot and registered-route probes.

## Story order

| Step | Story | Depends on |
|---|---|---|
| 1 | 1610-ac85 | None |
| 2 | 1611-fa3f, 1629-2262 (Once) | 1610-ac85 |
| 3 | 1612-70d1 | 1610-ac85 |
| 4 | 1613-67f3 | 1611-fa3f, 1612-70d1 |
| 5 | 1614-7e8f | 1610-ac85 |
| 6 | 1615-57cf | 1612-70d1, 1613-67f3, 1614-7e8f |
| 7 | 1616-1882 | 1615-57cf |
| 8 | 1617-290d | 1611-fa3f, 1613-67f3, 1615-57cf, 1616-1882 |
| 9 | 1618-685b | 1617-290d |
| 10 | 1619-b84d | 1618-685b |
| 11 | 1620-82ae | 1616-1882, 1618-685b |
| 12 | 1621-5dce | 1617-290d |
| 13 | 1622-be11 | 1618-685b |
| 14 | 1623-8b2a | 1615-57cf, 1619-b84d |
| 15 | 1624-c5f5 | 1614-7e8f, 1615-57cf |

- Acceptance criteria and concrete test cases for each step are recorded in the linked story files.

## Steps


### Phase 1 — Rust core

### Step 1: Definition storage

- **Files:** `automations/{mod,model,definitions}.rs (new)`
- **Constraint:** Strict storage preserves corruption; per-id mutations avoid array replacement races. Definitions contain literal prompts; Smart Prompt variable expansion is not available headless.
- **Validation:** test(automations::tests)

```rust
let file = ConfigFile::<AutomationsConfig>::new("automations.json");
file.update_with_strict(|latest| { latest.definitions.push(definition); Ok(((), true)) })
```

### Step 2: Cron, Once and timezone semantics

- **Files:** `automations/schedule.rs (new), src-tauri/Cargo.toml, Cargo.lock`
- **Constraint:** Use croner with chrono support plus chrono-tz; verify exact pinned API and DST policy against its source before adding it. No custom cron parser or RRULE in phase 1. Once stores one local date-time with the same IANA zone; reject past instants at creation and spring gaps, resolve folds to the earlier instant. Keep the definition after its one scheduled occurrence is consumed, and derive completed state from the durable cursor.
- **Validation:** test(automations::schedule::tests)

```rust
let zone: chrono_tz::Tz = timezone.parse().map_err(|_| "Invalid IANA timezone")?;
let zoned_now = now.with_timezone(&zone);
let next = cron.find_next_occurrence(&zoned_now, false)?;
```

### Step 3: Durable run ledger

- **Files:** `automations/{run,store}.rs (new)`
- **Constraint:** Record before dispatch. A database owner lock prevents desktop and remote sharing one config directory from dispatching twice; follow existing WorkflowRuntime ownership.
- **Validation:** test(automations::store::tests)

```rust
let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
tx.execute("INSERT INTO automation_runs (...) VALUES (...)", params![run_id, automation_id, occurrence])?;
tx.commit()?;
```

### Step 4: Scheduler admission and catch-up

- **Files:** `automations/scheduler.rs (new)`
- **Constraint:** One 30s tick, no minute-by-minute wake replay. Do not launch a runtime with an unimplemented dispatcher; Step 6 wires boot after real dispatch exists.
- **Validation:** test(automations::scheduler::tests)

```rust
for due in latest_due(now, &definitions)? {
    store.reserve_if_admitted(due, definitions.max_concurrent_runs)?;
}
```

### Step 5: Bounded precheck execution

- **Files:** `automations/precheck.rs (new), src-tauri/src/smart_prompt.rs`
- **Constraint:** Use test_support host shell helpers; no freshly executed per-test script. Precheck context injection is phase 3.
- **Validation:** test(automations::precheck::tests)

```rust
let result = run_precheck(&workspace, &precheck.command, precheck.timeout_secs).await?;
if result.exit_code != Some(0) { store.skip_precheck(run_id, result)?; }
```

### Step 6: Dispatch and shared runtime boot

- **Files:** `automations/{dispatcher,runtime}.rs (new), src-tauri/src/mcp_http/mcp_transport.rs, src-tauri/src/lib.rs`
- **Constraint:** No desktop launch in agents. This helper currently creates from HEAD; pass the explicit configured base branch through the shared worktree API, preserving existing workflow behavior.
- **Validation:** test(automations::dispatcher::tests) | test(automations::runtime::tests)

```rust
let worktree = create_daemon_workflow_worktree(state, &definition.repository, &branch).await?;
let launch = launch_agent_effect(state, &definition.run_config, &worktree, &definition.prompt)?;
store.bind_launch(run_id, launch)?;
```

### Step 7: Completion and maximum duration

- **Files:** `automations/completion.rs (new), automations/runtime.rs`
- **Constraint:** Do not equate agent idle with completed; unsupported detection must remain honest. Maximum-duration cancellation cannot target another task.
- **Validation:** test(automations::completion::tests)

```rust
match evidence {
    CompletionEvidence::ConfirmedDone => store.finish(id, RunStatus::Completed)?,
    CompletionEvidence::Lost => store.finish(id, RunStatus::Unknown)?,
    _ => {}
}
```

### Step 8: HTTP and IPC parity

- **Files:** `automations/api.rs (new), src-tauri/src/mcp_http/mod.rs, src-tauri/src/lib.rs, src/transport.ts, src/__tests__/transport.test.ts`
- **Constraint:** All runtime decisions remain Rust. Low-frequency changes use SSE and desktop emit; remote ownership must be explicit, never dispatch onto the wrong machine.
- **Validation:** targeted automations API nextest + transport.test.ts + pnpm exec tsc --noEmit

```rust
#[tauri::command]
async fn automation_action(input: AutomationAction, state: State<AppState>) -> Result<AutomationReply, String> {
    automations::api::execute(input, &state).await
}
```

### Phase 2 — UI

### Step 9: Automations dialog

- **Files:** `src/components/AutomationsDialog/ (new), src/stores/automations.ts (new), src/actions/actionRegistry.ts, src/components/CommandPalette/CommandPalette.tsx`
- **Constraint:** Visual verification required. No schedule computations or process orchestration in TypeScript. Browser/PWA use the same commands.
- **Validation:** dialog/store Vitest; tsc; screenshots

```rust
const preview = await invoke("automation_action", { input: { action: "preview", cron, timezone } });
```

### Step 10: Runs view and output navigation

- **Files:** `src/components/AutomationsDialog/RunsView.tsx (new), src/stores/automations.ts`
- **Constraint:** Visual verification required. UTC windows are elapsed durations, and the backend owns aggregates.
- **Validation:** runs-view Vitest; tsc; screenshots

```rust
const summary = await invoke("automation_action", { input: { action: "summary", window: "7d" } });
```

### Step 11: Needs-you and failure notifications

- **Files:** `automations/notifications.rs (new), existing native/web-push/Telegram adapters, AutomationsDialog`
- **Constraint:** Visual verification required for bell changes. No vendor traffic fixtures invented; reuse internal envelope fault models and record limits.
- **Validation:** notification nextest/Vitest; tsc for frontend

```rust
if store.claim_notification(run_id, transition)? { notifications.publish(run_notice).await?; }
```

### Step 12: MCP automation tool

- **Files:** `src-tauri/src/mcp_http/mcp_transport.rs, CLI HTTP adapter`
- **Constraint:** Phase 2 (Boss, 2026-10-09). Share the backend API with HTTP/IPC; no separate scheduler logic.
- **Validation:** targeted MCP/CLI parity tests

```rust
automation_api::execute(input, state).await
```

### Step 15: Precheck output as context

- **Files:** `automations/precheck.rs, dispatcher.rs`
- **Constraint:** Phase 2 (Boss, 2026-10-09). Session reuse remains outside this plan.
- **Validation:** targeted precheck-context tests

```rust
let prompt = attach_precheck_context(&definition.prompt, &result.stdout, result.truncated);
```

### Phase 3 — not started

### Step 13: Built-in templates

- **Files:** `automations/templates.rs (new), AutomationsDialog`
- **Constraint:** Not started. Requires explicit phase-3 authorization.
- **Validation:** targeted template tests + frontend tsc

```rust
let draft = templates::daily_review(repository, run_config, local_zone)?;
```

### Step 14: Worktree provenance and cleanup

- **Files:** `automations/worktrees.rs (new), shared worktree lifecycle adapter`
- **Constraint:** Not started. Requires explicit phase-3 authorization; never raw recursive deletion.
- **Validation:** targeted provenance/lifecycle tests

```rust
let safety = worktrees.lifecycle(&run.workspace)?;
if safety.removal_safe { worktrees.remove(&run.workspace)?; }
```

## Acceptance Criteria

- [ ] Phase 1 reserves before dispatch, evaluates stored zones correctly, recovers without retries and exposes identical IPC/HTTP behavior.
- [ ] Phase 2 supports cadence editing, pause, Run Now, history, 24h/7d summaries and needs-you navigation with verified visuals.
- [ ] Only targeted evidence is claimed per story; coordinator verifies integration before landing.

## Checklist

- [ ] Backend/config/API docs and route snapshots follow docs/sync-matrix.md.
- [ ] UI docs, translations and screenshots follow STYLE_GUIDE.md.
- [ ] Every Rust story adds to-test.md restart checks; no agent launches desktop or builds in main.
- [ ] Phase 3 remains pending until authorized. Session reuse, RRULE and token spend are excluded.
