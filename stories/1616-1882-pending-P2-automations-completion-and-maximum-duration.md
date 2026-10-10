---
id: 1616-1882
title: "Automations: Completion and maximum duration"
status: pending
priority: P2
type: feature
created: "2026-10-09T16:14:58.215Z"
updated: "2026-10-09T16:15:28.816Z"
dependencies: ["1615-57cf"]
plan: plans/automations-scheduler.md
plan_step: Step 7
depends_on: ["stories/1615-57cf-pending-P2-automations-dispatch-and-shared-runtime-boot.md"]
---

# Automations: Completion and maximum duration

## Problem Statement

Implement completion and maximum duration for the approved Automations scheduler. See the linked plan for the fixed contract and exclusions.

## Acceptance Criteria

- [ ] Consume task/PTy/progress signals for completed/failed/needs_you; idle alone is insufficient evidence of success. Handle broadcast lag by reconciling task/session state.
- [ ] Bound runs by persisted start/deadline and stop only the owned PTY/process when max duration expires; needs_you still counts as active and expires.
- [ ] Finalize lost/unverifiable completion as unknown; preserve interrupted on boot, no retry, and ignore stale/duplicate signals after finalization.
- [ ] Capture a bounded final output snapshot for history and expose failed/needs_you transitions through event_bus plus desktop dual-emission.

## Proof

- [ ] [completeness] Completeness
- [ ] [robustness] Robustness
- [ ] [security] Security

## Work Log


- Contract: history follows task, known PTY exit, and durable progress evidence; idle alone remains active. Needs-you retains the persisted deadline; expiration stops only the bound owned session, and terminal history is immutable with bounded output.
- Production evidence: TaskRegistry, session_maps exit/state/output, and ProgressStore. Assumption: these host-owned records identify the managed launch session; no vendor-output fixture is invented.

- Main audit: `git log main --grep=1616` found only unrelated event-trigger story 1616-5e1d; main runtime had dispatch-only ticks and no completion monitor. Feature contract above applied; RED not applicable.
- GREEN: `scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml -E 'test(automations::completion::tests) | test(automations::dispatcher::tests) | test(automations::store::tests::durable_occurrence)'` through mbx/build-slot with repository TMPDIR: 15 passed in 1.857s after a 9m49s build. Initial compilation found an AppState/Arc mismatch; corrected before the passing run.
- Validation: instruction links and git diff --check passed. Native task/progress reconciliation, bounded output, terminal immutability, needs-you expiry, ownership and interrupted boot are covered. Desktop dual emission inspected; live desktop checks await Boss restart.
- Risk: medium/high (shared execution state, process cancellation and event transport). Coordinator owns independent critic, batch suites and mutation checks.
