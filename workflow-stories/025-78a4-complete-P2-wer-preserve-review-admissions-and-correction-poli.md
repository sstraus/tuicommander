---
id: 025-78a4
title: "WER: Preserve review admissions and correction policy composition"
status: complete
priority: P2
type: feature
created: "2026-11-11T16:47:31.216Z"
updated: "2026-11-12T00:57:23.000Z"
dependencies: ["016-358f", "020-6471", "021-6e1b"]
depends_on: ["workflow-stories/016-358f-complete-P2-wer-deliver-inspectable-delivery-review-and-bounde.md", "workflow-stories/020-6471-complete-P2-wer-preserve-scoped-agent-roles-and-source-bound-r.md", "workflow-stories/021-6e1b-complete-P2-wer-expose-owned-story-proposals-and-idempotent-cr.md"]
plan: plans/workflow-engine-rework.md
plan_step: Step 5
risk: high
started_at: "2026-11-11T22:17:08.348Z"
---

# WER: Preserve review admissions and correction policy composition

## Problem Statement

Old consumers: story_delivery judge/repair and resolve_plan judge/replan, plus edited EvidencePolicy and follow-up proposal settings. native.correction.assess and correction_region already provide real source-bound assessment and bounded correction, but they do not translate the source role reports, distinct maxReviews, maxRepairs, proposal caps or finding dispositions. Compose the missing policy with those services rather than rewriting the correction kernel.

Judge/review admissions, finding disposition and correction/proposal accounting are grouped because they govern one retained assessment iteration. 016 keeps the loop/assessment owner; 020/021 supply role and proposal receipts.

## Acceptance Criteria

- [x] Before implementation, record the observable consumer effect, authoritative native state, external assumptions, affected consumers and owned paths in this worklog; audit existing implementation and compatible evidence first.
- [x] Compose the real selected body, native.correction.assess and correction_region with source role reports and finding disposition. Capture immutable criteria, exact candidate/head/tree/base and all host-configured checks; require the existing independent review/current-evidence floors.
- [x] Preserve separate review admissions, rejected reviews, corrections and check attempts. maxRepairs counts corrections after the initial attempt; neutral body limit is N+1. maxReviews independently caps every admitted reviewer, including replacement agents, without resetting on retry/restart/new artifact.
- [x] Reuse the recorded 017 zero/two repairs, story three repeats, replan eight repeats, review-limited repair and shared-run counting evidence where compatible. Add only uncovered composition cases; keep run-wide bounds separate from region bounds and technical retry/CI refresh separate from semantic correction spending. _(verified: 017 counting evidence rb 20261112T000503Z-79648 nextest 17/17 at 017 HEAD 7d1159cc40 plus author edits later committed in e82ffb339d (hashes match); compatible with this tree: the counting path (correction_driver, correction_fixture, correction_region, neutral_execution, evidence, graph) is hash-identical at integration HEAD, and the 025/026 edits in correction_native*/correction_generation are additive or native-assess only; uncovered composition cases are the 025 tests native_review_policy_tests, native_role_budget_tests, correction_policy_tests, GREEN in rb 20261112T005016Z-94151)_
- [x] Compose 021 per-review proposal receipts with explicit disposition of immediate correction, separate work and accepted risk. No proposal creation, check result, stale review/CI generation or model verdict may manufacture independent approval. Emit explicit operator/exhausted outcomes for 026 to handle.
- [x] Targeted cases catch maxReviews silently becoming maxRepairs, an extra attempt at exhaustion, duplicate CI/check spending, a replacement reviewer resetting budget, stale artifact evidence completing the body and discarded findings reopening an approved join.
- [x] Expose versioned catalog inputs/outputs, named outcomes and exact handler/descriptor/prompt/subflow pins through existing neutral admission and the shared Designer/IPC/HTTP/MCP contracts; no legacy scheduler wrapper, role-specific kernel branch or caller-supplied authority.
- [x] Reuse the existing effect ledger for ownership, exact approvals, cancellation and original-effect reconciliation. A lost reply or uncertain effect never authorizes a fresh write; draft edits cannot alter an active run.
- [x] Prove named failure modes with real native services in throwaway local repositories and focused tests of changed paths/consumers. Record tree, command, task id, tool versions or unmeasured versions and result; reuse compatible passing evidence. No per-story full suite, clippy, mutation run or automatic extra critic.
- [x] Planning leaves this story pending and does not authorize implementation, cutover, restart, push, live user-data/store migration or live writes outside the already approved private sandbox repository. Any sandbox effect must keep its existing exact action/target approval and cleanup; missing live evidence remains an explicit gap.

## Work Log


### 2026-11-11T22:17:08.495Z - REALITY contract beforecode: effect=selected correction body cannot enter an independent reviewer after its immutable maxReviews admission budget; retries, replacement sessions and new artifacts cannot reset it, while correction/check/proposal counts remain separately inspectable. state=RunStore owned effects and native review/check observations, CorrectionRegion N+1 bound, immutable EvidencePolicy,021 proposal receipts and020 source-role reports when supplied. assumptions=review transport returns native observed identity; existing protocol evidence is not paid-provider proof. radius=new native bounded review/policy composition modules, additive catalog/dispatch, existing native review and correction assessment, shared authoring/API docs and focused tests. Reuse existing017/016 correction counting evidence; do not retest unchanged loop boundaries. No new scheduler or fabricated passing role report. First independent slice installs bounded reviewer admission through existing native review service; full completion requires020 report seam, requested fromparent.


### 2026-11-11T22:59:06.931Z - Composition implementation and focused cases prepared. Bounded native reviewers and policy-bound020 reviewer/validator roles share durable run/scope admissions; two distinct executions spend two admissions. Selected body inspection rejects uncounted reviewers, wrong role-report producer/policy and output bypass; native assessment resolves full retained reports and021 proposal effect receipts while preserving actual independent review/configured check floors. Extra RED/Clippy/native-target proof remains explicit operator work, never model authority. One targeted rb wave launched; source frozen1350 Rust files, no green/completion claimed yet. 026 audit remains read-only until this result.


### 2026-11-11T23:02:02.409Z - Targeted wave never started: exit69 SSH rb100.121.52.117:22 operation timed out; runner explicitly reports box unreachable/deployment failed and no local fallback. No compilation or tests executed, no task receipt to count as evidence. Parent notified34158e95-e212-4dee-821c-7319d2aca3cc. Static follow-up corrected new composer test ObjectSchema JSON from {} to required fields:{}; original frozen hashes remain historical unexecuted inputs. Await actual remote availability before targeted execution;025 remains incomplete.


### 2026-11-11T23:23:02.121Z - rb20261111T231009Z-77036 compiled actual production/fixture consumers,12/14passed. New composer synthetic role schema omitted config.role; new native composition reused command id review-ready from initialrole preparation and idempotent replay returned old activation. Both are test setup errors; fixed only those2testfiles and narrow2case retry active.1350executed Rust hashes match frozen input; current diff exactly those2testfiles, verified partial-verified.json, clean/nonstale collected/stopped/released. Reuse12 unchanged greens incl shared native/role admission, native current review/checks,020role receipts,006AC3 and exact model request.


### 2026-11-11T23:31:02.676Z - Narrow retry20261111T232152Z-4964 composerPASS, nativecomposition reaches assessment and exposes real missing check accounting (expected1 actual0). Actual native assessment checks used verify_observed directly and were absent from neutral-verification effect counting. Added per-original-assessment/check durable admission+receipt journal: cached receipt prevents refresh/reopen duplicate process; missing reply remains uncertain, never fresh execution. Assertion retained and enhanced for cached receipt/reopen. New targeted validation required for changed production behavior. Prior retry reports stale because worklog changed while running; do not claim nonstale, all tracked source/story/doc writes now frozen during job.


### 2026-11-11T23:34:02.820Z - Check-journal wave20261111T233158Z-7793 failedcompile E0277: rusqlite0.40 does not implement FromSql for u64. Count query now reads SQLite i64 then checked-converts u64; no behavior/assertion change. No tests ran;1351frozen/executed/currenthashes and clean/nonstale collected/stopped/released verified beforeedit. Retry onlysameaffected consumers required after type fix.


### 2026-11-11T23:39:38.786Z - Current affectedconsumer wave20261111T233405Z-28955 GREEN6/6,1351 hashes match executed/frozen/current, nonstale clean collected/stopped/released exit0; source tree7d37340da00789c971118d9508ddedfd3fb339502908e2e5826687fedcfffdc5. Final inspection found a directassessment bypass of composer-only rolepolicy enforcement: native report resolver proved owner/source but did not require same retained reviewer admission policy. Added runtime producer-policy equality+original admission ordinal validation and actual retained-role cross-budget rejection assertion. One new direct-admission guard case requires only correction_policy_tests rerun; no other production changes.


### 2026-11-11T23:44:41.399Z - RESULT 025 policy implementation: GREEN current role-policy guard rb20261111T233941Z-77974 1/1, source tree e9fc6f1c43c94145de9a292954fe1ad0f7bb82633d09ae87f5f742b20e06ae45;1351 frozen/executed/current Rust hashes match, nonstale clean, exit0 artifacts collected/stopped/released. Reuse5 unchanged affectedconsumer greens from rb20261111T233405Z-28955 (6/6 including prior version of currentcase), and unchanged native review/020role/exact-model/AC3 greens from rb20261111T231009Z-77036. Composer case passed rb20261111T232152Z-4964: that receipt is stale due worklog-only update; its executed test/production bytes match current composer files, so do not label its metadata nonstale. No broad rerun. Rust1.99.1 from compilerlog; metadata target/toolchain unknown, Nextest version unmeasured. Existing two intake dead_code warnings and fixture-build unused_imports expectation are not new passing authority.
Contract: immutable same-run/scope maxReviews independently gates native reviews and policy-bound020 reviewer/validator roles, before paid provider entry; each actual execution spends one. maxRepairs remains ordinary pinned CorrectionRegion N+1; initial body not a repair. Replacement/new artifact/SQLite reopen cannot reset review admissions. Actual check attempts have original assessment/check admission+receipt journal; cached refresh/reopen does not run twice, missing reply remains uncertain. Native exact committed independent review and all captured check definitions still supply final floor. Full retained role receipt must match original same-source/current-generation/evidence/candidate/requirements, same admission policy and actual ordinal. Findings FIX/ACCEPT are exercised through actual native service pipeline; STORY requires stopped021 CreateReceipt with original source payload/effect/review/cap/finding links, checked directly in correction_policy.rs, and cannot grant approval. Duplicate seen source is discarded before findings reclassification. Exhaustion resolves only original stopped native review/role marker and returns absent repeat/operator, never success. Extra RED/strict-Clippy/native-target model metadata cannot authenticate those proof owners; explicit operator fallback remains and migration is not claimed.
API: additive version1 native.review.bounded/native.correction.policy.assess and optional policy-bound agent.role config, shared correction_policy_region action resolves exact same-project selected publication/transitive pins, rejects uncounted native/role reviewers, wrong policy/report producer and output bypass, then lowers through unchanged kernel. No new scheduler, caller authority, external routes or transport divergence. NativeHost model invoke/reconcile moved mechanically into child module to keep files bounded; existing script branch and006/020 consumers green.
Compatible016 evidence reused by exact matching current correction.rs/correction_assessment.rs/correction_region.rs/correction_region_tests.rs hashes against rb20261111T150718Z-83289 and finalgenerationtask20261111T152038Z-77791;016 worklog records zero/two bounds, same-scope count, all dispositions, duplicate source and stale approval tests. No017 result exists: criterion4 waits on017 evidence per coordinator c09e34f6-b332-4020-aefc-f620db5b4ebd. Keep025 in_progress and criterion4 unchecked; coordinator checks it after017. Other criteria are implemented/verified as qualified above. No paid live provider, desktop/UI launch, restart, push, migration, cutover or live-store actions. Rust needs authorized manual rebuild/restart to load; tracker updated. Catalog fixture capture remains integrator-owned.


### 2026-11-12T05:22:58.519Z - Boss Q8=A (2026-11-12, via or-3 31a3c33e): EvidencePolicy caps maxReviews at 32 (was 16). A policy correction region (correction_policy_region::review_budget) refuses maxReviews < 2 * (maxRepairs + 1), because each round admits a reviewer role and a native bounded review. The plan workflow keeps 8 rounds (maxRepairs 8, maxReviews 18), and the 017 converter no longer refuses resolve_plan on budget. RED: rb 20261112T051027Z-2601 (cap 16, per-round rule off): the new policy_region_budget_needs_two_admissions_per_round_up_to_32 and delivery_templates_refuse_with_named_blockers_and_without_writes FAIL. GREEN: rb 20261112T051550Z-44145, 36/36 (region, migration, convert, correction policy, review policy, role budget, migration count, evidence, wf11).


## Delivery Notes

Size estimate: L. Planning only; status stays pending.

Judge/review admissions, finding disposition and correction/proposal accounting are grouped because they govern one retained assessment iteration. 016 keeps the loop/assessment owner; 020/021 supply role and proposal receipts.

Authoritative existing seams: `src-tauri/src/workflows/correction_region.rs`, `src-tauri/src/workflows/correction_assessment.rs`, `src-tauri/src/workflows/run/migration_count_tests.rs`. The operation/consumer audit is `docs/backend/workflow-template-migration.md`; the actual catalog capture and evidence qualifications are `src-tauri/src/workflows/fixtures/neutral-native-catalog.json` and its sibling README. Inspect and reuse these; fixture presence is not live-service success.

### 2026-11-12T00:57:23.000Z - Coordinator closure (integration record reconciliation, or-3 1787b94b): AC4 checked from 017 counting evidence rb 20261112T000503Z-79648 (17/17), compatible by unchanged counting-path hashes; all criteria checked. Documented limits in this record stay limits, not passes. No new test run.
