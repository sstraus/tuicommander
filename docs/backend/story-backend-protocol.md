# Story backend protocol (version 1)

A repository can keep its plans and stories in an external backend. TUIC starts
the backend's adapter as a child process and talks to it with this protocol. The
adapter owns story content. TUIC keeps execution state (claims, review and QA
sub-states, receipts, transition history, idempotency keys) in its own SQLite
store, keyed by adapter id and story id.

Code: `src-tauri/src/story_adapter/` (`mod.rs` client and supervisor,
`port.rs` the `StoryBackend` adapter, `binding.rs` settings and watcher hook).
Reference adapter: `scripts/story-adapter-reference/adapter.mjs`.

## Automatic selection and explicit bindings

Automatic selection first retains native while an unarchived, unfinished native plan or a
persisted active native execution exists. The header says **Native retained while …**.
This continuity rule runs before connector discovery and approval; it is not fallback
after connector failure. Otherwise, TUIC selects Wiz when passive project data contains a `.wiz` directory or a
recognized hashed Wiz story filename. A generic `stories/` directory is not a
marker. Story-directory settings come from `.claude/settings.local.json`, then
global Claude settings, then `stories`; absolute paths and parent traversal are
rejected. Linked worktrees read the main checkout's shared story store. Detection
reads data only and never runs a repository script.

The installed plugin registry (`<CLAUDE_CONFIG_DIR or ~/.claude>/plugins/installed_plugins.json`)
suggests the installation of `wiz@lansweeper-tools`. Applicable registry scopes use
local > project > user precedence; project/local entries must map to the canonical
owning project, and unrelated projects do not create ambiguity. Scope never approves
code. Multiple entries at the winning scope fail closed with instructions to select
one installation in the plugin manager, remove the duplicate, then refresh.
The Stories header shows the
resolved path. The user must select **Approve installed Wiz connector** once.
TUIC stores the resolved installation, script path, entry SHA-256 and version in
its machine-owned `<config dir>/story-connectors.json`. Approval pins the displayed **entry/version**, not the full dependency tree. It
applies to projects selecting that installation, not one repository. A changed installation, entry or version needs
new approval. After renewed approval, an entry/version change replaces the bound
process even when its executable arguments are unchanged. The approved connector is `scripts/tuic-story-connector.js`, launched
directly with an absolute Node runtime path and project cwd. Do not use `wiz-run`:
its synchronous timeout wrapper cannot host a long-lived connector.

An unmarked project uses TUIC native SQLite. After the native continuity rule, a detected Wiz project with a missing,
broken, ambiguous or unapproved connector stays unavailable; it never shows native
content instead. Explicit overrides remain in `<config dir>/story-backends.json`:

```json
{
  "/abs/native-project": {"backend": "native"},
  "/abs/wiz-project": {"backend": "wiz-stories"},
  "/abs/legacy-project": {"command": "node", "args": ["/abs/adapter.mjs"]}
}
```

Explicit overrides take precedence over detection. A malformed file or invalid
binding is an error, not automatic selection. Legacy commands remain explicitly
trusted machine settings; repository files cannot supply commands. A project
switches to an external backend only when no native plan is open (unarchived and
not done). A backend cannot change while an execution is running or paused, even after
restart with no bound process. The existing durable run snapshot pins
`storyBackendIdentity` at admission (native or command + approved generation).
Active snapshot project paths use the same canonical owning-project resolver as
story actions. A run admitted before pins existed resolves by the pre-pin rule: the
legacy command binding, otherwise native. Overrides cannot be written while a run is
active, so the current settings still answer it; a backend override on such a project
fails closed. TUIC never infers provenance from possibly colliding plan IDs.
Existing native records stay in SQLite. Desktop and headless use the same selector.

The shared story action `backend_status` returns `{type:"backend_status",value:{
backend,label,provenance,reason,availability,installation,version,error,generation}}` even
when the connector cannot initialize. `provenance` is `automatic` or `explicit`;
`availability` is `available`, `approval_required` or `unavailable`. This status
probe can initialize an already approved connector; passive detection itself does
not start a process. `approve_connector {installation,expected_identity}` is an explicit human UI
write, refused for managed actors and unauthenticated HTTP callers. It re-reads
the installation, registry version and entry digest to reject stale suggestions before
it saves approval; `expected_identity` is the displayed `generation` value. The shared
human-only `set_backend {backend}` action accepts `native`, `automatic` (remove the
override), or `wiz-stories`. The header offers native and automatic selection.
Explicit Wiz still refuses an open native plan; selection writes refuse active runs.
Legacy command-map entries continue to tolerate extra metadata keys.

The dialog performs one final status confirmation after content reads. A changed
selection/generation clears the content and asks for refresh, not a retry loop.
Selection/read overhead has not been benchmarked; no latency claim is made.

Wiz supplies its own plan projection. Protocol 1 has no `wont_fix` value; the Wiz
connector omits those stories. Wiz declares `create_story=false` because TUIC's
protocol does not supply Wiz's required story type. TUIC does not invent a type
or pretend that unsupported operations are available.

## Mapping to the `StoryBackend` port

- `revision` = a hash of `version`. A write compares the caller's revision
  with the current record, then sends that record's `version` as
  `expected_version`.
- Status: `pending` = backlog, `ready`, `in_progress`, `blocked`,
  `complete` = done.
- Legacy aggregate (`set_fields:true`) `update_story` becomes one `apply` per change (`set_fields`,
  `add_criterion`, `add_dependency`, `remove_dependency`). The sequence is
  not atomic: a refusal leaves the earlier operations applied. Editing or
  removing a criterion, file scope, origin, type and risk are unsupported.
- Unsupported: plan creation, plan edit, archive of plans and stories, note
  receipts, unchecking or editing a criterion, `wont_fix`.

## Native workflow preparation boundary

Accepted issue preparation uses native transactional once-only plan creation
and story creation with retained problem text. Native proposal identity and
payload commit with the record; observation never creates a missing record.
The issue URL remains within the 20,000-character problem cap even when the
body is truncated. The version-1 external adapter does not gain new methods:
these workflow capabilities refuse when its port does not implement them.
An external-backend refusal never redirects creation to native SQLite.

## Execution ledger

TUIC keeps execution state in its SQLite store, keyed by adapter id and story
id (`external_story_execution`, `external_story_transitions`; code:
`story_adapter/execution.rs`, `stories/store/external_ledger.rs`):

- Claim: the session that claimed the story.
- Review and QA: TUIC sub-states while the adapter reports `in_progress`. The
  adapter never sees them.
- Transition history: every TUIC transition with its command, actor and the
  adapter version it wrote.
- Integration receipt: the adapter version at a TUIC approval.

A transition writes the adapter first (criterion checks, then `set_status`),
then the ledger. The two writes are not one transaction: a ledger failure
leaves the adapter changed and the error says so. The story revision covers the
adapter version and the ledger row, so a sub-state change is also a new revision.

A story that the adapter reports `complete` without a receipt was completed
outside TUIC. The plan view marks it `completedOutside`. It releases no
dependent: a dependent that the adapter reports `ready` shows as backlog and
cannot be claimed or started until every dependency has a receipt.
- `GET /stories/capabilities?path=<project>` (IPC `story_capabilities
  {project}`) answers the adapter's operations; the Plans and Stories window
  disables the rest. Without `path` it answers the native descriptor.

## Transport

- JSON-RPC 2.0. One JSON object per line (newline-delimited) on stdin and stdout.
- The adapter writes diagnostics to stderr only. TUIC logs stderr lines at debug level.
- One long-lived process for each bound repository. The working directory is the
  repository root.
- TUIC sends requests. The adapter sends responses and, optionally, the
  `changed` notification. The adapter never sends requests.

## Records

Every record carries an opaque `version` string. The adapter changes it when the
record changes. TUIC never parses it.

```json
{"id": "p1", "version": "…", "title": "Release 2"}
```

```json
{
  "id": "s1", "version": "…", "plan_id": "p1", "title": "Fix login",
  "status": "pending", "priority": 2, "problem": "…",
  "criteria": [{"text": "RED: …", "checked": false}],
  "dependencies": ["s0"],
  "notes": [{"at": "2026-11-06T10:00:00Z", "text": "…"}]
}
```

`status` is one of `pending`, `ready`, `in_progress`, `blocked`, `complete`.
An adapter can add fields. TUIC ignores fields it does not know.

## Methods

| Method | Params | Result |
| --- | --- | --- |
| `initialize` | `{protocol, project_root}` | `{protocol, adapter: {id, version}, capabilities: {<method or op>: bool}, watch_paths?: [path]}` |
| `list_plans` | `{}` | `{plans: [plan]}` |
| `list_stories` | `{plan_id?}` | `{stories: [story]}` |
| `get_story` | `{id}` | `story` |
| `create_story` | `{fields}` | `story` |
| `apply` | `{id, expected_version, op}` | `story` |
| `append_note` | `{id, text}` | `story` |
| `changes` | `{cursor}` | `{changed: [id], cursor}` or `{rescan_required: true, cursor}` |
| `shutdown` | `{}` | `{}`, then the adapter exits |

`apply` operations (`op.kind`): `set_status {status}`, `check_criterion {index}`,
`add_criterion {text}`, `add_dependency {id}`, `remove_dependency {id}`,
`set_fields {title?, problem?, priority?}`.

`initialize` must be the first request. TUIC refuses an adapter that answers a
different `protocol`.

`capabilities` keys are method names (`create_story`, `append_note`, …) and
`apply` operation names (`set_status`, …). A missing key means `false`. TUIC
disables the matching UI actions; unsupported server operations still refuse.

Optional protocol-1 `set_title` and `set_priority` flags each authorize exactly
one field in one atomic `set_fields` apply. Missing individual flags mean false.
Aggregate `set_fields:true` implies title, priority and problem support, including
combined metadata in a single apply; it does not imply support for TUIC-only
metadata (type, risk, origin, file scope). No protocol version bump is needed.

The TUIC capability descriptor retains `operations.update_story` unchanged:
native true, external `apply && set_fields`. Additive
`operations.story_edits` contains `{title, priority, problem, criteria_add,
single_change}`. Native supports all four with `single_change:false`. External
mapping is title = `apply && (set_fields || set_title)`, priority analogously,
problem = `apply && set_fields`, criteria_add = `apply && add_criterion`, and
single_change = `!set_fields`. Missing optional external keys grant nothing.
Older UI consumers without `story_edits` retain their existing aggregate behavior.

Restricted saves (`single_change:true`) accept exactly one supported title or
priority patch, or exactly one appended criterion preserving the entire existing
prefix. Unsupported metadata, editor dependencies, multiple metadata fields,
multiple additions and criterion edits/reorders/removals refuse before any apply;
unsupported fields refuse before the port reads content. Each successful save
sends exactly one ApplyOp, never a silent split. The editor disables unsupported
controls and enables Save only for one supported change, with a one-change hint.
Dedicated dependency methods keep their separate semantics. Native writes and
the existing non-atomic legacy aggregate apply sequence are unchanged.

`watch_paths` are absolute paths, or paths relative to the repository root. TUIC
watches them with its own watcher and calls `changes` when one changes. A
file-based adapter does not need a watcher of its own. The answer goes on the
event bus as `stories-changed {repo_path, payload: {rescan, ids, error?}}`
(SSE `/events`). The watch covers the repository tree only; a watch path
outside the repository is not observed. The watcher never starts an adapter.

## Change detection

Correctness never depends on events:

1. Every write carries `expected_version`. The adapter refuses the write with
   `conflict` when the record changed. The adapter must make the check and the
   write one atomic step.
2. TUIC calls `changes` when the watcher sees a change under a watch path,
   every 30 s for each running adapter, and before each decision of a
   workflow run on the project. A `null`
   cursor asks for a baseline: the adapter answers `rescan_required` with a
   current cursor. A cursor the adapter cannot use also gets `rescan_required`.
3. The `changed {ids}` notification is a hint for latency only.

## Errors

| Code | Name | Meaning |
| --- | --- | --- |
| `-32001` | `conflict` | `expected_version` does not match the current record |
| `-32002` | `refused` | A backend rule refused the write; `message` is the reason |
| `-32003` | `unsupported` | The adapter does not support this operation |
| `-32004` | `not_found` | No record with this id |
| `-32601` | method not found | Treated as `unsupported` |

## Supervision (TUIC side)

- A spawn failure, an `initialize` failure, a process exit, or a request with no
  answer within the timeout makes the backend `unavailable`. TUIC stops the
  process and restarts it on a later call. The first restart is immediate; each
  further consecutive failure waits longer (1 s, doubled to 60 s at most). One
  answered request resets the backoff.
- Each started process is a new generation. A cursor from an earlier generation
  is answered with `rescan_required`, because the adapter could have missed
  changes while it was down.
- When the backend is unavailable, reads return `unavailable`. A workflow run on
  the project pauses before its next decision with a cause that starts with
  `story backend unavailable`. The daemon probes the backend every 10 s and
  resumes the run when `changes` answers. Runs on a pull request or an issue do
  not wait. TUIC never reads or writes the native SQLite stories instead.

## Trust

The adapter command is code that TUIC runs. It comes only from
`story-backends.json` in the TUIC configuration folder. No repository file can
name a command, so opening a repository never starts an adapter that the user
did not configure.

## Known limits

- DEFERRED (2026-11-07): selection replacement in `binding.rs` can hold the global `BOUND` lock while
  an adapter call runs, so a hanging adapter can stall other repos' watcher
  callbacks for up to 30 s. Fix: clone the `Arc` and cursor, drop the guard, then call.
- DEFERRED (2026-11-07): a `story-backends.json` key that does not canonicalize
  (`~/x`, a typo, a relative path) is ignored and the project reads native stories.
- A workflow run reads and moves stories through its project's backend: the
  `StoryBackend` port carries the engine transitions (begin, check, submit,
  approve, reopen, history). On an external backend they write the adapter and
  the ledger; dependency readiness needs no reconcile write, because it is
  derived from receipts on every read.
- DEFERRED (2026-11-07): run story proposals (`create_story_once`) on an external
  backend are refused: the ledger has no idempotency row for an adapter create yet.
- DEFERRED (2026-11-07): after a lost response, an automatic run resumes its own
  criterion checks by comparing revisions in order. External revisions are
  hashes, so that resume is reliable for native stories only.
- DEFERRED (2026-11-07): release member reads and the issue hand-off plan still
  use the native store.
- DEFERRED (2026-11-07): closing a session releases its claim in the ledger only;
  the adapter keeps `in_progress`, because that path has no project.
- DEFERRED (2026-11-07): the MCP story tool does not read capabilities. It refuses
  an unsupported action on an external project at call time only.

## Adapter conformance runner

Run the portable protocol checks against the reference adapter:

```sh
scripts/with-test-tmp.sh node --test --test-concurrency=1 scripts/story-adapter-conformance/protocol.test.mjs
```

Set one JSON parameter, `TUIC_STORY_ADAPTER_TEST_TARGET`, to select another
adapter and its store-side fixture. Commands are argv arrays, never shell text:

```json
{
  "adapter": {"command": "node", "args": ["/absolute/wiz/scripts/tuic-story-connector.js"]},
  "fixture": {"command": "node", "args": ["/absolute/wiz-repo/tests/fixtures/tuic-story-backend.cjs"]}
}
```

Run the same command with that environment variable. Without the parameter,
`story-adapter-reference/adapter.mjs` and `fixture.mjs` remain the default.
The runner starts real processes and creates one throwaway store per test
under `TUIC_TEST_TMP_ROOT` or this checkout's `.tmp/` directory. It does not
modify a configured repository or the daily TUIC store.

The fixture receives three additional arguments: `<action> <root> <json>`.
It writes one JSON result to stdout and uses the backend's real store writer:

- `setup`: initialize the empty root and create a pending P2 story titled
  `Seed story`, with problem `A persisted story for protocol conformance`,
  unchecked criteria `first criterion` and `second criterion`, and no notes or
  dependencies. Return `{id, plan_id}`. IDs and plan names belong to the fixture;
  no check assumes `s1` or `default`.
- `create`: create a sibling story with the supplied fields; return `{id, plan_id}`.
- `change`: update the supplied story `id` to the supplied `title`, as a writer
  outside the adapter would. Return `{}`.
- `prepare_complete`: satisfy backend-specific prerequisites such as a work log
  and QA section for `id`, without checking criteria or changing its status.
  Return `{}`. The runner checks criteria through the adapter before this call.

Checks follow the adapter's declared capabilities. Unsupported create and
optional field writes must answer `unsupported` or method-not-found without
mutation, rather than fail a reference-store assertion. Two optional capability
keys refine `set_fields`: `set_fields_problem` and `set_fields_multiple`. Declare
them true only when the adapter supports problem edits and one atomic write of
multiple fields, respectively. Missing keys mean false. The reference adapter
declares both true; adapters with a one-field CLI writer can declare both false.
The runner reports which unsupported branches it verified.

The Rust tests in `story_adapter/` still cover TUIC client supervision, port
routing and the execution ledger with the reference store. They are distinct
from this adapter-independent stdio/store conformance runner. A passing protocol
run is not a claim that those Rust integration tests ran against another store.

The reference adapter and its fixture serialize writes with an exclusive store
lock, so two processes cannot both acknowledge writes from one expected version.
A lock timeout fails closed. After a writer crashes while holding the lock,
stop all writers before removing `<store>.lock`; the reference implementation
does not expire a lock by age, which could break a live writer's atomicity.
