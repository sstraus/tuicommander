---
id: 023-026f
title: "WER: Bind issue triage effects and source-linked story handoff"
status: complete
priority: P2
type: feature
created: "2026-11-11T16:47:30.749Z"
updated: "2026-11-13T08:05:00.000Z"
dependencies: ["006-6885", "010-978f", "020-6471", "022-597e"]
depends_on: ["workflow-stories/006-6885-complete-P2-wer-execute-mediated-model-calls-and-scoped-develo.md", "workflow-stories/010-978f-complete-P2-wer-admit-selectable-github-issue-and-pr-workflows.md", "workflow-stories/020-6471-complete-P2-wer-preserve-scoped-agent-roles-and-source-bound-r.md", "workflow-stories/022-597e-complete-P2-wer-dispatch-pinned-story-subflows-with-owned-chil.md"]
plan: plans/workflow-engine-rework.md
plan_step: Step 5
risk: high
started_at: "2026-11-11T17:03:14.777Z"
completed_at: "2026-11-13T08:05:00.000Z"
---

# WER: Bind issue triage effects and source-linked story handoff

## Problem Statement

Old consumer: issue_intake triage, GitHubIssue rejection/question/evidence/status comment and accepted/rejected label effects, then IssueHandoff to a pinned story. 010 source admission and provider services exist, but none of these installed catalog operations preserves the original issue/generation and child responsibility. Deliver the missing operations for one retained issue intake, not another poller.

Triage, issue effects and handoff are grouped because they share one admitted issue generation and disposition lifecycle. They reuse 020 scoped role translation and 022 child dispatch; no duplicate story launcher or GitHub poller.

## Acceptance Criteria

- [x] Before implementation, record the observable consumer effect, authoritative native state, external assumptions, affected consumers and owned paths in this worklog; audit existing implementation and compatible evidence first.
- [x] Reuse 010 event normalization, selectors, dedup/admission and retained generation snapshots. Produce typed accepted/rejected/needs-input triage bound to the same issue/account/repository/generation, configured scoped profile and evidence; an old generation cannot authorize current effects. _(verified at 9c53c63e4a, rb 20261112T010855Z-67868: native_issue_source.rs binds the issue to the retained 010 trigger admission and github_trigger repository normalization; native_issue_source_refuses_foreign_stale_and_edited_requirements refuses foreign/stale/edited sources; native_issue_triage_retains_all_dispositions_and_refuses_foreign_report covers accepted/rejected/needs-input and refuses another generation's report; native_issue_production_owner_triages_through_mediated_profile_and_refuses_child proves the installed production owner runs on the configured pinned profile and refuses a different one. Model process is a fixture; live model execution unproved.)_
- [x] Expose exact host-owned issue comment/label effects through existing account-aware GitHub services. Preserve every legacy IssueEffect meaning and configured label/body source; missing account, stale subject, unsupported effect or lost response returns a typed refusal/uncertainty rather than success.
- [x] Accepted handoff composes 022 pinned child dispatch with the original issue identity, triage evidence and retained responsibility. Reject/needs-input never dispatch; duplicate intake or handoff delivery cannot start another story. Child status comments refer to that same retained child.
- [x] Targeted cases catch old-generation triage releasing effects, accepting a label as authority, comment/label retry duplication, dropping issue identity at handoff, handoff on a rejected issue and duplicate story creation/dispatch.
- [x] Expose versioned catalog inputs/outputs, named outcomes and exact handler/descriptor/prompt/subflow pins through existing neutral admission and the shared Designer/IPC/HTTP/MCP contracts; no legacy scheduler wrapper, role-specific kernel branch or caller-supplied authority.
- [x] Reuse the existing effect ledger for ownership, exact approvals, cancellation and original-effect reconciliation. A lost reply or uncertain effect never authorizes a fresh write; draft edits cannot alter an active run.
- [x] Prove named failure modes with real native services in throwaway local repositories and focused tests of changed paths/consumers. Record tree, command, task id, tool versions or unmeasured versions and result; reuse compatible passing evidence. No per-story full suite, clippy, mutation run or automatic extra critic.
- [x] Planning leaves this story pending and does not authorize implementation, cutover, restart, push, live user-data/store migration or live writes outside the already approved private sandbox repository. Any sandbox effect must keep its existing exact action/target approval and cleanup; missing live evidence remains an explicit gap.
- [x] REALITY: effect=a rejected/needs-input triage or a pre-existing accepted label never prepares a plan/story, issue writes are exact approved source-bound effects, status comments name the retained child; state=RunStore effect ledger, retained 010 trigger admission, StoryStore plan/proposal identity; assumptions=model and provider replies controlled from the recorded REST fixture github-issue-rest.json, live provider only via the 2026-11-11 sandbox run; radius=native_issue_* operations and the issue_delivery graph composition, test-only change in this round
- [x] GREEN: rb 20261113T073954Z-67775 nextest c65fb87f, staged tree a9b13014759e, cargo nextest run --lib -E "test(native_issue_) | test(native_story_create_reservations_pin_cap_and_consume_unknown_results) | test(native_catalog_wire_matches_shared_designer_fixture)": 14 passed, 0 failed
- [x] TEST-CHANGE: src-tauri/src/workflows/run/native_issue_graph_tests.rs::native_issue_graph_joins_exact_preparation_created_story_and_returned_pinned_child — old expectation wrong because it accepted any status comment containing "is Failed"; AC4 requires the comment to name the same retained child, so it now expects "child <observe execution> is Failed"
## Work Log


### 2026-11-11T17:03:14.531Z - REALITY: effect=source-bound accepted/rejected/needs-input verdict, exact approved issue writes and pinned neutral handoff; state=010 retained trigger intent linked to current run/current source generation, account-aware live issue requirements, scoped report evidence, neutral effect ledger; assumptions=provider replies can be lost and remain uncertain, model outputs are data, existing recorded GitHub REST fixture proves schema only; radius=installed native catalog and host, issue intake, scoped role and child-dispatch consumers. Owned paths: new native_issue_*.rs; additive native_catalog registration, native_dispatch match arms, native_host admission/preparation arms and run/mod declarations, domain docs/manual tracker. No legacy scheduler, poller, generic HTTP workaround or live cutover. One rb targeted wave after edits; no fixture regeneration.
Authoring cases: owning source/effect boundary tests catch borrowed receipt/changed source generation/live edit, unapproved write, replacement label, lost-write success and replay. Existing native GitHub tests target PR artifacts and do not exercise source admissions. Use production Git checkout, source/trigger/effect SQLite and exact approval; substitute only external PrApi, use recorded REST input. No new test-only production export.


### 2026-11-11T17:05:28.412Z - Implemented independent source/effect slice: issue.read derives source from retained trigger request; issue.effect supports exact approved literal comment/additive-label requests with account, source generation and live requirements fences, current Push authority, cross-workspace issue resource and uncertain write receipts. Full six legacy effect semantic bindings,020 scoped triage evidence and022 actual handoff composition remain pending concrete dependency APIs requested from owner. Sandbox gh API read succeeded; no GH_TOKEN or GITHUB_TOKEN process variable is present, no token store read or external write. First targeted rb wave catches source borrowing/staleness/live-edit publication and unapproved/duplicate lost writes; existing native authority implementation is unchanged.


### 2026-11-11T17:16:22.096Z - First rb attempt 20261111T170532Z-14628 exit75: tree changed during sync, remote command never started; no compilation/tests ran. Cause: I added a new port file before remote sync froze its source. Changes now complete before retry. Added native.issue.triage/handoff via minimal backend-only IssueServices per coordinator direction, passing NativeBinding for captured caller inheritance, scoped source/profile report checks, accepted-only retained closure handoff and original-child observe/cancel. Semantic variants cover all six legacy IssueEffect values using settled source-bound triage/child evidence. Missing real020/022 port installation remains explicit integration gap, not a completion claim. Added external-owner port tests for dispositions, foreign report/live edits, rejected handoff and original-child identity/cancel/no redispatch; real model/child execution is not claimed. Added ignored live native sandbox comment/label acceptance; coordinator supplies command-only token from orchestrator .env. No fixture regeneration.


### 2026-11-11T17:26:43.730Z - Targeted rb 20261111T172014Z-68857 / nextest 628f2c05-9d9c-481d-999f-5cb2569ce068 compiled staged tree68b815a202d53277d9baf686b4deadb2664b6711: four passed; original-child identity case failed because matching source/publication metadata alone accepted a replacement child id. Added comparison against retained effect.handle; rerun only failed child case. rb rustc1.99.0 cargo1.99.0 per runner toolchain (exact versions to record from runner metadata if available). No real020/022 adapter claim.


### 2026-11-11T17:26:53.113Z - Correction to preceding entry: rb cargo/rustc versions were not measured; remove the inferred1.99.0 claim. Native mbx doctor reports cargo1.99.1/rustc1.99.1, which is separate from rb evidence.


### 2026-11-11T17:30:59.830Z - GREEN: rb original-child recovery case passed after handle fence; task20261111T172655Z-30741, nextest610d843c-b86a-4209-aefe-eb13b7f22476, staged tree76008bc20fc7c0242acd9afe2b7ebb5afb56f4f8. Command build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E test(native_issue_child_observe_and_cancel_keep_original_identity_without_dispatch). Earlier four passing cases reused unchanged test/source hashes; only native_host/issue.rs child observation changed. rb tool versions unmeasured. Warning: post-result box GC failed rc255; command exit0 and reservation released. Native sandbox build still running; no live success claimed yet.


### 2026-11-11T17:32:19.826Z - Native sandbox run0e49270d-2a3e-4b89-aef6-99885467267f failed in test setup: second publication normalized unchanged issue requirements and incorrectly required a newly emitted source event. First real comment had succeeded. Fixture issue9 closed and unique label removed by finally cleanup; leftover own comment then deleted via exact unique marker. Fixed test to reuse retained original event and always clean comments after failure. Production unchanged; rerun only ignored native sandbox case.


### 2026-11-11T21:28:56.122Z - Native retry eaeca670-3c77-44de-b2c7-dcdb309c52a8 reached real comment and label on issue10 but failed on GitHub request and cleanup GET request. Independent gh GET observed both unique own effects. Deleted comment6802887851, closed issue10, deleted own unique label. Fixed cleanup to retain returned comment id and attempt all cleanup operations before asserting failures, eliminating unnecessary cleanup GET. Production/domain sources unchanged. Network reads recovered through gh; rerunning only live case with revised cleanup.


### 2026-11-11T21:29:26.725Z - Correction: manual issue10 cleanup commands encountered network-down errors; preceding entry overstates closure/label deletion. Comment DELETE returned no error, but issue closure and label deletion are unverified. Fixture10 and label wer023-002771ef-ef13-4954-ab20-3d18fb1e0e55 remain a cleanup obligation until independently confirmed. Live retry already launched; no passing live evidence claimed.


### 2026-11-11T21:30:17.404Z - Further exact cleanup: PATCH issue10 returned state closed; DELETE unique label returned success. Final verification GET failed network-down. Peer020 supplied draft role/source report and same-parent-frame cohort dispatch contracts; current IssueServices remains uninstalled and requires adaptation to those owner/frame identities, not a separate launcher. Story remains in_progress.


### 2026-11-11T21:33:18.026Z - GREEN live: native macOS ignored sandbox case passed, nextest492c413c-7c4d-4e30-bd2e-262db5fb5785, committed treeed0971ab71bf1c6dfe0e30cd13d53c0e56cca77d (a9062ea8e). Command bash ~/Gits/.tmp/workflow-rework/023-issue/live-native.sh -> mbx build-slot.sh scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib --run-ignored ignored-only -E test(native_issue_live_sandbox_comment_and_label_have_provider_receipts). Real account-aware RepoApi + exact neutral approval published comment and additive label on only sstraus/tuic-e2e-sandbox, read them back, removed comment/label and verified fixture issue closure. Native cargo/rustc1.99.1; nextest version separately recorded. Domain test hashes unchanged except already-green child identity fix; live test setup/cleanup changed only. fmt-changed main...HEAD passed. Remaining acceptance: actual020 report consumption and022 same-parent child-frame adapter; no production IssueServices install. Criterion1 checked, story remains in_progress; no restart/push/landing.


### 2026-11-13T07:39:14.022Z - Finish contract (wer-023, main d3a98e4562+dea3bff745): audit only, H2 stands (handoff = graph composition native.issue.prepare -> native.story.create -> story.cohort.created -> pinned host.story.subflow child -> native.issue.graph.receipt, 2962-554e). Consumer effect: a rejected/needs-input triage or an accepted label on the issue never prepares a plan/story; status comments name the retained child execution. State: RunStore effect ledger + StoryStore. Assumptions: model/provider replies controlled from the recorded REST fixture; no live provider. Gaps found by audit: composed-path refusal for non-accepted triage is untested (only the refusing legacy handoff_owned is), and the status comment test does not assert the child identity. Owned paths: native_issue_prepare_tests.rs, native_issue_graph_tests.rs. No production change, no cutover/push/restart.


### 2026-11-13T07:44:56.394Z - GREEN rb 20261113T073954Z-67775, nextest c65fb87f-2230-4559-94c5-a598b7842d81, staged tree a9b13014759e (base dea3bff745 + the two test edits): build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib --no-fail-fast -E "test(native_issue_) | test(native_story_create_reservations_pin_cap_and_consume_unknown_results) | test(native_catalog_wire_matches_shared_designer_fixture)" -> 14 passed, 0 failed, 3.788s, RB_BOX_DIRTY=no, peak RSS 16108236 KiB. rb rustc/nextest versions unmeasured (local mbx rustc/cargo 1.99.1). New native_issue_prepare_refuses_non_accepted_triage_despite_accepted_label passed on first run: production already refused (native_issue_prepare.rs:38), so it is a guard, not a RED; no production change.


### 2026-11-13T07:44:56.602Z - Criterion audit at dea3bff745. AC3: native_issue_effect.rs publish() resolves RepoApi (account-aware, github.rs:563-593 errors on NeedsAccount/NeedsBind -> NativeFailure::Refused via native_git.rs:14) and refuses account/repository mismatch; native_issue_policy.rs resolve() maps all six legacy IssueEffect variants (definition.rs:22) from retained triage/policy/child evidence and refuses a disposition/policy mismatch; tests: native_issue_source_refuses_foreign_stale_and_edited_requirements (stale subject), native_issue_triage_retains_all_dispositions_and_refuses_foreign_report (unsupported effect), native_issue_effect_requires_approval_adds_label_and_preserves_lost_write (lost reply -> Uncertain, no repeat); real account path proven by live sandbox nextest 492c413c (2026-11-11). Missing-account refusal is code-inspection evidence only (no unit test: RepoApi::resolve reads the account registry and token store). AC4: under H2 the accepted handoff is the composed graph (2962-554e): native_issue_graph_joins_exact_preparation_created_story_and_returned_pinned_child (now also asserts the status comment names the retained child execution), composed_issue_execution_completed_child_reaches_ready_without_status (rb 20261112T214201Z-90082, reused); reject/needs-input: new prepare guard test + triage test; duplicate: prepare reuses the owned plan, story.create reservations test, 010 trigger dedup. AC5: all six failure modes map to the 14 passed tests above. AC6: native_catalog.rs registers the seven native.issue.* descriptors (versioned request/receipt, named outcomes), native_catalog_wire_matches_shared_designer_fixture passed; Designer fixture byte-identical to the backend capture (cmp). AC7: effect ledger approval/uncertainty/no-repeat tests above, child observe/cancel identity test, publication disposition tests; draft edits vs active run: neutral_start_replays_retained_closure_and_normalized_values_after_owner_restart (generic, reused). AC8: this run; no full suite, clippy, mutation. AC9: no cutover, restart, push or live store migration done.


## Delivery Notes

Size estimate: L. Planning only; status stays pending.

Triage, issue effects and handoff are grouped because they share one admitted issue generation and disposition lifecycle. They reuse 020 scoped role translation and 022 child dispatch; no duplicate story launcher or GitHub poller.

Authoritative existing seams: `src-tauri/src/workflows/trigger.rs`, `src-tauri/src/workflows/run/neutral_github_intake_tests.rs`. The operation/consumer audit is `docs/backend/workflow-template-migration.md`; the actual catalog capture and evidence qualifications are `src-tauri/src/workflows/fixtures/neutral-native-catalog.json` and its sibling README. Inspect and reuse these; fixture presence is not live-service success.

### 2026-11-12T01:14:35.000Z - Integration round 9 (merge 9c53c63e4a of feat/wer-issue-services 7859780edb, MediatedIssueServices T1+H2): AC2 checked with the cited rb 20261112T010855Z-67868 evidence (69/69, includes the 6 native_issue_ cases). AC4 stays open: under H2 the production owner refuses handoff/observe/cancel with the native.story.create + host.story.subflow blocker, so no composed 022 child dispatch is proven. AC3, AC5-AC9 unchanged by this owner; no new evidence. Live triage on the sandbox stays a to-test item.

### 2026-11-13T08:05:00.000Z - Landed on main as f7b0829be8. Every acceptance criterion was already checked on the story branch; no test was rerun for this closure.
