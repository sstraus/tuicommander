# Workflow definitions

Slice F exposes `start_graph {target:{type:story|plan,id},expected_revision?,definition_id,definition_revision,request_id,limits?}` through the existing owning-daemon service. Story starts require the current native revision and pin the selected publication; the request ID is bound to its payload. Omitted limits use Rust defaults. Public plan starts use the same graph-root entry point and slice E dispatch. The Plans and Stories dialog has no plan start control; its selector handles stories only. The `workflow action=run` MCP entry uses an inline schema generated from `RunAction` and the public `RunCommand` variants; it returns the same scoped snapshots and cursor-ordered events as IPC/HTTP. Run history shows pinned activations, decisions/evidence, repair counters, pause targets and complete event payloads across pages. Graph recovery uses `resume_graph {execution_id,activation_id,resolution}` after answering pending input; pause and cancel use the existing sequence-fenced commands. Legacy runs offer inspection and cancellation in the UI. No new persistence or client scheduler is added.

Workflow definitions are owned by a canonical project path and stored in `workflows.sqlite3` under the TUICommander configuration directory. A definition has a mutable draft revision and immutable published revisions. Editing a draft uses an expected revision; publishing validates the graph, resolves pinned story-template revisions, and inserts a new published row in one SQLite transaction. A run must pin a published revision; this definition layer does not execute nodes.

Each definition has a `closure` policy. Existing records migrate to `human`, and new drafts default to `human`; this legacy policy name means an explicit approval transition, which may be performed by any trusted caller. `update_closure` edits a draft with an expected revision. `automatic` can be saved as a draft but publishing it is refused until TUIC has an automatic evidence gate. Published revisions retain their pinned closure policy.

A definition also pins `requiredChecks`: up to 16 check commands, each with a stable ID, an `argv` array, and a timeout of 1–600 seconds. `update_checks` edits the draft with an expected revision; publication stores the exact set on that immutable revision. Commands invoke executables directly, without a shell. `execute_check` runs a pinned check in the assigned, clean story worktree and appends a durable `CheckRecorded` event with command, exit code, branch ref, commit/tree digest, and duration. A failed command also receives a receipt, but it cannot authorize integration. A changed worktree or ref makes the receipt stale.

The graph contains stable node IDs and directed edges. Node kinds are Start, Agent, Create Stories, Story Dispatch, Judge, Gate, Pause, Loop, exclusive Join, Fork, all-Join, For each, If/else, Notify, and End. An Agent carries a role, prompt template, and a bounded list of supported capabilities. Story Dispatch is a plan-only node that pins a published story definition ID and revision in the same project. Create Stories is plan-only. Join is story-only, with at least two incoming branches. A Loop is the only source of a back edge and requires `max_iterations` from 1 through 100 plus `repeat` and `exhausted` edges. Judge requires `yes`, `no`, and `uncertain`; Gate requires `pass` and `fail`; Story Dispatch requires `completed` and `blocked`. Invalid references, duplicate or unreachable nodes, missing outcomes, and other cycles are rejected on publish.

The first list or explicit seed action inserts `Story delivery` and `Resolve plan` once per project. The latter pins revision 1 of the former. Fresh story and plan seeds both pin the repository-integrity check (`git fsck --connectivity-only --no-progress`, 300-second timeout), so their drafts can be published and their runs perform deterministic final verification. Projects can replace this baseline with their own checks in a new revision; clearing every check still prevents executable publication. Both remain editable as drafts; a new published revision does not rewrite an older one.

Desktop IPC command `workflow_definition_action` and HTTP `POST /workflows/definition/action?path=<project>` call the same Rust service. Actions: `seed_templates`, `create_draft`, `list_drafts`, `get_draft`, `update_draft`, `update_closure`, `update_checks`, `publish`, `get_published`. The MCP `workflow` tool has no definition action, so these are HTTP/IPC only. `update_closure`, `update_checks`, edge removal (through `update_draft`) and Agent role and capabilities have no Designer control. The action is a JSON object tagged by `action`; replies are tagged `{ type, value }`. The Plans and Stories dialog has a Designer tab for draft selection, drag/drop or button-based node addition, edge connection, node inspection, saving and publishing. The backend remains the authority for graph validation at publication.

## Durable runs

`run/` stores plan runs in `<config dir>/workflow_runs.sqlite3`. A run pins a published plan revision, the Story Dispatch node's published story revision, and the canonical checkout's branch ref when Git exposes one. An immediate SQLite transaction appends each sequenced event and replaces its run, story-execution, node-attempt, and effect-intent projections. `replay` folds the event stream through a pure reducer; `events_after` pages at most 500 events from a sequence cursor. Clients should re-read by cursor after reconnecting.

Commands have caller-supplied IDs bound to their payload hashes and an expected run sequence. A retry returns the original receipt; a reused ID with a different payload fails, as does a stale expected sequence. Runs enforce one active instance per plan, loop/story-creation/spawn/duration budgets, a maximum of two concurrent story attempts by default (configurable from one to eight), and generation-fenced attempt reports. Running story attempts with unknown, globbed, or overlapping file scopes are serialized. For a plan with a workflow run, a prerequisite counts for dependent release only when it has a `StoryIntegrated` receipt at its accepted revision; the same predicate gates manual start, managed claim, workflow dispatch, and final run verification. A plan with no workflow run instead releases dependents as soon as every prerequisite is Done. A report for an old generation remains an audit event without advancing the attempt. Agent attempts start only on Agent nodes in the pinned story graph.

Before spawning an agent or creating a story, the runtime reserves a stable effect key. At desktop and headless startup, the AppState WorkflowRuntime takes an OS owner lock beside the run database before recovery or scheduling. A second daemon can read the database, but operator mutations report executor unavailable. RunStore opens and reads never recover live work. The owner scans active runs once: unfinished attempts become interrupted, intended effects become uncertain, and active runs pause for an explicit recovery decision. Failed dependency refreshes are logged per run; later reads do not retry recovery or interrupt resumed work. Normal runtime reconciliation refreshes dependencies without interrupting live attempts or changing intended effects. Effects are never repeated automatically. An operator must resolve each uncertain effect before resuming. Reconciliation also repairs dependent readiness from committed integration events after a crash. Planning closure stores a fingerprint of the current story set and requirements. The next command reopens planning if that set changes. Completion also requires every story to be Done, accepted, and integrated at its current revision, no active attempts or uncertain effects, and a current final-verification fingerprint. The runtime currently records the final-verification decision supplied by the caller; the policy and evidence gate are part of the later evaluation slice.

`workflow_run_action` and `POST /workflows/run/action?path=<project>` expose `start_plan`, `get`, `list_plan_runs`, `events`, `command`, `execute_check`, `record_integration`, and `recertify_canonical` using the same Rust service and canonical project checks. `list_plan_runs` returns the newest run snapshots for a plan with a limit of 1–100, so a timeline can discover its durable run ID. `record_integration` records an already completed, non-fast-forward merge: TUIC verifies that the checked source commit is the merge's second parent, the canonical checkout is clean, and all pinned checks pass again on the merge result. The receipt records canonical ref, base/source/result commits and trees, accepted story revision, and post-integration check receipts. The merge itself remains an explicit operator action in this slice. An out-of-band ref or tree change invalidates the prior tip's evidence. `recertify_canonical` runs the pinned checks on the clean new tip, verifies every integrated source commit is still an ancestor, and appends a durable `CanonicalRecertified` event. This receipt-only action remains available after a run completes or cancels, because other plans can depend on its integrated stories. A later ref, commit, or tree change invalidates that event too. `execute_check`, `record_integration` and `recertify_canonical` have no dialog control; the MCP `workflow action=run` takes the same run actions. This is an operator API. Agent binding and typed reports are refused on this API; they require the managed MCP session.

## Live launch inspection

`prompt_receipt` captures the final managed spawn payload separately in existing PTY metadata, including the peer preamble added after workflow rendering. Explicit system instruction arguments and bounded file snapshots are captured before spawn. MCP initialize responses are retained in existing protocol metadata, then copied to the owning PTY to cover startup races and protocol reaping. Workflow `AgentBinding.auditPreview` and `promptSha256` keep their current contract; the inspector never relabels them as the final launch prompt. Live receipts are bounded, redacted and session-lifetime only. See [AI Agents](../user-guide/ai-agents.md#inspect-launch-instructions) for caps and observation limits.

## Managed agent attempts

The `workflow action=launch` MCP entry accepts a run attempt and a Git worktree path already registered under the project. A plan attempt stays in the caller's isolated worktree. A story attempt may use another registered isolated worktree: the active coordinator assigns it in a durable event before worker spawn, and another story in the run cannot use that worktree. Later attempts for that story reuse the assignment. The service does not create, integrate, or remove those worktrees yet. For a plan attempt, it renders the pinned coordinator or planner prompt with plan source, current stories, revision and sequence cursor. For a story attempt, it renders the pinned role prompt with criteria, file scope, feedback, skill references and stable IDs. It redacts the delivered prompt, bounds it to 32 KB, and stores its contract version, SHA-256 hash and redacted preview. A story worker launch requires the calling PTY to own a running coordinator attempt in that run.

The engine reserves a spawn effect before calling the managed `agent spawn` path. The resulting PTY session and task handle are bound to the attempt in a durable event that also completes the effect. An already reserved spawn is never retried automatically; reconciliation pauses an uncertain run. Cancellation marks unfinished attempts interrupted and intended effects uncertain; a child that starts before the failed binding is killed. The `workflow action=report` MCP entry resolves the calling live PTY itself and accepts a typed report only for its bound attempt and current story revision. The report and criterion evidence are durable events. A pause prevents new work but preserves current-generation reports from already running parallel attempts. Duplicate identical reports return the original receipt; a different payload for the same attempt is rejected. PTY exit interrupts an unresolved attempt and pauses the run, regardless of exit code. The operator and browser clients receive the same `workflow-run-changed` cursor hint and replay events from storage.

In retained event contracts 2–4, a `needs_input` report includes `inputRequest {question,options}` and pauses the run. The operator command `answer_input {attempt_id,answer}` appends an answer event; the run remains paused until an explicit `resume`. An unanswered request blocks resume and final verification. A completed reviewer report includes `review {decision,artifactDigest,findings}`; each finding names a criterion index, severity, summary, and evidence. This is advisory evidence tied to the reported story revision, and does not change story status. After a committed story-worker report, the active coordinator gets inbox mail containing only the run ID, story ID and durable sequence cursor; it must read the event log for the actual result. Late or duplicate reports do not send a fresh wake. The review policy and artifact verification are separate work.

Launching attempts is explicit in this slice. Automatic graph scheduling, evaluation and review policy are subsequent slices.

The active plan coordinator can call `workflow action=proposal_create` with a stable proposal key. The story store commits the key and story in one transaction, so a lost MCP response and retry cannot create duplicates. The run separately reserves a bounded CreateStory effect and records its outcome; a crash between the two databases leaves an uncertain effect for operator reconciliation. A read-only proposal lookup can establish whether the story was committed, but the creation tool refuses uncertain effects and never replays them. A reused key with different content is rejected.

Published checks own their process trees (Unix process groups; Windows suspended
job assignment with kill-on-close). Cancelling a run synchronously stops its
active checks and fences late starts. Host shutdown tears down every active check
before exit; cancellation yields no successful receipt.

Workflow definition and run APIs do not accept or persist actor metadata. HTTP adapters call the same actorless services as desktop IPC. Story transition actors are tracking metadata, not action permissions. Story transitions through desktop IPC and credential-authenticated HTTP record Human; sessionless local HTTP (including missing caller-address metadata on Unix sockets) implies LocalApi. Managed story calls record their session identity. Local token exchange is accepted. Actor identity never refuses story or workflow actions; state, revision, project and integration checks still apply. The actor is host-selected; request JSON cannot choose it.

Plan aggregation uses the same receipt freshness authority as dependency release and final verification. Workflow-owned plans are Done only while every approved story has current integration evidence at its accepted revision; ref movement returns Active until recertification. Manual plans keep Done/WontFix aggregation. Explicit recertification remains available after cancellation to renew already integrated evidence.

Git receipt and dependency probes run before SQLite write transactions and before the story-store mutex is acquired. The short commit transaction revalidates the persisted run sequence and relevant story revisions; a concurrent change fails with a retry error. No filesystem fingerprint is persisted: ref movement after a probe invalidates receipt freshness on the next read.

## Graph contract foundation (slice A, 1446-ff21)

Run-store schema version 3 retains the version-2 ledger and adds explicit graph-root target semantics. Slice A introduced `eventContractVersion` and `graphExecutions` to snapshots. The event stream remains authoritative: a graph-start event pins the entire published definition (graph, checks, closure, project and revision), and graph transitions replay activations, serial predecessor activations and edge indices, selected decision edges, actor/evidence provenance, per-execution node repeat counters and resolved pauses. Transactions and existing payload-bound command receipts prevent duplicate successor creation. Counter values survive pause and database reopen. A cap of three permits three repeat traversals after the initial attempt; the run-wide loop budget also bounds repeats.

The version-2 graph-start boundary requires deterministic final checks for both kinds and an explicit non-Start/non-Pause `Pause.resume_to`. Normal draft publication remains the legacy record-only boundary; it is not a promise of automatic execution. Duplicate links, unreachable nodes, unbounded cycles and missing End are rejected by the shared graph validator. Existing published revisions are never rewritten or assigned guessed resume targets.

Pause optionally carries `resume_to` (required for executable graphs). Join remains an exclusive merge; Fork and all-Join are absent until slice G. Decision actor and pause resolution are bounded strings. ResolvePause uses the same uncertain-effect and pending-input fence as Resume, including replay. Serial history retains at most 4096 activations per execution, each linked to its completed predecessor and selected edge (or resolved pause). `eventContractVersion=2` governs both executable validation on graph start/replay and transition semantics; graph executions have no separate version or token ledger. Future graph semantics require a new event contract and a retained version-specific replay implementation.

Graph mutation commands are internal and rejected by the public run-action transport. They do not spawn agents, evaluate approval policy, execute checks, merge or schedule work. Completing an activation in this ledger is not an agent report or a story approval. Those authoritative bindings belong to subsequent slices. Graph runs cannot use unscoped AdvanceLoop, StartAttempt, StartPlanAgent, Complete or status-only Resume to bypass graph position.

Snapshots written before the event contract field existed decode as version zero with no graph positions. They replay and can be inspected or cancelled, but new mutation/resume commands are refused. Cancellation compares the actual stored projection encoding so additive default fields do not break the old-row update. New record-only runs retain their existing explicit command service during rollout. Older binaries refuse a version-2 run database.

## Daemon executor (slice B, 1446-ff21)

One WorkflowRuntime belongs to AppState on both desktop and headless boot. Per-run asynchronous actors wake after committed cursor hints and from timers; SQLite events and pinned definitions remain authoritative. Each turn uses expected-sequence transactions and at most 32 serial transitions. A waiting actor does not poll. The daemon persists DeadlineExpired at the run duration limit even without operator traffic. Paused time is excluded from active duration using committed event timestamps; resume retains the remaining active budget. Expired active budgets require cancellation and a new run.

The internal graph-root start service atomically writes Started, the root graph, Start activation/completion and the first ready successor. A payload-bound start request key returns the same run after a lost response or restart; a changed payload fails. `rootTarget` distinguishes a direct story from its native `planId`. Active graph projections reserve their target stories across pause and restart; cancellation releases them. Root creation and manual claim/start share the run database writer lock, then recheck native story state. The existing one-active-run-per-plan rule remains in force. Different plans have separate actor mailboxes and transactions.

`resume_graph {execution_id,activation_id,resolution}` names the pending activation and a bounded nonempty operator resolution. A graph Pause resumes at its pinned target; recovery of another paused graph position records GraphResumed. Pending input and uncertain effects must be resolved first. Status-only Resume remains available for record-only runs, and cannot resume a graph. Cancellation fences graph traversal and late reports. Every actor commit emits the existing desktop/SSE cursor hint.

Agent effects, policy evaluation and terminal delivery gates are not implemented by B. Executable graph start refuses nodes outside Start/Pause/End, and a reached End is retained for later authoritative delivery policy rather than closing a story or plan. Built-in delivery graphs remain unavailable as executable workflows. The internal root start service has no new transport or UI control until F; normal definition publication remains a record-only operation. Agent profiles, provider calls, approval and merge authority are unchanged. Independent approval and explicit merge authority remain open before D.

## Serial Agent execution (slice C, 1446-ff21)

The daemon follows pinned Start, Agent, Judge, Loop, Pause, Notify, exclusive Join and End nodes. A deterministic turn creates only the reached Agent attempt. The effect adapter reserves one spawn intent before worktree creation and uses the existing managed launch service to assign/reuse the isolated worktree, render the pinned prompt, launch the process and bind its owned report identity. Implementers require the named sol run profile; reviewers and validators require sonnet. Missing profiles or failed effects pause with a durable repository-journal reason. A running attempt advances only after a typed bound Completed report; process exit and idle are not success. Failed and NeedsInput reports retain unresolved work. AnswerInput records the operator response but does not complete the attempt. Resuming without completed work pauses again; cancellation remains available. This slice does not restart or continue a reported attempt automatically. Repair prompts retain previous reports and findings.

Judge selects one published edge and records its actor and artifact references. Changes requested against a current clean subject route no. A yes also requires independent reviewer identity, current explicit story approval, acceptance and every pinned successful check receipt for that subject. Missing, conflicting or stale evidence routes uncertain. Review prompts include the commit, tree and their SHA-256 digest (UTF-8 commit:tree). This slice does not approve stories or merge branches automatically.

Loop keeps its published node cap across pause/resume. Exclusive Join consumes the selected serial arrival without waiting for unselected predecessors. Notify reserves an effect, writes a repository progress notice and commits the receipt before following its successor; uncertain delivery is not replayed automatically. Every activation, attempt, selected decision edge, pause resolution and effect receipt remains in the existing RunStore ledger. Actors yield and continue after 32 deterministic transitions.

The shared IPC/HTTP/MCP publication service visibly refuses Gate, CreateStories, StoryDispatch and plan Agent nodes, plus unsupported story Agent roles. They remain editable drafts; historical seeded publications remain readable and are refused by executable root start. Internal definition-store writes retain their record-only role for legacy seed/ledger consumers. Executable publication requires final checks and pinned Pause targets. Fork/all-Join remains unavailable. Slice F exposes public story graph start and history controls. Resolve plan create/dispatch/replan, automatic approval and integration policy belong to later slices. Graph End records graph completion, not story or plan delivery.

Snapshots add pausedSinceMs and pausedDurationMs to the same JSON projection. Historical snapshots without these fields recover timing from the existing event ledger; historical deadline-expiry events remain replayable. Paused time never replenishes an exhausted active budget.


### Workflow story policy (slice D)

The daemon runs pinned deterministic checks before accepting an independent reviewer approval. Resuming a reached Gate after a corrected artifact commit records a new check receipt; its daemon command key includes the current run sequence. Native approval history preserves the reviewer actor and revision; the run retains the reviewed artifact and check receipts. A workflow implementer cannot approve its story after exit or claim release. Integration remains an explicit operator action.


### Workflow plan dispatch (slice E)

The owning daemon executes pinned plan coordinator and Create Stories visits through the existing proposal effects. Disjoint story children share a bounded project wave; overlapping or unknown scopes wait. A dependency starts only after its accepted revision has a current explicit Git integration receipt. Done alone and WontFix never release it. Approved children wait for an operator merge. The final plan Judge runs the pinned canonical checks and binds verification to the current plan fingerprint. Failed checks route through a bounded replan with a fresh coordinator attempt. Public start/history controls and parallel graph branches follow in later slices.

### Read-only run incidents

The `incidents` run action uses the same canonical-project check as other run reads. `run/incidents.rs` derives causes and suggested operator actions from recorded attempt reports and interruption outcomes. The shared IPC/HTTP adapter enriches active or interrupted bound attempts with retained session awaiting-input state, exit tombstones, notified pending initial prompts, and failed/cancelled/input-required task records whose session binding matches. Completed historical attempts never inherit live failures. No records, prompts or tasks are mutated. Working state alone is never a stuck diagnosis. Durable outcomes survive restart; transient evidence may be unavailable and the UI discloses this limit.

The Workflow Designer Pause inspector exposes **Resume to** and persists the selected node in `resume_to` when saving the draft. Start and Pause nodes cannot be resume targets. New story seeds resume at `implement`; new resolve-plan seeds resume at `create`. Existing published revisions remain immutable; existing drafts can be repaired with the inspector. Executable-graph validation failures on publication return HTTP 422 with a JSON `error` naming the invalid field (for example, `resume_to`).


### Event intake bridge

An explicit `event_intake.workflow` pins a published native story workflow revision.
Event dispatch creates the native plan/story, stores the automation prompt in the
story problem, and binds the automation's isolated worktree before waking the
workflow actor. Publication reuses `judge` and `policy::approved`: a completed graph
alone is not approval. It requires a separate current reviewer, matching clean
artifact digest, accepted native revision and every configured passing check.
The publisher checks the pushed GitHub branch SHA and includes the independent
findings in the draft body. Existing intake without a workflow remains unchanged.

## Shared contract and preservation baseline (WF01)

WF01 introduced event contract 3 and run-store layout 5. Version 2 serial graph
replay remains supported: Join is still an exclusive merge, not an all-Join.
Graph-start events add an immutable `contract` with version, definition SHA-256
and explicit policy imports. Current starts pin the existing native definition,
checks and closure; an empty import list does not claim imported global policy
or installed model/hook health. Old runs retain their recorded contract and
publications. Draft edits and publication never migrate an active run.

See the [WF01 shared contract and frozen baseline](workflow-baseline/contract.md)
for the effect/receipt schemas, policy ownership, I1–I10 source resolutions,
recorded serial-run fixtures, complete capability matrix and source/test pins.
This is a compatibility foundation, not operational migration or authority to
start a second scheduler, publish externally or retire legacy mechanisms.

## Typed command execution (WF05)

Run-store layout 9 refuses older binaries before they read the new command and workspace effect kinds; retained contract-2/3 serial histories remain replayable.

Published `run_command {command}` and `tool {tool_id,tool_revision,command}` nodes
have `pass` and `fail` edges. Their version-1 command contract pins an argv
invocation (no implicit shell), or an explicit `shell` invocation with a program,
script and shell-contract version. It also pins `workspace: "assigned"`, target,
resources, secret references, optional workspace-relative stdin artifact, timeout
(1–86400 seconds), and output schema. Tool IDs and revisions are receipt inputs;
provisioning tools are explicit authorized maintenance, never automatic installs.
These nodes do not add Designer controls; author them through the existing
versioned definition API. They require the run's assigned clean Git workspace.
Story commands can prepare that workspace before the first Agent node. Plan
commands without an assigned workspace fail closed; durable plan workspace
bindings remain part of WF04.

`local {os,arch}` refuses a different host. `rb` delegates to the installed
build-slot/rb adapters, including freeze, source manifests, reservations and
fair build admission. `rb_after_native {native_effect,os,arch}` additionally
requires successful local proof for the exact source and command/schema before
setting RB_FORCE. Linux proof cannot satisfy a native macOS requirement.
The remote Python runner executes on Linux rb and uses POSIX process groups;
a Windows client delegates to rb instead of running that script locally.
Local command receipts and secret redaction remain available on Windows. The
adapter root defaults to the orchestrator tools checkout and can be selected by
`TUIC_WORKFLOW_ADAPTER_ROOT` on daemon startup. No SSH scheduler is added.

Resources are Build (capacity/fairness owned by the wrapper), canonical Git
(resolved from the actual Git common directory), Mutation, GUI and Device.
The durable sidecar `workflow_runs.commands.sqlite3` records ready, queued,
admitted, uncertain and completed observations and named-resource ownership.
Secret/platform/native-proof checks and private input preparation finish before
admission. A confirmed spawn failure produces a failed, stopped/released receipt;
it is not an uncertain remote lease and cannot strand named resources.
The command intent pins its source subject and commits before starting a process. Duplicate identical effects
return their immutable receipt; conflicting payloads refuse. Freeze prevents new
admission but not history reads. A queued node pauses with its reason; explicitly
resume it after the recorded blocker changes. There is no busy polling loop.

`secretRefs` maps command environment names to daemon environment references.
Resolved values go only into the child environment or temporary private rb input
transport, never into definitions, argv or receipts. Local captured output is
redacted, including raw and JSON-escaped secret spellings; remote output is
redacted inside the admitted task before rb records it. Capture is bounded by
the serialized JSON envelope, not only by each source stream. Oversized output
ends with `[TRUNCATED]` and still produces a complete terminal receipt. Exit-status output checks the exit code. `test_summary`
requires JSON `{matched,passed,failed}` with a positive `minimum_matched`, every
matched test passing and no failures. `json_object` requires named fields. A zero
exit with no matching tests cannot pass a declared test-summary requirement.

Local commands use the same process-tree cancellation owner as pinned checks.
On Unix, shutdown stops and walks the owned descendants before killing the
wrapper, including the separate child groups created by local `--gate` commands.
Journal admission and local wrappers both use the same agent identity for
`BUILD_FREEZE` (`RB_AGENT_NAME` and `BUILD_SLOT_AGENT`).
A lost remote client or lease never releases a named resource. Reconciliation
uses the rb task/generation/fence and observed stop/release, not `result.txt`
alone. A recovered stop without retained command output is failed proof, not
success or permission to replay. Restart recovery also runs while paused. The
existing run actor rechecks pending stop proof every 30 seconds or on a wake,
using the admitted subject/handle without requiring clean source or current
secret references. An immutable receipt settles the existing uncertain effect;
it does not resume the graph. Failed/unknown lookup retains the reservation
and workspace. A completed receipt records source commit/tree, command and
output digests, target, toolchain, stop/release and remote manifest references.
Explicit Tool effects can change their assigned workspace; their receipts distinguish the starting subject from a clean result artifact, or disclose a dirty/unresolved result. Such a receipt cannot satisfy unchanged-artifact/native proof. A source-changing Run command fails proof but still releases its slot after observed stop. The graph follows only a committed terminal receipt. Old serial replay and
legacy `requiredChecks` keep their original contracts.

IPC, HTTP and MCP run actions accept `command_executions {run_id}`. The reply is
`{type:"command_executions",value:[...]}` with sequence-ordered durable command
observations. It is read-only and project-scoped. The existing run event cursor
still owns graph transitions; the command observation cursor is separate.
## Typed Create Stories input (WF04 foundation)

Contract-3 and newer plan graphs no longer require a Coordinator Agent node. A reached
Create Stories activation waits for `submit_story_proposals {batch}` through the
existing sequence-fenced `command` run action. The batch has camel-case
`executionId`, `activationId`, `planRevision` and `proposals` fields. Each proposal
uses the native `NewStory` shape and must name this plan with a `plan_step` origin.
An empty batch explicitly confirms that the visit has no additions. Submission
records an event; it does not spawn an author or infer proposals from prose.

The daemon creates each proposal through the existing keyed story/effect
transaction. Keys bind the execution, activation and proposal index, not an agent
name. Native story validation, story-creation budgets, cancellation and uncertain
effect fences remain in force. A changed plan revision refuses unconsumed input.
A lost response returns the same command receipt; a second batch for the visit is
refused. Pending input appears in read-only incidents. The Coordinator preset can
still author proposals, but its report is not input for this node. Contract-2
Create Stories visits retain their recorded coordinator-attempt behavior, and
story graphs still refuse Create Stories.

The lifecycle sections below define separate launch, routing, continuation and
close contracts. Typed proposal input alone does not authorize any of those effects.


### Daemon workspace attribution (WF04 increment)

Before a new daemon-owned workflow worker starts, the managed-launch journal
records its canonical Git common-directory key, per-checkout Git-directory key,
base HEAD, exact dirty status, staged and unstaged diff hashes, and hashes of
untracked files or symlink targets. It does not retain file payloads. Linked
checkouts share the repository key but have different workspace keys; repository
aliases and branch renames do not create another owner. An unreadable baseline
or missing base commit refuses the new launch rather than treating it as clean.

The baseline commits before spawn and is never refreshed by a retry. The
managed launch receipt exposes it as `workspace_baseline`; the durable attempt
binding also retains the managed `launchId`. Old bindings without this reference
remain replayable. A restarted daemon can read the original attribution even
when its live process inventory is unavailable, without spawning a replacement.

This baseline is evidence, not a cleanup authorization. Git cannot observe
unsaved editor buffers or exclude concurrent writers. Existing dirty work is
not attributed to the worker, and no worktree, file or branch is removed by this
process-close path. Launch and close receipts retain the original baseline; a
successor must independently establish its matching workspace ownership.

### Durable report disposition (WF04 increment)

Bound typed reports retain an accepted or refused decision, the resolved caller,
attempt and generation, submission SHA-256, decision time, and the event sequence
or refusal reason. An accepted decision commits in the same SQLite transaction
as the report event. If the decision cannot be saved, the report event rolls
back. A response-lost retry returns the original event and decision; a changed
payload or caller cannot overwrite them. Repeated identical refusals are retained
once; a later distinct decision is retained separately rather than erasing a
refusal. The journal does not copy report payloads.

The read-only run action `report_dispositions {run_id}` returns
`{type: "report_dispositions", value: [...]}` through the existing IPC, HTTP and
MCP run-action adapters, with the same canonical project scope check. An accepted
report means that the engine received valid evidence, not that the artifact was
approved, a successor was accepted, or a worker may be closed. Contract-2 event
replay remains unchanged. A malformed request or one that cannot resolve a
managed caller is refused before this bound-report journal.

### Owned process close (WF04)

The shared run action `close_agent` accepts `run_id`, `attempt_id`, `request_id`,
`expected_sequence` and an optional `successor_attempt_id`. It records a durable
refusal or stop intent in the managed launch database. An identical retry reads
that decision or reconciles the recorded process absence; it does not stop a
replacement. A conflicting request ID refuses.

Closure requires this run's durable launch binding, a reported attempt, and a
bound successor for the same target and canonical workspace, or a terminal run.
Dirty baselines, current dirty work, pending input, drafts, unread mail, live
background runners, isolated test apps and unknown process inventory refuse
closure. The process-tree check does not use the activity detector's startup-age
exemption. A stop never removes a workspace or branch and does not approve an
artifact. ACP closure requires its recorded direct-child PID, matching host-issued peer
environment, idle conversation and a quiet process tree; protocol
settlement alone is not process-stop proof. A retained settled turn is history,
not live work; queued prompts or pending interactions still prevent close.

### Explicit managed ACP routes (WF04)

A published Agent node can add `execution` with either
`{transport:"pty", profile:"<saved run config>"}` or
`{transport:"acp", profile:"<ego profile>", model:"<exact model>", effort:"<exact effort>"}`.
Omitting `execution` retains the historical PTY profile resolver and unchanged
serialized contract-2 definitions. ACP profiles and saved PTY run configurations
are separate namespaces. The machine ACP executable and repository profile
ceiling still apply; an unavailable exact ACP model/effort refuses delivery.
The explicit host policy below supplies the runtime and exact route; missing
policy refuses daemon delivery instead of falling back to mutable defaults.

ACP workers use a host-issued peer UUID and the real connection, generation and
conversation returned by `AcpClientManager`. They do not get a terminal, PTY
parent or task ID. Managed receipts have `transport:"acp"`, a null `session_id`,
and explicit peer/connection/conversation references. Attempt bindings retain
these under `acp`; their `sessionId` is then a peer address, not a terminal ID.
The run-store marker advances for these new bindings; serial replay stays serial.

The journal commits before connect, session creation and prompt submission.
The initial prompt remains durable as `not_delivered` until the workflow binding
commits. The exact retained payload is then released; a native ACP `PromptSent`
record for that turn and text proves handoff. A queued/unobserved submission is
not replayed. The immutable ACP MCP-channel origin fences reports to the bound
connection generation and conversation; looking up a peer's newest connection
is not report authority. Channel settlement and missing inventory do not prove
process stop or attempt success. Incidents carry launch and ACP references.
Process closure remains separate from artifact approval and worktree cleanup;
no cleanup is authorized. PTY receipts distinguish present inventory, an observed
exit, and unknown inventory after restart; missing maps never prove worker death.

### Explicit host routing policy

`WorkflowAction::ImportAgentPolicy {expected_revision,policy}` imports an operator
routing declaration with an optimistic revision. `AgentPolicy` reads it. Import
requires explicit source provenance, role, model, effort, capabilities, isolated
workspace scope and output-contract version. Each role has exact routes and at
most one explicit default. This does not parse or certify Markdown policy files.

PTY routes must name a saved configuration with the same explicit model and
explicit effort arguments. A canonical digest pins only the selected agent,
command, argv, model, environment, launch flags and effective launch instructions.
Changes to those inputs refuse delivery. Unrelated profiles, default-profile
selection and idle-close preferences do not invalidate the route. ACP routes pin an absolute executable and
machine profile, and the live ACP options must acknowledge model and effort.
Fresh roots capture the policy in `GraphContract`; children inherit that pin.
Identical start retries and replay never load the latest routing policy.
Contract-3 and newer graph Agent launches are daemon-owned. The retained MCP
`workflow action=launch` refuses these graphs before reserving a spawn effect;
it cannot substitute a caller-selected profile. Contract-2 replay and its
retained serial MCP launch and profile selection stay unchanged.

### Answer continuation

For a bound contract-3 or contract-4 attempt, `answer_input` keeps the existing launch and
conversation, advances the report generation and records the answer as `pending`.
The run still requires explicit graph resume. The daemon journals `handoff_started`
before external delivery and `delivered` after the ACP PromptSent acknowledgement
or exact peer-inbox handoff. PTY inbox delivery uses the normal payload-free wake;
it is not proof that the model consumed the answer. No new worker is allocated.

Missing inventory retains pending input. An interrupted handoff without its exact
retained receipt is uncertain and is not automatically replayed. Incidents expose
this state with the launch/conversation references. An old-generation report or
an answer alone cannot finish the node; the continued attempt needs a fresh typed
report after a confirmed delivery. Report command keys include the continuation
generation; identical retries retain their receipt and changed payloads refuse.
Contract-2 answer semantics and first-generation command keys are unchanged.


### Current version sequence

The run-store schema and layout are the same `PRAGMA user_version`, not separate
version counters. The combined build writes layout **11** and starts event
contract **5**. The retained sequence is:

| Run-store layout | Contract and compatibility boundary |
| --- | --- |
| 1 (or unversioned 0) | Original record-only ledger; additive snapshot fields decode with defaults. |
| 2 | Serial graph event contract 2. |
| 3 | Explicit graph-root targets, still contract 2. |
| 4 | Durable pause causes, still contract 2. |
| 5 | WF01 immutable definition pins and serial event contract 3. |
| 6 | WF19 durable timers for contract 3. |
| 7 | WF02 correlated regions and event contract 4; retained contract-3 timers still replay. |
| 8 | WF04 explicit worker transport/conversation bindings, answer continuation and durable report disposition. |
| 9 | WF03 branch-local questions and contract 5; contracts 2–4 retain their input/replay rules. |
| 10 | WF05 typed command, Tool and workspace effects; contract 5 remains current. |
| 11 | WF07 native integration saga, policy and proof-obligation outbox; contract 5 remains current. |

WF06 adds an optional evidence policy to the pinned definition and optional typed
review evidence/check requirements digests. It does not allocate a competing
layout or event-contract number. Omitted policy/evidence fields retain the legacy
contract; snapshots written before WF19 decode with an empty timer list. Layouts
0–10 open through the existing additive upgrade to layout 11 without rewriting
their snapshots or event streams. Contracts 2 and 3 keep serial Join semantics;
contracts 3–5 support Timer events, while contract 2 refuses them. A layout-10
binary refuses a layout-11 store because its `PRAGMA user_version` exceeds the
supported schema version; it cannot reopen the upgraded store. Layout 10 remains
the typed-effect boundary, and layout 7 remains the parallel-event boundary.

## Correlated parallel regions (WF02)

New runs use event contract 5 and run-store layout 11. Contracts 2 and 3 retain
serial replay, including exclusive Join; old definitions and events are not
rewritten. The pinned definition digest remains part of each new graph start.

A `fork` declares `join`, 1–32 distinct `{id,target,required}` branches and an
optional `repair_to`. Its matching `all_join` declares `fork` and explicit
`completed`, `failed`, `cancelled` and `skipped` edges. Branch links are node
fields, not duplicate graph edges. Publication rejects crossed regions, outside
arrivals, early exits, cycles and writable agents inside a parallel region.
Nested regions retain the complete parent-token stack. An activation carries
its actual predecessor, fork-instance ID, generation and branch ID. Graph Agent
attempts bind the exact execution/activation IDs; an unstarted superseded branch
cannot shift a later attempt's ownership. Historical serial runs keep their
ordinal lookup, and Create Stories retains its existing coordinator-report reuse. Every
branch must settle; optional branch failure does not fail the join, but optional
work is not abandoned while still running. A required failure, cancellation or
skip follows the corresponding non-success edge. Duplicate arrivals do not
release another successor. Old-generation arrivals cannot satisfy a replacement.

`for_each` uses the same region ledger: `join`, `body`, `items`, `max_items`
(1–32) and a required `empty` rule (`complete`, `skip`, `fail`). A declared
`receipt_evidence` Agent node can supply the collection dynamically from its
completed typed report's evidence list; literal `items` must then be empty.
Expansion records the exact source activation and items, and replay verifies
that they match the committed source report. Each item gets a distinct branch,
even when values repeat. Item context is included in the managed prompt.
Execution retains the existing 512-activation history bound; no expansion may
exceed it. Splitter compositions use the ordinary bounded Fork contract.

`if_else` accepts a typed `predicate`: `input {name,value}` declares a pinned
boolean or unknown value; `receipt {node_id,outcome}` reads a declared node's
typed terminal outcome. Its edges are `yes`, `no` and `uncertain`. There is no
expression interpreter, and absent evidence never implies approval. An all-Join
receipt reads its matching durable fork outcome, including the empty rule;
failed, cancelled and skipped joins never become completed merely because the
join activation has no enclosing branch token. Other sources require a typed
decision or terminal branch arrival; completion alone is unknown evidence.

The daemon scans independent runnable activations, rather than stopping at the
first running sibling. Agent attempts and asynchronous deterministic checks
share the configured per-run capacity. Resource waits and check claims are
committed graph transitions. Checks are bound to their branch activation and
fork generation; historical receipts cannot satisfy a replacement generation.
A branch Gate selects its own receipts. A Gate or approval Judge after all-Join
selects the current region's receipts, not its new activation ID. An empty
successful For each uses the ordinary post-region check receipts. Gate readiness
and missing-check selection use the same receipt set. Unrelated sibling cursor
or preflight contention releases the asynchronous claim and wakes the existing
actor to re-evaluate; it does not pause the run as a policy failure.
A superseded activity retains its admission slot and check claim until stop is
observed. Check claims include the owning daemon nonce: after a hard crash, an
empty registry in another daemon is unknown, not proof of process termination.
Run cancellation fences and stops every descendant check scope. Fork
cancellation fences only the old generation, leaving the replacement usable.

A current independent critic FIX supersedes the owning repair region before
repair begins. FIX validation reads the exact submitting Reviewer activation and
attempt; a later advisor's missing or approving report cannot mask it. When an
evidence policy is pinned, its typed triage decision controls supersession, not
the retained legacy review label. The same event fences its unfinished attempts and terminates
nested obligations. The daemon cancels its scoped checks, stops bound agents
and requires their exit observations before recording `fork_stopped`. PTY workers
require their exit tombstones. ACP workers use the exact managed launch, peer,
connection and generation, disconnect through the ACP manager, and require a
fresh OS inventory proving the recorded process ID absent. A settled connection
alone is not process-stop proof, and no terminal is created for an ACP stop. An
unknown spawn or missing stop observation blocks repair rather than freeing the
workspace. The stop transition enqueues one Implement activation outside the old
region. Its published successor opens a fresh generation and reruns both critic
and checks. Replayed stop transitions cannot enqueue a second repair. Existing
restart recovery still pauses unknown work for explicit reconciliation; reopening
a store alone neither repeats an effect nor invents stop proof.

These are backend definition/run contracts on the existing IPC/HTTP service.
The existing Designer can retain these node records; dedicated parallel-region
editing and inspection controls belong to WF11. No new scheduler or transport
is introduced.
## Typed evidence policy (WF06)

A graph draft can set optional `evidencePolicy` through the existing
`update_draft` action. Publication and graph-start contract pins include this
policy. Omit the field to retain the existing serial evidence contract; old
publications, event streams and seed templates are not rewritten. No live
workflow is opted in by this change. The same graph JSON crosses IPC and HTTP.

The policy has `maxReviews` (default 2, range 1–16), `maxRepairs` (default 1,
less than `maxReviews`), `maxStoryProposals` (default 16, range 0–100),
`requireRed`, `requireStrictClippy`, `nativeTargets`, `consumerScope` and
`changedTests`. Boolean defaults are false and list defaults are empty when
constructing the Rust default policy; a submitted policy must contain each
field. Only an explicit published policy can change these limits. A maximum
policy admits 16 review reports with at most 100 findings each; reports and
proof lists are bounded. Run-wide attempt, duration and activation limits still
apply. Counters derive from durable attempts and receipts: total reviews,
rejects, repair traversals and deterministic-check attempts remain separate
across restart and reviewer replacement. Exhaustion refuses another attempt;
it does not approve the work. The daemon pauses for an explicit decision.

For an opted-in graph, `review.evidence` contains:

- `subject`: a tagged `story` subject (`id`, `commit`, `tree`, `requirements`),
  or a reusable `pull_request` subject (`repository`, `number`, `base`, `head`,
  `tree`, `requirements`). Requirements are a SHA-256 digest; Git subjects use
  exact commit/tree IDs. A PR evidence verdict is not a native story transition.
- `findings`: stable IDs, integer `risk`, `value`, `effort` scores from 1–5,
  `addsMechanism`, `deletesOnly`, `scope`, optional `acceptedRisk` and `proposal`.
- `resolvedFixes`: earlier FIX IDs verified by the re-review. A replacement
  reviewer must resolve or repeat each earlier FIX.
- `proof`: `realityEffect`, `realityState`, `realityAssumptions`,
  `realityRadius`, optional `symptom` and `red`, `testChanges`, `checks`, and
  explicit `omissions`. A test change names `test` and
  `oldExpectationWrongBecause`. Every pinned changed test needs a rationale.

Triage uses these first-match rules: risk 5 → FIX; adds a mechanism → STORY
when value ≥3, otherwise ACCEPT; deletes only with effort ≤2 → FIX; risk 3–4
with effort ≤3 → FIX; value minus effort ≥2 → FIX; value ≥3 → STORY; otherwise
ACCEPT. Out-of-range or contradictory scores are unresolved. Only FIX findings
enter repair feedback. STORY needs a proposal whose explicit scope is contained
in the pinned consumer scope; it never creates work automatically. ACCEPT needs
named risk evidence. Publication reports retain these decisions and omissions.

A proof check names `id`, `subject`, `target`, `native`, `toolchain`, `argv`,
`configDigest`, `scope`, `matchedTests`, `exitCode` and `logDigest`. Final checks
must pass on the current subject. Nonempty test scope cannot pass with zero
matched tests. Every pinned consumer needs final coverage. Required native
targets need native evidence; Linux evidence cannot discharge Mac proof.
Required clippy evidence invokes `cargo clippy`, includes `--all-targets`, ends
with `-- -D warnings` and has no cap-lints override. RED names a pre-fix commit,
test and caught bug, with a failing check on that pre-fix subject and a SYMPTOM.
The native Judge verifies the actual pre-fix tree and ancestry against Git.
REALITY, TEST-CHANGE and omissions are typed records; the legacy note parser
extracts notes only and cannot turn prose into proof success.

The native Judge derives the current clean artifact and requirements from Git
and StoryStore, not from the submitted subject. Requirements hash the story ID,
title, problem, criteria and file scope; approval status, revision increments
and check ticks are not requirements. Host-executed required check receipts also
bind that digest. Any artifact or requirement change invalidates the evidence.
An independent reviewer cannot be any implementer of that story, including an
earlier generation. Native Review/Done, criterion and approval-transition
requirements remain in force. Required deterministic commands still execute in
the host check runner: reviewer proof, scores and accepted risk never bypass a
missing or failed required check. Typed proof describes the independent
reviewer's attestation; it does not claim that the host independently ran every
command named in that report. WF05 executor receipts remain a separate adapter
boundary. This change adds no PR execution or provider action.

## Durable timers (WF19)

Run-store layout 6 introduced Timer events in contract-3 runs; layout 7 retains
them in both contract-3 and contract-4 runs. Contract-2 serial
replay and the WF01 frozen inputs are unchanged. No Timer graph node or public
operator command is introduced. The existing internal `RunCommand::Timer`
service and pure reducer own scheduling, firing, retry and cancellation. Future
WF08, WF13 and WF03 consumers use this primitive, not another scheduler.

Each timer stores a stable key, owner scope, typed purpose, absolute Unix-ms due
and expiry times, delivery attempt, attempt budget, base/capped backoff and an
explicit expiry outcome. Keys cannot be reused, even after cancellation. A run
retains at most 4096 timers; each permits 1–100 deliveries. Backoff is positive,
capped at 86,400,000 ms, and doubles after each delivery. Provider retry hints
can lengthen that delay only up to the declared cap. Retries cannot extend the
absolute expiry. The final permitted delivery or absolute expiry records the
configured outcome instead of another retry wake. Budget and timestamps are
validated again during replay.

Purposes are `cohort_flush` (requires at least one row), `provider_retry` and
`question_deadline`. The timer is a durable **wake receipt**, not a proof result,
provider call or permission decision. WF08 can schedule a one-row cohort without
waiting for dispatch aggregation; WF13 consumes a wake with its provider adapter
and explicitly requests the next attempt after a retryable failure. WF03 question
timers require due time equal to expiry and an outcome of `timed_out` or
`incomplete`. Expiry never answers input, resumes work, accepts a story or grants
approval. Consumers deduplicate actual work by timer key and attempt and retain
their own durable effect receipts; a wake cannot justify replaying an uncertain
external write.

`service` scope names an independent owner (for example the proof-cohort service)
and continues while the run is paused, including restart recovery pauses.
`activation` scope pins execution and activation IDs. A pause keeps its absolute
due time but suspends delivery; resuming does not replenish time. A departed, completed or
superseded activation retires its timers; independent service timers remain owned
by their service. Run cancellation cancels every timer,
and terminal runs cannot fire or retry. Pending timers must fire or be explicitly
cancelled before run completion. Independent cohort ownership does not transfer
to a story child or imply a parent proof waiter; WF08 owns that subscription.

The sole existing daemon owner wakes its run actor at the earliest eligible
absolute deadline. Startup recovery keeps pending timers; reads never fire them.
Firing appends a payload-bound receipt, so a lost reply or second wake cannot
create another delivery. The actor waits on notifications/deadlines, not a
polling interval. Timer wakes during an adapter call retain the same pinned
adapter future rather than cancelling and restarting it. Existing check-process
timeout, run-cancellation and shutdown fences still own adapter teardown.
Internal consumers must emit the existing `workflow-run-changed` hint after
committing schedule/retry/cancel commands so the actor recomputes its deadline.
This foundation does not install provider polling, a proof collector or a question
inbox; their consumer stories remain responsible for those behaviors.

## Opt-in native integration (WF07)

Native integration is a typed `integrate` run action, not a shell command. The
per-repository `native_integration_enabled` policy defaults to false. Keep it
false through WF17 unless an isolated trial clone is under test or the specific
repository has explicit parity approval. `set_native_integration_policy` changes
admission only: disabling it does not remove receipts, pending recovery or debt.
The policy key is the canonical Git common directory, so linked worktrees share
one policy and one process-lifetime OS integration lock.

`integrate` takes `request` with `runId`, `storyId`, `commandId`,
`expectedSequence`, `expectedBase` and `expectedSource`. It requires the accepted
story revision, current source checks, clean checkouts and the pinned canonical
branch. It constructs a two-parent no-ff merge object, runs the published checks
in an isolated synthetic workspace, rechecks both parents, then fast-forwards
the canonical branch to that exact object. It does not construct a second merge
after validation. Publication refuses to overwrite ignored canonical files that the source starts tracking. A moved base refuses publication. Revalidation uses a new
command ID and fresh expected commits; the previous unmerged intent stays in
history. Repair-only records do not reserve the repository. A surviving synthetic validation ref is reused only at the pinned candidate. A published intent must recover under its original request.

The journal separates publication, proof-obligation outbox, canonical receipt,
dependency refresh and cleanup. Restart recovery first inspects refs. It resumes
only already-published candidates and never merges an unmerged intent by itself. An unavailable trial repository does not stop recovery for other repositories. Internal receipt replay is idempotent by command ID and does not depend on a saved operator-event cursor.
The outbox pins both parents, merge commit and exact changed-file scope. It is
WF07 debt, not WF08 proof completion or a release-ready result. Read it through
`integration_obligations`; read one saga through `integration {run_id,story_id}`.

Partial success is typed: `integrated_cleanup_pending` corresponds to exit 2,
`integrated_dependency_refresh_failed` to exit 3, and
`integrated_obligation_pending` to exit 4. None permits another merge. A missing
post-integration receipt or outbox entry remains recoverable debt. A failed
`pnpm install --offline --frozen-lockfile` keeps the merge and retries only the
refresh phase. Post-integration checks bind the real canonical ref and commit;
out-of-band ref or tree movement invalidates the existing freshness rules. Artifact inspection and check-setup errors after publication return durable obligation-pending outcomes, not failed-merge results.

`cleanup_integration {run_id,story_id}` is a separate owning-daemon action. It
proves integration, the exact source tip, native run/successor ownership, durable
managed-process closure and absence of live or suspended workspace users before
using TUIC's non-force worktree removal. The exact source tip is archived under
`refs/archive/<branch>` before branch retirement. Dirty files, including
managed-looking generated files and nested submodule dirt, keep the workspace;
this action never restores, stashes or discards them. Ignored work also keeps the workspace until it has explicit disposal attribution.
If checkout removal finishes before branch retirement, replay can retire the remaining branch only with the exact source tip and archive, through the same TUIC Git deletion boundary. Native successor assignment and managed launch/close stay fenced through removal. Cleanup failure does not revoke integration.

Code conflicts return `repair_required`. Append-only union is allowed only for
explicit repository-relative Markdown paths in `append_only_documents` on the
policy action, with no deletions from either parent relative to their common
ancestor. Nested, incomplete and diff3 conflict markers refuse union. The legacy
`record_integration` action remains a verifier of an already-performed clean
merge and does not gain this union policy.

## Branch-local questions and decision inbox (WF03)

Contract 5 turns a typed `needs_input` report into one atomic `question_requested`
event. The event retains the report, suspends its exact graph activation, and
records a revisioned question. Other graph branches and disjoint story executions
can continue within the existing resource and file-scope limits. A waiting worker
still owns its slot and file scope. A question never completes the worker or its
all-Join arrival. Explicit operator pause/cancel and uncertain-effect safety stops
remain run-wide; an answer does not clear such a pause.

`inputRequest` accepts optional `answerSchema` and `deadline`. The schema is tagged
by `type`: `text`, `boolean`, `single_choice {options}`, or
`multiple_choice {options}`. Without a schema, nonempty legacy `options` mean one
exact choice; otherwise the answer is nonempty text. The backend rejects invalid
choices, duplicate selections and wrong JSON types. A deadline is
`{atMs,outcome}`, where outcome is `timed_out` or `incomplete`, never approval.
Its service-owned WF19 timer is created in the same event transaction and survives
branch suspension, explicit pause and reconnect. Expiry leaves the activation
suspended and the unresolved question visible. A new revision is required before
answering an expired question.

Commands use the existing `command` action and expected run sequence:

- `answer_question {question_id,expected_revision,subject_digest,actor,answer}`
- `revise_question {question_id,expected_revision,request,actor}`

`actor` is `{kind,id}` with `human`, `ai` or `daemon` provenance. It describes the
source; it is not a new authorization boundary. The question records its requester,
exact subject digest and answer actor/time/value. The backend computes the subject
from run/attempt/generation, current story or plan revision, and the assigned Git
workspace. Workspace binding includes HEAD, staged and working changes, and
untracked file bytes. An unreadable subject refuses the decision rather than
assuming freshness. A revision refreshes the subject and schema explicitly;
it does not grant consent. Superseded fork generations retire their questions and
deadlines. Run cancellation withdraws all unanswered questions.

The first valid answer transaction wins. Reusing a command ID with identical
payload returns its original receipt; a changed payload, stale run sequence,
question revision or subject refuses. The legacy `answer_input` command is refused
for contract 5, so it cannot bypass the revision fence. A valid answer restores
only its activation and uses WF04 generation-bound continuation delivery to the
same worker. A fresh delivered-generation report is still required for completion.

`question_inbox` is a read-only project action. It returns
`{type:"question_inbox",value:{unansweredCount,items}}`; each item contains
`runId`, `runStatus`, `storyId` and the complete question. Pending, timed-out and
incomplete questions count; answered, superseded and canceled questions do not.
The projection reads persisted runs, requires no coordinator or runtime actor, and
is identical through IPC `workflow_run_action`, HTTP `/workflows/run/action` and
MCP `workflow action=run`. Reconnect must reread the projection; run-change events
remain cursor wake hints. The Stories answer form sends the revision and subject
for new runs and retains the old input command for historical runs. Expired
questions remain read-only records and do not hide the next pending question.
The form shows choices from the current structured schema; only historical
requests use legacy option suggestions. Boolean and multiple-choice values can
be entered as JSON in that existing form.


## Pinned reusable subflows (WF09)

Contract 6 adds same-daemon `call` and terminal `return` nodes. Contracts 2–5
retain their replay rules. Run-store schema 12 refuses older binaries before they
can recover a new Call ledger.

Story Dispatch remains the only story-delivery fan-out. A Call targets an exact
published **story-kind reusable decision/check definition**, not an independent
story assignment. Its child is a nested `GraphExecution` in the same run, with a
stable hashed ID bound to the parent execution and activation. It shares the
existing scheduler, duration, loop, command and cancellation ownership. No new
cross-daemon placement or second scheduler is introduced.

Publication resolves the complete definition closure, including Story Dispatch
and nested Calls. It rejects recursive paths, more than four Call levels and
more than 32 expanded calls (including conservative root-loop expansion).
Reusable graphs allow Start, If/else, Run command, Tool, Notify, Call and Return;
they cannot contain Agents, delivery, loops, parallel regions, Pause or End.
Return marks the reusable form in the existing story-kind publication namespace;
it has no implicit story final-check/evidence policy. Checks execute as explicit
Run command/Tool nodes and bind their actual receipts before a Return path.
Ordinary story-delivery publications still require their final checks.
A run also admits at most 32 Call children. Each graph retains its existing
512-activation limit. The 33 Call/root executions therefore reserve at most
16,896 activation slots; separately dispatched stories retain existing admission
limits. These are storage/admission bounds, not evidence of process completion.

A Call declares `definition_id`, `revision`, up to 16 nullable boolean `inputs`
and up to 16 unique `outputs`. Input names exactly match the reusable graph's
unbound If/else input declarations. Each Return declares the same output names
and one of `completed`, `blocked`, `failed`, `cancelled`, `uncertain`. The parent
must publish a distinct edge for each outcome. A typed Return can carry an
unknown boolean; unknown is not success. Return definitions cannot start as
standalone delivery roots or through Story Dispatch.

Root start persists every transitive publication and the imported routing policy.
A later dispatch or Call reads these persisted pins, never the latest draft or
policy. Each delivery publication includes its exact repository checks, closure and
evidence policy; each reusable definition pins its explicit command/check policy. Only explicitly imported routing policy is a routing grant;
absence of a publication-authority import does not authorize publication.

A child start is not a completion receipt. The reducer permits parent completion
only after the exact child reaches Return and supplies the exact typed outcome
and receipt reference. Reopen/retry reuses the same child identity. Receipts retain
a set of accounting execution IDs: use set union, never add an inclusive parent
and its descendants twice. Provider money/tokens remain **unknown**, not zero;
WF12 owns ingestion of the absent usage ledger.

Parent cancellation marks live nested bindings `stopRequested`, fences further
scheduling and requests stop through the existing run-owned command process-tree
registry. This is a request, not fabricated stop proof or a successful Return.
Uncertain effects remain in the run ledger for inspection. Run-wide pause/failure uses the existing shared run scope: it admits no new
child work or Return transitions until explicit resume. An already running owned
command can settle its journal receipt while paused; pause does not claim to
stop it. Explicit cancellation requests process-tree stop. A child terminal
failure follows its declared parent edge. The independent integration outbox is not deleted by cancellation.
WF08 remains the owner of post-land proof execution and is not implemented by
Call. Full implement/critic/repair/integrate/proof-batch end-to-end evidence still
requires that service; the WF09 regressions cover pinned nesting, typed outcomes,
replay, retry and real-Git proof-debt retention.
