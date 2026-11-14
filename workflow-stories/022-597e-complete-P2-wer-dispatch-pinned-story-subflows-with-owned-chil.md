---
id: 022-597e
title: "WER: Dispatch pinned story subflows with owned child receipts"
status: complete
priority: P2
type: feature
created: "2026-11-11T16:47:30.471Z"
updated: "2026-11-13T00:51:09.000Z"
dependencies: ["004-2809", "005-f907", "008-22c4", "015-608c"]
depends_on: ["workflow-stories/004-2809-complete-P2-wer-persist-and-execute-neutral-runs-with-one-sche.md", "workflow-stories/005-f907-complete-P2-wer-own-durable-asynchronous-activities-effects-an.md", "workflow-stories/008-22c4-complete-P2-wer-expose-native-repository-and-development-opera.md", "workflow-stories/015-608c-complete-P2-wer-expose-the-same-workflow-capabilities-through-.md"]
plan: plans/workflow-engine-rework.md
plan_step: Step 5
risk: high
started_at: "2026-11-11T21:29:31.757Z"
completed_at: "2026-11-13T00:51:09.000Z"
---

# WER: Dispatch pinned story subflows with owned child receipts

## Problem Statement

Old consumer: resolve_plan StoryDispatch; issue handoff also needs this child-execution primitive. Existing agent/session spawn starts a terminal but does not provide an exact neutral child-publication closure, inherited context, cancellation, completion join or retained evidence. Expose one bounded typed native child-dispatch contract through the existing neutral run owner, not a terminal or a hidden legacy story scheduler.

The operation accepts already-existing story IDs, so it can be implemented in parallel with 021. Creation is a separate effect; 017 composes their receipts.

## Acceptance Criteria

- [x] Before implementation, record the observable consumer effect, authoritative native state, external assumptions, affected consumers and owned paths in this worklog; audit existing implementation and compatible evidence first.
- [x] Dispatch the exact same-project published story/subflow revision and full transitive closure for explicit existing story identities. Snapshot the selected cohort, source/plan identity and typed inputs; return stable parent-effect/child-run correlation with completed, blocked and uncertain outcomes. _(verified: main d3a98e4562: native_cohort_dispatch_joins_real_ordered_story_observations and native_cohort_refuses_stale_foreign_or_unobserved_child_receipts (rb 20261111T220014Z-8989); native_dynamic_cohort_admits_only_stories_this_run_created_at_current_revision (rb 20261112T064307Z-95913). observe returns completed or blocked; an unproved child stays an Uncertain effect in the ledger (neutral_runtime.rs EffectOutcome::Uncertain))_
- [x] Reuse neutral run/subflow admission, current owner/effect ledger and non-amplifying context derivation. A child may receive only the current parent ceiling and declared subject references; creating another scheduler or treating arbitrary PTY exit as story completion is forbidden.
- [x] Repeat delivery or crash recovery reconciles the original child. A changed child draft/latest publication cannot substitute for the pinned version. Cancellation retains workspace/resource ownership until the owned child and effects are stopped or explicitly reconciled. _(verified: main d3a98e4562: subflow_frames_replay_same_owner_and_readmit_typed_child_arguments, child_revision_and_full_content_remain_pinned_across_reopen and cancelled_pending_effect_keeps_claim_until_proven_stop_without_reopening_graph (rb 20261111T215027Z-1380 aggregate filter))_
- [x] Preserve ordered/sequential-batch cohorts and their integration barrier as data and joins over owned children; concurrency=1 alone cannot claim combined-artifact critic or FIX re-entry parity. Missing batch obligations must remain individually blocked. _(verified: main d3a98e4562: native.story.batch.admit/land/pin/verdict (native_batch_* tests, rb 20261112T020708Z-17933 and 20261112T173304Z-50657) are composed with host.story.subflow children, the combined-artifact reviewer and FIX re-entry in composed_plan_execution_delivers_and_reenters_its_fix_owner (rb 20261112T215442Z-95613, 2962-554e). native.story.cohort.observe alone still reports the batch obligations blocked)_
- [x] Targeted cases catch duplicate child starts, latest-revision substitution, cross-project scope amplification, stale child releasing a join, partial cohort reported complete and cancellation releasing ownership early. _(verified: main d3a98e4562: duplicate start = native_batch_barrier_admits_in_order_one_slot_and_refuses_duplicate_starts; latest-revision substitution = child_revision_and_full_content_remain_pinned_across_reopen; cross-project = native_cohort_refuses_stale_foreign_or_unobserved_child_receipts and native_dynamic_cohort_admits_only_stories_this_run_created_at_current_revision; stale join = native_batch_verdict_routes_fix_reentry_and_never_completes_a_partial_cohort and native_batch_evidence_requires_owned_settled_push_and_exact_reviewer_report; partial cohort = the verdict test; early release = cancelled_pending_effect_keeps_claim_until_proven_stop_without_reopening_graph)_
- [x] Expose versioned catalog inputs/outputs, named outcomes and exact handler/descriptor/prompt/subflow pins through existing neutral admission and the shared Designer/IPC/HTTP/MCP contracts; no legacy scheduler wrapper, role-specific kernel branch or caller-supplied authority.
- [x] Reuse the existing effect ledger for ownership, exact approvals, cancellation and original-effect reconciliation. A lost reply or uncertain effect never authorizes a fresh write; draft edits cannot alter an active run.
- [x] Prove named failure modes with real native services in throwaway local repositories and focused tests of changed paths/consumers. Record tree, command, task id, tool versions or unmeasured versions and result; reuse compatible passing evidence. No per-story full suite, clippy, mutation run or automatic extra critic.
- [x] Planning leaves this story pending and does not authorize implementation, cutover, restart, push, live user-data/store migration or live writes outside the already approved private sandbox repository. Any sandbox effect must keep its existing exact action/target approval and cleanup; missing live evidence remains an explicit gap. _(verified: no cutover, restart, push or live store migration was done for this story; live evidence stays an explicit gap)_

## Work Log


### 2026-11-11T21:29:32.355Z - Contract before implementation (transferred from deleted duplicate020-2544): effect=explicit existing native story cohort enters exact pinned same-project subflow and returns observed completed/blocked receipt; state=selected native StoryBackend, immutable publication/transitive closure, inherited NativeContext and parent run/effect frames; assumptions=no terminal exit treated as completion and no combined-artifact batch attestation from concurrency; consumers=plan dispatch and issue handoff; owned paths=native_child/native_cohort, additive native registry, reusable operation validation and shared neutral action. Cancellation/recovery reuse existing frame/effect ownership. One targeted rb wave; no fixture regeneration.


### 2026-11-11T21:45:22.734Z - Implementation uses exact published subflow frames and selected native story backend; ordered observations expose completed/blocked selection and individually blocked combined-artifact, independent-batch-review and FIX-reentry obligations. No second scheduler. Integration base577b3172fc merged before targeted validation.


### 2026-11-11T22:05:51.978Z - Pinned cohort operation implemented with native backend observation and inherited parent frame ownership. rb20261111T220014Z-8989:2/2 cohort tests pass at tree29641f13b0e74945bc32ba4c73fb23e590655fdd; compatible27 green results from task20261111T215027Z-1380 cover reused owner/revocation/cancellation/replay paths. Initial failures were test setup compilation and stale generic condition; fixed and rerun only invalidated cases. Evidence/commands/versions:docs/backend/workflow-agent-child-evidence.md. Combined-artifact/batch critic/FIX obligations individually blocked; issue provider composition and template cutover remain separate. No broad gate or live restart.


### 2026-11-12T06:47:02.833Z - Decision (a), coordinator or-3 e5ceefa8 (2026-11-12, reversible, recorded for Boss): a dynamic cohort. save_child_dispatch without cohort registers a host.story.subflow with no pinned snapshot. native_cohort::admit then admits the run-time cohort input only when every member has a settled native.story.create receipt in the bound run (NativeContext carries the bound run, set only by the native host). The plan/source/project and the exact current revision are checked by Cohort::current; the child publication pin stays exact. RED rb 20261112T063452Z-38559 (foreign story admitted with the check stubbed). GREEN rb 20261112T064307Z-95913, 67/67. Removed the stale pending planning copy of this record, which the 017 merge reintroduced (d538ad14ed).


### 2026-11-12T07:08:22.818Z - Decisions 1(i) and 2(i), coordinator or-3 56010a5c (2026-11-12, reversible, recorded for Boss).
1(i): native.story.batch.land also accepts a settled native.commit.create receipt of this run, made after the member's admission (known landings now include commits). The pin still proves it is in the clean HEAD; pushes keep their approvals. A commit of another run is refused as foreign.
2(i): native.story.transition asks no question only when this run holds a settled correction.policy.assess for exactly that story (context origin) with an explicit repeat:false (the region success route) on the committed artifact now at the clean HEAD. It is re-proved at execution (authorize_covered); any other transition still asks. Note: CorrectionState.approval is never set in production, so the verdict is repeat:false.
RED: rb 20261112T065511Z-86121, both tests FAIL with the rules stubbed (commit landing refused; approved verdict not covering).
GREEN: rb 20261112T070015Z-26692, 47/48. The 1 failure was my first coverage rule, which keyed on state.approval; I fixed it to repeat:false and reran: rb 20261112T070359Z-57404, 9/9 (correction policy + native story).


### 2026-11-12T07:25:46.497Z - Decisions A(a) and B(a), coordinator or-3 8dd437f1 (2026-11-12, reversible, recorded for Boss).
A(a): StartManual asks no question only for a story this run created (a settled story.create receipt) that native.story.batch.admit admitted to this run's slot. CheckCriterion, SubmitReview and Approve stay covered only after the passing verdict.
B(a): native.story.cohort.created {plan, review} returns this run's created stories under the review, in creation order (catalog 47 -> 48).
Note: a passing verdict means an explicit repeat:false from correction.policy.assess, because CorrectionState.approval is never set in production.
RED: rb 20261112T071902Z-71155, native_created_cohort_and_start_coverage_stay_scoped_to_this_run FAILS with the creation check stubbed (an admitted story of another run is covered). The cohort half (another run's story never returned) holds by construction: created_stories reads one run's snapshot, so a stub could not make it fail without rewriting the reader.
GREEN + capture: rb 20261112T072219Z-90439, 18/18.


## Delivery Notes

Size estimate: L. Planning only; status stays pending.

The operation accepts already-existing story IDs, so it can be implemented in parallel with 021. Creation is a separate effect; 017 composes their receipts.

Authoritative existing seams: `src-tauri/src/workflows/neutral_publication.rs`, `src-tauri/src/workflows/run/neutral_runtime.rs`, `src-tauri/src/mcp_http/agent_routes.rs`. The operation/consumer audit is `docs/backend/workflow-template-migration.md`; the actual catalog capture and evidence qualifications are `src-tauri/src/workflows/fixtures/neutral-native-catalog.json` and its sibling README. Inspect and reuse these; fixture presence is not live-service success.

### 2026-11-12T02:10:00.000Z - AC5 contract before code (or-3 96c765e7, branch feat/wer-022-batch-barrier from 21543180a8)
Audit: the legacy sequential batch (run/batch/mod.rs, review.rs) admits one story at a time; a started member must integrate before the next starts. It pins the integrated, unbatched members at the clean trunk artifact (every member merge is an ancestor). It takes one critic verdict per pinned generation over the exact (commit, tree) digest; a FIX names exactly one member of that batch and reopens it as a fresh child generation. Plan completion needs every member pinned, no unapplied FIX and the latest batch approved. The 017 checkout (wer-018b, including uncommitted edits) only maps run.sequential_batch to delivery.sequentialBatch; it builds none of this. native_cohort observe keeps reporting the three obligations as blocked.
Effect: four native operations keep the batch as data in one retained ledger per (run, plan). The graph composes them as joins around the owned host.story.subflow children:
- native.story.batch.admit {cohort, story} -> ok. This is the barrier. The cohort (plan/source/order) is retained on the first admission and must stay identical. It admits only the first cohort member that has no admission, or the armed FIX owner. It refuses while another admitted member has not landed (one slot), and it refuses a second start of the same story generation.
- native.story.batch.land {plan, story, push} -> ok. Credits the slot holder's integration to a settled, completed native.publication.push effect of this same run. The push must not have been known when the member was admitted, and it must not be credited to another member. A stale or earlier-generation push cannot release the join.
- native.story.batch.pin {plan} -> ok. Requires a free slot and no armed FIX. Pins exactly the landed, unbatched members at the clean workspace (commit, tree). Every landing commit must be an ancestor of the pinned commit. Generations are consecutive.
- native.story.batch.verdict {plan, generation, report, source} -> completed | continue | fix. Takes the single verdict of the latest pin from a source-bound agent.role report (read_report) of this run. The role is reviewer or validator, and its source commit/tree is exactly the pinned artifact. The report body {decision: approve|fix, story?} is typed. A fix must name one member of that batch; it arms that story's next generation for re-admission. An approval gives completed only when every cohort story is in an approved batch; otherwise it gives continue. A partial cohort is never completed.
State: RunStore SQLite tables, written in one immediate transaction per effect. Each effect's receipt is retained by effect digest, so a repeat delivery returns the same receipt and records nothing new. Reconcile returns the retained receipt; without one, it refuses (no fresh write after a lost reply).
Assumptions: landing means a native push in this run (no legacy integration saga). The workspace checkout is the canonical artifact, as in legacy clean_artifact. The critic is a model role; its verdict authority is the typed report plus an exact artifact match, not a label.
Consumers: 017 delivery.sequentialBatch conversion; Designer/MCP through the shared catalog (fixture regenerates 41 -> 45).
Owned paths: run/native_batch.rs (ledger), run/native_batch_evidence.rs (push and critic evidence), native_catalog.rs, native_dispatch.rs, native_host/runtime.rs reconcile, tests, catalog fixtures and READMEs, docs/backend/workflows.md.
Not in scope: AC2/AC4/AC6 source composition, and the legacy independent proof cohort. Without these operations, a graph that dispatches children directly still gets no barrier attestation; native_cohort observe keeps saying not_attested.
RED/GREEN plan (targeted rb only):
- native_batch_barrier_admits_in_order_one_slot_and_refuses_duplicate_starts. Catches: a second member starting before the first lands; an out-of-order start; a duplicate child start; a replayed effect recording twice.
- native_batch_pin_holds_exact_landed_members_at_a_clean_containing_artifact. Catches: a pin while a slot is held; a member left out or added; a landing not in the pinned commit; a dirty artifact.
- native_batch_verdict_routes_fix_reentry_and_never_completes_a_partial_cohort. Catches: a second verdict; a FIX naming a non-member; a stale push releasing the re-entry join; a partial cohort reported complete; a later approval hiding an unapplied FIX.
- native_batch_evidence_requires_owned_settled_push_and_exact_reviewer_report. Catches: a foreign-run or unapproved push; a non-reviewer role; a report over another artifact.

### 2026-11-12T02:20:00.000Z - AC5 implementation (feat/wer-022-batch-barrier)
- wer-018b confirmed no overlap: 017 story_delivery is not converted, so no consumer uses these operations yet.
- RED: rb 20261112T020221Z-79672, test(native_batch_tests::), 0/4. The ledger write, landing and critic evidence were stubbed to refuse, so each test failed on its first assertion. Before that, one compile-only run (rb 20261112T015923Z-58939) failed on private test access; the test module moved under runtime.rs, like native_agent_tests.
- GREEN: rb 20261112T020708Z-17933, the same filter plus the catalog fixture test with TUIC_CAPTURE_NATIVE_CATALOG=1, 5/5. The catalog went 41 -> 45: native/story.batch.{admit,land,pin,verdict}@1 added, the other 41 unchanged and in order. SHA256 9fa5203ac0f82ed370ef309e8810046638e2c07b07359dfe1613566628bd030a in both fixture copies.
- The evidence test uses a real approved native push into a local bare remote, and real reviewer and implementer agent.role reports through the ACP protocol fixture (the model process is a fixture). The ledger tests use the real RunStore SQLite and a real Git checkout.
- AC5 stays unchecked for coordinator acceptance. The operations and their rules are proven at the module boundary. No test yet composes them in one graph with host.story.subflow children; a to-test item records the live check. Toolchain: unmeasured.

### 2026-11-13T00:51:09.000Z - Closed after the landing of 2962-554e on main d3a98e4562. The remaining gap (batch operations not composed in one graph with host.story.subflow children) is closed by the 2962-554e composed plan execution. Evidence was checked against main by code and test-name inspection; no test was rerun.
