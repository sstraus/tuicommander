# Orchestrator as a TUIC workflow — implementation plan

Date: 2026-10-30. Status: approved by Boss on 2026-10-30; includes the requested editable standard-workflow templates. The unattended-PR repository allow-list remains empty until WF17 names the exact trial repository. Direction: **B, pinned delivery subflows plus a portfolio view**. Boss authorized this plan revision, its commit and main-store story creation. No implementation, production change, operational trial, push, merge or release is authorized by this assignment.

## 1. Decision, scope and acceptance boundary

Build one native TUIC engine. The first executable subflow is A's explicit implement → independent critic → triage/repair → integrate → proof gate → post-landing batch tests chain. B adds pinned Call nodes and a portfolio projection over that engine, not another scheduler and not a permanent coordinator session. Agents perform bounded decisions and work; the daemon owns intake, admission, transitions, effects, recovery and obligation accounting even when no AI Chat or coordinator PTY exists.

Success requires all of the following:

1. Every capability ID in the current coordinator report and every feature in the origin screenshot has a destination, story and validation owner below. A mechanism can retire only after its replacement is verified. No capability disappears by omission.
2. All three use cases in section 5 run end to end from durable events, including negative, stale and uncertain outcomes. An incoming PR needs no human before its merge decision. Issue delivery requests developer approval after PR creation. Full orchestration needs no scheduler agent.
3. Boss can edit a real rule in a draft, publish it, inspect the running pinned subflow, answer a branch-local question, and see exact evidence, queue reasons and cost coverage through desktop or browser. AI clients use the same service contracts.
4. The paired real-story trial proves outcome, proof, time, cost and interventions at least equal to the current lane. No retirement on static mocks or a single happy-path demonstration.

### Authoritative inputs and source limits

- [Design study](orchestrator-as-workflow.md), including its exhaustive tool/rule register and paired-trial protocol.
- O = `/Users/stefano.straus/Gits/personal/orchestrator`. O report `reports/_all/2026-10-29-orchestrator-features.md`, source revision `b424656e1daff0afe8598162a4cce9966387c884`: 105 capability rows. Its status qualifiers remain in force: source presence is not installed/live health.
- O report `reports/_all/2026-10-29-flow-editor-vs-our-workflow.md`, section “What the screenshot shows”. The screenshot's packs are explicitly excluded by Boss.
- Approved canvas copies in O `reports/_all/img/2026-10-29-orchestrator-workflow/` and local `docs/mocks/orchestrator-workflow/`. Boss's brief names approval commit `b3fe9a9575`; this checkout's last canvas commit at inspection was `3a7e31a50c`. This plan uses the files read, not an assumed equivalence of those commits.
- TUIC baseline at drafting: `3a7e31a50c`. MDKB was queried first, then source was read where indexed results did not establish the contract: `src-tauri/src/workflows/definition.rs`, `run/runtime/policy.rs`, `src-tauri/src/automations/publish.rs`, and `docs/backend/workflows.md`.

Review revision: the complete Fable/Astra/Grok verdicts in `~/Gits/.tmp/tri-all.md` were read on 2026-10-30. All ten consolidated requests are incorporated; section 10 records their resolution. The approved contracts below are the source for main-store stories; story creation does not start implementation.

The later D/E/F and event-intake sections of the native workflow documentation supersede its historical slice-C publication refusals. Current definition validation permits Gate and plan create/dispatch roles. Do not file stories to reimplement them. Conversely, a documented API or GitHub primitive is not proof of an unattended workflow adapter. This plan distinguishes reuse from additions. Runtime health, external credentials, loaded schedules, remote installed versions and actual model availability were not probed in this documentation task.

## 2. Contracts that every story must preserve

### 2.1 One state owner, several adapters

The existing Rust definition/run services and event store remain authoritative. Extend their versioned graph/reducer contract, retaining old-event replay; do not build a JavaScript scheduler, wrap `coord-pump` as a long-running node, or copy the reducer into Fabbrica. Portfolio is a projection plus pinned parent/child edges, never a second writer to child state.

Definitions pin the complete transitive set of subflow revisions, policy digests, prompts and repository policy. Editing a draft cannot modify any active run. A child execution has one owning daemon. WF09 is same-daemon definition/policy pinning and nested inspection: Story Dispatch already supplies the story-level Call and must not dispatch a story twice. Cross-daemon workflow ownership, generalized placement and portable import/export schemas are outside this migration; WF05 defines only the typed executor boundary and reuses local/rb adapters. Disconnected execution is unknown, not done.

<!-- DEFERRED (2026-10-30) — cross-daemon workflow placement and portable-schema implementation are outside coordinator replacement; typed adapter contracts preserve the future seam without a second scheduler. -->

Execution ports accept `(run, execution, activation, attempt, generation, effect)` identity, artifact, argv or prompt/profile, target requirements, resource keys, deadline, secret references and output schema. Adapters return executor identity/fence, exact subject and toolchain, logs/artifact digests, terminal outcome or uncertainty, usage and stop proof. Local/rb adapters place and execute current work; only TUIC selects successor edges. A future Fabbrica adapter must obey this port, but its implementation is not a WF17 prerequisite. Reusable definitions contain artifact/workspace IDs rather than Mac paths.

Keep useful deterministic tools (formatters, Git probes, mutation-universe collectors, environment provisioning, browser helpers) behind typed adapters. Port the scheduling and correctness rules into TUIC. Retaining an executable does not retain a second coordinator if it cannot independently choose work or successors.

### 2.2 Identity, effects and recovery

- Intent commits before a spawn, merge, publication, notification or destructive cleanup. A repeated request with identical key/payload returns the receipt; conflicting payload fails. A lost response cannot create another effect.
- Unknown effects remain unresolved until lookup or stop proof establishes their outcome. Cancellation is requested, then observed; never release a shared slot or destroy the workspace merely because a lease expired.
- Distinguish artifact approval, integration, proof completion and release readiness. “Integrated, proof pending” is valid; “release ready” with debt is not.
- Launch ownership is a daemon-run authority, not the existence of a coordinator PTY. ACP peer identity must not be disguised as a terminal. Keep existing managed report bindings and generation fences.
- Child closure, continuation, cleanup and artifact successor handover are durable transactions. A typed RESULT is evidence, not automatic acceptance; idle/exit is never success.
- A canonical repository key unifies aliases, renamed worktrees and remote references. One integration writer per canonical repository across all runs and both old/new lanes during cutover.

### 2.3 Parallelism, decisions and budgets

Use Story Dispatch for separate stories. Use Fork/all-Join for independent activities within one delivery or review, with exact fork activation, branch set and generation. Existing Join remains an exclusive merge. Publication validates matching regions, required/optional arrivals, canceled/failed/skipped outcomes and bounded dynamic fan-out. A stale arrival never satisfies a new attempt. There is no parent all-Join over story receipts: Story Dispatch owns story fan-out and delivery aggregation. Exactly one parent proof-wait activation observes each cohort; story children finish at integrated/proof-pending and never wait for that cohort.

On critic FIX, the owning daemon closes the entire old fork generation as superseded, fences old reports, requests cancellation of old-generation checks and waits for owned-process stop proof before conflicting work resumes. It then runs Implement once outside the old fork, captures the new artifact and creates a new fork generation with both critic and checks branches. It reruns both branches on the new artifact; old join obligations terminate as superseded rather than waiting forever. Prior deterministic receipts remain history and do not supply new arrivals. The current generation alone has an all-Join. Review-count budget spans generations; check retries do not consume critic rounds.

Ask suspends only its activation and dependents. An explicit run-wide pause, an uncertain shared effect or a shared safety stop may suspend a wider named scope; the view shows that scope. Answers bind question revision and subject digest. First committed valid answer wins; stale or conflicting answers refuse. Deadlines never imply approval.

Two total critic reviews means initial review plus one re-review, not two repairs. Persist review count, rejects, repair traversals and deterministic-check attempts separately across replacement and restart. Exhaustion never auto-accepts. The seven ordered FIX/STORY/ACCEPT rules in the study become deterministic policy. STORY findings are proposals under intake policy, not unauthorized scope expansion. Model scores and decisions cannot override failed deterministic checks.

Resource policy separates ready from admitted. Preserve repository locks, build admission, freeze allow-lists, one mutation process, local JS versus rb Rust, native-only proof, GUI/device isolation and owned-process cleanup. Optional utilization targets cannot override capacity, global routing, assignment authority or per-run concurrency (currently default two, configurable one to eight). Missing exact approved model/profile is a visible block, not substitution.

### 2.4 Evidence and external actions

Evidence binds criterion/requirement revision, pre-fix commit for RED, clean source commit/tree, merge base/result, target/toolchain, command/config/test-scope digests, reviewer identity and freshness inputs. Preserve REALITY/SYMPTOM/TEST-CHANGE and precise omissions at legacy story boundaries. A zero-test command cannot satisfy a nonempty intended test scope. Required native evidence cannot be discharged by Linux results.

Atomically journal an integration receipt and proof obligations (or one recoverable cross-store outbox intent). Batches pin membership and artifact: one suite cannot clear later arrivals. Mutation reuse requires the complete requested universe plus compatible source/tests/tools/configuration/target evidence, not a historical tested row. Mutation registry backup/restore gets an owner before it becomes the sole evidence copy.

An external action uses a typed Tool contract, not an agent's free-form shell command: resource identity, expected issue/PR revision, exact action payload, policy authority, idempotency intent and reconciled receipt. GitHub actions compare current head/base before posting reviews or creating PRs. Authorization to run a workflow is not blanket push/merge/release authorization. Proposed workflow policies must receive Boss approval before production activation. No new local-file restriction or enterprise trust boundary is introduced.

### 2.5 Standard automation workflows, not permanent static paths

**Decision (Boss, 2026-10-30):** ship PR review, issue triage and label-polled issue delivery as standard workflows on this same engine. WF13 delivers the editable `pr-review` built-in template; WF14 delivers editable `issue-triage` and `issue-delivery` templates plus the label-poll trigger binding. Users can open each graph in the existing Designer, customize a draft, publish it or duplicate it as an independent workflow. These are ordinary versioned definitions with template origin/version metadata, not special opaque automation executors. New runs pin the selected publication; existing runs retain their graph and policy pins. Built-in updates offer a diff/new draft and never overwrite a customized definition or repin a run. Essential open/edit/duplicate controls and transport parity use WF11, not the optional WF18 gallery.

**Obstacle analysis, based on source:** `automations/pr_reviews.rs` separates snapshot admission, read-only review and `post_pinned_review`; the latter currently posts COMMENT with exact `commit_id`, not APPROVE. `triage.rs` separates typed Accepted/Rejected/NeedsInput from host-owned comment/label effects and stops advancement on unknown comment delivery. `event_intake.rs` separates polled snapshots, atomic admission keys and native-story bridging. These are useful adapter boundaries, not evidence that static scheduling must remain. `workflows/definition.rs` does not yet declare these Tool/trigger contracts; WF05 supplies typed effects, WF06 evidence policy, WF09 pinning and WF19 timers. No source-level obstacle requires a second engine or a permanently fixed automation path. This conclusion is a design feasibility finding, not proof of live provider credentials, installed subscriptions or production parity.

Extract/reuse their current logic as typed backend contracts: PR snapshot/admit/current-head probe and pinned review publication; triage evaluation receipt and ordered comment/label publication; label-poll snapshot/admission/digest-generation and native-story intake. Existing Rust GitHub, poller and persistence adapters remain reusable implementation code. What retires under R7 is their fixed orchestration/dispatch authority, not the exact-head or uncertain-effect safeguards. The shared poller produces events; a versioned trigger binding selects one owning workflow consumer, rather than a second watcher deciding successors.

| User can customize in a versioned draft | Fixed backend safety invariant, not an editable escape hatch |
| --- | --- |
| Role prompts and approved model/profile choices | Global routing and role/authority contracts still apply; no secret values in templates |
| Targeted checks, matching targets, required-CI selection and stricter proof rules | A review-only PR workflow cannot emit APPROVE without passing current required proof and CI; unknown/stale is not pass |
| Verdict table, thresholds, bounded recheck/repair and comment wording | The section 5.2 minimum refusal/incomplete rules cannot be weakened; malformed or unsafe tables fail publication |
| Intake/rejection labels and Accepted/Rejected/NeedsInput routing | Host-owned idempotent comment/label effects preserve ordering and lookup-only uncertainty; label changes never erase admitted history |
| Repository subscriptions, poll/retry policy and selected delivery definition | No unattended PR creation outside the Boss-approved exact `owner/name` allow-list (currently `[]`); choosing a repository in a template is not authority |
| Graph composition, notifications and branch-local questions | Exact-head review pinning, current subject fencing, independent proof and explicit merge/release authority remain enforced in typed services, even in duplicated graphs |

**Lossless settings migration (WF13/WF14; operational cutover only WF17):** inventory and snapshot `AutomationsConfig` and referenced definitions before conversion. Map `pull_request_reviews.repositories/definition_id` to a PR-review trigger binding and a pinned editable instance; map `event_intake.repositories/label/definition_id/triage/workflow` to issue trigger bindings, triage policy inputs and the exact pinned candidate definition. Preserve each triage repository, policy version, read-only attestation and rejection label. Carry referenced prompts, run profiles, repository/workspace/base branch, enabled state, schedule/timezone/once/grace/overlap, precheck including output injection, duration and shared concurrency into equivalent graph inputs, trigger/admission policy or retained definition references. Preserve original IDs and creator provenance in migration metadata; one referenced definition may serve multiple bindings without being copied over a user's edits. A field without an exact equivalent blocks that subscription's cutover with an actionable report; never silently discard or reinterpret it.

The import is versioned, idempotent and expected-revision guarded, with old-ID → definition/revision/trigger mappings and a field-by-field comparison receipt. Preserve disabled subscriptions as disabled. For existing PR-review subscriptions, preserve COMMENT-only publication as the migrated default; enabling APPROVE/REQUEST_CHANGES is a separately published policy change subject to the proof/CI floor, not an automatic privilege expansion. Preserve existing NeedsInput no-write behavior and existing repository boundaries; requested-information comments and new delivery behavior require an explicit new policy. Imported settings never expand the empty unattended-PR creation list. Current authorized legacy behavior is inventoried, not silently granted to new PR creation.

Import run/event keys, in-flight claims, publication/notification intents, unknown outcomes, admission generations and delivery receipts as historical/reconciliation bindings, not new runnable starts. Active legacy runs drain under their original owner unless an explicit receipt-preserving handover is proven. Stage converted bindings disabled; compare settings and replay fixtures before activation. WF17 transfers each subscription once by draining/fencing the old consumer, committing a cursor/admission watermark and enabling its native binding atomically or by a recoverable cutover intent. One consumer owns each intake and publication authority at all times. Rollback restores the snapshot/binding after reconciling native receipts and debt; it cannot replay a POST or resurrect already consumed event keys. R7 remains gated by per-subscription parity and Boss approval.

## 3. Approved implementation stories and order

WF IDs below are **plan-local step keys**, not main-store story IDs. Boss approved main-store story creation on 2026-10-30, with plan and plan-step linkage and the dependencies below. All criteria are unchecked future contracts, not completed work. RED names identify plausible regressions to protect, not claims of existing reproduced defects. Before calling any item a bug fix, reproduce it on the pinned pre-fix code; feature stories use contract-first tests. One integration owner schedules final targeted/batch validation; do not run full suites per child story.

| Wave | Story | Depends on | Delivery boundary |
| --- | --- | --- | --- |
| 0 | WF01 Contract/version and migration baseline | Approval | Replay and inventory contract |
| 1 | WF02 Fork/all-Join token semantics | WF01 | Within-graph parallelism |
| 1 | WF04 Daemon-owned agent/workspace lifecycle | WF01 | No coordinator-session/node prerequisite |
| 1 | WF05 Typed commands, resource admission and executor port | WF01 | Local/rb execution |
| 1 | WF06 Evidence and delivery decision policy | WF01 | Current independent proof |
| 1 | WF19 Durable Timer and bounded backoff | WF01 | Restart-safe flush/retry deadlines |
| 2 | WF03 Branch-local Ask and decision inbox | WF02, WF19 | Nonblocking questions |
| 2 | WF07 Transactional integration and cleanup | WF04, WF05, WF06 | Flagged one-writer integration, separate from WF08 |
| 2 | WF09 Same-daemon definition/policy pinning | WF02, WF04 | Reuse Story Dispatch, no duplicate story Call |
| 3 | WF13 Ship editable PR-review standard workflow | WF02, WF04, WF05, WF06, WF09, WF11, WF19 | Typed review nodes and built-in template, no merge |
| 3 | WF08 Proof obligations, batch gates and registry continuity | WF05, WF07, WF19 | Timer-flushed post-land proof |
| 4 | WF10 Portfolio intake, claims and scheduling | WF03, WF07, WF08, WF09 | Complete unattended delivery |
| 3 | WF11 Essential rule controls and API parity | WF03, WF05, WF06, WF09, WF19 | Existing Designer extended only as needed |
| 3 | WF12 Live graph, incidents, usage and costs | WF03, WF09 | Operator visibility |
| 4 | WF14 Ship editable issue-intake/triage/delivery standard workflows | WF03, WF09, WF13 | Typed polling/triage nodes and built-in templates |
| 5 | WF15 Release scope, countdown and publication policy | WF08, WF10, WF13 | Release readiness, not consent |
| 5 | WF16A Effective policy and retained-tool continuity | WF04, WF05, WF10 | Bounded policy/compatibility baseline |
| 5 | WF16B Owned-process health and recovery | WF04, WF05, WF19 | Critical health/safety contracts |
| 5 | WF16C Reports, acknowledgment and memory | WF10, WF12, WF19 | Communication continuity |
| 6 | WF17 Paired parity, fault campaign and phased cutover | WF01–WF15, WF16A–WF16C, WF19 | Core retirement authority; excludes WF18/WF16D/WF16E |
| Optional | WF18 Drag palette, presets and authoring conveniences | WF11 | Not on replacement/parity critical path |
| Optional | WF16D Environment/provisioning maintenance nodes | WF05, WF16A | Retained tools sufficient for WF17 |
| Optional | WF16E Upkeep/reduction convenience subflows | WF16A, WF16B, WF16C | Retained tools sufficient for WF17 |

The wave number follows prerequisite order; independent work in one wave is not permission for unbounded concurrency. WF17 requires preservation of the original capabilities, not rewriting their optional tool interfaces. WF16A pins the retained deterministic tools and proves their opt-in invocation/ownership without the old pump. WF16D/WF16E and WF18 may ship later without blocking retirement; their corresponding matrix rows must cite that retained baseline, not claim the optional native nodes are already implemented.

### WF01 — Version the shared contracts and freeze the preservation baseline

Problem: parallel tokens, subflows and new effect types must not reinterpret existing run events or quietly omit a legacy requirement.

- [ ] Define event-contract upgrade, retained old replay, receipt/effect schemas and ownership boundaries. Existing serial runs remain inspectable/cancelable with unchanged semantics; migration of an active run is explicit, not a draft edit.
- [ ] Pin the feature report, study register, policy sources and current native tests. Record effective hook/scheduler/model installation as verified, absent or unknown; source inventory alone cannot claim health.
- [ ] Define project policy imports for global routing, repository gates and publication authority. Resolve I1–I10 from the feature report explicitly; do not silently blend stale templates or hook coverage.
- [ ] RED: `old_exclusive_join_replays_without_parallel_wait` — catches reinterpretation of historical Join; `draft_publish_does_not_repin_active_run` — catches moving a live run to edited policy. Fixtures come from existing serialized runs.
- [ ] Evidence: versioned contract diff, migration fixtures, matrix baseline and API compatibility assertions. No operational migration in this story.

### WF02 — Add correlated Fork and all-Join

Problem: current exclusive Join cannot wait for parallel reviewer/check receipts.

- [ ] Add new node/event kinds with fork-instance and generation IDs; validate regions, exact required arrivals, bounded For each/Splitter fan-out and explicit canceled/skipped/failure edges. Add typed If/else predicates over declared inputs/receipts with explicit unknown outcomes, not an arbitrary expression evaluator. Preserve exclusive Join.
- [ ] Persist branch-local readiness and resource waits; one failed required branch cannot appear as successful join completion. Out-of-order and duplicate arrivals are idempotent. Critic FIX supersedes the complete old fork generation: daemon cancels/fences old checks, observes stop, runs Implement once, then creates a fresh fork and reruns both branches; old all-Join ends superseded.
- [ ] RED: `critic_fix_supersedes_fork_and_restarts_both_branches` — catches waiting forever for a stale-generation check receipt or reusing its arrival after repair; restart during cancellation must replay the same replacement generation.
- [ ] RED: `late_old_generation_cannot_complete_all_join` — catches cross-attempt receipt mixing; `cancelled_required_branch_blocks_join` — catches green-by-missing-branch; `duplicate_arrival_does_not_release_twice` — catches duplicate successors.
- [ ] Evidence: reducer replay across reopen/crash and graph-publication negative cases, including nested forks and empty fan-out with an explicit published rule.

### WF03 — Make Ask/approval durable and branch-local

Problem: current NeedsInput pauses the complete run and needs an explicit resume; that contradicts unattended parallel delivery.

- [ ] Introduce branch suspension and question revisions, structured answer schemas, digest/actor provenance, deadline outcomes and an inbox projection. An answer resumes only eligible dependents, with no auto-consent.
- [ ] Preserve explicit run-wide pause/cancel and legacy NeedsInput replay. Multiple tabs/AI answers race through expected revisions and payload-bound keys.
- [ ] RED: `unanswered_ask_does_not_stop_disjoint_story` — catches run-wide question blocking; `old_subject_answer_cannot_resume_new_artifact` — catches stale approval; `two_answers_commit_one_branch_transition` — catches double resume.
- [ ] Evidence: actual two-branch engine test, restart/reconnect inbox test and IPC/HTTP/MCP parity. An unanswered question must be visible and counted without a running coordinator.

### WF04 — Own agent, workspace and result lifecycle in the daemon

Problem: current managed story launch requires an owning coordinator PTY, and shell-side ownership files protect handovers and cleanup.

- [ ] Replace the scheduler-session prerequisite with durable run-owned launch authority. Support PTY workers and genuine ACP peers without fabricating terminal IDs; pin full role/prompt/model/effort/tool/scope/output contracts against global policy.
- [ ] Remove the Coordinator-Agent-role dependency for Create Stories in `definition.rs:305–321` and the first-Coordinator-report lookup in `run/runtime/effects.rs:37–53`. Use a typed, revision-bound proposal/input effect under daemon authority instead. Keep `definition.rs:105` plan-only graph validation: WF10 uses a plan graph, while WF14 reuses the intake bridge for story creation rather than placing Create Stories in a story graph. Coordinator role remains an optional bounded Author preset, not an execution requirement.
- [ ] RED: `create_stories_without_coordinator_node_uses_typed_proposal` — catches removing only the PTY check while executable publication/effects still require a Coordinator Agent; `story_graph_still_refuses_create_stories` protects the deliberate plan/story boundary.
- [ ] Use registered isolated workspaces, baseline attribution and canonical keys; preserve unsaved and pre-existing work. Persist launch, continuation-not-yet-delivered, report disposition, successor and close transaction. Restrict closure to owned idle processes with no live background receipt/test app.
- [ ] Resume/read incidents use durable conversation/stop/launch references. Missing inventory is unknown, not a dead-child verdict. No idle/spinner heuristic completes an attempt.
- [ ] RED: `daemon_delivery_launches_with_no_coordinator_session` — catches the permanent-PTY dependency; `lost_spawn_response_does_not_spawn_again` — catches duplicate workers; `close_preserves_unattributed_dirty_work` — catches handover data loss; `live_background_receipt_prevents_idle_reap` — catches premature closure.
- [ ] Evidence: daemon restart, delayed report, dead child and undelivered continuation tests against the real managed launch seam; compare baseline owners before/after rename.

### WF05 — Execute typed commands through bounded target/resource adapters

Problem: requiredChecks are not arbitrary positioned commands, and shared machine rules currently live in wrappers.

- [ ] Add Run command and typed Tool dispatch with argv/no implicit shell, workspace/artifact, target, secret references, timeouts, output schema and immutable receipts. Explicit shell use is visible and versioned. Never record secret values.
- [ ] Add durable ready/admitted/queued reasons and named resources for build, canonical Git, mutation, GUI and device operations. Reuse build-slot/rb admission initially; preserve freeze, fairness, reservations, stdin/environment transport and source manifests. No independent scheduler in those adapters.
- [ ] Reconcile remote handles across client loss; stop proof precedes retry/cleanup. Native Mac proof remains a separate requirement before an authorized forced rb rerun. The port records future task/generation/fence fields where relevant, but implementing Fabbrica or generalized cross-daemon placement remains deferred outside WF17.
- [ ] RED: `zero_matched_tests_cannot_pass_required_scope` — catches vacuous success; `lease_loss_keeps_effect_and_slot_uncertain` — catches overlapping replacement; `linux_receipt_cannot_discharge_native_mac_requirement` — catches false platform proof; `freeze_blocks_new_admission_not_inspection` — catches unsafe starts.
- [ ] Evidence: local command process-tree cancellation; rb transfer/queue/reconnect receipts; secret-redaction tests; matching-target proof. Provisioning/VM tools remain explicitly authorized maintenance tools, not automatic installs.

### WF06 — Port independent review, evidence freshness and bounded triage

Problem: current native Judge/Gate protects current checks/reviewer identity but does not encode the full coordinator's proof vocabulary and triage.

- [ ] Bind reviews/checks to source and requirements; preserve current native story approval semantics. Add generic review-subject contracts for PRs without marking them native stories Done. Required checks remain deterministic and unskippable by Judge.
- [ ] Encode all seven first-match FIX/STORY/ACCEPT rules from the study, malformed-score → unresolved, scope-limited STORY proposals, and explicit accepted-risk evidence. Enforce two total reviews by default with one repair traversal, configurable by pinned policy.
- [ ] Validate RED pre-fix provenance, REALITY/SYMPTOM/TEST-CHANGE, final consumer scope, strict all-targets clippy where required, relevant native checks and omissions. Legacy text is an adapter only, not the state model.
- [ ] RED: `artifact_edit_invalidates_review_and_checks` — catches stale acceptance; `implementer_cannot_supply_independent_review` — catches self-approval; `second_review_exhaustion_does_not_get_third_review` — catches retry off-by-one; `failed_gate_cannot_be_overridden_by_judge_yes` — catches prose bypass.
- [ ] Evidence: table-driven triage oracle for every ordered rule/boundary and report replay with changed requirements, reviewer replacement and accepted risks.

### WF07 — Integrate once and recover cleanup independently

Problem: native record_integration certifies an already performed merge; the coordinator currently owns locking, preflight, merge and partial-success recovery.

- [ ] Keep WF07 separately reviewable from WF08 and behind an explicit per-repository `native_integration_enabled` policy flag, default false, through WF17. Only isolated trial clones may enable it before repository-specific parity approval; rollback disables new integration admission without deleting receipts or debt.
- [ ] Add typed Integrate with canonical-repository serialization, expected base/source, synthetic merge-result checks, no-ff merge and both-parent/result receipts. Trunk movement forces revalidation; an out-of-band change invalidates freshness.
- [ ] Journal merge, dependency refresh, proof-obligation outbox and cleanup as separate recoverable phases. Preserve existing exit-2/3/4 distinctions as typed outcomes, not generic failed merge. Crash recovery inspects refs before any action.
- [ ] Cleanup proves integration and successor ownership before removing a worktree/branch through TUIC. Preserve permitted managed dirt, archive exact tips before authorized branch retirement, and never discard unknown work. Code conflicts go to repair; append-only union is only for declared documentation files and refuses malformed conflicts.
- [ ] RED: `merge_crash_preserves_obligation_and_unattributed_work` — catches a no-ff merge followed by crash/outbox recovery/cleanup removing a workspace with unattributed work; assert one merge, durable proof obligation and preserved workspace after restart. `disabled_repository_policy_refuses_native_merge` protects staged rollout.
- [ ] RED: `crash_after_merge_never_merges_twice` — catches replayed integration; `moving_base_requires_new_merge_checks` — catches stale admission; `cleanup_failure_keeps_integrated_state` — catches repeating a successful merge; `obligation_outbox_recovers_merge_commit_crash` — catches invisible proof debt.
- [ ] Evidence: isolated real Git clones, two simultaneous runs for one canonical repository, dirty/protected workspace cases, dependency-refresh failure and submodule-pointer attribution.

### WF08 — Own post-land obligations and complete proof batches

Problem: gate-queue, tested rows, batch Vitest and mutation evidence currently decide when landed work is actually proven.

- [ ] Import explicit obligation types and states (pending/running/converged/filed/blocked/unverified), exact landing file scope, survivor links and bounded batches (preserve current maximum twelve intake rows unless changed by approved policy). No ambiguous dispatch auto-respawn.
- [ ] Use WF19 durable Timer to flush any nonempty cohort, even one row, and persist bounded retry/backoff. WF08 cohort service starts independently when the integration outbox arrives, not after story aggregation. Exactly one parent proof-wait activation owns the wait per cohort; story children expose integrated/proof-pending and do not wait.
- [ ] RED: `single_landing_timer_flush_has_one_cohort_waiter` — catches waiting for a full batch or a child/parent cyclic barrier; prove the only story child terminates before proof and the parent observes one cohort completion after timer/restart.
- [ ] Pin cohort membership, source and environment; run required Rust default/headless tests, strict clippy/headless build on rb and frontend suite locally under one validation owner. A newer landing creates new debt; empty pending set is not new proof.
- [ ] Preserve mutation registry identities, complete-universe reuse rules, CRAP/coverage thresholds, AWS-only Test Janitor evidence comparison and no-production-change scope. Keep registry tools behind typed ports; implement consistent SQLite backup/restore and digest verification with a named operator owner.
- [ ] RED: `batch_success_does_not_clear_later_landing` — catches moving-batch debt loss; `partial_mutant_universe_refuses_reuse` — catches false cached proof; `dead_gate_worker_is_blocked_not_passed` — catches lost worker green; `registry_restore_preserves_identity_and_omissions` — catches unusable backups.
- [ ] Evidence: real saved registry fixtures, current environment measurement receipts, cohort restart and changed-test/config/target invalidation; no costly whole-tree mutation per story.

### WF09 — Pin same-daemon definitions and reuse existing Story Dispatch

Problem: B needs pinned reusable delivery definitions and visible nested ownership, not a second implementation of the story-level Call already provided by Story Dispatch.

- [ ] Pin exact same-daemon definition/policy revisions and declared input/output receipts. Reuse Story Dispatch as the story-level Call, including its child binding, budgets and integration aggregation; do not add a Call for each already-dispatched story. Limit new Call use to same-daemon bounded reusable decision/check subflows where needed by WF13; no generic cross-daemon schema or placement implementation.
- [ ] Bound recursion/fan-out at publication; correlate parent activation and child run, propagate blocked/failed/canceled/uncertain distinctly, and aggregate costs once. Child completion cannot mean “spawned”.
- [ ] Define cancellation and parent failure semantics: parent cancellation requests child stop; shared batch obligations survive and retain ownership. Recovery reuses the existing child ID/effect rather than starting another child.
- [ ] RED: `new_subflow_publication_does_not_change_running_call` — catches mutable template references; `lost_call_receipt_reuses_child` — catches duplicate delivery; `parent_cancel_does_not_erase_shared_proof_debt` — catches obligation loss.
- [ ] Evidence: expand/replay the exact explicit A delivery chain under B; nested child output, uncertainty and cancellation fixtures.

### WF10 — Replace pump/intake/staffing with portfolio delivery

Problem: without durable intake, ownership, wake/disposition and admission, removing the coordinator leaves work orphaned.

- [ ] Build opt-in event/manual/timer intake → deduplicated request → classify/claim → existing Story Dispatch with pinned delivery definition. Keep Boss requests, mail/results and decisions until linked to a story/run or explicitly dismissed. Repeated label polls are not new assignments; no webhook implementation is assumed.
- [ ] Create Stories uses WF04 typed daemon proposal/input effects without a Coordinator Agent node; executable plan publication and execution must both succeed without that role. Never add story-level Create Stories or a second dispatch layer. Complete delivery depends on WF08, whose independently triggered cohort service owns proof execution.
- [ ] Derive unowned/claimed/ready-wave/hold/parked/dirty-trunk/stash/publication-debt views from native stores plus declared legacy connectors during migration. Unknown inventory cannot release a claim or justify reaping. Keep full detail behind bounded summaries.
- [ ] Dispatch eligible stories by dependency integration receipt, scope conflicts, capacity and optional utilization policy. Preserve sequential batch mode and one critic over its complete artifact. Separate release membership, next and genuine Boss-only waits.
- [ ] RED: `duplicate_intake_creates_one_delivery` — catches redelivery duplicates; `unmerged_branch_claim_prevents_reassignment` — catches dual implementers; `done_without_integration_does_not_release_dependency` — catches premature dependent start; `report_without_disposition_remains_open` — catches lost responsibility.
- [ ] Evidence: no coordinator process present during intake, parallel delivery, blocked question, restart and release-owner alarms; bounded external inventory commands and cursor-based wake replay.

### WF11 — Expose essential rule controls through the existing Designer

Problem: the existing Designer already edits and publishes graphs, but the real critic cap, policy fields, pin navigation and new runtime decisions need usable controls/API parity.

- [ ] Expose standard workflow open/edit/duplicate with independent draft identity and template origin/version metadata; keep user edits on built-in updates and live pins on publication. These essential controls do not depend on WF18.
- [ ] Extend existing draft save/publish controls with review/reject/repair counters, exact Author role/prompt/profile/tools/scope, typed command/decision inputs, validation errors and pinned definition navigation. Keep distinct Publish/Discard semantics and expected revisions. Do not redesign palette dragging to satisfy coordinator replacement.
- [ ] Add definition-management MCP actions and every applicable IPC/HTTP/WS mapping assertion. Existing desktop/browser controls use the same backend validation; publishing a critic-cap change leaves live pins unchanged.
- [ ] RED: `discard_draft_keeps_publication_and_live_pin` — catches destructive discard; `critic_cap_edit_publishes_without_script_change` — catches an inspector that only changes mock/local state. Existing all-Join runtime tests cover parallel semantics, not a new UI test of the same reducer.
- [ ] Evidence: actual rule edit/publish and live/replay inspection in browser/isolated desktop, matching sync-matrix documentation. WF18 drag palette, preset gallery and spatial authoring polish are explicitly outside the WF17 gate.

### WF18 — Optional drag palette and authoring conveniences

Problem: the approved mock shows convenient dragging and configurable presets; these are useful authoring improvements, not prerequisites for daemon delivery.

- [ ] Implement drag-item palette with accessible click/keyboard fallback, persisted spatial layout and named outcome-edge editing. Use six-dot handles, no checkbox-like node icons or misleading Modified/100% toolbar pair.
- [ ] Add Planner/Research/Prompt draft/Mockup/Implement/Validate presets and For each/Splitter convenience controls on the existing configurable contracts. Presets cannot introduce provider defaults that bypass global policy.
- [ ] RED: `drag_palette_item_adds_at_canvas_coordinates` — catches faux drag interactions; `preset_edit_keeps_published_profile_authority` — catches presets silently overriding approved model/tool constraints.
- [ ] Evidence: focused browser/isolated desktop screenshots and keyboard/mobile interaction tests. Delivery parity may complete before this story; no capability is silently dropped, because essential authoring remains WF11 and this is a separate approved follow-up.

### WF12 — Show live runs, evidence, incidents and cost coverage

Problem: replacing invisible scripts is not sufficient if nested delivery, waits, partial merges and missing usage remain opaque.

- [ ] Render portfolio and expanded delivery activations from durable cursors: actual artifact/policy/version, branch state, queues, questions, rejects/repairs, partial integration, obligations, incidents, evidence/owned session links and recovery actions. Keep stale/disconnected/unknown separate from terminal outcomes.
- [ ] Attribute provider tokens/money by run/node/attempt and nested child once, including failed attempts, advisors and legacy coordinator lane during comparison. Preserve Claude audit import; add generic provider/harness usage where available. Report settled/estimated/unknown and rate/currency source; subscription unknown is not zero. Machine seconds are separate.
- [ ] Retain question/report acknowledgment, bilingual persistent digests, local-asset accessibility and policy-compliant notifications. AI Chat reads snapshots and cursor deltas, never reconstructs authoritative state from chat.
- [ ] RED: `reconnect_replay_matches_live_branch_state` — catches lost cursor state; `nested_usage_is_counted_once` — catches double costs; `missing_usage_cannot_display_zero_cost_parity` — catches false cheapness.
- [ ] Evidence: replay/live snapshot equality, cost fixtures with nested work and missing usage, editor/run screenshots and operator tasks without shell archaeology.

### WF13 — Ship the editable PR-review standard workflow with typed GitHub effects

Problem: `automations/pr_reviews.rs` already admits review-only runs from polled PR snapshots, excludes drafts, uses no-host-tool ACP and posts through `post_pinned_review` on the exact head. Extract these contracts into typed nodes and ship an editable built-in workflow, not a second PR-review executor or a permanent static path.

- [ ] Deliver the ordinary editable/duplicable `pr-review` built-in definition and versioned trigger binding described in section 2.5. Extract `ReviewSnapshot::admit`, `execute` and `post_pinned_review` in `automations/pr_reviews.rs` into shared typed node/effect contracts; retain its subscription checks, exact-head posting and unknown-publication/no-retry safeguard. Preserve a review-only default; policy-required isolated proof activities are bound effects added to the same owning run, never a new competing PR watcher or publisher.
- [ ] Implement section 2.5 lossless PR settings migration, including COMMENT-only legacy policy, disabled state, definition/profile fields, run/event/publication history and a field-by-field receipt. Stage the new binding disabled; no second consumer or automatic permission expansion. Test idempotent reimport and rollback after an uncertain review POST.
- [ ] Add PR-subject intake keyed by repository, number, head/base SHAs and requirement/policy digest; snapshot local-trunk diff, commits, threads and CI. Page complete data and preserve a living PR map. Deduplicate redelivery; synchronize/force-push invalidates the previous subject.
- [ ] Keep current draft exclusion as a visible `deferred_draft` outcome in the PR map, with retrigger when polling observes ready-for-review. Evaluate code × proof × CI by section 5.2: code pass plus failed proof/required CI refuses approval; unknown proof/CI is incomplete, never success.
- [ ] Execute independent code review and targeted proof in isolation, combine by Judge, then post comment plus REQUEST_CHANGES or APPROVE under preapproved review-only policy. No human gate in that lane before merge. Disagreement/uncertainty receives one bounded automated advisor/recheck, then an explicit cannot-complete report and visible retry trigger; never guessed approval.
- [ ] Add typed GitHub comment/review effects with current-head fencing and lookup-only uncertain recovery. Preserve draft-PR publisher safeguards instead of reimplementing a less safe POST path. Handle account self-approval refusal as a visible provider limitation, not success.
- [ ] RED: `head_change_prevents_posting_stale_approval` — catches outdated review; `lost_review_response_does_not_post_duplicate` — catches duplicate requests/comments; `review_without_checks_never_approves` — catches code-verdict/proof conflation; `draft_pr_defers_until_ready_poll` — catches reviewing/publishing an excluded draft; `code_pass_proof_or_ci_fail_refuses_approval` — catches optimistic code-only APPROVE.
- [ ] Evidence: recorded real GitHub payload fixtures, adapter tests, and isolated authorized trial repository PRs (not Boss's live PRs). Merge stays a separate human decision, with no merge API invoked by this subflow.

### WF14 — Ship editable label-polled issue-triage and delivery standard workflows

Problem: `automations/event_intake.rs` already polls labels and deduplicates issue/label events; `automations/triage.rs` already implements Accepted/Rejected/NeedsInput with host-owned rejection comment/label publication and unknown-delivery protection. Extract these seams into typed nodes used by editable built-in templates, not a second issue classifier/publisher or permanent static dispatch path.

- [ ] Deliver ordinary editable/duplicable `issue-triage` and `issue-delivery` built-in definitions and the label-poll trigger binding from section 2.5. Extract current polling/admission, triage verdict and ordered host comment/label behavior into shared typed nodes. Designer edits to prompts, checks, verdict table, labels and repository subscriptions publish pinned revisions; backend safety floors still apply to duplicates.
- [ ] Implement section 2.5 lossless issue settings migration and receipt-preserving single-consumer handover/rollback. Preserve all triage policies, definition/workflow pins, disabled state and current NeedsInput no-write behavior; unsupported mappings block cutover rather than lose settings. RED: `automation_template_migration_preserves_settings_and_unknown_effects` — catches reimport resetting edits, dropping a policy field or replaying an uncertain comment/PR; `duplicate_template_cannot_bypass_publication_safety` — catches edits removing exact-head/proof/CI or repository authority checks.
- [ ] Pin requirements and issue content digest. Map sufficient → Accepted, insufficient → Rejected, unresolved → NeedsInput. Preserve current NeedsInput no-write behavior for existing policies; the new issue-delivery returned-for-information policy explicitly requires a question comment through that same host publisher. Reuse triage comment/label effects and lookup-only/no-repeat recovery; do not claim unknown publication was confirmed.
- [ ] Add an explicit returned-issue retrigger: poll an issue carrying the existing intake label and compare its current body/requirements digest with the saved returned digest. A changed digest creates one new admission generation; unchanged repeated polls create none. A label remove/re-add alone is not assumed to change the current issue/label event key. Re-read current label/digest before admission; withdrawn labels refuse. Capture the digest from actual polled/API body data, extending snapshots if required.
- [ ] Reuse the intake bridge's idempotent native story creation rather than a Create Stories node in a story graph. Story Dispatch pins the candidate-delivery definition once. Requirement edits invalidate affected admission/review/approval evidence.
- [ ] Draw and implement every terminal failure edge: critic/check repair exhaustion → evidence comment to reporter → returned-delivery-failed/no PR; definite push failure → comment/no PR; push uncertainty → lookup-only reconciliation, status comment and no PR creation until exact SHA confirmed; PR-creation uncertainty → lookup-only reconciliation plus status comment, no second PR create. Unknown is not a claim that no PR exists: a first POST may have succeeded. Comment uncertainty uses its own retained intent and never blindly reposts.
- [ ] After verified push and reconciled draft PR creation, Notify developer → revision-fenced Ask. Approve executes typed Tool `mark draft PR ready for review` on the same current PR/head, with policy/intent/receipt and no merge authority. Changes requested routes through bounded repair/review; reject retains work; expiry is not consent. A subsequent ready-action uncertainty reconciles provider state without replay.
- [ ] Unattended PR-creation repository allowlist for this plan: **empty (`[]`; no repositories authorized)**. Every other repository is rejected at publication, including this checkout until Boss explicitly names and approves its exact GitHub `owner/name`. Trial repositories also require named authority before live PR creation. WF14 activation cannot pass until that named list is recorded; do not invent repository permission from a local path or plan approval.
- [ ] RED: `insufficient_issue_comments_without_spawning_implementer` — catches wasted work; `changed_body_retriggers_returned_issue_once` — catches permanently stuck returned issues or duplicate polling starts; `issue_edit_invalidates_old_admission` — catches stale requirements; `repair_exhaustion_comments_and_creates_no_pr` — catches falling through to publication after failed delivery.
- [ ] RED: `push_failure_or_uncertainty_blocks_pr_and_records_comment` — catches creating a PR for an unverified branch; `uncertain_pr_creation_comments_without_second_post` — catches duplicate PRs while preserving may-exist state; `developer_approval_executes_ready_action_on_current_head` — catches an approval edge that only records prose; `unnamed_repository_rejects_unattended_pr_creation` — catches unapproved publication. Preserve `developer_approval_before_pr_exists_is_refused` and `lost_pr_create_response_keeps_publication_uncertain`.
- [ ] Evidence: captured polling/triage/provider fixtures for insufficient/amended/sufficient/exhausted/push-failed/push-unknown/PR-unknown/ready-action cases. Live trial only after a named sandbox repository is approved; no production authorization is implied.

### WF15 — Preserve release scope and publish only with explicit authority

Problem: story completion, proof debt, CI and publication are different conditions; release countdown must not erase them.

- [ ] Pin release/next/Boss-wait scope and repository-specific version rules, plugins-first ordering and ego private/public history separation. New work does not silently enter frozen release scope.
- [ ] Count each release separately, notify once at zero eligible remaining work, and require current proof plus exact-commit successful required CI jobs/platform assets. No combined cross-release total. Green CI is readiness, not publication consent.
- [ ] Gate branch/tag/package/release publication by explicit recorded authorization for exact artifact/action. Preserve frontend-debt pre-push gate, validated tag-only exception and denied-publication-material scan; absent required policy fails visibly. Wait for every required CI run and uploaded asset, with failure/timeout outcomes.
- [ ] RED: `zero_count_with_proof_debt_is_not_release_ready` — catches countdown false green; `tag_exception_cannot_bypass_unrelated_push_debt` — catches broadened exception; `ci_for_other_sha_cannot_authorize_release` — catches stale CI; `ready_without_consent_does_not_publish` — catches accidental publication.
- [ ] Evidence: separate-release snapshots, CI/platform failure fixtures and dry-run publication intents. Real publication only under a separately authorized rehearsal/release.

### WF16A — Effective policy and retained deterministic-tool continuity

Problem: policy imports and existing optional tools need explicit ownership without a monolithic maintenance rewrite.

- [ ] Audit effective global routing, MDKB/source-verification, handoff and phase-specific test-policy injection for each supported harness; source files alone are not installed health.
- [ ] Publish a bounded retained-tool registry: exact entry point, opt-in/manual or durable-Timer invocation, prerequisites, owner, output receipt and no-pump dependency. This is the preservation baseline for optional environment/upkeep rows; do not implement their optional new nodes to pass WF17.
- [ ] Retain no-pattern-kill/PATH, unowned-deferral and owned-tool-failure rules. Reconcile duplicates without changing the global routing authority.
- [ ] RED: `policy_installation_unknown_is_not_reported_healthy` — catches assuming a checked-in hook is active; `retained_tool_runs_without_coordinator_pump` — catches accidental dependence on retired scheduling.
- [ ] Close with effective-policy inventory, retained-tool receipts and row-level preservation references; no provisioning, process reaping or optional upkeep rewrite is needed.

### WF16B — Owned-process health, diagnosis and bounded recovery

Problem: health/reap/recovery safeguards must remain effective when the coordinator exits.

- [ ] Connect current health/guard tools to WF19 Timer and owned-process records; retain unknown-roster protections, scoped reaping, freeze/footprint/automation alarms, sidecar-heal retry bounds and ProcessGuard diagnostics availability.
- [ ] Keep native/browser/device isolation and scoped screenshots, stash metadata-only inventory and explicit authorization for dedicated dev restart. No restart of Boss's daily app or unsolicited cache purge.
- [ ] RED: `unknown_roster_refuses_process_reap` — catches killing live work; `unauthorized_daily_app_restart_is_not_maintenance_retry` — catches destructive recovery; `live_owner_blocks_stranded_cleanup` — catches unsafe housekeeping.
- [ ] Close with isolated owned-process/unknown-roster tests and read-only schedule/last-run audit. External diagnostics and private status line remain available retained tools, not mandatory new nodes.

### WF16C — Persistent reports, acknowledgment and memory

Problem: unanswered investigations, reports and durable lessons must not vanish with an AI session or expiry sweep.

- [ ] Use bounded report/memory templates for due story triage, source-stale/expired reports, bilingual digests, MDKB-first research, one-home lessons and bug cause/history/prevention evidence. Apply prior ticks before rewriting; retain unread answers and accessible image fallback.
- [ ] Connect existing report/triage tools to durable Timer/notification receipts, not a coordinator loop. Preserve markers, persistent paths, no Mission Control iframe and explicit unknown prerequisites.
- [ ] RED: `unread_report_survives_expiry_upkeep` — catches lost answers; `digest_rewrite_applies_existing_ticks` — catches erased decisions; `source_stale_report_is_not_current_evidence` — catches misleading progress reports.
- [ ] Close with report-store fixtures, memory reconciliation and effective scheduled-entry-point audit; a general-purpose new upkeep engine is not required.

### WF16D — Optional environment/provisioning maintenance nodes

Problem: typed convenience nodes can later improve explicit provisioning, VM, browser access and diagnosed cache/quarantine operations.

- [ ] Reuse existing setup/VM/browser/quarantine/cache tools with prerequisites, opt-in authority, retained logs and owned-resource cleanup; never install globally or read/print credentials automatically.
- [ ] RED: `provisioning_node_requires_named_authority` — catches unsolicited environment mutation; `cache_gc_keeps_live_reservation` — catches deletion of a running source/cache view.
- [ ] Close on authorized isolated fixtures for only the selected adapter. WF16A retained-tool contracts already preserve these capabilities for WF17; no environment-node rewrite is a retirement prerequisite.

### WF16E — Optional upkeep and reduction convenience subflows

Problem: root-budget upkeep, Test Janitor and stranded-work convenience nodes can be migrated incrementally after core replacement.

- [ ] Reuse existing root-upkeep/tidyup/test-janitor tools and declared output contracts. Preserve final checks, unread reports, WORK report-only scope, discard approval, AWS-only compatible mutation proof and no-production-write reduction scope.
- [ ] RED: `upkeep_never_discards_unapproved_work` — catches destructive convenience cleanup; `test_reduction_without_compatible_aws_evidence_refuses` — catches weakened deletion proof.
- [ ] Close only the selected optional subflow with its bounded fixture evidence. Existing opt-in tools remain callable through WF16A, so this story is explicitly outside the WF17 gate.

### WF19 — Persist Timer deadlines, cohort flush and bounded backoff

Problem: proof flushing, polling retry/backoff and question expiry cannot depend on a coordinator loop or the later maintenance bundle.

- [ ] Add durable Timer events with stable activation/effect keys, absolute due time, owner, purpose, bounded attempts/backoff and declared expiry outcome. Restart fires due work once; paused/canceled branches follow published scope rules, and expiry never implies approval.
- [ ] Supply WF08 nonempty-cohort flush, WF13 provider retry and WF03 question deadlines before those stories integrate. Use provider retry hints subject to approved caps; do not busy-poll.
- [ ] RED: `timer_reopen_fires_once_with_remaining_backoff` — catches lost/duplicate deadlines; `timer_expiry_never_grants_approval` — catches auto-consent; `single_row_cohort_flush_survives_restart` — catches indefinitely pending proof debt.
- [ ] Close with reducer/store reopen and owner-loss tests plus adapter timeout/cancel evidence. This small foundational story precedes WF08, unlike optional maintenance nodes.

### WF17 — Prove parity, reconcile active work and retire the old scheduler

Problem: a native happy path does not establish equivalent delivery or authorize dropping old operational safeguards.

- [ ] Execute section 7's paired real-story protocol, all three use cases and all required fault injections. Publish raw outcomes, proof, elapsed/queue time, cost coverage and intervention ledger, including failed iterations.
- [ ] Demonstrate a full operating window without coordinator PTY/AI Chat, plus restart/reconnect and an unanswered Ask while disjoint work completes. Reconcile every preservation row and explicit retirement item with evidence or Boss-approved exception. WF18 drag/presets and optional WF16D/WF16E native upkeep nodes are not gate prerequisites; validate their retained WF16A tool baseline instead.
- [ ] Keep WF07 disabled by default per canonical repository through this trial. The combined post-no-ff crash case must show one merge, recovered proof obligation and preserved unattributed workspace dirt before Boss enables that repository; WF08 remains a separate obligation/batch story.
- [ ] Drain old ownership, import/reconcile obligations and active effects with operator approval, switch one project at a time and prove rollback does not replay already merged/published effects. Old scripts remain read-only evidence until the rollback window closes.
- [ ] RED: `cutover_cannot_start_second_integration_writer` — catches dual schedulers; `rollback_does_not_replay_committed_external_effect` — catches double actions; `unresolved_legacy_debt_blocks_retirement` — catches stranded proof.
- [ ] Evidence: Boss-approved comparison dossier and row-level acceptance record. No deletion/retirement before approval; story creation is explicitly authorized, but it does not authorize implementation or operational activation. Include R7 per-subscription migration, settings comparison, single-consumer cutover and rollback evidence.

## 4. Preservation matrices

**K** = keep capability; implementation may replace its mechanism. **R** = retire a redundant mechanism after parity while retaining its purpose at the named destination. **D** = deliberate feature/behavior drop, requires explicit approval below. Story references are implementation/validation ownership, not claims that a current script is already native. Partial/optional source capabilities stay partial until prerequisites are verified.

### 4.1 Every coordinator capability

| ID | Capability from features report | Kept by node/subflow/policy; mechanism disposition | Story |
| --- | --- | --- | --- |
| A01 | Complete brief, resume, parent and workspace | K Author + daemon launch/profile/workspace contract | WF04 |
| A02 | Idle delivery/busy mail and stable name | K managed delivery effect + peer identity; payload receipt not idle inference | WF04 |
| A03 | Long command completion wake | K Run command async effect + cursor notification | WF05, WF12 |
| A04 | Atomic ownership/close/rename recovery | K lifecycle transaction + canonical workspace identity | WF04 |
| A05 | Checkout baseline attribution | K workspace baseline and named writer ownership | WF04, WF07 |
| A06 | Successor before close, protect dirt/test apps | K close transaction and owned-process gate | WF04 |
| A07 | Continue without false delivery claim | K continuation intent and separate delivered receipt | WF04 |
| A08 | Idle child housekeeping with protections | K maintenance close policy; R shell idle sweeper as scheduler | WF04, WF16B |
| A09 | Live/missing/stalled/background inventory | K run incident/attempt/session projection; unknown inventory preserved | WF12 |
| A10 | Ego conversation and stop recovery | K attempt conversation/stop references + resume inspector | WF04, WF12 |
| A11 | One or three independent advisors | K bounded Author Fork/all-Join consultation under global routing | WF02, WF06 |
| A12 | Claude usage/time audit | K usage adapter retains JSONL audit, generic coverage added | WF12 |
| E01 | Batch mail/results with delivery progress | K durable intake/disposition and cursors; R coordinator pump | WF10 |
| E02 | Alternative singleton watcher | K same durable event subscription; R second shell watcher implementation | WF10 |
| E03 | Unfinished-work refresh/changed alarms | K portfolio projection + timer notifications; R refresher loop | WF10, WF16A |
| E04 | Owners/orphans/holds/landings/publication debt | K portfolio open-work index and bounded/full views | WF10, WF12 |
| E05 | Bounded story/ownership collection | K story-store queries + bounded legacy inventory adapter | WF10 |
| E06 | Parked queues, dirty trunks, stashes, repo keys | K canonical-key portfolio inventory/holds | WF10, WF16B |
| E07 | Reports remain open until disposition | K durable report lifecycle/successor transaction | WF04, WF10 |
| E08 | Boss prompt/report spool and classification | K durable intake → story/run/dismissal relation | WF10 |
| E09 | Unseen result/restart watermarks | K cursor replay + legacy result import; R filesystem scan authority | WF10, WF17 |
| E10 | Idle episode scoped nudge, exclude background | K incident policy with episode/generation key | WF04, WF12 |
| E11 | Staffing capacity and unowned eligible work | K resource/dependency admission + ownerless alarm | WF05, WF10 |
| E12 | Locked maintenance sweep and gate tick | K timer maintenance Calls and resource locks | WF08, WF16A |
| E13 | Restricted mechanical follow-up worker | K optional scoped Author template with escalation receipt | WF04, WF16A |
| E14 | Ready wave without integrator | K portfolio integration-queue alarm | WF07, WF10 |
| E15 | Ownerless work and false Boss-only waits | K owner/hold classification Judge + reason evidence | WF10 |
| S01 | Prevent duplicate unmerged story assignment | K claim/branch/workspace admission invariant | WF04, WF10 |
| S02 | Due review and evidence-based story triage | K Timer → bounded classifier Author → Judge | WF16C |
| S03 | Boss requests still unbuilt, child progress | K intake-to-nondoc-artifact lineage projection | WF10, WF12 |
| S04 | Criteria, bug proof and commit close preflight | K evidence Gate and story transition adapter | WF06 |
| S05 | Sequential stories, combined gate, one failure owner | K sequential delivery Call mode and pinned batch artifact | WF08, WF09, WF10 |
| S06 | Release/next/Boss waits distinct | K portfolio release scope policy, never automatic publish promise | WF10, WF15 |
| S07 | External PR review/local trunk/living map | K PR review subflow and GitHub effect adapter | WF13 |
| Q01 | Independent bounded critic brief | K Author template from actual pinned story/artifact + review counter | WF04, WF06 |
| Q02 | Fixed FIX/STORY/ACCEPT rule | K ordered deterministic triage policy | WF06 |
| Q03 | Unused older critic template | R obsolete alternate text, active policy/template retained; approval item R5 | WF01, WF06 |
| Q04 | Pinned Rust batch and completed suites | K proof batch Run commands on rb | WF05, WF08 |
| Q05 | Source-bound tested ledger, no historical reuse claim | K typed check history; legacy rows imported as historical only | WF06, WF08 |
| Q06 | Landing obligations/bounded dispatch/recovery | K proof obligation store/cohort scheduler | WF08 |
| Q07 | Atomic mutation artifacts/query/export/context | K registry port + durable artifact references/backup | WF08 |
| Q08 | Complete-universe mutation skip decision | K deterministic mutation reuse Gate | WF08 |
| Q09 | Current remote source/environment measurement | K read-only rb measurement Tool, installed state verified separately | WF05, WF08 |
| Q10 | Exact requested mutant scope/identity | K requested-universe Tool and identity schema | WF08 |
| Q11 | Test Janitor behavior-preserving reduction | K opt-in reduction subflow, no production writes, AWS-only mutation proof | WF08, WF16A, WF16E |
| Q12 | Complete-run dominance comparison | K retained evidence comparison Tool, never auto-delete tests | WF08 |
| L01 | Serialized landing/dead lock/precheck | K Integrate canonical resource + recovery | WF07 |
| L02 | Merge-format-types-notices-compile/debt/deps | K Integrate admission/merge/refresh/obligation phases | WF05, WF07, WF08 |
| L03 | Pinned full frontend batch, clear only success | K local JS cohort Run command and exact membership discharge | WF08 |
| L04 | Proven integration/ownership before cleanup | K lifecycle cleanup Gate + TUIC worktree adapter | WF07 |
| L05 | Trunk discovery/rename/shared identity | K canonical repository/workspace adapter | WF04, WF07 |
| L06 | Safe merged-worktree sweep | K Timer maintenance cleanup Call with live-owner protection | WF07, WF16B |
| L07 | Stranded committed/dirty work and intentional holds | K portfolio inventory and hold evidence | WF10, WF16B |
| L08 | Rescue/triage/review stranded personal work | K maintenance Author/Judge → normal delivery; Ask for discard, WORK report-only | WF07, WF16A, WF16E |
| L09 | Exact-tip branch archive and deletion preparation | K typed archive Tool + explicit deletion decision, no guard bypass | WF07, WF16A |
| L10 | Append-only documentation conflict union | K narrow merge-union Tool; malformed/code conflicts refuse | WF07 |
| L11 | Clean initialized submodule advance/attribution | K typed submodule Tool with baseline exact-entry attribution | WF04, WF07 |
| L12 | Wait all GitHub CI for full SHA | K CI observation Tool/Timer with success/failure/timeout | WF13, WF15 |
| L13 | Authoritative CI and validated tag-only evidence | K CI evidence Gate and exact tag-only publication policy | WF15 |
| L14 | Platform CI, version/order/private-public release rules | K pinned repository release subflow + separate approval | WF15 |
| B01 | Memory/load/disk/concurrency/remote build admission | K resource adapter initially reusing build-slot/rb | WF05 |
| B02 | Machine freeze reason/allow-list | K durable admission freeze policy, existing bridge retained | WF05 |
| B03 | Changed-file formatting only | K positioned format Tool, no unrelated module/vendor formatting | WF05, WF07 |
| B04 | Diagnosed Tauri metadata quarantine, no deletion | K explicit maintenance recovery Tool, stop owning builds first | WF05, WF16A, WF16D |
| B05 | Isolated app-up signal separate from launcher | K live-check Tool with readiness and owned launcher receipts | WF05, WF16B |
| B06 | Ego wrapper smoke/capture/transcript | K authorized browser access-check Tool and retained evidence | WF16A, WF16D |
| B07 | Protect daily app/screen/device isolation | K native live-check policy/resource keys/scoped screenshots | WF05, WF16B |
| R01 | Pinned rb transfer/command/evidence | K rb execution port preserves manifests and results | WF05 |
| R02 | Fair remote classes/runtime/client-loss/source detection | K rb admission/runner adapter; verify installed version | WF05 |
| R03 | Durable remote gates/list/cancel/collect | K remote job handle and reconciliation port | WF05, WF08 |
| R04 | Env/stdin/bounded SSH/replica generation protection | K transport/lease/fingerprint port, no secret receipts | WF05 |
| R05 | Safe remote caches/replicas and locked local snapshots | K explicit maintenance GC Tool preserving live reservations | WF05, WF16A, WF16D |
| R06 | Build user/queue/tools and opt-in cache experiment | K explicit provisioning Tool; optional experiment remains opt-in | WF16A, WF16D |
| R07 | Token-scoped headless remote Claude | K optional authorized executor template; missing token visible blocked | WF04, WF05 |
| R08 | Isolated Linux VM/agent install/marker scenarios | K authorized VM compatibility maintenance subflow; credentials never printed | WF16A, WF16D |
| C01 | Open decisions, apply ticks, recommend consequences | K durable inbox/digest Ask policy | WF03, WF12 |
| C02 | Investigation answers retained until acknowledged | K unread report/answer acknowledgment projection | WF12, WF16C |
| C03 | Separate release counts, waits/next, once-at-zero | K release countdown projection; D cross-release total, D2 below | WF15 |
| C04 | Scope complete plus recorded CI release alarm | K release readiness Gate + deduplicated Notify, no publish | WF15 |
| C05 | Source-stale progress reports regenerate | K Timer/source-change → scoped report Author | WF16C |
| C06 | Expired/missing-expiry report candidates | K upkeep inventory, unread-protection Gate | WF16C |
| C07 | Accessible digest images | K retained embedding Tool until native assets proven equivalent; no assumed retirement | WF12, WF16C |
| C08 | Markers/short chat/bilingual persistent long reports | K communication adapter and pinned presentation policy, no Mission Control iframe | WF12, WF16C |
| C09 | Evidence-based cause/history/prevention/tests reply | K investigation/report Author output contract + evidence links | WF06, WF16C |
| H01 | Identity/restart briefing of work/builds/results/budgets | K daemon resume snapshot and cursor replay; R coordinator-context reconstruction | WF12, WF16A |
| H02 | TUIC responsiveness/freeze/footprint/slow health | K health observation Tool/Timer, report actual install/last run | WF16B |
| H03 | Bounded sidecar exec/signature heal | K diagnosed recovery Tool, never app rebuild/replacement | WF16B |
| H04 | Explicitly authorized dedicated dev restart | K typed owned-PID restart Tool with approval, never daily-app auto-retry | WF16B |
| H05 | Owned orphan process reap with protected roster | K maintenance process registry/reap policy; unknown roster refuses | WF04, WF16B |
| H06 | Reap/memory/failed-automation alarms | K Timer guard observations + deduplicated Notify | WF16B |
| H07 | Private Claude status-line utility | K retained optional external utility, not authoritative workflow telemetry | WF16A |
| H08 | Old stash metadata only | K metadata-only maintenance inventory; never auto-drop stash | WF16B |
| H09 | ProcessGuard load/thermal/network diagnosis | K optional external diagnostics Tool, availability explicit | WF16A |
| M01 | MDKB first, source verification, durable lessons | K research/memory policy and scoped tools, not run-state storage | WF04, WF16C |
| M02 | Root ownership/budgets/triage compaction/gate view | K upkeep Tool and final deterministic check | WF16A, WF16E |
| M03 | Consolidate lessons/reports/memory/nightly corrections | K maintenance subflow with unread/expiry protections and final Gate | WF16A, WF16E |
| M04 | One global routing/delegation/handoff authority | K authoritative policy import; R competing local routing tables | WF01, WF04 |
| M05 | Fix failed owned tools at owner, one lesson home | K failure ownership/escalation policy + memory reconciliation | WF16A |
| G01 | Stop guard for unowned work/waves/staffing | K ownerless/wave alarms and admission; D forced staffing floor as turn-stop prerequisite, D1 | WF10, WF17 |
| G02 | MDKB-before-research/recursive-search protection | K effective harness policy/tool injection audit across clients | WF04, WF16A |
| G03 | No invisible pump/no blocking interactive question | K daemon ledger visibility and branch-local inbox; R pump-shell guard after pump removal | WF03, WF10 |
| G04 | No pattern-wide kills/PATH-clobber protection | K owned-process and executor-environment invariants plus effective hook audit | WF05, WF16A |
| G05 | No new unowned deferrals, retain valid history | K proposal/hold owner validation and source commit guard; infra UNKNOWN explicit | WF06, WF16A |
| G06 | No publication with frontend debt; tag exception | K proof-debt publication Gate with exact validated exception | WF08, WF15 |
| G07 | Denied publication content scan, preserve repo hooks | K publication preflight Tool/policy, missing denylist refuses | WF15 |

### 4.2 Every origin screenshot feature

These rows enumerate the features of “What the screenshot shows”, not speculative features of an unavailable application. A configurable preset is not a distinct hard-coded engine kind.

| ID | Screenshot feature | Destination/disposition | Story |
| --- | --- | --- | --- |
| F01 | Brief input | K Start input schema + immutable requirement snapshot | WF01, WF10 |
| F02 | Three parallel Research nodes, each width 1 | K Fork of three configurable Authors, per-node concurrency 1 | WF02, WF04 |
| F03 | Compare using claude | K independent Author/decision with globally allowed exact profile, no fixed vendor | WF04, WF06 |
| F04 | Check catalog shell command | K Run command with explicit shell only when published as argv | WF05 |
| F05 | Approve catalog Gate without PTY | K deterministic policy Gate + asynchronous approval task where required | WF03, WF06 |
| F06 | rejects 0/2 | K persisted visible reject counter separate from total-review/repair counters | WF06, WF11, WF12 |
| F07 | Apply catalog shell command | K mutating command effect with intent/reconciliation, no blind retry | WF05 |
| F08 | Done terminal | K End only after required current receipts/obligations | WF06, WF08 |
| F09 | Dashed fail/reject edges | K typed explicit outcomes and canvas edge styling | WF02, WF11 |
| F10 | Apply refused → Pause | K named refusal branch and explicit resume target/reconciliation | WF05, WF11 |
| F11 | Author family: Planner | K editable Author preset | WF04, WF11, WF18 |
| F12 | Author: Research | K editable Author preset | WF04, WF11, WF18 |
| F13 | Author: Prompt draft | K editable Author preset with artifact output schema | WF04, WF11, WF18 |
| F14 | Author: For each | K bounded collection expansion into correlated Author activations | WF02, WF11, WF18 |
| F15 | Author: Mockup | K configurable Author preset, declared artifact/native evidence policy | WF04, WF11, WF18 |
| F16 | Per-node role | K Author inspector and pinned role | WF04, WF11 |
| F17 | Per-node prompt | K template/editor/final launch receipt distinction | WF04, WF11 |
| F18 | Per-node model | K exact profile/model/effort checked against global authority | WF04, WF11 |
| F19 | Per-node tools | K declared capabilities and write/authority scope | WF04, WF11 |
| F20 | Build: Implement | K Author implementer preset | WF04, WF11, WF18 |
| F21 | Build: Run command | K typed command node | WF05, WF11 |
| F22 | Build: Tool | K typed adapter/action node, same effect protocol | WF05, WF11 |
| F23 | Check: Validate | K configurable validator Author plus deterministic checks as applicable | WF04, WF06, WF11, WF18 |
| F24 | Check: Judge | K current Judge extended with typed subject/evidence/uncertainty | WF06, WF11 |
| F25 | Review pack | D explicitly excluded by Boss; not a replacement requirement | WF01 |
| F26 | Validate pack | D explicitly excluded by Boss; not a replacement requirement | WF01 |
| F27 | Route: If/else | K deterministic typed predicate/outcome node, no arbitrary frontend expression engine | WF02, WF11 |
| F28 | Route: Splitter | K bounded Fork/collection routing preset | WF02, WF11, WF18 |
| F29 | Route: Call | K pinned subflow with expansion | WF09, WF11 |
| F30 | Route: Loop | K existing bounded Loop retained, counters distinct | WF06, WF11 |
| F31 | Route: Notify | K durable existing Notify extended by delivery adapters | WF12, WF16C |
| F32 | Hold: Ask | K branch-local structured inbox question | WF03, WF11 |
| F33 | Hold: Gate | K policy/approval without blocking unrelated branches | WF03, WF06, WF11 |
| F34 | Hold: Pause | K explicit pause scope and pinned resume target | WF03, WF11 |
| F35 | Hold: Timer | K durable deadline/wake effect, restart-safe and no consent on expiry | WF19, WF03 |
| F36 | Draft edited while published v3 runs | K immutable active/transitive pins, concurrent draft revision | WF01, WF09, WF11 |
| F37 | Run 104 paused | K inspect/resume explicit paused run independently of editor | WF03, WF12 |
| F38 | Publish action | K validated expected-revision publication, already exists at core | WF01, WF11 |
| F39 | Discard draft action | K discard draft changes only, preserve publications and runs | WF11 |
| F40 | Header token counter | K usage projection with coverage/unknown disclosure | WF12 |
| F41 | Header cost counter | K estimated/settled amount and rate source, not fabricated money | WF12 |
| F42 | Run queue | K durable ready/admitted/blocked/running view with resource reasons | WF05, WF10, WF12 |

### 4.3 Explicit drop and retirement decisions

**Proposed capability/behavior drops (complete list):**

- **D1, G01:** remove forced minimum active-agent headcount as a prerequisite for a coordinator/AI ending its turn. A session cannot be required to remain alive to make progress in a daemon-owned design; arbitrary staffing also conflicts with global delegation and actual resources. Preserve ownerless-work alarms, ready-wave integration alarms, optional utilization targets and fair admission. Requires Boss acceptance at plan approval.
- **D2, C03:** remove the cross-release `all:` total from countdown presentation. Preserve every per-release count, release-work/Boss-wait/next partition and once-at-zero notification. The feature report records that the aggregate contradicts Boss's existing presentation rule; no underlying work is hidden.
- **D3, F25/F26:** do not implement Review pack or Validate pack. This exclusion is already explicit in Boss's screenshot requirements; individual configurable review/validation nodes remain.
- **D4, mock chrome:** remove “Modified” and the unrelated “100%” readout from the static canvas toolbar, rather than implying a percent-complete edit state. Published/draft revisions and Publish enabled state remain. This is a requested mock correction, not removal of dirty-state conflict checks from the real editor.

**Review decision:** Fable, Astra and Grok accepted D1–D4 and R1–R6; Boss approved the revised plan on 2026-10-30. R7 is the later Boss-requested standard-template migration, not a claim of another advisor review. R1–R7 apply only after WF17 parity approval by Boss. During cutover exactly one consumer owns each inbox/intake and exactly one writer owns each canonical integration lane; never run old and new consumers in parallel.

**Mechanism retirements, not capability drops, conditional on WF17 proof and approval:**

- **R1:** pump/mail watcher/open-work refresher/Stop-hook scheduling and result-file watermarks as authority → durable intake/events/cursors and portfolio obligations. Do not run a second consumer for the same inbox during cutover.
- **R2:** permanent coordinator PTY and resume-from-context as execution authority → daemon run ownership, bounded Author decisions and durable recovery.
- **R3:** children/gate/tested TSV/JSON files and shell lock conventions as state authority → native stores, typed historical imports and canonical locks. Keep raw artifacts for audit; never discard unresolved legacy rows.
- **R4:** shell sequences/generated landing commands as the transition engine → typed Integrate/command/effect phases. Useful deterministic tools remain adapters.
- **R5, Q03:** consolidate the unused older critic brief and contradictory README signatures into the approved active template/policy. Preserve its unique valid requirements during reconciliation; confirm external users before deleting a file. No review capability is dropped.
- **R6, M04:** retire competing repository model tables and stale test-rule copies as executable authority → the global routing policy plus explicit phase-specific repository gates. Keep historical decisions as history, not conflicting defaults.
- **R7, WF13/WF14:** retire fixed PR-review, issue-triage and label-polled intake orchestration/dispatch in `automations/pr_reviews.rs`, `triage.rs` and `event_intake.rs` → editable built-in standard workflows on the shared engine using their extracted typed nodes. Keep reusable GitHub/poller adapters, safety contracts and audit history. Require lossless settings/run-intent migration, per-subscription parity and Boss approval at WF17. Drain/fence the old consumer before enabling the new binding; never two consumers or publishers for one subscription. Rollback reconciles receipts before restoring old authority.

No other coordinator capability is proposed for removal. Optional status lines, image embedding, VM provisioning, external diagnostics and Test Janitor remain available; lack of current prerequisites is a visible limit, not a reason to drop them.

## 5. Three end-to-end canvas flows

### 5.1 Full orchestration with no coordinator session

#### Parent canvas: `portfolio-delivery@N` (plan graph)

```text
Manual/Event/Timer Start(requirement revision, repository policy)
  → Intake Tool(deduplicate, retain original request)
  → Author(Classify/plan bounded task; not scheduler or mandatory Coordinator role)
  → Judge(scope sufficient, authorized, dependencies known)
      no        → Ask(requirements/authority; only this item waits)
      uncertain → bounded advisor Call → Judge → Ask if still reserved/unknown
      yes       → Create Stories(typed daemon proposal in this plan graph)
                → Story Dispatch(pinned delivery definition; sole story fan-out)
                    owns eligibility/claims/dependencies; invokes each story once
                    children finish integrated/proof-pending or explicit blocked/failure
                → Dispatch completed / blocked outcome (native aggregation, NOT Join-all)
                → Proof wait(current required cohort IDs; sole waiter per cohort)
                → Judge(scope closure, integrations and proof all current)
                    no        → bounded repair/replan intake
                    uncertain → Ask(exact unresolved evidence)
                    yes       → readiness/countdown projection → Notify → End

Independent daemon service (not downstream of Story Dispatch completion):
Integration outbox → WF08 cohort collector → WF19 Timer/explicit flush(any nonempty set)
  → pinned batch commands → durable proof receipts → wake parent Proof wait
```

Story Dispatch is the existing story-level Call. WF09 pins its definition/policies; do not add a second Call per story or a parent all-Join over story receipts. The WF08 cohort service starts as soon as an integration obligation is committed, regardless of whether Dispatch has completed. Exactly one durable parent proof-wait activation owns each cohort subscription, registered atomically by cohort ID; if other portfolio scopes share the cohort, they read its durable proof-state predicate and receive cursor hints, never create a second waiter. Child nodes contain a read-only cohort reference but never wait for it. Cohort execution ownership belongs to the WF08 service, not to a story child. A timer flushes a single-row cohort, so neither a twelve-row capacity nor unfinished siblings can prevent proof progress. The parent may only reach its proof-wait activation after dispatch aggregation, but the cohort is already running or complete and the receipt persists.

Release readiness is a projection, not a Call from End that grants publication. WF15 owns separate explicit artifact-bound release authorization, successful exact-commit CI/platform assets and repository policy. Countdown keeps release, next and Boss waits separate while other repositories progress.

#### First child canvas: `delivery@N` (A's chain; story graph)

```text
Start(story + criteria revision + workspace baseline)
  → Author Implement(isolated worktree; outside fork)
  → daemon capture clean artifact → Fork generation g(review, deterministic checks)
       review: Author Independent critic → Judge(ordered findings)
          FIX/no   → supersede entire g; cancel/fence old checks and observe stop
                   → Loop(total-review cap) → Implement → new artifact
                   → new Fork generation g+1 (rerun BOTH critic and checks)
          STORY    → bounded intake proposal; risk-policy outcome
          uncertain/exhausted → Ask(reserved decision, never check override)
          yes      → current review arrival for g
       checks: Run command(authorized phase proof) → current check arrival for g
  → Join all(current g only; old join terminates superseded on repair)
  → Gate(current independent reviewer + checks)
  → Integrate(policy flag + expected base + merge checks + canonical lock)
       moved base/conflict → scoped repair/recheck through fresh fork generation
       uncertain          → Pause repository integration lane → reconcile
       merged             → obligation outbox + dependency refresh + cleanup phases
  → End(integrated, proof pending; durable obligation/cohort reference)
```

The owning daemon, not the critic or model, controls cancellation, generation replacement and both-branch restart. Old check/review receipts remain historical but cannot satisfy the new join. Stop proof gates conflicting effects; a crash mid-repair replays the same supersession/replacement intent. The review budget persists across all generations. A failed check returns through declared repair policy or explicit blocked/exhausted state; it cannot fall through to Gate pass. The parent alone decides delivered/release-ready after proof, so child End is not falsely called full delivery completion. Sequential batch mode keeps one combined critic over the complete batch artifact under its pinned policy.

**What runs where:** daemon owns intake/claims/counters, current-generation joins, integration outbox, cohort execution and snapshots. Authors use registered isolated worktrees. Rust/build/mutation/CRAP run on rb (AWS for the Test Janitor-specific policy); JS/TS runs locally; Mac-only proof needs an authorized isolated matching instance. WF05 reuses these typed adapters; generalized cross-daemon/Fabbrica placement is deferred. Canonical integration has one owner and remains disabled per repository until WF17 approval.

**Human boundaries:** reserved product/destructive/publication choices and exhausted critic acceptance use durable Ask. Routine authorized staffing/review/integration/proof needs no conversation. Disjoint stories continue during Ask, subject only to dependencies/shared resource uncertainty. There is no coordinator PTY or mandatory Coordinator Agent node for Create Stories; WF04 replaces that predicate with a typed proposal. The plan-only Create Stories restriction remains intentional.

**Failure/evidence trail:** request/requirements → idempotent native stories → claims/baseline/launch contracts → typed reports and generation supersession/cancellation → RED/consumer checks/current independent review → merge-result admission → one no-ff integration → durable obligation outbox → independent cohort/Timer/batch proof → parent verification/countdown → cleanup disposition. Crash after merge cannot replay merge, lose proof debt or discard unattributed workspace work. Partial cleanup/refresh stays visibly incomplete. Unknown inventory and dead/idle workers never complete a node.

**Existing/missing/story gap:** existing drafts/runtime/Create Stories/Story Dispatch/serial policy and integration certification are reused. WF04 removes PTY and Coordinator-role prerequisites; WF02/WF03 add safe fork repairs/local Ask; WF05/WF06 add positioned proof/evidence policy; WF07 performs flagged actual integration; WF08/WF19 own independent cohort flush; WF09 pins existing dispatch; WF10 aggregates portfolio delivery without a second story dispatcher; WF11/WF12 provide essential controls/visibility; WF15/WF16A–WF16C preserve release/policy/health/report continuity. WF18 and optional upkeep-native-node stories are not in this critical path.

### 5.2 Incoming PR fully reviewed without a human before merge

#### Canvas: `pr-review@N`

```text
Polled PR snapshot / explicit Start(repository, PR number, delivery key)
  → Tool Snapshot(PR head/base, local trunk, diff, requirements, threads, CI)
  → If/else(duplicate/current/closed/draft policy)
       duplicate → existing receipt; closed → terminal not-applicable
       draft     → End(deferred_draft, no review POST; retrigger on ready-for-review poll)
       admitted  → Fork
         Author Independent code review(read-only candidate worktree)
         Run command targeted proof(local JS/rb Rust/native as required)
         Tool CI/dependency/publication-policy observations
  → Join all(exact PR subject)
  → Judge(code × proof × required CI table below; authority checked separately)
       yes       → Tool Re-read current head/base
                 → Tool Post findings + APPROVE → Notify developer → End(reviewed)
       no        → Tool Re-read current head/base
                 → Tool Post findings + REQUEST_CHANGES → Notify → End(changes requested)
       uncertain → bounded independent advisor/recheck Call → Judge
                 → on exhaustion Tool Post cannot-complete evidence/comment
                   + Notify + End(review incomplete; retry on new evidence/event)
```

**Reuse and verdict oracle:** the migrated run starts through the standard workflow and typed contracts extracted from the existing `automations/pr_reviews.rs` snapshot/admission/execution path. Extend `post_pinned_review` rather than add another watcher/review publisher; current draft exclusion becomes the visible deferred outcome above. Its existing review-only/no-PTY default remains; any targeted proof work is an explicitly bound effect of the same run. Preserve exact-head posting and unknown-publication/no-retry semantics.

| Code verdict | Targeted proof | Required CI | Automated outcome |
| --- | --- | --- | --- |
| Pass | Pass | Pass (or policy explicitly says not required) | APPROVE after current-head/base/policy probe |
| Pass | Fail | Any | Refuse approval; REQUEST_CHANGES with attributable proof failure |
| Pass | Pass | Fail | Refuse approval; REQUEST_CHANGES with exact required-CI failure |
| Fail/actionable findings | Any, including unknown | Any, including unknown | REQUEST_CHANGES for actionable code findings; disclose proof/CI failures or gaps separately |
| Pass or uncertain | Unknown/unavailable | Any | Incomplete, bounded recheck then cannot-complete comment; never APPROVE |
| Pass or uncertain | Any | Unknown/pending required CI | Timer/backoff then incomplete on budget exhaustion; never APPROVE |
| Uncertain/conflicting | Pass | Pass | Bounded advisor/recheck, then explicit incomplete if unresolved |
| Any | Any | Any, but draft/closed/superseded | Deferred/not-applicable/superseded; no current review POST |

A failed command caused by infrastructure is unknown/unavailable proof, not a fabricated code defect. `review_without_checks_never_approves` and `code_pass_proof_or_ci_fail_refuses_approval` protect the refusal cases. Review permission remains separate from proof and merge authority.

This lane contains **no human Ask/Gate before the merge decision**. It is activated only after Boss approves a review/comment/request-changes/approve policy for selected repositories. “No human” does not mean “always approve”: ambiguous, unavailable proof or account self-review limitations yield an explicit incomplete outcome and retry trigger, not an approval. Deterministic failures request changes with concrete evidence; missing infrastructure gets an incomplete report rather than false blame on code.

**Merge boundary:** a separate `merge-decision` task belongs to the developer. Review automation neither merges nor invokes a merge endpoint. If later authorized, a human approval must bind the latest PR head/base and current checks before a distinct Integrate/merge action. An approval of an old head is not reusable.

**Placement:** owning daemon accepts/deduplicates events and stores effects; code inspection runs in an isolated PR candidate clone/worktree against fetched local trunk, not Boss's active checkout. Commands use matching local/rb/native targets. GitHub API calls use the configured authenticated adapter. Review authors do not receive authority to merge or mutate the candidate. CI is observed by exact SHA, complete pagination and required jobs, not optimistic labels.

**Failure paths:** synchronize/force-push/base update invalidates the whole review subject and cancels/fences old pending publication; a new keyed review starts. Before each external post, compare head/base; a concurrent change after the probe is recorded as a stale review if detected, and follow-up reconciliation posts supersession rather than claiming current approval. GitHub APIs may not offer atomic head/base compare-and-post for every comment type: WF13 must prove the provider-specific precondition or expose this residual race and prevent stale review from becoming merge authority. Rate limit/auth/network failures enter bounded backoff/Timer with provider hints. A lost POST reply invokes lookup; absence/ambiguous lookup stays uncertain, never blind resend. Repeated PR snapshot polls return the original effect/run; no webhook entry point is assumed. No test result for old head/trunk can satisfy the new subject.

**Evidence:** event key and captured PR payload → local trunk/head/base refs and diff digest → code findings and reviewer identity → exact command/test/native/CI receipts → Judge policy/version → submitted GitHub action body/head and returned review/comment IDs → notification/delivery receipt → living PR map. Preserve all incomplete and superseded reviews; never rewrite history into current proof.

**Existing/missing/story gap:** PR-intake skill supplies review/local-trunk/living-map requirements (S07). TUIC already has autonomous review-only PR runs in `automations/pr_reviews.rs`, exact-head `post_pinned_review`, native review/checks and the reviewed draft-PR publisher. WF13 extracts that existing PR path into typed nodes and an editable built-in workflow with proof/CI verdicts, typed subject/effect receipts and explicit draft/incomplete outcomes; it must not create a competing PR-review service. Add generic PR subject/review evidence (WF06), reusable target/check effects (WF05), review/comment/request-changes intent/reconciliation and triggers (WF13), bounded advisor calls (WF02/WF09) and operator map/cost (WF12). No human dependency is added to mask missing automation.

### 5.3 Issue lifecycle: requirements return or PR plus developer approval

#### Canvas: `issue-delivery@N` (existing label-polled intake plus pinned candidate workflow)

```text
Versioned label-poll trigger binding / explicit Start(repository, issue, policy)
  → Tool Snapshot(current intake label, body/requirements digest, reporter)
  → Typed triage evaluator → Judge Accepted / Rejected / NeedsInput
       Rejected  → existing triage rejection comment + label effect
                 → End(returned for information; open issue, no implementer/PR)
       NeedsInput→ bounded read-only assessment
                 → required information comment under pinned returned-info policy
                 → End(returned/unresolved; no PR, no global Ask)
       Accepted  → existing intake bridge creates one native story (no story Create Stories node)
                 → Story Dispatch once, pinned delivery-candidate definition
                    Implement → critic/checks → Judge/repair
                      exhausted/failed → Tool Comment evidence to reporter
                                       → End(delivery failed/returned, no push/PR)
                      current proof   → Gate unattended-publication policy
                        not named/authorized → End(publication rejected, no push/PR)
                        authorized → Tool Push exact reviewed candidate commit
                          definite failure → Tool Comment push failure → End(no PR)
                          uncertain → lookup-only push reconciliation + Tool Status comment
                            unresolved → End(push unknown, NO PR create)
                            exact SHA confirmed → continue
                          confirmed → Tool Create/reconcile reviewed draft PR
                            definite refusal → Tool Comment publication failure → End(no PR)
                            uncertain → lookup-only PR reconciliation + Tool Status comment
                              unresolved → End(PR may exist; NO second create, no approval Ask)
                              found/current → continue
                            confirmed/current → Notify developer(PR + proof)
                              → Ask(PR/head/requirements/check digests)
                                approve → Tool Mark draft PR ready for review
                                  confirmed → Notify ready → End(ready, not merged)
                                  uncertain → reconcile ready state → End(ready unknown if unresolved)
                                  refused → Notify refusal → End(blocked ready action)
                                changes → bounded repair/review → update PR → fresh Ask
                                  exhausted → Tool Comment failure → End(changes unresolved)
                                reject → End(rejected; retained work)
                                expiry → pending/expired per policy, never consent
Every status/comment unknown → retain its own intent + lookup-only reconciliation;
  never repost blindly and never report confirmed delivery from uncertainty.
```

**Exact existing seams:** `automations/event_intake.rs` polls labelled issues and currently deduplicates issue/label keys. `automations/triage.rs` already provides Accepted/Rejected/NeedsInput, confirmed rejection comment then rejection label, and stops publication advancement on Unknown. Current NeedsInput writes nothing: the required requested-information comment for this use case is a deliberate WF14 extension under its pinned returned-info policy, without changing other existing policies. `automations/publish.rs` already checks reviewed native approval, pushed SHA and saved publication intent with lookup-only recovery. Extract these services as typed nodes in the editable issue templates; replace their static orchestration after R7 parity rather than keep a second issue-triage/intake/publisher path.

**Returned-issue retrigger:** on the next successful intake-label poll, fetch/retain the current body plus requirement-policy digest. If it differs from the last returned digest and the intake label remains present, admit exactly one new generation. Repeated unchanged polls do nothing. Extend the current event/triage key to include the admitted digest/generation, with migration for old keys; merely removing/re-adding the same label does not by itself defeat today's durable key. If the intake label was removed, the reporter must restore it and change the relevant body/requirements before the new generation is eligible. Link all previous rejection/question/comment receipts. Labels and digest are re-probed before admission.

**Repository authorization (complete named list):** `unattended_pr_creation_repositories = []` — **none** are currently authorized, including the TUIC checkout. This plan cannot infer a GitHub owner/name or Boss consent from a filesystem path. Therefore the publication Gate rejects every production unattended PR request until Boss explicitly records exact `owner/name` repositories and allowed actions; live trial repositories also need named approval. This is a closed activation prerequisite, not an invitation to use any registered repo. No rejection of this review requirement: the evidence supports an empty/default-none allowlist (`event_intake.rs:1–2`), not authority to invent a named production list.

**Candidate and failure semantics:** the candidate uses the same engine but does not Integrate into canonical trunk before creating a PR. Failed implementation/proof exhausts to an evidence comment/no PR, not an inherited interactive Ask or a falsely passing draft. A definitive push refusal creates no PR. Uncertain push prohibits PR creation until exact SHA is observed. Uncertain PR creation prohibits another POST but cannot truthfully assert no PR exists; status comment and outcome explicitly say may-exist. Definite refusal/no PR and unknown/may-exist are distinct. Retain the candidate workspace and proof while external uncertainty is reconciled.

**Developer approve executes an effect:** approval is requested only after the existing draft PR is confirmed, bound to current requirements/head/checks. Approve reaches the typed `mark draft PR ready for review` Tool with exact PR identity, re-read head, allowed policy, intent and receipt; it is not a prose-only authorization. Reject/expiry never merge or discard. Changes requested rerun bounded repair/review, supersede old approvals and return to a fresh Ask. Ready-action uncertainty is reconciled against provider state. Actual merge and release remain separate explicit authorization; no auto-close issue.

**What runs where/evidence:** daemon stores polled snapshots/digest admissions and effect generations; existing triage/publisher own authenticated GitHub calls. Authors/checks use registered isolated workspaces and local/rb/native policy. Preserve checklist citations, triage decision/comment/label IDs, native story/proposal, generation supersessions, clean candidate/review/check receipts, verified remote SHA, PR intent/URL, developer answer and ready-action receipt or every retained failure/unknown. Disjoint work continues during approval waits.

**Existing/missing/story gap:** WF14 ships editable standard workflows using typed nodes extracted from current label polling, triage and candidate publisher for digest retrigger, explicit delivery/push/PR-failure branches, status comments and ready action. WF13 extends the shared host-owned GitHub effect seam; WF04 removes Coordinator-node/PTY predicates for plan creation while this issue path keeps using the existing intake bridge; WF03 supplies branch-local approval; WF06/WF09 pin candidate evidence/definition; WF11/WF12 provide essential controls/visibility. Production activation is blocked until the empty repository list is replaced by explicit Boss-approved names.

## 6. Current native reuse and gap register

| Current verified seam | Reuse | Addition and owner |
| --- | --- | --- |
| Mutable expected-revision drafts, immutable publication, pinned run graph | Keep service/store boundaries | Transitive Call/policy pins, Discard UX, richer inspector; WF01/09/11 |
| Start/Agent/Judge/Gate/Pause/Loop/exclusive Join/Notify/End | Keep serial replay and current policy | Fork/all-Join/predicates/collection expansion/Timer and branch tokens; WF02/03/19 |
| Create Stories plan-only guard (`definition.rs:105`), Coordinator-node predicate (`definition.rs:305–321`; `run/runtime/effects.rs:37–53`), and Story Dispatch | Keep plan/story distinction, proposal keys and single story fan-out | Remove Coordinator-role/report dependency via typed daemon proposal; issue path reuses intake creation; WF04/10 |
| RunStore intent/effect keys, sequence and generation fences, uncertainty recovery | Extend, do not bypass | All external effects and subflow identity, cross-store outboxes; WF01/05/07/09/13 |
| Bound managed reports and worktree assignment, current coordinator PTY launch requirement | Keep report provenance | Daemon authority + complete workspace lifecycle/ACP boundary; WF04 |
| requiredChecks direct argv, source-bound receipts, independent native review | Keep freshness and process cancellation | Positioned Run command, proof vocabulary, target resources, PR subject; WF05/06 |
| record_integration and recertify_canonical | Keep receipt/ancestry/freshness checks | Actual merge transaction, preflight, outbox, partial phases and cleanup; WF07 |
| Current NeedsInput pauses run; answer then resume | Preserve historical semantics | New branch-local event-contract semantics/inbox; WF03 |
| Desktop/headless AppState runtime | Keep one owner lock and service | Explicit node execution target port; runtime on headless does not already mean cross-machine placement; WF05 |
| HTTP/IPC definition actions; MCP run start/read/events/commands | Keep common services | Definition MCP, new action parity/WS cursor hints and canvas; WF11 |
| Run history/incidents | Keep durable report/event evidence | Nested live graph/portfolio, queue/recovery/partial state and usage; WF12 |
| `automations/event_intake.rs` label polling, issue/label admission deduplication and native story creation bridge | Extract typed poll/admission nodes and trigger bindings, no assumed webhooks or duplicate story creation | Editable issue templates, lossless settings migration, digest-generation retrigger and R7 cutover; WF14 |
| `automations/triage.rs` Accepted/Rejected/NeedsInput, rejection comment + label, Unknown protection | Extract host-owned triage nodes, keep NeedsInput no-write in migrated policy | Editable triage template, returned-info policy, failure comments, typed receipts and reconciliation; WF14 |
| `automations/pr_reviews.rs` review-only polled PR runs, draft exclusion and exact-head `post_pinned_review` | Extract typed review/publication nodes with no-retry-on-Unknown, no second review pipeline | Editable PR-review template, COMMENT-only settings migration, proof/CI verdicts and R7 cutover; WF13 |
| Event intake reviewed draft-PR publisher (`automations/publish.rs`) | Keep current approval/pushed-SHA and lookup-only recovery | Explicit push/PR-failure outcomes and post-PR developer ready action; WF14 |
| Existing GitHub approve implementation | Reuse authenticated provider primitive | Workflow-owned request-changes/comments, preconditions and effect reconciliation; WF13 |

## 7. Validation, parity and cutover

### 7.1 Contract/gate selection

Before implementation of each story, record observable effect, real authoritative state, external assumptions/fixtures and blast radius. Existing tests are reused when they protect the named bug. Capture external PR/issue/CI/executor fixtures from the real system with secrets removed, never hand-invent provider behavior. Features use contract-first tests; a claimed defect requires its targeted RED on the pinned pre-fix commit. Test changes state why the old expectation is wrong.

The integration owner runs final impact-selected validation once per approved batch. Rust tests/builds/clippy use the brief's rb build-slot path, matching native exception only when required; JS/TS checks run locally through build-slot and repository temp wrappers. Preserve the repository's IPC/HTTP route parity gate, transport mapper fixtures, relevant workflow reducer/service tests, targeted Vitest and style/control/D&D checks. Mutations and broader suites are scheduled by the integrator, not each child. No full application suite is required for this documentation-only plan change.

### 7.2 Paired real-story trial

1. Before running either lane, Boss selects approved real stories: small pre-fix-RED fix, TS consumer change, Rust shared contract, dependent pair, disjoint pair, merge-conflict case and mutation-survivor batch. Snapshot actual IDs/criteria/revisions; historical story numbers are not invented eligible work.
2. Pin equivalent requirement snapshots, base commit/tree, dependencies, model/profile/effort, policy, fixtures, target/toolchain and resource budgets. Use **isolated clones**, one trial canonical ref per lane, fresh equivalent agent context and no production main. Disable real pushes/releases/publication in both lanes; external use-case trials get an explicitly authorized sandbox repository or recorded fixture replay.
3. Run the coordinator lane and native lane independently on the same inputs; neither reviewer sees the other lane's output. Alternate lane order to reduce cache bias. Compare accepted behavior/proof against the same independent criterion oracle, not patch bytes. Retain all failed/aborted/repaired trials in time and cost totals.
4. Execute the complete chain through integration, proof batches, local frontend/remote Rust, strict clippy, mutation/CRAP/coverage where policy requires, story/plan closure and cleanup. Record matched test scope/count and omissions. No current proof may be replaced by weaker evidence merely to get parity.
5. Start with one small pair, then disjoint/dependent and the full case set. Add complete PR-review and issue-return/amendment/PR-approval rehearsals. Run the native lane with no coordinator session; terminate an idle AI Chat decision client and show deterministic work continues.
6. Agree observation window and metric normalization/tolerance **before** results. Default decision is at least equal on every required dimension, not an average that hides a proof regression. When slower/more costly, diagnose queue versus engine work, change/publish policy or implementation, rerun affected pairs and invalidated scenarios; retain previous iterations.

| Dimension | Raw records for both lanes | Acceptance oracle |
| --- | --- | --- |
| Outcome | Criteria, source/canonical commits, dependency release, scope drift, cleanup | Same requirements met, no unauthorized action, data loss, false completion or stale dependency |
| Proof | RED pre-fix, final checks, reviewer identity, triage, merge tree, native target, all obligations/universe/reuse/omissions | At least all current proof retained, current and independently attributable; unknown/stale never green |
| Time | Wall, agent active, executor queue, Boss wait, recovery, duplicate work | Native no worse under the preagreed matched comparison; publish raw and queue-normalized pairs |
| Cost | Provider tokens/money including failed attempts/coordinator/advisors/nested work, rate source, subscriptions, machine seconds | Native no worse on measured dimensions; missing coverage blocks cost-parity claim, never becomes zero |
| Interventions | Every decision/manual script/restart/merge/repair/cleanup with reason/minutes | Native no more; none merely to reconstruct scheduler state or unblock unrelated branches |
| Operator control | Find pending Ask/rule/artifact/proof/cost/queue/recovery without shell, edit cap and publish | Real editor and live/replay agree across desktop/browser/AI reconnect; running revision stays pinned |

### 7.3 Fault campaign and expected invariants

Inject faults only into isolated owned processes, clones and approved trial API resources. Use controlled failpoints/transport fixtures, not load generation on Boss's Mac.

| Fault | Required observation |
| --- | --- |
| Crash immediately before/after effect intent/receipt | Exactly one effect or explicit uncertainty; no invented success or automatic repeat |
| Lost spawn response/binding and dead/idle child | One retained child/effect; typed missing/stalled outcome; idle is not completion |
| Duplicate report/answer/webhook/result file | Original receipt returned, one successor, report disposition retained |
| Repaint/old draft/native modal during notification | No injected work mistaken for delivery; managed delivery receipts remain authoritative |
| Old reviewer/check result after artifact or requirements edit | Historical evidence retained, current Gate refuses it |
| Trunk moves during merge checks | Old admission invalidated, synthetic merge rerun before integration |
| Crash after no-ff merge before debt/refresh/cleanup with unattributed dirt present | One integration, recovered obligation outbox, independent incomplete phases, workspace retained; per-repository integration flag blocks unapproved activation |
| New landing after proof cohort snapshot | New debt remains; old batch cannot clear it |
| Empty test filter/partial mutation universe/changed toolchain | Explicit failed or incompatible evidence, no green-by-absence |
| Unanswered Boss/developer Ask | This branch waits, disjoint story completes; no timeout consent |
| Build freeze/resource starvation | Queued reason/fair ordering visible, no bypass or false failed test |
| Remote disconnect/lease expiry/late result | Fenced old result, retained reservation until stop/reconcile; no duplicate writer |
| GitHub POST succeeded but response lost/rate limit/auth error | Lookup-only recovery or unknown, bounded backoff; no duplicate PR/comment/review |
| PR force-push/base change/issue edit while approval pending | Old subject invalid, no reuse for current review/developer authorization |
| Self-approval disallowed by provider | Incomplete/unsupported outcome with evidence, never fabricated APPROVE |
| Missing roster/registry backup/schedule installation data | Unknown visible; no unsafe reap, evidence-reuse claim or healthy-install claim |
| Two runtimes/cutover contenders for one canonical repo | One writer/admission owner; second waits/refuses without mutation |
| Rollback after native merge/publication | Existing receipt reconciled; old lane cannot replay committed action |

### 7.4 Retirement/rollback gates

For each matrix row, record actual story/commit, targeted contract test, operational parity evidence and Boss-approved exception if any. WF17 cannot close while a core preservation row is merely “covered by design”, a proof obligation exists only in legacy files, usage coverage is insufficient for claimed cost parity, or a proposed drop remains unapproved.

Optional WF18 palette/preset improvements and WF16D/WF16E native upkeep nodes are not WF17 completion gates. Their original capabilities must remain available through the verified WF16A retained-tool baseline; do not retire those tools merely because their new nodes are deferred. This is preserved functionality with a smaller migration, not a silent drop.

Run a read-only active-work inventory: claims, branches/workspaces, reports without disposition, queued/running gates, registry snapshots, publication intents, unread decisions and live processes. Import by explicit IDs and receipts; uncertain old actions remain uncertain. Take restorable store/registry snapshots. Drain old pump/watchers and landing writers before enabling native intake for one pilot repository. Record one active inbox/intake consumer and one canonical writer; R1–R7 remain inactive until WF17 parity approval. Keep the per-repository WF07 integration flag default false through the isolated-clone trial and enable production only after that repository receives Boss approval. Never enable two schedulers for the same story/repository. Disable the old mechanism only after the row's invariant is demonstrated and Boss approves D1/D2 and R1–R7.

Rollback stops new native dispatch, preserves evidence and reconciles active/uncertain effects and proof debt before old ownership resumes. Do not automatically rerun a native failed graph through shell scripts. Keep read-only old scripts, source-policy snapshots and raw trial evidence through the agreed rollback window. Report the exact remaining unknowns rather than calling retirement complete.

## 8. Risks and concrete worst cases

These are design bounds/test scenarios, not measured production timings or new resource defaults.

| Risk | Concrete worst-case analysis | Mitigation / approval boundary |
| --- | --- | --- |
| Token explosion | Eight admitted stories × three parallel activities × two total review generations = 48 activation-generations before any nested calls; an unbounded For each would multiply without limit | Publish bounded fan-out/recursion/total attempt budgets; resource cap applies across nested calls, not separately per child |
| Off-by-one critic cap | A cap interpreted as two retries produces three reviews instead of two, a 50% excess in reviewer invocations before advisors | Persist total reviews and one default repair separately, test exhaustion/restart |
| Batching deadlock | A cohort max of twelve rows with only one landed story can wait forever if a child waits for proof that a parent starts after aggregation | Story Dispatch children end integrated/proof-pending; outbox independently starts WF08 cohort; WF19 flushes any nonempty cohort; exactly one parent proof waiter, no portfolio Join-all |
| Blind merge replay | Two post-crash merge attempts can alter history twice or consume the next base even when the first merge succeeded | Intent, canonical lock, inspect both parents/result before reconciliation; never retry uncertainty |
| Post-land lost debt | Crash between one merge and one obligation insert could label an untested artifact release-ready | One transaction if colocated, otherwise durable integration outbox plus release gate on unresolved outbox |
| Branch Ask becomes global stop | One unanswered decision stalls all eight admitted stories, not just its dependents | Branch tokens and shared-scope pause semantics; prove disjoint completion |
| Remote partition overlap | One uncertain job plus one replacement doubles execution and may create two writers even if old results are fenced | Stop proof/quarantine/reservations before retry; one canonical owner, fencing alone is insufficient |
| Cost double count | One provider session recorded on parent and child makes its money/tokens appear twice; one missing subscription reported zero falsely wins parity | Stable usage identity, settled/estimate/unknown coverage, independent machine time |
| Stale external review | PR changes after inspection but before POST; an old approval could be visible on new code depending on provider rules | Exact-head provider semantics, immediate re-probe/reconcile, supersession and separate current merge gate; record unclosable API race explicitly |
| Provider self-review limitation | Automation account authored PR and cannot approve it; no amount of retries changes that | Approved reviewer account/prerequisite or explicit incomplete state; never human gate inside automated review lane |
| Destructive upkeep | A missing roster interpreted as empty could reap every live worker; an expiry sweep could erase unread answers | Unknown fails closed for destruction; explicit owner/acknowledgment and archive proofs |
| Legacy policy drift | One old template authorizes broad tests or different routing while new graph claims current policy | WF01 resolves source conflicts, pins authority and audits effective harness injection; no automatic weaker fallback |
| Overbroad migration | Trying to rewrite all 122 operational scripts before the first delivery delays the useful engine and risks untested provisioning | Port state/transition semantics first; retain deterministic tools behind typed ports, validate optional prerequisites separately |
| Weak parity conclusion | One faster happy path masks a failed proof case or missing provider costs | Per-pair independent oracle, all failures retained, no aggregate compensation, explicit unknowns block relevant claim |

## 9. Mock corrections and documentation-only validation record

The local static canvas mock now uses a six-dot drag handle instead of the square checkbox-like icon, makes palette items draggable onto the SVG canvas at the drop coordinate, and retains click-to-add as an accessible fallback. The “Modified”/“100%” toolbar pair is removed; draft/publication revisions and Publish state remain. The live mock does not accept palette drops. Tool-permission checkboxes in the Author inspector remain checkboxes because those controls really select permissions, not nodes.

These are static mock behaviors only: no workflow API, process, repository or provider effect is invoked. The approved sibling report copy was read as evidence and not silently overwritten; the coordinator can refresh that report asset copy from this committed mock when publishing the review pack. This plan does not claim the product already has the editor or any missing node.

Validation for this change is limited to source/matrix/link checks, JavaScript syntax, a focused mock interaction/render capture and the changed-file formatting helper. The static matrix check passed: exactly the source report's 105 IDs, each once; 42 screenshot-feature rows; 17 defined/referenced story contracts in the original commit; valid local Markdown links and balanced fences. The advisor revision expands/splits the contracts; its final static count is recorded in the handoff, not inferred from that earlier check. `node --check docs/mocks/orchestrator-workflow/mock.js` passed. The instruction-link checker reported both instruction pairs valid. A bounded headless Chrome CDP check passed: palette drag added exactly one node at the requested canvas position (within one SVG unit), the drag handle replaced the square icon, removed toolbar text stayed absent, and live palette items remained disabled/non-draggable. Editor/live screenshots were captured at 1400×900 under `~/Gits/.tmp/wfplan-evidence/editor.png` and `run.png`; these are render captures and DOM/interaction evidence, not a claim of human visual approval. The owned browser was stopped after capture.

The advisor revision changes this Markdown plan only. Its static validation passed: 105 coordinator IDs mapped exactly once, 42 screenshot rows, 23 defined/scheduled story contracts, an acyclic dependency graph, required WF13/WF08/WF10 prerequisite edges, optional-node exclusions from WF17, existing named automation seams, valid local links and balanced fences. The original static mock remains unchanged, so its prior syntax/interaction evidence is not rerun or relabeled as a new product test.

Production Rust/JS suites, rb checks, live GitHub actions and parity trials are intentionally not run for this docs-only revision. Main-store stories are created from these approved contracts without starting them; no criterion is checked and story-close is not applicable to this documentation assignment. The final handoff records the formatting-helper result and commit; future RED criteria above are not represented as passing tests.

## 10. Advisor-change resolution record

1. **Reuse:** WF13/WF14 and section 6 extract typed contracts from `pr_reviews.rs`, `triage.rs` and label-polled `event_intake.rs` into editable standard workflows; no second review/triage path. Boss's later template request supersedes keeping static orchestration permanently.
2. **Portfolio deadlock:** sections 2.3/5.1 use Story Dispatch only, no parent story Join-all, independently triggered cohort execution and one parent proof waiter; WF08 names the single-row Timer regression.
3. **Repair generations:** section 2.3, WF02 and 5.1 assign whole-generation supersession/cancel/stop/restart of both branches to the daemon; old join terminates superseded.
4. **PR verdicts:** WF13/5.2 include deferred drafts and code × proof × CI table; code-pass/proof-fail or required-CI-fail refuses approval, unknown never approves.
5. **Issue endpoints:** WF14/5.3 draw/test exhaustion, push/PR/comment uncertainty, no-create/may-exist distinction, digest-poll retrigger and approve → ready Tool. The exact authorized repository set is empty; all unnamed requests reject until Boss supplies named authority.
6. **Create Stories:** WF04 names/removes Coordinator-role validation/effect lookup, not just the PTY requirement; WF10 is a plan graph, WF14 reuses intake creation, and the plan-only graph boundary stays deliberately intact.
7. **Dependencies:** table adds WF02/WF09 to WF13, WF08 to complete WF10, and foundational WF19 durable Timer before flush/backoff consumers.
8. **Scope:** WF16 split into WF16A–WF16E, WF18 moves palette/presets off critical path, WF17 excludes optional native upkeep nodes while verifying retained capabilities, WF09 reuses existing story dispatch and limits new pinning/Calls to one daemon; cross-daemon implementation is deferred at section 2.1.
9. **Integration risk:** WF07 remains separate from WF08, preserves outbox/cleanup phases, adds default-off per-repository flag through WF17 and combined no-ff crash/debt/unattributed-work regression.
10. **Retirement:** section 4.3 records unanimous advisor acceptance of D1–D4/R1–R6 and Boss's later requested R7. R1–R7 execute only after WF17 parity/Boss approval with exactly one consumer/writer during cutover.

## 11. Boss-requested template revision (2026-10-30)

Boss approved the plan and the empty unattended-PR allow-list, to be revisited at WF17 with the exact trial repository. Section 2.5 records why the current static automations can become editable built-in workflows, the typed reuse boundary, customization/safety split and lossless migration. WF13/WF14 deliver those templates; WF11 supplies essential editing/duplication, so WF13 now depends on WF11. R7 places static-path retirement under the same WF17/Boss and one-consumer gates. This revision changes documentation and creates pending stories only; it does not start implementation or authorize provider effects.

Final template-revision static validation covers source capability-ID equality/uniqueness, all screenshot rows, one defined/scheduled contract per WF key, dependency references and cycle detection, local Markdown links and balanced fences. Main-store story creation copies every unchecked contract criterion and records these same dependencies; all stories remain pending. No Rust/JS production suite or live provider action is part of this revision.
