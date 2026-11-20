# Neutral workflow engine: execution model, use cases and test matrix (L0)

Status: study, 2026-11-15, branch `docs/wf-test-study`, base `f3ea8f88d8`.
Method: engine source read; test coverage taken from test **names** listed by
script (350 tests in `src-tauri/src/workflows`, `mcp_http`, `telegram`,
`stories`). A cell names a test only when its name states the behavior. Bodies
were not re-read for every test. A cell marked `GAP` means "no test with such a
name found"; confirm before writing a new test.

Paths are relative to `src-tauri/src/workflows/` unless noted. `run/X` is
`workflows/run/X`.

## 0. Findings that change the plan

1. **The converted-template tests named in story 018 and 2962 no longer exist.**
   `composed_plan_execution_*`, `composed_issue_execution_*` and
   `migration/convert_tests.rs` were deleted by `499b60fc76`
   (legacy engine deletion, #3034-365e). Their evidence (rb runs) stays in the
   story worklogs, but the code cannot be rerun. Acceptance C is now covered
   only by `run/plan_delivery_tests.rs` (4 tests) and
   `run/pr_review_workflow_tests.rs` (5 tests). `resolve_plan` and
   `issue_delivery` conversion has no test at all.
2. **Only 2 built-in templates ship**: `templates/github-issue-triage.json`
   and `templates/github-pr-review.json` (`neutral_api/templates.rs:18`). The
   delivery workflow is built in Rust (`run/plan_delivery.rs`), not a JSON
   template.
3. **No black-box test drives a whole run through HTTP or MCP.** HTTP route
   `/workflows/neutral/action` (`mcp_http/mod.rs:1116`) has one test
   (`mcp_http/workflow_neutral_routes.rs:54`, replay authority). MCP has
   `mcp_transport_neutral_tests.rs` (2 tests: drafts, schema) and one
   input-validation test (`mcp_transport_ancillary_tests.rs:2528`).
4. Trigger sources in the engine: `Github`, `Gitlab`, `Scheduled`
   (`trigger.rs:11`). **There is no Jira, Telegram or HTTP trigger source.**
   Jira, Telegram and HTTP are action connectors and wait callbacks only.
5. Live proof exists for GitHub only (`run/parity_github_live.rs`,
   `run/neutral_issue_template_live.rs`, `run/native_issue_live_tests.rs`,
   `pr_review_workflow_tests.rs:548`; all `#[ignore]`/opt-in).

## 1. Execution model as implemented

### 1.1 One ledger, replayed state

A run is an append-only event log in SQLite
(`neutral_workflow_runs`, `neutral_workflow_events`; schema `run/neutral.rs:15`,
store `run/store.rs:15`). **State is never stored. It is computed by replay.**

- Event 1 is `Started` with the retained run (`run/neutral.rs:190-255`).
- Every later event is `Execution { command, decision }`
  (`run/neutral_execution.rs:1054`). `replay_observed`
  (`neutral_execution.rs:1145-1208`) re-runs `decide(command)` on each stored
  command and refuses the history if the result differs from the stored
  decision, if the sequence skips, or if the SHA-256 differs. A corrupt run
  fails alone: startup lists ids only (`neutral_execution.rs:1075`).
- One decision = one `IMMEDIATE` SQLite transaction
  (`neutral_command_at`, `neutral_execution.rs:998-1069`): replay, check
  idempotency, check `expected_sequence`, decide, apply, insert event, commit.
  **This commit is the only durable point.**

Idempotency: a repeated `command_id` with the same hash returns the receipt
projection as of the original sequence (`:1015-1021`). Same id with different
hash is an error. A stale `expected_sequence` returns
`sequence changed; retry` (`:1022`). Callers loop on that text
(`neutral_runtime.rs:127-134`, `:151-164`).

### 1.2 Identity: activation, attempt, generation

`NeutralAttemptFence { run, execution, activation, attempt, scope }`
(`neutral_execution.rs:24-30`).

| Field | Meaning | Where it changes |
|---|---|---|
| `execution` | Frame id: `"root"` or a subflow frame | `enter_subflow` `:478` |
| `activation` | Count of control-node entries in the run | `Route` apply `:829-850` |
| `attempt` | Constant 1 for plain activities; one slot per scope item | `:418`, `neutral_scopes.rs` |
| `scope.identity` | `CollectionIdentity{run, scope, generation}`; generation = activation | `neutral_scopes.rs:109-120` |
| `scope.item` | Item or branch key inside the scope | slots map |

`pending_attempt` (`:618-652`) is the single fence check. It rejects a result
whose scope identity differs from the active scope, whose slot owner differs,
whose fence differs from the prepared attempt, or whose result already exists
("result is immutable"). A late result from a previous activation, iteration or
generation fails here.

`max_activations` counts every `Route` (`:807-818`). `max_concurrency` and
`max_duration_ms` are run limits (`:407`, `:283-288`).

### 1.3 Control nodes and routing

`ControlNode` is `Start | End | Condition | Wait | Activity | Collection | Fork |
Loop` (`neutral_control.rs`). `decide_inner_mode` (`:255-427`):

- `Condition` yields `true|false|unknown` with data provenance
  (`:373-391`). **Unknown is a separate outcome, never false.**
- `Wait { duration_ms }` first decision `ArmWait{due_ms}` stored in
  `wait_due_ms`, later `Route "next"` when `at_ms >= due` (`:392-398`). The
  actor sleeps until `min(wait_due, deadline)` (`neutral_runtime.rs:518-528`).
  After restart `wait_due_ms` is rebuilt by replay, so the wait is not re-armed.
- `Activity` prepares inputs from typed bindings (`prepare_activity`
  `:698-770`). An `Unknown` source fails the activity outside a scope and
  defers inside a fork (`:721-733`).
- `End` resolves typed workflow outputs and finishes (`:361-372`). In a subflow
  it returns to the parent frame through `Complete` (`:939-958`).

### 1.4 Effects: prepare, approve, dispatch, observe

Types: `neutral_effects.rs:15-75`. Commands `Prepare`, `Question`, `Answer`,
`Dispatch`, `Observe`, `Cancel`, `BindModel`, `ResumeModel` (`:145-187`).

Order inside the actor (`dispatch_neutral`, `neutral_runtime.rs:196-329`):

1. **Admission** on every dispatch, with current host context and grants
   (`admitted` `:75`). A refusal with no effect yet fails the activity
   (`:223-239`). A refusal with an existing approved effect only skips it, so
   old approval never works as authority (`:215-221`).
2. Subflow targets take `EnterSubflow`, not an adapter call (`:240-253`).
3. `host.prepare()` then `NeutralEffect::prepare` computes the `EffectKey`
   (fence + digest). `Prepare` is committed (**durable point DP-prepare**).
   Re-preparation after restart must produce the same key or the effect is
   skipped (`:277-281`).
4. If preparation needs approval, the actor returns. Approval is a `Question`
   with `revision` and `deadline_ms`.
5. `Answer{revision, approved}` (`neutral_effects.rs:588-657`) is accepted only
   while the attempt is pending, the run and question deadlines are not past,
   the revision matches, no answer exists and the effect is not dispatched.
   A "no" settles the activity as failed (`:640-651`). Expiry never becomes yes
   (`:424-446`: `Refuse` at dispatch).
6. `Dispatch{key}` is committed **before** the adapter is called
   (`neutral_runtime.rs:296-312`; admission rules `neutral_effects.rs:417-458`):
   run running, not past deadline, not already dispatched, approved, capacity,
   no resource conflict (`:787-801`). **DP-dispatch.** The adapter call
   (`host.invoke_effect`) runs only after this commit, and the call id is held in
   memory (`neutral_effect_calls`).
7. `observe_neutral_future` (`:331-376`) commits `Observe`. A returned error or
   a refused observation becomes `Uncertain` and keeps the claim (`:349-370`).
   **DP-observe.**
8. `Observe{Completed, stopped:true}` creates the activity result in the same
   transaction (`neutral_effects.rs:539-543`, `callback_decision`). A completed
   result with `stopped:false` does not release resources (`:478-535`,
   `project_resources` `:781`).

Immutability rules (`neutral_effects.rs:459-535`): a completed result cannot
change, a `stopped` flag cannot go back to false, uncertainty cannot replace a
completed result, and a pending handle is set once. An early callback wins over
the pending acknowledgement (`early_callback_wins_pending_ack...`).

Uncertainty: `Uncertain` is a state of the **effect**, not of the graph. The
graph stays on the activity. Reconciliation is an explicit call
(`host.reconcile`), never a retry of the write
(`recover_neutral_effects`, `neutral_runtime.rs:379-461`).

### 1.5 Waits, questions, timers

- Operator or model question = `Question` on an effect (above). Telegram wait
  callbacks (`w:` callback data) are filtered in `telegram/workflow.rs:49-100`
  (private chat, allowlist, sender = chat, message id) before they reach
  `Answer`. Retained receipts resolve the wait after restart
  (`retained_telegram_receipt_resolves_exact_wait_after_restart_once`).
- `Wait` node timer: section 1.3.
- Run deadline: `Step` after the deadline yields `Expire` (`:283-287`); any
  other command fails with "deadline elapsed".
- Branch-local wait: a question holds only its slot. The scheduler skips a
  slot with an unapproved effect and tries siblings (`neutral_runtime.rs:211-214`).

### 1.6 Fork, join, ForEach (collection), loop, subflow

All scopes share the owner's transaction and activation budget
(`neutral_scopes.rs:1`). `ScopeDecision` is `Open | Skip | Settle | Iterate`
(`:60-78`).

- **Collection (ForEach)**: expansion computed once at `Open`; one slot per
  item, ordered key (`:109-160`). Aggregate settles in item order
  (`settle_scope` `:318`). Capacity: `scope_capacity` (`:286`) else limit.
- **Fork**: named branches; a branch input may be `Unknown` while a producer
  branch is current, so the branch waits (`prepare_activity` `:721-731`).
  Failed arrival is retained (`fork_owner_waits_for_current_producers...`).
- **Loop**: iteration number and carried values; a result from the previous
  iteration is refused (`loop_owner_tests_current_carried_snapshot...`).
- **Subflow**: `EnterSubflow` pushes a child frame with the **retained pinned
  child publication** (`:456-505`). `Finish` or failure pops it and applies a
  `Complete` to the parent fence. Effects are global across frames
  (`std::mem::take(&mut self.effects)` `:437`, `:946`).

### 1.7 Pins and drafts

`RetainedClosure::resolve` (`run/neutral.rs:82`) copies root and child
publications, descriptors and handler references into the run at start.
A draft edit or republish changes no running run. A retry with the same
`request_id` returns the original run without calling the catalog again
(`neutral.rs:204-208`).

### 1.8 Triggers

Backend normalizers submit `TriggerEvent` (`trigger.rs:17-45`); the model is
not deserializable from a client. Admission (`run/neutral_triggers.rs:13-80`)
runs under `neutral_trigger_lock`:

1. flush pending intents (a crash between run creation and receipt link retries
   the exact persisted request, `:117-168`);
2. for each enabled binding that matches: skip a GitHub binding with no
   baseline (`:34-41`), skip self-generated events (`trigger_suppressed`), build
   the pinned request, **reserve an intent** (unique on
   project/binding/resource/generation/replay_id, `neutral_trigger_store.rs:15-21`);
3. per resource only one active run; a newer generation sets
   `pending_receipt`. On flush with `supersede`, the active run gets
   `Cancel`, and the new run starts only after every dispatched effect of the
   old run is settled (`:130-160`).

Sources: GitHub (webhook payload and polling `github_poll.rs`), GitLab
(`gitlab/`), Scheduled (`scheduled_trigger.rs`, reuses the automation tick).
Replay needs `ReplayAuthority` made by an authenticated owner (`trigger.rs:50`).

### 1.9 Cancellation

`Cancel` is accepted while running; it sets `Cancelled` (`:280-282`, `:967`).
It does **not** release effects. The actor then, per dispatched unsettled effect,
commits `EffectCommand::Cancel` and calls `host.cancel`
(`neutral_runtime.rs:419-444`). Resources stay claimed until `stopped:true`
evidence (`expired_actor_requests_cancel_but_does_not_release...`). Retained
claims survive actor exit; a late callback can settle them but never reopens
the graph (`:481-486`). Correction generations can cancel a run and re-issue
the command (`neutral_execution.rs:1028-1039`).

### 1.10 Restart reconciliation

`WorkflowRuntime::spawn` (`runtime.rs:100-175`): acquire the owner lock (standby
until free), list all run ids, install the host, wake one actor per id. Each
actor:

1. replays and drives `Step` until no change (`drive_neutral_turn` `:145`);
2. if the run has no live caller binding, **does nothing** until the same start
   request rebinds it (`neutral_run_unbound`, `:171`, actor `:473-481`);
3. `recover_neutral_effects`: for each dispatched unsettled effect, one
   `reconcile` per actor lifetime (`seen`); a model parent waits until its
   tool effects settle; an ended run gets `Cancel`;
4. otherwise `dispatch_neutral`.

Rule that the tests rely on: **a restart never calls the adapter for a
dispatched effect again**. It only reconciles or waits.

### 1.11 Durable points (fault-injection targets)

| DP | Committed event | Crash before = | Crash after = |
|---|---|---|---|
| DP1 | `Started` (`neutral.rs:240`) | no run | run exists, actor idle until woken |
| DP2 | `Route`/`Prepare`/`ArmWait` (Step) | step repeats | state replays |
| DP3 | `Effect::Prepare` | prepare repeats, same key needed | effect waits for answer or dispatch |
| DP4 | `Question` / `Answer` | wait unanswered | answer retained |
| DP5 | `Effect::Dispatch` | adapter not called | **adapter may or may not have been called** → reconcile only |
| DP6 | `Observe` (pending / completed / uncertain) | result lost, effect `dispatched` | result retained |
| DP7 | `Complete` (activity result) | effect settled, graph not advanced | next `Step` routes |
| DP8 | `Finish` / `Cancel` / `Expire` / `Limit` | run still running | terminal |
| DP9 | trigger intent reserved | event unseen (source retries) | intent flushed on next admit |
| DP10 | `trigger_started` link | duplicate start attempt, exact request replayed | run linked |
| DP11 | subflow `EnterSubflow` / return | child not entered | child frame replayed |
| DP12 | scope `Open` / slot result / `Settle` | scope reopens | slots replay in order |

## 2. Use-case catalog

Legend for the status column: **W** = expressible today as a workflow;
**P** = partly (named gap); **M** = needs a missing capability.

### 2.1 Acceptance A-E (story 018-bb68)

| ID | Use case | Status | Evidence |
|---|---|---|---|
| A | Scheduled text → model → condition → notification or approval, no Git | W | `parity_schedule_tests.rs:15` |
| B | Script transform → native operation → pinned subflow output; extra script descriptor renders | W | `parity_tests.rs:12`, `neutral_script/tests.rs:134` |
| C | Delivery, review, correction as configuration | P | `plan_delivery_tests.rs` (4), `pr_review_workflow_tests.rs`; conversion tests deleted (section 0) |
| D | Paginated HTTP → bounded per-item subflow → ordered aggregate → condition | W | `parity_tests.rs:89` |
| E | GitHub issue/PR event; Jira, GitLab, Telegram slices | P | GitHub live; Jira/GitLab/Telegram fixtures only |

### 2.2 Templates and built-in workflows

| ID | Workflow | Source | Status | Tests |
|---|---|---|---|---|
| T1 | GitHub issue triage | `templates/github-issue-triage.json` | W | `neutral_issue_template_tests.rs:35,107`, `neutral_issue_template_live.rs:113`, `NeutralDesigner.authoring.test.tsx:152` |
| T2 | GitHub PR review | `templates/github-pr-review.json` | W | `pr_review_workflow_tests.rs:409-481`, live `:548` |
| T3 | Plan and story delivery (implement, verify, review, one commit; max 3 attempts) | `run/plan_delivery.rs:23-35` | W | `plan_delivery_tests.rs:152,185,210,270` |
| T4 | `resolve_plan` conversion | deleted with legacy | **no test** | none |
| T5 | `issue_delivery` conversion | deleted with legacy | **no test** | none |
| T6 | Operator wait / automatic close | `native.operator.*` | W | `native_operator_tests.rs` (4+1) |
| T7 | Issue graph: prepare, effect, publication, child | `native.issue.*` | W | `native_issue_*_tests.rs` |

### 2.3 Connectors

| Connector | Read | Write | Trigger | Wait callback | Live proof |
|---|---|---|---|---|---|
| GitHub | snapshot, issue read | review, comment, label, push | webhook + poll | none | yes |
| GitLab | issue, MR, diff, pipeline | review note | `TriggerSource::Gitlab` | none | no |
| Jira | lookup, search, transitions | create, update, comment, transition | none | none | no |
| Telegram | update | send, reply | none | callback `w:` | no |
| HTTP | paginated GET | POST/PUT with approval | none | async write ack | no |

Tests per connector: GitHub `neutral_github_*`, `native_github/review/tests.rs`;
GitLab `neutral_gitlab_*`, `gitlab/tests.rs`; Jira `neutral_jira_*` (13 tests);
Telegram `neutral_telegram_tests.rs` (5), `telegram/workflow.rs` (3); HTTP
`neutral_http_tests.rs` (4), `neutral_http_boundary_tests.rs` (3),
`native_http_dispatch_tests.rs` (2).

### 2.4 Designer authoring

Vitest, `src/__tests__/components/NeutralDesigner.*.test.tsx` (16 tests):
authoring (5), engine (3), native (4), rpc (3, real invoke/HTTP mapping), base.
Rust transport: `neutral_transport_tests.rs` (3),
`neutral_trigger_transport_tests.rs` (3). No browser test runs
author → publish → bind → run → inspect.

### 2.5 The 22 processes of plan section 11a

Verdicts come from the installed handler list in
`fixtures/neutral-native-catalog.json` (`native.*`, `host.*`, connectors).
"M" names the missing capability. Verdicts marked `?` need confirmation by
building the draft; they are not tested.

| # | Process | Status | Why |
|---|---|---|---|
| 1 | Assignment (spawn/resume agent, deliver task) | P | `native.agent.role` runs an ego role; spawning a TUIC peer session is not a catalog operation |
| 2 | Parallel staffing | P | ForEach with `max_concurrency` exists; selection of ownerless work with scope-conflict check has no operation |
| 3 | Sequential development batch | W | `native.story.batch.admit/land/pin/verdict` |
| 4 | Independent review | W | `native.review.bounded`, `native.review.change`, `native.agent.role` |
| 5 | Finding disposition | W | `native.correction.assess`, `native.correction.policy.assess` |
| 6 | Advisor consultation | P? | parallel `host.model` calls in a Fork; no consensus operation |
| 7 | External PR intake | P | GitHub snapshot + review exist; compatibility check with local trunk needs `native.git`-style read |
| 8 | Story intake and closure | W | `native.story.read/create/transition`, `native.operator.automatic_close` |
| 9 | Integration (serialized landing) | P | `native.story.batch.land`, `native.publication.push`; machine-wide landing serialization is not a node |
| 10 | Compilation and verification on a remote box | M | `native.verification.run` and `host.script` run locally; no remote slot admission |
| 11 | Deferred proof batching | M | no durable queue of pending validation obligations |
| 12 | Evidence reuse | P | `run/evidence.rs` hashes proof; no workflow operation |
| 13 | CI observation for a commit | P | `host.http` poll loop with `Wait`+`Loop`; no CI connector |
| 14 | Public release snapshot | M | no operation; separate authority required |
| 15 | Result/event routing (mail, pump) | M | no inbound peer-mail trigger source |
| 16 | Blocked-work recovery | M | needs agent session registry read operation |
| 17 | Agent handover | M | no successor-recording operation |
| 18 | Workspace rescue | M | `native.workspace.*` pin/stage only; no stranded-worktree scan |
| 19 | Operator decisions and reporting | W | `native.operator.wait`, `telegram.send`, `host.approval` |
| 20 | Progress projection | P? | `host.script` can compute; no read of story/plan counters except `native.story.read` |
| 21 | Health and cleanup | M | no owned-resource reaper operation |
| 22 | Knowledge upkeep | M | no memory-store operation |

Count: W = {3,4,5,8,19} = 5; P = {1,2,6,7,9,12,13,20} = 8;
M = {10,11,14,15,16,17,18,21,22} = 9. Total 22. Most M items need an inbound
trigger source or a new native operation family, not a kernel change.

## 3. Permutation matrix

### 3.1 Test index

IDs below are used in the matrix. File names are under `run/` unless noted.

| ID | Test (file:line) |
|---|---|
| R1 | `neutral_execution_replays_selected_path_results_and_wait_after_owner_restart` neutral_execution_tests:292 |
| R2 | `restart_keeps_dispatch_claim_inspectable_and_never_reinvokes_it` neutral_runtime_tests:243 |
| R3 | `lost_write_response_survives_restart_and_reconciles_without_second_write` neutral_effect_tests:291 |
| R4 | `restarted_owner_resumes_the_run_without_repeating_a_committed_effect` plan_delivery_tests:270 |
| R5 | `restart_reconciles_original_write_and_delivers_new_model_prompt` neutral_model_recovery_tests:91 |
| R6 | `retained_telegram_receipt_resolves_exact_wait_after_restart_once` neutral_telegram_tests:83 |
| R7 | `neutral_start_replays_retained_closure_...after_owner_restart` neutral_tests:148 |
| R8 | `subflow_frames_replay_same_owner_and_readmit_typed_child_arguments` neutral_runtime_tests:300 |
| R9 | `collection_owner_fences_results_and_replays_ordered_join_once` neutral_scope_tests:220 |
| C1 | `cancelled_pending_effect_keeps_claim_until_proven_stop_without_reopening_graph` neutral_effect_tests:505 |
| C2 | `expired_actor_requests_cancel_but_does_not_release_without_stop_evidence` neutral_effect_tests:642 |
| C3 | `neutral_execution_fences_results_and_retains_cancelled_history` neutral_execution_tests:399 |
| C4 | `cancellation_stops_owned_process_and_keeps_mutation_certainty_separate` neutral_script/tests:261 |
| C5 | `cancelled_pinned_check_stops_descendants_and_fences_late_workers` check_tests:77 |
| C6 | `stopped_orphan_observation_settles_without_graph_reopen` neutral_effect_orphan_tests:7 |
| L1 | `lost_create_response_retains_original_effect_and_never_repeats_create` neutral_jira_mutation_tests:10 |
| L2 | `review_publication_lost_post_response_stays_uncertain` / `..._lost_reply_is_never_written_again` native_github/review/tests:110,278 |
| L3 | `uncertain_note_reconciliation_never_reposts` neutral_gitlab_tests:104 |
| L4 | `telegram_missing_send_receipt_blocks_callback_and_reconcile_never_resends` neutral_telegram_tests:203 |
| L5 | `asynchronous_and_rate_limited_writes_retain_original_effect_without_retry` neutral_http_boundary_tests:196 |
| L6 | `mutation_with_lost_output_remains_uncertain_after_stop_replay_and_reconciliation` neutral_script/tests:182 |
| L7 | `model_tool_lost_write_reconciles_same_call_without_reinvoking` neutral_model_tool_tests:160 |
| D1 | `early_callback_wins_pending_ack_and_exact_results_are_immutable` neutral_effect_tests:225 |
| D2 | `exact_approval_revision_has_one_committed_winner_and_never_expires_to_yes` neutral_effect_tests:334 |
| D3 | `declined_effect_cannot_reopen_with_new_revision` neutral_effect_regression_tests:202 |
| D4 | `telegram_old_question_and_changed_target_cannot_select_new_wait` neutral_telegram_tests:136 |
| D5 | `model_mcp_waits_for_committed_result_and_duplicate_calls_write_once` / `concurrent_model_tool_duplicates_enter_the_writer_once` neutral_model_mcp_tests:127, neutral_model_tool_tests:236 |
| D6 | `stale_results_cannot_satisfy_a_later_join_snapshot` fork_scope_tests:108 |
| D7 | `aggregate_refuses_stale_duplicate_and_invalid_results` collection_tests:134 |
| D8 | `neutral_start_is_atomic_and_refuses_changed_request_identity` neutral_tests:204 |
| V1 | `current_admission_refuses_and_invalid_effect_result_retains_uncertainty` neutral_runtime_tests:195 |
| V2 | `publication_and_dispatch_enforce_installed_boundaries_and_current_grants` neutral_http_boundary_tests:15 |
| V3 | `installed_admission_rejects_forged_metadata_and_revoked_grants` neutral_gitlab_tests:23 |
| V4 | `installed_binding_rejects_foreign_references_override_and_revoked_write` neutral_jira_tests:140 |
| V5 | `native_story_create_refuses_stale_source_and_revoked_actor` native_story_create_tests:151 |
| V6 | `client_labels_cannot_supply_authority` catalog_tests:161 |
| V7 | `expired_approved_effect_settles_and_later_sibling_runs` neutral_effect_regression_tests:63 |
| P1 | `publishing_a_new_revision_mid_run_keeps_the_run_on_its_pinned_revision` native_http_dispatch_tests:26 |
| P2 | `stale_writers_cannot_replace_a_revision` neutral_publication_tests:151 |
| P3 | `neutral_control_is_revalidated_on_verified_publication_read` neutral_control_tests:351 |
| P4 | `NeutralDesigner.authoring: edits recorded draft ... saves at the original revision` test.tsx:71 |
| S1 | `latest_pr_generation_waits_for_original_effect_certainty_and_stop` neutral_trigger_coalescing_tests:13 |
| S2 | `updates_of_an_active_pr_follow_overlap_policy_and_publish_one_review_per_head` pr_review_workflow_tests:409 |
| S3 | `a_head_pushed_during_review_supersedes_the_publication` pr_review_workflow_tests:464 |
| S4 | `unchanged_snapshot_does_not_re_emit_retained_event` neutral_trigger_baseline_tests:222 |
| S5 | `run_own_label_write_records_self_event_and_starts_no_second_run` neutral_trigger_baseline_tests:246 |
| S6 | `enabling_a_binding_does_not_admit_preexisting_open_resources` neutral_trigger_baseline_tests:6 |
| S7 | `replay_requires_project_authority_and_retains_original_start_request` neutral_trigger_tests:116 |
| S8 | `issue_template_binds_per_repository_and_label_and_starts_one_run_per_generation` neutral_issue_template_tests:107 |
| S9 | `scheduled_text_reaches_model_condition_and_exact_local_approval_without_git` parity_schedule_tests:15 |
| W1 | `branch_local_question_and_uncertain_claim_leave_read_only_sibling_runnable` neutral_effect_tests:562 |
| W2 | `fork_owner_waits_for_current_producers_and_retains_failed_arrival` neutral_scope_tests:377 |
| W3 | `actor_collection_nested_subflow_settles_failed_fork_without_capacity_deadlock` neutral_runtime_tests:786 |
| W4 | `operator_wait_retains_exact_typed_answer_and_only_fixed_continuation` native_operator_tests:73 |
| W5 | `neutral_wait_publication_refuses_durations_beyond_existing_workflow_ceiling` neutral_control_tests:227 |
| W6 | `neutral_execution_routes_unknown_and_records_generic_limits` neutral_execution_tests:478 |
| X1 | `neutral_execution_serializes_decisions_and_rejects_corrupt_history` neutral_execution_tests:513 |
| X2 | `neutral_actor_storage_write_failure_does_not_become_workflow_failure` neutral_runtime_tests:827 |
| X3 | `neutral_replay_refuses_corrupt_bytes_and_unsupported_contracts` neutral_tests:304 |
| M1 | `neutral_transport_mcp_drafts_share_the_desktop_contract_without_result_wrapping` mcp_transport_neutral_tests:5 |
| M2 | `workflow_typed_actions_reject_invalid_inputs_before_effects` mcp_transport_ancillary_tests:2528 |
| H1 | `neutral_transport_replay_requires_verified_owner_not_transport_admission` mcp_http/workflow_neutral_routes:54 |
| H2 | `neutral_transport_owner_emits_wait_and_result_cursors_and_retains_inspection` neutral_transport_tests:190 |
| U1 | NeutralDesigner.rpc `sends exact neutral definitions and pins through invoke and HTTP` rpc.test:82 |
| U2 | NeutralDesigner.rpc `reads retained ledger envelopes through real HTTP mapping` rpc.test:132 |
| G1 | `github_sandbox_pr_gets_one_review_from_the_neutral_engine` pr_review_workflow_tests:548 (live) |
| G2 | `sandbox_labelled_issue_gets_one_triage_comment_and_no_second_run` neutral_issue_template_live:113 (live) |
| G3 | `github_sandbox_issue_and_pr_sources_execute_installed_authenticated_connector` parity_github_live:29 (live) |
| G4 | `native_issue_live_sandbox_comment_and_label_have_provider_receipts` native_issue_live_tests:88 (live) |

### 3.2 Matrix

Columns: **Trg** trigger source; **Shp** data/collection shape; **Wt** waits;
**Rd/Wr** effects read/write; **Rst** restart at each durable point;
**Can** cancel; **Lost** lost response; **Dup** duplicate/late callback;
**Rev** revoked authority; **Dft** draft republish.
`-` = dimension does not apply. `GAP` = no test found.

| Use case | Trg | Shp | Wt | Rd/Wr | Rst | Can | Lost | Dup | Rev | Dft |
|---|---|---|---|---|---|---|---|---|---|---|
| A scheduled text→model→cond→approve | S9 | GAP (single text) | D2, W4 | model read; local approval | R1 (wait only); R2 | C1 | GAP | D2 | V1 | P1 |
| B script→op→subflow | - (manual Start) | R8 | - | native.artifact.pin write | R8, R2 | C4 | L6 | D1 | V1 | P1 |
| C delivery (T3) | GAP (StartDelivery manual) | loop of attempts | GAP | git commit write | R4 (one DP) | GAP | GAP | GAP | GAP | GAP |
| D paginated HTTP→ForEach→agg | - | R9, D7, `parity_tests:89` | - | HTTP read | R9 (one DP) | GAP | `pagination_and_response_failures_never_become_success` | D7 | V2 | GAP |
| E1 GitHub issue triage (T1) | S4,S5,S6,S8 | GAP | W4 | issue read; comment/label write | R2 | GAP | L2 (review only) | S5 | V6 | P1 |
| E2 GitHub PR review (T2) | S1,S2,S3 | GAP | D2 | PR read; review write | R3 | C1 | L2 | S2,S3 | GAP | P1 |
| E3 GitLab | GAP (source exists, no run-level trigger test beyond `neutral_gitlab_intake_tests:10`) | - | - | read: `neutral_gitlab_read_tests:11`; write: `neutral_gitlab_tests:69` | R3 (generic) | GAP | L3 | `general_note_runs_through_owned_effect_and_is_not_duplicated` | V3 | GAP |
| E4 Jira | no trigger source | - | - | `neutral_jira_observation_tests:10`; `neutral_jira_mutation_tests` (5) | `neutral_jira_recovery_tests` (2) | GAP | L1 | GAP | V4 | GAP |
| E5 Telegram | no trigger source | - | R6 | send/reply | R6 | GAP | L4 | D4 | `installed_telegram_metadata_is_pinned_and_current_chat_is_checked` | GAP |
| E6 HTTP write | - | - | GAP | `native_http_dispatch_tests:*` | R3 (generic) | GAP | L5 | GAP | V2 | P1 |
| F model + tools | - | - | GAP | `neutral_model_tool_tests` (4) | R5 | GAP | L7 | D5 | `model_mcp_denies_unknown_tools_methods_ids_and_stale_origins` | GAP |
| G fork/join | - | W2, D6 | W1 | - | GAP | GAP | - | D6 | - | GAP |
| H loop | - | `loop_owner_tests_current_carried_snapshot...` | - | - | GAP | GAP | - | same | - | GAP |
| I nested subflow in scope | - | W3, R8 | - | - | R8 (no DP inside scope) | GAP | - | GAP | - | P1 |
| J Wait node timer | - | - | R1, W5 | - | R1 | `Cancel` during Wait: GAP | - | GAP | - | - |
| K operator wait (T6) | - | - | W4 | - | `operator_answer_fences_artifact_and_reconciles_original_resume:234` | GAP | - | `operator_answer_fences...` | `operator_generic_approval_and_automatic_missing_proof_fail_closed:206` | GAP |
| L issue graph (T7) | S8 | `native_issue_graph_joins_...:95` | D2, `issue_publication_approval_decline_routes_refusal_without_dispatch` | issue write | GAP | `native_issue_child_observe_and_cancel_keep_original_identity_without_dispatch:54` | `native_issue_effect_requires_approval_adds_label_and_preserves_lost_write:204` | GAP | `native_issue_source_refuses_foreign_stale_and_edited_requirements:160` | GAP |
| M Designer authoring | - | - | - | U1,U2 | - | - | - | - | - | P4, P2 |
| N MCP/HTTP transport | H1 | - | - | M1,M2,H1 | - | GAP (Cancel via HTTP/MCP) | - | GAP | V6 | GAP |

Reading the matrix: the **lost response** and **revoked authority** columns are
covered per connector. The **restart** column is covered at DP2, DP5, DP6 and
DP11 only in single scenarios. The **cancel**, **duplicate callback** and
**draft republish** columns are empty for most use cases.

### 3.3 Restart coverage by durable point

| DP | Test | Missing |
|---|---|---|
| DP1 | R7 | none |
| DP2 | R1 (Wait path) | restart between `Prepare` decision and effect `Prepare` |
| DP3 | none by name | prepared-not-dispatched, approval pending, restart |
| DP4 | R6, `operator_answer_fences...` | restart while question open and deadline passes during downtime |
| DP5 | R2 | restart for each connector family (only generic) |
| DP6 | R3, R5 | restart after `Pending{handle}`; restart after `Completed, stopped:false` |
| DP7 | none by name | crash after effect settled and before `Complete` applied |
| DP8 | `neutral_actor_storage_write_failure...` (X2) | restart after `Cancel` with unsettled effect |
| DP9/10 | `flush_trigger_intents` has no test named for crash recovery | crash after intent reserve, before run start |
| DP11 | R8 | restart with a child-frame effect in flight |
| DP12 | R9 | restart with half of the items dispatched |

## 4. Prioritized gap list

Transport key: **IP** in-process Rust (scenario harness L1); **HTTP** against
`make agent-test` (`:19876`); **MCP** `workflow` tool; **UI** Designer in
agent-browser; **LIVE** real provider.

### P1 (a defect here loses or duplicates a write, or leaves a run stuck)

| # | Gap | Plausible bug it would catch | Transport |
|---|---|---|---|
| 1 | Restart at every DP (DP3, DP4 with deadline passing, DP6 pending, DP7, DP8 with live effect) for one write connector, parametrized | Recovery calls the adapter a second time after a crash between `Dispatch` commit and `Observe` commit; or a run waits forever because `seen` was recorded for an effect whose reconcile never ran | IP |
| 2 | Cancel during each of: approval wait, `Wait` node, ForEach with items in flight, subflow child with effect, model tool call | Cancel releases a resource claim before `stopped:true`; cancel of a parent leaves a child-frame effect uncancelled (`recover_neutral_effects` walks `snapshot.effects`, shared across frames) | IP |
| 3 | Duplicate and late callback matrix: same `Observe` twice, `Observe` after `Cancel`, `Answer` after dispatch, `Answer` with stale revision, callback for previous activation of a loop | A late result of loop iteration N satisfies iteration N+1; a second answer re-opens a settled effect | IP |
| 4 | Black-box run through HTTP `/workflows/neutral/action`: Start → Inspect → Answer → Inspect → Cancel on a real instance | Transport maps `expected_sequence` or `command_id` differently from the desktop path, so a retry after a timeout starts a second run or answers twice | HTTP |
| 5 | Same run through MCP `workflow` tool, including invalid-then-valid retry | MCP wrapper strips or renames `command_id`, so idempotency is lost only on MCP | MCP |
| 6 | Trigger crash recovery: kill between `reserve_trigger` and `start_trigger`/`trigger_started`, then replay the same event | Event consumed twice (two runs), or consumed zero times (intent never flushed because `supersede=false` path skips it) | IP |
| 7 | Trigger → run → cancel by supersede while the old run has an uncertain write, then new event for a third generation | Newest generation starts while the old effect is still unsettled (resource written twice) or the middle generation is lost | IP |

### P2 (a defect here gives a wrong result or a wrong status, no data loss)

| # | Gap | Plausible bug | Transport |
|---|---|---|---|
| 8 | Draft republish while a run is **waiting** (not only mid-dispatch, `P1`): edit draft, publish new revision, answer the old wait, assert the pinned definition and the pinned child still execute | Answer path re-resolves the publication from the current catalog and runs the new graph | IP |
| 9 | Designer end-to-end: author from template → save → publish → enable binding → fire fixture event → open run inspection | Designer sends a definition that passes Vitest with fake client but fails `validate` on the backend (key order, ordinal map keys); inspection shows a stale cursor | UI + HTTP |
| 10 | Revoked authority **between** approval and dispatch for each connector (grant removed after `Answer`, before `Dispatch`) | Old approval is used as authority after the grant is removed (`dispatch_neutral` skips only when an effect exists; check `admitted` is called first for every connector family) | IP |
| 11 | Scope shapes with faults: ForEach of 0 items, 1 item, page boundary, item fails while siblings run, cancel mid-scope, restart mid-scope | Empty collection never settles; failed item leaves other items running after the scope is settled | IP |
| 12 | Acceptance C regression after legacy deletion: converted delivery with correction loop and review rejection → re-enter fix owner; `resolve_plan` and `issue_delivery` templates (T4, T5) | A template regression is invisible because the tests that proved conversion were deleted (`499b60fc76`) | IP |
| 13 | GitLab and Telegram and Jira as **trigger/wait** full runs (only slice tests exist) | Normalizer produces a generation that does not change when the MR head changes, so a new head is ignored | IP |

### P3 (coverage hygiene and live proof)

| # | Gap | Plausible bug | Transport |
|---|---|---|---|
| 14 | Model-driven path with a tool write and approval on a branch inside Fork | Branch-local approval blocks siblings | IP |
| 15 | HTTP write with approval and timeout (`Pending` ack never arrives) | Run stays `running` past the run deadline with no `Expire` because the actor sleeps on `wait_due` only | IP |
| 16 | Live suite for Jira, GitLab, Telegram | Fixture shape differs from the real provider | LIVE |
| 17 | Per-DP crash test harness for `Limit`/`Expire` while effects are in flight | Expired run leaves a dispatched effect with no cancel call | IP |
| 18 | The 9 M and 8 P process shapes of section 2.5: write one draft per P process to prove the stated gap, one test per M process that the catalog refuses it | A process looks expressible but fails at publication time | IP |

## 5. Proposed use of the layers

- **L1 scenario harness**: one builder that gives a run, a fake host with
  per-call fault points (before `Dispatch`, after `Dispatch`, after adapter,
  after `Observe`), a clock, and "restart owner" (drop the owner, reopen
  `RunStore::open_at`, `WorkflowRuntime::spawn` equivalent). Gaps 1, 2, 3, 6,
  7, 8, 10, 11, 12, 13, 14, 15, 17 run here.
- **L2 black-box**: gaps 4, 5, 9 against `make agent-test` (`:19876`) with
  throwaway repositories; no mocks.
- **L3 live**: gap 16 only, behind explicit credentials.

## 6. Unknowns

- Test bodies were not read for every named test. A named test may cover less
  than its name states.
- Whether `flush_trigger_intents` crash recovery is tested under a name that
  does not mention it (gap 6).
- Scheduled and GitLab trigger end-to-end tests were found only by name.
- Section 2.5 verdicts are by catalog handler names, not by building drafts.
