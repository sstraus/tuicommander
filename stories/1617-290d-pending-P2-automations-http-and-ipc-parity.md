---
id: 1617-290d
title: "Automations: HTTP and IPC parity"
status: pending
priority: P2
type: feature
created: "2026-10-09T16:14:58.217Z"
updated: "2026-10-09T16:15:29.664Z"
dependencies: ["1611-fa3f", "1613-67f3", "1615-57cf", "1616-1882"]
plan: plans/automations-scheduler.md
plan_step: Step 8
depends_on: ["stories/1611-fa3f-pending-P2-automations-cron-and-timezone-semantics.md", "stories/1613-67f3-pending-P2-automations-scheduler-admission-and-catch-up.md", "stories/1615-57cf-pending-P2-automations-dispatch-and-shared-runtime-boot.md", "stories/1616-1882-pending-P2-automations-completion-and-maximum-duration.md"]
---

# Automations: HTTP and IPC parity

## Problem Statement

Implement http and ipc parity for the approved Automations scheduler. See the linked plan for the fixed contract and exclusions.

## Acceptance Criteria

- [ ] Expose list/get/create/update/delete/run_now/list_runs, pause via update, schedule preview/presets and run aggregates through identical HTTP and Tauri shapes.
- [ ] Transport handlers call shared Rust business logic; preserve per-id definition edits and immutable run snapshots; validation errors are consistent.
- [ ] Add COMMAND_TABLE mappings and mapping assertions; regenerate command_table_paths.txt with Vitest and pass registered-route PATCH probes.
- [ ] Document HTTP, IPC, backend configuration, specification and feature availability; add targeted API error and happy-path tests.
- [x] GREEN: 336 targeted Vitest tests, TypeScript, 5 targeted nextest tests including registered-route PATCH probes, and fmt-changed pass
## Proof

- [ ] [completeness] Completeness
- [ ] [robustness] Robustness
- [ ] [security] Security

## Work Log

### 2026-10-10T11:00:17.846Z - Contract: HTTP POST /automations/action and IPC automation_action share one Rust input parser and implementation. Consumers observe persisted per-id definitions, normalized save replies, cursor-aware list/preview, retained immutable run history, UTC aggregates, and owner-guarded Run Now. State comes from instance-scoped DefinitionStore and RunStore; calls execute on the addressed backend with no remote fallback. Existing schedule/admission/dispatcher behavior is reused; no external provider fixtures or live agents are required for API tests.

### 2026-10-10T11:05:22.162Z - REALITY: effect=matching HTTP/IPC JSON and persisted per-id edits/history/previews/aggregates; state=DefinitionStore locked document and owner-guarded RunStore, with reservations produced by existing store API in isolated tests; assumptions=no live agent/provider dispatch claimed, native macOS validation only; radius=shared desktop/headless API, command mapper and Automations dialog. API tests cover stale pause overwrite, deletion/history retention, immutable run snapshots, Once cursor consumption, UTC window bounds and invalid actions. Existing phase-1 dispatcher deliberately rejects recurring Run Now; retained as documented.

