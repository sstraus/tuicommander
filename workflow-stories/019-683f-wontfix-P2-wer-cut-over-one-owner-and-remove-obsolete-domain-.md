---
id: 019-683f
title: "WER: Cut over one owner and remove obsolete domain coupling"
status: wontfix
priority: P2
type: feature
created: "2026-11-10T08:29:18.833Z"
updated: "2026-11-13T23:21:04.315Z"
dependencies: ["018-bb68", "020-6471", "021-6e1b", "022-597e", "023-026f", "024-c9ee", "025-78a4", "026-5ab8"]
plan: plans/workflow-engine-rework.md
plan_step: Step 6
depends_on: ["workflow-stories/018-bb68-pending-P2-wer-prove-end-to-end-contract-and-connector-parity.md", "workflow-stories/020-6471-complete-P2-wer-preserve-scoped-agent-roles-and-source-bound-r.md", "workflow-stories/021-6e1b-complete-P2-wer-expose-owned-story-proposals-and-idempotent-cr.md", "workflow-stories/022-597e-complete-P2-wer-dispatch-pinned-story-subflows-with-owned-chil.md", "workflow-stories/023-026f-complete-P2-wer-bind-issue-triage-effects-and-source-linked-st.md", "workflow-stories/024-c9ee-complete-P2-wer-publish-exact-github-reviews-and-publication-i.md", "workflow-stories/025-78a4-complete-P2-wer-preserve-review-admissions-and-correction-poli.md", "workflow-stories/026-5ab8-complete-P2-wer-retain-operator-continuations-and-independent-.md"]
completed_at: "2026-11-13T23:21:04.314Z"
---

# WER: Cut over one owner and remove obsolete domain coupling

## Problem Statement

Workflow engine rework 019-683f. Scope: prepare and, only after explicit migration approval, execute recoverable cutover. Initial deliverable is an inspectable migration proposal and dry-run receipts; no implicit authority from this plan.

Delivery prerequisites: 018-bb68. These native prerequisites also appear in the linked plan and dependency metadata. All work remains pending.

Contract-first feature: record consumer effect, authoritative state, external assumptions and affected consumers before implementation. Reuse existing services/evidence where compatible; no mandatory duplicate full suites or extra critic rounds. Planning creates pending work only and does not authorize implementation or live effects.

## Acceptance Criteria

- [x] Snapshot definitions, triggers, effects and ownership; stage converted inactive drafts and reconcile receipt namespaces.
- [ ] Operator approves the boundary before draining legacy runs under their existing owner; do not guess their neutral execution position.
- [ ] Retain historical inspection without a permanent second executor.
- [ ] Transfer each trigger once and use shared effect/delivery keys where necessary to prevent repeated writes.
- [ ] Validate recoverability and rollback without replaying uncertain effects.
- [ ] Demonstrate one execution owner and no unaccounted capability loss.
- [ ] Delete obsolete role/kind scheduling and the global policy UI only after the affected parity rows pass; unresolved rows block cutover.
- [x] Source/version fixtures define compatibility decisions explicitly, not speculative backward compatibility.
- [x] GREEN: rb 20261113T193917Z-68331, head 50903ef956 (tree 45c79207e3), rustc 1.99.1, build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib --no-fail-fast -E "test(cutover) | test(workflows::migration::)": 42 passed, 0 failed
- [x] REALITY: effect=cutover_snapshot returns definitions with staged conversions, trigger owners with receipt namespaces, open legacy runs and blockers, and writes nothing (cutover_snapshot_action_reports_the_staged_binding_and_legacy_debt_without_writing) ; state=automations.json, automation run store, WorkflowStore conversions and both binding tables, legacy workflow run store ; assumptions=none, no external read ; radius=new read-only automation action (IPC/HTTP), enabled_trigger_bindings now filters trigger_bindings, active_run_ids pub(crate), migration_conversion test-only
## Work Log

### 2026-11-11 dependency planning after Boss decision

Boss selected conversion (option A, "converti") rather than permanent legacy scheduling. Direct prerequisites now include pending 020-026 as well as 018. This records a delivery decision only: converted drafts, parity evidence, recoverable proposal and explicit cutover-boundary approval remain required. No trigger transfer, live-run position guess, restart, store migration or external write is authorized by this dependency edit.

### 2026-11-13T19:49:32.254Z - Contract (t-019w, 2026-11-13): consumer = operator reading automation_action cutover_snapshot (IPC + POST /automations) before approving any transfer; real state = automations.json, automation run store, WorkflowStore (legacy drafts, neutral_migration_conversions, workflow_trigger_bindings, neutral_trigger_bindings) and legacy workflow run store, read only; assumptions = none about external systems (no GitHub read). Implemented 7b80a55a1a+50903ef956 on feat/wer-019-cutover-prep (merges feat/wer-018-parity 0570c9f597): automations/cutover_snapshot.rs, workflows/migration/staged.rs, fixtures/cutover-compatibility.json, docs/backend/workflow-cutover-019.md (transfer plan, rollback check, deletion list with file:line; none executed). Findings: neutral and legacy engines have no fence between them; legacy keys (head/digest) vs neutral epoch generations are not comparable, neutral enable baselines open resources (neutral_github_admission.rs:55-67); no neutral->legacy rollback bridge; neutral_trigger_self_events has no production writer (neutral_trigger_store.rs:26/205), so a run own label write re-emits an issue event: issue transfer blocked until dispositioned. No trigger transfer, drain, store migration, external write or deletion. GREEN rb 20261113T193917Z-68331, head 50903ef956 tree 45c79207e3, rustc 1.99.1: build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib --no-fail-fast -E "test(cutover) | test(workflows::migration::)" -> 42/42 PASS (6 new). First run 20261113T193153Z-45251 failed compile only (test variable shadowed blockers fn). No clippy (landing owns it).

### 2026-11-13T20:21:14.516Z - Pre-boundary work landed ad4391a607; transfer waits for Boss approval (open ask A8, 2026-11-13)

### 2026-11-13T23:21:04.108Z - Superseded by Boss 2026-11-14: legacy engines are deleted (3034-365e), no transfer

