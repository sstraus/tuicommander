# HTTP API Reference

## Request authentication and browser boundary

Every TCP request validates its Host and Origin before reaching a handler, including login, health and preflight requests. Unknown/missing/duplicate Host or foreign/opaque Origin returns 403. Allowed hosts are localhost, loopback/private IP literals, local interface IPs and the detected Tailscale FQDN. Allowed origins are the exact bundled WebView origins, Vite `http://127.0.0.1:1421`/`http://localhost:1421`, and the HTTP/HTTPS origin of the validated Host. Cross-site requests are rejected except for the explicit WebView/development origins.

Protected routes require the existing `?token=...`, `tui-session` cookie or Basic Auth even from loopback/LAN. Native HTTP clients may omit Origin, but must send Host and credentials. Login assets and valid preflight remain public; the headless `/health` probe remains public. Local Unix socket and Windows named-pipe clients keep their existing IPC access.

HTML GET navigation to `/` and `/mobile` uses `/mobile/login` when credentials are missing and a password is configured. After login, `next` may return to `/` (including its query) or a mobile path. API calls and non-HTML requests keep the existing 401 Basic challenge.

## Workflow runs

Slice F exposes `start_graph {target:{type:story|plan,id},expected_revision?,definition_id,definition_revision,request_id,limits?}` through the existing owning-daemon service. Story starts require the current native revision and pin the selected publication; the request ID is bound to its payload. Omitted limits use Rust defaults. Plan dispatch remains unavailable in this build and its start control says so. The `workflow_run` MCP tool uses an inline schema generated from `RunAction` and the public `RunCommand` variants; it returns the same scoped snapshots and cursor-ordered events as IPC/HTTP. Run history shows pinned activations, decisions/evidence, repair counters, pause targets and complete event payloads across pages. Graph recovery uses `resume_graph {execution_id,activation_id,resolution}` after answering pending input; pause and cancel use the existing sequence-fenced commands. Legacy runs offer inspection and cancellation in the UI. No new persistence or client scheduler is added.

`POST /workflows/run/action?path=<absolute-project>` accepts one tagged `RunAction` and returns `{type,value}`. Actions: `start_plan {plan_id,definition_id,definition_revision,limits}`, `get {run_id}`, `incidents {run_id}` (read-only causes and manual next steps), `list_plan_runs {plan_id,limit}`, `events {run_id,after_sequence,limit}`, `command {run_id,command_id,expected_sequence,command}`, `execute_check {run_id,story_id,check_id,command_id,expected_sequence}`, `record_integration {run_id,story_id,command_id,expected_sequence}`, and `recertify_canonical {run_id,command_id,expected_sequence}`. `list_plan_runs` returns newest first and accepts a limit of 1–100. Run commands include planning closure, agent attempt and effect bookkeeping, loop advancement, story acceptance, final verification, pause/resume, cancellation, and completion. The server checks canonical project ownership for every action and rejects a command whose expected sequence is stale. Event cursors start at zero and return up to 500 entries. A duplicate command ID returns its original receipt only when the payload matches. See [Workflow runs](../backend/workflows.md) for recovery and completion rules. Serial story graph execution is available through `start_graph`; `start_plan` remains the legacy record-only ledger.

The `command` action also accepts `answer_input {attempt_id,answer}` for a paused run. The answer is an idempotent durable event; graph runs resume separately with an explicit activation and resolution. An unanswered input request prevents resume.

`start_plan.limits` accepts `maxParallelStories` (default 2 when omitted, range 1–8). `start_attempt` conservatively serializes unknown or overlapping file scopes and refuses dependent stories lacking a current integration receipt at the accepted revision. `execute_check` runs only a check from the pinned story definition in its assigned worktree; `record_integration` verifies a completed non-fast-forward merge and runs the pinned checks again on the canonical result. `recertify_canonical` validates a later clean canonical tip and reruns those checks without claiming a new merge. `assign_worktree`, `record_check`, `record_integration`, and `record_recertification` as nested commands are internal and rejected on this operator API.

## Workflow definitions

`POST /workflows/definition/action?path=<absolute-project>` accepts one `WorkflowAction` object and returns `{ type, value }`. Actions are `seed_templates`, `create_draft {name,kind,graph}`, `list_drafts`, `get_draft {id}`, `update_draft {id,expected_revision,graph}`, `update_closure {id,expected_revision,closure}`, `update_checks {id,expected_revision,checks}`, `publish {id,expected_revision}`, and `get_published {id,revision}`. `closure` is `human` or `automatic`, defaults to `human`, and `automatic` cannot be published until its evidence gate exists. Each check has `id`, `argv`, and camel-case `timeoutSecs`; the published revision pins its `requiredChecks`. A graph has `nodes` and `edges`; see [Workflow definitions](../backend/workflows.md) for node, outcome, and publication rules. The path resolves to the canonical owning project. Published revisions are immutable. This endpoint defines workflows; it does not start one.

## Native stories

`GET /stories/capabilities` returns JSON `true` when the running backend supports the native stories dialog. The frontend probes it once per dialog opening, recognizes a missing route by HTTP 404, and preserves other HTTP failures as errors.

`POST /stories/action?path=<absolute-project>` accepts `{ "action": StoryAction, "sessionId"?: string }` and returns a tagged `StoryReply` (`{type, value}`). `StoryAction` uses a snake-case `action` discriminator: `create_plan`, `list_plans`, `list_plan_sources`, `add_plan_source`, `get_plan`, `plan_state`, `plan_view`, `create_story`, `list_stories`, `get_story`, `transition_history`, `add_dependency`, `remove_dependency`, `claim`, or `transition`. Create-story input uses the shared camel-case `NewStory` fields. `create_plan` takes `title` and `source`; `add_plan_source` takes a local document `source` and derives its title from front matter or the first heading. `list_plan_sources` reads top-level Markdown files in `plans/` and `.claude/plans/`, so a new file appears on the next call. It returns `{type:"plan_sources",value:[{title,source}]}`. `get_plan`, `plan_state`, `plan_view`, and `list_stories` take `plan_id`; `get_story` and `transition_history` take `story_id`; `claim` takes `story_id` and `expected_revision`; `transition` also takes a `command`; both dependency actions take `story_id`, `dependency_id`, and `expected_revision`. `transition_history` returns committed transitions with their resulting revisions, commands, and actor provenance.

Story transition provenance comes from host-validated credentials: desktop IPC and credential-authenticated HTTP record `human`; sessionless local HTTP records `local_api`. Managed calls record their session identity, including when the claiming session approves its own story. Actor identity never restricts actions; state, revision, project and claim-conflict rules remain enforced.

`plan_view` returns `{type:"plan_view",value:{stories,state,wontFixCount,allCancelled}}`. Each story in `stories` includes a read-only `abandoned` boolean, derived from whether that story or any dependency reachable from it is WontFix. The summary and state are derived from the same read; no abandonment flag is persisted.

The project path is resolved to its canonical owner, so a managed worktree shares its parent project's plans. Unknown action and create-story fields are rejected. Claim requires a live PTY session in that project. `transition` accepts the `start_manual` command to begin a Ready story without a terminal. Actor identity is tracking only; managed, local API and human callers use the same transition rules. `remove_dependency` accepts only a Backlog dependent with a direct WontFix prerequisite; it returns the revised story, Ready only when all remaining prerequisites are Done. WontFix never satisfies dependencies. A nonempty plan with only Done/WontFix stories has `plan_state: done`, while an empty plan is `draft`. Reads and writes verify the stored plan's project; a story ID alone grants no cross-project access. Revisions are required for mutations to detect stale clients. The same service backs desktop IPC and MCP. There is no import or export endpoint.

## Project Progress

Six `POST` routes and one `GET` route, all with normal route authentication.
The mobile PWA uses `GET /progress/projects` to discover journal projects,
newest activity first. The `POST` routes other than `/progress/flow/detail`
require an explicit `path` query naming a registered project.

| Route | Body | Response |
|---|---|---|
| `/progress/report` | `{ type, text, step? }`, `type` is `done` or `blocked` | `{ id }` |
| `/progress/list` | `{ blockedOnly?, ptyId?, limit?, cursor? }` | `{ project, entries, total, nextCursor, ptyIds, lastViewedMs? }` |
| `GET /progress/projects` | none | Array of project paths with journal entries, newest first |
| `/progress/delete` | `{ ids }` | `{ deleted }` |
| `/progress/viewed?ptyId=<id>` | none | `{ lastViewedMs }` |
| `/progress/flow` | `{ ptyId? }` | `ProgressFlow` |
| `/progress/flow/detail` | `{ ptyId, agentId, part }`, `part` is `prompt` or `report` | `{ text }` |

Unknown fields are rejected. `text` is capped at 500 characters and `step` at 80.
All Progress entry kinds redact secret-shaped text and step fields before
storage; agent and target names are redacted and capped at 80 characters.
Omit `ptyId` on `/progress/viewed` to mark the repository aggregate; supply it
to mark one PTY. Each scope keeps a separate last-visit timestamp.
`intent` is a valid entry *kind* but not a reportable one — TUIC writes those
itself from the agent's `intent:` marker, and `/progress/report` refuses one.

`ptyId` selects one PTY; omitting it returns the repository aggregate. Each
entry includes `ptyId` when TUIC knows its source. Older entries and direct
IPC/HTTP reports have no PTY ID and remain visible in the aggregate. `ptyIds`
lists the PTYs with stored history, including closed PTYs. The list is
newest-first in pages of 8 entries by default. `limit` is clamped to 1–100;
`total` counts entries matching the filters before the cursor, and `nextCursor`
is `null` after the last page. Pass it back as `cursor` to read older entries.
There is no revision: the journal is append-only, so an entry is written once
and either kept or deleted. The one exception is a host-written `intent` that
repeats that PTY's newest intent (same text, same agent): it returns the
existing entry, because a screen repaint is not a new intent. `delete` is scoped
to the project in the query, so one project cannot delete another project's row
by id.

Recorded entries are published as `progress-recorded` on `/events` with
`{ entry }` as the payload.

`delegated` and `message` are host-written like `intent`. TUIC writes a
`delegated` entry when a terminal runs `agent action=spawn` and a `message`
entry when it runs `agent action=send` to a peer that has a terminal. The entry
is attributed to the sender's PTY and adds `targetPtyId` and `targetName`. Its
text is the prompt or message, redacted first and then cut to 500 characters;
the full text is not stored. `/progress/report` refuses both kinds.

`/progress/flow` draws the journal as a delegation sequence. All ordering,
joining and redaction happen in Rust:

```
ProgressFlow = {
  project, truncated,                    // truncated: journal > 500 entries
  participants: [{ id, kind: "terminal"|"subagent", title, agentType?,
                   state: "busy"|"idle"|"awaiting"|"closed"|"running"|"done",
                   parent?, intent?, toolCalls, ptyId, agentId? }],
  events: [{ id, kind: "intent"|"done"|"blocked"|"delegated"|"message"
                  |"subagent_spawn"|"subagent_return",
             from, to?, summary, text?, detail?: { ptyId, agentId, part },
             step?, atMs }]              // oldest first
}
```

A terminal is a participant when it wrote an entry or was the target of one. A
subagent id is `<ptyId>/<agentId>`. Participants come parents first. A child's
`done`/`blocked` has `to` set to the terminal that delegated to it; without a
parent, `to` is absent and the event is a note on its own column. Subagent
spawns and returns are read from the Claude transcripts of open terminals only.
`summary` is redacted and at most 200 characters. `text` is the whole redacted
journal text, present only when the summary is shorter. `detail` marks a
subagent arrow whose full prompt or report must be fetched from
`/progress/flow/detail`; that route looks `ptyId` up in `AppState` and compares
`agentId` with the subagents listed on disk, so neither value can reach a path.
An event `id` is stable across reads (`entry:<journal id>`, or
`<ptyId>/<agentId>:spawn` / `:return`); key client state by it, never by
position. Past 64 subagents per terminal, the newest are kept plus every one
still running. Both routes run on the blocking pool.
With `ptyId`, the flow keeps that terminal, its direct parent and its direct
children.

REST API served by the Axum HTTP server when MCP server is enabled. All Tauri commands are accessible as HTTP endpoints.

## Base URL

- **Local (Unix socket):** `<config_dir>/mcp.sock` — always started on macOS/Linux. No auth, MCP always enabled. Used by the local MCP bridge binary.
- **Remote (TCP):** `http://<host>:{remote_access_port}` — only started when remote access is enabled in settings. HTTP Basic Auth required.

## Authentication

- **MCP mode (localhost):** No authentication
- **Remote access mode:** HTTP Basic Auth with configured username/password, or
  the `?token=` session token the QR code embeds.

Both halves of the credential pair are required: `validate_basic_auth`
(`mcp_http/auth.rs`) reports `NotConfigured` when either the username or the
password hash is empty, and the server then answers **every** Basic Auth attempt
with `401` and the body `Scan the QR code or authenticate with Basic Auth` — the
same response a request with no `Authorization` header gets. A 401 carrying that
body while credentials were sent means the pair is incomplete, not wrong.

### `GET /api/auth/session-token`

Trades an authenticated Basic Auth call for the running session token:

```json
{ "token": "b0b9…" }
```

It is in `shared_routes()`, not the desktop router, on purpose — the daemon
`run_remote` starts builds `build_remote_router`, so a route registered only in
`build_router` would 404 on the one server that needs it.

This is how a client escapes header-only Basic Auth. A WebSocket upgrade cannot
carry an `Authorization` header, and with `remote_auth` on the server answers
`Access-Control-Allow-Origin: *`, which forbids credentialed cookies; `?token=`
is the only credential HTTP, WS and SSE can all carry. `503` means the server
has no session token configured.

`tuic-remote` generates its token on the first start and stores it in the
credential vault like the desktop does, so a restart keeps the token and every
paired phone stays logged in. A host with no usable vault falls back to a token
that lives for one run (logged as a warning); a client re-fetches the token on
every connect either way.

### Session cookie

A successful login (QR token, Basic Auth or the form below) sets the
`tui-session` cookie. It is **sliding**: every authenticated request re-issues
it with a fresh `Max-Age`, so a device in use never expires. The lifetime is
`services.auth.session_token_duration_secs`, **30 days** (2592000) by default —
the longest a device may sit unused before it must log in again. Rotating the
session token logs every device out.

### Mobile login form: `/mobile/login` and `POST /auth/login`

A page navigation of the mobile app (`GET /mobile`, `/mobile/*`, `Accept:
text/html`) without a valid session is redirected (`302`) to
`/mobile/login?next=<path>` when a username and password are configured. An iOS
home-screen app never shows the native Basic dialog, so the in-app form is the
only way back in. Every other unauthenticated request keeps the `401` +
`WWW-Authenticate: Basic` contract, and so does a navigation when no password is
configured.

Public without a session: `GET /mobile/login`, `GET /mobile-login.js` and
`POST /auth/login` (nothing else).

```
POST /auth/login
Content-Type: application/json
{ "username": "…", "password": "…", "next": "/mobile/session/a" }
```

- `200 {"ok":true,"next":"/mobile/session/a"}` + `Set-Cookie: tui-session=…`.
  `next` is returned only if it is inside `/mobile`, otherwise `/mobile`.
- `401` wrong credentials or no password configured; no `WWW-Authenticate`.
- `403` cross-origin: `Sec-Fetch-Site` must be `same-origin`, or, when the
  browser sends none (plain HTTP on a LAN address), `Origin` must match `Host`.
- `415` not `application/json`; `400` malformed body; `413` body over 4 KiB.
- `429` + `Retry-After`: the per-IP failure budget is spent.

Credentials are checked by the same code as the Basic fallback (per-IP rate
limit, failed-credential cache, bcrypt on a blocking thread), so form and Basic
failures spend one shared budget. The service worker never caches
`/mobile/login` or a non-200 response as the offline shell.

## Server Limits

Both the desktop and remote routers apply request bounds
(`with_server_limits` in `mcp_http/mod.rs`):

- **`408 Request Timeout`** — a handler that has not produced a response within
  301 s is cut off. This includes slow worktree creation with warm build artifacts
  while still bounding a wedged handler. 301 s is a deliberate 1 s margin above
  `ui action=confirm`'s own 300 s answer window, so that handler's own timeout
  body always wins the race instead of a bare 408 (`docs/backend/mcp-http.md` →
  "Server Limits"). SSE (`/events`) and WebSocket endpoints return their
  headers immediately and then stream for as long as they like, unaffected.
  Streamed `/fs/upload-copy` uses a 30 s idle deadline per body chunk instead
  of this total response deadline. A separate 17-minute total receive budget
  also prevents trickling clients from retaining an upload slot.
- **`413 Payload Too Large`** — a request body over 2 MB is refused rather than
  buffered.

See [`docs/backend/mcp-http.md`](../backend/mcp-http.md) → "Server Limits" for
the rationale and the tests that pin both.

## Unknown Paths

The desktop server also serves the frontend, so any path that matches no route
falls through to a catch-all. That catch-all splits on the first path segment:

- **API path** — the first segment is one of `agent`, `agents`, `ai`, `api`,
  `attachments`, `audio`, `claude`, `codex`, `config`, `debug`, `diagnostics`, `dictation`,
  `events`, `exec`, `fs`, `generators`, `github`, `health`, `logs`, `mcp`,
  `metrics`, `plugins`, `process`, `prompt`, `registry`, `repo`, `sessions`,
  `stats`, `system`, `terminal`, `tunnels`, `watchers`, `worktrees`. The
  response is `404` with `{ "error": "no such endpoint: /<path>" }`.
- **Anything else** — a deep link such as `/settings` or `/mobile/session/<id>`.
  The response is `200` with the SPA shell (`index.html`, or `mobile.html` under
  `/mobile`), so client-side routing takes over.

A registered path called with the wrong method still returns `405`, not `404`.
The catch-all is the router's fallback, so it answers on **every** method: an
unregistered path returns `404` whether it was reached with `GET`, `POST` or
anything else. It used to be a `GET`-only route, which made any non-`GET` call
to a path that does not exist answer `405` — a reply that claims the path is
real. The parity gate below depends on telling those two apart.

Without this split an unregistered API path answered `200` with HTML, which the
client read as success and then failed to parse as a command result — every
missing route looked like a malformed response. The prefix list is checked
against the registered routes by a test, so a new route family cannot silently
drop back to the HTML answer.

The `tuic-remote` daemon embeds no frontend and has no catch-all: unknown paths
there return a bare `404`.

## Route Parity Gate

Every `COMMAND_TABLE` entry in `src/transport.ts` must resolve to a registered
route. Two tests enforce it, one per language:

1. `src/__tests__/transport.test.ts` executes every mapper and snapshots the
   resulting paths to `src-tauri/src/mcp_http/command_table_paths.txt`.
2. `mcp_http::tests::command_table_paths_all_hit_a_registered_route` reads that
   file and `PATCH`-probes each path against `build_router`. `PATCH` is used
   because no route accepts it, so a registered path answers `405` without
   running its handler, while an unregistered one falls through to the
   catch-all. A `404` or an HTML body fails the test.

Adding a command therefore fails the Vitest half first (stale snapshot).
Regenerate with:

```bash
pnpm vitest run src/__tests__/transport.test.ts -u
```

If the route was never registered, the Rust half then fails. Both run in
`make check`. The gate proves the **path** exists, not that it accepts the
method the table declares — probing the declared method would execute the
handler, which is what the technique avoids.

`INTENTIONALLY_UNMAPPED` commands never enter the snapshot: they are not
`COMMAND_TABLE` entries, and the Vitest half asserts the two sets stay disjoint.

## Session Endpoints

### List Sessions

```
GET /sessions
```

Returns array of active session info (ID, cwd, worktree path, branch,
`display_name`, `display_name_is_custom`, `display_name_from_spawn`,
`is_remote`, optional `pty_description`, optional terminal `alias`, optional
`parent_session`, optional `tuic_session`, and nested state). The
`alias` field is the only record of a tab's alias after a WebView reload, because
`term-alias-assigned` fires once, at spawn; `parent_session` is the same for the
sub-agent tag, which `session-created` publishes once. It holds only a resolved
parent, never a `pending-mcp:` placeholder.
`tuic_session` identifies the live agent bound to this PTY, which may differ from
`session_id`; clients use it to name a spawned child's parent. It is omitted
when no live identity is bound. `display_name_from_spawn` is true when
`agent action=spawn` named the session and no user rename has replaced it; a
non-custom name synced back from an OSC or intent title does not set it. The
origin fields let browser and desktop clients preserve manual-title protection
and remote-completion muting across reconnects. For detected agents,
`state.agent_state` distinguishes PTY
silence (`idle`) from explicit protocol completion (`completed`); the latter
requires a parsed `suggest: [ ... ]` marker. Other values are `starting`,
`working`, and `awaiting_input`. `state.background_work` is true when meaningful
non-helper descendants keep autonomous work alive despite an input-ready
terminal (`state.shell_state == "idle"`).

Sessions running on a **connected remote machine** are in this list too, each
with `connection_id` naming the connection that owns it; a local row has no such
field (#791-055e). The rows come from `remote_mirror.rs`, which reads the remote
daemon's own `GET /sessions` and then follows its `/events`. The Tauri command
`list_active_sessions` answers with the same rows, from the same builder.

### Create Session

```
POST /sessions
Content-Type: application/json

{
  "rows": 24,
  "cols": 80,
  "shell": "/bin/zsh",    // optional
  "cwd": "/path/to/dir",  // optional
  "alias": "tu-1"         // optional — reclaim a persisted alias
}
```

Returns `{ "session_id": "..." }`.

When `cwd` is missing or names a file, the server returns `400` with `{ "error": "Working directory ..." }` before registering a PTY session.

`alias` lets a client restore the short address a tab had before a restart. The server
honours it only when it still has the `<prefix>-<number>` shape and no live session
holds it, and then raises the per-prefix counter past that number so the next
auto-assigned alias cannot collide. Anything else is ignored and the session receives a
freshly minted alias.

### Create Session with Worktree

```
POST /sessions/worktree
Content-Type: application/json

{ "pty_config": { ... }, "worktree_config": { ... } }
```

Creates a git worktree and a PTY session in one call.

### Spawn Agent Session

```
POST /sessions/agent
Content-Type: application/json

{ "agent_type": "codex", "prompt": "Fix the bug", "args": ["resume"] }
```

Spawns an AI agent in a PTY session. The request is flat; browser transport sends
only the HTTP spawn fields from the desktop `pty_config` and `agent_config` objects.
It uses `agent_config.cwd` when present, otherwise `pty_config.cwd`. Desktop-only
PTY fields such as `shell`, `tuic_session`, and `alias` are omitted. The optional
`env` map and `model` string use the same field names as desktop IPC spawn. By default,
supported interactive CLIs use native scrollback according to the agent's
`prevent_alt_screen` setting.
The child receives its own `TUIC_SESSION`, equal to the returned `session_id`,
and `GET /sessions` exposes that identity as `tuic_session` immediately.

### Write to Session

```
POST /sessions/:id/write
Content-Type: application/json

{ "data": "ls -la\n" }
```

### Submit One Managed-Agent Reply

```
POST /sessions/:id/submit
Content-Type: application/json

{ "input": "Please wait for my approval" }
```

Authenticated browser/PWA counterpart of `session action=submit`. It checks the
managed agent's idle state and empty composer, writes the whole reply and Enter
atomically, and returns the same submission receipt. A closed session returns
HTTP 404 with `submitted: false` and `reason: "session_not_found"`; another
rejection returns HTTP 409 and its precise `reason`. A successful write can
still have `acknowledged: false`: the client must not retry blindly. While a
confident question is open, a human reply can pass queued automated messages;
those messages remain parked until the question clears. Automated `session
action=submit` cannot answer the question.

### Write Several Inputs at Once

```
POST /sessions/:id/write-parts
Content-Type: application/json

{ "parts": ["/", "help", "\r"] }
```

One round trip and one PTY writer lock for the whole batch, but the parts stay
separate: the backend applies its post-write bookkeeping once per part. This is
**not** the same as joining them into `/write` — that bookkeeping reads each part
as one keystroke, so a lone `/` opens slash mode and an exact option key answers
a choice prompt, and a joined payload matches neither. The transport uses this
route when keystrokes arrive while an earlier write is still in flight; a
solitary keystroke keeps the plain `/write` route.

### Queue a Command for the Next Idle Window

```
POST /sessions/:id/queue
Content-Type: application/json

{ "text": "run the tests", "idempotencyKey": "bg-job-1" }
                                  -> { "accepted": true, "typed": false, "queued": 2 }

GET /sessions/:id/queue            -> [ { "id": 7, "text": "run the tests" } ]

DELETE /sessions/:id/queue         -> 2   (commands dropped)

DELETE /sessions/:id/queue/:cmdId  -> true  (false when it already drained)
```

Hands the text to the same idle gate peer messages use instead of typing it now:
submitted immediately when the agent is idle (`typed: true`, `queued: 0`),
otherwise parked until the agent's next busy→idle transition, so a running turn
is never steered. User commands and peer messages share one typed FIFO and are
submitted one per idle window in backend acceptance order (a run of hands-free
voice entries at the head is joined into one submission). `queued`,
`state.queued_commands`, and `DELETE` count or remove only user commands;
clearing Compose commands never deletes pending peer/orchestrator delivery.

`idempotencyKey` is optional. Use the same key for retries of one logical
command; distinct commands need distinct keys even when their text is identical.
Keys contain 1–128 UTF-8 bytes. The backend
remembers the last 128 accepted keys per live PTY, including drained or cancelled
entries. A recognized retry returns `accepted: true`, `typed: false` and the
current queue depth without appending or flushing again. `accepted` confirms
queue acceptance, not a model turn. Keys expire on eviction, PTY teardown or
backend restart; this is an in-memory retry window. Omitted keys preserve the
usual append behavior. HTTP and Tauri use the same request and response fields.

Agent sessions only — `400` for a plain shell (`"Session is not running an
agent"`) or empty text, `404` when the PTY is gone. The current depth is also on
every session snapshot as `state.queued_commands` (omitted when zero).

### Resize Session

```
POST /sessions/:id/resize
Content-Type: application/json

{ "rows": 30, "cols": 120 }
```

### Read Output

```
GET /sessions/:id/output?limit=4096&format=text
```

Returns recent output. Format controls what is returned:

| `format` | Response shape | Description |
|----------|----------------|-------------|
| (omit) | `{ "data": "<string>", "data_length": N, "total_written": N }` | Raw PTY output as a lossy-UTF-8 string (not base64), read from the ring buffer |
| `text` | `{ "data": "<string>", "data_length": N, "total_written": N }` | One canonical terminal-grid snapshot, joined by `\n` (not from the ring buffer) |
| `log` | `{ "lines": [...], "total_lines": N, "screen": [...], "input_line"? }` | VT100-extracted clean lines (no ANSI, no TUI garbage) plus current screen rows and optional input line |

| Param | Default | Description |
|-------|---------|-------------|
| `limit` | raw: 8192 bytes; text/log: all | `raw`: max bytes; `text`/`log`: max lines to return |
| `offset` | (tail) | `text`/`log`: absolute start row/line offset. When omitted, returns the newest `limit` rows/lines. When provided, returns data starting from that offset |
| `format` | (raw) | See table above |

`format=log` reads from `VtLogBuffer` — a VT100-aware buffer that extracts only scrolled-off lines, suppressing alternate-screen TUI apps (vim, htop, claude). Ideal for mobile clients.

`format=text` is a point-in-time canonical grid view. It does not concatenate
the finalized log cursor with the visible screen, because growing a viewport can
move history rows back onto the screen and make that concatenation overlap.
`total_written` is the snapshot's total grid-row count for this format.

`total_lines` in the response is a monotonically increasing counter — it never decreases when old lines are evicted from the buffer. Use it as a stable cursor for paginated reads. The `offset` parameter operates in the same coordinate space.

### Kitty Protocol Flags

```
GET /sessions/:id/kitty-flags
```

Returns the current Kitty keyboard protocol flags (integer) for a session.

### Foreground Process

```
GET /sessions/:id/foreground
```

Returns the foreground process info for a session. Detection uses the spawn-recorded root role and foreground process group. Returning to a shell root revokes an observed agent; a child of a direct agent holds unattended input without revoking its identity. Unknown root ownership refuses unattended input. Concurrent observations apply in generation order.

### PTY / Terminal Read State

```
GET  /sessions/:id/shell-state                         -> { "state": "busy"|"idle"|null }
GET  /sessions/:id/shell-family                        -> "posix"|"windows-native"|"unknown"|null
GET  /sessions/:id/last-prompt                         -> { "prompt": string|null }
GET  /sessions/:id/input-buffer                        -> { "content": string }
GET  /sessions/:id/leaf-pid                            -> { "pid": number|null }
GET  /sessions/:id/chat-view?from_seq=N&epoch=E        -> { epoch, nextSeq, reset, updates[], unknownRows, malformedRows }
GET  /sessions/:id/has-foreground                      -> { "process": string|null }
POST /sessions/:id/visible              { "visible": bool }   -> { "ok": true }
GET  /sessions/:id/terminal/selection-text?startRow=&startCol=&endRow=&endCol=&historyBase=  -> { "text": string }
GET  /sessions/:id/terminal/logical-line?row=N         -> [logicalStartRow, text]
GET  /sessions/:id/terminal/hyperlink-span?row=R&col=C -> [startCol, endCol, url] | null
GET  /sessions/:id/terminal/styled-rows?start=N&count=N -> application/octet-stream (packed rows)
GET  /process/stats                                    -> ProcessStats[]
```

`chat-view` is the transcript of a Claude terminal as ACP `SessionUpdate`s, tailed from the agent's own session file (never the grid). `updates` are the entries from `from_seq` in the `epoch` the client last saw; `reset: true` means the client holds a different conversation (`/clear`, new session, file truncated) or fell behind the bounded log, and must drop what it has and apply `updates` from scratch. A terminal with no bound Claude agent answers 500 `{"error":"not_bound: <reason>"}`. A read keeps the tail alive for 20 s; the server wakes clients with the `chat-view-changed` event while one reads.

The `/agents/map`, `/agents/map/data` and `/agents/map/prompt` routes were
removed on 2026-09-23. The Progress Flow view (`/progress/flow`, see Project
Progress above) replaced them.

`terminal/styled-rows` fills the CanvasTerminal client-side row cache and answers
**binary**, not JSON: a 64-row chunk is ~141 KB of packed cells, which as a JSON
number array becomes ~350 KB of decimal text for the client to parse back into the
bytes it started as. The desktop `terminal_styled_rows` command returns the same
payload raw (`tauri::ipc::Response`), and `rpcImpl` decides between
`arrayBuffer()` and `json()` on the content-type alone. An empty body means "no
such session or range" — a valid empty chunk, not an error.

`shell-family` classifies the session's shell so the client picks the right control
sequences (Ctrl-U is line-kill under POSIX readline, a literal character on
`cmd.exe`/PowerShell). It answers the bare value, **not** a `{field}` wrapper,
because `get_session_shell_family` returns `Option<ShellFamily>` bare over IPC and
`src/utils/sendCommand.ts` reads both transports with the same code. An unknown
session is `null`, which is the same "fall back to the host default" answer.

Read-only PTY/terminal state mirroring the desktop Tauri commands (story 062). The
`{field}`-wrapped responses are unwrapped by the frontend transport to match the
command's bare return (e.g. `Option<String>` → `null`). The desktop-only commands
themselves are absent from the remote binary, so these handlers read `AppState`
directly.

`terminal/selection-text` reads grid-relative scrollback coordinates (row zero
is the oldest retained row), rejoins
soft-wrapped rows, trims terminal padding, and removes only coherent multi-line
Claude `NBSP NBSP ▎` visual gutter runs. Desktop IPC returns the same string.
The optional `historyBase` is the evicted-row count from the frame used to
select the text. When supplied, the backend rebases both row coordinates against
the current history while holding the grid lock; an evicted selection is
rejected with HTTP `409` and `{"error":"selection rows are no longer retained"}`
instead of reading replacement rows. Omitting it retains the original
current-grid coordinate semantics.

### Pause/Resume

```
POST /sessions/:id/pause
POST /sessions/:id/resume
```

### Rename Session

```
PUT /sessions/:id/name
Content-Type: application/json

{ "name": "my-session", "isCustom": true }
```

Sets a display name and its origin. `isCustom: true` protects an explicit user
rename from subsequent OSC/intent titles; spawn-assigned and dynamic titles use
`false`. Omitting the field preserves the legacy custom-rename behavior.

### Close Session

```
DELETE /sessions/:id?cleanup_worktree=false
```

## Design Mode Endpoints

The inspector opens a dedicated Chrome window on the machine running
TUICommander. It binds to an agent terminal, even when the request comes from
a browser/PWA client. The page URL is read from the local per-repository
`dev_server_url` setting; an unset URL opens `about:blank`.

### Start or rebind inspection

```
POST /design-mode/start
Content-Type: application/json

{ "sessionId": "<agent session ID>" }
```

Starting another terminal in the same repository reuses its Chrome window and
changes the bound session. A plain shell or unknown session is refused, and
so is a second start while the repository's Chrome window is still launching.

### Stop inspection

```
POST /design-mode/stop
Content-Type: application/json

{ "repoPath": "/path/to/repository" }
```

### Read status

```
GET /design-mode
```

The status endpoint returns an array of `{ "repoPath": string, "sessionId":
string, "status": "armed" | "stopped" }` objects. Start, stop and status use the same backend state as
their Tauri command counterparts. Status changes also arrive as the
`design-mode-changed` event on `GET /events` with the snake_case payload
`{ "repo_path": string, "session_id": string, "status": "armed" | "stopped" }`.
A grab is
prefilled into the bound agent draft, never submitted by these endpoints.

## Streaming Endpoints

### WebSocket PTY Stream

```
WS /sessions/:id/stream
```

Receives real-time PTY output as text frames. One WebSocket per session.

### WebSocket JSON Framing (Mobile/Browser)

WebSocket connections to `/sessions/:id/stream` receive JSON-framed messages:

```json
{"type": "output", "data": "raw terminal output text"}
{"type": "parsed", "event": {"type": "question", "text": "Allow?"}}
{"type": "watcher-lines", "session_id": "abc", "lines": [{"text": "clean line", "matched_ids": ["<client_id>/w0"]}]}
{"type": "exit"}
{"type": "closed"}
```

Frame types:
- `output` — Raw PTY output (ANSI-stripped when `?format=text`)
- `log` — VT100-extracted clean lines batch (when `?format=log`): `{"type":"log","lines":[...],"offset":N}`
- `parsed` — Structured events (questions, rate limits, errors) from the output parser
- `watcher-lines` — A batch of assembled PTY lines for the plugin OutputWatchers registered through `POST /api/plugins/output-watchers`. Sent on the raw stream and on `?format=grid`; `?format=log|text` does not carry it. Each entry is `{text, matched_ids}`: `text` is the cleaned line Rust matched on, so the client can run its own `RegExp` on it and obtain the capture groups; `matched_ids` are the qualified ids (`<client_id>/<watcher_id>`) of the watchers Rust matched. A client ignores the ids of other clients. While every registered pattern compiles, only the matched lines are sent; while one does not, every line is sent
- `exit` — Session process exited
- `closed` — Session was closed

#### WebSocket format=log

```
WS /sessions/:id/stream?format=log
```

When `?format=log` is specified, the connection streams VT100-extracted log lines instead of raw PTY chunks:
- On connect: sends all accumulated lines as a single catch-up frame
- While running: polls every 200ms and sends new lines batched by offset
- PTY input passthrough is still available (write text/binary frames to send to PTY)

#### Grid cell text and search offsets

Packed styled rows and grid frames may end in a sparse
[`TCX1` cell-text trailer](../frontend/canvas-terminal-audit.md#cell-text-extension-tcx1).
It carries retained zero-width scalars without changing the 11-byte cell core.
Absent trailers remain valid. Binary consumers must retain these extensions to
reconstruct complete cell text, and clear previous extensions in replaced spans.
Text row and selection endpoints return the same stored codepoint sequence.
Buffer-search string ranges use UTF-16 offsets for JavaScript slicing; terminal
search highlight coordinates remain grid-cell based. Matching is exact, without
NFC normalization. Ranges cover whole
matching cells: a regex matching only a combining mark still reports its base
cell's full text span, since native search points have no subcell index.

#### WebSocket format=grid

```
WS /sessions/:id/stream?format=grid
```

The transport behind `CanvasTerminal` in browser/PWA mode. Binary frames carry the
serialised terminal grid; JSON text frames carry the side-channel events the canvas
needs, each one the desktop Tauri payload plus a `type` key:

```json
{"type": "osc133", "marker": "D", "line": 42, "exit_code": 0}
{"type": "cwd", "cwd": "/Users/me/project"}
{"type": "watcher-lines", "session_id": "abc", "lines": [{"text": "clean line", "matched_ids": ["<client_id>/w0"]}]}
```

- `osc133` — Shell-integration marker (`A` prompt, `B` command start, `C` output start,
  `D` command end). Drives command blocks, gutter marks and Cmd+Up/Down navigation.
  `exit_code` is `null` for every marker except `D`, and `null` on a `D` without one.
  The field is `exit_code`, not `exitCode` — it is serialised from the same Rust struct
  as the desktop `pty-osc133-{id}` event, and the two must not drift
- `cwd` — OSC 7 working-directory change. Same `{ cwd }` object as the desktop event
- `watcher-lines` — See the frame list above; carried here as well as on the raw stream

Frames the server has no grid consumer for are dropped rather than forwarded, so this
socket does not carry `output`, `parsed` or the activity pulse.

**Initial replay.** Attachment sends a full binary viewport immediately. When a live grid watch has no available frame, the server sends `{"type":"grid-replay-empty"}` instead. This transport control message confirms a healthy empty replay; it is not a PTY event and has no Tauri listener. It uses the same negotiated text framing as the side-channel events. Clients can finish their initial replay deadline on a binary viewport or this marker, while unrelated text events do not establish grid health.

**Dropped-frame recovery.** Binary frames are deltas, and the `watch` channel behind
this socket keeps only the newest value — a client that cannot keep up skips frames
and would apply a delta onto a row map missing rows. Each published frame therefore
carries a Rust-internal sequence number; when the reader sees a gap it re-serialises
the full grid and sends that instead. The sequence never reaches the wire, so the
binary frame format is unchanged.

#### WebSocket compress=deflate

```
WS /sessions/:id/stream?format=grid&compress=deflate
```

What is compressed on a remote connection, and by what:

| Traffic | Compressed by | Where |
|---|---|---|
| HTTP request and response bodies over 860 bytes | `CompressionLayer` (gzip/br/deflate, from `Accept-Encoding`) | `mcp_http::build_router` |
| Stream WebSocket frames, direct remote peer | raw deflate per frame, level 6, this option | `mcp_http::ws_compression` |
| Stream WebSocket frames, SSH-tunnelled peer | `ssh -C` on the tunnel channel | `tunnels::command`, `ProfileOptions::compression` |
| Stream WebSocket frames, local peer | nothing — there is no link to save | — |

The WebSocket half needs its own mechanism because `CompressionLayer` is an HTTP
layer and never sees a WebSocket frame, and tungstenite 0.30 — behind both
`axum::extract::ws` and the tunnel's client — has no permessage-deflate.

**Negotiation.** `?compress=deflate` on the upgrade, plus `tuic.deflate` in
`Sec-WebSocket-Protocol`. It is the only value offered; anything else, including
absent, leaves the socket in the framing described above, byte for byte, so a
client that does not know about this option is unaffected. The server refuses to
deflate for a **loopback peer** even when it asks — a browser on this machine has
no link, and an SSH-tunnelled client arrives through the local ssh process, whose
channel `Compression=yes` already deflated. A loopback peer is decided on the
**canonical** address, so the `::ffff:127.0.0.1` an IPv4 client wears on the
dual-stack `[::]` listener counts as local. Such a socket still gets the tagged
framing it asked for, with every tag saying identity.

**The acceptance is on the handshake, not inferred.** A server that is going to
tag its frames selects the `tuic.deflate` subprotocol, which RFC 6455 lets it do
only for a subprotocol the client offered. A client reads `ws.protocol` in
`onopen` — before any frame can arrive — and uses the tagged framing only when it
reads that value back. Against a server that predates the option, the parameter
and the offer are both ignored, no subprotocol comes back, and the client reads
the original framing. Inferring acceptance from the request alone is what this
replaces: an untagged grid frame beginning `0x01` fails to inflate, one beginning
`0x00` reaches the renderer a byte short, and a JSON frame arrives as a string
the tag decoder throws on.

**Framing.** On a negotiated socket every frame is a **binary** WebSocket message
whose first byte names the rest. Text travels as binary because deflate output is
not UTF-8, and a frame whose WebSocket type changed with its size would leave the
reader guessing:

| Tag | Payload |
|---|---|
| `0x00` | binary (a grid frame), as it is |
| `0x01` | binary, raw deflate |
| `0x02` | UTF-8 (one of the JSON frames above), as it is |
| `0x03` | UTF-8, raw deflate |

Raw deflate, no zlib wrapper — `DecompressionStream("deflate-raw")` in a browser.
A frame under 860 bytes, or one deflate does not shrink, is sent as it is with an
identity tag, so compression can never make a frame larger than the one byte that
describes it. An unrecognised tag must close the socket rather than be rendered.

**Measured**, on 957 grid frames replayed from a committed capture of a real Codex
session (`mcp_http::ws_compression::measurement`): 2,199,978 raw bytes become
127,243 — 5.8%. One full-screen repaint is 111,158 bytes raw and 3,004 compressed,
for 0.234 ms of added latency, against a 16 ms grid tick. Permessage-deflate with
context takeover (what adopting `yawc` would buy) reaches 53,540 bytes, a further
3.3% of the raw stream; that was judged not to pay for replacing the WebSocket
implementation on both the server and the tunnel client, nor for a stream in which
one dropped frame corrupts every frame after it.

### Server-Sent Events (SSE)

```
GET /events?types=repo-changed,pty-parsed&stream_id=<uuid>
```

Broadcasts server-side events to all browser/mobile clients. Supports optional `?types=` query parameter for comma-separated event name filtering. Omitting `types` asks for every event; sending it empty is an empty allowlist and delivers none. Uses monotonic event IDs and 15-second keep-alive pings.

`stream_id` is a client-chosen id for the connection. With one, `types` is only the
*initial* filter and the client can widen it later on the same connection:

```
POST /events/types
Content-Type: application/json

{ "stream_id": "<uuid>", "types": ["repo-changed", "dir-changed"] }
```

`types` is the full set the client wants from now on, not a delta. `204` applies it to
the live stream; `404` means the server is not tracking that stream (it ended, or more
than 64 streams are already tracked), and the client must fall back to reconnecting with
a wider `?types=`.

A panel that mounts late needs an event type the stream was not opened with, and
reconnecting to get it loses every event published between the close and the new
subscription — the server subscribes to the event bus at connect time and replays
nothing. That is what this route exists to avoid. A client that lets `EventSource`
auto-reconnect must re-post its set on every `onopen`: the reconnect replays the URL, so
the server is back to the filter the connection was opened with.

| Event | Payload | Description |
|-------|---------|-------------|
| `session-created` | `{session_id, cwd, agent_type, display_name, parent_session}` | New session started; `display_name` is the optional stable assigned name; `parent_session` is the `$TUIC_SESSION` of the agent that spawned it (null otherwise) |
| `pty-description-changed` | `{session_id, description}` | Orchestrator updates the short task description shown above a PTY |
| `chat-view-changed` | `{session_id, seq}` | The chat view of a Claude terminal has entries past `seq`; read them with `GET /sessions/:id/chat-view` |
| `session-renamed` | `{session_id, name, is_custom}` | An MCP `session action=rename` changed a tab's display name |
| `session-suspend-requested` | `{session_id, request_id}` | An MCP `session action=suspend` asked the UI to suspend that tab |
| `term-alias-assigned` | `{session_id, alias}` | A session received its terminal alias (e.g. `tu-3`); published once, after `session-created` |
| `session-closed` | `{session_id}` | Session ended |
| `design-mode-changed` | `{repo_path, session_id, status}` | Design Mode for the repository changed to `armed` or `stopped`; the bound agent tab follows this status |
| `repo-changed` | `{repo_path, kind}` | Repository changed. `kind` is `"git-state"` (`.git/` was written — a commit, ref or index change) or `"working-tree"` (files changed and `.git` did not). A git-state emit cancels the pending working-tree one, so `"git-state"` does **not** mean "only `.git` changed" — a client that needs working-tree news must react to both kinds. |
| `head-changed` | `{repo_path, branch}` | Git HEAD changed (branch switch) |
| `pty-parsed` | `{session_id, parsed}` | Structured output event from PTY parser |
| `pty-exit` | `{session_id}` | PTY process exited |
| `pty-activity` | `{session_id}` | Bytes are flowing from the PTY. Payload-free pulse, at most one per second — a dropped pulse loses nothing |
| `pty-osc133` | `{session_id, marker, line, exit_code}` | Shell-integration marker (OSC 133). `exit_code` is `null` except on a `D` marker that reported one |
| `pty-cwd` | `{session_id, cwd}` | Working directory changed (OSC 7) |
| `plugin-watcher-lines` | `{session_id, lines}` | A batch of assembled PTY lines for the plugin OutputWatchers; each line is `{text, matched_ids}`, where `text` is the cleaned text the match was made on |
| `plugin-changed` | `{plugin_ids}` | Plugin(s) installed/removed/updated |
| `upstream-status-changed` | `{name, status}` | MCP upstream server status change |
| `mcp-toast` | `{title, message, level, sound, origin_repo_path?, origin_session_id?}` | Toast notification from MCP layer, including the caller repository/cwd and the caller's TUIC session when known. Clients use the session id to focus the terminal that raised the toast |
| `session-state-changed` | `{session_id, state}` — `state` is the same object `GET /sessions` returns per session (`shell_state`, `agent_state`, `awaiting_input`, `question_confident`, `background_work`, `queued_commands`, …), snake_case, with the fields serde skips at their zero value omitted | A session's derived lifecycle state moved. Published by the session-state accumulator (`state.rs publish_session_state_change`) once per real transition, deduped by `SessionState`'s `PartialEq` — a repaint that changes only `last_activity_ms` publishes nothing. Dual-emitted on the Tauri window under the same name and with the same payload, so `useAgentPolling.ts` consumes both transports with one handler instead of polling `list_active_sessions`. Absence of a field means its zero value, not "unknown" |
| `dictation-download-progress` | `{downloaded, total, percent}` | A Whisper model is downloading. Dual-emitted on the Tauri window with the identical body |
| `speech-download-progress` | `{asset, downloaded, total, percent}`, then `{asset, done: true}` | A speech asset is downloading; the `done` event ends it, success or failure. `asset` is the only difference from the line above, and it is what a client joins the bar against |
| `speech-utterance` | `{utteranceId, state, error?, turn}` — the `SpokenReply` shape `POST /dictation/speech/speak` returns | A reply moved between `queued`, `rendering`, `speaking` and one of `finished` / `interrupted` / `failed`. Pushed by the render thread that performed the transition, so a client no longer polls `speech/status` |
| *(any of the above, mirrored)* | the daemon's own payload plus `__tuic_origin: {connection}` | An event this machine repeated from a connected remote daemon (`remote_mirror.rs`). It arrives under the daemon's own event name, so a client needs no new subscription; `__tuic_origin` says which connection it came from. A frame that already carries the key is dropped rather than repeated, so a mirrored event never crosses a second hop |
| `lagged` | `{missed}` | Client fell behind; N events were dropped. Dropped events are never resent, so a client that derives state from the stream must re-read it — `subscribeEvents`' `onResync("lagged")` callback exists for that. It also fires with `"reconnect"` on any EventSource re-open after the first, because a drop loses the same way silently. Both are SSE-only: Tauri `listen()` is in-process and cannot drop |

### MCP Streamable HTTP

```
POST /mcp
Content-Type: application/json

{ JSON-RPC message }
```

Single endpoint for all MCP JSON-RPC requests (legacy `initialize`, current
`server/discover`, `tools/list`, `tools/call`). Returns JSON-RPC responses
directly in the HTTP response body. Session ID returned via `Mcp-Session-Id`
header on legacy initialize. For 2026-07-28 requests, `server/discover` and
`tools/list` return `resultType: "complete"`, `ttlMs: 0`, and
`cacheScope: "private"`; completed `tools/call` results, including tool-level
errors, return `resultType: "complete"` without cache fields. Legacy result
shapes are unchanged.

The native `session` tool includes `action=submit` for a managed-agent command
and bounded terminal-movement receipt in that same JSON-RPC response. It is
loopback-only, never queues, and rejects a busy/dialog/partial composer before
writing. `action=input` and `POST /sessions/:id/write` remain raw write-only
surfaces; neither returns submission acknowledgement. See
[MCP & HTTP Server](../backend/mcp-http.md#mcp-tool-session-atomic-submission).

```
GET /mcp          → 405 Method Not Allowed
DELETE /mcp       → Ends MCP session (pass Mcp-Session-Id header)
```

## Git Endpoints

### Repository Info

```
GET /repo/info?path=/path/to/repo
```

Returns `RepoInfo` (name, branch, status, initials).

### Git Diff

```
GET /repo/diff?path=/path/to/repo
```

Returns unified diff string.

### Diff Stats

```
GET /repo/diff-stats?path=/path/to/repo
```

Returns `{ "additions": N, "deletions": N }`.

### Changed Files

```
GET /repo/files?path=/path/to/repo
```

Returns array of `ChangedFile` (path, status, additions, deletions).

### Single File Diff

```
GET /repo/file-diff?path=/path/to/repo&file=src/main.rs
```

Returns diff for a single file.

### Read File

```
GET /repo/file?path=/path/to/repo&file=src/main.rs
```

Returns file contents as text.

### Branches

```
GET /repo/branches?path=/path/to/repo
```

Returns sorted branch list.

### Repo Summary

```
GET /repo/summary?path=/path/to/repo
```

Aggregate snapshot: worktree paths, merged branches, and per-path diff stats in one round-trip. Replaces 3+ separate IPC calls.

### Repo Structure (Progressive Phase 1)

```
GET /repo/structure?path=/path/to/repo
```

Returns `{ "worktree_paths": { "branch": "/path", ... }, "merged_branches": ["branch", ...] }`. Fast path — no diff stats computation.

### Repo Diff Stats (Progressive Phase 2)

```
GET /repo/diff-stats/batch?path=/path/to/repo
```

Returns `{ "diff_stats": { "/path": { "additions": N, "deletions": N }, ... }, "last_commit_ts": { "branch": N, ... }, "workspace_statuses": { "workspace-id": { "dirty_files": 24, "commit_status": "unmerged", "removal_safety": "requires_force" } } }`. Slow path — computes per-worktree diff stats, timestamps, and lifecycle verdicts. Lifecycle entries are keyed by workspace id, never branch name.

### Background Git Actions

`POST /repo/run-git` accepts `{path, args}` for GitPanel and sidebar operations. Unsupported subcommands or options return HTTP 400 before execution. The flag policy is shared with IPC; see [Git backend](../backend/git.md).

### Local Branches

```
GET /repo/local-branches?path=/path/to/repo
```

Returns local branch list.

### Checkout Remote Branch

```
POST /repo/checkout-remote
Content-Type: application/json

{ "repoPath": "/path/to/repo", "branchName": "feat-remote" }
```

Creates a local tracking branch from `origin/<branchName>`.

### Rename Branch

```
POST /repo/branch/rename
Content-Type: application/json

{ "path": "/path/to/repo", "old_name": "old", "new_name": "new" }
```

### Check Main Branch

```
GET /repo/is-main-branch?branch=main
```

Returns `true` if the branch is main/master/develop.

### Initials

```
GET /repo/initials?name=my-repo
```

Returns 2-char repo initials.

### Markdown Files

```
GET /repo/markdown-files?path=/path/to/repo
```

Returns list of `.md` files in a directory.

### Recent Commits

```
GET /repo/recent-commits?path=/path/to/repo
```

Returns recent git commits.

### GitHub Status

```
GET /repo/github?path=/path/to/repo
```

Returns PR status, CI status, ahead/behind for current branch.

### PR Statuses (Batch)

```
GET /repo/prs?path=/path/to/repo
```

Returns `BranchPrStatus[]` for all branches with open PRs.

### PR Statuses (Multi-Repo Batch)

```
POST /repo/prs/batch
Content-Type: application/json

{ "paths": ["/repo1", "/repo2"], "include_merged": false }
```

Returns aggregated PR statuses across multiple repositories.

### Issues

```
GET /repo/issues?path=/path/to/repo
```

Returns `GitHubIssue[]` for the repo, filtered by the user's configured issue filter.

### Close Issue

```
POST /repo/issues/close
Content-Type: application/json

{ "repo_path": "/path/to/repo", "issue_number": 42 }
```

Closes the specified issue via GitHub GraphQL API.

### Reopen Issue

```
POST /repo/issues/reopen
Content-Type: application/json

{ "repo_path": "/path/to/repo", "issue_number": 42 }
```

Reopens a closed issue via GitHub GraphQL API.

### GitHub Auth & Diagnostics

Browser/PWA parity for the GitHub settings panel. Registered on the loopback
router only (the headless `tuic-remote` daemon does not expose GitHub).

```
GET  /github/viewer-login                       -> string (login)
GET  /repo/ci-failure-logs?repoPath=&branch=&checkUrl=&headSha= -> string (logs; checkUrl and headSha optional)
GET  /circleci/token                          -> CircleCiTokenStatus (no token value)
POST /circleci/token       { token }           -> null (store read-only token)
DELETE /circleci/token                         -> null
POST /github/pr-hide-drafts   { hide }          -> null
POST /github/auth/start                         -> DeviceCodeResponse
POST /github/auth/poll        { deviceCode }    -> PollResult (default login)
POST /github/accounts/poll    { deviceCode }    -> PollResult (named account; preserves default login)
POST /github/auth/logout                        -> null
POST /github/auth/disconnect                    -> null
GET  /github/auth/status                        -> AuthStatus
GET  /github/diagnostics                        -> GitHubDiagnostics
```

Auth commands share the desktop `*_impl` (device-code flow + OS-keyring token via
`crate::credentials`). `get_all_issues` is intentionally unmapped — it has no frontend
`invoke()` caller (the `/repo/issues` route already serves browser issue lists).

### Merged Branches

```
GET /repo/branches/merged?path=/path/to/repo
```

Returns list of branch names merged into the default branch.

### Orphan Worktrees

```
GET /repo/orphan-worktrees?repoPath=/path/to/repo
```

Returns list of worktree directory paths that are in detached HEAD state (their branch was deleted).

`GET /repo/orphan-cleanup-assessment?repoPath=/path/to/repo` returns each orphan's
`{ path, safe, reason?, live_sessions? }`. A checkout is safe for automatic removal only when it
has no tracked or untracked changes, its HEAD is reachable from a local or
remote branch, and no live session has its cwd inside it. `live_sessions`
(`[{ session_id, name }]`, omitted when empty) names those sessions and `reason`
repeats them. Ignored files do not make it dirty. Assessment failures are
unsafe.

While the Ask dialog is open, `POST /repo/orphan-cleanup/begin` with
`{ "repoPath": "...", "paths": ["..."] }` registers the pending request.
`GET /repo/orphan-cleanup/pending?repoPath=...` returns its answer (`true` for
remove, `false` for keep, or `null` while unanswered).
`POST /repo/orphan-cleanup/answer` with `{ "repoPath": "...", "decision": "remove" }`
answers it; remove rechecks every path server-side and refuses unsafe (including live-session) or stale
worktrees. `POST /repo/orphan-cleanup/clear` with `{ "repoPath": "..." }` clears
the request when the dialog closes. MCP `repo action=orphan_cleanup_answer`
uses the same answer operation with `path` and `decision=remove|keep`.

### Remove Orphan Worktree

```
POST /repo/remove-orphan
Content-Type: application/json

{ "repoPath": "/path/to/repo", "worktreePath": "/path/to/worktree", "safeOnly": true }
```

Removes an orphan worktree by filesystem path. The worktree path is validated against the repo's actual worktree list. `safeOnly` is optional; when true, the server rechecks clean status, branch reachability and live sessions immediately before removal and refuses a checkout a session still works in. With `safeOnly` false the caller has confirmed the removal from the assessment: `confirmedSessions` (default `[]`) lists the `live_sessions` ids it showed, and the server refuses when a live session outside that list works in the checkout.

### Merge PR via GitHub

```
POST /repo/merge-pr
Content-Type: application/json

{ "repoPath": "/path/to/repo", "prNumber": 42, "mergeMethod": "squash", "expectedHeadSha": "<head sha the caller reviewed>" }
```

Merges a PR via the GitHub API. `mergeMethod` must be `"merge"`, `"squash"`, or `"rebase"`. `expectedHeadSha` is required and is sent to GitHub as `sha`: if the PR head moved since the caller saw it, GitHub answers 409 and the route returns an error starting with `PR head changed`; the caller must refresh and review, never retry with the new head. Returns `{"sha": "..."}` on success.

### Approve PR

```
POST /repo/approve-pr
Content-Type: application/json

{ "repoPath": "/path/to/repo", "prNumber": 42 }
```

Submits an approving review on a PR via the GitHub API.

### Update PR Branch

```
POST /repo/update-pr-branch
Content-Type: application/json

{ "repoPath": "/path/to/repo", "prNumber": 42, "expectedHeadSha": "<head sha the caller saw>" }
```

Merges the base branch into the PR branch (GitHub update-branch, 202 accepted; the merge commit lands asynchronously). `expectedHeadSha` is required: if the PR head moved, the route returns an error starting with `PR head changed`. Returns `{"ok": true}`.

### Close PR

```
POST /repo/close-pr
Content-Type: application/json

{ "repoPath": "/path/to/repo", "prNumber": 42 }
```

Closes the PR without merging. Returns `{"ok": true}`.

### CI Checks

```
GET /repo/ci?path=/path/to/repo
```

Returns detailed CI check list.

### Unresolved Review Threads

```
GET /repo/pr-review-threads?path=/path/to/repo&pr_number=42
```

Returns `{"bot": N, "human": M}`: unresolved review threads of one PR (first 50), split by the author of the first comment. One GraphQL point per call.

### PR Diff

```
GET /repo/pr-diff?path=/path/to/repo
```

Returns diff for the current branch's open PR.

### Conflict Assist

```
GET  /repo/merged-prs?path=&sinceTag=                      -> MergedPr[]
POST /repo/conflict-assist   { repoPath, prNumber }        -> ConflictAssistResult
```

`/repo/conflict-assist` creates a worktree on the PR head and rebases it onto
the base. `status` is `clean` only when the base was refreshed from origin,
`clean_unverified` when a conflict-free result used an existing tracking ref or
local fallback, and `conflicts` when manual resolution is needed. The response
includes `base_source`, an optional `base_warning`, the conflicted-file list,
and an agent prompt; it never pushes or merges. None of it calls a model.

### Review, changelog and improvement scan (ego)

```
POST /repo/pr-review                 { repoPath, prNumber }   -> PrReviewResult
GET  /repo/changelog?path=&sinceTag=                          -> ChangelogResult
POST /repo/improvement-scan          { repoPath, focus }      -> ImprovementScanResult
POST /repo/create-issue-from-proposal { repoPath, proposal }  -> CreatedIssue
```

Each of the first three is **one unattended ego turn** (#795-320b) over the
`acp::oneshot` seam: the whole input goes inline, ego is offered no host tools,
and every permission request is refused. TUICommander stores no API key and makes
no provider HTTP call for any of them; which model runs is ego's configuration.

They are **not** desktop-gated — ego is reached over ACP, so `tuic-remote` serves
them too. `/repo/merged-prs` is unchanged and still a plain GraphQL query.

`PrReviewResult` carries `{ repo_path, pr_number, head_sha, summary, files[] }`,
each file `{ path, summary, findings[] }` and each finding
`{ path, line, hunk, severity, confidence, message }`. The confidence gate runs in
Rust before the response is built (default `0.7`, overridable with
`TUIC_REVIEW_CONFIDENCE_THRESHOLD`), so a finding that arrives has already passed
it. `head_sha` hashes the reviewed diff — it is not a git sha.

`ChangelogResult` is `{ markdown, json }`; `json` is `null` when ego answered in
prose only, which is a valid answer rather than an error.

An ego that cannot be reached is a `4xx`/`5xx` with ego's own sentence in the
error body — never a `200` with an empty result. `POST /repo/pr-review`,
`POST /repo/improvement-scan` and `POST /repo/conflict-assist` also emit
`review-progress`, `proposals-ready` and `conflict-assist-status` on `/events`
while they run — on **every** build. The routes are mounted in `build_router`,
which the desktop app serves, and a browser reading
`/events` is the only client a non-desktop build has: the desktop window emit is
gated on `feature = "desktop"`, the `event_bus` send never is (#808-84e1). They
are **not** in `shared_routes`, so the `tuic-remote` daemon does not serve them
at all (#810-4986).

`POST /ai/review/pr` and `POST /ai/improvements/scan` are gone with the engine
#784-0aec deleted; the `/ai/` prefix belonged to it, so the replacements sit
under `/repo/`.

`install_agent_mcp`/`remove_agent_mcp` (config-file writes, also no caller).

`GET`/`PUT /config/agents` read and write `agents.json`, including each agent's optional `prevent_alt_screen` override (`false` disables TUIC's screen control; absent means enabled). The route is in `shared_routes()`, so the `tuic-remote` daemon serves it too: a remote repository's agents run with that machine's `agents.json`, and the frontend loads it per connection. The `/config/agents/{agent}/…` sub-routes below stay desktop-only.

`GET /config/agents/{agent}/native-status-signals` returns `{ "enabled": boolean }`. `PUT` accepts the same boolean field for Claude or Codex and changes launch behavior for new sessions only. The existing `/hook-instrumentation` route remains the explicit global installer.

### No provider keyring routes

`/config/provider-key*`, `/config/slot-test` and `/config/ollama-models` were
removed with the provider registry (#784-0aec). TUICommander stores no model API
key and makes no provider HTTP call, so there is nothing for a browser or remote
client to proxy. Story 786-4a6d brings a provider list back (now on the Settings → AI Chat
page), configured against ego rather than against a registry of TUICommander's own.

The OAuth upstream flow (`start_mcp_upstream_oauth` / `cancel_mcp_upstream_oauth`) is
**not** mapped: `start` binds a loopback callback server and opens the OS browser, so
the redirect can't return to a remote/PWA client. Desktop drives it over IPC; browser
clients get a clean host-only error until the redirect UX is redesigned.

### Hash Password

```
POST /config/hash-password
Content-Type: application/json

{ "password": "..." }
```

Returns bcrypt hash string.

### Notification Config

Interactive config `PUT` routes below (except `/config/repositories`, which has
its own keyed mutation protocol) accept `{ "base": <last GET response>,
"config": <edited document> }`. The server computes the changes from `base` to
`config` and applies them to the latest file under its lock. Objects merge by
key, arrays replace whole, and JSON null deletes a key. A field unchanged by
this caller is never written. Clients should serialize their own overlapping
saves so each later request has the preceding desired document as its base.
`PUT /config` uses this same envelope. Its `GET` response omits secret values;
unchanged omitted secrets remain on disk.

```
GET /config/notifications
PUT /config/notifications
```

Load/save `NotificationConfig`.

### UI Preferences

```
GET /config/ui-prefs
PUT /config/ui-prefs
```

Load/save `UIPrefsConfig`.

### Config Defaults

```
GET /config/defaults
```

Read-only. Returns `ConfigDefaults` — see `docs/backend/config.md` → "Config
Defaults" — for Settings "expert mode": `{ app, notifications, agent_settings,
repo_defaults, agents, github_accounts, dictation? }`. `dictation` is present only on desktop
builds. `None` fields are omitted (`skip_serializing_if`); a missing key inside
a present object means `null`.

### Repository Settings

```
GET /config/repo-settings
PUT /config/repo-settings
```

Load/save per-repository settings.

### Repository Defaults

```
GET /config/repo-defaults
PUT /config/repo-defaults
```

Load/save default settings applied to new repositories.

### Check Custom Settings

```
GET /config/repo-settings/has-custom?path=/path/to/repo
```

Returns `true` if the repo has non-default settings.

### Repositories

```
GET /config/repositories
PUT /config/repositories
```

`GET` loads the repositories document. Every `PUT` must send the same versioned
`mutationVersion: 1` delta as desktop `save_repositories`: keyed
`repos`/`groups` entries carry `{id,before,after}`, while `repoOrder`,
`activeRepoPath`, and `groupOrder` optionally carry `{before,after}`. The
backend applies the delta to the latest document under the cross-process lock.
Different repository/group IDs and independent order membership changes
compose; incompatible changes to the same record return `409 Conflict` and a
malformed or unversioned delta returns `400 Bad Request`.

A `PUT` that actually moves the document broadcasts a payload-free
`repositories-changed` SSE event (subscribe with `GET /events?types=repositories-changed`)
so the other clients re-read the document instead of saving over it from a stale
baseline. A delta that was already applied changes nothing on disk and is not
announced. See `docs/backend/config.md` for what a receiving client is allowed to
adopt.

### Stale-temp Repository Repair (#763-d219)

```
GET  /config/repositories/stale-temp
POST /config/repositories/stale-temp
```

`GET` returns `StaleTempCandidate[]` (`{path, displayName}`) — a read-only
preview, re-classified against the live document on every call, of rows that
match ALL of: local path absent, path under a recognized OS/project temp root,
`isGitRepo: false`, exactly one shell-only workspace with no terminals/saved
terminals/parent/commit state, and no user metadata (see `docs/backend/config.md`
for the exact classifier). Never mutates.

`POST` body `{"paths": ["/tmp/…", …]}` — the user-explicit repair. Re-validates
every named path against the classifier on the document as it stands on disk
*right now*; if even one no longer matches, the whole request is rejected with
`400 Bad Request` and nothing is written (no implicit or partial deletion). On
success it writes an exact pre-repair backup (`repositories.repair-backup-<UTC
timestamp>.json`) in the config directory, removes the validated rows from
`repos`/`repoOrder`/every group's `repoOrder` (and clears `activeRepoPath` if it
pointed at a removed row) in one write, returns `StaleTempRepairSummary`
(`{removed, backupPath}`), and broadcasts `repositories-changed` like `PUT
/config/repositories`. Requires local-or-authenticated like every other
mutating config route.

### Prompt Library

```
GET /config/prompt-library
PUT /config/prompt-library
```

Load/save prompt entries.

### Notes

```
GET /config/notes
PUT /config/notes
```

Load/save notes (opaque JSON, shape defined by frontend).

### Remote Connections

```
GET    /config/remote-connections
PUT    /config/remote-connections
DELETE /config/remote-connections/{id}
PUT    /config/remote-connections/{id}/password
GET    /config/remote-connections/{id}/password
POST   /config/remote-connections/{id}/token
POST   /config/remote-connections/{id}/install
DELETE /config/remote-connections/{id}/install
GET    /config/remote-connections/{id}/update
POST   /config/remote-connections/{id}/update
```

The configured remote machines and their vault password. The password is write
only: it goes to the OS credential vault keyed by the connection's UUID and is
never returned, never written to `connections.json` — `GET .../password` answers
whether one is stored, not what it is. `POST .../token` trades it for the remote
daemon's in-memory session token.
`PUT /config/remote-connections` takes `{ "base": null, "connection": {...} }`
for a new machine, or the loaded connection as `base` when editing. The server
merges only changed fields into that connection's latest locked record.

The install pair is SSH-only. `POST .../install` stages the matching release
binary, installs and starts a systemd user unit or launchd agent, and persists
`deploy = "installed"`. `DELETE .../install` stops and removes the service
files and persists `deploy = "on_connect"`. Pairing tokens stay in the credential
vault; neither response nor the connection document contains them.

`GET .../update` previews the selected daemon binary: `remote_build`,
`desktop_build` (version, target triple, SHA-256), `source` (`release` or
`local`), `session_count`, and `out_of_date`. The release asset is preferred;
when it is absent, a locally built sibling `tuic-remote` is accepted only for
the same target triple. `POST .../update` takes
`{ "confirmedSessions": N, "expectedSha256": "..." }`. A changed count or
binary after confirmation is rejected. Direct connections upload over the
authenticated daemon route; SSH connections use the existing SCP deployment.
The call returns after `/health` reports the new SHA-256, or an error if the
restart cannot be verified. Live remote PTY sessions are lost.

`DELETE /config/remote-connections/{id}` tears the live connection down before it
rewrites the store: status poll, mirror task, mirrored session rows, SSH tunnel
and session token all go, through the same `remote_runtime::teardown` the
disconnect route uses. Deleting a connected machine therefore needs no
disconnect first — and must not be given one, because the tunnel is keyed by the
profile the delete is about to remove.

### Remote Connection Runtime

```
GET    /config/remote-connections/status
POST   /config/remote-connections/{id}/connect
DELETE /config/remote-connections/{id}/connect
```

Live state, not configuration: `GET .../status` answers with one object per
connection — `{ id, status, base_url?, token?, protocol_version?, build?, out_of_date?, live_sessions?, update_notice?, update_in_progress?, error?, step? }`,
where `status` is `disconnected | connecting | deploying | connected |
unauthenticated | error`. `step` is present while deploying. `base_url`, `token`
and `protocol_version` are present **only** while
connected; `update_in_progress` is present as `true` while a manual or unattended update
owns the connection. The route and token fields answer "where do I send a call", and a
connection that is not connected has no such answer.

`POST .../connect` brings a connection up and `DELETE .../connect` takes it
down. Neither returns the new status: every transition is pushed as a
`remote-connection-status` event on `/events` SSE (and to the desktop window),
so one client connecting is visible to all of them.

Both directions are idempotent. A second `POST .../connect` while one is already
in flight is a no-op rather than a second handshake, and `DELETE .../connect` on
an unknown id creates nothing. A connect that fails stops the SSH tunnel it
started, so a bad password leaves no `ssh` process behind. A connection in
`error` keeps its poll: the daemon coming back is a `connected` push, not a
reconnect the user has to ask for.

These three routes are desktop-only — they are registered on `build_router`, not
in `shared_routes()`. A `tuic-remote` daemon is the far end of a remote
connection; it does not hold connections of its own.

### SSH Discovered Hosts

```
GET /tunnels/ssh-hosts/discovered
```

Returns `{ "hosts": [{ "host", "target", "user", "port", "source": "config | known_hosts" }], "hashed_count": n }`:
non-wildcard `~/.ssh/config` aliases plus plain `~/.ssh/known_hosts` names,
deduplicated by resolved `target` and port (`target` is the alias's `HostName`,
else the name itself). Names that start with `-`, contain control characters or
have a port outside 1-65535 are dropped; known_hosts is read up to 4 MiB and at
most 2000 of its names are listed. Hashed known_hosts entries cannot be
listed and are only counted. No host is contacted.

### SSH Host Status

```
GET /tunnels/ssh-hosts/status
```

Probes the `~/.ssh/config` aliases (at most 64) and returns
`[{ "host": "name", "target": "resolved", "port": 22, "auth": "shell | no_shell | auth_failed | unreachable" }]`.
known_hosts names are never probed in bulk.

```
POST /tunnels/ssh-hosts/probe   {"target": "10.0.0.5", "port": 2222}
```

Probes one entry of the discovered list, identified by `target` and `port`
(404 when it is not in the list). A known_hosts entry is probed with
`StrictHostKeyChecking=yes`.
The probe runs only on request, checks at most four hosts concurrently, uses
batch authentication with a five-second connect timeout, and caches results for
60 seconds.

### MCP Status

```
GET /mcp/status
```

Returns MCP server status (enabled, running, active sessions, connected MCP clients, maximum sessions). `native_tools` contains the unfiltered native MCP registry as `{name, summary, description}` entries, including disabled tools. `summary` is the first line of the full registry description. This Settings inventory is independent of upstream tools, collapse mode and progress tracking; MCP client discovery still applies all configured filters.

### MCP Suspend Response

```
POST /mcp/suspend-response
Content-Type: application/json

{"request_id": "...", "ok": false, "reason": "agent working"}
```

The tab's verdict on a `session action=suspend` request announced by the `session-suspend-requested` event. The waiting MCP call returns it. Unknown or already-answered ids are ignored.

### MCP Upstream Credentials

`POST /mcp/upstreams/credential` accepts `{name, token, header?}`. Without `header`, it saves the existing Bearer credential. With `header: {name, credential_ref}`, it saves a secret header value in the upstream-scoped OS credential vault. `credential_ref` must be a UUID. Names and values are validated without echoing values in errors.

`DELETE /mcp/upstreams/credential` accepts `{name, header?}` and removes that credential. Both return JSON `null` on success, matching IPC unit results. `POST /mcp/upstreams/reconnect` also returns JSON `null` on success. Config entries carry only `headers: [{name, credential_ref}]`; the API never returns header values.

### MCP Upstream Status

```
PUT /mcp/upstreams
Content-Type: application/json

{
  "base": { "servers": [...] },
  "config": { "servers": [...] }
}
```

`base` is the configuration previously loaded by the caller and `config` is its
desired result. The backend derives an ID-keyed three-way delta, then applies it
to the latest `mcp-upstreams.json` under the cross-process file lock. Removing a
server from `config` explicitly deletes that ID; removing an optional `auth`
field explicitly clears it. Fields and servers unchanged from `base` preserve
concurrent updates, including OAuth/DCR auth written by another process. After
the atomic write, the live registry hot-reloads the exact locked pre/post
configurations. Returns `200` with JSON `null`, `400` for invalid config or
duplicate IDs, and `500` for persistence or conflicting-add failures.

```
GET /mcp/upstream-status
```

Returns status and metrics for all upstream MCP servers (connecting, ready, circuit_open, disabled, failed).

### MCP Instructions

```
GET /mcp/instructions
```

Returns dynamic server instructions for the MCP bridge binary as `{"instructions": "..."}`.

## Filesystem Endpoints

The sender-side `fs_transfer_remote_paths` coordinator is desktop IPC only (`INTENTIONALLY_UNMAPPED`), with no `/fs/transfer-remote` HTTP route. Data leaves the machine; Finder source paths cannot be gated to registered repository roots, so HTTP token holders must not trigger exfiltration. The desktop coordinator uses the existing authenticated daemon connection; only the receiving upload endpoint is exposed over HTTP.

`POST /fs/upload-copy?destDir=<absolute-directory>&name=<leaf-name>&directory=true|false` accepts a streamed, uncompressed tar body under the existing daemon authentication. All archive paths must start with `name`; only files and directories are accepted. Limits: 256 MiB of archive and extracted file data, 10,000 entries, two concurrent uploads. Destination resolution uses registered repository directory capabilities, rejects traversal and escapes through symlinks, and never follows a target symlink. Staging is removed on failure/disconnect; the completed top-level file or directory is published with a no-replace atomic rename. A concurrently created target is skipped; existing targets drain the bounded body before returning `skipped`. The upload is exempt from the global response timeout and aborts after 30 s without a body chunk. Its total receive budget is 17 minutes. A connection below roughly 2 Mbit/s cannot finish a 256 MiB drop within that budget. The concurrency permit lasts through extraction even if the handler is cancelled. Filenames use the receiver platform rules (POSIX permits colon and backslash); Windows rejects colon and backslash. Unix tar permissions are masked to `0755` for executable files/directories and `0644` for ordinary files, with the receiving umask applied and no privilege bits. Normal request-drop cleanup consumes the staging directory handle before removing its tree, including on Windows. Abrupt process termination (for example, kill -9) can leave a `.tuic-upload-<uuid>` directory in the destination; there is no startup sweep, and the leftover can be removed manually once no upload is running. Final directory permissions are applied after publication; a permission failure is logged and the already published copy remains successful. The sender uses the existing `tui-session` cookie header; tokens are absent from upload URLs and reqwest error text. No new listener or credential is created.

`POST /attachments/upload?kind=pty|acp&id=<session-or-connection-id>&name=<filename>`
streams a binary request body into the target's working directory at
`.tuic/attachments/<timestamp>-<safe-name>`. It returns `{ "path": "/absolute/path", "size": N }`.
The default per-file cap is 25 MiB (`attachment_max_bytes`), including uploads
without `Content-Length`; partial and empty files are removed on failure. Git
repositories receive a local `.git/info/exclude` entry for that directory.
Authentication matches the other shared HTTP routes.

```
GET  /fs/list?repoPath=/path/to/repo&subdir=src
GET  /fs/search?repoPath=/path/to/repo&query=main&limit=50
GET  /fs/search-content?repoPath=/path/to/repo&query=foo&caseSensitive=false&useRegex=false&wholeWord=false&limit=200
GET  /fs/search-content-all?query=foo&caseSensitive=false&limit=200   -> cross-repo BM25 over every ready index
GET  /fs/read?repoPath=/path/to/repo&file=src/main.rs
GET  /fs/markdown-image?repoPath=/path/to/repo&file=docs/images/chart.png -> image bytes
GET  /fs/read-external?path=/absolute/path/to/file
POST /fs/write         { "repoPath": "...", "file": "...", "content": "..." }
POST /fs/write-if-unchanged { "repoPath": "...", "file": "...", "expected": "...", "content": "..." } -> true | false (false: disk text differs from `expected`, nothing written)
POST /fs/mkdir         { "repoPath": "...", "dir": "..." }
POST /fs/delete        { "repoPath": "...", "path": "..." }
POST /fs/rename        { "repoPath": "...", "from": "...", "to": "..." }
POST /fs/copy          { "repoPath": "...", "from": "...", "to": "..." }
POST /fs/gitignore     { "repoPath": "...", "pattern": "..." }
GET  /fs/resolve-terminal-path?cwd=/repo&candidate=src/x.ts   -> ResolvedFilePath | null
POST /fs/resolve-terminal-paths { "cwd": "/repo", "candidates": [...] } -> (ResolvedFilePath | null)[]
POST /fs/resolve-markdown-link { "root": "/repo", "currentFile": "docs/readme.md", "href": "../guide.md#intro" } -> MarkdownLinkTarget
GET  /fs/stat?path=/absolute/path                              -> PathStat (exists/is_dir/size/modified_at)
POST /fs/warm-index    { "repoPath": "..." }                   -> { "ok": true } (fire-and-forget BM25 build; strategy-gated, see below)
POST /fs/write-external { "path": "/abs", "content": "..." }   -> { "ok": true }
POST /fs/copy-abs      { "from": "/abs", "to": "/abs" }        -> { "ok": true }
POST /fs/move-abs      { "from": "/abs", "to": "/abs" }        -> { "ok": true }
POST /fs/transfer      { "destDir": "/abs", "paths": [...], "mode": "move"|"copy", "allowRecursive": bool } -> TransferResult
```

`/fs/warm-index` returns `{ "ok": true }` whether or not it scheduled anything.

`/fs/markdown-image` serves image MIME types up to 10 MiB for mobile Markdown previews. The requested file and repository root are canonicalized before containment is checked, so traversal and symlinks outside that root are refused. It uses the same HTTP authentication as the other filesystem routes.
Under the `disabled` or `active_only` index strategies it deliberately builds
nothing, so a repo switch cannot index behind a setting that asked it not to.
The `warm_content_index` IPC command applies the identical gate — the two
transports share one implementation so they cannot drift.

`/fs/search-content-all` reports why a repo produced nothing, so the UI can tell
"no match" apart from "not searched yet". Its result and every
`content-search-batch` carry `repos_searched`, `repos_pending`, and
`repos_indexing` — the last being the subset of `repos_pending` with a build
actually in flight. Only that subset justifies telling the user to retry; the
rest are waiting on a scheduling event that may never come.

Content-search results expose `match_start` and `match_end` as zero-based,
end-exclusive UTF-16 code-unit offsets within `line_text`. They can be passed
directly to JavaScript `String.slice`, including when text before the match
contains multibyte characters or non-BMP emoji.

Sandboxed filesystem operations for the file manager panel. `/fs/read-external` reads an arbitrary absolute path (not sandboxed to a repo).

## Agent Usage Endpoints

```
GET /claude/usage?sessionId=<id>               -> UsageApiResponse (session profile rate limits, 5-min cache per profile; omitted id selects default)
GET /claude/projects                           -> ProjectEntry[]
GET /claude/timeline?scope=all&days=7          -> TimelinePoint[] (hourly token aggregation)
GET /claude/session-stats?scope=current        -> SessionStats
GET /codex/usage                               -> CodexUsageApiResponse (rate-limit usage, 5-min cached)
GET /codex/stats                               -> CodexStatsResponse (token history + lifetime stats)
GET /grok/usage                                -> GrokUsageApiResponse (billing usage, 5-min cached)
```

### Terminal theme

```
POST /terminal/theme-colors   {foreground:[r,g,b], background:[r,g,b], cursor:[r,g,b]}
```

Publishes the resolved terminal theme so the emulator can answer OSC 10 / 11 /
12 colour queries. Not session-scoped: one window, one palette.

This is not cosmetic. An app asks for the background with `OSC 11 ; ? ST`
followed by `ESC[c` as a fence — DA is universally supported, so a DA reply
arriving with no colour reply before it is meant to mean "this terminal cannot
answer". A terminal that stays silent therefore never reads as "no", and the
probe simply retries: Claude Code re-sent the pair every ~1.2 s for the whole
life of a session, and every DA reply it triggered was written into the PTY as
if typed. Until the frontend posts here, the emulator answers from a dark
default — a plausible wrong colour ends the probe, silence does not.

Powers the Claude Usage dashboard in browser/PWA/remote. `scope` is `"all"`,
`"current"`, or a project slug. `timeline`/`session-stats` are desktop-only Tauri
commands; the handlers call non-gated `*_impl` siblings so they also serve the
remote daemon.

Both Codex routes launch the documented Codex App Server over JSONL and request
`account/rateLimits/read` plus `account/usage/read` in one cached snapshot. The
Codex CLI owns OAuth, refresh, and its upstream protocol; TUIC never reads
`~/.codex/auth.json` or calls private ChatGPT backend routes. The response keeps
the existing snake-case TUIC transport shape, but fields that the official
surface does not expose remain `null` rather than being inferred.

`/grok/usage` launches `grok agent stdio` briefly and calls Grok Build's
`_x.ai/billing` ACP extension. It returns billing-period utilization,
subscription tier, on-demand amounts, and prepaid balance. This process is
telemetry-only and does not change the PTY routing of interactive Grok tabs.

Gemini has no corresponding endpoint: its `/stats` and `/usage` commands are
session-local UI data, not a stable account quota contract. TUIC therefore does
not expose a Gemini account-usage route.

**Absolute-path write boundary.** `/fs/write-external`, `/fs/copy-abs`, and `/fs/move-abs` are gated to **registered repository roots** for the HTTP boundary (a 403 otherwise), mirroring `/fs/read-external`. The gate rejects traversal syntax (`..`), NUL bytes, and relative paths *before* the containment check: containment is `Path::starts_with`, which is purely lexical, so `/repo/../../etc/passwd` is "inside" `/repo` by components while the OS resolves it far outside. Paths are deliberately **not** canonicalized — a symlink inside a registered repo that points outside it is an accepted design decision in this project. `/fs/transfer` gates only its `destDir` — sources are commonly external (a file dragged in from the desktop). `/fs/stat` and `/fs/resolve-terminal-path` return only metadata (no content) so they are not repo-gated; both also refuse macOS TCC-protected directories. `/fs/resolve-terminal-path` returns JSON `null` on a miss (`Option<ResolvedFilePath>`). `/fs/resolve-terminal-paths` is its batched sibling and is a POST for one reason: a whole terminal screen's candidates do not fit a query string, and being able to send many of them is the point. It answers **positionally** — the array it returns has one entry per input candidate, in order, `null` where that candidate resolved to nothing — so a caller may index the response by the index of the request.

`/fs/resolve-markdown-link` returns a tagged `kind` (`heading`, `file`, `missing`, or `blocked`). It decodes path and fragment separately, resolves relative to the source file, permits local symlinks outside the root, and refuses UNC/network paths before filesystem access.

## Monitoring Endpoints

### Health Check

```
GET /health
```

Returns `{ "ok": true, "uptime_secs": N, "session_count": N, "protocol_version": 1, "build": { "version": "...", "target": "...", "sha256": "..." }, "socket_path"?: "...", "instance_id": "<uuid>", "survive_secs"?: N }`.

The one route served without a credential. `instance_id` identifies the running
**process** (minted at startup, not derived from the instance id or the config
directory): a remote connection compares it against its own before mirroring and
refuses a base URL that resolves back to itself. A daemon that omits the field is
older than the check and still connects.

The daemon hashes its running executable once at startup, so `build.sha256`
identifies the process that answered even after an update stages a new file.
`POST /remote/update` exists only on the daemon router. It needs the same
session token as PTY access, supplied only as a `tui-session` cookie (a
`?token=` query is rejected with 401 on this route and on `/fs/upload-copy`), and the `x-tuic-target`, `x-tuic-sha256`, and
`x-tuic-confirmed-sessions` headers. It streams at most 512 MiB into the
daemon executable's own directory, verifies the hash and current session
count, and atomically promotes the file before restarting. Windows currently
returns 501 because a running executable cannot be replaced there.

`survive_secs` is present when `tuic-remote` was launched with an idle lifetime.
Desktop-managed SSH deployment sets the vault pairing token as this daemon's
session token, so the same `?token=` contract protects every authenticated HTTP,
SSE and WebSocket route without a separate pairing bypass.

### Orchestrator Stats

```
GET /stats
```

Returns `{ "active_sessions": N, "max_sessions": 50, "available_slots": N }`.

### Session Metrics

```
GET /metrics
```

Returns `{ "total_spawned": N, "failed_spawns": N, "bytes_emitted": N, "pauses_triggered": N }`.

### Raw PTY Capture

`POST /diagnostics/capture` accepts `{ "enabled": true, "session_id"?: "<id>" }`
to start recording one or all sessions; `{ "enabled": false }` stops it.
`GET /diagnostics/capture` reports the active directory, session filter and
bytes recorded. Captures default to `<config dir>/captures/<id>.tcap`.
Set the process environment variable `TUIC_CAPTURE_DIR` to an absolute path
before launch to select another directory. A relative value returns
`{ "enabled": false, "error": "TUIC_CAPTURE_DIR must be absolute" }` on enable.

### Local IPs

```
GET /system/local-ips
```

Returns list of local network interfaces and addresses.

### Local IP (Primary)

```
GET /system/local-ip
```

Returns the preferred local IP address (single value).

### Home Directory

```
GET /system/home-directory
```

Returns the serving machine's home directory as a JSON string. The remote repository picker uses this route through the selected connection.

## Watcher Endpoints

### Head Watcher

```
POST   /watchers/head?path=/path/to/repo
DELETE /watchers/head?path=/path/to/repo
```

Start/stop watching `.git/HEAD` for branch changes. Browser-only mode.

### Repo Watcher

```
POST   /watchers/repo?path=/path/to/repo
DELETE /watchers/repo?path=/path/to/repo
```

Start/stop watching `.git/` for repository state changes. Browser-only mode.

### Directory Watcher

```
POST   /watchers/dir?path=/path/to/directory
DELETE /watchers/dir?path=/path/to/directory
```

Start/stop watching a directory (non-recursive) for file changes (create/delete/rename). Emits `dir-changed` SSE event. Used by File Browser panel for auto-refresh.

### Hot Repos

```
PUT /watchers/hot-repos
```

Body: `{"paths": ["/path/to/repo", ...]}`

Updates the set of "hot" repository paths (repos with active terminals). Cold repos (not in this set) get throttled watcher debounce (15s vs 1.5s) and reduced GitHub polling frequency (~10min vs ~1min). HTTP equivalent of the `set_hot_repos` Tauri command, served by both the desktop server and `tuic-remote` — a browser client of the desktop app needs it just as much as a remote one.

### Every `/ai/*` route is unmounted (#784-0aec)

`GET/PUT /ai/chat/config`, the `/ai/chat/conversation*` CRUD, the per-chat and
per-session token WebSockets, `/ai/watchers*`, `/ai/conversation/*`,
`/ai/session-knowledge`, `/ai/knowledge/*`, `/ai/scheduler/config`,
`/ai/suggestions/toggle`, `/ai/triage/run`, `/ai/improvements/scan` and
`/repo/create-issue-from-proposal` are all gone, together with the Rust modules
behind them. Nothing under `/ai/` is served.

`build_router` is the authority, and the route parity gate holds the two halves
together: `command_table_paths_all_hit_a_registered_route` `PATCH`-probes every
path `src/transport.ts` can produce, so a `COMMAND_TABLE` entry pointing at a
removed route fails the Rust suite rather than 404-ing at runtime.

The ACP routes are the replacement surface — see **ACP endpoints** — and they
carry no provider configuration: TUICommander launches one ego executable named
by `ego_executable` in `app_config.json` and holds no API key.

## Agent Endpoints

### Verify Agent Session

```
POST /agents/verify-session
Content-Type: application/json

{ "agentType": "claude", "sessionId": "af467730-5e79-49d9-8a17-ebd94c99f262", "cwd": "/work/project", "agentPid": null, "envOverrides": { "CLAUDE_CONFIG_DIR": "/profiles/work" } }
```

Returns a JSON boolean. `agentPid` is a live process ID when available and `null` after restart; `envOverrides` carries the saved launch profile so verification reads the same session store the agent used. The same fields apply to Codex (`CODEX_HOME`) and Gemini (`GEMINI_CLI_HOME`).

### Detect All Agents

```
GET /agents
```

Returns detected agent binaries and installed IDEs.

### Detect Specific Agent

```
GET /agents/detect?binary=claude
```

Returns `{ "path": string|null, "version": string|null, "supports_no_alt_screen": boolean }` for a specific agent binary name or absolute executable path. Codex and Grok probe `--no-alt-screen`; OpenCode probes `--mini`. Successful help results are cached per executable version. A failed probe warns once and is retried after a short cooldown; each attempt has a two-second deadline. An absolute path checks that exact installed version.

### Prepare Agent Launch Arguments

```
POST /agents/launch-args
Content-Type: application/json

{ "agentType": "codex", "binaryPath": "codex", "args": ["resume"] }
```

Returns the argument array with the agent's supported native-scrollback option inserted before an interactive subcommand. An explicit option, a disabled per-agent `prevent_alt_screen` setting, or a non-interactive subcommand leaves the array unchanged. The removed `allowAltScreen` request field is rejected, as is `allow_alt_screen` on HTTP agent spawn. The desktop `prepare_agent_launch_args` command uses the same Rust builder. Remote HTTP callers must authenticate.

### Detect Installed IDEs

```
GET /agents/ides
```

Returns list of installed IDEs.

### Batch Detect Agent Binaries

```
POST /agents/detect-all
Content-Type: application/json

{ "binaries": ["claude", "codex"] }
```

Returns `{ "<binary>": { "path": string|null, "version": string|null, "supports_no_alt_screen": false }, ... }`.
Detection runs in parallel and skips version lookup for speed; use
`GET /agents/detect` when the version matters. Blank names are dropped, so a name
that was sent may be absent from the map.

### Open Path in Application

```
POST /agents/open-in-app
Content-Type: application/json

{ "path": "/repo/src/main.rs", "app": "vscode", "line": 42, "col": 7 }
```

Opens the path in an IDE, terminal or file manager. `line` and `col` are optional
and only used by editors that support them. An unknown `app` is rejected before
anything is spawned. Returns `null` on success, matching the `open_in_app` command.

## Dictation Endpoints

The HTTP routes and Tauri commands share the root dictation adapter. The `tuic-dictation` crate supplies the audio and speech domain without changing these route shapes.

Desktop-only: audio capture and the whisper model live behind the `desktop`
feature, so the remote daemon serves none of these. Every response is exactly what
the matching `dictation::commands` Tauri command resolves to — a bare string for
`inject`, `null` for the commands that return nothing — because
`src/stores/dictation.ts` reads both transports with the same code. Before the app
handle is up, every state-touching route answers `503`.

```
GET  /dictation/status                              -> DictationStatus
GET  /dictation/models                              -> ModelInfo[]
POST /dictation/models/download  { "model": "..." } -> "Downloaded to <path>"
POST /dictation/models/delete    { "model": "..." } -> "<deletion message>"
GET  /dictation/speech/assets                       -> SpeechAssetInfo[]
POST /dictation/speech/assets/download  { "asset": "..." }
                                                    -> "Installed to <path>"
POST /dictation/speech/assets/cancel    { "asset": "..." }
                                                    -> "<cancellation message>"
POST /dictation/speech/assets/delete    { "asset": "..." }
                                                    -> "<deletion message>"
GET  /dictation/speech/voices?language=it           -> VoiceChoice[]
POST /dictation/speech/voices/import    { "language": "it", "name": "...", "dataBase64": "..." }
                                                    -> "<import message>"
POST /dictation/speech/voices/delete    { "language": "it", "name": "..." }
                                                    -> "<deletion message>"
POST /dictation/speech/voices/preview   { "language": "it", "voice": "...", "text": "..." }
                                                    -> null
POST /dictation/speech/speak     { "text": "...", "turn": 3 }
                                                    -> SpokenReply
POST /dictation/speech/stop                         -> SpeechStatus
POST /dictation/speech/pause                        -> SpeechStatus
POST /dictation/speech/resume                       -> SpeechStatus
GET  /dictation/speech/status?utterance=7           -> SpeechStatus
POST /dictation/start          { "source": "fn" | "hotkey" | "ui" } -> null
POST /dictation/stop                                -> TranscribeResponse
GET  /dictation/corrections                         -> { "<from>": "<to>", ... }
PUT  /dictation/corrections      { "map": { ... } } -> null
GET  /dictation/devices                             -> AudioDevice[]
POST /dictation/inject           { "text": "..." }  -> "<corrected text>"
GET  /dictation/config                              -> DictationConfig
PUT  /dictation/config           {base, config}     -> null
GET  /dictation/hands-free                          -> HandsFreeStatus
GET  /dictation/hands-free/default-notice           -> string
POST /dictation/hands-free/arm   { "sessionId": "...", "owner": "..." }
                                                    -> HandsFreeStatus
POST /dictation/hands-free/disarm                   -> HandsFreeDisarmed
GET  /dictation/hands-free/audio?owner=<id>         -> WebSocket upgrade
```

`POST /dictation/stop` stops the recording and transcribes it, returning
`{ text, skip_reason, duration_s, truncated_s }`. A final transcription gate's
`skip_reason` is returned unchanged. A capture without 200 ms of activity at
the configured transcription RMS floor returns `no sustained speech` before
Whisper; an empty successful transcription uses
`no speech detected`. `PUT /dictation/config` takes the loaded `base` and
edited `config`; unchanged keys survive a concurrent save.

The speech-asset routes take `asset`, an id from the catalogue in
`dictation::speech::assets`. That is an allowlist, not a hint: an unknown id is
a `400`-shaped error string rather than a path or a URL built from what the
caller sent. `download` is minutes long and streams nothing back — progress
arrives as `speech-download-progress`, on the desktop window **and** on
`/events`, carrying `{ asset, downloaded, total, percent }`, then one final `{ asset, done: true }`
whether the download succeeded or failed. That last event is how a client that
did not start the download ends its bar; it carries no outcome, so re-read
`GET /dictation/speech/assets` for the new state. The Whisper-model
download beside it pushes `dictation-download-progress` with the same body minus
`asset`.

`SpeechAssetInfo` is **snake_case on the wire** — it carries no serde rename,
unlike `SpeechStatus` and `HandsFreeStatus` beside it. Its `language` is the
**Whisper language code** (`"it"`), null for the runtime library: the same
alphabet as `DictationConfig.language` and `SpeechStatus.language`, so a client
can join an asset to the configured language directly. For a language, `voices`
lists the voice it ships. `get_speech_assets` also lists every catalogue voice
(`kind: "voice"`, `language` = its language's code, `voice` = the voice name),
which the download routes above accept like any other asset.

### Voices

`language` is the Whisper code on all four routes.

- `GET /dictation/speech/voices?language=` answers the voices the language can
  speak with now — the values `speech_voice` may be set to — as
  `[{ id, source }]`, `source` being `"default"`, `"downloaded"` or `"user"`.
  It is `[]` while the language itself is not fully downloaded.
- `POST /dictation/speech/voices/import` stores a voice file the user chose. The
  body carries the whole file as base64 under `dataBase64`, the same camelCase
  key as the IPC argument. **This route alone accepts a body of about 85 MiB**
  (`SPEECH_VOICE_IMPORT_BODY_BYTES`: a 64 MB file in base64 plus 64 KiB); every
  other route keeps the 2 MB limit. The server checks the name, the size and
  whether the file fits the language's model before it stores anything, and
  refuses with the reason otherwise.
- `POST /dictation/speech/voices/delete` removes a user voice file. Absent is
  success.
- `POST /dictation/speech/voices/preview` speaks `text` (at most 200 characters)
  in `voice` on the speaker of the machine that runs TUICommander, with the saved
  loudness settings. It needs no hands-free conversation and does not change the
  saved voice. It is refused, not queued, while a hands-free reply is queued,
  rendering or playing.

### Spoken replies

The three speech routes are the browser/remote half of `speak_reply`,
`stop_speech` and `get_speech_status`, and carry the identical payloads — the
same store code reads both transports.

`POST /dictation/speech/speak` takes `{ text, turn? }` and answers
`SpokenReply { utteranceId, state, error?, turn }`. `state` is what the queue
knows at that instant, which is `"queued"` and never `"finished"`: accepting a
reply is not the user hearing it. Poll `GET /dictation/speech/status?utterance=`
with the id to learn the outcome — `finished`, `interrupted` (normal, the user
talked over it) or `failed`. An id the conversation no longer remembers comes
back as `"unknown"`, which is a different answer from "you asked about nothing".

`turn` is optional and refuses a reply written for a turn the user has already
talked over; omitting it means "now". `POST /dictation/speech/stop` stops
immediately, drops the queue and opens a new turn, returning the `SpeechStatus`
so the caller learns that turn.

All three are answerable at any time: with nothing armed, or with no working
voice, `status` reports `available: false` and names the reason rather than
failing.

Polling is no longer the only way to learn the outcome: every transition is
pushed as `speech-utterance`, carrying the same `SpokenReply` body this route
returns, on the desktop window and on `/events`. It is emitted by the render
thread that performs the transition rather than by `speak` — `finished` and
`interrupted` happen long after `speak` returned — so a client that subscribes
sees `queued → rendering → speaking → finished` without asking.

`SpeechStatus.language` is the two-letter code this conversation is being held
in: the dictation setting when it names one, and what Whisper detected when the
setting is `auto`. Empty means nobody has spoken yet under `auto` — the one
state in which no reply can be spoken, reported as `available: false` with a
reason naming Auto rather than filled in with a default. Neither the speak
payload nor the `voice` MCP tool takes a language or a voice: the language comes
from this field and the voice from the `speech_voice` setting, and a second
source would disagree with them the first time the user switched languages.

`speech_voice` names one of the voices the configured language ships, reported
in `voices` by `GET /dictation/speech-assets`. Empty — what every configuration
written before the setting existed says — means the first voice that language
ships. A name the language does not ship is refused with a message that lists
what it does ship, rather than being replaced by a voice nobody chose.

`PUT /dictation/config` with a different `language`, `speechCommand` or
`speech_voice` drops the voice built for the previous one, cancelling whatever
it was speaking. Every other field leaves it alone.

### Hands-free

`POST /dictation/hands-free/arm` binds the delivery target and the audio owner,
and opens a generation. It does **not** open a microphone. A target that cannot
take a Compose entry — an unknown session, or a session that is not running an
agent — is refused with the error the Tauri command returns, so the two
transports reject the same cases.

`owner` is the caller's own endpoint identity: a remote client that arms here
binds *itself*, so a later disconnect can disarm the mode it owns. Both fields
are bounded at 256 bytes and may not be blank. **Open the audio socket below
first** — arming an owner with no socket is refused with `No client is connected
for audio owner '<owner>'`, because the backend will not substitute the server's
own microphone for a client's (see `docs/backend/dictation.md`).

`POST /dictation/hands-free/disarm` disarms the whole mode and drops the
transcript that never reached the model (`discardedPending`). It is idempotent:
the second call reports `wasArmed: false`. Spoken turns are typed straight into
the bound agent's composer — busy or not — and never parked in the Compose
queue, so there is nothing to cancel and nothing typed can be taken back.

Both bodies are the structs `dictation::commands` serializes, camelCase on the
wire and identical on both transports:

- `HandsFreeStatus { armed, phase, sessionId, owner, generation, pendingText, holdBackMs, error, deliveredTurns, droppedTurns }`
- `HandsFreeDisarmed { wasArmed, generation, discardedPending, discardedCapture, status }`

`pendingText` is the turn waiting out its hold-back, or held by a permission
dialog or a draft in the composer (phase `holding_back`) until it can be typed.
With an activation phrase, `holdBackMs` reports the effective minimum of 5000
ms, even when the saved hold-back setting is shorter. A continuation that
starts before that deadline joins the same pending turn.

`phase` is one of `disarmed`, `waiting`, `capturing`, `transcribing`,
`holding_back`, `delivered`, `error`.

`deliveredTurns` counts spoken turns typed into the composer and
`droppedTurns` counts turns the activation phrase dropped. Both are monotonic
for the life of the backend process and are never reset on arm, so a client
detects a turn by comparing two polls — the earcons key on them, because a
`delivered` phase can be overwritten before the next poll and a drop has no
phase at all. `hands_free_earcons` in the dictation config (default `true`)
turns them off; only the frontend reads it.

Arming and disarming also tell the bound model so, unless
`hands_free_notify_model` is `false` in the dictation config (it defaults to
`true`; `DictationConfig` is snake_case on the wire, unlike the two structs
above). The start notice is typed like a spoken turn; a dialog or a draft holds
it in the mode, ahead of any turn, and a disarm that arrives first drops it — in
which case no end notice is sent, because the model never read the start one.
`hands_free_start_notice` replaces the start notice's text (empty means the
built-in one, which `GET /dictation/hands-free/default-notice` returns as a JSON
string); Rust folds it to one line before it is typed.
`error` carries the reason when the queue refuses a notice; arming still
succeeds. Turning the setting off never leaves speech running: a disarm revokes
it either way.

### The client's microphone and speaker (832-e730)

`GET /dictation/hands-free/audio?owner=<id>` upgrades to the WebSocket that
*is* a browser client's audio hardware, seen from the conversation's side. It
carries capture up and replies down on one socket, in both directions at once.

| Direction | Frame | Meaning |
|---|---|---|
| up | binary | `f32` little-endian samples, mono, **16 kHz** |
| up | `{"type":"playback-ended"}` | the reply finished playing |
| down | binary | `u32` little-endian sample rate, then `f32` little-endian samples |
| down | `{"type":"stop"}` | stop playing and drop anything queued |
| down | `{"type":"pause"}` | hold the reply where it is, and any that arrives |
| down | `{"type":"resume"}` | continue from where it was held |

The downlink carries its own rate because it is not the uplink's: the engine
renders at 24 kHz and the client resamples. A reply decoded at the capture rate
plays 50% slow — audible to a person, invisible to an assertion about lengths.
A trailing partial sample on the uplink is ignored rather than rejected.

`owner` must be present, non-empty and **not** the literal `desktop`; both
violations are a `400`. Registering as `desktop` would let a remote client
impersonate the machine's own endpoint, which is the one owner that means local
hardware.

Opening a second socket under an owner that already has one replaces it and
disconnects the first, so the conversation the first socket held disarms with
`OwnerDisconnected` rather than silently following the new client. Closing the
socket does the same. There is no reconnect window: a conversation belongs to a
socket, not to a name.

Playback here is best-effort by construction — the server can stop *sending*,
but only the client can stop *playing*. The `playback-ended` ack is what lets
the backend see the end of a reply; without it the utterance stays `speaking`
until its rendered duration elapses.

## Desktop Integration Endpoints

Desktop-only, like the three commands they mirror: the relay client, the audio
output and the updater are all gated on the `desktop` feature.

### Relay Status

```
GET /system/relay-status
```

Returns `{ "enabled": bool, "connected": bool, "url": string, "session_id": string }`.
Shares one body with the `get_relay_status` command, so the two transports cannot
drift.

### Play Notification Sound

```
POST /system/notification-sound
Content-Type: application/json

{ "sound": "question", "volume": 0.5, "device": "Studio Display Speakers" }
```

`sound` is one of `question`, `completion`, `error`, `warning`, `info`,
`attention`. `device` is the chosen audio output; `null` or omitted uses the
system default. Returns `null`, matching the command.

### Check Update Channel

```
GET /system/check-update?channel=nightly
```

Returns `{ "available": bool, "version": string|null, "notes": string|null, "release_page": string|null, "not_found": bool }`.
Channel URLs are hardcoded — no user-supplied URL is accepted. An unpublished
channel is `not_found: true`, not an error; an unknown channel name is a 500.

## Prompt Endpoints

### Process Prompt

```
POST /prompt/process
Content-Type: application/json

{ "content": "...", "variables": { ... } }
```

Substitutes `{{var}}` placeholders in prompt text.

### Extract Variables

```
POST /prompt/extract-variables
Content-Type: application/json

{ "content": "..." }
```

Returns list of `{{var}}` placeholder names found in content.

## Plugin Endpoints

### List Plugins

```
GET /plugins/list
```

Returns array of valid plugin manifests.

### Plugin Development Guide

```
GET /plugins/docs
```

Returns the complete plugin development reference as `{"content": "..."}`. AI-optimized documentation covering manifest format, PluginHost API, structured event types, and example plugins.

### Plugin Data

```
GET /api/plugins/:plugin_id/data/*path
```

Reads a plugin's stored data file. Returns `application/json` if content starts with `{` or `[`, otherwise `text/plain`. Returns 404 if the file doesn't exist. Goes through the same auth middleware as all other routes.

**Note:** `write_plugin_data` maps to `POST /api/plugins/:plugin_id/data/*path`; `delete_plugin_data` has no HTTP route (no frontend caller). Data is sandboxed to `~/.config/tuicommander/plugins/{plugin_id}/data/`.

### Plugin RPC (host capabilities, story 071)

Browser/PWA parity for the plugin host RPC surface. Every route is `:plugin_id`-scoped
and reuses the same per-plugin sandboxing as the Tauri commands (`plugin_fs.rs` path
jail, `plugin_http.rs` allowed-URL check, `plugin_exec.rs` binary whitelist).

```
GET  /api/plugins/:plugin_id/fs/read?path=<p>                     -> string        (plugin_read_file)
GET  /api/plugins/:plugin_id/fs/read-base64?path=<p>&maxBytes=<n> -> string        (plugin_read_file_base64; maxBytes optional, 10 MiB default, 512 MiB host ceiling)
GET  /api/plugins/:plugin_id/fs/tail?path=<p>&maxBytes=<n>        -> string        (plugin_read_file_tail)
GET  /api/plugins/:plugin_id/fs/list?path=<p>&pattern=&sortBy=    -> string[]      (plugin_list_directory)
POST /api/plugins/:plugin_id/fs/write    { path, content }        -> { ok }        (plugin_write_file)
POST /api/plugins/:plugin_id/fs/write-base64 { path, content, maxBytes? } -> { ok } (plugin_write_file_base64; 10 MiB default, 512 MiB host ceiling, atomic replace)
POST /api/plugins/:plugin_id/fs/rename   { from, to }             -> { ok }        (plugin_rename_path)
POST /api/plugins/:plugin_id/build-artifacts/scan   { repoPaths, forceRefresh? } -> BuildArtifact[]
POST /api/plugins/:plugin_id/build-artifacts/delete { path, repoPaths } -> { ok }
POST /api/plugins/:plugin_id/build-artifacts/trim   { path, repoPaths } -> { ok }   (intermediates only; keeps executables)
POST /api/plugins/:plugin_id/exec        { binary, args, cwd? }   -> string        (plugin_exec_cli)
POST /api/plugins/:plugin_id/http        { url, method?, headers?, body?, allowedUrls } -> HttpResponse
GET  /api/plugins/:plugin_id/pty/output?sessionId=<id>&maxLines=  -> string        (plugin_read_session_output)
POST /api/plugins/:plugin_id/register    { capabilities }         -> { ok }
POST /api/plugins/:plugin_id/unregister                           -> { ok }
GET  /api/plugins/:plugin_id/readme                               -> string | null
```

Build-artifact scans normalize the root set, share an in-flight scan across callers,
and reuse completed results for 30 seconds. Set `forceRefresh: true` to bypass a
completed cached result; a scan already running for the same roots remains shared.

Intentionally **not** mapped (native/host-only, stay Tauri-only): `plugin_watch_path` /
`plugin_unwatch` (change events need AppHandle/WS delivery), `plugin_read_credential`
(OS keychain), and user-plugin install/uninstall (`install_plugin_from_*`,
`uninstall_plugin` — local-FS install + AppHandle emit). `delete_plugin_data` is unmapped
for lack of a frontend caller (YAGNI).

### Plugin Output Watchers

```
POST /api/plugins/output-watchers
Content-Type: application/json

{
  "client_id": "b7d1…",
  "seq": 3,
  "watchers": [{ "id": "w0", "pattern": "model is at capacity", "flags": "i" }]
}
```

Replaces the compiled OutputWatcher set the PTY reader thread matches lines against
(`set_plugin_output_watchers`). `pattern` and `flags` are the source and flags of a JS
`RegExp`; `i`, `m` and `s` are applied. The route is not `:plugin_id`-scoped: one
frontend owns one set, holding the watchers of all its plugins, and pushes all of it on
every add or remove.

`client_id` identifies the frontend. Sets are per client (at most 8; the least recently
synced is evicted, and an empty `watchers` array leaves a parked record so a delayed
older sync cannot resurrect the disposed set), so a desktop window and a browser tab
cannot overwrite each other. A client is expected to re-post its set every 30 s while it
holds any watcher: nothing signals a disconnect, so that heartbeat is both what keeps it
from being evicted as dead and how it recovers if it was. It must not contain `/`, which qualifies the
watcher ids reported back. `seq` is a monotonic per-client counter that orders the
mutations: a sync whose `seq` is not above the stored one is stale and changes nothing.

Returns `{ "applied": bool, "rejected": [id] }`. `rejected` lists the ids the Rust
`regex` crate cannot compile (lookaround, backreferences, a negated class escape inside
a character class). A rejected id is not an error — the frontend keeps matching that
watcher itself, and Rust ships every assembled line for as long as one is registered.
When `applied` is `false` the sync was stale: the client must ignore `rejected`, because
it describes a set the backend does not hold.

Assembled lines are pushed back as the `watcher-lines` WebSocket frame (on
`/sessions/:id/stream` in both `?format=grid` and raw mode; `?format=log|text` does not
carry it) and the `plugin-watcher-lines` SSE event.

## Worktree Endpoints

### List Worktrees

```
GET /worktrees
```

Returns list of managed worktrees.

### Create Worktree

```
POST /worktrees
Content-Type: application/json

{ "base_repo": "/path", "branch_name": "feature-x", "base_ref": "main" }
```

`base_repo` must be an absolute, normalized path. The route rejects invalid paths
before invoking git, matching MCP `repo action=worktree_create` validation.

Creation always produces a linked worktree and refuses a branch that already
has a checkout. Parent tracked and untracked changes are not carried over.

`201` returns:

```json
{
  "name": "feature-x",
  "path": "/path__wt/feature-x",
  "workspace_id": "feature-x",
  "branch": "feature-x",
  "base_repo": "/path"
}
```

`instructions` is the model-facing payload: it states that tracked changes were
not carried over, lists warm artifact directories with their sizes and
`warmed_directories`, and explains that refs and objects are shared with the
parent. It is byte-identical to
what MCP `repo action=worktree_create` returns — one value, two carriers — and
it is the ONLY instruction channel: there is no enforcement layer behind it.
Its `warm_artifacts.status` starts as `pending`; wait for `done` or `failed`
in `GET /worktrees/paths?path=<base_repo>` before installing or building.
The setup script completes before warming begins. Desktop IPC creation also
returns `pending` and warms in the background.

`workspace_id` is how every later call addresses this workspace — `DELETE
/worktrees/:workspaceId`, `POST /worktrees/finalize`,
`GET /repo/worktree-dirty`, MCP `action=worktree_remove`. Read it from the
response rather than deriving it from display data. MCP
`repo action=worktree_create` returns the same two fields.

`GET /worktrees/lifecycle` provides the removal preview: branch history, dirty and untracked counts, live session names, and warnings. `DELETE /worktrees/:workspaceId` includes the same warnings on success.

Creation announces itself on both transports as `worktree-created`
(`{ repo_path, workspace_id, branch, worktree_path, kind, creator_session, spawn_session }`, with `kind` equal
to `"worktree"`) and removal as
`worktree-removed` (`{ repo_path, workspace_id, branch }`) — the desktop Tauri
event and the `/events` SSE frame serialize the same struct, so the field names
are identical by construction. Payload table: `docs/sync-matrix.md`.

`creator_session` is the live PTY id resolved from the MCP connection, or null
for creation without a bound caller. When `spawn_session` is false, the UI moves
only that session within its existing repository. Explicit session creation
leaves the caller in place. Placement does not change the shell cwd or send
input to an agent. The existing per-workspace terminal snapshots preserve the
placement on restart; an inactive caller does not change the current selection.

### Worktrees Base Directory

```
GET /worktrees/dir
```

Returns the base directory where worktrees are created.

### Get Worktree Paths

```
GET /worktrees/paths?path=/path/to/repo
```

Returns `{ "<workspace-id>": { "branch": "feature-x", "path": "/worktree/path", "kind": "worktree", "warm_artifacts": { "status": "pending" } }, ... }`.

The map is keyed by workspace id and carries the branch explicitly as display
data. Linked-worktree ids currently equal their branches, but clients should
use the returned id for later calls.

### Workspace Lifecycle Preflight

```
GET /worktrees/lifecycle?repoPath=/path&workspaceId=feature-x~a1b2c3d4
```

Returns a fresh `{ dirty_files, missing_checkout, dirty_fingerprint?, submodule_unpushed_commits, commit_status, removal_safety, error? }` verdict
for one exact workspace. `dirty_files` counts the staged, unstaged and untracked
files a removal would discard; `null` means the inspection failed and is not the
same answer as `0`. `dirty_fingerprint` identifies checkout status, HEAD, and submodule refs for
revalidation. `submodule_unpushed_commits` lists counts per initialized module
for commits absent from its remote-tracking branches. `commit_status` is
`unmerged`, `in_sync`, `merged`, or `unknown` — `in_sync` is HEAD sitting on the default branch's tip, which
satisfies the same ancestry check as `merged` while having merged nothing.
`removal_safety` is `safe`, `requires_force`, or `unknown`. An inspection
failure is returned as an `unknown` verdict and must never be treated as zero or
safe. A missing registered checkout returns `missing_checkout: true`, `dirty_files: null`, no fingerprint, and `requires_force`; unknown ids remain `unknown`. The response also includes `untracked_files`, `live_sessions` with names, and `warnings` for removal review. This is the HTTP twin of `get_workspace_lifecycle`.

### Generate Worktree Name

```
POST /worktrees/generate-name
Content-Type: application/json

{ "existing_names": ["name1", "name2"] }
```

Returns a unique worktree name.

### Finalize Merged Worktree

```
POST /worktrees/finalize
Content-Type: application/json

{ "repoPath": "/path/to/repo", "workspaceId": "feature-x", "action": "archive", "force": false }
```

Finalizes a merged worktree, addressed by workspace id. The merge already
happened, so no branch is needed here — only which checkout to dispose of. `action` must be `"archive"` (moves to archive directory) or `"delete"` (removes worktree and branch).
For `action: "delete"`, the response includes `branch_delete_warning` when the worktree was removed but safe branch deletion failed, for example because the branch has unmerged commits.

`force` (optional, default `false`) records explicit confirmation. A dirty, unverified, or live worktree returns `{ "action": "needs_confirmation", "merged": true }` without cleanup; an archive also waits if commit integration is unverified. Ask the user, then re-send with `"force": true` and `"expectedFingerprint"` from the confirmed lifecycle verdict. A changed fingerprint aborts cleanup. This route shares `finalize_merged_worktree_impl_with_confirmation` with the Tauri command, so both transports pass the identical gate.

### Run Setup Script

```
POST /worktrees/run-script
Content-Type: application/json

{ "script": "pnpm install", "cwd": "/path/to/worktree" }
```

Runs the script through `sh -c` (Unix) or `cmd /C` (Windows) in `cwd` and returns
exit code plus captured output. `cwd` accepts `~`. A `cwd` that does not exist is a
500 with `{ "error": ... }`. Same body as the `run_setup_script` command.

### Remove Worktree

```
DELETE /worktrees/:workspaceId?repoPath=/path&deleteBranch=true
```

Query parameters:
- `repoPath` (required) -- base repository path
- `deleteBranch` (optional, default `true`, or `false` when `force=true`) -- when `true`, also requests deletion of the local git branch
- `force` (optional, default `false`) -- when `true`, permits discarding dirty linked-worktree files but does not bypass branch proof or a lock
- `overrideLock` (optional, default `false`) -- explicit authorization to override a locked worktree during removal
- `expectedFingerprint` (required with `force=true` for an existing checkout) -- lifecycle fingerprint shown at force confirmation; removal refuses if checkout status, HEAD, or submodule refs changed
- `confirmMissingCheckout` (required with `force=true` for a missing registered checkout) -- confirms the lifecycle result without inventing a fingerprint; removal refuses if the checkout reappears

The path segment is the opaque workspace id from `GET /worktrees/paths`, not a branch name.

Returns `{ "ok": true, "branch_delete_warning": null, "removal_rule": "ancestry" }`
on full success. `removal_rule` names the rule that allowed removal:
`in_sync`, `ancestry`, `patch_equivalence`, `kept_branch`, or `force`.
A non-force request requires a clean worktree and submodules with no Git
operation in progress, even when `deleteBranch=false`. A populated submodule
requires one `git worktree remove --force` after a fresh clean-state check.
Initialized submodule refs are preserved in the main module repository; an
uninitialized submodule without Git state does not block removal. A clean
branch whose commits were squash- or rebase-merged can use
`patch_equivalence` when `git cherry` finds no unique patches. Merge commits
are refused because `git cherry` does not compare their resolution changes. When
`deleteBranch=true` and the branch fails proof or moves after the preflight,
the compare-and-delete operation keeps the branch and the request
still succeeds with `branch_delete_warning` set so clients can report the
partial outcome.

## Push Notification Endpoints

### Get VAPID Public Key

```
GET /api/push/vapid-key
```

Returns the VAPID public key for `PushManager.subscribe()`. No authentication required.

**Response:** `{ "publicKey": "<base64url>" }`

Returns 404 if push is not enabled.

### Subscribe

```
POST /api/push/subscribe
Content-Type: application/json

{ "endpoint": "https://...", "keys": { "p256dh": "...", "auth": "..." } }
```

Register a push subscription. Idempotent (same endpoint updates keys).

Question and completion pushes are sent when the desktop window is unfocused or,
on macOS, HID input has been idle for at least 120 seconds. The idle threshold
also covers a focused window left in front when Boss walks away. Other platforms
use focus when HID idle time is unavailable. A committed `progress type=blocked`
report supplies the question text to the same session event and 30-second push
limit as a parsed question. Empty blocked text is rejected by Progress validation.

### Unsubscribe

```
DELETE /api/push/subscribe
Content-Type: application/json

{ "endpoint": "https://..." }
```

Remove a push subscription by endpoint.

### Test Push Delivery

```
POST /api/push/test
Content-Type: application/json

{}
```

Returns `{ "sent": 1, "failed": 0, "stale_removed": 0 }`, where `sent` counts
push-service acceptance, not display on the phone. HTTP 404 means no saved
subscription. HTTP 503 with `sent: 0` means push is disabled or the private
VAPID key is unavailable. A 410 Gone response removes the stale subscription
and increments `stale_removed`; Boss must re-subscribe from the phone PWA.

## ACP (ego)

Every `acp_*` Tauri command has a route here, with the same request field
names, the same response body, and the same error body — an `AcpClientError`
serialized whole (`code`, `message`, `connectionId`, `sessionId`, `operation`,
`retryable`). The HTTP status is a translation of `code`, never a second
opinion about it:

| `code` | Status |
|--------|--------|
| `invalid_input` | 400 |
| `not_found` | 404 |
| `capability_unavailable` | 501 — the agent never advertised it; the caller did nothing wrong |
| `transport_closed`, `stream_gap` | 410 — what the URL names is genuinely gone |
| `unsupported_protocol`, `initialization_failed`, `agent_error` | 502 |
| `protocol_violation` | 502 — the agent denied a method it advertised; the connection is now failed |

```
POST   /acp/connections                                          {root}                    -> AcpConnectionSnapshot
GET    /acp/connections/{connection_id}                                                    -> AcpConnectionSnapshot
DELETE /acp/connections/{connection_id}                                                    -> AcpConnectionSettlement
POST   /acp/connections/{connection_id}/kill                                               -> AcpConnectionSettlement
POST   /acp/connections/{connection_id}/reconnect                {root}                    -> AcpConnectionSnapshot
GET    /acp/connections/{connection_id}/sessions?cwd=&cursor=                              -> ListSessionsResponse
POST   /acp/connections/{connection_id}/sessions                 {authority}               -> AcpAttachmentSnapshot
POST   /acp/connections/{cid}/sessions/{session_id}/load         {authority}               -> AcpAttachmentSnapshot
POST   /acp/connections/{cid}/sessions/{session_id}/resume       {authority}               -> AcpAttachmentSnapshot
POST   /acp/connections/{cid}/sessions/{session_id}/fork         {authority}               -> AcpAttachmentSnapshot
DELETE /acp/connections/{cid}/sessions/{session_id}                                        -> null
POST   /acp/connections/{cid}/sessions/{session_id}/close                                  -> null
POST   /acp/connections/{cid}/sessions/{session_id}/prompt       {prompt:[ContentBlock], viewedRepo?}   -> AcpTurnId
POST   /acp/connections/{cid}/sessions/{session_id}/cancel                                 -> null
DELETE /acp/connections/{cid}/sessions/{session_id}/queue/{turn_id}                       -> null
POST   /acp/connections/{cid}/sessions/{session_id}/config       {configId, value}         -> [SessionConfigOption]
POST   /acp/connections/{cid}/sessions/{session_id}/pause        {requestId}               -> EgoHoldResponse
POST   /acp/connections/{cid}/sessions/{session_id}/resume-turn  {requestId}               -> EgoHoldResponse
POST   /acp/connections/{cid}/sessions/{session_id}/compact      {requestId}               -> EgoCompactResponse
GET    /acp/connections/{connection_id}/interactions                                       -> [AcpPendingInteraction]
POST   /acp/connections/{cid}/permissions/{request_id}/response  {outcome}                 -> AcpInteractionSettlement
POST   /acp/connections/{cid}/elicitations/{request_id}/response {action}                  -> AcpInteractionSettlement
POST   /acp/one-shot                                            {root, prompt}            -> EgoTurn
```

The ACP session prompt route accepts a buffered JSON body large enough for the
shared 10 MiB image draft cap after base64 encoding, plus 64 KiB for JSON and
text. Other JSON routes retain the 2 MiB default body limit.

`POST /acp/connections`, `.../reconnect` and `POST /acp/one-shot` are the three
that launch a process, and they are the three that take the
loopback-or-authenticated guard. The executable is never in the body: it comes
from the `ego_executable` setting.

`prompt` returns an ID immediately. When the session already has a turn, the
host queues the prompt and sends it only after the current ACP response leaves
the session idle; a paused session keeps its queue until resume succeeds. It
never puts two prompts in flight on that session. Each attachment snapshot has
`queuedPrompts: [{turnId, summary}]`, and `promptQueueChanged` replaces that
list on both streams. A `promptSent` event records the user-visible text when
ego actually receives the prompt. Image data stays in the actor until dispatch,
not in queue snapshots or journal events. An accepted prompt that later fails
publishes `turnFailed {message, state}` on the ACP stream with its session and
turn in the frame. Either transport can remove a queued
ID with the DELETE route; `cancel` stops the active turn for every view.

### One-shot (`POST /acp/one-shot`)

One unattended ego turn for a Smart Prompt in `api` mode. It takes no connection
id because it owns the whole lifetime — launch, one turn, shutdown — so it can
neither be handed a connection the AI Chat panel is using nor leave one behind.

The session is opened with **no MCP server**, so ego cannot reach TUICommander's
terminals or repositories from it, and every permission request and elicitation
is refused the moment it arrives: there is no seat for anyone to answer from,
and a question nobody answers is a turn that never ends.

```json
{ "text": "the answer, trimmed", "stopReason": "end_turn", "declined": 0 }
```

`declined` counts the questions refused. It is the only way to tell "ego had
nothing to say" from "ego wanted a tool this mode cannot grant".

Two budgets, both server-side and neither a parameter — a timeout a request body
could choose is a way to pin an ego process for as long as the sender likes. The
launch (`initialize`) gets 60s and the turn itself gets 240s, so the whole call
is bounded by 300s and fits inside the router's 301s `REQUEST_TIMEOUT` with the
same one-second margin the confirm handler keeps. It has to: the outer layer
answers a bare 408, while these two answer a sentence naming what ran out.

The work is not the caller's to cancel. The handler runs the turn on its own
task and awaits the result, so a request that is dropped — by that outer
timeout, or by a browser that navigates away — abandons the answer and not the
turn: ego is still shut down and the connection still unregistered. Before
#804-2ec8 the drop skipped the shutdown, and nothing else would ever have done
it.

### Stream (WebSocket)

```
GET /acp/connections/{connection_id}/stream?after={sequence}     (WebSocket)
```

The browser half of `acp_subscribe`. Frames are `{"kind":"event",...}`,
`{"kind":"gap",...}` or `{"kind":"end"}` — byte-identical to what the desktop
Channel carries. `after` is the first sequence wanted; `0` means everything the
journal still holds. A cursor the journal has dropped is refused **before** the
upgrade, with a 404 and the usual error body, rather than by opening a socket
and closing it.

A gap is terminal: the missing frames exist nowhere in this client, and a
stream that silently resumed past them would render a turn with a hole in it.
Recovery is a fresh connection and `session/load`.

This is deliberately not on `/events`: one turn emits more frames per second
than the 256-entry SSE broadcast can carry without lagging every other
subscriber. `/events` carries only the low-frequency `acp-notice` wake signal
(`ready`, `settled`, `interaction_pending`, `interaction_settled`, `card`), whose
payload names the connection, generation, sequence and — when it has one — the
session and request it is about.

## ego Command Line (`mcp_http/ego_routes.rs`)

The browser half of the dedicated ego configuration commands — what the Settings →
AI Chat page reads and writes. Same field names, same response body, same error
body: an `EgoCliError` serialized whole (`code`, `message`, `command`, `stdout`,
`stderr`, `exitCode`).

```
GET  /ego/providers[?refresh=true]     -> EgoProviders
POST /ego/providers/model              {model} -> EgoProviders
GET  /ego/perimeter                      -> PerimeterView
POST /ego/perimeter/roots                {roots:{rootDir,rootAccess,readAllowlist,writableDirs}} -> PerimeterView
POST /ego/perimeter/network              {enabled} -> PerimeterView
```

**All** routes take the loopback-or-authenticated guard, which is stricter than
`/acp/*`, where only `connect` and `reconnect` do. The difference is deliberate:
every route here runs a process, and the write one changes a configuration file
that decides which model a later run uses. Neither is a read of state TUIC
already holds.

The executable is never in the request. It comes from the `ego_executable`
setting, read per call. Only `model`, `roots` and `network` are writable keys, each through its own
route rather than a `key`/`value` pair, so no body can reach `sandbox` or
`permissions.judge`.

The status is a translation of `code`, never a second opinion about it:

| `code` | Status |
|--------|--------|
| `notConfigured` | 409 — no ego executable is set; nothing was run |
| `invalidInput` | 400 — the model id, root paths or workspace were refused before ego was started |
| `launchFailed` | 424 — a binary is named and could not be started |
| `commandFailed`, `unreadableOutput` | 502 — ego ran and failed, or printed something unreadable |

## Tauri-Only Commands (No HTTP Route)

The following commands are accessible only via the Tauri `invoke()` bridge in the desktop app. They have no HTTP endpoint.

| Command | Module | Description |
|---------|--------|-------------|
| `get_claude_usage_api` | `claude_usage.rs` | Fetch rate-limit usage from Anthropic OAuth API |
| `get_claude_usage_timeline` | `claude_usage.rs` | Get hourly token usage timeline from session transcripts |
| `get_claude_session_stats` | `claude_usage.rs` | Scan session transcripts for aggregated token/session stats |
| `get_claude_project_list` | `claude_usage.rs` | List Claude project slugs with session counts |
| `plugin_watch_path` | `plugin_fs.rs` | Start watching path for changes (change events need AppHandle/WS) |
| `plugin_unwatch` | `plugin_fs.rs` | Stop watching a path |
| `plugin_read_credential` | `plugin_credentials.rs` | Read credential from system store |
| `fetch_plugin_registry` | `registry.rs` | Fetch remote plugin registry index |
| `install_plugin_from_zip` | `plugins.rs` | Install plugin from local ZIP file |
| `install_plugin_from_url` | `plugins.rs` | Install plugin from HTTPS URL |
| `uninstall_plugin` | `plugins.rs` | Remove a plugin and all its files |
| `get_agent_mcp_status` | `agent_mcp.rs` | Check MCP config status for an agent |
| `install_agent_mcp` | `agent_mcp.rs` | Install TUICommander MCP entry in agent config |
| `remove_agent_mcp` | `agent_mcp.rs` | Remove TUICommander MCP entry from agent config |

## Private secret entry

- `GET /secrets/forms/{nonce}` returns only the pending schema, argv and entry
  capability; no stored values. A guessed or expired nonce returns 404.
- `POST /secrets/forms/submit` accepts `{nonce,status,values,template}`. Status
  is `stored`, `approved` or `declined`; values must exactly match requested
  non-SSO fields. Approval accepts no values; decline accepts neither values
  nor a template. A valid submit consumes the nonce and returns names/status.
  Invalid or replayed submissions return 400 without echoing values.

The native private-window identity is the only bootstrap authority. There is no
public endpoint listing forms or issuing their nonces. Open the entry path
shown in that window on your trusted server address; it uses the existing
application origin, authentication and transport. This feature does not enforce
TLS or origin isolation. Responses carry
`Cache-Control: no-store`, `Referrer-Policy: no-referrer` and frame denial.

### Remote peer mail

`GET /mcp/peer?connection_id=<configured-id>&token=<daemon-token>` upgrades to
the desktop-initiated duplex peer-mail WebSocket. A real daemon token is required even
on loopback. One hub is admitted per daemon. JSON frames use `kind:call` with
`id`, `sender`, `arguments` and optional `message_id`, or `kind:reply` with
`id` and `result`. Calls allow register/list_peers/send/inbox/wait; daemon-to-hub
calls allow send/list_peers. Sender host is bound to the authenticated connection.
Frames and outstanding requests are bounded; heartbeat loss closes the link.

`GET /sessions/{id}/output?format=mcp|mcp_raw` returns the native MCP output object,
including `exited`, cursor and truncation fields. It accepts `limit`, `from_line`
and `since_cursor`, plus `from_byte` for raw source-byte pages. Follow
`next_cursor` while `has_more` is true; `continuation` names the next native
request. Tail reads name how to fetch older output. `oldest_offset` and
`missed_count` identify eviction gaps; evicted data cannot be recovered. Explicit
raw pages mask secrets before slicing and keep original-byte cursor positions.
`POST /sessions/{id}/submit` also accepts `timeout_ms`.
These are the configured remote desktop MCP adapters, sharing native backend behavior.

Peer handshakes serialize per configured connection, so a mute daemon cannot hold
mail calls to another host behind its network deadline. Session targets reject empty
ids/prefixes before owner selection. Forwarded notice deduplication survives inbox
reads: it retains fingerprints of the last 100 forwarded ids per sender and registered
recipient, without retaining message bodies. The cache holds at most 65,536
ids globally and 1,024 per remote host. A host quota rejection names the host;
one host cannot consume every other host's replay budget. Full 100-id windows
can still rotate in place, and retained replays still deduplicate at either cap.

There is no time expiry: the sender's outbox lives until acknowledgement.
Only under global or host quota pressure, the cache reclaims all windows of
the least-recently-active sender with no live peer shadow (within the pressured
host when its quota is full). Live sender windows are never reclaimed.
Sender retirement alone preserves dedupe; recipient unregister frees only
that recipient's records and quota. Accepted risk: a departed sender that
reconnects after pressure evicted its history can deliver one duplicate.
Disconnect retires that host's existing shadows synchronously, independently
of a pending handshake or a later reconnect generation. This is a bounded replay horizon,
not unbounded or restart-persistent exactly-once delivery.

Workflow definition and run APIs are actorless and share the same services across transports. Story transition actors track provenance and never restrict actions. Desktop IPC and valid HTTP credentials record Human; sessionless local requests record LocalApi; managed story requests record their session. Missing ConnectInfo on Unix sockets and in-process services means local/unknown caller metadata, not HTTP 500. Local token exchange is accepted; state, revision, project and integration checks remain enforced.

### Stored terminal marker coordinates

OSC 133 event `line` and hook-generated `UserInput.line` are eviction-stable all-time rows, identical to IPC. Scroll-to, line reads and search results keep retained-grid coordinates. Convert stored marker rows using the current grid frame `historyBase`.

### Telegram Settings

`GET /config/telegram` mirrors `telegram_settings`. `PUT /config/telegram` accepts `{ "change": { "action": "..." } }` and mirrors `telegram_setup`, including token replacement/check, one-use pairing, typed chat IDs and enable/target updates. Both routes require local access or the existing authenticated remote session. The read response contains only `token_set`, never the token. Chat IDs are decimal strings.

Workflow run snapshots add `eventContractVersion` and default-empty `graphExecutions`. Graph events use the existing paged history shape. Internal graph transition commands are rejected by public `workflow_run_action`; no autonomous-start action is added in slice A. Pre-contract runs can be inspected and cancelled but cannot resume.

The daemon executor now owns recovery and duration timers under an OS run-database lock. Reads never recover live work, and a non-owner daemon refuses run mutations. Graph resume uses `resume_graph {execution_id,activation_id,resolution}` with an explicit pending activation; status-only resume cannot bypass graph position. Graph start controls, Agent effects and delivery policy remain unavailable until their later slices.

The ACP session fork route accepts optional `atMessageId` beside `authority`. It is forwarded as `_meta.ego.atMessageId` only when ego advertises `sessionCapabilities.fork._meta.ego.atMessage`; unsupported agents are refused.

The host ACP session list gathers all ego pages before ordering ancestry. Each row retains `_meta.ego.lineage` and adds `_meta.tuicommander.lineageDepth`; placeholder rows for deleted parents add `_meta.tuicommander.deleted=true`. The response has no continuation cursor after collecting the pages.

### Memory diagnostics

`GET /diagnostics/memory` performs payload accounting and the native large-malloc-block census on demand. The watchdog logs only cheap allocator counters and structure counts at footprint thresholds; it does not walk payloads or malloc zones.

`accounted_bytes` sums the measured `maps` rows. `malloc_large_blocks` counts all large allocator blocks, including map-owned blocks. Its `may_overlap_accounted_bytes: true` field warns that its bytes must never be added to `accounted_bytes` as attributed memory. Model `holders` remain separate from heap accounting.

## Launch instruction receipts

`GET /sessions/{id}/prompt-receipt` mirrors `get_prompt_receipt`. Missing sessions return 404. The response is `{sections, captureLimited}`. Each section has `label`, `source`, `bytes` (original UTF-8 payload size, or null when unknown), `text` (redacted bounded preview), `status` (`sent`, `queued`, `served`, `file_snapshot`, or `not_observable`) and `truncated`. The endpoint reads live PTY/MCP metadata and never rebuilds text from settings. Capture/retention limits and unobservable cases are documented in [AI Agents](../user-guide/ai-agents.md#inspect-launch-instructions).

### Run incident projection

`POST /workflows/run/action?path=<project>` accepts `{"action":"incidents","run_id":"<id>"}`. The read-only reply is `{"type":"incidents","value":[...]}`. Entries use camelCase fields: `runId`, nullable `attemptId`, `storyId`, `nodeId`, `sessionId`, `taskId`, plus `source`, `cause` and `nextAction`. Sources are `workflow_report`, `attempt_interrupted`, `prompt_delivery_failed`, `session_exit`, `state_change`, `task_record`, or `run_state`. Only evidence explicitly bound to the run is returned. Project ownership is checked before reading live evidence. The action never mutates the run or executes recovery. The desktop `workflow_run_action` returns the same shape.

### Conversation-specific AI Chat launch

`POST /acp/chat/open` accepts `{ "profile": "coordinator", "workspace": "/absolute/workspace", "executable": "/absolute/ego" }`. All fields are optional for a new conversation; omitted values use global defaults. It returns `{ connection, sessionId, launch, replayed }`, where `launch` contains the resolved executable, profile, workspace and durable `peerId`. Use the existing ACP session prompt and stream routes with the returned ids.

Reopen with `{ "sessionId": "<returned-session-id>" }`; saved launch options cannot be replaced on reopen. This route requires localhost or authenticated access, like ACP connect. Each custom conversation owns a dedicated peer/process. Its options are persisted under `ai_chat_launches` without modifying the global settings.

### Caller-bound worktree declaration

An agent that uses `git -C` or tools without changing shell cwd can declare an
existing linked worktree with:

```text
session action=declare_worktree worktree_path=/absolute/path/to/worktree
```

The authenticated MCP binding identifies the caller's live PTY. Omit
`session_id`; a foreign session target is rejected. The backend resolves the
repository from the immutable launch directory and discovers current worktrees
from Git. The main checkout, unknown paths and worktrees owned by another
repository are rejected before any configuration or placement changes. External
worktrees are discovered without recreating or deleting them.

The response and `session-worktree-declared` event use the worktree lifecycle
payload, with `creator_session` naming the caller and `spawn_session=false`.
Only that tab moves. Its real cwd, sibling tabs and an inactive selection stay
unchanged. Retrying the same declaration is idempotent.

The stable `TUIC_SESSION` association is stored in the owning repository's
`declaredWorktrees` map using the existing locked repository delta. Any saved
caller snapshot moves to the target workspace; sibling snapshots are preserved.
After a restart or WebView reconnect, HTTP `GET /sessions` and IPC
`list_active_sessions` return the declared `worktree_path` and `worktree_branch`
while `cwd` remains the actual shell directory. The frontend restores placement
from that worktree path. Declaration does not acquire worktree cleanup ownership.
A concurrent repository edit can return a configuration conflict; retry the
declaration after refreshing rather than overwriting that edit.

The explicit placement remains authoritative over later shell cwd notifications
until another declaration changes it. The frontend retains the backend placement
path separately from the observed cwd during reconciliation.

**Intentional request transport exception:** `declare_worktree` is an MCP-only
action, callable over the existing HTTP `POST /mcp` route with a bound caller.
There is no Tauri command or `COMMAND_TABLE` entry: window IPC has no managed
agent caller binding. Its push event is dual-emitted over Tauri and `/events` SSE,
and session-list response fields are identical over IPC and HTTP. Schema and
serialization regressions cover these shared contracts.

### Terminal title notifications

Raw, text and log `/sessions/{id}/stream` WebSockets carry OSC titles as
`{"type":"title","title":"Claude Code"}` and title resets with `"title":""`.
`GET /events` also carries `event: pty-title` with `{session_id,title}`. These are
presentation events, independent of `activity` pulses and semantic lifecycle.
Grid clients continue to receive binary rendering frames; terminal metadata
consumers use the separate session event subscription.
