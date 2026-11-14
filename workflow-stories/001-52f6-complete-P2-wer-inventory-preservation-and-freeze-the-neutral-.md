---
id: 001-52f6
title: "WER: Inventory preservation and freeze the neutral workflow contract"
status: complete
priority: P2
type: feature
created: "2026-11-10T08:29:18.816Z"
updated: "2026-11-10T09:55:08.498Z"
dependencies: []
plan: plans/workflow-engine-rework.md
plan_step: Step 1
started_at: "2026-11-10T09:47:19.512Z"
completed_at: "2026-11-10T09:55:08.497Z"
---

# WER: Inventory preservation and freeze the neutral workflow contract

## Problem Statement

Workflow engine rework 001-52f6. Scope: inspect definitions, editor controls, service APIs, persisted runs/events, triggers, permissions, coordinator capability inventory and Fabbrica consumers. Produce a field/operation/effect/permission/user-action matrix with destination, owner, reusable seam, validation evidence and unknowns. This is design work; do not implement a second scheduler.

Delivery prerequisites: none. These native prerequisites also appear in the linked plan and dependency metadata. All work remains pending.

Contract-first feature: record consumer effect, authoritative state, external assumptions and affected consumers before implementation. Reuse existing services/evidence where compatible; no mandatory duplicate full suites or extra critic rounds. Planning creates pending work only and does not authorize implementation or live effects.

## Acceptance Criteria

- [x] Every existing capability in the inventory has a named destination or an explicit blocking unknown; catalog existing support separately from missing work.
- [x] Record actual serialized definitions/events as migration fixtures with provenance, not invented examples.
- [x] Inventory publication pins, history, generation fences, resource admission, artifact freshness, branch-local questions, cancellation, recovery, budgets and all transports.
- [x] Identify domain dependencies in RunSnapshot/store APIs and verify what transactions, event cursors and executor services can be reused unchanged.
- [x] Inventory each required Jira, GitHub, GitLab and Telegram operation, existing clients/tool adapters and missing actions.
- [x] Identify actual affected documentation from the sync matrix.
- [x] Name tests/evidence to reuse; source presence is not runtime proof.
- [x] GREEN: Static inventory check verifies every baseline ID, its source intent and named plan owner, recorded event/definition fixtures, referenced seams, and whitespace.
- [REJECTED] REALITY: The observable effect is a source-backed preservation inventory; the authoritative inputs are the pinned baseline, native source and recorded fixture provenance. Connector live behavior is not assumed. Scope is documentation and story metadata only; runtime consumers do not change. (Replaced by the structured REALITY criterion required by the validator.)
- [x] REALITY: effect=readers obtain a source-backed preservation inventory ; state=recorded RunStore fixtures, native source and pinned baseline ; assumptions=live connector behavior is not assumed ; radius=inventory readers and later rework owners, no runtime changes
## QA

Documentation-only source inventory; recorded v2 fixture provenance and all source capability rows verified by static checks. Live provider behavior, neutral runtime parity and future operation implementations remain explicit migration blocks.

## Work Log

### 2026-11-10T09:33:33.414Z - Initial source inventory: docs/backend/workflow-baseline/{contract.md,capabilities.json,sources.json} records prior preservation scope and source-only evidence; src-tauri/src/workflows/run/model.rs RunSnapshot retains plan, story, PR and issue domain fields; run/store.rs uses immediate SQLite transactions. Fabbrica paths include workflows/file.rs and fabbrica_attention.rs. This does not establish unchanged reuse, complete connector operations, real serialized migration fixtures or runtime parity; all criteria remain open.

### 2026-11-10T09:40:06.402Z - Added preliminary docs/backend/workflow-rework-inventory.md: all 147 baseline IDs have explicit migration blocks and owner, existing captured v2 graph-start streams have provenance, and connector/runtime/documentation seams are listed. Static Python check verified each baseline ID occurs once and both recorded event streams parse and contain graph starts. This is not a complete parity, authority, executor-reuse or connector-operation proof; no acceptance criterion was checked. Required story-close --check currently refuses because it hardcodes repo/stories, which points to the original main store rather than workflow-stories.

### 2026-11-10T09:47:19.077Z - Inventory now records the 147 original source intents with candidate rework owners and explicit unverified migration blocks; preserved native serialized v2 events and embedded published definitions are linked to their original capture provenance. Added current definition/editor, runtime fences, persistence and executor reuse, desktop/HTTP/MCP/SSE/file surfaces, Fabbrica consumers, connector operation/evidence limits and documentation obligations. Static ID/fixture/source checks passed; no runtime parity or new execution is claimed.

### 2026-11-10T09:48:08.002Z - Static validation passed: Python parsed both recorded v2 RunStore streams, confirmed each graph-start holds its serialized definition, checked 147 unique baseline IDs and original intent rows, candidate story IDs in the approved plan, explicit migration blocks, selected cited source paths and inventory whitespace; git diff --check passed. Initial assertion incorrectly looked for definition directly in event; inspection showed execution.definition and the corrected check passed. No runtime test or live connector parity is claimed. Candidate replacements stay blocked until their owning stories verify them.

### 2026-11-10T09:54:43.850Z - Extended the inventory with a per-source-intent candidate story crosswalk, native node/editor and current user-action/authority surfaces, exact transaction/cursor and command-executor reuse limits, recorded connector evidence and missing workflow adapters, callback persistence, pinned trigger binding and Fabbrica consumer boundaries. No connector writes, application tests or cutover were performed.

### 2026-11-10T09:55:08.094Z - Final static validation passed on Python 3.9.6: all 147 baseline IDs occur once with unchanged source intent, plan destinations and explicit migration blocks; both captured v2 graph-start streams contain serialized definitions and match their documented SHA-256 hashes; selected cited source paths and document whitespace exist. git diff --check passed. Corrected story-close.sh --check passed. No runtime or external service validation was performed.

### 2026-11-10T09:55:38.727Z - After completion renamed the story file, repaired only the derived workflow-stories/001 dependency path in 002-eeb6; its native dependencies list still contains 001-52f6 unchanged. Corrected story-close --check passed again.
