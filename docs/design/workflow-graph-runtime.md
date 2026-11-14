# Native workflow graph runtime

**Status:** Approved staged design; slices A/B and the serial slice C executor are implemented. **Date:** 2026-10-03.
**Scope:** Execute published Story delivery and Resolve plan graphs in the owning Rust daemon. Slice A introduces no scheduler or agent effects; executable delivery remains unavailable.

## Consumer contract

Starting a published workflow creates one durable execution that advances along its published edges. The daemon starts agents, consumes typed reports, evaluates decision policies and records every node activation and selected edge. Closing the frontend does not stop execution. Reopening Run history or reading through MCP returns the same ordered history.

A positive story verdict requires independent approval and current artifact evidence. Delivery to a dependent also requires verified integration into the canonical branch. A failed review returns through the bounded repair loop. Missing or conflicting evidence pauses. Cancellation and restart must not replay an uncertain spawn. An agent's exit or idle state never means success.

This proposal preserves the existing `human` closure policy as explicit approval by an independent reviewer or operator. It does not enable the currently rejected `automatic` closure policy. The scheduler records and applies approval evidence; it is not an independent approving actor.

## Verified starting point

See [workflow contracts](../backend/workflows.md), the definition store/validator, run model/store/reducer, prompt renderer, managed MCP launch/report handlers, Designer, StoriesDialog and related tests.

| Existing seam | Current behavior | Design change |
|---|---|---|
| `workflows/store.rs`, `definition.rs` | Immutable published revisions; graph and outcome validation; pinned checks and story template | Pin runtime semantics and node settings; validate executable graphs and pause/join contracts |
| `RunStore::start_plan` | Writes Started with empty attempts; one active run per plan | Atomically create initial graph activation and durable scheduling work; add idempotent start key |
| `RunCommand::StartPlanAgent`, `StartAttempt` | Select Agent IDs; no predecessor traversal | Reuse attempt creation behind an internal eligible-activation check; reject skipping predecessors |
| `AdvanceLoop` | Increments only the run-wide counter | Address a Loop activation; enforce its pinned node cap as well as the global budget |
| RunStore/reducer | Atomic events/projections, sequences, payload-bound command IDs, effects, recovery | Add graph execution/activation projections and events; keep replay as the authority |
| MCP `workflow action=launch` | Requires a managed caller, existing attempt and registered worktree | Extract a shared launch service; allow a verified daemon execution context without impersonating a PTY |
| MCP `workflow action=report` | Live bound actor, generation and story-revision fences | Reuse verification; publish scheduler work after the report commits |
| Story transition journal | Manual and managed approval provenance; claim-based self-approval guard | Add durable workflow role/reservation authorization and idempotent workflow transition receipts |
| Check/integration service | Checks currently require accepted story revision; verified non-fast-forward merge receipts | Add pre-approval checks against an evidence subject; retain post-integration rechecks and ancestry verification |
| `workflow-run-changed` | Desktop/SSE cursor hint; paged event replay | Retain hint; include new node and decision events in the same history |
| StoriesDialog/Designer | Manual work, history, input/resume, editable/published definitions | Start workflow and cancel/recovery controls; executable node settings; clients never schedule |

At this baseline the seeded Story delivery has seven nodes and eight links. `repair` is a Loop returning to `implement`, not a second repair agent. Resolve plan has eight nodes and ten links. Counts are observations, not invariants to pin in tests. The validation matrix below includes concurrent runs on different plans.

## Ownership and scheduling

One `WorkflowRuntime` belongs to `AppState` on the owning host. Both desktop startup and `run_remote` start the same Rust service after stores and the managed-agent adapter are ready. A desktop process owns its local runs; a connected remote daemon owns its remote runs. No WebView, browser component, model coordinator or polling frontend chooses the next node.

Use an asynchronous actor per active run, supervised by one runtime registry. Serialize commands to that run through the existing SQLite expected-sequence transaction. An in-memory mailbox is only a wake mechanism; durable eligible activations and effect intents are the queue. Startup scans active graph runs once. Report, decision, timer, dependency and effect commits wake the actor. A lost wake is repaired by reading persisted pending work on recovery, not by replaying the external effect.

Acquire a configuration-database execution-owner lock before recovery or scheduling. A second process pointing at the same config database can serve reads, but cannot reconcile or execute the first owner's runs. If owner-lock acquisition fails, start requests show an explicit executor-unavailable error. Recovery occurs after acquiring ownership, not on the first arbitrary HTTP read. This prevents a second process from interrupting healthy attempts through today's lazy `RunStore::open` reconciliation.

A scheduling turn:

1. Read the pinned definition, current run sequence, graph activations and authoritative story/evidence revisions.
2. Reject terminal, paused, expired or unowned executions. Apply the scheduler's bounded deterministic policy to eligible activations.
3. In one immediate transaction, consume predecessor arrivals, record the activation/edge decision and reserve any side-effect intent. Use a stable command key derived from execution, activation and transition, with its payload hash.
4. Commit and emit the cursor hint. Run reserved external work outside database transactions and outside the per-run actor's blocking path.
5. Commit the result against the effect/activation identity, then wake the run. Refresh after a sequence race; do not repeat the external operation to recover a stale sequence.

Cap deterministic activations per turn (proposed 32), then yield. A per-project execution limit and existing run spawn/story/concurrency budgets bound work. External effects use a bounded pool; slow checks and spawns do not block other run actors or hold SQLite locks. A run waiting for input, approval, checks or integration has no busy polling loop. Persist deadlines and arm runtime timers; duration expiry must pause even when no caller sends a command.

## Durable graph state and traversal

Add graph state to the existing run database, not a separate workflow authority. A plan run contains its plan graph execution and linked story graph executions; a direct story run contains one story graph execution. Reuse `StoryExecution` as the delivery/evidence projection. The root run owns shared budgets, reservations and history.

Add an explicit root target (`plan` or `story`) and target ID; do not reuse plan_id or the current attempt.story_id == plan_id convention to disguise a direct story as a coordinator. A direct story retains its native plan relationship for dependency checks, while the run targets that story. Each graph execution pins definition ID/revision and runtime contract version. Each activation contains execution ID, stable activation ID, node ID, incoming token IDs, generation, loop-epoch vector, state, optional attempt/effect IDs, deadline and result. States distinguish ready, running, waiting_input, waiting_approval, waiting_integration, succeeded, failed, interrupted and cancelled. Overall run status remains running/paused/completed/cancelled; show the waiting reason separately.

A token records the exact source activation, published edge and destination. Ordinary nodes consume one arrival and produce the one permitted unlabeled successor. Judge/Gate/Loop/Story Dispatch consume one arrival and select exactly one labeled edge. Start produces the first token once. End consumes a terminal token. A node may activate again in another loop epoch; node ID alone is not an attempt identity.

Events include graph start, node activation/completion, selected edge, decision (actor and evidence references), loop repeat/exhaustion, fork scope if enabled, join arrival/release, pause reason/resolution, deadline expiry and graph completion. Keep existing attempt/effect events and reference them rather than duplicating reports or PTY output. A unique `(execution, source_activation, edge)` arrival key prevents duplicate traversal. Replay must reconstruct eligible nodes and joins, not just their labels.

`StartAttempt` and `StartPlanAgent` become implementation primitives used by the scheduler. An operator or managed coordinator cannot start `review` before the token reaches it, start another implementation outside a repair transition, or use an arbitrary node to bypass the cap. A diagnostic/manual drive API must obey the same eligibility check; do not preserve an unrestricted alternate path.

### Runtime versions and existing persisted runs

Bump the persisted schema version and add explicit runtime-contract fields and event migrations. Published definitions retain their original bytes. New starts can use the approved runtime interpretation of an old valid graph; store that interpretation in the run. Published settings changed by this proposal require a new definition revision.

Legacy command-driven runs lack graph positions. Preserve read/replay/cancel/receipt access; do not guess positions from their attempt list, silently start agents, or manufacture approval evidence. Proposed default is to refuse graph resume of a legacy run and offer cancellation followed by a new start after reservations are released. Older binaries must refuse a newer database rather than corrupt it. There is no parallel legacy graph executor.

## Node semantics

All settings below are proposals pinned at publication. Optional evaluator settings never permit direct provider calls from the TUIC backend.

| Node | Entry, result and successor | Failures and waits |
|---|---|---|
| Start | Root start commits one activation and one outgoing token | Duplicate start key returns original run; a different payload fails |
| Agent: Planner/Coordinator | Render pinned plan context; reserve spawn; bind typed attempt; record proposals and planning closure against the current plan fingerprint | Invalid proposal refuses without advancing; missing report/exit pauses; session replacement retains logical execution identity |
| Agent: Implementer | Assign/reuse isolated story worktree and durable reservation; spawn with exact criteria, file scope and prior review feedback; consume typed current report | Failed report pauses for retry/repair policy; no successor from exit/idle; never approves its own output |
| Agent: Reviewer | Separate session evaluates the immutable artifact subject and returns structured findings plus approve/changes_requested | Approved with findings, stale subject or own implementation identity is rejected; valid findings feed Judge |
| Agent: Validator | Separate validation role reports evidence; backend runs pinned deterministic checks against the exact artifact subject | Model claims do not replace command receipts; failed check is durable evidence |
| Judge | Deterministic policy maps evidence to yes/no/uncertain and selects one matching published edge | Missing/conflicting/stale evidence yields uncertain with reason; no model-only score can authorize yes |
| Gate | Execute pinned check IDs, or wait for the configured approval/integration predicate; then select pass/fail | A known failed check follows fail; absent/unavailable prerequisites wait or pause visibly, not vacuous pass |
| Pause | Persist run-level reason and origin activation; retain unresolved question/effects and pause execution | Resume requires resolution and explicit target; cancel fences all work; never blindly restart a spawn |
| Loop | Increment that node's repeat count for the graph execution and enclosing epoch; follow repeat within cap, exhausted otherwise | Global loop/spawn/time limits still apply; exhaustion records both cap and observed count |
| Notify | Commit a notification intent with stable activation key; deliver a durable repository-scoped notice; follow successor after its receipt | Persist failed delivery; retry uses same key; native transient delivery is best effort |
| Join | Consume arrivals within a declared merge/fork scope and release exactly once under its pinned join mode | Never wait for an edge not activated by the selected branch; failed required branch pauses rather than silently disappearing |
| Create Stories | Consume validated coordinator proposals from the current plan revision; use existing stable proposal keys and bounded effects | No unbounded agent-generated creation; uncertain cross-database result requires reconciliation |
| Story Dispatch | Select dependency-eligible stories with disjoint scopes; create linked story executions; await approved integration of the wave | completed only after all dispatched children have current receipts; blocked routes to the published blocked edge |
| End | Complete this graph only when its contract's terminal gates hold | Story graph waits for integration after approval; plan graph cannot finish with open planning, blocked children or stale verification |

### Judge, approval and artifact evidence

Story Judge runs on the exact criteria/content revision and backend-observed `(ref, commit, tree)` of the assigned clean worktree. Capture the subject before review and validate it again before consuming the decision. Reviewer findings, deterministic checks and approval all name that subject. Changed source or requirements invalidate the decision and return to review/validation; do not accept an earlier approval by translating its label onto new code.

Recommended rules:

- **yes:** independent bound reviewer explicitly approves the current subject, no blocking findings remain, all required backend checks pass, and policy permits that actor's approval. Commit the native story approval with the reviewer's real session provenance and linked report. An operator approval is recorded as operator/local_api according to the existing identity contract, never forged as a reviewer.
- **no:** a current reviewer requests changes, or a required check proves a failure. Store the exact findings/failed receipts, select no and pass the feedback to the next implementation epoch.
- **uncertain:** absent reviewer, unknown evaluator outcome, incompatible evidence or a check that cannot run. Record the reason and follow uncertain; do not silently choose yes or no.

The `human` policy does not permit a daemon actor to invent approval. Persist all implementer session IDs for a delivery generation even after their PTYs exit or manual claims are released. A workflow participant's approval eligibility comes from its durable role assignment, not the absence of `claim_session`. If the native transition API is used to approve a workflow-owned story, it must consult the same reservation/role/evidence predicate. Manual unrelated stories retain their current contract.

Today's check API requires acceptance before checks, while Judge needs checks before approval. Add a backend check path for an unaccepted subject; reuse the command runner and digest verification. Keep pre-approval receipts bound to the reviewed criteria/content revision. The approval status transition increments native story revision, so atomically link its new accepted revision to the unchanged evidence subject through an approval receipt; do not pretend the original receipt evaluated a later requirement edit. Post-integration checks still evaluate the canonical merge result.

Typed workflow reports currently do not move native story status. Add a workflow-authorized transition service: start the durable reservation as InProgress, apply criterion results only from the current implementer assignment, submit Review only when criteria are met, and let the independent approval transition produce Done. A failed review leaves the reservation intact and returns the native story to InProgress for repair. These transitions must use the same revision checks and journal as manual operations. Do not borrow an ephemeral manual claim to authorize a durable workflow; manual attempts cannot take an already workflow-reserved story.

Story and workflow SQLite databases cannot commit together. Apply approval through a durable transition effect with a unique operation key recorded in the story database alongside its transition. Retrying the same operation returns the committed transition/provenance. Then append StoryAccepted in the run database. A crash between them reconciles by key; a conflicting story edit blocks adoption. This is a local idempotency mechanism, not permission for repeating a merge.

For Resolve plan, the coordinator proposes rather than schedules. Stage proposals during the Coordinator activation with proposal keys and current plan fingerprint; Create Stories applies them only when its predecessor token arrives. Existing `workflow action=proposal_create` authorization must be changed to stage a proposal rather than bypass that node, and the daemon applies it through the existing idempotent story-store creation service. A completed coordinator report can explicitly propose planning closure only after the accepted proposal set is applied; computing the fingerprint before creation would close the wrong set. The logical coordinator identity survives replacing its agent.

The baseline replan repeat edge points to create, which cannot produce new planner input after a completed Coordinator activation. Recommended seed revision changes that edge to coordinate, then create -> dispatch. The new revision is published; old pinned revisions are not overwritten. Existing graphs that deliberately revisit Create Stories without a planner may only reuse already staged, current proposals; they cannot invent a new wave. If the plan/story fingerprint changes after closed planning, the scheduler invalidates final verification and returns through the pinned replan path before dispatching new work.

Plan Judge requires explicit current planning closure, accepted terminal outcomes, verified integration and a current final-plan check receipt. Existing `FinalVerificationPassed` is replaced as an unrestricted public assertion by a service operation that executes the pinned final checks and rechecks the plan/story fingerprint before committing. The seed has no plan check policy today; publishing an executable Resolve plan must require the configured final checks or an explicit independent operator final-verification policy. The proposed default requires deterministic checks.

### Pause and resume

Current Pause has no outgoing edge and current Resume only changes run status. Recommended new definition policy gives Pause one explicit `resume_to` setting; it does not treat a terminal node as successful. Publication validates its target, and each resume requires a specific pending resolution.

For the seeded uncertain story path, `resume_to=judge`: an independent reviewer/operator supplies a typed resolution tied to the unchanged evidence subject; Judge is reactivated with that resolution. A plain resume without evidence remains paused. For repair exhaustion, resuming to Judge permits evaluation of existing work or cancellation; it does not reset the repair cap. More repairs require an explicitly authorized budget extension event or a new run, never a silent counter reset. A needs_input agent pause instead resumes the waiting agent activation with its answer; replacement uses a new fenced attempt generation and includes the durable answer in its prompt.

For multiple parallel requests, answers/resolutions name a pause or attempt ID. Resume requires all blocking requests resolved, uncertain effects reconciled and a valid recovery target. Reports from already-running parallel attempts remain recordable while paused, as supported today. No new agents or graph transitions start until resume. Timers and cancellation still operate.

### Loop accounting

For `max_iterations=3`, recommended interpretation is **three repeat traversals**, after the initial attempt: at most four implementation attempts for the seed. Count by `(graph_execution, loop_node, enclosing_epoch)`; iteration is recorded before the repeat token. Re-entering from another repair within the same scope cannot reset it. An outer loop creates a new inner scope only if explicitly defined; the overall run budget counts every repeat across scopes and stories.

At the fourth visit to that Loop, select exhausted and pause. If the global budget, spawn cap or deadline is reached sooner, record budget exhaustion and pause with its own reason; do not mislabel it as the node's exhausted outcome. `AdvanceLoop` receives activation identity and expected sequence and cannot be called on a graph with no eligible Loop token.

### Join and parallel branches: explicit design choice

Existing ordinary nodes permit one outgoing edge and Judge/Gate choose one outcome. Thus existing graphs cannot create true concurrent branch tokens. Multiple incoming edges may only be mutually exclusive alternatives or loop arrivals. Interpreting every current Join as an all-predecessor barrier would deadlock a valid exclusive branch.

Recommended design:

- Pin `Join.mode=merge` for an exclusive merge: one live arrival releases it for that scope; non-selected conditional edges are not awaited. Publication rejects ambiguous arrivals that could activate merge twice in the same scope.
- To support parallel branches, add an explicit **Fork** node, paired with a `Join.mode=all` and `fork_id`. Fork records the required branch set and produces one token per branch in one transaction. Join waits for exactly that set in the same fork/loop scope and releases once. Unrelated dispatch waves and old loop epochs cannot satisfy it.
- Publication requires structured forks with a common matching Join, no branch escape before joining and no loop crossing a fork boundary. Agent nodes retain one successor; adding a second connection must not secretly become a fork. Conditions may branch inside a fork, but each selected path reaches the matching Join.
- On branch failure, pause the scope, preserve completed arrivals and fence the failed activation. Explicit retry replaces only that generation. Cancel invalidates every outstanding branch. Do not treat a cancelled branch as successful arrival.

The design uses Fork plus paired all-Join for enforced read-only review/validation branches. It is not present in the baseline palette. Slice A adds its schema/validator; G adds execution and client editing. Do not claim its runtime verified before G and H.

## Worktrees, concurrency and dependencies

Reuse registered worktree validation and assignment. The scheduler creates story/coordinator worktrees through the backend worktree service with a durable create-worktree effect; it does not shell out from the frontend. Setup must succeed before agent spawn. Record the created identity and preserve it after failure for inspection. Cleanup is an explicit safe effect after terminal delivery; do not delete dirty or unregistered worktrees.

A durable reservation prevents two roots from owning one story. Acquire it before dispatch/direct story start and keep it across PTY exit, pause and restart. Release only at terminal cancellation/delivery or explicit takeover. A unique story reservation also covers a direct story run racing a plan run. Keep the existing one-active-run-per-plan constraint; separate plans may run concurrently within daemon/project limits.

Within a run, retain `max_parallel_stories`, dependency receipts and file-scope conflict checks. Across runs, apply the same scope/resource check to reservations in the same canonical project, including manual live claims. Unknown/globbed scopes serialize. For the first Fork slice, intra-story concurrent Agent branches are explicitly read-only review/validation of one immutable artifact subject. Enforce this restriction through the execution sandbox/worktree permissions, not just prompt text; unavailable enforcement means publication/start refuses the parallel graph. Concurrent writable intra-story branches are out of that slice: separate worktrees need a defined composition/merge step to create the one artifact the Join evaluates, plus explicit mutation authority. Scope labels alone cannot make shared worktree writes safe. Parallel mutation is already supported across independently reserved stories. Reviewer/validator artifacts stay immutable while read in parallel.

Dependent release uses the current shared integration predicate. Done alone is insufficient; WontFix never supplies delivered output. A dispatch with no ready stories checks for active children, blocked prerequisites and open planning rather than treating emptiness as completed. An included WontFix story requires an explicit independent plan-disposition decision before plan closure; its dependents remain blocked until their requirements are changed through the existing operator action.

Integration remains explicit in the first executable slice. At successful story End, persist graph completion/approval and show delivery waiting_integration until the operator merges and `record_integration` verifies it. Story Dispatch's completed edge requires that receipt. An operator-only integration effect can be added later after a separate automatic-merge design is approved; a coordinator report alone can never merge. A canonical ref advance requires existing recertification; approvals/checks must not silently survive it.

## Failure, side effects and recovery

| Boundary | Durable intent/result and retry rule |
|---|---|
| Commit succeeds, client loses start response | Start request key returns the same run; one active reservation remains |
| Agent reservation committed, no spawn started | Adapter records known-not-started failure; explicit retry may create a new attempt |
| Spawn started, binding not committed | Effect is uncertain; look up operation key and owned child, then adopt verified binding or resolve failure; never spawn blindly |
| Binding committed, wake lost | Read durable binding; no second spawn |
| Agent exits without typed report | Attempt interrupted; run pauses regardless of exit code |
| Valid report duplicated | Original receipt and successor activation are returned; duplicate sends no new coordinator wake |
| Late/stale report | Retain audit event; never advance the current graph activation |
| Cancel races with spawn | Cancel fences generation and effects; adapter stops only the child it owns; record orphan cleanup/reconciliation |
| Check times out | Stop only its own process group; record failed receipt; Gate/Judge follows policy |
| Notification receipt lost | Stable notice key prevents a duplicate durable notice; transient OS notice is best effort |
| Story creation/approval crosses databases | Lookup committed proposal/transition key; adopt exact matching result; contradictory payload remains blocked |
| Integration result uncertain | Inspect Git and current receipts; require explicit operator reconciliation; never replay merge |
| Daemon restart | Executor ownership first; recover events/activations, interrupt unresolved attempts and pause before any new external effects |

Extract spawn from `launch_workflow_agent` into an adapter called by both MCP and runtime. The MCP entry retains caller authorization; the daemon entry uses a trusted context derived from the committed run/activation/reservation, not caller-supplied actor fields. Generate a launch operation ID before process creation and attach it to the managed PTY registry. It supports reconciliation within a live process, but does not promise exactly-once external execution across crashes. A missing durable binding remains uncertain after restart.

Restart preserves the current conservative rule: pause active executions and require explicit safe recovery, rather than continue agents automatically. Resume adopts only a verified surviving child binding or creates a new generation after the old effect is explicitly resolved. Completed node/effect receipts never repeat. Extend terminal-run event handling to accept cleanup/effect-reconciliation receipts without reopening the run; today terminal runs reject those events, which would otherwise lose a cancellation-race outcome. Process ownership and effect keys bound what can be terminated; no ancestor-process kills.

## Starting, controlling and observing runs

Add a **Start workflow** control to the selected native story/plan. Keep **Start work** as manual story work. The client sends a published definition ID/revision, start request ID, expected story/plan revision, bounded limits and an allowed role-profile selection. Drafts cannot start. Backend validates policy, ownership, checks, reservations, resource availability and pins the selected revision in one transaction. The response includes the durable run ID and initial sequence; scheduling is daemon work after commit.

Extend existing `workflow_run_action` with `start_story` and the start-key contract for `start_plan`. Expose a `workflow action=run` MCP entry with generated Rust schema for start/get/list/events, cancel, answer/resume and effect resolution as authorized. Use the same backend service for IPC, guarded HTTP and MCP. Managed actors may read their owning project and submit their own bound reports/proposals; operator mutations require the configured operator authorization. Loopback transport does not establish human identity or permission. Resolve the existing local-agent/operator authorization concern before enabling autonomous execution of editable command policies.

Add HTTP/WS parity for every IPC surface and map actions through `COMMAND_TABLE`, its generated-path Vitest assertion and Rust route probe. Carry canonical owner path so connected-remote starts reach that daemon. Publish changes through the existing dual-emitted desktop/SSE cursor event. MCP events are paged by the same sequence; they do not depend on the agent remembering inbox mail. Add graph activation/decision inspector data to Run history; it must show waits, selected edges, actor provenance and interrupted attempts honestly.

Run controls use expected sequence, stable request IDs and payload hashes. Cancel is available while running or paused. Answering a request does not itself resume. Resume names the recovery/decision being resolved; cannot bypass evidence, deadlines or budget caps. The UI renders backend validation errors and exposes no browser-side scheduling computation.

## Automated test strategy and 37-case mapping

These are planned tests, not runs or passes. Write behavior tests only where they protect the named failure below. Existing valid tests remain in place; extend them rather than duplicating coverage.

Use three layers:

- **Rust domain:** pure reducer/policy with real store transactions and controlled clock; an in-process recorded-effect adapter only for TUIC-owned side-effect boundaries. Test the event/receipt contract, not a mocked model conversation. Startup/crash tests open real isolated SQLite databases and recover committed boundaries.
- **Service/transport:** actual route/router and managed identity resolution; reuse launch/report tests, check runner and toy Git repositories for digest/integration behavior. Generated IPC/HTTP route parity and MCP schema assertions protect consumers.
- **Headless E2E/UI:** later build `tuic-remote` from the delivered worktree and run against the disposable repository with configured role profiles. Use actual agent reports, not hand-authored fixtures pretending to be captured external output. Record real report payloads for repeatable contract tests. Browser checks use isolated sessions, screenshots and trusted interactions.

| # | Case and expected outcome | Planned test / named bug | Layer |
|---|---|---|---|
| 01 | Story start visits implement from the published Start | `start_story_activates_first_successor` — catches a Running snapshot with no runnable node | Rust + API + E2E |
| 02 | Judge yes reaches End with current approval | `judge_yes_requires_current_independent_evidence` — catches model-only success closing a story | Rust + E2E |
| 03 | Judge no repairs then returns to implement | `judge_no_carries_findings_to_repair_epoch` — catches a failed review ending or losing feedback | Rust + E2E |
| 04 | Repair cap chooses exhausted | `loop_uses_pinned_node_cap` — catches run max_loops overriding the node cap/resetting retries | Rust + E2E |
| 05 | Judge uncertain records decision and pauses | `judge_uncertain_pauses_with_evidence_reason` — catches uncertainty silently passing | Rust + E2E |
| 06 | Resume resolves the graph pause | `resume_requires_resolution_and_reactivates_target` — catches status-only resume or immediate re-pause | Rust + API + E2E |
| 07 | Cancel while paused is terminal | `cancel_paused_run_fences_pending_work` — catches resumed/stale reports advancing cancellation | Rust + API |
| 08 | Planner add/connect/save/publish/run | `planner_published_node_executes_pinned_prompt` — catches editable prompts leaking into live planning | Rust + UI + E2E |
| 09 | Implement add/connect/save/publish/run | `implementer_requires_predecessor_and_reservation` — catches skipped predecessor or canonical-checkout mutation | Rust + UI + E2E |
| 10 | Review add/connect/save/publish/run | `reviewer_requires_separate_current_subject` — catches self-review/stale artifact approval | Rust + UI + E2E |
| 11 | Validate add/connect/save/publish/run | `validator_report_cannot_replace_check_receipt` — catches fabricated validation authorizing pass | Rust + UI + E2E |
| 12 | Judge node selects exactly one outcome | `judge_selects_one_published_edge_once` — catches duplicate/simultaneous outcome traversal | Rust + UI |
| 13 | Gate executes pass/fail | `gate_never_passes_missing_or_failed_checks` — catches vacuous pass or ignored failure | Rust + UI + E2E |
| 14 | Pause is saved, entered and resolved | `pause_pins_resume_target_and_pending_request` — catches lost decision after draft edit/restart | Rust + UI |
| 15 | Loop repeats only eligible activation | `advance_loop_rejects_unreached_node` — catches arbitrary command bypassing predecessor/cap | Rust + UI |
| 16 | Notify is durable and deduplicated | `notify_retry_keeps_one_durable_notice` — catches duplicate/lost notice on receipt retry | Rust + UI + API |
| 17 | Join releases correct active branches once | `join_scopes_arrivals_and_ignores_unselected_edges` — catches exclusive deadlock or cross-epoch release | Rust + UI + E2E if Fork approved |
| 18 | Unconnected node visibly refuses publication | `publish_rejects_unreachable_node_in_designer` — catches an apparently published orphan | Existing validator + UI |
| 19 | Cycle without Loop refuses publication | `publish_rejects_unbounded_cycle_in_designer` — catches an unbounded runnable graph | Existing validator + UI |
| 20 | Missing End refuses publication | `publish_requires_terminal_delivery_node` — catches a graph that can never deliver | Existing validator + UI |
| 21 | Duplicate links are rejected/deduplicated | `duplicate_edge_cannot_duplicate_activation` — catches repeated effect from duplicate arrival | Existing validator + UI + Rust |
| 22 | Draft edits preserve in-flight revision | `live_run_keeps_pinned_graph_and_settings` — catches latest-draft traversal/check substitution | Existing version tests + Rust + E2E |
| 23 | Cancel mid-node fences spawn/report | `cancel_racing_spawn_kills_only_owned_child` — catches orphan agent or late progression | Adapter + Rust + E2E |
| 24 | Child crash never implies success | `child_exit_without_report_interrupts_node` — catches successful exit advancing an unresolved attempt | Managed service + E2E |
| 25 | Isolated restart pauses without effect replay | `restart_recovers_graph_without_respawning` — catches duplicate spawn/lost position | Store recovery + E2E |
| 26 | Timeouts fire without operator traffic | `deadline_pauses_idle_run_and_bounds_check` — catches deadlines enforced only on next command | Clock + real runner + E2E |
| 27 | Independent stories run concurrently | `disjoint_stories_share_bounded_dispatch_wave` — catches unconditional serialization or over-cap spawn | Existing scope tests + E2E |
| 28 | Two starts of one story conflict/idempotently retry | `story_reservation_blocks_second_root` — catches concurrent plan/direct/manual ownership | SQLite concurrency + API |
| 29 | Dependent cannot start before integration | `dependent_requires_current_integration` — catches Done alone releasing output | Existing receipt tests + E2E |
| 30 | Current verified integration releases dependent | `merge_receipt_releases_only_current_dependency` — catches stale/foreign merge acceptance | Existing Git receipt tests + E2E |
| 31 | WontFix never releases dependency | `wontfix_remains_unsatisfied_in_dispatch` — catches cancellation treated as delivery | Story service + runtime |
| 32 | Implementer cannot approve even after claim/exit | `workflow_implementer_cannot_self_approve_after_exit` — catches claim-only authorization forgetting role | Managed story + runtime |
| 33 | Reviewer approval preserves actor | `workflow_approval_keeps_report_actor_and_subject` — catches daemon impersonation or wrong artifact | Cross-store receipt + API |
| 34 | Resolve plan creates, dispatches, replans and closes | `resolve_plan_reaches_verified_fixed_point` — catches empty-ready-queue or stale fingerprint completion | Rust + E2E |
| 35 | Run history UI shows every node/decision/page | `history_renders_replayed_decisions_and_waits` — catches missing later-page/paused node events | UI + browser E2E |
| 36 | MCP history matches HTTP/IPC | `mcp_run_history_matches_owner_event_cursor` — catches missing read action or transport divergence | Router/MCP/transport |
| 37 | Two plans run independently within shared limits | `parallel_plan_runs_share_project_reservations` — catches one run starving another or overlapping writes | SQLite scheduler + E2E |

Add adversarial cases to these behavioral tests: duplicate command with changed payload, false check digest, edited criteria during review, cancellation after report before successor commit, two executor owners, missing run config, unpublished draft start, uncertain effect resolution before resume, fork branch failure and plan revision change after final verification. These are boundary variants, not arbitrary test-count targets.

End-to-end verification uses a disposable repository and isolated instance configuration. Drive negative and ambiguous conditions through exact file, check and input state. Record source and graph revisions, role profiles, run and actor IDs, artifacts and the expected/observed matrix. Remove test registrations and instance configuration afterward.

## Landable slices

Each slice has a truthful capability boundary. A graph may be saved as draft while unsupported, but cannot be published/startable as executable until every node it contains is supported. Do not present half a scheduler as successful delivery. Documentation updates follow [sync matrix](../sync-matrix.md): backend/user guide/API/SPEC/FEATURES/CHANGELOG and runtime event/transport mappings as their contracts change.

| Slice | Deliverable and reuse | Acceptance and focused validation |
|---|---|---|
| A. Semantics and schema | Add versioned activation/token/decision/loop state, replay and executable graph validator; retain definition store | Cases 12, 14, 15, 18-22. Target changed definition/reducer/store tests; schema reopen/replay |
| B. Daemon executor | AppState runtime, owner lock, durable initial activation, predecessor checks, timers and pause/resume/cancel | Cases 01, 06-07, 25-26, 28, 37. Scheduler/store/ownership tests; no model/provider dependency |
| C. Agent effects and identity | Extract launch adapter, worktree effects, reservations, operation keys, fenced reports/exits and durable role authority | Cases 08-11, 23-24, 27, 32. Managed launch/report tests plus real process boundary probes |
| D. Story policy | Pre-approval checks, independent approval receipt, Judge/Gate/Loop/Pause/Notify/exclusive Join; typed resolution | Cases 02-05, 12-17, 33. Policy matrix, receipt and cross-store crash tests |
| E. Plan dispatch | Story child executions, proposal effect reuse, wave/dependency eligibility, operator integration wait and final-plan gate | Cases 29-31, 34, 37. Existing Git receipt tests plus plan fixed-point scenarios |
| F. Start and history parity | Story/plan start controls, generated MCP run schema, IPC/HTTP mapping, controls and full history inspector | Cases 01, 06-07, 18-22, 35-36. Changed Vitest files, route mapping/probe, scoped MCP tests |
| G. Structured parallel graph branches | Explicit Fork, paired all-Join, enforced read-only branches, scopes/generations and failure handling | Cases 17, 23, 25, 27 with branch variants. Token/concurrency tests; otherwise capability visibly unavailable |
| H. Isolated end-to-end verification | Build delivered headless binary; actual cheap agents and browser UI; execute and record all 37 cases | Functional matrix plus runtime crash/restart/approval evidence. No unsupported case counted passed |

Ordering: A -> B -> C -> D -> E -> F -> H. G is in scope, depends on A-D and lands before H. The operator-authorization contract must be enforced before autonomous scheduling; approval authority must be defined before D. A-F may land incrementally with executable publication/start disabled until the seed contracts are supported.

## Design decisions

- **0 — Delivery:** GO for the native runtime, slice by slice A -> F, then H. Slice G is included after A-D and before H.
- **2 — Parallel branches:** Explicit Fork with paired Join `all`, restricted to enforced read-only review/validation branches. Writable intra-story composition is outside this scope.
- **3 — Pause and repairs:** Pin `resume_to` and require typed resolution. `max_iterations=3` permits the initial implementation plus three repairs. Resume never resets counters.
- **4 — Plan verification:** Executable Resolve plan publication requires deterministic final checks. A caller-supplied boolean cannot certify completion.
- **5 — Legacy runs:** Inspect and cancel only; no resume and no guessed graph positions. New execution requires a new graph run.
- **6 — Authorization prerequisite:** Enforce operator authorization before enabling autonomous scheduling. It is a prerequisite of slice B. Slice A does not enable an executor.
- **7 — Resource/profile defaults:** Two parallel stories by default, a per-project spawn limit, explicit sonnet/sol profiles, and visible errors when a required profile is missing.

## Approval and integration authority

**1 — Approval and integration authority:** approval comes from an independent reviewer; a workflow implementer may not approve even after claim release or exit. Merge remains explicit, never automatic. An included WontFix never releases dependents.

## Slice A implementation boundary

Run-store schema 2 and event contract 2 persist serial transitions through the existing immediate transaction and replay reducer. The event contract selects executable graph validation and replay semantics; future contracts must retain the version-2 semantics. Activations retain predecessor and edge provenance, with a 4096-activation history bound. All settings/checks are copied from the run-pinned published revision; current drafts are never traversed. Judge/Gate/Dispatch decisions require one published outcome and bounded string actor/reason/evidence provenance, without granting approval authority. Loop counts are scoped to the graph execution and node and do not reset on resume. Pause follows the pinned target and stores a bounded string resolution. ResolvePause and Resume share uncertain-effect and unanswered-input checks, including event replay.

Executable graph start requires deterministic final checks and pinned Pause targets. Normal publication retains its existing semantics for UI/API consumers, including built-in Pause nodes without resume targets and plans without checks. Fork/all-Join schema and execution wait for G; Join retains exclusive serial merge semantics. No executor, start control, new MCP surface, role authorization, process effect or approval/merge policy is enabled by A.

## Slice B implementation boundary

AppState owns a WorkflowRuntime started on both daemon boot paths. An OS lock beside workflow_runs.sqlite3 precedes recovery; store reads never recover. Per-run Notify mailboxes wake serial expected-sequence turns, and persisted duration budgets arm idle timers. Schema 3 adds an explicit optional rootTarget; the event contract remains 2. The existing transaction atomically writes the root Start and its first successor, with payload-bound start retries. Active graph projections are reservations; the run writer lock serializes root starts against manual claim/start. One active run per native plan remains enforced, including direct roots.

ResumeGraph records a bounded explicit resolution and names the actual pending activation. It resolves a pinned Pause target or records restart/operator recovery without resetting budgets; uncertain effects and input remain fences. DeadlineExpired is durable and emitted without client traffic. No agent effect, approval, merge, notification, dispatch policy or UI is enabled. Executable start rejects unsupported nodes; End waits for authoritative delivery semantics in D/E. Cases 01,06-07,25-26,28,37 cover the B ledger/daemon boundary only; later-slice effects and H E2E are not claimed.

### Slice C capability boundary (2026-10-05)

Workflow graph slice C: the owning daemon executes pinned serial Agent/Judge/Loop/Pause/Notify/Join nodes through the existing RunStore ledger and managed launch fences. Named sol/sonnet profiles are required. New unsupported Gate and plan node publications are refused visibly. Manual pauses suspend active duration. Slice F supplies public story graph starts and history controls. Approval automation, plan dispatch and explicit integration remain later slices; graph completion alone never closes a native story.

Resolve plan create/dispatch/replan remains slice E. Independent approval automation remains D. Historical seeded definitions remain record-only; executable root start checks capabilities.

### Slice D capability boundary

Story Judge waits for daemon-computed pre-approval checks and an independent native approval transition before yes. Native history retains the approving session and revision; the bound report and current Git digest retain the artifact subject. Failed checks cannot approve. Gate selects pass/fail from current deterministic receipts. Merge stays explicit; plan dispatch follows in E. No new persistence format is introduced.

### Slice E capability boundary

Plan agents and repeated Create Stories visits reuse managed attempts and idempotent proposal keys. Dispatch creates child graph executions under existing writer-fenced project reservations and scope limits. Done without integration waits; WontFix blocks dependencies. Final canonical checks record existing canonical receipts and verification fingerprints before completion. Replan requests a fresh bound coordinator. No automatic merge, second state machine or new persistence format is introduced.

### Slice F capability boundary (2026-10-06)

Public `start_graph` reaches the owning daemon over the existing IPC/HTTP action route and generated `workflow action=run` MCP schema. Story starts pin publications and request IDs; run history shows graph positions, decision evidence, pause targets and complete paged event payloads. Graph resume is explicit and sequence-fenced. Public plan starts share the graph-root entry point with slice E dispatch; the story-only UI selector leaves plan start visibly unavailable. Browser and end-to-end verification are separate from the API capability boundary.

## Neutral catalog contract boundary

The independent neutral rework now defines immutable reusable script/subflow
descriptors and publication format 4. This is not a migration of this runtime.
The existing executor remains unchanged; target pins and typed output validation
do not supply dispatch authority or make adapters executable. See the internal
contract boundary in the backend workflow reference.
