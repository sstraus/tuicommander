---
id: 026-5ab8
title: "WER: Retain operator continuations and independent closure consent"
status: complete
priority: P2
type: feature
created: "2026-11-11T16:47:31.452Z"
updated: "2026-11-12T00:24:07.763Z"
dependencies: ["005-f907", "008-22c4", "015-608c"]
depends_on: ["workflow-stories/005-f907-complete-P2-wer-own-durable-asynchronous-activities-effects-an.md", "workflow-stories/008-22c4-complete-P2-wer-expose-native-repository-and-development-opera.md", "workflow-stories/015-608c-complete-P2-wer-expose-the-same-workflow-capabilities-through-.md"]
plan: plans/workflow-engine-rework.md
plan_step: Step 5
risk: high
started_at: "2026-11-11T23:48:35.801Z"
completed_at: "2026-11-12T00:24:07.762Z"
---

# WER: Retain operator continuations and independent closure consent

## Problem Statement

Old consumers: story_delivery and resolve_plan Pause.resume_to, human/automatic closure, release/Boss-wait consent and affected coordinator preservation rows. Native effect approval is already exact and durable, but consenting to an effect is not the legacy operator continuation or final closure decision. Add a typed retained operator-decision contract and compose existing independent closure/release services without treating readiness as permission.

Operator waits and closure/release consent are grouped because they share a retained exact-subject decision lifecycle, but their decision kinds and authority checks must stay distinct. Do not rebuild the release service or generic notifications.

## Acceptance Criteria

- [x] Before implementation, record the observable consumer effect, authoritative native state, external assumptions, affected consumers and owned paths in this worklog; audit existing implementation and compatible evidence first.
- [x] Expose an inspectable operator wait/continuation bound to the exact run, pinned publication, source node/resume target, sequence, artifact and current source generation. Resume reaches only the configured continuation and preserves owner/effect history; no new scheduler or arbitrary jump position.
- [x] Keep resume, approval of one effect, human/automatic closure and release publication consent as distinct typed decisions. Reuse the 005 question/approval ledger and native closure/release authority checks; record the human actor, reason, time and exact action/target/artifact where consent is required.
- [x] Automatic closure still requires current independent review and every configured check. Readiness, model labels, completed children, timeout, notification delivery or operator resume cannot grant release/push/API approval. Stale or duplicate answers cannot release a newer artifact or a different wait.
- [x] Preserve frozen release/next/Boss-wait membership and responsibility references through existing release/portfolio services. This story adds decision/continuation bindings only; readiness, release rules, notification delivery and scheduling stay existing-service composition obligations in 017/018, with unverified rows still blocking cutover.
- [x] Use existing notification activities for an explicitly configured operator destination; notification is optional observation, not proof of consent. Targeted cases catch effect approval used as resume, wrong resume_to, duplicate/stale answer advancement, restart losing a wait and readiness automatically publishing.
- [x] Expose versioned catalog inputs/outputs, named outcomes and exact handler/descriptor/prompt/subflow pins through existing neutral admission and the shared Designer/IPC/HTTP/MCP contracts; no legacy scheduler wrapper, role-specific kernel branch or caller-supplied authority.
- [x] Reuse the existing effect ledger for ownership, exact approvals, cancellation and original-effect reconciliation. A lost reply or uncertain effect never authorizes a fresh write; draft edits cannot alter an active run.
- [x] Prove named failure modes with real native services in throwaway local repositories and focused tests of changed paths/consumers. Record tree, command, task id, tool versions or unmeasured versions and result; reuse compatible passing evidence. No per-story full suite, clippy, mutation run or automatic extra critic.
- [x] Planning leaves this story pending and does not authorize implementation, cutover, restart, push, live user-data/store migration or live writes outside the already approved private sandbox repository. Any sandbox effect must keep its existing exact action/target approval and cleanup; missing live evidence remains an explicit gap.
- [x] REALITY: effect=exact pinned operator continuation and independent human/automatic closure/release consent; state=005 owned question/answer ledger, native actor/workspace/current artifact/source generation, selected StoryBackend and WF10/WF15 journals; assumptions=optional notifications and external017/018 migration evidence cannot supply consent; radius=typed native operator/shared API/host, opaque generic answer receipt, existing closure/release bindings and targeted tests. Original pre-code contract is the2026-11-11T23:48:35.955Z worklog.
## QA

Automated:9 targeted greens with final2 affected tests replayed; code-audited native human closure/shared schema and optional notifications. No live service or restart validation claimed.

## Work Log


### 2026-11-11T23:48:35.955Z - REALITY contract before code: observable effect=one exact operator wait resumes only its pinned configured continuation; human closure, automatic closure, release consent and one-effect consent remain distinct and cannot confer each other authority. Authoritative state=005 owned effect/question/answer ledger, NativeContext backend actor/workspace/check definitions, retained source generation and immutable publication/control graph, actual StoryBackend transition rules and existing release/portfolio snapshots where available. Assumptions=notifications optional observation; unavailable native release snapshot/closure proof must fail closed, not be invented. Radius=generic recorded answer receipt in existing effect ledger (no operator scheduler branch), typed operator activity/service/shared neutral action, native host authorizations and current native closure/check consumers, synchronized docs and targeted sandbox tests. Reuse025 independent review/check evidence. Plan: same005 answer decision with opaque backend receipt retained atomically, typed host action authenticates actor and exact run/key/revision/sequence/kind/target/artifact/generation/source/resume. Ordinary fixed outcome edge moves graph; no arbitrary position. Generic effect Answer cannot satisfy operator activity without authentic typed receipt. No app/live service/cutover/migration/push authorization. Release/portfolio owning native seam still being located; missing owner remains an explicit gap.


### 2026-11-12T00:03:52.800Z - Implementation audit: portfolio WF15 release scope/authorization and WF10 portfolio release are existing authority seams per coordinator e25a99fd. Bind frozen scope digest inside the existing authorization transaction; no new release service or publication. Automatic closure discovers actual latest stopped independent native review plus every configured verification effect from the same run, then calls selected StoryBackend Approve with bound actor; human closure uses authenticated owner receipt. Existing notification activities stay optional composition. Targeted proof will catch generic bool approval, wrong target/caller/run/sequence, durable reopen, artifact replacement, original reply reconciliation and readiness without release consent. Catalog capture remains integrator-owned. No live actions.


### 2026-11-12T00:11:42.227Z - First targeted rb task20261112T000506Z-79960 stopped at compilation: two nested test module paths and release binding visibility; no tests executed. All1355 frozen Rust hashes matched, clean/nonstale, stopped/released/collected, Rust1.99.1 from compiler path. Fixed those causes and removed unused import. Added real native closure pipeline proof: actual independent review adapter (controlled external reply only), actual configured Git verification, and isolated StoryBackend transition, including refusal when the configured check was omitted. Added retained trigger generation supersession case. Ordinary approval winner and existing consent regressions remain selected because this wave touches their ledger/helper. No broad run.


### 2026-11-12T00:18:23.360Z - Targeted GREEN9/9 on rb20261112T001146Z-36152, tree8fd702578416b398e6c82362153921ce490ce81aed99cddab513f0d670c176e2; all1356 frozen executed/current Rust hashes match, clean/nonstale stopped/released/collected. Six new operator tests plus three005/release regressions. One test-only unused assignment warning removed; now checking actual final-node advancement and actual native Findings review refusing closure as two affected tests only. Existing two intake dead-code warnings unchanged. Rust1.99.1, nextest/target metadata unmeasured. Real local Git/SQLite/native review adapter/current check/isolated StoryBackend; external model reply controlled, no live ACP/publication.


### 2026-11-12T00:22:50.102Z - Final affected GREEN2/2 rb20261112T001825Z-3634 tree1778d03a01199daa1022ed9248fea53e1e3e16003ea587413acb4a49c1214340; all1356 frozen executed/current Rust hashes matched before final comment-only cleanup, clean/nonstale stopped/released/collected. Actual graph reaches done with pinned publication unchanged; actual native Findings review refuses closure, missing check refuses closure, passing native review+allchecks closes through isolated StoryBackend and stale revision refuses repeat. Reuse remaining7 greens from001146Z wave; unchanged production and unaffected test files. Final only source difference removes one test comment line to meet300line limit, with comment-stripped token equality verified and rustfmt --check green; exact before/after hashes in026-operator/post-proof-comment-only.json. No duplicate build for comment cleanup. Missing referenced docs/backend/workflow-template-migration.md remains017 documentation/scenario obligation; existing portfolio-delivery.md is authoritative seam. No live/paid ACP, notification delivery, publication, desktop, restart, push, migration or cutover evidence claimed.


### 2026-11-12T00:23:27.473Z - Acceptance audit:1 contract precedescode;2 exact wait/publication/key/sequence/source/artifact/fixededge and reopen/finalnode proved;3 typed Resume/HumanClosure/AutomaticClosure/ReleaseConsent/PortfolioRelease, backendOwner receipt and real StoryBackend/release journal compose existingauthority;4 actual review Findings/missingchecks/currentpassedchecks plus wrong/stale/foreignanswers proved, no readiness grants publication;5 frozen scope digest checked atomically and WF10 responsibilityid/revision retained,017/018 scenario/readiness/connector rows remain cutoverblockers;6 existing optional configured notification activities unchanged, delivery never produces typed answer, named wait failures covered;7 installedv1 catalog/shared NeutralAction/schema and normal admission/publication pins, no transport fork;8 same005 ledger, current generation/artifact, no lost-write replay, pure originalresume reconciliation, no draft mutation;9 exact targeted receipts/provenance recorded;10 implementation authorized by coordinator order, no live/cutover/restart/push/migration. Human branch native StoryBackend authority is code-audited and shares proven transition seam; notification delivery/live externalservices not claimed. RESULT: bounded026 implementation criteria satisfied with external017/018 obligations explicit.


### 2026-11-12T00:24:06.923Z - Lifecycle gate requires REALITY as a structured acceptance criterion, not only the pre-code worklog recorded2026-11-11T23:48:35.955Z. Adding the criterion now as an index to that existing contract; this does not claim it was created before implementation. All behavior/proof remains unchanged. Initial completion refused; no commit/staging succeeded in that attempt.


### 2026-11-12T00:27:02.286Z - Commit pre-hook refused unsorted module declarations in run/mod.rs. Targeted rustfmt moved only native_operations declaration before native_operator registrations; no code/attribute/body change. All staged Rust files pass rustfmt --check skip_children. Exact before/after hashes and lines preserved026-operator/post-proof-module-format.json. No behavior rerun for declaration-order formatting.


## Delivery Notes

Size estimate: M. Planning only; status stays pending.

Operator waits and closure/release consent are grouped because they share a retained exact-subject decision lifecycle, but their decision kinds and authority checks must stay distinct. Do not rebuild the release service or generic notifications.

Authoritative existing seams: `src-tauri/src/workflows/run/neutral_effects.rs`, `src-tauri/src/workflows/neutral_api/model.rs`. The operation/consumer audit is `docs/backend/workflow-template-migration.md`; the actual catalog capture and evidence qualifications are `src-tauri/src/workflows/fixtures/neutral-native-catalog.json` and its sibling README. Inspect and reuse these; fixture presence is not live-service success.
