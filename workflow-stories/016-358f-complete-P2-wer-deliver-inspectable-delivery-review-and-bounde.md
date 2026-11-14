---
id: 016-358f
title: "WER: Deliver inspectable delivery, review and bounded correction metanodes"
status: complete
priority: P2
type: feature
created: "2026-11-10T08:29:18.831Z"
updated: "2026-11-12T00:57:23.000Z"
dependencies: ["006-6885", "008-22c4", "010-978f", "014-5d73", "015-608c"]
plan: plans/workflow-engine-rework.md
plan_step: Step 5
depends_on: ["workflow-stories/006-6885-complete-P2-wer-execute-mediated-model-calls-and-scoped-develo.md", "workflow-stories/008-22c4-complete-P2-wer-expose-native-repository-and-development-opera.md", "workflow-stories/010-978f-complete-P2-wer-admit-selectable-github-issue-and-pr-workflows.md", "workflow-stories/014-5d73-complete-P2-wer-author-catalog-activities-mappings-and-metanod.md", "workflow-stories/015-608c-complete-P2-wer-expose-the-same-workflow-capabilities-through-.md"]
---

# WER: Deliver inspectable delivery, review and bounded correction metanodes

## Problem Statement

Workflow engine rework 016-358f. Scope: deliver specialized composite metanodes, including the complete latest requirement above, as inspectable version-pinned typed subflows rather than hidden scripts.

Delivery prerequisites: 006-6885, 008-22c4, 010-978f, 014-5d73, 015-608c. These native prerequisites also appear in the linked plan and dependency metadata. All work remains pending.

Contract-first feature: record consumer effect, authoritative state, external assumptions and affected consumers before implementation. Reuse existing services/evidence where compatible; no mandatory duplicate full suites or extra critic rounds. Planning creates pending work only and does not authorize implementation or live effects.

## Acceptance Criteria

- [x] Independent review plus configured deterministic checks returns semantic outcomes for an exact artifact; Reviewer/Critic labels confer no authority.
- [x] A configurable correction region repeats on failing CI or relevant comment dispositions (`correct`, `ignore-with-reason`, `needs-decision`).
- [x] Expose criteria, selected region, required inputs/outputs, entry/exit and internal graph in Designer and MCP. _(verified: MCP half by 016 Phase B worklog (CorrectionRegionView via NeutralAction::CorrectionRegion); Designer half by 014 record at dfed33d9c0 — CorrectionRegionEditor renders criteria, selected activity, max corrections and generated contracts, opens the selected child via GetPublication; NeutralDesigner.native.test.tsx GREEN and screenshots docs/design/wer-014/designer-correction-region.png, designer-correction-child.png)_
- [x] Limit means correction traversals after the initial attempt: 0 permits none and 2 permits at most two.
- [x] Persist count/scope identity across commits, replacement workers and restart; duplicate events and recovery re-dispatch do not double-spend or replenish it.
- [x] Exhaustion routes to blocked/operator, never success.
- [x] Repeat only the selected region, not issue/workspace/PR creation outside it.
- [x] Relevant findings retain reason/source; ambiguity takes operator branch.
- [x] Stale CI/review cannot release a newer artifact's join/publication.
- [x] Technical retries remain separate and reconcile uncertain effects.
- [x] Focused scenarios catch off-by-one exhaustion, reset-on-new-agent, duplicate CI spending and retained old approval; contract-first features must not be mislabeled reproduced RED regressions.

## Work Log


### Phase A contract — 2026-11-11

Consumer effect: an ordinary pinned neutral subflow repeats only its selected body, after an initial attempt, up to the configured correction limit. Exhaustion and ambiguous evidence return blocked/operator; adapter uncertainty does not complete a traversal. Findings retain source and reason.
Authoritative state: RuntimeOwner/RunStore own the loop invocation, committed iteration admission, attempt fences, effect identities and replay. The business limit lowers with checked N+1 arithmetic. Worker and artifact identity never key or replenish this budget. Typed carried assessment state invalidates review/check/approval evidence on artifact change.
External assumptions: no provider readiness or live receipts are claimed. Phase B must bind native review/check receipts and GitHub observations to the typed assessment seam, then expose the same pinned publication through Designer/MCP. Only backend-bound evidence can authorize external operations. Tests use internal kernel activities, not fabricated provider fixtures.
Named bugs: zero/two off-by-one; replacement/restart resets budget; duplicate evaluation spends twice; old artifact approval survives; setup repeats outside the selected body; ambiguity drops source/reason; uncertain effects buy corrections.
Validation: one filtered rb nextest run covering correction modules and affected neutral loop/subflow/effect consumers. This is a contract-first feature, not a reproduced RED regression. No critic or broad suite in this assignment.


Phase B seams: `correction::CorrectionReceipt` receives 008 exact-change review/configured-check verdicts; `CorrectionObservation` and `CorrectionState::advance_artifact` receive 010 current artifact/CI/comment generations; `correction_region::CorrectionRegion::definition` and its shared input/state schemas supply 014/015 publication inspection/authoring. Phase A neither installs provider adapters nor grants authority to these data values. Criteria 1, 3 and 9 remain pending their real consumers and integrated evidence.
Assessment cases protect distinct contracts absent from generic scope tests: source-level delivery deduplication and stale approval invalidation; all three finding dispositions retaining rationale; technical uncertainty returning only the original reconciliation identity. Owner scenarios execute the generated publication through nested subflow return and durable RunStore replay, asserting body/setup executions and final routing rather than descriptor flags. Existing neutral effect-ledger tests cover lost writes and exact approval ownership; reuse them rather than create a second effect mechanism.


### Phase A verification — GREEN

- Base: `ab728193a14d5907f0bf33eb2abe9a2c585dd420`; tested staged tree: `dc0414a1eab4093b201cea903dd22c99df6274c8`.
- Command: `PATH=/opt/homebrew/bin:$PATH TMPDIR=$HOME/Gits/.tmp/tuic-tests/ build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(correction_) | test(loop_owner_tests_current_carried_snapshot_and_rejects_previous_iteration) | test(lost_write_response_survives_restart_and_reconciles_without_second_write) | test(exact_approval_revision_has_one_committed_winner_and_never_expires_to_yes)'`, launched through `tuic-bg.sh`.
- rb task `20261111T130654Z-23800`; nextest run `bc131f0b-95d7-402d-950c-9a772b237924`: **10 executed, 10 passed**, 4.901s execution after 4m10s compilation. rb toolchain versions unmeasured. Remote Linux evidence only; no native Mac binary or desktop check claimed.
- Evidence: `~/Gits/.tmp/workflow-rework/016/targeted-rb.log` and `~/Gits/.tmp/rb/20261111T130654Z-23800/`. Thirteen existing warnings name unchanged trigger/host/runtime integration surfaces; no warning names the new correction modules.
- AC2/4/6/7: `correction_zero_and_two_are_after_initial_and_repeat_only_selected_region` runs the real published nested graph: limits zero/two allow one/three total bodies, exhaustion returns operator, a passing second body succeeds, and three parent setup activities each execute once.
- AC5/11: `correction_admission_survives_head_worker_and_reopen_without_double_spend` reopens RunStore after committed iteration-two admission before dispatch; duplicate command replay leaves the current snapshot unchanged, previous fences are rejected, new artifact state retains consumed source identity, and the original budget still exhausts. `correction_assessment_deduplicates_sources_and_invalidates_old_approval` additionally protects source-revision deduplication and generation invalidation. These are internal feature scenarios, not native provider readiness or reproduced RED regressions.
- AC8: `correction_findings_keep_source_reason_and_ambiguity_routes_operator` covers all dispositions and missing reason/evidence; `correction_unknown_and_technical_failure_never_release_success` exercises the generated graph's operator and technical exits.
- AC10: `correction_unknown_effect_requires_original_reconciliation` yields no body completion; unchanged real-file lost-response and exact-effect-approval consumers passed in this same run. No retry scheduler or new effect authority was introduced.
- Formatting/instruction checks: instruction-link validation passed (two pairs); changed-file rustfmt checks passed. After the run, only module declaration ordering was formatted and this evidence/criteria text was added; production function bodies and test file hashes stayed unchanged.
- Phase A is complete; full story remains pending Phase B (AC1/3/9) and actual native/provider plus Designer/MCP integration. No live external writes, restart, push, merge, full suite, mutation run or critic round occurred.

### Phase B contract — 2026-11-11

Consumer effect: native review/check results for the exact current change drive correction or operator routing; missing configured checks and old-head reviews never count as passed. The existing neutral action returns the generated correction graph and its selected pin, criteria, entries/exits and limit for MCP; Designer rendering remains 014's gap.
Authoritative state: native stopped effect receipts and NativeContext's captured check-name registry, plus 010's retained trigger intent/active resource generation and the neutral owner's loop admission. No graph-supplied caller or receipt body is authority. Freshness must be checked again at committed assessment consumption.
External assumptions: no live provider/model output is fabricated. Existing real Git/native service tests can substitute only the external model answer boundary. Missing 008 service interfaces are requested from COORDINATOR2, never replaced with a success stub. Phase A evidence remains compatible where source/test hashes are unchanged.
Base merged once: e6bd48f664, merge a1981aeaeb. Conflicts combined both workflow documentation additions and retained additive correction modules; a mistakenly dropped github_trigger declaration was restored during validation (see below).

Phase B implementation notes: `CorrectionRegion.criteria` is pinned as a constant body input and exposed in `neutral_api::model::CorrectionRegionView {region,definition}` through `NeutralAction::CorrectionRegion`; no transport is added. Exact 014 consumer: render region.criteria, region.selected, region.max_corrections and definition.inputs/outputs/control/loops/subflows; resolve the selected child with GetPublication and inspect retained loop iteration minus one for consumed count. Designer remains an explicit AC3 gap.
`run::correction_native::assess` consumes 008 observed_review of a committed base/head/tree, executes every captured configured check with verify_observed, and refuses criteria digest mismatches. It never creates a provider pass. The standalone 008 receipt commit 34ed538fa9 was cherry-picked as de1ee91561; no uncommitted files were copied. Native host/dispatch changes are additive and coordinator-authorized (mail f22d288b).
`run::correction_generation` reads 010's retained trigger intent/resource state; pinned PR head/base mismatch refuses. The shared neutral_command_at consumption hook runs inside its existing immediate event transaction, so a new pending generation refuses completion before cancellation flush. Late effect stop/receipt evidence settles ownership but cannot advance the old graph.
Named phase B cases: stale completion in admission/flush window; stale stop leaking resource ownership; retained review after candidate replacement; check omitted/unexecuted counted as pass; failed checks losing verdict; a caller-completed fake review standing in for actual service; wire shape hiding criteria/pin/limit/operator exhaustion.

### Phase B verification — GREEN, Designer gap retained

AC1 and AC9 are verified; the MCP half of AC3 is verified. AC3 remains unchecked because Designer rendering belongs to 014. Exact consumer DTO: `neutral_api::model::CorrectionRegionView {region, definition}`. Render `region.criteria`, `region.selected`, `region.max_corrections`, `definition.inputs`, `outputs`, `control`, `loops` and `subflows`; resolve the selected child with `NeutralAction::GetPublication`, and display consumed corrections from retained scope iteration minus one. Do not infer a provider pass from DTO data.

Base merge: a1981aeaeb merges e6bd48f664 into phase A. A module declaration was lost in my merge resolution, not in the integration branch: restored `pub(crate) mod github_trigger;` in workflows/mod.rs. Compared all declarations in workflows/mod.rs and workflows/run/mod.rs against e6bd48f664; none else dropped. Initial rb task 20261111T150344Z-60916 failed compilation due to that missing declaration; no tests executed.

First executed targeted wave: staged tree b72da3bb479c2267af4ee9dd0cd2ed1f0d94d924, rb task 20261111T150718Z-83289, Nextest 0e8bd126-b946-4f56-9420-5bcb270b4709. Build 4m36s; 19 tests executed, 18 passed, one failed. The new stale-effect test caught my post-decision mutation breaking deterministic replay. Fixed by recording ordinary Cancel first, then retrying the original stopped observation; exact command/decision replay remains unchanged. No special replay bypass was added.

Final targeted rerun: staged source tree 1f30dc24c85795e6a9e85166687c4bf097dc9262, rb task 20261111T152038Z-77791, Nextest 8a12e778-3d4b-4407-bc6a-8c7d41d7c61d: **3/3 passed**, 0.374s after 2m07s build. Command:
`PATH="/opt/homebrew/bin:$PATH" TMPDIR="$HOME/Gits/.tmp/tuic-tests/" /Users/stefano.straus/Gits/personal/orchestrator/tools/build/build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(correction_generation_tests) or test(latest_pr_generation_waits_for_original_effect_certainty_and_stop)'`

Reused the other passing test-file hashes from the first executed wave: only correction_generation.rs, correction_generation_tests.rs and neutral_execution.rs changed afterward. Reran the changed generation test file plus the affected 010 cancellation consumer. All 28 changed Rust source/test files match final rb frozen SHA256s. Final source-tree SHA256 091c5d653b1398651b23d0559daca7a9af7ed8105cae76cd600810022d975b54. Final receipt: command_exit0, artifacts collected, stopped/released, box_dirty=false, stale=false. rb toolchain and target versions **unmeasured**. Two existing dead-code warnings in GitLab/trigger intake remain. No full suite, clippy, mutation or critic round was run.

Initial executed filter: `test(correction_) or test(native_committed_review_observation_retains_verdict_and_exact_artifact) or test(native_empty_review_is_uncertain_without_reviewer_session) or test(native_nonzero_check_retains_candidate_receipt) or test(native_review_receipt_covers_only_passing_exact_diff) or test(native_pins_and_named_verification_use_current_git_artifact) or test(latest_pr_generation_waits_for_original_effect_certainty_and_stop)`. Logs: ~/Gits/.tmp/workflow-rework/016/phase-b-targeted-rb-retry.log and phase-b-generation-rb.log. Complementary receipt-only evidence from 008: task20261111T150423Z-66128, 48/48 targeted passed (mail0332c266); no redundant rerun after this evidence arrived.

Additive shared edits: workflows/mod.rs (merge declaration repair); run/mod.rs (new modules); neutral_execution.rs (generation consumption guard); native_host.rs (descriptor install, publication and registry hooks); native_dispatch.rs (typed assessment validation/dispatch); neutral_api.rs and neutral_api/model.rs (existing action/DTO). No forbidden native_operations/native_catalog/agent/model or plugin edits by 016. Dependency-owned receipt files entered solely through committed cherry-pick. Docs, manual restart tracker, rustfmt, instruction-link check and diff check updated/passed. No live/paid ACP, external writes, desktop instance, app restart, push, merge into integration/main or landing. This is a contract-first feature, not a reproduced pre-fix regression.

### 2026-11-12T00:57:23.000Z - Coordinator closure (integration record reconciliation, or-3 1787b94b): AC3 checked from 014 Designer evidence dfed33d9c0 and the 016 MCP evidence; all criteria checked. Documented limits in this record stay limits, not passes. No new test run.
