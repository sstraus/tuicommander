---
id: 015-608c
title: "WER: Expose the same workflow capabilities through MCP, HTTP and desktop"
status: complete
priority: P2
type: feature
created: "2026-11-10T08:29:18.830Z"
updated: "2026-11-12T00:57:23.000Z"
dependencies: ["005-f907"]
plan: plans/workflow-engine-rework.md
plan_step: Step 4
depends_on: ["workflow-stories/005-f907-complete-P2-wer-own-durable-asynchronous-activities-effects-an.md"]
---

# WER: Expose the same workflow capabilities through MCP, HTTP and desktop

## Problem Statement

Workflow engine rework 015-608c. Scope: expose catalog inspection, workflow/subflow/metanode authoring, mappings, validation/publication, trigger configuration, starts, results, waits and run inspection through shared backend contracts.

Delivery prerequisite: 005-f907. Implementation and evidence are recorded below; the native caller binding is an explicit round2 integration handoff.

Contract-first feature: record consumer effect, authoritative state, external assumptions and affected consumers before implementation. Reuse existing services/evidence where compatible; no mandatory duplicate full suites or extra critic rounds. Planning creates pending work only and does not authorize implementation or live effects.

## Acceptance Criteria

- [x] Designer and MCP discover the same catalog IDs/versions and metanode internal graphs, customize the same subflows and preserve equivalent schemas/outcomes/authority.
- [x] Desktop IPC and HTTP/WS/SSE provide equivalent request/response/event shapes with explicit documented desktop-only exceptions only where justified.
- [x] Add required transport table/route snapshots and registered route probes for changed APIs using the existing generated parity mechanism.
- [x] Changed push paths dual-emit as required; remote clients cannot miss lifecycle/results.
- [x] Model labels or client choice never alter authorization.
- [x] Contract checks name drift in mapping, result casing and omitted wait/progress events; reuse compatible existing evidence.

## Work Log

### 2026-11-11 — transport contract and prerequisite audit

Consumer effect: Designer, desktop, browser and managed MCP callers inspect and
edit the same neutral definition, descriptor versions, schemas, mappings and
metanode publications. Run readers observe retained results, waits and effect
uncertainty from the same cursor-ordered ledger. A client label, model choice or
serialized descriptor cannot supply host authority.

Authoritative state: `WorkflowStore` owns immutable neutral publications,
project-owned reusable descriptors and activity presets. `RuntimeOwner` and
`RunStore` own neutral starts, execution decisions, effect intents and results.
`HostRegistry`, `HostContext` and the installed `NeutralHost` own current service
admission. Transports only deserialize, select a project and call these owners.

External assumptions: no external service call, live write, app restart or
operational cutover is authorized. Fixtures must use real internal contracts;
a test host cannot be presented as a production native provider.
Affected consumers: existing workflow MCP grouping, generated transport route
parity, HTTP auth guards, Designer 014, run cursor SSE and desktop invalidation.

#### Wire contract reserved for 014 (not implemented yet)

One command: `workflow_neutral_action {project, action}`.
One route: `POST /workflows/neutral/action?path=<canonical project>`; its JSON body
is exactly `action`, with no wrapper or field conversion.
MCP: `workflow {"action":"neutral","input":<action>}`. MCP resolves the project
from the bound live managed caller, never from an `input.project` claim.
Replies: `{"type":<reply tag>,"value":<body>}` on every transport.
All neutral fields and tags use **snake_case**, including `run_id`,
`after_sequence`, `expected_revision`, `input_bindings` and `execution_policy`.
The existing `workflow-run-changed` invalidation envelope stays unchanged:
`{"repo_path":project,"payload":{"runId":run_id,"sequence":sequence}}`.
Clients re-read `inspect`/`events` after this event and after reconnect; the event
is not a substitute for retained event pages. No second scheduler or event log.

Requests below are closed objects: additional fields refuse. `integer` means a
JSON integer, and `value_map` means an object with arbitrary JSON values. Existing
Rust contract names refer to their exact serde wire representation, not a new
Designer-specific model. The full machine-readable JSON Schema must be derived
from the final shared request types when the provider inputs below are supplied.

| action | Required fields beyond `action` | Reply type/value |
|---|---|---|
| `catalog` | none | `catalog`: `{activities: ActivityDescriptor[], metanodes: NeutralPublication[]}` |
| `create_draft` | `name: string`, `definition: NeutralDefinition` | `draft`: draft envelope below |
| `list_drafts` | none | `drafts`: draft envelope array |
| `get_draft` | `id: string` | `draft` |
| `update_draft` | `id: string`, `expected_revision: integer`, `name: string`, `definition: NeutralDefinition` | `draft` |
| `validate` | `definition: NeutralDefinition` | `validation`: `{valid: boolean, diagnostics: Diagnostic[]}` |
| `preview` | `definition: NeutralDefinition`, `inputs: value_map`, `subject: value_map or null` | `preview`: resolved sample IO and preparation diagnostics; no execution or durable receipt |
| `publish` | `id: string`, `expected_revision: integer` | `publication`: exact `NeutralPublication` |
| `get_publication` | `id: string`, `revision: integer` | `publication`: exact `NeutralPublication`, including internal control graph |
| `clone_publication` | `id: string`, `revision: integer`, `name: string` | `draft`: new ID, revision 1, unchanged definition content |
| `save_operation` | `expected_version: integer`, `descriptor: ActivityDescriptor` | `operation`: exact retained descriptor |
| `save_preset` | `id: string`, `expected_revision: integer`, `instance: ActivityInstance` | `preset`: exact `ActivityPreset` |
| `get_preset` | `id: string`, `revision: integer` | `preset`: exact `ActivityPreset` |
| `start` | `request: NeutralStartRequest` | `inspection`: retained run projection |
| `inspect` | `run_id: string`, `after_sequence: integer`, `limit: integer (1..500)` | `inspection`: snapshot plus retained event delta |
| `events` | `run_id: string`, `after_sequence: integer`, `limit: integer (1..500)` | `events`: exact `NeutralEvent[]` |
| `cancel` | `run_id: string`, `command_id: string`, `expected_sequence: integer` | `inspection` |
| `answer` | `run_id: string`, `command_id: string`, `expected_sequence: integer`, `key: EffectKey`, `revision: integer`, `approved: boolean` | `inspection` |

Draft envelope: `{id:string, project:string, name:string,
revision:integer, latest_published_revision:integer,
definition:NeutralDefinition}`. This is persistence/transport metadata around
the existing definition; it must not introduce another workflow graph model.
A draft can be invalid while edited. Publish uses the existing host-admitted
publication path, optimistic revision and exact descriptor/subflow pins.

`Diagnostic`: `{path:string, code:string, message:string}`; `path` is a JSON
Pointer into `definition`, including the exact target input or control edge.
Validation must name incompatible bindings without client-side graph computation.
A preview cannot manufacture outputs for an unexecuted producer. Unknown, absent,
null and empty arrays retain their existing `DataState` distinction. Pure effect
preparation must use the real installed host, return unavailable diagnostics when
current context is missing, and never call `invoke_effect` or mutate a store.

`NeutralDefinition` retains all fields from `neutral_publication.rs`: `control`,
`outputs`, `outcomes`, `inputs`, `subject`, `instances`, `input_bindings`,
`conditions`, `collections`, `loops`, `forks`, `output_bindings`, `subflows`.
`control` is `{nodes: {node_id: ControlNode}, edges: {node_id: {outcome: node_id}}}`.
`ControlNode` is tagged by `type`: `start`, `end {outcome}`,
`activity {activity}`, `condition {condition}`, `collection {scope}`,
`fork {scope}`, `loop {scope,condition}`, `wait {duration_ms}`.
An activity ordinal is a zero-based integer into `instances`.
`ActivityInstance` is `{descriptor:{namespace,name,version}, config:value_map,
execution_policy:string}`. `ObjectSchema` is `{fields:{field: {value_type,
required:boolean, default:JSON or null}}}`. `value_type` is one of
`workspace_reference`, `artifact_reference`, `credential_reference`, `null`,
`string`, `boolean`, `integer`, `number`, `object`, `array`.
Mappings retain the existing `source` tag (`workflow_input`, `activity_output`,
`scope_output`, `constant`) and typed condition/scope contracts without renaming.

Inspection must project retained `NeutralExecution`: `run`, `sequence`, `status`,
`node`, `execution`, `parent`, `parent_fence`, `attempt`, `active_scope`,
`wait_due_ms`, `deadline_ms`, `activation_count`, `activity_outputs`, `outputs`,
`effects`, and cursor-paged `events`. `status` needs an explicit serialized
representation from the backend enum, not an inferred client label. Full effect
records retain original keys, question revisions, stop evidence and uncertainty.
A metanode is a pinned existing subflow publication, not an opaque new node kind.

#### Exact missing inputs before production implementation

At base `e54abb2999`, `run/neutral_runtime.rs::NeutralHost` exposes only
`registry()`, `context(invocation)`, preparation and effect methods. Its setter is
`#[cfg(test)]`; `RuntimeOwner.neutral_host` starts empty. There is no production
host catalog accessor or publication-scoped context constructor.
`NeutralAdmissionService::publish` requires the catalog, registry and a backend
context. Using `save_neutral_publication` directly would bypass that service's
host publication check. Supplying a catalog or grants from client JSON would
create a second authority and violate AC5.

Required input from 008: name the backend project catalog accessor and its exact
registry/publication-context contract, plus how the same installed host reaches
the existing owner. 015 must consume that seam rather than implement native
adapters, edit 006/008-owned modules, or invent permissive placeholder providers.

Additional owner contract required for complete requested scope: neutral trigger
configuration. The current `TriggerBinding` points at legacy publications and
supports PR/issue poll sources; it is not the neutral typed trigger service.
The intended 010 callable action/request/reply contract is not in this base.

No acceptance criterion is checked. No runtime transport, catalog, preview or
execution success is claimed by this contract note. Targeted route/schema and
lifecycle tests remain required after these inputs are resolved.

Validation so far: clean starting worktree; instruction-link check passed
(`2 instruction pairs`). No Rust/JS tests run; this note changes no behavior.
Tool anomaly: `mdkb memory confirm clu-0435a1cda8e9c8b2 --outcome confirmed`
failed with `Prior ... has multiple open sessions; pass session`; no retry or
new duplicate memory entry was made.

### 2026-11-11 — coordinator input resolves the provider ownership blocker

COORDINATOR2 authorizes a narrow accessor implemented by 008. Continue with
shared DTOs and neutral draft wrappers; do not create another host/catalog owner.
The earlier BLOCKED report is superseded by this instruction. Neutral trigger
transport is an explicit handoff to 010, which will extend the neutral action;
it is not a gap owned by 015. Legacy trigger transports remain unchanged.

Implementation contract: draft edits retain the existing NeutralDefinition and
optimistic revision; catalog/publication/preview use the host-installed source,
not descriptor/grant input. Labels/editor metadata, if authorized, stay separate
from execution semantics. Required regression protection: cross-project access,
stale draft mutation/publication, lost mapping fields through buffered enum
serde, client-provided authority, dropped terminal/wait/effect cursor events and
transport casing/path drift. No live external fixture is assumed.

### 2026-11-11 — implemented shared action contract (validation in progress)

The reserved contract above is now implemented in `workflows/neutral_api/model.rs`
and the single shared `neutral_action` service. Desktop IPC, the authenticated
HTTP route, and grouped MCP deserialize this same enum and serialize the same
reply. JSON Schema is derived from that request enum in the existing MCP schema
mechanism. Extra request keys are rejected. Numeric input-binding map keys are
decoded through JSON explicitly to survive internally tagged enum buffering.

Editor extension agreed with the coordinator: `create_draft`/`update_draft` accept
optional `editor:{labels:{control_node_id:text},presets:{control_node_id:text}}`.
The default is empty maps. Both are bounded presentation metadata on the draft
wrapper, stored atomically beside each publication, returned on publication
inspection and preserved when cloning. They never enter the definition hash,
descriptor pins or host admission. Catalog metanodes are `PublicationView[]`:
exact `NeutralPublication` fields plus this `editor` field.

Inspection reply value is `{snapshot:NeutralExecution,events:NeutralEvent[]}`.
Snapshot status is explicitly tagged `type` with snake_case names. Event paging
uses `after_sequence` and `limit`; the snapshot and page share a read transaction.
All committed neutral decisions publish the existing desktop and SSE cursor
invalidation, including start, wait, effect and terminal decisions. Reconnects
read the retained event ledger. No high-frequency token stream was introduced,
so no dedicated WS channel is needed.

Preview returns `{validation,activities,outputs}`; outputs are the declared
schema. Each activity contains resolved `Datum` inputs (including absent/unknown
and provenance), output schema, outcomes and admitted effect preparation or its
unavailable reason. Preview never invokes an effect or commits a run. This tests
the internal preparation contract only, not an external service assumption.

Authority remains backend-owned. A missing installed native catalog fails with
`native catalog not installed`; no substitute catalog or permissive host is
created by a transport. The new `NeutralCatalogHost` accessor shares the same
Arc with `RuntimeOwner`; 008 integrates its production provider separately.
No transport accepts native caller bindings or a graph label as a grant.

Tests are at distinct owning boundaries: HTTP exercises JSON/route/persistence
and stale/project checks; MCP exercises grouping and exact unwrapped replies;
the owner integration observes retained output 8 from input 7 and wait/result
cursor events; schema checks protect public actions versus internal effect
observations; the existing generated TypeScript table and Rust route probe
protect path parity. Existing publication/admission/runtime consumers are in the
same targeted rb run. No test-only production seam was introduced.

Local evidence so far: pnpm 12.15.0, rustc 1.99.1; transport Vitest 380/380 GREEN
and `pnpm exec tsc --noEmit` GREEN (`~/Gits/.tmp/wer-015/validate.log`). Biome
reported formatting only; corrected before validation2. The generated route
snapshot was produced by Vitest `-u`, never edited manually. Rust outcome and
final tree are not yet claimed.

Trigger handoff now active: 010 owns exact neutral binding/action types and
replay authority. It has received the shared enum/service contract. 015 is the
sole writer of the facade enum and will add the agreed variants once supplied.
Replay authority must originate in a trusted backend context, never request
JSON. Legacy trigger transports remain untouched.

### First rb outcome and contract correction

Task `20261111T113323Z-26664`, staged code tree
`da7b8291fc3eb6c550d15e0e578f179c865e89c6`, nextest run
`4766c44f-5266-4160-8593-80c474c9d36f`: 13/15 passed. Route probe, typed schema,
MCP search budget, publication/preview native authority, retained wait/result
cursor integration and seven existing native runtime consumers passed. The two
failures reproduced one JSON contract defect: serde unit variants ignored extra
keys despite `deny_unknown_fields`. Catalog/list_drafts are now empty struct
variants, retaining exactly the same `{"action":"catalog"}` wire request while
rejecting extra grant claims. This is a contract correction, not authorization
from those fields. MCP's negative case failed for the same reason (it reached
the missing-provider error instead of deserialization). Unused fixture imports
were removed. Targeted rerun covers changed enum consumers and those test files.

The preceding task `20261111T113023Z-80436` exited 75 because this worklog changed
during sync; no Rust command ran. Local validation2 passed Vitest 380/380 and
Biome. Further repository edits are frozen while rb snapshots the next tree.

### Green neutral contract and trigger extension

Task `20261111T114414Z-63104`, tree
`42d06a64f7a5d0d868409059d4e765a1899f6e2b`, nextest run
`4ea52168-c48d-4637-bf59-7661502ea19f`: **31/31 GREEN**. Command:
`build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(neutral_transport_) | test(workflows::neutral_publication_tests::) | test(workflows::neutral_admission_tests::)'`.
Remote toolchain versions are **unmeasured**; the landing records them. The
versions previously listed are local only. Unchanged route, schema-budget and
runtime consumer tests reuse task26664 evidence; the changed enum consumers and
affected publication/admission consumers passed63104. No full suite was run.

The coordinator supplied and authorized importing two isolated 010 contract
commits with `-x`: 21530a7b7c (local42cd1e2a1b), d6d2e44b106 (local22c08b5ec4).
015 adds only JsonSchema derives on those shared request types and module
registration. The source enum is closed; installed schemas use a plain backend
match. Designer014 has the updated shapes. All other 010 trigger implementation
remains owned by010; its committed domain facade is the remaining input for
compiling the trigger extension, not an invented transport implementation.

| New action | Fields | Reply tag/value |
|---|---|---|
| trigger_schema | source: TriggerSource, kind: string | schema: ObjectSchema |
| trigger_bindings | none | trigger_bindings: NeutralTriggerBinding[] |
| save_trigger_binding | expected_revision, draft: TriggerBindingDraft | trigger_binding |
| enable_trigger_binding | id, expected_revision, enabled, acknowledged_overlaps: id→revision | trigger_binding |
| preview_trigger_mapping | draft, snapshot: TriggerEvent | trigger_preview: {inputs, subject, overlaps} |
| replay_trigger | binding_id, resource, generation: string, replay_id | trigger_admission: TriggerAdmission[] |

Only the existing authenticated/local-owner IPC/HTTP/MCP boundaries select
`neutral_owner_action`. The shared service canonicalizes the project before
constructing backend-only `ReplayAuthority::authenticated_owner(project)`.
Ordinary backend authoring calls cannot replay; JSON cannot supply that token.
The added public-action regression checks installed schema selection, revision
CAS, typed input mapping, exact overlap acknowledgement and refusal of forged
replay flags. It uses internal stored definitions; no live event delivery or
external write is assumed.

010 additionally requires `github_poll::refresh(state)` after save/enable,
because those operations change the existing source poller's subscriptions.
015 wires that call and surfaces a failure explicitly with the committed binding
ID/revision. Persistence is not rolled back across poller configuration. The
public-action test uses an unregistered repository on purpose: it proves the
configuration error and durable binding are both observable without any live
account lookup or event delivery. This extends the earlier test's expected
successful metadata change with an explicit unconfigured subscription result.

Operational source-neutral dependency c3e4974dc1 is consumed with `-x` as
43d608abf6. The single module conflict keeps 015 neutral_api and adds only
trigger_bindings; unrelated009 connector/http_operation module context is not
imported into this checkout. Incoming runtime lock and replay/admission modules
are additive. 010 owns these domain implementations and their tests. The
metanode transport regression now explicitly inspects catalog discovery after
publication and compares its internal graph/presentation to the retained
publication, preventing an ID-only metanode catalog from passing.

### Final source-poller ownership decision (supersedes refresh proposal)

COORDINATOR2 explicitly removes post-save/enable refresh from015. Source010 (and
GitLab012) re-read current enabled bindings on each existing poll cycle. The
transport returns the committed binding revision and has no github_poll module
dependency. The prior refresh/error proposal above is superseded; the code and
regression now assert successful durable save/enable. Activation timing belongs
to the source's next existing cycle, not a transport-side timer.

The shared entry takes a non-serde `NeutralCaller` backend argument; only
`neutral_owner_action`, selected after existing transport guards, supplies the
owner provenance for replay. Other actions use ordinary Author provenance.
The same implementation handles both, with no test-only production entry.

### Trigger transport GREEN and008 integration handoff

Task `20261111T121133Z-59856` (tree/run IDs recorded in the evidence below),
nextest11/11 **GREEN**: public trigger mapping/CAS/overlap/replay refusal,
MCP enum/schema/search-budget consumers, metanode internal graph discovery,
preview authority and retained wait/result integration. The10 compiler warnings
belong to imported010 normalizer/source APIs not yet consumed in this transport
branch; no blanket suppression or unrelated source follow-up is added.

COORDINATOR2 assigns the008 caller-binding handoff: a single `start_request`
boundary is extracted in run/neutral_api.rs. It keeps current semantics and
centralizes exact-retry/new-run handling for the round2 binding. This extraction
and the actual engine-fixture capture are the only code changes after59856;
the affected transport tests run once more, and unchanged trigger/schema cases
reuse their GREEN evidence.

Round2 exact need:008 owns NativeCaller::{Owner,Agent(capability_key)},
NativeContext and a default-refusing NeutralCatalogHost::bind_run hook, because
its types are absent this base. Desktop/HTTP owner provenance comes from backend
auth; MCP agent identity must come from bound PTY/ACP capability state, never
request text. The publication pins verification names only.008 resolves each
name against the host-installed project verification config; unknown names
refuse, and strings never become commands. Capture precedes durable start;
the same installed provider binds the persisted run before wake, including
idempotent retry/recovery. No008 merge, downcast, second provider or invented
check/caller identity is added now. Native dispatch remains fail-closed pending
the explicitly authorized round2 base integration. This is a named integration
handoff, not evidence of successful native external writes.


### Final evidence and acceptance audit

Final touched-source task `20261111T122028Z-84608`, tree
`fc160e44c6b3ce817f734a93fa9853614938392e`: **3/3 GREEN**. Command:
`build-slot.sh --remote env TUIC_NEUTRAL_FIXTURE_CAPTURE=1 scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib --success-output immediate -E 'test(workflows::run::runtime::neutral_transport_tests::)'`.
This covers the extracted start boundary and changed fixture capture. The actual
engine-observed catalog, pinned publication/internal graph and retained result
are captured verbatim as JSON in `src-tauri/src/workflows/fixtures/neutral-transport-engine.json`.
It records internal host.increment provenance and output answer8, not a production
native provider or external service success. The artifact is generated data;
its persisted hashes, IDs and paths were not normalized or invented.

Unchanged trigger/schema consumers reuse task `20261111T121133Z-59856`, tree
`e7e1ba89f42205b8aeef06bec22741629de3487b`, nextest
`8519bc92-0b6e-420d-9d13-7454df38a9a6`, **11/11 GREEN**. Command:
`build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(neutral_transport_) | test(workflows::run::neutral_trigger_tests::) | test(workflows::run::neutral_trigger_coalescing_tests::) | test(workflow_schema_keeps_a_margin_under_the_search_budget) | test(workflow_run_schema_exposes_typed_starts)'`.
The touched-source capture task and this run are compatible: only run/neutral_api
was mechanically extracted and the retained-output test gained optional capture;
no trigger/schema production or test file changed afterward. Publication/admission
consumers reuse63104 (31/31). Runtime/route consumers reuse26664 passed cases;
the two failures in that initial run were fixed and subsequently green. No full
suite, clippy, mutation run or critic round was executed. rb toolchain versions
remain **unmeasured** per coordinator; cite these task IDs at landing.

Local validation2 transport Vitest380/380 and Biome GREEN; prior tsc GREEN.
Versions measured locally only: pnpm12.15.0, Vitest5.0.0, rustc1.99.1.
`fmt-changed.sh <this worktree> main...HEAD` completed GREEN after final capture;
`git diff --check` also GREEN. The generated route file came from Vitest -u.

| Criterion | Evidence |
|---|---|
| AC1 shared catalog/metanodes/schema/authority | One neutral_action/DTO service; catalog-publication-preview test checks IDs, internal definition, retained editor metadata and native refusal; publication/admission consumers63104 prove child pins and transitive schema/admission. Designer014 receives actual observed fixture and exact wire contract. |
| AC2 IPC/HTTP/MCP shapes and lifecycle transport | IPC and HTTP call neutral_owner_action with the same NeutralAction/Reply; HTTP/MCP boundary tests pass, snake_case/ordinal maps preserved; no desktop-only neutral API. Low-frequency cursor events reuse SSE; no new token WS stream. |
| AC3 generated parity | Vitest380/380 regenerated paths; registered route probe GREEN26664. No path changed afterward. |
| AC4 dual emit/reconnect | New start/decision commits call publish_neutral_cursor; existing run/api.rs emit_run_cursor dual-emits desktop plus bus and sse_routes.rs bridges WorkflowRunChanged. Capture84608 observes wait/result cursor and retained answer8; events page remains available on reconnect. |
| AC5 labels/client choice do not authorize | Label-only draft changes keep definition digest/pins; denied backend host refuses publication; extra grant/replay fields reject; replay receives non-serde backend provenance. No native caller binding inferred from JSON. |
| AC6 named drift protections | Catches comments name mapping-key loss, Result wrapping, casing/path drift, omitted wait/result cursors, stale/project mutation, client grants, missing internal metanode graphs and trigger overlap/replay drift. Every relevant targeted final run is GREEN. |

Remaining integration handoff is explicit:008 owns typed native provenance and
named-check resolution/default-refusing bind_run;015 prepared one start boundary.
Round2 merges that base once before binding caller/workspace/check names to the
same installed host. Until then native external writes refuse. No live external
write, app restart, desktop launch, push, main/base merge or landing occurred.
Rust changes require the next authorized rebuild/restart to load; this session
does not restart Boss's app. The post-rebuild check is in to-test/workflows-stories-1.md.

### Final owner-authentication edge case

Code audit found that the HTTP `Authenticated` marker means transport admission
and may reflect LAN bypass rather than verified credentials. MCP already uses
CallerAuthority::is_owner. Replay now also requires the HTTP UserAuthenticated
marker or loopback owner through the same CallerAuthority predicate; admitted
remote traffic alone cannot manufacture ReplayAuthority. New boundary coverage
sends the same request with admission-only versus verified-owner extensions:
admission-only must403; verified owner reaches the explicit missing-native-host
service refusal. This exercises the real route, without a live run/effect or
credential/account fixture. Recheck only this added case and the existing HTTP
draft roundtrip affected by the additional request extractor. Existing trigger,
publication, retention and schema cases reuse compatible GREEN evidence above.

HTTP owner-auth task `20261111T122751Z-99136`, tree
`9fd9ba16ddd8d0c6ca314b3b9e6ad24656ddea01`, nextest
`83bb6ff3-59c8-4d03-bbc0-4300d0224125`: **2/2 GREEN**. Command:
`build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(neutral_transport_replay_requires_verified_owner_not_transport_admission) | test(neutral_transport_draft_roundtrip_preserves_mapping_keys_and_project_revision)'`.
No further HTTP implementation or test changes follow this result.

Designer014 requests additional actual serialized authoring replies for its
approved fixture-based integration. The existing publication/preview regression
now optionally prints its already-asserted catalog, draft, updated draft,
publication, clone, preview(input value7) and invalid validation envelopes.
One targeted rerun captures those replies, rather than inventing UI JSON or
claiming a live native-provider integration. Retained inspection remains in the
separate engine observation; their independent project/ID sets are preserved.

### Final authoring fixture delivery

Task `20261111T123303Z-73484`, tree
`cb41c7e172b7f80b14b6c03d88ea46dce886bd97`: **1/1 GREEN**. Command:
`build-slot.sh --remote env TUIC_NEUTRAL_FIXTURE_CAPTURE=1 scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib --success-output immediate -E 'test(neutral_transport_catalog_publication_preview_keep_native_authority)'`.
Only optional observation output changed after the previous compatible result;
all assertions remained in place. `neutral-transport-authoring.json` is parsed
verbatim from the single WER015_AUTHORING_FIXTURE stdout record. It supplies014
with actual draft/update/clone/preview/invalid-validation replies, plus matching
catalog/publication. Its project/IDs remain separate from the retained-run
fixture. Neither artifact claims a production provider or external effect.

Final production changes are covered by99136/59856/84608 as recorded above;
changed authoring capture is GREEN73484. No additional production code changes
follow this run. Remaining edits are these generated observation artifacts,
fixture provenance README and this evidence log. All six criteria are checked.
The native caller/check binding remains the explicit008 round2 integration
handoff above, with fail-closed writes until the integrated base is provided.

### Critic r1 fix round — O1/O2 deletions

COORDINATOR2 unfreezes the branch for the two deletions in
`~/Gits/.tmp/results/critic-015-608c-r1.md` (reviewed912aa358f2).
Verified O1: every production caller selected neutral_owner_action, which derived
the extra NeutralCaller tag from the requested action itself. The inner replay
checks therefore refused only a fixture path and added no production authority.
Deleted the enum/wrapper, caller parameter and both redundant inner guards;
all transports call neutral_action directly. HTTP CallerAuthority::is_owner,
MCP caller.is_owner and local desktop IPC remain the actual replay boundaries.
ReplayAuthority stays backend-only and is constructed after those guards.

Verified O2: TriggerSchema and TriggerBindings return before host/catalog setup
in neutral_action. Their duplicate arms inside triggers::action were unreachable
and are removed. Its extra argument/allow annotation disappear with O1.
The structural direct-Author replay test tail is deleted; public HTTP403 owner
coverage remains. No useful behavioral test is removed and no new test is added.
This is not008 NativeCaller, which remains the typed round2 caller-binding seam.
Docs now describe the real transport boundary rather than the removed wrapper.

Accepted notes: N1 MCP requires verified/loopback owner for all neutral calls,
where HTTP admits non-replay calls through existing progress_auth; MCP is stricter,
with no capability leak. N2 start/command event replies include only the first
500 events; clients page retained events after the cursor/reconnect as documented.
No scope expansion for these accepted observations.

Validation plan: one rb run filtered to `test(neutral_transport_)` covers the
changed HTTP/MCP/shared action/trigger consumers and preserves public owner
refusal. No TypeScript changed, so compatible Vitest evidence is reused. No
critic rerun, full suite, clippy or mutation run.

Fix-round validation: rb task `20261111T125026Z-12137`, staged source tree
`3fd8ddeacf2c94ebeaac8831672cf09d10ab3c8d`, nextest run
`a5b43734-36fd-4abd-927e-7bce04427b91`: **7/7 GREEN**.
Command: `build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(neutral_transport_)'`.
This one run covers changed HTTP/MCP/shared service/trigger consumers, retained
outputs and public owner403. No TypeScript changed; previous Vitest evidence is
reused. rb toolchain unmeasured; no additional test/critic/full suite/clippy run.
Only this evidence log changes after the GREEN source tree. O1 and O2 resolved;
accepted N1/N2 and the008 round2 handoff remain as recorded.

### 2026-11-12T00:57:23.000Z - Coordinator closure (integration record reconciliation, or-3 1787b94b): All acceptance criteria checked with evidence cited in this record; closed per ~/Gits/.tmp/workflow-rework/HANDOFF.md (implementation criteria complete, coordinator closure only). Documented limits in this record stay limits, not passes. No new test run.
