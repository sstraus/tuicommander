---
id: 021-6e1b
title: "WER: Expose owned story proposals and idempotent creation"
status: complete
priority: P2
type: feature
created: "2026-11-11T16:47:30.222Z"
updated: "2026-11-13T00:51:09.000Z"
dependencies: ["005-f907", "008-22c4"]
depends_on: ["workflow-stories/005-f907-complete-P2-wer-own-durable-asynchronous-activities-effects-an.md", "workflow-stories/008-22c4-complete-P2-wer-expose-native-repository-and-development-opera.md"]
plan: plans/workflow-engine-rework.md
plan_step: Step 5
risk: high
started_at: "2026-11-11T17:02:47.247Z"
completed_at: "2026-11-13T00:51:09.000Z"
---

# WER: Expose owned story proposals and idempotent creation

## Problem Statement

Old consumers: resolve_plan create/coordinator and delivery reviewers allowed to propose follow-up work. story_adapter::create_story/apply already implement backend operations, but the installed catalog only exposes story.transition. Add the missing proposal-to-created-story receipt contract, preserving each source proposal, source review, responsibility and per-review cap. It must not become an implicit lifetime cap or a second story store.

Proposal acceptance and create are grouped because they share one source-review/proposal idempotency and responsibility contract. Existing transition stays in 008; no duplicate backend.

## Acceptance Criteria

- [x] Before implementation, record the observable consumer effect, authoritative native state, external assumptions, affected consumers and owned paths in this worklog; audit existing implementation and compatible evidence first.
- [x] Expose bounded typed proposal validation and story creation using the existing selected story backend, create_story/apply and backend version/status/actor rules. Preserve plan/source story, source review/attempt, responsibility, evidence and created-story identity in retained native receipts.
- [x] Enforce maxStoryProposals per admitted review, including zero. Replaying one review/proposal cannot create a second story; a later independent review receives its own cap. Rejects, corrections and story creation counters remain distinct.
- [x] Persist original proposal-to-story mapping before reporting completion. On lost backend response reconcile the original create identity; if the backend cannot prove the outcome, remain uncertain and require disposition rather than issuing another create. Unknown backend capability blocks only the affected operation.
- [x] Targeted cases catch duplicate creation after a lost reply, wrong per-review/lifetime cap, responsibility loss, stale backend version, unauthorized actor and changed proposal bytes reusing an old receipt.
- [x] Expose versioned catalog inputs/outputs, named outcomes and exact handler/descriptor/prompt/subflow pins through existing neutral admission and the shared Designer/IPC/HTTP/MCP contracts; no legacy scheduler wrapper, role-specific kernel branch or caller-supplied authority. _(verified: main d3a98e4562: native/story.create@1 is in src-tauri/src/workflows/fixtures/neutral-native-catalog.json, checked by native_catalog_wire_matches_shared_designer_fixture and the NeutralDesigner.native Vitest; the 2962-554e coordinator create body fixes review and cap as constants (no caller-supplied authority))_
- [x] Reuse the existing effect ledger for ownership, exact approvals, cancellation and original-effect reconciliation. A lost reply or uncertain effect never authorizes a fresh write; draft edits cannot alter an active run.
- [x] Prove named failure modes with real native services in throwaway local repositories and focused tests of changed paths/consumers. Record tree, command, task id, tool versions or unmeasured versions and result; reuse compatible passing evidence. No per-story full suite, clippy, mutation run or automatic extra critic.
- [x] Planning leaves this story pending and does not authorize implementation, cutover, restart, push, live user-data/store migration or live writes outside the already approved private sandbox repository. Any sandbox effect must keep its existing exact action/target approval and cleanup; missing live evidence remains an explicit gap. _(verified: no cutover, restart, push or live store migration was done for this story; external create-to-mapping uncertainty stays an explicit gap)_
- [x] REALITY: effect=approved bounded proposals create one story and retain responsible caller/source/effect receipt; state=exact run effect ledger and selected project StoryBackend; assumptions=external adapter create-to-mapping uncertainty requires disposition, no replay; radius=resolve_plan coordinator and delivery reviewer proposal consumers
- [x] GREEN: final staged tree 2073ff3a61dbd3ba5b81ddf5bb8e32aff2535af6; rb task 20261111T170441Z-4524; filtered native_story_create tests 4 passed, exit 0
## Work Log


### 2026-11-11T17:02:47.051Z - Contract recorded before implementation in the now-deleted duplicate 021-2fd9: approved creation uses project-selected StoryBackend::create_story_once; external delegates to story_adapter::create_story. SQLite reservations pin per-review cap, source, caller and payload before backend writes. Lost replies retain capacity; reconciliation reads original mapping without creation replay. Consumers are resolve_plan coordinators and delivery reviewers. Owned paths: native_story_create modules plus additive native catalog, dispatch and host runtime routing, workflow docs and manual tracker. Source/version and evidence added after assigned criteria were received.


### 2026-11-11T17:02:47.410Z - First targeted rb task 20261111T165109Z-60494: six tests, five passed, one setup failure (reservation test read effects before dispatch), no production failure. Fixed test to prepare effect by dispatch. Added source/version, revoked actor and original mapping reconciliation coverage; second wave running. Existing native transition and runtime story consumer checks passed on unchanged code.


### 2026-11-11T17:03:35.339Z - Static review caught a zero-cap persistence gap: returning refusal rolled back first scope policy. Commit the cap policy before returning exhausted, so a later request cannot increase zero. Added this distinct boundary assertion; independent review still gets its own cap. Final test rerun will cover this delta.


### 2026-11-11T17:10:02.657Z - GREEN: rb task 20261111T170441Z-4524, nextest run 3df49a25-76f0-45e2-a087-780734c4dc72, staged tree 2073ff3a61dbd3ba5b81ddf5bb8e32aff2535af6. Command: build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E test(workflows::run::native_story_create::). Four passed in 0.509s, exit 0. Source hashes: Gits/.tmp/workflow-rework/021/final-source-hashes.json. Tool versions unmeasured. Two inherited dead-code warnings in neutral_gitlab_intake and neutral_triggers. Boundary cases prove native creation, approval and project fences, source revision/revoked caller refusal, read-only recovery after discarded receipt, changed original request refusal, per-review zero/independent caps, cap escalation and repeated reservation refusal. Reused first-wave native transition and runtime consumer passing results; no duplicate suite.


### 2026-11-11T17:10:02.783Z - Limits: installed descriptor native/story.create@1, handler native.story.create@1, request/receipt object contract. Integrator owns catalog fixture regeneration and template source-review/attempt binding. Review string must identify the original admitted review including its attempt; graph approval pins scope/cap. No external adapter live credential writes performed. Native backend recovery was tested; external create-to-mapping gap stays uncertain by existing backend contract. No second content store. Shared dispatcher/runtime changes are typed routing, mutation preparation and read-only reconciliation. Rust changes need manual backend rebuild/restart; tracker updated. No restart, push, landing or live data migration.


## Delivery Notes

Size estimate: M. Planning only; status stays pending.

Proposal acceptance and create are grouped because they share one source-review/proposal idempotency and responsibility contract. Existing transition stays in 008; no duplicate backend.

Authoritative existing seams: `src-tauri/src/story_adapter/mod.rs`, `src-tauri/src/workflows/run/native_operations.rs`. The operation/consumer audit is `docs/backend/workflow-template-migration.md`; the actual catalog capture and evidence qualifications are `src-tauri/src/workflows/fixtures/neutral-native-catalog.json` and its sibling README. Inspect and reuse these; fixture presence is not live-service success.

### 2026-11-13T00:51:09.000Z - Closed after the landing of 2962-554e on main d3a98e4562. Evidence was checked against main by code and test-name inspection; no test was rerun.
