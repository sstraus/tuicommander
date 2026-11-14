---
id: 024-c9ee
title: "WER: Publish exact GitHub reviews and publication intents"
status: complete
priority: P2
type: feature
created: "2026-11-11T16:47:30.986Z"
updated: "2026-11-13T08:05:00.000Z"
dependencies: ["005-f907", "008-22c4", "009-e32f", "010-978f"]
depends_on: ["workflow-stories/005-f907-complete-P2-wer-own-durable-asynchronous-activities-effects-an.md", "workflow-stories/008-22c4-complete-P2-wer-expose-native-repository-and-development-opera.md", "workflow-stories/009-e32f-complete-P2-wer-deliver-typed-http-operations-and-the-shared-c.md", "workflow-stories/010-978f-complete-P2-wer-admit-selectable-github-issue-and-pr-workflows.md"]
plan: plans/workflow-engine-rework.md
plan_step: Step 5
risk: high
started_at: "2026-11-11T17:04:02.560Z"
completed_at: "2026-11-13T08:05:00.000Z"
---

# WER: Publish exact GitHub reviews and publication intents

## Problem Statement

Old consumers: pr_review GitHubReview COMMENT/APPROVE/REQUEST_CHANGES and edited delivery GitHubPublication intents, including MarkReady. native.publication.github already supports approved branch/draft-PR/comment/close/delete actions; a PR comment is not a GitHub review event and MarkReady is absent. Extend the existing native provider contract instead of creating another GitHub client or granting publication authority from a successful review.

Review events and publication intents share the existing account/PR/artifact-bound native GitHub effect contract; keep their distinct action types, receipts and approvals. Do not combine their authority.

## Acceptance Criteria

- [x] Before implementation, record the observable consumer effect, authoritative native state, external assumptions, affected consumers and owned paths in this worklog; audit existing implementation and compatible evidence first.
- [x] Expose actual GitHub review publication with the exact COMMENT/APPROVE/REQUEST_CHANGES event, body/findings, repository/account, PR, head/base and current source generation. Bind independent review evidence to that exact artifact; plain PR comments cannot satisfy the review effect.
- [x] Reuse native.publication.github, existing account-aware review/PR services and the original-effect approval/reconciliation ledger. Add the missing explicit MarkReady action and source publication-intent translation while preserving draft/create/push semantics and native authority floors.
- [x] Require separate current approval for each exact action/target/artifact; review Pass, readiness, graph completion and prior approvals grant no publication authority. A newer head/generation or changed destination invalidates the old action. Read back provider state and retain uncertainty if the exact effect cannot be proven.
- [x] Targeted cases catch COMMENT substituted for APPROVE, stale-head review, MarkReady on the wrong PR, old action approval reused for another target, and duplicate review/publication after lost response. Any authorized live check uses only the approved private sandbox and cleans its unique effects.
- [x] Expose versioned catalog inputs/outputs, named outcomes and exact handler/descriptor/prompt/subflow pins through existing neutral admission and the shared Designer/IPC/HTTP/MCP contracts; no legacy scheduler wrapper, role-specific kernel branch or caller-supplied authority.
- [x] Reuse the existing effect ledger for ownership, exact approvals, cancellation and original-effect reconciliation. A lost reply or uncertain effect never authorizes a fresh write; draft edits cannot alter an active run.
- [x] Prove named failure modes with real native services in throwaway local repositories and focused tests of changed paths/consumers. Record tree, command, task id, tool versions or unmeasured versions and result; reuse compatible passing evidence. No per-story full suite, clippy, mutation run or automatic extra critic.
- [x] Planning leaves this story pending and does not authorize implementation, cutover, restart, push, live user-data/store migration or live writes outside the already approved private sandbox repository. Any sandbox effect must keep its existing exact action/target approval and cleanup; missing live evidence remains an explicit gap.

## Work Log


### 2026-11-11T17:03:46.387Z - Contract: consumer observes a submitted exact-head GitHub review or separately approved MarkReady. Authority remains the existing account-bound RepoApi, NativeContext, immutable invocation and effect approval ledger. Evidence must bind caller/workspace/base/head/tree and independent native reviewer. External prerequisites are GitHub API permissions and command-scoped credentials; live writes only sstraus/tuic-e2e-sandbox. Owned paths native_github.rs and its review submodule, documentation and this story. Existing publication v1 descriptor is extended, no second client or regenerated catalog. Initial contract was retained in working documentation before corrected story became available. Targeted rb task 20261111T165638Z-20895 failed compilation E0061 in one new test; no test executed. Fixed missing base parameter, added focused MarkReady and approval target cases; retry running in targeted-2.log. No live writes: GH_TOKEN absent. Source-template intent translation and source-generation association still require implementation; do not mark story complete.


### 2026-11-11T17:08:09.407Z - Retry task 20261111T170318Z-89640 exited75 before any remote command: tree changed during sync while importing the coordinator-authorized shared story commit. Completed new source-generation gate against retained trigger intents/current event generations/resource ownership, with receipt generation and pre/post checks. Added optional source_intent validation for Push/CreateDraftPr/MarkReady against exact native action. Added real SQLite/Git source-replacement fixture; it explicitly attaches an existing persisted run and does not claim public ingress integration. Existing ReviewEvent enum is reused. Frozen retry will include these changed cases; no full suite or live claim.


### 2026-11-11T17:14:43.060Z - GREEN: rb task20261111T170842Z-45984, nextest e865cf0c-46e9-4e5f-b243-dbd0d759a157, staged tree26d73dad7ebdfd43602ce91022e0ad62a2f53a45. build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E test(review_publication_) or test(github_empty_pr_body_null_readback_is_accepted):9 passed in1.177s after3m30s compile, exit0, RB_BOX_DIRTY=no. Tool versions unmeasured on rb; initial compile log identifies rustc1.99.1 aarch64 Linux. Only two inherited unused trigger methods warn. Log /Users/stefano.straus/Gits/.tmp/wer023/targeted-3.log. Fake PrApi tests prove wire request/readback behavior, not a live provider response; real Git/SQLite fixtures prove exact current authority and retained generation refusal. No live credentials/readiness/review evidence, desktop restart, full suite or cutover claimed. Complete legacy graph translation and public source-start composition remain017/018 obligations. This story remains in_progress for integration/live acceptance; no criterion is waived.

### 2026-11-13T08:56:00.000Z - Audit and completion on feat/wer-024-finish (base main d3a98e4562). Evidence per criterion: AC1 contract is the 2026-11-11T17:03 entry. AC2 GitHubAction::Review (native_github.rs:47) posts commit_id+event and reads back id/commit/state/body/head/base (review.rs publish); publish_approved binds body, verdict and reviewer session to observed_review of exact base/head/tree in the same run; Comment is a separate action; receipt carries source_generation (native_github.rs:296). AC3 one client (RepoApi, publish_owned), MarkReady via markPullRequestReadyForReview with exact node/head/base read-back; NEW: convert_issue carries source_intent push/create_draft_pr/mark_ready next to each action, so validate (intent::require) refuses a converted node mapped to another effect; the composed issue harness asserts the intent is present. pr_review conversion is convert_pr_review.rs. AC5 cases: COMMENT-for-APPROVE = review_publication_pins_all_three_verdicts (oracle now literal GitHub wire values, not serde) and NEW review_publication_verdict_requires_matching_independent_evidence; stale head = review_publication_refuses_stale_head_before_write; MarkReady wrong PR = review_publication_mark_ready_pins_node_and_observes_state, review_publication_source_generation_replacement_refuses; reused approval = review_publication_exact_approval_cannot_change_action_or_target; duplicate after lost response = review_publication_lost_post_response_stays_uncertain and NEW review_publication_lost_reply_is_never_written_again (publish refused before account resolution, NativeHost::reconcile refuses, MarkReady and Review). AC6 native.publication.github@1 is in the installed catalog and the shared Designer wire fixture (native_catalog_wire_matches_shared_designer_fixture); converted graphs validate against the installed catalog through neutral validation. AC7 authorize requires dispatched, unobserved, approved effect (native_operations.rs authorize_with); generic ledger proof lost_write_response_survives_restart_and_reconciles_without_second_write. GREEN rb: build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(review_publication_) or test(issue_mark_ready_requires) or test(composed_issue_execution) or test(converting_seeded_issue_delivery)': 14 passed, RB_BOX_DIRTY=no (~/Gits/.tmp/wer024/targeted-1.log); -E catalog fixture, converting_seeded_pr_review, converted_pr_review_binds, lost_write_response_survives_restart: 4 passed (targeted-2.log). rb rustc/nextest versions unmeasured. GAP: no live sandbox write; no command-scoped GH_TOKEN and token stores are off limits, so ignored native_github_live_sandbox_publication did not run and live provider evidence for review/MarkReady is missing. No cutover, restart, push or store migration. Edited (non-seeded) legacy graphs stay refused by convert (017/018).


## Delivery Notes

Size estimate: M. Planning only; status stays pending.

Review events and publication intents share the existing account/PR/artifact-bound native GitHub effect contract; keep their distinct action types, receipts and approvals. Do not combine their authority.

Authoritative existing seams: `src-tauri/src/workflows/run/native_github.rs`, `src-tauri/src/workflows/native_catalog.rs`. The operation/consumer audit is `docs/backend/workflow-template-migration.md`; the actual catalog capture and evidence qualifications are `src-tauri/src/workflows/fixtures/neutral-native-catalog.json` and its sibling README. Inspect and reuse these; fixture presence is not live-service success.

### 2026-11-13T08:05:00.000Z - Landed on main as 12ff4a8456. Every acceptance criterion was already checked on the story branch; no test was rerun for this closure.
