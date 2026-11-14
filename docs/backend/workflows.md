# Workflow definitions

The internal neutral runtime has a typed HTTP adapter with pinned installed
connector contracts, separate network/write grants and bounded cursor results for
collections. See [HTTP connector contracts](workflow-http-connectors.md). Native
host composes configured families and binds authenticated public callers before
dispatch. Operational cutover remains staged. The installed
[Telegram connector](workflow-telegram-connectors.md) adds typed send/update/reply
operations through the existing worker and durable exact-revision callbacks.

## Trusted public native starts and answers

Desktop IPC supplies authenticated Owner identity. HTTP supplies Owner only for
loopback or verified user credentials; transport admission alone supplies none.
MCP preserves the actual live managed peer as Agent, including ACP generation
and unsettled-connection checks. Request JSON has no caller or capability field.
The installed host captures the canonical checkout identity before persisting a
new start and binds the same provider before wake. A staged candidate is allowed;
review/commit services still check the exact parent/tree/diff. Coarse agent launch
continues to require a clean baseline. Starts and exact retries serialize through
the existing owner; a retry cannot replace the caller, workspace, captured rights
or check definitions. Missing/revoked backend capability bindings fail closed.

Verification check names come only from published constant `request` bindings on
`native.verification.run` in the retained root/children, resolved by the installed
host. Dynamic check requests and names supplied only in start inputs are refused.
After restart, volatile native bindings require trusted backend rebind; no caller
metadata or grant registry is migrated into the run database.

Authenticated Owner may answer any bound native run. Agent may answer only its
own run with both captured and current Operator authority. The ledger still
checks the exact effect key, question revision and command sequence. Existing
non-native host answers retain their behavior. HTTP/IPC/MCP wrappers share this
backend boundary; live transport parity and operational cutover require their
separate acceptance evidence.

## Neutral workflow transport (015)

Desktop `workflow_neutral_action {project, action}`, HTTP
`POST /workflows/neutral/action?path=<project>` (body is the action), and MCP
`workflow {"action":"neutral","input":<action>}` call one backend service.
Replies are `{type,value}`. Neutral fields remain snake_case; no transport
converts mapping keys, result casing, unknown data or effect uncertainty.
MCP takes the project from its live managed PTY/ACP binding and requires a local
or authenticated caller. Request fields never grant service authority.

Actions: `catalog`, `templates`, `create_draft`, `list_drafts`, `get_draft`, `update_draft`,
`validate`, `preview`, `publish`, `get_publication`, `clone_publication`,
`save_operation`, `save_preset`, `get_preset`, `start`, `inspect`, `events`,
`cancel`, `answer`, `start_delivery`, `delivery_runs`. The MCP input schema derives from the same `NeutralAction`
used by HTTP and desktop. Internal effect observation/completion commands are
not public variants. Neutral triggers remain a 010 handoff; legacy bindings keep
their existing transports.

### Plan and story delivery workflow

`run/plan_delivery.rs` builds this workflow only from neutral catalog nodes.
No legacy definition is converted. The first `start_delivery {story, key}`
installs it in the project: four saved script operations (`delivery/implement`,
`delivery/verify`, `delivery/review`, `delivery/deliver`), the published
subflow `delivery-attempt` (implement, verify, review) and the root draft
"Plan and story delivery", which it publishes. The Designer edits that draft
like any other; an installed and published draft is never overwritten.

The root runs the attempt in a bounded loop (`correction`, three attempts).
The loop repeats while the pair `{passed, approved}` from the verification exit
status and the review verdict is one of the rejected pairs. The next attempt
receives the findings of the previous one. Only a complete loop reaches
`deliver`, which commits the workspace once and ends `delivered`. An exhausted,
unknown or failed loop ends `blocked` without a commit. The implementer and the
reviewer get the prompt on stdin of the instance `command` (default
`ego run --sandbox workspace -` and `ego run --sandbox ro -`; the reviewer
approves with `APPROVE` on its first line). The default verification command
is only `git diff --check HEAD`: set the project check command in the Designer.
Each implement step records the paths whose Git status or content changed
during its agent call; the loop carries their union. The deliver step stages
only those paths, so an untracked file that was there before the run stays out
of the commit.

The run input `story` is a backend snapshot of the story (id, title, problem,
criteria, plan). The request id is `story-delivery:<story>:<key>`;
`delivery_runs {story}` lists the story's runs newest first with `run_id`,
`key`, `status` and `bound`. Plans and Stories shows them under the story.

Caller provenance is volatile. After a restart the native host has no binding
for a running run, so the actor waits (`NeutralHost::bound`) instead of failing
its next step. `start_delivery` with the same key re-issues the retained
request, rebinds the caller and wakes the run at its recorded position. A
settled effect is never invoked again; an effect without an observation
reconciles as uncertain.

Drafts wrap the existing `NeutralDefinition` with `id`, `project`, `name`,
`revision`, `latest_published_revision` and optional presentation `editor`.
Create takes name/definition/editor. Update additionally takes id and
expected_revision. `editor` has `labels` and `presets` string maps keyed by
control-node ID, both defaulting empty. Presentation is stored beside each
publication; definition hashes, pins and admission never read it. Publish
checks the draft revision inside the publication transaction, retains the
presentation atomically, and refuses unchanged republishing. Exact publication
read/clone preserve the internal subflow graph and immutable pins.

Catalog lists authoritative activity descriptors and pinned metanode publication
views. A production native adapter installs the same host Arc for authoring and
the existing execution owner. Without installation, catalog/publication/preview
and new execution refuse with `native catalog not installed`. Draft editing and
retained run inspection remain available. This transport is staged; it does not
replace the scheduler or activate operational cutover.

The installed `NativeHost` supplies the eight native descriptors and delegates
the closed set of script, HTTP, model, Jira, GitLab and Telegram adapters through
that same provider. Saved script and typed HTTP targets come from the existing
project reusable-operation store. Jira/GitLab descriptors require installed
plugin configuration matching core account metadata; GitLab additionally needs
the plugin's service-observed `resolvedProject {path,id}` for its current path.
Discovery reads local metadata, never credentials or a remote project lookup.
Telegram requires its enabled configured alias. Model requests require the
configured absolute executable and exact nonempty profile; model and effort are
explicit request fields, with no fallback. Its literal `request` contains the
existing model request contract; `tools` retains exact bounded host operations,
and structured output is returned under the descriptor's `result` object.

Absent configuration is omitted from the catalog. Unknown/changed adapters
refuse dispatch. Authenticated owners may use installed configuration; agents
need Operator in both their captured ceiling and current backend capability set
for script/model/connector execution. HTTP and Jira/GitLab mutations also require
the exact ledger write approval. Paid model calls require Operator without a
separate per-call approval. Native Push authority stays separate. Current
configuration and authority are rechecked immediately before delegation.

Validation returns `{valid,diagnostics:[{path,code,message}]}` with JSON Pointers
for incompatible data links. Preview takes definition/inputs/subject and returns
validation, per-activity inputs as existing `Datum` values, output schemas,
outcomes, optional pure effect preparation and an unavailable reason. Unknown
producer/scoped samples remain unknown. Preview performs bounded publication and
resolved-value checks, then prepares intent without requiring a run binding.
Its synthetic fence never invokes an effect or writes a run/receipt and supplies
no dispatch authority. A displayed unbound approval deadline is only intent;
runtime preparation retains the bound run's actual deadline.

Start takes the exact `NeutralStartRequest` (publication, request_id, inputs,
subject and limits). Inspect takes run_id/after_sequence/limit (1..500), returning
`{snapshot,events}` from one retained read snapshot. Status is tagged `type`:
running, completed (outcome), cancelled, expired, limited (reason), failed
(diagnostic). Outputs, waits, failures, uncertainty, effect questions and nested
execution boundaries come from the retained owner. Events use the existing
NeutralEvent snake_case envelope. Cancel and answer require command_id and
expected_sequence; answer also binds the exact effect key and question revision.

Every committed neutral start/decision dual-emits the existing
`workflow-run-changed` invalidation: desktop and `/events` SSE receive
`{repo_path,payload:{runId,sequence}}`. Low-frequency invalidation reuses the
existing event bridge; no token stream or extra WS is needed. Clients re-read
inspect/events by sequence after notices and reconnect, including final results
or effect settlement after a run ended. A label or model selection is never an
approval. Backend/native providers retain current rights and original-effect
reconciliation responsibility.


Slice F exposes `start_graph {target:{type:story|plan,id},expected_revision?,definition_id,definition_revision,request_id,limits?,secrets?}` through the existing owning-daemon service. `secrets` is a map of environment names to values for the agent children of this run only (PTY spawn environment and ACP connection environment). The values stay in daemon memory: run state, events, receipts, prompts, the launch journal and logs hold the sorted names only (`secret_names` on the run snapshot). `TUIC_SESSION` and `TUIC_PARENT` are refused. Completion, cancellation and a daemon restart drop the binding; a later agent launch of a run whose binding is gone fails before spawn and pauses the run. Story starts require the current native revision and pin the selected publication; the request ID is bound to its payload. Omitted limits use Rust defaults. Public plan starts use the same graph-root entry point and slice E dispatch. The Plans and Stories dialog has no plan start control; its selector handles stories only. The `workflow action=run` MCP entry uses an inline schema generated from `RunAction` and the public `RunCommand` variants; it returns the same scoped snapshots and cursor-ordered events as IPC/HTTP. Run history shows pinned activations, decisions/evidence, repair counters, pause targets and complete event payloads across pages. Graph recovery uses `resume_graph {execution_id,activation_id,resolution}` after answering pending input; pause and cancel use the existing sequence-fenced commands. Legacy runs offer inspection and cancellation in the UI. No new persistence or client scheduler is added.

Workflow definitions are owned by a canonical project path and stored in `workflows.sqlite3` under the TUICommander configuration directory. A definition has a mutable draft revision and immutable published revisions. Editing a draft uses an expected revision; publishing validates the graph, resolves pinned story-template revisions, and inserts a new published row in one SQLite transaction. A run must pin a published revision; this definition layer does not execute nodes.

Each definition has a `closure` policy. Existing records migrate to `human`, and new drafts default to `human`; this legacy policy name means an explicit approval transition, which may be performed by any trusted caller. `update_closure` edits a draft with an expected revision. `automatic` publishes only for a story delivery graph that has a Gate node whose `pass` edge leads to the Judge and a pinned `evidencePolicy`. No person moves an automatic story to Review: when the implementer reports completed and the story is still in progress at the reported revision, the run checks exactly the non-manual criteria that report proved and submits the story, as the managed actor `workflow:<implementer attempt id>` in the story transition history. It does this before the reviewer starts, so the review binds the submitted revision. A retried turn resumes a submission that a lost response interrupted only when every later story transition is that actor's own criterion check. The run never checks a `[MANUAL]` criterion; such a story moves to QA, and the run pauses with `Story <id> awaits manual QA` before any reviewer starts. The Judge then closes the story only when the gate passes and the reviewer's typed evidence validates (GREEN checks, REALITY); a story with a `RED:` criterion also needs RED proof. Under automatic closure the daemon, not the reviewer, runs the RED test: the reviewer's RED command must extend a pinned `requiredChecks` command, and the daemon runs it in a detached checkout of the pre-fix commit. It records a durable `RedReceiptRecorded` receipt (argv, commit, tree, exit code, count of output lines naming the test, log digest). The Judge replaces the reviewer's reported exit code, match count and log digest with the receipt; a missing receipt, a zero exit or no matching line leaves the story in Review. A failed gate or missing proof leaves the story in Review with the reason on the run. Published revisions retain their pinned closure policy.

A definition also pins `requiredChecks`: up to 16 check commands, each with a stable ID, an `argv` array, and a timeout of 1–600 seconds. `update_checks` edits the draft with an expected revision; publication stores the exact set on that immutable revision. Commands invoke executables directly, without a shell. `execute_check` runs a pinned check in the assigned, clean story worktree and appends a durable `CheckRecorded` event with command, exit code, branch ref, commit/tree digest, and duration. A failed command also receives a receipt, but it cannot authorize integration. A changed worktree or ref makes the receipt stale.

The graph contains stable node IDs and directed edges. Node kinds are Start, Agent, Create Stories, Story Dispatch, Judge, Gate, Pause, Loop, exclusive Join, Fork, all-Join, For each, If/else, Notify, and End. An Agent carries a role, prompt template, and a bounded list of supported capabilities. Story Dispatch is a plan-only node that pins a published story definition ID and revision in the same project. Create Stories is plan-only. Join is story-only, with at least two incoming branches. A Loop is the only source of a back edge and requires `max_iterations` from 1 through 100 plus `repeat` and `exhausted` edges. Judge requires `yes`, `no`, and `uncertain`; Gate requires `pass` and `fail`; Story Dispatch requires `completed` and `blocked`. Invalid references, duplicate or unreachable nodes, missing outcomes, and other cycles are rejected on publish. A node is reachable through an edge or as the `resume_to` target of a reachable Pause. The optional `layout` object maps node IDs to Designer canvas positions (`{x, y}`, unsigned 16-bit). It is omitted when empty and execution never reads it.

The first list or explicit seed action inserts `Story delivery` and `Resolve plan` once per project. The latter pins revision 1 of the former. Fresh story and plan seeds both pin the repository-integrity check (`git fsck --connectivity-only --no-progress`, 300-second timeout), so their drafts can be published and their runs perform deterministic final verification. Projects can replace this baseline with their own checks in a new revision; clearing every check still prevents executable publication. Both remain editable as drafts; a new published revision does not rewrite an older one.

Desktop IPC command `workflow_definition_action` and HTTP `POST /workflows/definition/action?path=<project>` call the same Rust service. Actions: `seed_templates`, `create_draft`, `list_drafts`, `get_draft`, `update_draft`, `update_closure`, `update_checks`, `publish`, `get_published`, `save_file`, `load_file`. The MCP `workflow action=definition` entry exposes the two file actions to the calling managed session's project. `update_closure` and `update_checks` have no Designer control. The action is a JSON object tagged by `action`; replies are tagged `{ type, value }`. The Plans and Stories dialog has a Designer tab for draft selection, pointer drag (mouse, pen, touch) or button-based node addition at canvas positions, Author presets, For each and Splitter regions, keyboard or pointer node moves, outcome-link editing, node inspection, saving and publishing. The backend remains the authority for graph validation at publication.

## Structured script adapter (007-7ce3, staged)

The internal [script activity contract](workflow-script-interface.md) uses the
existing owned-process executor and neutral effect ledger. It validates a
versioned fenced JSON result, keeps stderr separate, bounds execution/IO and
retains mutation uncertainty across loss and cancellation. Custom descriptors
remain generic catalog data. Production host installation, neutral public
transports and Designer rendering remain their respective consumer stories.

## Neutral execution ledger (004-2809, staged)

The internal neutral ledger records control decisions, prepared activity inputs
and subject, validated immutable results, condition provenance, absolute wait
deadlines, cancellation, duration expiry and activation-limit exhaustion.
Commands use the same RuntimeOwner lock and RunStore writer transaction, with
expected sequence and exact command-payload identity. Inspection replays retained
events without the current catalog or a story backend; a duplicate command returns
its original receipt projection. A completed result cannot be replaced, and a
terminal run rejects late completion. Checked timestamp addition rejects overflow.

The authored owner integration adds collection item, fork branch and loop
iteration fences, immutable settled scope outputs, and retained subflow frames.
The existing actor selects the persisted contract before any legacy scheduler,
backend gate, health check or effect path. Neutral restart enumeration shares the
same owner and mailbox. Startup enumerates neutral identities without replaying
all histories; each actor verifies its own run, so one corrupt payload does not
block healthy recovery. Every ordinary actor return releases its mailbox entry.
Only dispatched unresolved ancestor work consumes nested execution capacity;
prepared siblings cannot block their active child frame. Storage errors leave the
cursor retryable; only deterministic Step decision errors become run failures.
Each actual invocation requires a current backend context
and exact host registry, including resolved child arguments and non-amplifying
subflow context derivation. No context or grant is stored in the event stream.
Missing host services refuse execution rather than bypassing their invariants.

Run-store schema 21 prevents an older owner from ignoring execution decisions;
existing rows are not migrated. Focused runtime validation passed, including
corrupt-history isolation, contract-read error recovery, nested collection/subflow
execution and refused SQLite-write retry. This internal integration does **not**
enable a public neutral start or operational cutover. Preparation is not admission or a dispatch permit. A persisted dispatch
claim is never automatically invoked again after restart. Full effect correlation,
approval, uncertainty resolution and native stop/resource observation belong to
005; real native service adapters and freshness evidence remain required by 008.

## Neutral run persistence (004-2809, staged)

The neutral runtime contract is under implementation, not an operational cutover.
Its internal start boundary belongs to the existing `RuntimeOwner` lock and
`RunStore` database. Run-store schema 20 adds `neutral_workflow_runs` and
`neutral_workflow_events`; it does not convert legacy runs or invent their
execution positions. Older builds must refuse this database version.

A start retains the verified root and descendant publication bodies, the union
of exact descriptor bodies (including schemas and handler versions), inline
configuration/prompt text, normalized typed inputs, optional subject, and generic
activation/concurrency/duration limits. The first sequenced event is immutable
and content-checked on inspection. No plan, story, review counter or compiler
policy is needed. An exact project/request-ID retry returns its original run
without consulting today's catalog or handler registry; changed arguments refuse.
The immediate transaction rechecks that identity before inserting the run/event.

The original 004A persistence slice did not dispatch activities or wake an
actor, and stored no host context or grant. The subsequent 004D owner integration
above now supplies control-flow routing, resolved per-activity admission,
activation/attempt fences, durable scopes and restart-safe scheduling. Current
authority and handler availability are checked at each actual host-port dispatch;
retained bodies are never permits. No new IPC/HTTP/MCP start surface is exposed
by this internal integration.

## Legacy engine removed (3034-365e)

The role/kind graph engine, its definitions, runs, bindings, host routing policy,
migration converter and run history are deleted. Only the neutral engine and the
automations prompt scheduler remain. On the first start of this build each store
writes one backup next to itself (`<file>.legacy-workflows.bak`, SQLite
`VACUUM INTO`; `automations.json.legacy-workflows.bak` for the JSON file), then
drops the legacy data:

| Store | Removed | Kept |
|---|---|---|
| `workflows.sqlite3` | every `workflow_*` table and `neutral_migration_conversions` (user_version 1) | neutral drafts, publications, presets, reusable operations, neutral trigger bindings |
| `workflow_runs.sqlite3` | every `workflow_*` table and the `commands`, `command_observations`, `command_resources` journal (user_version 23) | `neutral_*` and `native_*` tables |
| `automation_runs.sqlite3` | review heads, event keys, event decisions and every run whose trigger is not `scheduled` or `manual` (user_version 2) | prompt schedule runs and their deliveries |
| `automations.json` | `pull_request_reviews`, `event_intake`, `triage_executor` and the definitions only those engines could start | prompt schedules |

An existing backup is kept, so a later start never overwrites the state taken
before the first removal. No live legacy run survives the upgrade.

## Neutral reusable operation contracts (002-eeb6)

The internal catalog supports host-owned native descriptors and project-owned
reusable script/subflow descriptors. Each descriptor declares its typed config,
inputs, outputs, subject, named outcomes, exact handler name/version and execution
policies. Instance configuration is data, not a second schema language.

`WorkflowStore::save_reusable_operation` appends immutable versions in the existing
workflow database. Creation expects version 0; updates name the current version
and insert its successor. Exact-version reads verify row/document identity,
content, target interface and current handler availability. Native descriptors
cannot be saved through this user-operation seam. Save also refuses an ID already
present in the supplied catalog; exact verified reads may reuse an identical
existing operation but reject a conflicting body.

The backend-owned `HostRegistry` binds exact handler versions to Script or Subflow
through `register_reusable_handler`. A descriptor cannot select an available
native, wrong-kind or unregistered handler version as a reusable target. Save,
read, project-catalog construction and publication/dispatch preflight enforce
that binding before invoking an adapter. This is a host routing invariant, not
permission supplied by descriptor capability strings or workflow JSON.

`build_project_catalog` combines the host catalog with verified stored operation
versions for one project. It refuses every ID collision with a host entry, loads
all operation descriptors before resolving child interfaces, and then verifies
their complete targets. Thus subflow dependencies need not follow row-sort order,
and old versions remain available for publication pins. It is an internal
store-backed consumer, not a public route, runtime adapter or plugin loader.

A script target contains the existing version 1 explicit shell invocation with a
nonempty interpreter and inline source, each at most 32768 bytes and without NUL.
Descriptor content pins cover the source. The exact trusted handler name/version
is the handler pin; interpreter paths are not executable-byte attestations.
External script/prompt asset resolution is not implemented by this contract.

A subflow target names an exact same-project publication revision and content
hash. Its descriptor IO, subject and named outcomes must equal the child interface;
explicit mappings remain a separate data-contract concern. Publication closure
includes these target edges and checks every interface and pin, with the existing
depth 8 and 32 expanded-edge limits. Later child revisions do not retarget an
existing operation or publication. Neutral publication format 11 retains a required
explicit control graph to the correlated fork/join scopes, return bindings,
typed loop-carried results, bounded collections, named conditions, instance input
bindings and declared outputs/outcomes. Earlier internal formats are refused,
not migrated.

The output validator rejects unknown outcomes, missing required fields and wrong
types. It does not run a process or advance a graph. Root dispatch preflight also
refuses descriptor-backed subflows until child arguments can be resolved. Saved
operations, presets and pins grant no permission. Runtime effect-boundary checks,
script execution, Designer and transport authoring are not delivered
by this internal slice. Explicit data projections are described below. Inline prompt text remains content-pinned; arbitrary
handler interpretation of configuration strings is not proved safe by storage.


## Neutral outcome control declarations (004-2809, staged)

`control.nodes` maps stable node IDs to start, end, activity, condition,
collection, fork, loop or timer-wait declarations. `control.edges` maps each
source node's named outcomes to target node IDs. Publication and verified read
require exactly one start, reachable nodes, declared end outcomes, and a complete
acyclic ordinary graph. Repetition belongs only to declared bounded scopes.
Each instance and scope has exactly one control owner; a scoped body cannot also
be dispatched as an ordinary graph activity.

Activity ports come from the pinned descriptor, not a kernel business-outcome
switch. Conditions require `true`, `false`, and `unknown` edges. Collections and
forks require `complete` and `blocked`; loops require `complete`, `exhausted`,
`unknown`, and `failed`. A loop evaluates its declared condition after a
completed body traversal. Timer waits use the existing workflow duration ceiling:
1 through 604,800,000 milliseconds (seven days), with a `next` edge. The same
shared ceiling bounds neutral run duration; the existing duration limit in seconds
is unchanged. Values such as `i64::MAX` are not useful bounded waits and refuse
publication. Actual timer scheduling must also refuse absolute-deadline overflow
with checked arithmetic before persistence. End nodes name declared workflow outcomes and have no outgoing edge.
Missing or extra ports, unknown targets, ordinary cycles and omitted declarations
refuse publication.

These declarations are not a second scheduler or a dispatch permit. Actual
same-owner traversal, resolved host admission, durable results/scopes/timers and
restart fences remain under implementation in 004. Test-only data-contract
fixtures author explicit control declarations before calling the same publication
validator; no implicit graph or old-format runtime fallback exists in production.

## Explicit scope output sources (004-2809, internal)

Publication format 11 adds `scope_output { scope, field }` to the ordinary data
source contract. Save and verified read derive scope schemas before checking
mappings, conditions and workflow returns. Collections expose integer counts,
`state`, and ordered `items`; forks expose `state` and a `branches` object of
arrival states; loops expose their declared typed carried fields. Unknown scopes,
unknown fields and incompatible types refuse. Collection sources and loop initial
bindings can read earlier scope results. Dependency checks expand a scope source
to every body producer, including all fork branches, so own-scope and cross-scope
cycles refuse. Only explicit loop result bindings cross an iteration boundary.

Resolution receives a separate immutable scope snapshot context, never synthetic
activity outputs or root-input substitution. A scope with no settled snapshot is
Unknown, not false or a default. An omitted optional carried field is Absent,
not null. Settled collection items retain source order, state, named outcome,
present output values and explicit absent field names. Failed and skipped items
retain their diagnostic or reason instead of fabricated success output. Scope
and underlying data provenance remain data, not authority. Pending collection or
fork folds cannot produce a settled scope snapshot.

## Nested projection and object sources (003 extension)

Two data sources compose native requests from runtime receipts. Bindings, loop
and collection inputs, and conditions share them; there are no expressions,
functions, defaults or arithmetic.

- `projection { from, pointer, value_type }` reads an RFC 6901 pointer inside one
  `workflow_input`, `activity_output` or `scope_output` field of type object or
  array. Nested values are opaque in the shallow schema, so the projection
  declares its type; publication checks it against the target like any other
  source. At run time a missing path is Absent (a required target then refuses),
  and a present value of another type is Unknown: a binding refuses and a
  condition decides `unknown`. Provenance is the base field provenance.
- `object { fields }` builds an object from named member sources. An Unknown
  member makes the object Unknown, Absent members are omitted, and provenance is
  the union of member provenance.

Dependency, self-dependency and collection-body checks expand both variants to
their base sources. A loop result must still be a direct body output. The
variants are additive inside publication format 11: existing publications never
contain them and parse unchanged. Example: a condition compares
`{"source":"projection","from":{"source":"activity_output","activity":1,"field":"receipt"},"pointer":"/assessment/decision/outcome","value_type":"string"}`
with the constant `"accepted"`. The Designer edits both variants as JSON.

These are source/schema/value contracts, not durable scheduling proof. The
single run owner must still check current run/scope/generation and item
activation/attempt before publishing a snapshot, persist it, and fence release
across restart. Fork arrival-state projection does not invent activity outputs
that the join fold has not retained. No runtime dispatch, migration, cutover or
external effect is enabled by this slice.

## Neutral input projection contracts (003-e678, internal)

A neutral definition may declare `input_bindings`, keyed by the instance ordinal
in its immutable published vector. Each target field explicitly selects a workflow
input, another instance output field, an explicit scope output field, or a JSON constant. This is a top-level
projection, not an expression language. Save and verified read reject unknown
source/target fields, missing activities, self references, incompatible types and
required targets without a guaranteed source/target default, and ordinary data
dependency cycles (including indirect cycles). Integer data may feed a
number field. Bindings and constants are covered by the publication content pin.

Resolution uses immutable producer snapshots, never completion order. A producer
without a result is unknown and cannot be replaced by a default. A completed
producer may omit an optional field (absent); a target default then applies. A
producer schema default does not guarantee an optional output at this snapshot
boundary, so it cannot satisfy a required target with no target-owned default.
Workflow input defaults are resolved before projection and do supply a guarantee.
Explicit JSON null and empty arrays remain present values: null is accepted only by an explicit `null` field type, never as an absent
required field, while an empty array remains empty. Null defaults are refused
because the current stored `Option<Value>` default field uses null for no default;
bind an explicit null instead. Projection
preserves supplied provenance and adds its declared source. Provenance is data,
not authority or evidence of a trusted source.

The root-only admission preflight resolves workflow/constant projections before
calling host ports. Upstream-output projections remain unknown without producer
results; it cannot admit them by reusing root arguments. This is not an executor,
subflow invocation, collection scheduler or live authority proof. Concrete runtime
snapshots, every-dispatch checks and sensitive sink admission remain required by
004/005/008. The 003 collection and structured-return contracts are pure data
contracts, not proof of their runtime execution. No public route or Designer
action is added here.


### Closed typed conditions

Named condition contracts share the mapping sources and provenance. Publication
and verified read validate all source fields and operand types. The closed
operators are `exists`, `compare` (equal, less, less-or-equal, greater and
greater-or-equal) and array `contains`. Ordered comparisons accept numbers or
strings only. Scalar numeric comparisons preserve adjacent large integers rather
than rounding both through floating point; mixed integer/number comparisons
compare their numeric value. Other equalities use typed JSON structural equality.
Array membership does not coerce strings to numbers; its elements remain opaque
under the shallow array schema. No code/expression operator is accepted.

Evaluation returns true, false or unknown plus the union of input provenance.
`exists` is false for a known absent field, unknown for a pending producer, and
true for present null, false, an empty string or an empty collection. Other
operators return unknown for absent or pending operands, never false. Runtime
branch routing and unknown/error handling remain the responsibility of 004; these
internal contracts do not schedule any successor or confer authority.


### Bounded collection contracts

Internal neutral definitions declare named `collections`. Each ForEach binds an
array from a workflow input, a constant or any activity output. It declares the
item type, body instance ordinal, distinct item/index input fields, positive
`max_items` and `max_concurrency` (not greater than `max_items`), empty-input
behavior (`complete` or `blocked`) and partial-failure policy (`collect` or
`blocked`). Publication checks the source type, constant items and limits, scoped
body ports, conflicting ordinary bindings and ordinary dependency cycles that
include collection sources. One body instance belongs to one collection scope.

Expansion validates the complete array before producing any items. Unknown,
absent and null sources are not empty arrays. Duplicate values remain separate
items, identified by run, scope, generation and source index. Item/index values
retain source provenance. The execution owner must retain that immutable source
snapshot across replay; a new traversal has a new generation.

Aggregation folds an immutable result snapshot into source order, never completion
order. Pending, completed, failed and skipped items remain explicit. Optional
omitted output fields remain absent, not null. Typed output/outcome validation
rejects malformed completed items. Foreign run/scope/generation, duplicate item
results and out-of-range indices refuse. A missing result leaves the aggregate
pending. These are deterministic data contracts, not callback admission or a
scheduler: durable activation/attempt fences and join release belong to the
execution owner. No API pagination, external service execution or live authority
has been proved by these contracts.


### Typed loop state and structured returns

A loop contract declares its body instance, positive `max_iterations`, carried
object schema, initial projections and current-body result projections. Carried
fields are scoped body inputs; ordinary mappings cannot overwrite them. Initial
projections may read workflow data or earlier activity outputs, but not that
loop's own body. Subsequent state projects only the current body's outputs, with
required/type validation and provenance retention. State snapshots carry run,
scope, generation and completed-iteration identity. Zero denotes the initial
state; exceeding the declared bound refuses. Persistence, immutable per-iteration
output selection and traversal admission belong to the durable execution owner.

Workflow `output_bindings` explicitly project the declared structured output
schema. Publication rejects unknown/incompatible sources and unbound required
outputs. The same return resolver serves a root or reusable subflow and retains
field provenance. Data projections do not grant workspace, artifact or credential
rights; the trusted host must check actual references and inherited authority at
every dispatch. No native authority check is claimed by this data contract.


### Explicit data references and child context

Typed fields may use `workspace_reference`, `artifact_reference` or
`credential_reference`. Their closed JSON values are respectively
`{"reference":"workspace","id":"..."}`,
`{"reference":"artifact","id":"...","revision":"..."}` and
`{"reference":"credential","id":"..."}`. IDs and artifact revisions must be
nonempty. Extra fields (including embedded credential material) refuse. The
`reference` tag denotes an explicit typed reference, not a generic object cast.
The host resolves these opaque IDs through its existing services. Values and
provenance never establish current access or artifact freshness by themselves.

Reference defaults, constants, projections and returns use the same schema
checks. Only fields explicitly typed as references enter the host's
`ReferenceInputs`; an opaque object does not implicitly select a workspace or
credential. This contract does not read any credential store or resolve secrets.

`HostRegistry::subflow_context` validates resolved inputs and the backend-owned
subflow handler kind, then asks the trusted parent context to derive a child
context. The default implementation refuses rather than silently sharing the
parent. Host implementations must not widen the parent's effective authority.
Derivation is not a dispatch permit: the execution owner must still run current
host admission at every child dispatch, including actual reference/freshness and
sensitive-sink checks. These are internal interfaces; real native-authority and
revocation proof remains with the implementing adapters.


### Neutral fork/join contracts and data demonstration

Named `forks` declare at least two distinct body instance ordinals, stable branch
names, required/optional outcome policy and positive `max_concurrency` not greater
than the branch count. A body may target a pinned reusable subflow. Body ownership
cannot overlap another collection, loop or fork scope. All iteration/fork scope
names are unique. Conditions retain their closed true/false/unknown decision
contract; an unknown predicate is never an implicit false branch.

A join snapshot names the exact run, scope, generation and each branch's current
activation/attempt owner. Every branch must arrive, including optional branches;
required failure/cancellation/skip makes the settled result blocked. Optional
failure remains visible but does not change the configured required outcome.
Identical duplicate arrivals are idempotent; conflicting, foreign or stale
arrivals refuse. This pure fold does not persist or emit a release. The durable
owner must commit join release once across restart and fence callbacks before
invoking it. Named regressions include out-of-order aggregation and
`stale_results_cannot_satisfy_a_later_join_snapshot`; durable replay/duplicate-release
proof remains with the execution owner.

Settled collections expose typed `total`, `completed`, `failed`, `skipped` and
`state` summary fields with scope-result provenance. Pending collections cannot
produce final summaries. `complete` means all results are known, not that every
item succeeded; conditions can inspect failure counts. Full per-item results and
absent outputs remain in the aggregate rather than being replaced by counts.

`data_contract_demo_tests` composes a documented list-response projection, pinned
structured identity-subflow return, bounded per-item collection, source-order
aggregate and closed summary condition using real internal functions. The fixture
README records the official source and the failed public API probe. This proves
contract composition only: actual HTTP pagination, scheduler execution, current
native authority and integrated transport behavior remain mandatory in their
implementing layers. No live API or external-write success is claimed.

The current `check_dispatch` method is root/workflow-input-only preflight, not
per-activity resolved admission. It has no upstream-output or scoped activation
snapshot, so activity-output bindings and required item/index/carried inputs
refuse. Unbound, unscoped activity inputs retain the inherited root-field-name
identity compatibility; this is not explicit mapping proof. The execution owner
must validate each activity's fully resolved arguments before every actual
dispatch rather than treat whole-publication preflight as an effect permit.
Collection `ItemIdentity` covers run/scope/generation/index, not activation/attempt;
the durable owner must fence each item's current activation/attempt before
accepting its result. The fork snapshot's stronger owner check does not supply
that missing collection runtime fence.

### Neutral asynchronous effects and approvals (005)

The internal neutral host port now separates pure effect preparation from
invocation, reconciliation and cancellation. The existing owner commits the
exact retained publication, descriptor, configuration, resolved arguments,
optional subject and full attempt/scope fence as an immutable effect intent.
Its digest is the callback correlation. Intent and correlation commit before
adapter entry; neither retained intent nor an approval replaces current host
admission. Preparation must remain deterministic for the same resolved
invocation, including the absolute approval deadline; resumed approval must not
regenerate a relative-to-now deadline. Native provider installation and real
authority/freshness evidence remain in 008. Public desktop/HTTP/MCP authoring and inspection remain in 015.

An adapter observation is pending with an immutable handle, completed with a
schema-validated known result, or uncertain with a diagnostic. Stop evidence is
separate. A transport error or invalid completion cannot prove that an external
write failed. Such observations leave the effect uncertain, hold conflicting
resources and do not supply graph output. Recovery observes the original handle
or precommitted correlation; it never reinvokes the operation. An early callback
cannot be overwritten by a later pending acknowledgment. Committed results are
immutable, and foreign argument digests or attempt/generation fences are refused.
A valid owned effect may settle after its graph frame has closed: its result and stop
evidence release ownership without advancing or reopening that frame.

Questions bind the exact effect digest and a monotonically increasing revision.
The first valid committed answer wins; an expired, stale or already answered
question cannot grant approval. A declined or resolved attempt cannot be reopened
by a question revision. An approved question that expires while waiting for a
resource or dispatch slot fails that branch before adapter entry. Invalid initial
host preparation also records a local failure so independent branches can proceed.
A question waits only its own branch. Prepared, waiting and uncertain work does not occupy a dispatch permit, but actual calls
still obey the scope/root concurrency limits. Independent read-only diagnosis
may continue while conflicting mutations remain blocked.

Run-store schema22 adds only a transactionally maintained resource projection
of this ledger. Trusted preparation supplies mutation classification and resolved
resource keys. Native adapters retain their own permission and ownership checks;
the projection is not a replacement sandbox or another queue. Mutation claims
commit with dispatch and release only after a validated known result AND proven
stop. Cancellation and run expiry can request stop but cannot prove rollback or
release. Late stop/result observations remain inspectable on an ended run and
cannot reopen its graph. Existing domain-shaped questions and the Git-specific
command journal are not called as generic neutral services.

### Neutral model ownership (006, implementation in progress)

The kernel can retain a host-pinned named-tool policy and bind the exact original
model connection, session and generation to its parent effect. Each original tool
call ID names one child effect in the same event ledger and resource projection.
Intent commits before service entry; identical duplicates read that retained
effect, while changed arguments, names or session generations are refused. The
host rechecks current admission before entry. Tools have their own bounded call
and concurrency limits and do not create graph activations. A validated parent
response remains retained until every owned tool effect has a known stopped
result. Recovery observes each effect identity separately, including children
that share their parent's graph-attempt fence.

Coarse workspace evidence captures the host-selected checkout, Git directory and
clean baseline. Artifact evidence pins a clean descendant commit/tree and exact
regular-file Git blob IDs. Path escapes, symlinks and uncommitted output refuse;
an observational scope refuses Git-visible changes. These checks prove committed
content, not interception of all shell writes, process termination or withholding
credentials. The caller must establish original-process stop independently.

Exact profile/settings and restricted host MCP wiring are implemented internally;
native provider installation and original provider-call continuation remain pending.
Reopening a kernel tool record and reconciling its write is distinct from resuming
the provider's original open call. A new prompt or model-generated retry does not
satisfy that requirement. Native service authority, public neutral transports and
cutover remain 008, 015 and 019 respectively. No live model call or native privilege
isolation is claimed.

The exact-request adapter now uses the existing ACP manager with a fresh,
per-connection MCP host and fixed `--no-tools --no-hooks --bare` launch flags.
Its strict request accepts an exact profile, model and effort, literal instructions
and context, the catalogue's shallow object output schema, bounded context/output
bytes and a whole-request deadline. The only unavailable-model policy is `fail`;
unknown settings and policies refuse. Model and effort must both be acknowledged
after configuration before the kernel binds the session and sends one prompt.
Complete JSON output must satisfy the declared schema; fences, prefixes, malformed
or oversized output fail without another prompt. The adapter awaits owned ACP
shutdown but does not call transport settlement proof of OS/workspace termination.
The activity entry constructs the narrow host; ordinary ACP connections retain
their existing launch behavior and host catalogue. Original-call recovery remains
open and no provider is installed by this internal adapter.

The model activity entry now constructs its own scoped MCP host. `tools/list`
contains only the retained named descriptors and strict shallow input schemas;
`tools/call` never forwards to ordinary TUICommander MCP. Other methods and names,
missing/invalid lowercase SHA-256 `tuic/call_id` metadata, changed arguments and
stale ACP origins refuse. Correlation uses the stable ID emitted by the runtime's
durable tool item; the host does not derive a fresh ID from RPC arrival order.
Every call enters the existing kernel effect/admission path and waits on owner
commit notifications for a validated, stopped result from that ledger. Pending
handles are not tool responses. Uncertain original effects return explicit errors
without redispatch.

After restart, the original tool effects reconcile first. Once every owned tool
has a stopped, validated ledger result, the model parent can start a new exact
configured ACP request containing its original context plus recorded call IDs,
arguments, effect IDs and results. A durable continuation event fences the
previous session and result digest before prompting; old callbacks cannot write
through the replacement session. This closes the interrupted provider turn and
continues with a new prompt, rather than resuming an open provider RPC (Boss,
2026-11-11, A2 option B). Active original connections, missing/uncertain results,
expired/ended/cancelled parents, stale continuation commands and context overflow
refuse. Cancellation never starts a recovery prompt. Named tool admission and
run-wide call budgets persist across continuation; native authority is rechecked.
This has owned-file/SQLite/ACP fixture evidence, without paid-provider proof.


The restricted coarse entry deterministically returns
`restricted_execution_unavailable` before any process or session launch. Existing
Git hooks/PATH shims cannot enforce direct privilege withholding. The entry has no
request validation or original-handle reconciliation because it cannot start a
process; native installation belongs to 008. Artifact helpers pin content only and
cannot establish process stop. Individual commit/push/API receipts require explicit
native service operations in 008. This unavailable entry and artifact evidence do
not claim that a confined coarse-agent execution implementation exists.

## Installed Jira Cloud activities (011-2a30)

The internal `JiraHost` implements the existing neutral host port. It uses
`plugin_http_fetch_impl` and its account authentication, manifest consent,
destination checks and credential redaction. It adds no scheduler, credential
store, IPC command or HTTP route. Composition in 008, Designer authoring in 014
and public neutral transports in 015 remain separate work.

A backend-installed `JiraBinding` pins its site prefix, plugin, credential/account
reference, project ID and operation in the descriptor. Register each binding's
`descriptor` with the catalog and install it under its exact handler in
`JiraHost.bindings`. The host supplies `HttpAuthority`; network and write grants
are separate and rechecked before dispatch. An instance cannot change the pinned
metadata. Project writes share one resource key, so issue keys and numeric IDs
cannot bypass mutual exclusion. Cancellation supplies no remote rollback proof.

Each descriptor accepts a `request` object, parsed as the closed `JiraRequest`
union; operation must match the installed descriptor. Results are closed
`JiraResult` values under `result`. An issue result includes an account-scoped
canonical numeric `reference.id`, issue `key` and authoritative `fields`.
Issue input references contain `account` and Jira ID or key; cross-account and
cross-project references fail before mutation. Authentication is never a graph
input. These are Jira Cloud REST v3 contracts, not a claim of Data Center parity.

| Operation | Request data | REST and result |
|---|---|---|
| `lookup` | `issue` reference | GET issue; canonical issue result |
| `search` | JQL **predicate**, nullable `next_page_token`, `limit` 1–100 | POST enhanced search/jql; installed project predicate, typed issues and nullable opaque next token |
| `create` | `issue_type_id`, `fields` | Paginated create metadata, POST issue, GET canonical read-back |
| `update` | `issue`, `fields` | GET editmeta, PUT settable fields, GET read-back |
| `comment` | `issue`, plain-text `body` | POST ADF comment plus original-effect property; typed comment ID |
| `transitions` | `issue` | GET current transitions with field metadata |
| `transition` | `issue`, `transition_id`, `target_status_id`, `fields` | Current transition/required-field validation, POST transition, GET status/field read-back |

`fields` is a closed object: nullable `summary`, `description`, `labels`,
`priority_id`, `assignee_account_id`, and a `custom` map keyed by
`customfield_<numeric id>`. Omitted custom defaults to empty. Description/body
text becomes an ADF document; it is not interpreted as Markdown. Empty description
clears it. Custom values retain the tenant's JSON shape and Jira validates those
values. Project/type come from installation/create data; status uses transitions.
Metadata enumerates allowed fields and required create/transition values. Search
returns one bounded page explicitly; callers may feed its next token to the next
activity. Repeated/oversized tokens fail. Metadata and comment observation are
bounded to 20 pages of 100; an exhausted bound is an error. Responses are bounded
to 1 MiB each. There is no extra request/value byte cap; the existing account
HTTP owner retains destination checks and Jira validates service limits.

No request retries automatically. HTTP 429 reports Retry-After without repeating
a possible write. HTTP 204 is a successful empty receipt, followed by authoritative
read-back where needed. Transport errors, unexpected responses or failed read-back
leave the original mutation uncertain in the existing neutral effect ledger.
`ReadOnlyServiceObservation` identifies service-specific reconciliation, not the
shared HTTP callback protocol. Recovery checks current network authority and
observes the original dispatched effect even when its write grant was revoked:

- Create includes `tuic-effect-<original effect digest>` in labels. Reconciliation
  searches that label within the installed project, requires one match and verifies
  its label by issue read-back. Zero matches can mean search-index lag and never
  authorizes another create. Labels must be available in create metadata.
- Comment includes the `tuic.workflow.effect` property. Reconciliation collects
  numeric IDs from bounded GET comment pages, then reads
  their properties with POST `/rest/api/3/comment/list?expand=properties`. This
  documented POST reads existing comments; it never creates a replacement.
  Missing, foreign or duplicate IDs stay uncertain; a unique digest is required.
- Update and transition observe the desired field values/status. A match proves
  the requested state, not historical attribution to this particular request.
  Numeric integer/double representations compare by value. Expanded option
  objects compare supplied id/value fields, and option arrays match one-to-one
  regardless of order; ordered document arrays keep their order.
  A missing or changed state stays uncertain; no PUT or transition is repeated.

`classified_issue_create` maps the existing typed `GitHubIssue` snapshot and a
classification-to-issue-type configuration into ordinary request data: title to
summary, source URL/number/classification to description, and labels to labels
with each whitespace character replaced by `-`. Unknown
classification fails. It does not depend on GitHub trigger binding code or create
a Story root, and classification grants no write authority.

### Jira inventory and external evidence limits

The existing plugin owns lookup/detail, enhanced search, create fields/create,
edit-field/update, comment pagination/comment, transitions and Cloud ADF handling
(`plugins/jira/{client,provider,query,fields,adf}.js`). Rust cannot execute these JS
ports without another platform. The adapter reuses their REST contracts and the
existing Rust plugin/account client. The operation matrix and reuse decisions are
in the 011-2a30 worklog; no required Cloud operation was dropped.

Tests replay scrubbed real response bodies from
`plugins/jira/fixtures/cloud.json` (recorded 2026-10-27; method/path/status provenance
retained). Constructed faults/property/label/metadata cases are labeled as such.
Authoritative contract references, inspected 2026-11-11: Jira Cloud REST **v3**
[issues](https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issues/),
[enhanced search](https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issue-search/)
and [comments](https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issue-comments/).
The earlier [live plugin report](jira-cloud-live-2026-10-27.md) proves the plugin
boundary at its recorded revision, not this new workflow adapter.

No credentials or live writes were supplied/authorized. Live acceptance requires an
operator-authorized Cloud account/site, scratch project/type, metadata-required
custom fields, and consent for create/update/comment/transition plus read-only
lost-response recovery. Natural rate-limit behavior, marker persistence/search
visibility, current permissions, tenant validation and the new adapter's account
binding remain explicit live validation gaps. Fixture success does not close them.

### Internal GitLab connector slice (012-1981)

The installed `gitlab` v1 descriptors expose single issue/MR lookup, exact MR
`diff_version`, exact head `pipeline`, and a general `review_note`. Installation
pins the REST v4 root, numeric project and credential reference in the descriptor;
current host read/write grants remain separate. These are internal host adapters,
not new IPC endpoints or an activated trigger. Native composition and transport
publication belong to their respective rework stories.

Diff receipts verify head/base/start and refuse missing/collapsed/too-large file
sets. Pipeline receipts check the current MR head pipeline ID and exact SHA/base;
failed, canceled or skipped pipelines never return the successful outcome. Notes
include the reviewed head/base and original effect digest. The service cannot
atomically condition a general note on the current head; this is an artifact-bound
comment, not MR approval. A lost response retains the original effect/resources
and reconciliation never resends the note. No collection scan or latest-pipeline
scheduler is introduced.

HTTP and GitLab clients share the same final settlement policy: an unsuccessful
non-mutating read settles as a stopped, failed activity. A write error retains
uncertainty because the service may already have committed it.

`gitlab/event.rs` normalizes issue/MR/pipeline snapshots with original delivery,
installed project and source generation independent of delivery or binding edits.
The installed schema maps these fields into the shared `TriggerEvent` contract;
the backend `admit_gitlab_event` facade forwards to neutral admission. The ingress
owner must authenticate the delivery first. Shared bindings own typed mapping,
revision/enablement, durable generation dedup and replay authority. Unknown
merge-base stays absent. See
`src-tauri/fixtures/gitlab-workflow/README.md` for recorded/documented provenance
and exact live validation gaps. Inline discussion publication and MR approval are
missing and not required by the selected flow.


### Bounded correction composition (016-358f, phases A/B)

The internal `CorrectionRegion::definition` constructor lowers a selected pinned
subflow into an ordinary neutral loop. `max_corrections` excludes the initial
attempt: zero allows one body, two allow three. Checked addition rejects overflow.
The body contract has immutable `context`, pinned `criteria` and carried `state` inputs, optional
`repeat`, and an `assessed` outcome. It returns required `state` and optional
`repeat`. True requests another correction, false completes the assessment, and
absent takes the operator branch. Exhaustion also takes the operator branch;
known technical failure has its own exit. An assessment success is not permission
to publish or commit.

RuntimeOwner and RunStore remain the only execution authority. Committed loop
admission consumes a correction before dispatch; a failed replacement worker does
not refund it. Scope identity follows the retained metanode invocation, independent
of worker, head and leaf attempt. Replayed commands return their retained result;
old attempt fences cannot complete the replacement. Only the selected body repeats.
Issue, workspace and PR creation stay in the parent graph before the metanode.
The constructor cannot prove what an arbitrary user-authored body does internally;
publication inspection and native service authority still apply to that body.

`CorrectionPolicy::assess` accepts typed artifact-bound receipts and findings.
Provider/resource/revision identifies a source observation independently of polling
or webhook delivery. A previously consumed batch supplies no new completion.
Each configured criterion needs one current result. Missing, contradictory or
ambiguous results, missing reasons and `needs_decision` route to operator.
Relevant `correct` findings or failed checks request correction;
`ignore_with_reason` retains the rationale and source. Mixed or stale artifact
batches wait for current evidence. `advance_artifact` clears current review/check
and approval data while preserving consumed sources and historical findings;
previous outputs remain in the event ledger. It never changes the loop budget.
Uncertain observations name the original effect for reconciliation and supply no
completed body or new correction. The existing neutral effect ledger retains
uncertain writes and prevents dependent advancement until observation settles.

The existing `workflow_neutral_action` exposes
`correction_region {region:{selected,max_corrections,criteria:{criteria:[...]}}}`.
It returns `NeutralReply::CorrectionRegion(CorrectionRegionView)` with the same
configuration and generated `definition`: the selected publication pin, immutable
criteria binding, required inputs/outputs, entry, internal graph and exits.
Normal draft validation/publication still applies. No transport is added.
Designer rendering remains a 014 gap: render this exact DTO, follow its selected
publication through `get_publication`, and inspect retained scope iteration minus
one for the consumed correction count.

The installed `native.correction.assess` activity implements the body assessment
contract. It accepts real stopped `native.review.change` effects from its own run
through 008's committed-candidate `observed_review` service. It runs every captured
host-configured check through `NativeContext::verify_observed`; missing execution
or infrastructure failure cannot become a passing check. The declared criteria
must equal `independent_review` plus `check:<id>` for every captured check, and
its digest must match the carried artifact. Nonzero exit requests correction;
review findings request correction; uncertain review takes the operator branch.
The independent review service owns its fresh tool-less session. This adapter
never calls a model or supplies a passing verdict itself.

An artifact is a committed candidate head and tree against a recorded base.
Review and checks cover that same candidate, and any tree/head mismatch refuses
assessment. The retained 010 source intent pins PR head/base. A newer pending
source generation invalidates old completion even before cancellation flush:
the kernel fences result consumption in its event transaction. Late effect
observations still record stop/receipt evidence and release ownership while
refusing graph advancement. A refused old generation must restart through source
admission; it cannot approve the new head. Within a retained invocation, changing
the carried artifact invalidates evidence without changing its correction budget.
No live provider, Designer rendering or scheduler cutover is asserted here.

Neutral trigger authoring uses the same action: `trigger_schema {source,kind}`,
`trigger_bindings`, `save_trigger_binding {expected_revision,draft}`,
`enable_trigger_binding {id,expected_revision,enabled,acknowledged_overlaps}` and
`preview_trigger_mapping {draft,snapshot}`. Schemas come from the installed
source match. Preview returns typed inputs, optional subject and overlapping
binding IDs with exact revisions. Enable requires that current overlap map.
`replay_trigger {binding_id,resource,generation,replay_id}` uses a string source
generation and requires backend-created owner authority after transport auth;
there is no request grant or replay-enable flag. Replay returns retained trigger
admissions through the existing runtime. Replay owner authentication is checked at the desktop IPC, HTTP or MCP
boundary before the shared service is invoked.

A source snapshot has no workspace reference. A subject field of type
`workspace_reference` that the mapping leaves unmapped gets the binding's own
project (`{"reference":"workspace","id":<project>}`), so a GitHub binding can
start a workflow of native operations, which run only in their bound project.

`templates` returns the built-in neutral definitions
(`src-tauri/src/workflows/templates/`). Each has `key`, `name`, `description`,
`editor` and `definition`. The Designer opens one as a new unsaved draft; Save,
Publish and the trigger binding are the normal draft actions. The
`github-issue-triage` template reads the admitted issue (`native.issue.read`),
gives `model.request` the receipt `text` (the issue as JSON: repository, number,
title, body and label names), asks for `post_comment` and `comment`, and on true
posts the comment and adds the `triaged` label (`native.issue.effect`, each after
its exact approval). On false the run ends `skipped` and does not change the
issue. Each `native.issue.effect` approval names its action, the issue and the
content: the comment text or the label name. Bind it with event `labeled` and filter `label` for one label, or
events `opened`+`labeled` and filter `labels` for issues that carry the label.
The run's own label write is a self event and starts no second run.
The `github-pr-review` template is described in
[workflow-trigger-admission.md](workflow-trigger-admission.md#github-pull-request-review).

Saving or enabling a neutral binding returns its committed revision. The source
poller reads current enabled bindings on its next existing poll cycle; the
transport does not own a subscription refresh or start another polling loop.

### Native operation provider (008)

Daemon startup installs one NativeHost through NeutralCatalogHost; authoring and
execution use the same provider. The native catalog contains versioned workspace
and artifact pins, named verification, exact-change review, story transition, commit and local bare
repository push operations. Strict request types reject extra caller/capability or
receipt fields. The backend binds a captured NativeContext to an existing run;
workflow JSON cannot supply caller provenance. Missing bindings after restart
refuse dispatch until a trusted backend rebinds the run. Project-only publication
context grants no execution rights.

Each dispatch rechecks actual backend session capability state and inherited
scope. Verification uses the existing named check runner. Story operations retain
the selected backend's revision/status/actor rules. Commit, push and story changes
require separate exact ledger approvals and resource ownership; push additionally
requires current Push authority under the captured ceiling. Commit binds the parent
and staged tree, without staging or amend. It also requires a stopped, passing native review effect for the same caller/workspace/base/tree/diff digest. Review uses the native staged diff reader and no-tool ACP reviewer; binary or oversized diffs refuse. Push accepts one exact commit/branch ref
and a canonical local bare repository, without force, tags or broad ref updates.
Known refusals and uncertain native outcomes never become success receipts.

Public starts bind caller-to-run provenance before dispatch. Native GitHub publication
requires separate destination/account/action approval. Sandbox evidence covers
only its explicitly authorized private repository. Provider installation itself
grants no publication authority and performs no migration.

The backend-only native caller is typed: authenticated Owner retains the existing
owner authority; Agent carries a real capability-map key and keeps its captured
ceiling with fresh revocation checks. Receipt identities distinguish those cases,
and story transitions preserve the corresponding native actor. Capture accepts
check names only. The installed NativeHost resolves them from backend-installed
project verification configuration; unknown names refuse. Graph or transport
command definitions never configure that registry. The shared provider bind_run
hook refuses by default and NativeHost binds the exact persisted run without
reinstalling a provider. Public transport integration remains015 responsibility.

### Native GitHub publication

The typed publication.github request selects an account reference, repository,
current commit/tree and one action: create a new branch, open a draft PR, comment,
close PR, or delete a branch at the expected artifact. Every action requires its
own exact ledger approval and current Push authority; authoring/start grants none.
The native account resolver must match both the requested account and destination.
Native Git uses literal HTTPS destinations, command-only child credentials, bounded
execution and exact ref leases: creation cannot replace an existing branch and
deletion cannot delete a ref changed to another commit. URL rewrite configuration
refuses. API actions reuse the account-aware client and read back the provider
objects. PR actions require the approved head artifact in the same repository.
PR read-back treats a null body as empty only when the approved body is empty;
a differing nonempty body still leaves the publication uncertain.
Uncertain writes retain uncertainty; they are not replayed as new writes.

Live evidence is restricted to the explicitly authorized private sandbox test,
ignored during unattended runs. It cleans its unique PR/branch even on failure and
prints only provider object identifiers. Credentials never enter workflow JSON,
receipts, command arguments or token files. Public caller integration remains015.

Recorded private sandbox evidence: the native test created PR8, read back its comment, closed the PR and verified its unique branch was deleted. This is native publication evidence, not public transport or live model-review proof.

## Neutral source bindings (010-978f)

The source-neutral binding/preview/admission contract shares the existing run owner and effect ledger. See [trigger admission](workflow-trigger-admission.md) for generation identity, overlap activation, owner replay and GitHub polling limits. Designer and transport consumers belong to 014/015.
### Native correction observations

`review.change` accepts either the staged tree used by `commit.create`, or an
explicit committed candidate `head` against `base`. Both use the same retained
ownership and diff checks and pin the exact reviewed tree. Committed review
observations retain Pass, Findings or Uncertain; only Pass satisfies the staged
commit approval gate. An empty diff records Uncertain with no reviewer session.
Nonempty review uses a fresh tool-less ACP session; its session identifier is
inspection metadata and does not establish authority. The internal one-shot
session field is omitted from the public one-shot response.

Named verification returns a candidate-bound CheckReceipt even for a nonzero
command exit. Infrastructure, interruption and changed artifacts refuse instead
of becoming successful proof. Approval consumers must require zero explicitly;
correction assessment can inspect failed checks and every captured check name.

### Scoped roles and pinned story children (020-6471, 022-597e)

The installed configured catalog adds `agent.role@1` (`host.agent.role@1`). It uses the existing mediated ACP adapter and named native tool ledger. Configuration pins `role`, the exact `request` profile/model/effort/literal prompt/report schema, `runtime`, and `tools`; inputs bind `source: {origin, generation, commit, tree}` and a workspace subject. Origin/generation are retained source data: the upstream domain adapter must verify their provider identity. They grant no rights. The actual Git artifact is checked before admission. Source PTY profiles require an explicit mediated translation; unavailable/mismatched profiles refuse with no launcher fallback. Reviewer and validator roles require fresh tool-less sessions; native exact-change review remains a separate service and its approval cannot be manufactured from a role report.

The host returns `report: {effect, publication, role, source, observed_artifact, identity, report}`. The effect contains run/execution/activation/attempt; identity comes from the observed ACP connection/session/generation. The payload is checked against the pinned report schema. `native.agent.report.read@1` accepts `{report: EffectKey, source}` and resolves the stopped original effect in the same run, refusing foreign/stale sources, changed artifacts and incomplete/uncertain results. Current native authority remains mandatory for every tool. Model output cannot choose an ownership envelope or turn a report into publication consent.

`save_child_dispatch` accepts `expected_version`, an operation `id`, an exact `publication: PublicationPin`, and `cohort: {plan, plan_revision, source, stories: [{id, revision}]}`. It creates an immutable reusable operation through the existing save/catalog path, using the registered `host.story.subflow@1` handler. The child interface must require the cohort and workspace and return `receipt` from `native.story.cohort.observe@1`, routing its `completed`/`blocked` outcomes directly. Admission checks the selected project's real story backend, exact plan/source and each existing story revision. The input cohort must equal the saved snapshot; cross-project references cannot widen inherited scope.

Children run as pinned frames in the same run owner, retaining their transitive publications and typed mappings. They do not launch terminals or a second scheduler. The native observation retains its effect identity, ordered cohort and observed stories. It reports completed only when every selected story is Done; otherwise it reports blocked. Lost replies, cancellation, uncertain effects and resource retention use the existing effect ledger and frame joins. No new write is inferred from missing evidence. Ordered cohorts retain order as data; `integration_barrier: not_attested` and individually blocked `integration_obligations` (combined artifact, independent batch review and FIX re-entry) explicitly block any claim of combined-artifact critic, independent batch approval or FIX re-entry parity. Those obligations require their own configured graph and evidence.
### Controlled implementer writes (2960-d562)

`native.workspace.stage@1` is the only write path of a mediated implementer (Boss Q7=A). It is an installed, target-less native operation, so an `agent.role` binds it as an exact host tool in its `ModelToolPolicy`. The model gets no shell, PTY, process or broad filesystem privilege.

The request is `{files: [{path, content}]}`, where `content: null` deletes a tracked file. Paths are plain workspace-relative paths. The operation refuses an absolute path, `..`, a `.git` component, a backslash, a path through a symlink, a repeated path, more than 64 files and more than 4 MiB. It also refuses a workspace that already has unstaged or untracked changes, so foreign dirt is never staged or attributed. It writes and `git add`s (or `git rm`s) exactly the listed files and returns `{base, tree, paths}`, where `tree` is the index tree. Staging is local and is not an approval-gated mutation; a lost reply is not repeated.

The native artifact is HEAD plus the exact index tree when the worktree equals the index and nothing is untracked. With a clean worktree this is the previous clean rule. The role report records the staged tree as `observed_artifact`, `native.review.change {base, tree}` reviews it, and `native.commit.create` commits it after its own exact approval. A later unstaged edit makes the artifact stale. Verification (`native.verification.run`) stays a separate step on the committed artifact, because the check runner still requires a clean worktree.

### Owned native story proposals (021-6e1b)

`native.story.create` v1 accepts one closed `request`: `review` (approved scope
within the run), `cap` (0–100), `proposal` (stable key), and `story` (`NewStory`).
It requires exact current native effect approval and captured caller/workspace
ownership. The plan must belong to that workspace. It uses the project-selected
story backend's existing proposal service, including external adapter creation.

The run ledger pins the first cap for each review and atomically reserves each
proposal before creation. Reservations count even when the adapter outcome is
unknown. Repeated proposals, changed caps and exhausted budgets refuse without
another write. The receipt includes the backend story, responsible backend caller,
review, proposal and original effect key. Responsibility records proposal ownership;
it does not claim or assign the created story. Reconciliation retains the existing
native unknown-effect policy; it never repeats creation. Review scope and cap are
part of approved graph input, not authority inferred from model text. Coordinators
and reviewers share this operation; migration must bind their original policy.

Optional `source_story` plus `source_revision` fences the source to the target
plan and current backend version. `evidence` retains bounded source references.
The full accepted request is included in the receipt. Reconciliation reads the
original reservation and backend proposal mapping without calling creation;
a missing mapping remains uncertain and requires explicit disposition.

### Exact-head review publication (024-c9ee)

The existing `native.publication.github` v1 descriptor accepts the `review`
action with `number`, nonempty `body` and `verdict` (`COMMENT`, `APPROVE`, or
`REQUEST_CHANGES`). Existing account, workspace, artifact, current Push authority
and exact-effect approval checks apply. The adapter checks the open PR head,
sends the approved `commit_id` to `pulls/{number}/reviews`, reads the review back,
and checks the head again. A changed head or divergent read-back after POST is
uncertain, not success or permission to repeat publication. Receipts retain the
review id, URL, exact commit and action. This extends the existing descriptor;
no second GitHub transport or catalog operation is introduced.

Contract: the consumer observes an actual submitted review, not an issue comment.
Authoritative state is the existing effect ledger, native artifact and GitHub
review read-back. GitHub API access and review permission are external
prerequisites; own-PR approvals may be rejected by GitHub. The affected consumer
is `pr_review`; legacy migration remains separate. No live review proof is
claimed without command-scoped credentials and the authorized sandbox.

A review request also includes `base` and `review` (the retained native review
EffectKey in the same run). Its exact committed artifact, caller and workspace
must match, its independent reviewer session must exist, and its body must equal
the retained review body. APPROVE requires Pass; REQUEST_CHANGES requires Findings. The `review.change` and `review.bounded` receipts include their own `effect` key, so a graph binds the review into this action without caller-supplied keys; the consumer still checks the key against the retained effect.
`mark_ready {number}` separately verifies an open draft PR and read-back of the
same node id, head and non-draft state. Complete source-template graph conversion remains a migration obligation; these
native actions do not claim legacy template conversion.

A converted publication may retain `source_intent` (`push`, `create_draft_pr`,
`mark_ready`); admission rejects any mismatch with its explicit native action.
Existing requests without this optional field keep their behavior. Trigger-started
review/readiness effects resolve their source from the retained trigger request,
require the current resource-owning run and generation, and recheck before issuing
a success receipt. Receipts include `source_generation` when source-bound. A
changed source after publication is uncertain. The converter still owns building
complete graphs and supplying pinned target data; this native action grants none.

### Source-bound issue operations (023-026f)

The native catalog includes `native.issue.read` and `native.issue.effect` version 1.
Both use the existing workspace subject and `{request: ...}` / `{receipt: ...}`
contract. Read derives the issue from the run's retained trigger receipt and
resolves the workspace's installed GitHub account. Its receipt contains the source
pin and current issue. Graph JSON and labels cannot create source ownership.

An issue effect selects one comment body or one additive label. The source pin
contains the admission receipt, account, repository, issue number, generation and
requirements digest. Each write rechecks the run-owned admission, current retained
generation and live open issue requirements, then exact effect approval and current
Push authority under the captured caller ceiling. Issue resources serialize writes
across workspaces sharing an account/repository/number. A lost or malformed write
reply remains uncertain; reconciliation never posts it again. This operation does
not install an intake trigger or transfer ownership from the old scheduler.

`native.issue.triage` validates a stopped, scoped accepted/rejected/needs-input
report against the same source and exact profile/model/effort/instructions. It
re-reads requirements after evaluation. Policy labels and the NeedsInput comment
choice are retained with that verdict. Semantic `issue.effect` variants cover
rejection/question comments, accepted/rejected labels and original-child
evidence/status comments. These variants consume actual settled triage or child
receipts; labels or a caller-written verdict cannot replace that evidence.

`native.issue.handoff` accepts only an accepted triage effect from the same intake
and a child publication already in the retained project closure. It delegates to
one installed child owner keyed by the parent effect. Running children keep the
parent effect pending; observation/cancellation must return the same source,
triage, publication and child identity, without calling dispatch again.

The scoped evaluator and child owner enter through the backend-only
`IssueServices` port, receiving the retained native caller binding.
`NativeHost::install` installs the production owner. Its triage runs one turn
on the shared mediated model path (`run_request`, the path `agent.role` uses)
with the exact `IssueProfile`: the profile must equal the configured ego ACP
profile, and model, effort and instructions are pinned. The triage effect is
therefore prepared as a tool-less model parent (empty tool policy, no
resources), and its model session identity is bound to that effect. The
production owner never starts a child: handoff, observation and cancellation
refuse with a conversion blocker. Converted issue graphs compose
`native.story.create` and the pinned `host.story.subflow` child dispatch
directly. Tests may install a scripted owner to prove the original-child
lifecycle projection. The protocol fixture substitutes only the model process;
live model execution remains unproved.

Native GitHub publication keeps its approved literal HTTPS destination and token through command-scoped identity rewrites, overriding host-wide Git URL rewrites without modifying saved configuration. Competing exact rewrites refuse; artifact, approval and ref lease checks still apply.


### Retained review and correction policy composition

`native.review.bounded` v1 pins `{policy:{scope,evidence}}` in activity config.
`evidence` uses the existing camelCase EvidencePolicy fields. Each nonempty native
review spends one durable run/scope admission immediately before the reviewer is
entered; lost replies cannot re-enter that effect. Policy-bound reviewer/validator
`agent.role` activities share that same budget and emit a retained exhaustion
marker before opening a replacement session. Separate native and role reviews
cost separate admissions. Restart, retry, new candidate and replacement identity
do not replenish `maxReviews`. Empty diffs supply no independent approval.

`native.correction.policy.assess` v1 consumes the existing context, criteria and
state plus the whole retained `agent.role` report output and optional original
story proposal effect keys. It resolves reports and proposals from stopped native
effects, checks the exact source generation/candidate/requirements, and classifies
findings as immediate correction, receipt-backed separate work, accepted risk with
reason, or operator decision. It retains the existing independent native review
and every host-configured verification floor. Model proof metadata cannot grant
RED, strict Clippy or native-target proof: such extra requirements remain operator
work until their owning proof bindings are supplied. `accounting` reports review
admissions, rejected reviews, corrections after the initial attempt, check attempts
and proposal effects separately. Exhaustion gives absent `repeat`, the ordinary
loop's operator route; it never grants success or consent.

The shared `correction_policy_region {region:{region,policy}}` action inspects the
selected same-project publication and transitive pins, requires bound reviewer and
assessment policies, binds a source role output, rejects uncounted native reviews,
and refuses state outputs bypassing the assessment. It lowers through the existing
correction region: `max_corrections` equals `maxRepairs`, total body traversals are
N+1. Normal draft/publication admission, immutable prompts/handlers/subflow pins,
owned effects, cancellation and original-effect reconciliation still apply. This
is an additive backend contract, not template migration or deployment evidence.

Policy assessments journal each configured check admission and its actual receipt under the original assessment effect. Refresh/reopen reuses the retained receipt without another process; a missing reply is uncertain and requires original-attempt reconciliation. Check attempts include this journal and standalone native verification effects, independently of correction spending.

Operator decisions use the existing effect ledger and ordinary pinned edges; [typed native bindings](workflow-operator-decisions.md) keep resume, closure, release consent and effect approval independent.

### Directory-only scheduled text workflows

Scheduled neutral bindings reuse the automation tick and trigger receipts. The
installed model accepts literal `text` input; `inbox.approval@1` waits for the
existing exact effect Answer and returns `approved: true`. These activities use
backend caller scope without requiring a Git checkout. Native repository
operations retain their Git scope requirement. See [scheduled starts](workflow-trigger-admission.md#scheduled-neutral-starts).


### Ledger-selected neutral plan batches

The native `story.batch.pending` operation accepts `{cohort, limit}` (1-128)
and returns `{cohort, members}`. It selects only unstarted members of the same
retained run/plan cohort, or its armed FIX owner alone. Each member carries the
unchanged admission cohort and a current-revision single-story cohort for child
admission. The original effect retains its selection receipt for reconciliation.

The proposed seeded-plan composition configures a limit of eight, a sequential collection,
one pinned combined-artifact reviewer and an eight-round `LoopContract`; these
limits are not global engine rules. The verdict receipt includes `repeat` for
ordinary typed loop-carried data.
