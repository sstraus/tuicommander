# MCP & HTTP Server

## Headless GitHub monitoring

Both routers mount one shared GitHub monitoring surface: poller start/stop, visibility, immediate poll, path updates, issue filter and API-debug controls; repository status (`/repo/github`), PR snapshots (`/repo/prs`, `/repo/prs/batch`) and issues (`/repo/issues`). The poller does not require an AppHandle. It publishes through the event bus and `/events` SSE without a window. TCP keeps the existing authentication and request-boundary middleware; the trusted Unix socket uses the local router. GitHub account/auth, mutation and AI routes stay outside this shared surface. A credential-free no-remote smoke proves transport and lifecycle, not authenticated GitHub fetching.


Planner Ideas use the same `StoryAction` envelope as plans and stories: `add_idea {text,source?}`, `list_ideas`, and `update_idea {idea_id,text}`. IPC `story_action_command`, HTTP `POST /stories/action?path=<project>` (`{action:<StoryAction>,sessionId?:string}`), CLI `tuic story`, and MCP `workflow action=story input=<StoryAction>` share the Rust owner. Add/update return `{type:"idea",value:<camelCase Idea>}`; list returns `{type:"ideas",value:[...]}`. The project sets `repoPath`; list also includes global Ideas, while update requires project ownership. Source defaults to the bound session id or `local-api`. Text is trimmed to a nonblank maximum of 16000 characters; source is nonblank with a maximum of 256. Writes use the strict locked `notes.json` owner and publish payload-free `ideas-changed` on desktop and `/events` SSE. UI `save_notes(base,config)` / `PUT /config/notes` applies per-id and per-field intent instead of replacing stale arrays. See [Ideas through the planner](../user-guide/native-stories.md#ideas-through-the-planner).

Native plans have a stored `revision`. `update_plan` takes `plan_id`, `expected_revision`, and `patch` with optional `title` and `source`. At least one field is required. The shared service validates both fields and commits them with one revision increment in one transaction; stale revisions leave the plan unchanged. Existing records start at revision 1. A metadata change changes the workflow planning and verification fingerprints, including plans with no stories. Source prose stays in the external document; the update does not move or rewrite that file. Source-changing edits are refused with a user error while a plan coordinator/planner attempt is running, including an unbound attempt or a paused run. Finish the coordinator or cancel its run first. Title-only and unchanged-source edits remain allowed. The same action is available through MCP, HTTP, desktop IPC, and `tuic story`.

The compact `ui` description includes `design_start`, `design_stop`, `design_status`,
`design_reload`, `design_navigate`, `design_screenshot`, `design_errors` and
`design_eval`. `ui action=help` retains their full usage guidance together with
the panel, notification and confirmation guide. Design actions keep the existing
caller/repository checks, extension host grants and saved `design_mode` denials.

## Scoped plugin settings

The plugin settings IPC command and `POST /api/plugins/{id}/settings` dispatch to the same scoped Rust owner. Catalogue descriptors contain no stored values; secrets read as set/unset. `plugin-changed` carries only plugin IDs over desktop and SSE after a durable commit, scheduling existing reload callbacks. This adapter does not publish the general settings MCP tool.

Slice F exposes `start_graph {target:{type:story|plan,id},expected_revision?,definition_id,definition_revision,request_id,limits?}` through the existing owning-daemon service. Story starts require the current native revision and pin the selected publication; the request ID is bound to its payload. Omitted limits use Rust defaults. Plan dispatch remains unavailable in this build and its start control says so. The `workflow action=run` MCP entry uses an inline schema generated from `RunAction` and the public `RunCommand` variants; it returns the same scoped snapshots and cursor-ordered events as IPC/HTTP. Run history shows pinned activations, decisions/evidence, repair counters, pause targets and complete event payloads across pages. Graph recovery uses `resume_graph {execution_id,activation_id,resolution}` after answering pending input; pause and cancel use the existing sequence-fenced commands. Legacy runs offer inspection and cancellation in the UI. No new persistence or client scheduler is added.

## Explicit repository registration

`repo action=add` requires an absolute local directory path. It resolves the
canonical path, reads real repository metadata, and persists and activates a Git
or shell workspace under the repositories file lock. Repeat calls preserve the
existing repository and terminal state. The backend broadcasts
`repositories-changed` to desktop and SSE consumers after a change, then starts
the repository watcher. A watcher failure returns a warning alongside successful
registration; invalid paths and persistence failures return an error.

The CLI's directory opener calls this same MCP operation over its selected local
IPC endpoint, including named instances and `TUIC_SOCKET`. No OS deep-link handler
or active frontend is required. `ui tab` accepts the native `tuic://open` and
`tuic://edit` content schemes; other `tuic://` actions return an error. Use
`repo action=add` for `tuic://open-repo` intent.

## Browser shell authentication

Unauthenticated HTML navigation to `/` and `/mobile` redirects to the existing `/mobile/login` form when a password is configured. The login handler permits root or mobile app destinations only. API authentication and non-HTML 401 responses are unchanged. The desktop entry checks touch-capable iPad/Mac user agents and selects `/mobile` before mounting the desktop app; native WebViews and standalone secret forms keep their own entry.

## Remote file copies

The shared filesystem router exposes streamed `/fs/upload-copy` on the daemon through existing authentication. Sender-side `fs_transfer_remote_paths` coordination is desktop IPC only and intentionally unmapped: data leaves the machine, Finder source paths cannot be gated to registered roots, and HTTP token holders must not trigger exfiltration. There is no `/fs/transfer-remote` HTTP route. The receiver resolves registered repository roots through `cap-std` directory handles, validates archive paths, rejects links, bounds bytes/entries/concurrency, and publishes the staged top-level source with an atomic no-replace rename. Uploads use the existing session-cookie header, a 30-second chunk idle deadline (exempt from the global response deadline), and hold their concurrency permit through blocking extraction. Daemon startup sweeps abandoned upload staging inside registered roots without following symlink directories. Cleanup restores owner directory access only inside disposable staging when restrictive tar modes would prevent removal; published permissions remain unchanged. This path uses neither SSH nor shell commands. See the filesystem HTTP API for the wire contract.

## Open desktop Markdown tabs

`GET /repo/open-markdown-tabs?cwd=<session-directory>` returns file-tab metadata
for the deepest registered repository or workspace containing the directory.
It uses the existing remote authentication middleware and returns 403 for an
unregistered directory. No file contents are returned.

The desktop publishes through `sync_open_markdown_tabs`; its HTTP twin is
`PUT /config/open-markdown-tabs`, restricted to loopback. Rust stores a temporary
snapshot and emits `open-markdown-tabs-changed` on desktop IPC and SSE whenever
it changes. Mobile clients refetch after invalidation or reconnect. Saved pane
layouts contain only tab IDs/types and cannot supply these file paths.

## CI logs

Use HTTP `GET /repo/ci-failure-logs` with `repoPath` and `branch` to fetch logs
for the local branch head. For a remote-only PR or a particular CircleCI check,
use the same route with `checkUrl` and `headSha`, or the
`fetch_ci_failure_logs` Tauri command with `check_url` and `head_sha`. Both
verify the selected check against that PR head.

## Native stories

Native stories expose `qa` for work that waits on manual checks. A criterion starts with `[MANUAL]`, or is marked through `set_criterion_manual {index,manual}`. Active work automatically enters QA when all unchecked criteria are manual; `submit_qa` explicitly requests that state. `reject_qa` returns QA to In progress; `approve` requires every criterion checked. QA keeps plans Active and dependencies blocked. All transitions retain expected-revision checks. See the [native stories guide](../user-guide/native-stories.md#manual-qa).

### Reversible native record archival

All native story transports share `archive_story`, `unarchive_story`, `archive_plan`, and `unarchive_plan`. Story actions take `story_id`; plan actions take `plan_id`. Each requires `expected_revision` and returns the updated record with its incremented `revision` and `archived` flag. Stale revisions fail without a write. Archival preserves status, criteria, dependencies, and child archive flags; it never deletes records or cascades to children.

`list_plans` and `list_stories` default to `archived: false`. Set `archived: true` to select archived records. The story filter combines with the other filters and is bound into pagination cursors. `filtered_plan_view` also accepts camel-case `filter.archived` (default false), which selects archived list ids and board columns without changing the active-plan state calculation. Direct reads remain available. Default plan views and workflow dispatch omit archived stories; archived plans and stories cannot start manual or workflow work. Restore the parent plan before starting its stories.

Both archive and restore refuse claimed stories and plans that contain a claimed story. An active or paused plan-root workflow owns the plan's story set. A story-root workflow owns its target and execution stories, not unrelated siblings. Cleanup of an owned story or its parent plan is refused until the run ends. The workflow writer lock remains held through the archive transaction, so a concurrent run cannot start between the ownership check and the write.

`story_archive_history` (`story_id`) and `plan_archive_history` (`plan_id`) return `{type:"archive_history",value:[{revision,archived,actor}]}` in revision order. Events commit atomically with the record and retain caller provenance. Existing status transition history remains separate. Consecutive archive-only revisions preserve accepted-work integration receipts; a work change or a moved canonical Git ref still invalidates them. Example: `tuic story '{"action":"archive_story","story_id":"<id>","expected_revision":1}' --project /absolute/project`.


The `workflow action=story` MCP entry accepts `{ action: "story", input: StoryAction }`. It resolves the owning project and live PTY from the bound MCP caller; caller-supplied project and session IDs are ignored. The shared Rust story service enforces revision and project checks. `list_plan_sources` discovers Markdown plans in the project's `plans/` and `.claude/plans/` roots; `add_plan_source` derives a title from the selected document and reuses an existing record for that source. The dialog calls the same actions, so agents and users see one discovery result. Any trusted caller may remove a direct WontFix prerequisite from a Backlog story with its current revision. The `plan_view` action returns Rust-derived story abandonment and cancellation summary without storing those projections. The HTTP route `/stories/action` and desktop command `story_action_command` use the same service. `tuic story '<JSON action>' [--project PATH] [--session-id PTY]` provides a local CLI path. The first native slice does not include story import/export.

`GET /stories/capabilities` mirrors the desktop `story_capabilities` probe. It returns JSON `true` without opening the story database; the dialog uses it to distinguish an outdated backend from an action error.

HTTP's access marker does not prove human identity, because loopback requests also receive it; an HTTP action without `sessionId` records `local_api` provenance, including for approval. A managed session that claimed a story cannot approve that story; an independent reviewer session can approve after checking its criteria. Desktop and browser clients can approve through the shared story service.

Workflow definitions use the shared `/workflows/definition/action` HTTP route and `workflow_definition_action` desktop command. They are project-scoped and published by immutable revision; execution has its own API.

The grouped `workflow` MCP tool exposes five generated discriminated actions: `story`, `run`, `proposal_create`, `report` and `launch`. Pass the existing service action as `input` for `story` (`StoryAction`) and `run` (`RunAction`). The other three actions use their generated typed service inputs. The advertised schema is the pre-dispatch contract: missing, mistyped, cross-action and unknown fields are rejected before effects. The dispatcher reuses the existing handlers and their project, revision, ownership and local-launch checks; there is no new persistence owner.

The catalogue cutover removes the old `story` and `workflow_*` tool names without aliases. Saved member denials migrate to action capabilities and still hide and deny those grouped actions. Disabling `workflow` hides and denies the grouped entry, not the separate `automations` scheduler. External policy rules need review: the `workflow` name covers all five actions.

`workflow action=launch` takes `{input:{runId,attemptId,worktreePath,agentType,skills?,feedback?}}` from a bound local managed PTY. The worktree must be a registered branch checkout of the project, outside the archive pool, and cannot be the main checkout or have a Git operation in progress. Launch validation is separate from detached-orphan removal, which still refuses attached branch checkouts. Plan attempts use the caller's isolated worktree; story attempts require that run's active coordinator and bind a distinct registered worktree to the story before worker spawn. It renders the pinned prompt in Rust, reserves the spawn effect, launches through the existing managed-agent path, and binds the returned session and task handle to the durable attempt. A story worker requires the caller to own the run's active coordinator attempt. `workflow action=report` takes `{input:AttemptReport}` with `contractVersion`, `runId`, `storyId`, `storyRevision`, `attemptId`, `generation`, `outcome`, `summary`, `criterionResults`, and `evidence`. The server derives the reporting PTY and project from the MCP connection, validates attempt ownership and revision, and records an idempotent run event. Plan-agent reports use `storyId=planId`, `storyRevision=0`, and no criterion results. Generic `agent send` and process exit are observations, never semantic completion reports.

For `workflow action=report`, a `needs_input` outcome requires `inputRequest {question,options}`. A completed reviewer report requires `review {decision,artifactDigest,findings}` with criterion-indexed findings. These fields are durable evidence; they do not promote a story to Done. After a committed story report, the active coordinator's inbox receives a `workflow_event` cursor with `runId`, `storyId` and `sequence`; the coordinator replays the run log rather than treating mail as an outcome.

`workflow action=proposal_create` takes `{input:{runId,proposalKey,story}}` from the active bound plan coordinator. The story belongs to the run's plan and uses a `plan_step` origin. A proposal key is durable across retries: the same payload returns the same story, while a changed payload is rejected. The run records a bounded CreateStory effect before the story transaction and its outcome afterward.

`list_stories` shares optional top-level `state`, `priority`, `story_type`, `limit` (1–100), and `cursor` across all transports. Without a limit it returns every matching story; limited responses retain the `value` array and add `next_cursor` only when more matches remain. Cursors require a limit and the same plan/filters. See the HTTP API for bounds and live-read semantics.

`filtered_plan_view` uses the same service on IPC, HTTP, MCP, and CLI. It takes `plan_id` and `filter` (camel-case `visibility`, `storyType`, `untypedOnly`), and returns the full plan with filtered list ids and per-status board ids. Optional `storyType` on create input and persisted stories is feature, fix, refactor, chore, or task; omitted types stay untyped. See the HTTP API for defaults and combined-filter rules.

`update_story` takes `story_id`, `expected_revision`, and a camel-case `patch`. Omitted fields stay unchanged. The patch accepts `title`, `priority` (1–3), `fileScope`, `origin` (including a plan step), nullable `storyType` (`feature`, `fix`, `refactor`, `chore`, `task`), optional `problem` and `risk` (`low`, `medium`, `high`), ordered `criteria`, and the final `dependencies` set. Null clears `storyType`, `problem` or `risk`; old story records default both to absent. Unknown fields, invalid values, cross-plan dependencies, duplicates, cycles and stale revisions are rejected without partial story writes. Only unclaimed Backlog/Ready stories without active or paused workflow ownership can be amended. Unique byte-identical criteria retain their checks after reorder; changed, new or ambiguous duplicate criteria become unchecked. A priority-only patch preserves all checks. Dependency changes recompute readiness. Planning and execution fingerprints include the amended requirements/revision, so existing evidence cannot certify changed work.

## Project Progress reporting

The compact `progress` native tool is available directly in classic and
every collapsed discovery (Grok, ego), and through the meta-tool index, unless disabled by
`mcp_capabilities.denied` or by the global `progress_tracking` flag. It takes
`{ type, text, step? }` with `type` restricted to `done` or `blocked`, derives
the project and the reporting agent from the caller, appends to the shared
journal, and then dual-emits `progress-recorded`. A per-agent
`progress_tracking: false` answers `progress_tracking_disabled` instead of
hiding the tool — the tool index is global and has no session context.

The journal's third kind, `intent`, is written by TUIC from the agent's
`intent:` marker in `pty.rs`. An agent that reports one is refused: the point of
the kind is that it is observed rather than claimed.

The `repo` tool exposes exactly one progress action, `progress_list`. Reading
back is occasionally useful to an agent; pausing, clearing, correcting and
exporting are the reader's business and cost instruction budget in every
`initialize`.

`repo action=progress_list` takes `path` and optional `input` with `blockedOnly`,
`ptyId`, `limit` and `cursor`. The default page has 8 entries; `limit` is
clamped to 1–100. Entries are newest first. The response includes `total`
(matching entries before the cursor) and `nextCursor` (`null` on the final
page). Pass `nextCursor` as the next request's `cursor` to read the entire
journal without overlap. Filters apply before paging. Desktop `progress_list`
and HTTP `POST /progress/list?path=…` use the same input and response.

**Module:** `src-tauri/src/mcp_http/mod.rs`

Optional HTTP/WebSocket server that exposes all Tauri commands as REST endpoints. Enables browser-mode operation and MCP (Model Context Protocol) integration for external AI tools.

## Installed bridge lifetime

The primary instance installs the adjacent `tuic-bridge` into
`<config_dir>/mcp-bridge/<sha256>/tuic-bridge` (`.exe` on Windows) before updating
agent MCP configs, only when an eligible integration needs repair. Publication
uses a temporary file in the destination directory and atomic rename, with
executable permissions set before publication. The source is hashed before copying;
both the copy and a second source read must match before publication. Identical
bytes reuse the existing file. Different builds get separate revision directories;
old revisions remain available to running agents and saved config commands.
Cargo target cleanup, mbx view refreshes and app upgrades therefore cannot remove
the configured executable. Failed installation leaves agent configs unchanged.
Startup migrates TUIC bridge commands from cargo/mbx targets and its installed
revision directory, while preserving custom working commands and transports.
Claude migration covers `~/.claude.json`, an existing
`~/.claude-private/.claude.json`, inherited `CLAUDE_CONFIG_DIR`, and the Claude
agent's environment flags and saved run-config environment roots. Override roots
use `<CLAUDE_CONFIG_DIR>/.claude.json`; canonical paths are deduplicated. Relative
overrides cannot be resolved before a launch working directory is known and emit
a warning. Disabled integrations and custom transports remain unchanged. A missing
or non-executable configured absolute bridge command emits a warning naming the
config and command even when no adjacent bridge is available to repair it.
Secondary-instance ownership rules still apply. An explicit Settings > Agents
Install can install under user authority. Manual setup snippets only inspect
installed copies and never create them; without a copy they report the bare
`tuic-bridge` command. Revision cleanup is deferred until every agent config root,
including profiles used only by external shell launchers, can be discovered.

## Activation

The server has two independent listeners:

- **IPC listener** (always started): On macOS/Linux, listens at `<config_dir>/mcp.sock` for the default instance. Named instances use a deterministic short socket name in the OS temp directory because macOS limits Unix socket paths to 104 bytes. On Windows, listens on `\\.\pipe\tuicommander-mcp` (named pipe). No authentication — used by the local `tuic-bridge` sidecar.
- **TCP listener** (opt-in): Only starts when remote access is enabled. Binds to `0.0.0.0:<port>` (port from `services.server`) with Basic Auth.

The shared `tuic-ipc` crate owns `AppInstance`, endpoint naming and HTTP
framing. Informational responses (1xx except 101) are skipped until the final
response arrives. More than 32 interim replies per response, or a status/header
section above 64 KiB including its terminator, returns an invalid-data error.
These limits persist across reads; response bodies are not subject to the header
limit. Status 101, 204 and 304 responses finish at the header
terminator regardless of Content-Length or Transfer-Encoding (RFC 9112 §6.3).
The clients do not issue HEAD requests.
The server reuses its short named socket path; CLI and bridge select it
with `--instance <id>` or `TUIC_APP_INSTANCE`. An explicit Unix `TUIC_SOCKET`
wins. Bridge fallback discovery stays within the selected instance's socket
prefix, so an unavailable named instance cannot fall through to the default
instance. Sync CLI and async bridge adapters retain their separate I/O and timeout
policies. Both finish length-delimited responses without waiting for EOF and
decode chunked bodies before converting UTF-8.

The `mcp_server_enabled` config flag controls whether the `/mcp` protocol route is active (MCP tool discovery and invocation), not whether the server itself starts. The HTTP API endpoints (sessions, git, config, etc.) are always available on the IPC listener.

The daemon's `POST /remote/update` handler requires the current session token
in a `tui-session` cookie. Basic Auth alone does not authorize executable
replacement. `/fs/upload-copy` also authenticates by cookie (or Basic Auth)
through the shared middleware. A `?token=` query is ignored on both routes, so
a query-only request answers 401. QR pairing and WebSocket/SSE query
authentication are unchanged.

Release boundary (step 2 of the two-release upload-token migration): the release
before this one accepts cookie or query on these routes (step 1); this release
removes the query form and sends the cookie from the update client. Clients of
the earlier release that still send `?token=` get 401 and must be updated.

The local IPC listener is independent from the **Remote Access** TCP toggle. Turning remote access on or off only starts or stops the authenticated TCP listener; it does not disable the MCP socket or the local MCP route. Lifecycle logs state whether a transition affects TCP or the always-on IPC listener.

For isolated desktop verification, `TUIC_PORT=<port>` overrides the configured TCP
port for the process without persisting the value. The normal startup retry still
tries the next two ports when the selected port is occupied.

Configuration via Settings > MCP and Settings > Remote Access, or `config.json`:

```json
{
  "mcp_server_enabled": true,
  "mcp_port": 9876
}
```

On startup, the server:
1. Binds the IPC listener: Unix socket at the instance-specific path described above (macOS/Linux) or named pipe `\\.\pipe\tuicommander-mcp` (Windows)
2. If remote access is enabled, binds a TCP listener on the configured port
3. Starts Axum HTTP server on a background tokio thread
4. Enables CORS for browser mode
5. Spawns MCP session reaper (evicts stale sessions after 1h TTL; see [Identity outlives its transport](#identity-outlives-its-transport))
6. Spawns upstream health checker for proxied MCP servers

## Unix Socket Lifecycle (macOS/Linux)

The Unix socket is managed with two safety layers to survive crashes and rapid restarts:

| Layer | Mechanism | Purpose |
|-------|-----------|---------|
| **Retry bind** | 3 attempts × 100 ms, each removes stale file before trying | A crashed previous run leaves a dead socket file that blocks `bind(2)` — retrying clears it |
| **Real liveness check** | `UnixStream::connect()` in `get_mcp_status` | `file.exists()` returns `true` for stale sockets; only a real connect reveals whether the server is alive |

**Why this matters for AI tool integrations:** The `tuic-bridge` sidecar connects via the Unix socket to expose TUICommander tools to Claude Code. If the socket is stale (app crashed, Tauri force-quit), the bridge cannot connect and returns `tools: []`, silently disabling all MCP tools in the agent session. The retry bind ensures the socket is always valid on restart; the real liveness check ensures the UI accurately reports the server state.

## Server Limits

Both `build_router` and `build_remote_router` pass their assembled routes through
`with_server_limits` (`mcp_http/mod.rs`), which applies two bounds:

| Limit | Value | Response | Why |
|-------|-------|----------|-----|
| Response timeout middleware | `REQUEST_TIMEOUT` = 301 s | `408 Request Timeout` | A wedged handler otherwise holds its connection forever. 301 s includes warming a linked worktree with large ignored build artifacts and remains far below "never" |
| `DefaultBodyLimit` | `MAX_BODY_BYTES` = 2 MB, with route-scoped ACP prompt and voice import exceptions | `413 Payload Too Large` | Bounds buffered JSON request bodies |

**301 s, not 300 s — the layer must outlast every deadline it wraps.**
`ui action=confirm`'s own answer window (`CONFIRM_TIMEOUT`, `mcp_transport.rs`)
used to be 300 s too, an exact tie this outer `TimeoutLayer` could win: it drops
the handler's future on expiry, so a win here meant a bare 408 instead of the
handler's own `{confirmed:false, reason:"no answer within 300s"}` body, and the
dropped future skipped the handler's own cleanup — `confirm_responses` leaked
the entry and no `McpConfirmResolved` event told any client to drop the dialog
(#760-c29f). `REQUEST_TIMEOUT` is now a deliberate 1 s above `CONFIRM_TIMEOUT`,
which stays itself 4 s under the local bridge's 305 s wait for a confirm or a
long workspace op (see below) — three layers, each strictly larger than the
one it wraps, pinned by
`mcp_transport::tests::request_timeout_beats_every_in_handler_deadline` and
`server_limits`'s `confirm_left_unanswered_resolves_clean_through_the_real_server_stack`.

**The timeout does not apply to streaming.** `tower_http`'s `ResponseFuture` races
its sleep only against the future that produces the `Response`; once headers are
returned the timeout is dropped and the body streams unwatched. `Sse` and
`WebSocketUpgrade` both return immediately, so neither `/events` nor a PTY socket
can be cut off mid-stream. `server_limits_do_not_cut_off_a_long_lived_stream`
pins this — if anyone swaps in a layer that wraps the response body, it fails.

`MAX_BODY_BYTES` is deliberately equal to axum's own `DefaultBodyLimit` default,
so stating it explicitly changes no behaviour. The value is that the bound is now
asserted: a `DefaultBodyLimit::disable()` added outside this layer, or an axum
release that drifts its default upward, fails a test instead of silently
uncapping the server.

## REST API Endpoints

### Session Management

HTTP and MCP agent readers resolve their inherited `TUIC_SESSION` from the live
PTY identity binding when no explicit reader identity is supplied. Generic HTTP
shell readers use the same boundary. This lets native Codex hooks bind their exact
Chat transcript without relaxing foreign-owner, live-PID, rollout-id or cwd checks.
HTTP/IPC and MCP session lists copy PTY metadata and release the PTY mutex and
map guard before validating native bindings. Validation probes the live PTY again;
keeping the row lock during that probe would deadlock a bound Codex terminal.


| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/sessions` | List active PTY sessions |
| `POST` | `/sessions` | Create new PTY session |
| `POST` | `/sessions/:id/write` | Write data to session |
| `POST` | `/sessions/:id/resize` | Resize session terminal |
| `GET` | `/sessions/:id/output` | Read session output (ring buffer) |
| `GET` | `/sessions/:id/shell-family` | Shell classification (`posix`/`windows-native`/`unknown`, or `null`) so the client picks the right control sequences |
| `POST` | `/sessions/:id/pause` | Pause session output |
| `POST` | `/sessions/:id/resume` | Resume session output |
| `DELETE` | `/sessions/:id` | Close session |

### Design Mode

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/design-mode/start` | Start/rebind the dedicated Edge/Chrome profile (`{sessionId, browser?}`) |
| `POST` | `/design-mode/stop` | Stop inspection for a repository (`{repoPath}`) |
| `POST` | `/design-mode/reload` | Ask the armed browser to reload the reviewed tab once (`{repoPath}`) |
| `POST` | `/design-mode/navigate` | Ask the armed browser to open a localhost URL in the reviewed tab once (`{repoPath, url}`) |
| `GET` | `/design-mode` | Read the array of per-repository Design Mode statuses |
| `POST` | `/design-mode/extension` | Versioned native-host pick, send, discovery and configured server-start operations |

The chosen browser opens on the host running TUICommander, including when a
remote browser initiates the request. `design-mode-changed` is dual-emitted to
the desktop window and `/events` SSE with `{repo_path, session_id, status}`;
`status` is `armed` or `stopped`. See the [HTTP API](../api/http-api.md#design-mode-endpoints)
for the request bodies and the [user guide](../user-guide/design-mode.md) for
the workflow.

### Monitoring

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Health check, including running build version, target and SHA-256 |
| `GET` | `/stats` | Orchestrator stats (active/max/available) |
| `GET` | `/metrics` | Session metrics (spawned, failed, bytes) |
| `GET` | `/process/stats` | CPU% and RSS memory for TUIC and all child process trees |
| `GET` | `/process/monitor` | Self-contained HTML dashboard for process metrics (for remote/PWA/mobile) |

The `/agents/map` routes and their HTML page were removed on 2026-09-23. The
Progress Flow view (`POST /progress/flow`, `POST /progress/flow/detail`)
replaced them and reuses how they read subagents:

**Subagents are read from their own transcripts.** Claude records an `Agent`
spawn in the parent transcript but writes the subagent's turns to
`<config>/projects/<cwd-slug>/<uuid>/subagents/agent-<id>.jsonl` — measured over
582 real transcripts: 833 spawns and zero `isSidechain:true` rows in any parent
file. The parent transcript supplies the `Agent` call's timestamp and prompt; a
subagent whose join fails still renders from its own file, with its own first
message standing in for the prompt. The teammate join compares the prompt with
the *decoded* first message, because the raw JSON line escapes every newline.
The report is the subagent's last reply; tool results are never kept.

`/progress/flow/detail` takes a PTY id looked up in `AppState` and an agent id
compared against the subagents listed on disk — **never a path**. Every path
component comes from the session's cwd, the agent process's
`CLAUDE_CONFIG_DIR` and the session uuid Claude published. Only sessions TUIC
has identified as `claude` are read: Claude discovery falls back to "newest
unclaimed session file under the project dir", so asking it about a shell tab
would hand that session another tab's transcript (issue #119).

Both files are read through per-path byte cursors held in
`AppState.subagent_map_cache`, so a refresh parses only what was appended — the
biggest parent transcript on the development machine is 31 MB. Tool activity
is a count (at most 32 distinct names per subagent, the rest counted as
`other`).

### Git Operations

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/repo/info?path=` | Get repository info |
| `GET` | `/repo/diff?path=` | Get git diff |
| `GET` | `/repo/diff-stats?path=` | Get diff stats |
| `GET` | `/repo/changed-files?path=` | List changed files |
| `GET` | `/repo/branches?path=` | List git branches |
| `GET` | `/repo/github-status?path=` | Get GitHub status |
| `GET` | `/repo/pr-statuses?path=` | Get batch PR statuses |
| `GET` | `/repo/ci-checks?path=` | Get CI check details |
| `POST` | `/repo/create-pr` | Create a PR (gh wrapper, UI-gated) |
| `POST` | `/repo/create-issue` | Create an issue (gh wrapper, UI-gated) |
| `POST` | `/repo/post-pr-review` | Post a PR review with inline comments |
| `GET` | `/repo/merged-prs?path=&sinceTag=` | Merged PRs via GraphQL |
| `POST` | `/repo/conflict-assist` | Worktree + rebase; reports verified/unverified clean or conflicts, base source, warning, and agent prompt |

### Configuration

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/config` | Get app config |
| `PUT` | `/config` | Save app config |
| `POST` | `/auth/hash-password` | Hash password for remote access |

`GET /config` redact remote-access secrets:
`services.auth.password_hash`, `services.auth.session_token`,
`services.relay.token`, and `services.push.vapid_private_key`. The config shape
exposes only the corresponding `*_exists` booleans for secret presence.

`PUT /config` require `{ base, config }`, where
`base` is the caller's loaded snapshot and `config` is its edited document.
The backend applies only the base-to-config changes to the latest locked file;
unchanged fields keep concurrent edits. Both also share `server_settings_changed` with
the IPC `save_config` and rebind the listener through
`restart_after_server_settings_change`, so no transport can leave the process
serving a configuration the disk disagrees with. See
[`config.md`](config.md#application-config-configjson) for the merge semantics.

### Agents

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/agents/detect` | Detect installed agents and IDEs |
| `POST` | `/agents/detect-all` | Batch-detect the named binaries in parallel, path only (no version lookup) |
| `POST` | `/agents/open-in-app` | Open a path in an IDE, terminal or file manager (optional `line`/`col`) |

### Plugins

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/plugins/docs` | Plugin development guide (AI-optimized reference) |
| `GET` | `/api/plugins/:plugin_id/data/*path` | Read plugin data file (JSON or plain text) |
| `POST` | `/api/plugins/output-watchers` | Replace the OutputWatcher set of one frontend (`client_id` + `seq` + patterns) — what the PTY reader matches lines against. Returns `{applied, rejected}`; a rejected id keeps matching in the WebView. Not `:plugin_id`-scoped — one frontend owns one set for all its plugins |

### Worktrees

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/worktrees` | Create worktree |
| `DELETE` | `/worktrees` | Remove worktree |
| `GET` | `/worktrees/paths?path=` | Get linked-worktree paths for a repo, keyed by workspace id and carrying `kind` |
| `GET` | `/worktrees/lifecycle?repoPath=&workspaceId=` | Fresh dirty/commit/removal-safety verdict for one exact workspace; unknown fails closed |
| `POST` | `/worktrees/run-script` | Run a setup script in a directory; returns exit code and captured output |

### Dictation and Desktop Integration

Desktop-only — audio capture, the whisper model, the relay client and the updater
are all gated on the `desktop` feature, so `build_remote_router` serves none of
these. Handlers live in `mcp_http/dictation_routes.rs` and
`mcp_http/system_routes.rs`; each answers exactly what its Tauri twin resolves to,
so `src/stores/dictation.ts` and `src/notifications.ts` work unchanged on both
transports. Full request/response shapes: `docs/api/http-api.md`.

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/dictation/status` | Model + recording status |
| `GET` | `/dictation/models` | Whisper models with download state |
| `POST` | `/dictation/models/download` | Download a whisper model |
| `POST` | `/dictation/models/delete` | Delete a downloaded model |
| `GET` | `/dictation/speech/assets` | Speech languages and the ONNX runtime, with install state |
| `POST` | `/dictation/speech/assets/download` | Download and verify an asset from the pinned catalogue |
| `POST` | `/dictation/speech/assets/cancel` | Abandon a download in flight |
| `POST` | `/dictation/speech/assets/delete` | Unload the engine and remove the files |
| `POST` | `/dictation/start` | Start recording |
| `POST` | `/dictation/stop` | Stop recording and transcribe |
| `GET`/`PUT` | `/dictation/corrections` | Read/replace the text-correction map |
| `GET` | `/dictation/devices` | List audio input devices |
| `POST` | `/dictation/inject` | Apply corrections and inject text into the active terminal |
| `GET`/`PUT` | `/dictation/config` | Read/save the dictation config |
| `GET` | `/system/relay-status` | Cloud relay connection status |
| `POST` | `/system/notification-sound` | Play a notification sound on a chosen output device |
| `GET` | `/system/check-update?channel=` | Check a beta/nightly channel for updates (hardcoded URLs) |

### ACP (ego)

Shared routes, not desktop-only: driving ego from a phone is the point of the
client. Full request/response shapes in `docs/api/http-api.md`.

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/acp/workspace` | The AI Chat workspace root (`acp_workspace_root`) |
| `POST` | `/acp/connections` | Launch the configured ego and initialize |
| `GET` `DELETE` | `/acp/connections/:id` | Snapshot / disconnect |
| `POST` | `/acp/connections/:id/kill` | Kill the child |
| `POST` | `/acp/connections/:id/reconnect` | Settle and start a new generation |
| `GET` | `/acp/connections/:id/stream?after=` | WebSocket: the connection's ordered frames |
| `GET` `POST` | `/acp/connections/:id/sessions` | List / create |
| `POST` | `/acp/connections/:cid/sessions/:sid/{load,resume,fork,close}` | Attach / detach |
| `DELETE` | `/acp/connections/:cid/sessions/:sid` | Delete for good |
| `POST` | `/acp/connections/:cid/sessions/:sid/{prompt,cancel,config}` | Run a turn, stop it, set an option |
| `POST` | `/acp/connections/:cid/sessions/:sid/{pause,resume-turn,compact}` | ego's own extensions |
| `GET` | `/acp/connections/:id/interactions` | Questions waiting on a person |
| `POST` | `/acp/connections/:cid/{permissions,elicitations}/:rid/response` | Answer one |

Only the two routes that launch a process — `POST /acp/connections` and
`.../reconnect` — take the loopback-or-authenticated guard. The rest need a
connection id one of those two handed out. The executable is never in a request
body; it is the `ego_executable` setting, read per call.

## Streaming

### WebSocket (`/sessions/:id/stream`)

Real-time PTY output streaming per session. Connects to the session's broadcast channel.

```
Client ──WebSocket──> /sessions/{session_id}/stream
                      Server pushes PTY output as text frames
```

### Session Lifecycle Events

When sessions are created or closed (via HTTP, MCP, or PTY exit), the server broadcasts events through the SSE event bus:

- **`session-created`** — Emitted when a new PTY session is created (both local and MCP-spawned). Carries `session_id`, `cwd`, `agent_type`, the optional stable `display_name`, and `parent_session` (the spawning agent's `$TUIC_SESSION`, present only for `agent action=spawn` by a caller with a registered identity — a `pending-mcp:` placeholder is never published; the UI marks such tabs with a shared robot icon). Frontend uses this to auto-add remote tabs; a spawn-assigned name is stored as a custom name (`display_name_is_custom`), like `session rename is_custom=true`: neither an `intent:` title nor an agent's OSC 0/2 title replaces it, only a user rename does, while session-list snapshots carry independent `display_name_is_custom`, `display_name_from_spawn` and `is_remote` flags, plus `parent_session` and the live `tuic_session` bound to each PTY, for reconnect and parent-name resolution.
- **`term-alias-assigned`** — Emitted when a session receives its human-friendly alias. Carries `session_id` and `alias`. Published after `session-created` on the bus, and also as a desktop window event. Frontend uses this to update tab tooltips; it fires once, so after a reload the alias comes from the session list (`alias` on `GET /sessions` / `list_active_sessions`).
- **`session-renamed`** — Emitted when MCP `session action=rename` changes a tab's display name. Carries `session_id`, `name` and `is_custom`. The desktop and browser UIs update the tab bar and sidebar from it. The name must be one line of at most 256 characters without control characters.
- **`session-suspend-requested`** — Emitted when MCP `session action=suspend` passed the busy check. Carries `session_id` and `request_id`. The UI ends that tab's PTY and keeps the tab suspended.
- **`session-closed`** — Emitted when a session exits. Carries `session_id`. Frontend uses this for cleanup.

These events are available on the SSE `/events` stream used by the mobile PWA and any connected WebSocket clients.

`workflow-run-changed` is a low-frequency wake hint `{repo_path,payload:{runId,sequence}}` emitted on desktop and `/events` after a workflow run mutation. Consumers page `/workflows/run/action` with `events` from their last durable sequence, including after reconnect or an SSE lag notification. The hint does not carry the full run state.

### ACP stream (`/acp/connections/:id/stream`)

The browser half of the desktop `acp_subscribe` Channel, carrying identical
frames. `?after=` resumes from a sequence; a cursor the journal has dropped is
refused with 404 before the upgrade rather than by opening and closing a
socket. A `gap` frame is terminal — recovery is a fresh connection and
`session/load`, because the missing frames exist nowhere in this client.

Turn frames never ride `/events`. What does is one low-frequency
**`acp-notice`** per connection — `ready`, `settled`, `interaction_pending`,
`interaction_settled`, `card` — naming the connection, generation and sequence, plus
the session and request id when it has them. It is a wake signal: react to it
by reading the snapshot, the interactions list, or the stream from the sequence
it names.

### Streamable HTTP (`POST /mcp`)

MCP Streamable HTTP transport, serving **two lifecycles on one endpoint**:

```
Client ──POST──> /mcp   (JSON-RPC request, response in body)
Client ──GET───> /mcp   (SSE stream for server notifications, requires Mcp-Session-Id header)
Client ──DELETE─> /mcp  (end session, pass Mcp-Session-Id header)
```

**Legacy (2025-11-25) — `initialize`.** It negotiates the revision: it echoes
`params.protocolVersion` when that value is one of `2026-07-28`, `2025-11-25`
or `2025-03-26`, and answers `2025-11-25` for anything else or for a request
that names no version. A missing `MCP-Protocol-Version` request header remains
accepted for older clients; `tuic-bridge` supplies the version carried by each
proxied stdio request and falls back to `2025-11-25` when the request carries
none.

**Modern (2026-07-28, SEP-2549) — `server/discover`.** One stateless request,
no `initialize`, no `mcp-session-id` minted or required. The result carries
`resultType: "complete"`, the full `supportedVersions` list, the same
`capabilities` the legacy handshake declares (`tools.listChanged` plus
`experimental.claude/channel`), the server instructions, `ttlMs: 0` and
`cacheScope: "private"`, with `_meta."io.modelcontextprotocol/serverInfo"`
naming the server. `ego` is the client this exists for: it pins the revision as
a const, sends only `server/discover`, and treats a JSON-RPC error as a hard
failure — so the previous method-not-found meant zero TUIC tools reached it and
`session/new` was refused.

A stateless client has no handshake to record its identity in, so it sends it on
**every request** in `params._meta."io.modelcontextprotocol/clientInfo"`.
`tools/list` reads that name when there is no protocol session to read it from,
which is what keeps collapsing and lazy discovery working on the new lifecycle.

**On 2026-07-28 a list result is a cache entry, not a bare array.** `tools/list`
adds `resultType: "complete"`, `ttlMs: 0` and `cacheScope: "private"` — the same
three fields `server/discover` carries, and for the same reason: the tool surface
moves with upstream connects and config toggles, so none of it is cacheable. The
revision is re-read per request (`MCP-Protocol-Version` header, else the
`params._meta."io.modelcontextprotocol/protocolVersion"` key) because there is no
session to remember it in, and the fields are **withheld** from a legacy client,
which has neither the fields nor a reader for them.

Completed `tools/call` responses, including tool-level errors, carry
`resultType: "complete"` on 2026-07-28 requests. They do not carry `ttlMs` or
`cacheScope`: tool calls are not cache entries. The legacy result shape remains
`content` plus `isError`; a proxied upstream result keeps its other fields.
When the stdio bridge is temporarily disconnected, its local `tools/call`
error carries the same discriminator. Its offline `tools/list` result carries
the complete, private, zero-TTL cache envelope for current requests. Legacy
offline responses keep their previous shape.

Not schema tidiness: ego enforces all three before it admits a server. Measured
2026-09-19 against real ego 0.1.0 (#783-3c1b) — `server/discover` answered,
`tools/list` answered with the whole catalogue, and `session/new` still failed
with `the supplied MCP servers could not be admitted`, because the result carried
`tools` and nothing else. Injecting the three fields in a proxy, changing nothing
else, opened the session.

`ego` therefore gets the **collapsed** catalogue whatever `collapse_tools` says
(`client_requires_meta_tools`), for a different reason than grok's `__`
delimiter limit: ego captures one immutable catalogue per generation and ships
every definition on every model call, while `/mcp` proxies 200+ upstream tools
on a normal day. Measured 2026-09-13 at 190 tools: 35.104 tokens per turn
against 615 collapsed, and flat as upstreams grow. Nothing is unreachable —
every native and upstream tool is still callable through `call_tool`, and
`progress` stays directly callable — the cost is one extra round trip before
the first use of an unfamiliar tool.

The handshake declares `tools.listChanged` (plus `experimental.claude/channel`).
The GET `/mcp` SSE stream emits `notifications/tools/list_changed` when the
upstream set changes, and a client may ignore a notification for a capability
the server never advertised.

**`tools/call` does not require `Mcp-Session-Id`.** Identity is resolved per call
rather than demanded up front. A caller that sends the header gets its protocol
session refreshed — a stale id (app restart, or a long-lived client like Claude Code
that lost its session) auto-recovers instead of erroring. A caller that sends none is
served anyway: most tools need no caller identity at all, so a plain `curl` against
`POST /mcp` now works.

**Every `initialize` is logged.** `source = "mcp_initialize"`, one record per
handshake, with `event` naming how the client arrived:

| `event` | Meaning |
|---|---|
| `fresh` | No session id presented — first contact |
| `resumed` | Presented an id we still hold — the same connection continuing |
| `reconnected` | Presented an id we no longer hold (reaped by the idle sweep, or lost with a restart). The stale id is in `presented_session`; a new one was minted |

`reconnected` is the record behind an agent announcing "TUICommander is back".
Before it existed, that arrival was indistinguishable from `fresh`: both silently
received a new UUID, and the only nearby log — `Reclaimed stale MCP peer binding
during initialize` — fires only when a prior peer binding still exists, which a
reaped session no longer has. Claims about the connection dropping were therefore
confirmable but never refutable.

The identity-scoped actions (`agent action=register|send|inbox|wait`) still refuse
without a protocol session — but each refuses on its own, with the concrete next step
(`initialize`, or `agent action=register`) that the previous blanket `-32600` never
gave the caller. Making identity itself independent of the protocol session is a
separate step (Phase E of the dual-era plan); an `x-tuic-session` header binds a TUIC
identity **to** an existing `mcp-session-id`, it does not substitute for one.

`GET /mcp` and `DELETE /mcp` still require the header — both are bound to a specific
protocol session, and `GET` answers `401` without it.

The `GET /mcp` SSE stream emits `notifications/tools/list_changed` whenever the available tool set changes (e.g., native tools are enabled/disabled via config, or upstream MCP servers connect/disconnect). The bridge sidecar subscribes to this stream and forwards the notification to the AI agent.

A session may briefly hold two `GET /mcp` streams: a reconnect can be accepted while
the stream it replaces is still half-open. Each stream takes a generation on the
session and its teardown releases the session — the `has_sse_stream` flag and the
per-session messaging sender — **only while it still holds that generation**. An
unconditional release let the older stream's drop remove the sender its own
replacement had just subscribed to, closing the new stream immediately.

Three details in that guard are easy to get wrong:

- Generations come from **one process-wide counter**, never per session. A
  `DELETE /mcp` can retire a session id while its stream is still draining, and the
  next `GET` auto-recovers that id from scratch — a per-session counter would restart
  and reissue a number the half-open stream still holds.
- The teardown holds the `mcp_sessions` entry **across** the channel removal, and
  `mcp_get` holds it across its subscribe. Split apart, a superseded stream can pass
  its generation check and then remove the sender after the replacement subscribed.
  Lock order is `mcp_sessions` → `messaging_channels`; nothing may hold a
  `messaging_channels` reference while taking `mcp_sessions`.
- That hold is taken with `entry`, not `get_mut`, because the **absent** case needs
  it too. When `DELETE` or the reaper has already retired the id there is no
  generation to compare and teardown removes the channel outright; `get_mut`
  returning `None` releases the shard before that removal, and a reconnect landing
  in the window recreates the session, subscribes, and then loses its brand-new
  sender to the older stream's drop. Only `entry` keeps the shard locked over a key
  that is not there.

The bridge uses the standard MCP `ping` request for its three-second liveness check. This keeps health traffic constant-size as terminal count grows; it does not rebuild or serialize the complete tool catalog. If IPC is reachable but `/mcp` is unavailable, the bridge reports that the MCP endpoint is unavailable instead of incorrectly claiming the desktop process is not running.

The bridge retains the downstream client's last `initialize` request and replays
it internally after reconnecting to a restarted TUIC process. This restores
session-scoped metadata such as Grok compatibility mode without sending a
second initialize response to the client.

On reconnect, a peer may reclaim its stable TUIC identity after the prior MCP protocol session has no live SSE subscriber and has missed the bridge activity grace period. The takeover retires the old forward and reverse routing entries atomically.

When the asserted identity has no live PTY binding, the first MCP request also
checks the foreground processes of local terminals. Exactly one process whose
actual `TUIC_SESSION` matches the header restores that terminal association.
This repairs a missing restore-time binding even when the MCP route already
exists, so session lists publish the parent's identity and mobile can nest its
children. Existing live bindings stay intact; unrelated or ambiguous matches
remain terminal-less. ACP conversation identities do not acquire a terminal.

On normal stdio EOF, the bridge sends `DELETE /mcp` after its in-flight requests settle, releasing that protocol session immediately. Each fresh initialize for an existing `x-tuic-session` identity also retires protocol metadata, routes, and message channels of stale sibling sessions left by abrupt exits. Subscribed and recently active siblings remain intact. Sessions without a stable identity still use the hourly maintenance sweep after an abrupt exit.

After a failed upstream initialize, the bridge waits before trying again. The pause starts at one second, doubles up to eight seconds on consecutive failures, and resets after a successful connection. Request-triggered, downstream initialize, and background retries share this limit within one bridge process.

A currently subscribed or recently active owner is never replaced — but it can be *joined*. One PTY may hold more than one bridge (Codex opens two). They usually inherit the same `$TUIC_SESSION` and assert the same `x-tuic-session`. Only a process that inherited that PTY's environment can assert it, so a second asserting bridge is a sibling, not a claimant: it is added to the identity's routing (`mcp_to_session` plus the `session_to_mcp` list) while the live owner keeps delivery ownership. Ownership stays put on purpose — two live siblings that traded it on every request would flip the delivery channel back and forth. The inbox is keyed by the PTY identity, so both bridges read the same mail. `agent action=register` from a joined sibling is a rename, not a takeover; a protocol session with neither the header nor an existing route is still refused with "already registered to another active MCP session" (throttled to one WARN per claimant pair). Ending one co-owner's protocol session drops only its own routes and promotes a survivor to delivery owner; the peer entry, inbox and orchestrator role are torn down only when the last co-owner goes.

If a second bridge in the same live PTY asserts a different UUID (the persisted
tab UUID versus the PTY key), initialize joins the first registered peer's
mailbox. `register` from that bridge keeps the shared identity. Both asserted
UUIDs still resolve through the PTY for `agent send`, but `list_peers` exposes
one recipient and both MCP connections read the same inbox. This coalescing
requires the same live PTY; peers on different PTYs remain separate.

**Blocking tool actions run off the runtime worker.** Dispatch is action-aware:
session create/input/close/kill/resize, agent spawn/send, and config
writes use `run_blocking_handler` (`tokio::task::spawn_blocking`) because they may
sleep, spawn processes, write PTYs, or touch disk. Common read-only actions such
as session list/output/status and agent list_peers/inbox execute
inline without cloning their JSON payload or scheduling blocking-pool work.
Async waits remain on their event-driven async handlers. A panicking blocking
handler becomes a tool error rather than killing the request task; `POST /mcp`
does not add another blanket wrapper.

### Native argument repair contract (#2007-59f6)

Native argument contracts are enforced before dispatch, both for direct tools
and `call_tool`, on HTTP and MCP-over-ACP. The server keeps the complete private
JSON Schema, including action-dependent requirements (for example `ui tab` needs
`id`, `title`, and exactly one of `html`/`url`). Validation cannot fetch external
schema references.

Advertised `inputSchema` objects have no root `oneOf`, `anyOf`, or `allOf`:
Anthropic tool clients reject that shape. Full listing, lazy search, and exact
schema lookup publish the same flat action enum and properties. Conditional
requirements are in description text; nested parameter alternatives use `anyOf`.
The private validator still enforces exclusive alternatives and required fields
before any effect starts, including grouped workflow actions and agent task handles.
Static rejection returns a tool error with:

- `code: "invalid_arguments"`, `outcome: "rejected"`, `effect: "not_started"`;
- `retryable: false`: do not repeat unchanged arguments;
- `repair.retry_after_correction: true`: a corrected request may be submitted;
- at most 16 `violations`, each with JSON-pointer `path`, `schema_path`, the
  schema `constraint` (limit, required fields or allowed values), and a masked
  explanation. Input values are not copied into repair errors.

This is not auto-retry advice for permission, configuration or safety refusals.
Those remain existing refusals, without the static repair marker. Runtime
preconditions and authorization remain the handler's responsibility; a valid
schema never grants access or proves an effect completed.

Progress text permits 1..500 Unicode scalar values before trimming, not UTF-8
bytes, UTF-16 units, or grapheme clusters; whitespace-only text is invalid.
The shared Progress model enforces the same raw length for HTTP `/progress/report`
and IPC `report_progress_event`, which retain their typed transport errors.

`src-tauri/src/mcp_http/fixtures/action-input-contract-2007.json` is a local
consumer contract fixture, not a captured client trace. It covers the audit's
582/543/502-character retries, the valid boundary, emoji and combining scalars,
missing titles/text/paths, and an invented action. Ego must validate its admitted
schema against these cases without a server round trip; that cross-repository
consumer integration is not proven by TUIC's tests.

### On-demand native tool guidance

`session`, `agent`, `repo`, `ui`, `voice`, `telegram` and `automations` advertise
compact agent-facing descriptions with every action and the hard constraints.
Call the same tool with `{"action":"help"}` for its complete usage guide in the
`description` response field. Help retains all displaced rules, examples and
response details. It does not perform the requested operational action.
The normal disabled-tool and conditional-owner gates apply to help too.
Argument schemas and pre-dispatch validation remain authoritative; discovery
continues to publish flat client-compatible schemas. Other already compact
native tools are unchanged, including the directly loaded `progress` primitive.
Settings summaries and tooltip copy do not change. Reconnect cached MCP clients
after loading this backend build to obtain the updated catalogue.

### Lazy Tool Discovery (`collapse_tools`)

When `collapse_tools: true` in `config.json` (or via Settings > MCP > TUIC Tools > "Collapse tools"), the server replaces the full tool list in `tools/list` with three meta-tools (the Speakeasy pattern) plus the compact `progress` tool when enabled:

| Meta-tool | Purpose |
|-----------|---------|
| `search_tools` | BM25 search over the full native + upstream tool corpus; returns name/summary pairs, an opaque `catalogue_generation`, and an optional complete `top_schema` |
| `get_tool_schema` | Returns the full `{name, description, inputSchema}` for a specific tool |
| `call_tool` | Dispatches to the named tool — routes to the native handler or `proxy_tool_call` for `{upstream}__{tool}` names |

Search grammar is server-owned. Plain words rank names and descriptions with BM25. Each `+term` is a required, case-insensitive substring of the tool name (including any upstream or server alias prefix); several required terms use AND. Remaining plain words rank the matching names but do not exclude them. Filtering happens before the result limit, and a required term with no name match returns no results. `select:<name>` selects one exact catalogue name without ranked fallback.

Rationale, measured 2026-09-13 on the running desktop instance with 190 tools connected (see [Measuring the surfaces](#measuring-the-surfaces) for the method): the full list is 154,117 bytes / 35,104 tokens in every agent turn, against 2,810 bytes / 615 tokens for the collapsed surface — which does not grow with the upstream count, because the upstream tools are no longer in the list. The agent fetches other schemas on demand. `progress` stays direct so routine reporting needs no search/schema preflight. Toggling `collapse_tools` fires `notifications/tools/list_changed` so connected clients refresh their tool cache.

TUIC also selects this three-tool surface automatically for two clients
(`client_requires_meta_tools`), for two unrelated reasons. The selection is
per-client: it does not change `collapse_tools` or the surface returned to
anyone else.

- **Grok** (`clientInfo.name` starts with `grok-shell-`) accepts only one `__`
  namespace delimiter in a qualified MCP tool name, so it otherwise discards
  proxied names such as `tuicommander__upstream__tool`. The meta-tools keep the
  upstream identifier in the `call_tool` argument instead of the qualified id.
  Session-local, read from `initialize`.
- **ego** (`clientInfo.name` is `ego`) pays for the catalogue on every model
  call — one immutable tool list per generation, every definition sent with
  every request — so the numbers above are its per-turn cost, not a one-off.
  ego is stateless, so the name is read from each request's `_meta` rather than
  from a handshake.

A request that names a client in `_meta` decides the surface **by itself**; the
session's remembered flag is the fallback for a legacy client that named itself
once at `initialize`. The precedence runs that way because ego reaches `/mcp`
through `tuic-bridge`, which opened the transport session under its own name:
preferring the session made `server/discover` advertise the collapsed surface and
the very next `tools/list` deliver the full one, for the identical request.

**Schema admission and reuse (#2013-73f9).** Search includes the complete definition of
its first visible match when the serialized UTF-8 JSON fits `schema_byte_budget`
(default 16384 bytes, maximum 65536; 0 disables inline schemas). The budget applies
to the definition, not the whole search response. Oversized schemas are omitted,
never truncated; select a result and fetch it with `get_tool_schema` instead.
A client can admit `top_schema` directly and call the tool without a second
schema-fetch hop. Reuse an admitted definition until `notifications/tools/list_changed`
or a different `catalogue_generation`. The generation is a content digest of the
search corpus, not an authorization token. An unchanged rebuild keeps it stable.
HTTP SSE and MCP-over-ACP refresh the index before forwarding invalidation, including
lag recovery. Dispatch still checks live permissions and visibility; a cached schema
never grants access. Clients must implement admission and invalidation themselves;
server tests are not evidence of a real ego client's hop reduction.

**Filter enforcement.** Both `search_tools` and `call_tool` re-apply the safety filters that the full listing would apply: `mcp_capabilities.denied` is checked up-front in `handle_call_tool`, and upstream allow/deny filters are enforced at both enumeration time (`aggregated_tools`) and dispatch time (`proxy_tool_call`). This is critical under collapse mode: discovery no longer gates dispatch, so an agent that knows a filtered tool name cannot bypass the filter by calling `call_tool` directly. `search_tools` and `get_tool_schema` also reject meta-tool names, and `call_tool` refuses to recurse into itself.

The BM25 index lives in `AppState::tool_search_index` (`parking_lot::RwLock<ToolSearchIndex>`, backed by `src-tauri/src/tool_search.rs`). A background task subscribes to the `mcp_tools_changed` broadcast and rebuilds the index whenever the tool set changes (upstream connect/disconnect, `mcp_capabilities.denied` edit, `collapse_tools` toggle).

The MCP instructions string returned by `initialize` (`build_mcp_instructions`) swaps to a "lazy discovery" guide when `collapse_tools: true` or the connecting Grok session requires compatibility mode, so agents know to call `search_tools` first rather than looking for a flat tool table.
The initialize instructions carry no repository paths. The `## Repos` section is one line: the repository count, the count per group and per remote connection, and the `repo action=list` call (the `call_tool` form when tools are collapsed). `repo action=list` returns each repository with its `group` and, for a repository on a remote connection, `connection_id`; local rows have no `connection_id`. The `## What TUIC gives you` section names the native tools registered on that install by purpose (reach the user, drive the platform, other) and the connected upstream servers with their tool counts and how to discover those tools (ToolSearch for Claude Code, `search_tools` when collapsed). It names only tools that `tools/list` serves and is omitted when there are none. The `suggest:` marker is omitted for a spawned child (an MCP session bound to a peer with a parent session); root sessions keep it. Upstream servers have no description field in their configuration, so only names are listed.
The TUIC connection acknowledgment in those instructions is emitted exactly once per MCP connection or reconnect, never once per conversational turn. TUIC protocol context remains in initialize instructions and native core-tool descriptions only; upstream tool descriptions are preserved instead of receiving a repeated TUIC preamble.
When intent markers are enabled for the connecting agent, initialize instructions require `intent:` on its own line, at the start of every user task and on material phase changes. "On its own line" is in the instruction because the ack sentence is required to open that same first message: an agent that runs the two together leaves the token mid-row, where the parser has to reach for it (see `docs/backend/output-parser.md` → Intent). The backend stores that dynamic intent independently from the spawn-time PTY description and the last submitted prompt.

### Two instruction surfaces, and which one owns a rule

Initialize instruction prose stays within the existing 1,600-byte classic and
1,664-byte collapsed empty-state budgets. Compact wording preserves the answer
marker, urgent mail and progress reporting rules. The answer marker can start a
row or the text of a Markdown list item; status, tool and hook/task notices stay
excluded.

TUIC supplies protocol instructions and tool descriptions on different wire
surfaces. Client receipt does not prove that the model can read either surface
on its first turn. Deferred tool discovery can expose them later.

| Surface | Transport receipt | Model visibility | Owns |
|---|---|---|---|
| `initialize.instructions` (`render_mcp_instructions`) | Returned in the initialize response | Harness-dependent: Claude exposes a bounded system block; Codex exposes namespace metadata after discovery in the measured mode | Wire protocol markers, a capability map, cross-tool rules, live state |
| tool `description` (`native_tool_definitions`) | Returned for advertised tools on `tools/list` | May be deferred by the harness; in Speakeasy mode native descriptions arrive through discovery | That tool's actions, arguments and semantics |

**Pinned canary evidence (2026-10-04).** Claude Code **2.1.286**, with
`claude-sonnet-5-5`, exposes server instructions even when tool descriptions are
deferred. Its default instruction cap is **2,048 characters**: the 1,024- and
2,048-byte ASCII controls cite START/MIDDLE/END; the 4,096–32,768-byte controls
cite START only (`CLAUDE_DEFAULT_*`). The unused probe description is absent
until tool loading. Disabling tool search exposes that description but does not
remove the instruction cap. `CLAUDE_CODE_MAX_MCP_DESCRIPTION_LENGTH=65536`
exposes all three instruction canaries at 32,768 bytes
(`CLAUDE_MAX_OVERRIDE_32768`).

Codex CLI **0.160.0**, with `gpt-6-luna`, receives the instructions, but its
initial no-tool turns cite **NONE** at all six tested sizes. After metadata
discovery, it cites the instruction START/MIDDLE/END and description token at
1,024, 4,096 and **32,768 bytes** (`CODEX_SEARCH_*`, including
`CODEX_SEARCH_32768.stdout`). Thus the field is not universally discarded, and
post-discovery visibility does not establish first-turn visibility.

The harness report at
`~/Gits/personal/orchestrator/reports/tuicommander/2026-10-04-mcp-instructions-harnesses.md`
records versions, controls and limits; its raw canary evidence is under
`~/Gits/.tmp/mcp-canary/` (including `CLAUDE_DEFAULT_2048.stdout`). Speakeasy
advertises meta-tool descriptions first; native `agent`, `repo` and `session`
semantics require discovery. A description fallback therefore cannot guarantee
that startup rules prevent an earlier shell action. Actual within-process
reconnects and untested harness versions remain unmeasured.

Instruction files are a separate surface. The
rules-file study at `~/Gits/.tmp/rules-file-study/result.md` measures Claude
`@file` expansion, literal unexpanded `@file` lines in Codex `AGENTS.md`, and
explicit ego `--instructions FILE` loading in headless mode. Do not treat a
file reference, MCP receipt, or parent context as proof of child or lifecycle
visibility. The studies do not change TUIC's launch behavior.

The rule that follows: a statement a tool description carries is not repeated in
the instructions. A client reading both paid for it twice, and — worse — a
client that loaded only descriptions could otherwise go without that rule. That is the direction
story `078-8b2e` set, and `instructions_do_not_repeat_what_tool_descriptions_already_say`
now enforces it in both directions, asserting the removal *and* the surviving
copy. Three things moved out of the instructions this way: the per-tool
catalogue (`tools/list` already carries it in the same turn), the `## Workflow`
section (the `agent` description's five-line primer restated), and the
`**UI feedback:**` line (the `ui` description's `Use:` block restated). The
orchestrator role and its `mail_wake` contract moved *into* the `agent`
description, where a headerless caller can actually find them.

Two rules stay in the instructions on purpose and are not duplication:

- **Worktrees** — it forbids `git worktree add/remove`, a tool that is not ours.
  No TUIC tool description is a reliable place to ban a shell command.
- **Submit** — it spans two calls (text, then Enter) and a polling loop, so it
  belongs to no single action. Agents split submits until this line existed; it
  is pinned byte-exact in both modes by
  `initialize_instructions_pin_the_one_call_submit_rule_in_both_modes`.

There is no "full prompt" mode on the server and none is planned: the long-form
operator guidance lives in this document, which is the optional surface a human
opts into, not a per-turn cost every agent pays.

#### Measuring the surfaces

`mcp_instruction_surface_bytes_stay_within_budget` measures every surface a
client receives and asserts a ceiling on each. It is reproducible because
nothing in it reads the host: `render_mcp_instructions` takes an explicit
`InstructionContext` (repos, sessions, peer count) instead of reading the user's
`repositories.json` and live PTY map, and `spawn_response` renders from fixed
ids instead of a launched process.

```bash
# byte counts + the dump
TUIC_MCP_SURFACE_DUMP=$PWD/.tmp/mcp-surface.json \
  cargo nextest run --lib -E 'test(mcp_instruction_surface_bytes)'

# token counts from the same dump
python3 -m venv /tmp/tokvenv && /tmp/tokvenv/bin/pip install tiktoken
/tmp/tokvenv/bin/python - <<'EOF'
import json, tiktoken
enc = tiktoken.get_encoding("o200k_base")
for k, v in json.load(open(".tmp/mcp-surface.json")).items():
    print("%-34s %7d bytes %7d tokens" % (k, len(v.encode()), len(enc.encode(v))))
EOF
```

**Tokenizer:** tiktoken 0.14.0, encoding `o200k_base`. It is a GPT tokenizer and
therefore a *proxy* — Anthropic publishes no offline tokenizer, so a Claude token
count cannot be produced here. Bytes are exact; treat the token column as an
order-of-magnitude figure and never quote it as a Claude cost.

**Deployed baseline** — measured 2026-09-13 against the running desktop instance
(TUICommander v1.7.7, `GET /mcp/instructions` and `POST /mcp` `tools/list` on
`localhost:9876`), 16 repos, 28 sessions, 26 peers, 2 upstream servers:

| Surface | Bytes | Tokens (o200k_base) |
|---|---:|---:|
| instructions, static prose | 3,525 | 891 |
| instructions, dynamic repos + sessions | 3,057 | 1,178 |
| instructions, total | 6,582 | 2,069 |
| `tools/list`, 190 tools | 154,117 | 35,104 |
| `tools/list`, 6 native tools only | 21,990 | 5,127 |

Two things that baseline settles. The `~35k tokens` figure quoted for the
uncollapsed tool list is real, not folklore — 35,104 measured. And the static
prose is *not* where the instructions grew: at 891 tokens it still fits the
1,000-token budget story `1179-1cf9` set, while the dynamic repo/session block
that no deduplication can touch is now the larger half at 1,178. Read any
claimed saving against that split before believing it.

**Checkout measurement, before and after the deduplication** — same fixture both
sides (`test_state`, which leaves `mcp_capabilities.denied` empty, so its classic
figure covers all nine native tools where production hides `config` and `debug`);
`.empty` is zero repos/sessions/peers, `.loaded` is 8 repos, 12 sessions, 6 peers:

| Surface | Before (B) | After (B) | Δ | After (tokens) |
|---|---:|---:|---:|---:|
| instructions, classic, empty | 3,510 | 1,373 | −2,137 | 334 |
| instructions, classic, loaded | 4,497 | 2,445 | −2,052 | 738 |
| instructions, collapsed, empty | 2,300 | 1,417 | −883 | 345 |
| instructions, collapsed, loaded | 3,287 | 2,489 | −798 | 749 |
| `schema.repo` | 2,626 | 4,075 | **+1,449** | 899 |
| `schema.agent` | 6,721 | 7,013 | +292 | 1,627 |
| `schema.ui` | 3,444 | 3,689 | +245 | 889 |
| `schema.session` | 5,105 | 5,105 | 0 | 1,219 |
| `response.register` | 2,882 | 2,882 | 0 | 668 |
| `response.spawn` | 815 | 815 | 0 | 244 |
| `tools/list`, classic | 22,423 | 24,409 | +1,986 | 5,685 |
| `tools/list`, collapsed | 2,377 | 2,758 | +381 | 603 |

And the number that actually matters — instructions plus `tools/list`, which is
what one connection pays:

| Mode | Before | After | Net |
|---|---:|---:|---:|
| classic | 25,933 B | 25,782 B | **−151 B (−0.6%)** |
| collapsed | 4,677 B | 4,175 B | **−502 B (−10.7%)** |

Read that honestly. The deduplication removed **2,137 bytes** of prose from the
classic instructions, and about 590 of those bytes did not disappear — they moved
into the `agent` and `ui` descriptions, where clients ignoring instructions can
finally see them. Against that, the same change *added* **1,449 bytes** to the
`repo` description for nine `progress_*` actions that were advertised and
undocumented. Netting the two leaves classic essentially flat.

So: the story's saving is real but it is not a size saving in classic mode. What
it bought is that every rule now has exactly one owner, and that owner is the
surface every client receives. Collapsed mode, where `tools/list` is four tools
instead of nine, keeps a measurable 10.7%.

### MCP Native Tools

Native tools are organized by domain. Settings access and diagnostics are disabled by default via capability denials; listing, search, schema lookup and dispatch respect this filter in both full and collapsed mode. The enabled `progress` tool is additionally kept on the direct collapsed surface.

The payload measurements above predate `voice` and are left as recorded: they
say what was measured, not what the list costs today.

| Tool | Actions | Default |
|------|---------|---------|
| `session` | list, create, submit, input, output, status, wait, resize, rename, close, kill, pause, resume | Enabled |
| `agent` | spawn, wait, register, list_peers, send, inbox, subscribe, ack, unsubscribe, task_get, task_cancel | Enabled |
| `remote` | preview, update | Only with a connected remote server |
| `repo` | list, active, status, branch_integrations, branch_integration, worktree_list, worktree_lifecycle, worktree_create, worktree_remove, orphan_cleanup_answer, branch_delete, progress_list | Enabled |
| `progress` | *(no actions — appends one `done` or `blocked` entry)* | Enabled, unless `progress_tracking` is off |
| `ui` | tab, toast, confirm, screenshot, design_start/stop/status/reload/navigate/screenshot/errors/eval | Enabled |
| `workflow` | story, run, proposal_create, report, launch | Enabled |
| `automations` | Scheduled definitions and runs | Enabled |
| `plugin_dev_guide` | *(no actions — returns guide text)* | Enabled |
| `voice` | speak, stop, status | Only with voice configured |
| `telegram` | register, unregister, begin, activity, finish, send | Only with Telegram configured |
| `settings` | describe, get, set, reset, patch, perform, refresh | Disabled; trusted UI opt-in required |
| `debug` | agent_detection, logs, sessions, invoke_js, help | Disabled |

### Capability cutover and client migration

`mcp_capabilities` is the persisted policy object. `denied` defaults to `["settings", "diagnostics"]`. Settings access additionally requires trusted UI opt-in. Parent capabilities deny every action in their domain; member capabilities deny only their actions. Listing, search, exact schema lookup and direct/meta dispatch apply the same live policy, including before the search index refreshes.

On the first load, `disabled_native_tools` migrates to this object and is removed on save. Mappings: `task` → `agent_tasks`; `story` → `workflow.story`; `workflow_run` → `workflow.run`; `workflow_story_create` → `workflow.proposals`; `workflow_report` → `workflow.reports`; `workflow_launch` → `workflow.launch`; `design_mode` → `design`; `debug` → `diagnostics`; `remote` → `remote_deployment`; `plugin_dev_guide` → `plugin_reference`. Other current tool names map to the same-named capability. An old parent denial still denies all grouped actions. Unknown names are logged and retained in `migration_unknown_tools`; Settings displays them for review. They are not silently dropped.

**Breaking change:** retired MCP names have no aliases. Use `agent action=task_get/task_cancel`, `workflow action=story/run/proposal_create/report/launch` (retain the generated `input` object), and `ui action=design_*`. Reconnect cached clients and update literal CLI calls and external allow rules. Existing `agent`, `workflow` and `ui` allow rules now cover their grouped actions; review that scope explicitly. HTTP/IPC service routes and `tuic story` do not change. Unknown external callers cannot be migrated automatically.

Compact `tools/list` measurement (2026-10-20, serialized `tools` array only; no JSON-RPC envelope):

| Mode | Before (`c79f74bfe3`) | After cutover | Change |
|------|-----------------------|---------------|--------|
| Classic | 92,781 bytes | 73,435 bytes | −19,346 bytes (20.9%) |
| Collapsed | 3,581 bytes | 3,581 bytes | Unchanged |

Both runs use zero upstreams, Settings enabled and optional owners unconfigured. The regression test `capability_cutover_rejects_retired_names_and_measures_catalogue` prints these compact byte counts and rejects every retired name. These are byte measurements, not token estimates or a complete configured-owner inventory.

Settings reads `native_capabilities` from `get_mcp_status` / `GET /mcp/status`; each entry has `name`, `title`, `description` and `optional`. The six optional switches are Secrets, Telegram, Settings access, Diagnostics / JS inspection, Remote daemon deployment and Progress tracking. Additional restrictions contain core domains and migrated member capabilities. The unfiltered `native_tools` reference inventory remains available separately. Disabled and unavailable capabilities remain configurable; upstream policy, trusted Settings opt-in, Design Mode host grants, voice binding and deployment consent remain separate.

Native MCP inputs use `path` for a repository root in `agent register/list_peers` and `repo`, and `branch` for `repo worktree_lifecycle/worktree_remove`. The old `project` and `workspace_id` input names are rejected. The shared `worktree_create` response still includes `workspace_id` alongside `branch` for HTTP parity. `spawn_session=true` on worktree creation starts a bare shell PTY; spawn an agent separately when one is needed.

`repo action=orphan_cleanup_answer path=<repo> decision=remove|keep` answers the
currently open orphan cleanup dialog. A remove answer rechecks every pending
worktree for tracked or untracked changes, a HEAD reachable from a branch and
live sessions inside it; an unsafe or stale request is refused. The frontend consumes the answer and
closes the dialog before removal.


#### Conditional native tools

`remote`, `voice` and `telegram` use one shared availability predicate for
classic listing, lazy search, exact schema lookup and direct or `call_tool`
dispatch. A known name does not bypass a missing condition. Explicit native
config denials still take precedence.

- `remote`: at least one `RemoteRuntime` entry has `RemoteStatus::Connected`.
  Saved records, connecting tunnels, failed authentication and health-only
  probes do not count. Preview digests and session-loss confirmation stay in
  the deployment owner and are not relaxed by this condition.
- `voice`: the persisted `dictation-config.json` parses as `DictationConfig`
  with both `enabled` and `hands_free_spoken_replies` true. A missing or corrupt
  document is not configured. Engine selection is not the opt-in: an unset
  engine resolves to Edge. This does not require an armed conversation or
  probe the microphone, speech model or network.
- `telegram`: the private setup config has `enabled=true` and a nonempty
  `bot_alias`, and the private `bot.token` file exists. Discovery checks only
  credential-file availability, never its contents. Network connectivity,
  chat selection and agent registration remain separate runtime checks.

Remote connected-set transitions and successful voice/Telegram owner setting
changes send `notifications/tools/list_changed` through the existing HTTP SSE,
bridge and ACP notification paths. The Telegram polling owner also observes
cross-process setup revisions. Clients that ignore catalogue notifications
must reconnect after a condition changes. The Settings tool inventory remains
unfiltered so users can inspect and enable optional capabilities.

A configured voice can still answer `available: false` with a runtime reason.
A build without audio retains that answer after checking terminal binding;
an unbound connection is refused before it can receive voice status.

Every action is bound to the **calling terminal**: `resolve_mcp_origin_session`
maps `mcp-session-id` onto a TUIC session, and that session must be the one
hands-free is armed for. A connection with no TUIC session is refused outright
rather than falling back to the user's own control surface. `status` checks the
binding too — the fields alone say what somebody else's conversation is doing.

Because `handle_call_tool` preserves `addr` and `mcp_session_id`, the direct and
collapsed paths reach the same handler with the same identity;
`voice_binds_to_the_calling_terminal_on_the_direct_and_collapsed_paths` asserts
both produce byte-identical answers, bound and unbound.

The schema takes `action`, `text`, `turn` and `utterance_id` — **no language and
no voice**, and `a_model_cannot_choose_the_language_or_the_voice_it_is_spoken_in`
pins that list. Both come from the conversation: the dictation language when it
names one, and what Whisper detected under `auto`, reported as
`status.language`. A model that could pass either would be a second source, and
the two would disagree the first time the user switched languages. The
description says so as well as the schema preventing it, because a model that is
merely blocked writes its reply in English and wonders why it sounds Italian.

The description also **places the two mode notices** the model will receive
(`MODE_ENTRY_HINT` and `MODE_EXIT_HINT`, see
[dictation.md](dictation.md#telling-the-model-the-mode-changed-821-842a)). They
arrive as ordinary terminal input, indistinguishable from something the user
typed, so the tool says whose they are and that the end notice is final —
otherwise a model can answer "hands-free voice is off" instead of obeying it. It
also says the user can turn them off, so their **absence is not** evidence that
nothing is armed; `action=status` is.
`the_voice_tool_places_the_notices_the_model_will_receive` pins both sentences.

This table is generated from the same `*_ACTIONS` constants the schemas use, and
`every_documented_action_constant_matches_schema_and_description` keeps the three
in step: the constant, the `action` enum in the schema, and a documenting line in
the tool's own description. It is the gate that closed the gap where `REPO_ACTIONS`
and the `repo` schema both advertised nine `progress_*` actions that the
description body never mentioned — invisible to any client reading descriptions
alone. `debug` is the single exemption: its description points at `action=help`,
which returns the full usage guide.

#### One session, three addresses

Every action that takes a `session_id` — and `agent action=send`'s `to` — accepts any
of three names for the same terminal: the **PTY id** the server minted, the
**`tuic_session`** the tab persists across restarts, and the repo-derived **alias**
(`tu-1`). `AppState::resolve_session_ref` maps the first two forms onto the live PTY
id and `AppState::resolve_peer_ref` maps any of them back onto the peer identity that
owns that terminal, so a model never has to remember which of the three it was handed.

Resolution deliberately **falls through instead of failing**. A reference that matches
nothing is passed to the action unchanged, because a session whose process has exited
leaves its output buffers behind while its registry entry is gone: hard-erroring on an
unresolvable reference would break `session action=output` on exactly the session an
orchestrator most needs to read. The action that owns the reference still reports
`Unknown session` when it genuinely needs a live one.

`ui action=confirm` blocks only its requesting tool call while the native dialog
is open. The dialog runs on the blocking pool, so an unanswered confirmation
cannot occupy the async MCP workers serving other agents and sessions. The
stdio-to-IPC bridge gives this call 305 seconds: the confirm handler's own
300-second answer window plus five seconds for response framing and scheduler
latency. That 305 s is bridge-side slack only — the operation itself is still
bounded by the server's own 301-second `REQUEST_TIMEOUT` (see "Server Limits"
above), which is what actually resolves the call before the bridge's wait would
matter.

**`ui action=toast` sound.** `sound` accepts `true`/`false` or a notification-sound
name: `question`, `completion`, `error`, `warning`, `info`, `attention`. `true`
resolves from `level` (info→info, warn→warning, error→error); a name overrides it.
`attention` is a triangular G4→G4→E5 callback meant for an agent working
unattended that is blocked on the user: two quick knocks followed by a longer
rise. An unknown name is an error, never a silently silent toast. The backend
resolves the name *before* emitting, so the toast event carries a concrete sound
and every client plays it through the user's notification settings (volume,
output device, per-sound mutes) rather than inventing a tone. The event is
dual-emitted — Tauri `emit` for the desktop WebView **and** the event bus for
SSE clients; a bus-only send never reaches the desktop, which has no bus→window
forwarder.

**`ui action=toast` urgency.** `urgency` accepts `normal` (default) or `urgent`, the same names as `agent action=send`. Other values are rejected; the value rides the `mcp-toast` event on both transports. MCP toast notices show a transient card only while their origin terminal tab is active. Settings → Notifications → **Show cards from the active terminal** defaults on and is saved per client; turning it off restores urgent-only cards. Urgent notices always show a card, and notices without an origin session remain bell-only unless urgent.


The toast also resolves the calling MCP session to its peer project, PTY cwd, or
session repository metadata (in that order). The resulting `origin_repo_path` is
dual-emitted with the toast so clients can show its source and scope retained
bell history to the repository that raised it. Callers do not provide this field.

Alongside it the toast carries `origin_session_id`, the caller's TUIC session,
taken from the same lookup. A repo holds many tabs, so the path alone cannot
navigate: this is what lets a client focus the terminal that actually raised the
toast when the user clicks it. It is absent for a caller that is not bound to a
PTY — an unbound caller gets no id rather than a guessed one, because a wrong id
would send the click to somebody else's terminal.

Native responses omit optional values when they are unavailable. In particular,
`session action=output` includes `exit_code` only after an exit status is known, and
blocking wait timeouts return only the condition state without a repeated follow-up hint.
Native tool values are wrapped as compact JSON text in the MCP `content` envelope.
A proxied upstream value that is already a valid MCP `CallToolResult` object (an object
with a `content` array) instead becomes the JSON-RPC result directly, preserving its
`content`, `isError`, `structuredContent`, and any extension fields without mutation.
This applies both to direct `{upstream}__{tool}` calls and to the collapsed `call_tool`
meta-path. A malformed upstream value falls back to the native compact JSON text
envelope so the response remains protocol-valid and inspectable.

#### Agent task actions — long-running orchestration past the 300s ceiling

`agent action=wait` and `session action=wait` are server-side blocking long-polls
clamped to `WAIT_MAX_MS` (300 000 ms). An orchestrator supervising a peer that works
longer than five minutes cannot hold one open, and a client that drops mid-wait loses
the outcome entirely.

`agent action=spawn` therefore also returns a **task handle**:

```json
{ "session_id": "…", "task_id": "…", "poll_interval_ms": 1000, "…": "…" }
```

The handle is polled with `agent action=task_get` instead of holding a wait open. This is
purely additive — every field a pre-task client already read keeps its name and type,
and a client that ignores `task_id` behaves exactly as before.
The retired `task` tool has no alias. Its saved denials migrate to `agent_tasks`,
which blocks `agent task_get/task_cancel`; disabling `agent` blocks all of its actions.
The grouped actions use the existing task handler and registry.

**Lifecycle.** A task is created `working` once the PTY is live (a refused or failed
spawn leaves none). `mark_session_exited` (`pty.rs`) drives it to `completed` with
`result = {session_id, exit_code}`, or to `failed` with `error` when the exit code is
non-zero — so the outcome is recorded whether or not anyone was listening.

| Status | Meaning |
|--------|---------|
| `working` | The agent is running |
| `input_required` | Waiting on input; still live |
| `completed` / `failed` / `cancelled` | **Terminal and immutable** — never change again |

The vocabulary is the MCP `2026-07-28` Tasks vocabulary from day one, so the standard
`tasks/*` front door stays a serialization change rather than a semantic remap.

**`agent action=task_get`** returns `{task_id, status, status_message?, result?, error_detail?,
poll_interval_ms}`, absent optional fields omitted. A failed task reports its reason in
`error_detail`, **not** `error` — a top-level `error` always means the call itself failed,
so reusing it would make a successful poll look like a broken one.

**`agent action=task_cancel`** marks the task `cancelled` and does **not** kill the agent
(`session action=kill` does that). Cancelling an already-finished task is not an error:
it reports the state that stands with `cancelled: false`, so a cancel racing the agent's
exit never looks like a failure. Because terminal states are immutable, a cancel that
lands first is never overwritten by the later exit.

**Ownership.** A `task_id` is a capability over a spawned agent, so ownership is checked
before any state is returned or mutated: one agent cannot inspect or cancel another's
children. A caller that spawned before registering is stamped with its pending id and
still reaches the handle after it auto-binds a TUIC identity. `cancel` additionally
re-checks the same loopback guard as `agent spawn`; `get` is read-only monitoring and
stays open to authenticated remote clients.

**Retention.** Tasks live in memory for 24 h and are reaped by the existing
`mcp_sessions` reaper, not a separate timer. They are deliberately **not** persisted: the
case this exists for is a *client* restart, which the TUIC process outlives. Disk would
only cover a TUIC restart — and that tears down every PTY, so a recovered `working` task
would describe an agent that no longer exists.

#### `ui` tool — `tab` URL schemes

The `url` param of `action=tab` supports three schemes:

| Scheme | Behaviour |
|--------|-----------|
| `http(s)://` | Loaded directly in a sandboxed iframe; external HTTPS and localhost HTTP use the permissive `default-src` CSP. |
| `file://` | Read via IPC and rendered as inline HTML in a sandboxed iframe |
| `tuic://edit/<path>?line=N` | Opens a native code-editor tab at the given file and line. Absolute paths require a `//` prefix: `tuic://edit//Users/x/file.rs?line=42`. Relative paths resolve against the active repo root. |
| `tuic://open/<path>` | Opens a native markdown/preview tab |

Site `X-Frame-Options` and CSP `frame-ancestors` still apply. There is no per-directive CSP, including `frame-src`; `http:` and `asset:` in `default-src` preserve localhost wildcard ports and asset frames.

Custom URL schemes (`vscode://`, `x-devonthink://`, etc.) do **not** work inside iframes and must not be used with `action=tab`.

### One tool family, and why the second one went

TUICommander exposes **one** MCP tool family: `session`, `agent`, `repo`,
`workflow`, `automations`, `remote`, `secret`, `progress`, `ui`, `plugin_dev_guide`,
`voice`, `settings`, `debug`. Few tools,
many actions.

It used to expose a second — 13 flat `ai_terminal_*` tools behind the
`ai_terminal_mcp_enabled` flag. They overlapped this family without being
equivalent, so a model that saw both had to guess, and paid for both catalogues
on every turn. Six of the 13 needed a per-session filesystem sandbox only the
embedded agent loop creates, so they refused every external caller before
dispatch. Deleted in story 789-f6ed; the tool-by-tool comparison behind it is
`plans/ego-integration/archive/tool-family-comparison.md`. The embedded agent loop that
created those sandboxes is itself gone (#784-0aec).

**The surviving tool names are a public contract.** They appear in users' ego
rule files (`allow 'tool(tuicommander/session)'`), so renaming one silently
stops a user's policy from matching — the call starts prompting, or stops. The
contract covers the tool names and the MCP server name. It does **not** cover
the action strings inside them: ego matches an action as an opaque argument, so
an action may still be added or renamed under the normal deprecation rules.

The three gates the deleted family carried, and where each landed:

| Gate | Outcome |
|---|---|
| **Secret redaction** on screen reads | **Preserved, and widened.** `redact_secrets` moved out of the condemned `ai_agent` module into `crate::redaction` and is now applied in `session action=output` — the screen read every client uses, Claude Code included, which never had it |
| **Concurrent-writer interlock** | **Retired.** Its only producer was `conversation_engine::ACTIVE_CONVERSATIONS`, i.e. the embedded agent loop. `session action=submit` guards more, not less: it refuses on `session_not_found`, `not_managed_agent`, `partial_composer`, `awaiting_input`, `agent_not_ready` and `queued_commands_pending` before touching the PTY. `session action=input` stays raw and unguarded, as it always was |
| **Mandatory write confirmation** (native dialog) | **Deliberately dropped.** AGENTS.md → Security Scope names agents driving TUIs a feature, not a risk to gate. A blocking native dialog is also unanswerable by a remote client — the exact defect that moved `ui action=confirm` onto the `mcp-confirm` event. A caller that wants a human gate calls `ui action=confirm`, which any client can answer |

Two capabilities have no replacement and were not reinstated: `wait_for`'s regex
and stability wait (poll `session action=output` with `since_cursor` instead),
and `get_context`'s compact summary.

**Session aliases** — Every action that accepts a `session_id` also accepts a
human-friendly alias (e.g. `tu-1`). Aliases are auto-assigned from the repo
directory name: a multi-word name contributes the first character of each
segment, a single word its first two characters (`tuicommander` -> `tu`), plus a
per-repo counter. `session action=list` includes the `alias` field.

An alias survives an app restart. The frontend persists it with the rest of the tab
state and replays it at create time; the backend reserves the requested alias when it
still has the `<prefix>-<number>` shape and no live session holds it, and raises the
per-prefix counter past it so the next auto-assignment cannot collide. A restored alias
is untrusted input, so a malformed or already-taken value is dropped and the session
gets a freshly minted alias instead.

### MCP Tool: `debug` — `invoke_js` and the Debug Registry

`invoke_js` executes JavaScript in the WebView (localhost-only). Results are logged with `source='eval_js'` and read via `debug(action='logs', source='eval_js', limit=1)`.

`POST /debug/reload_webview` is loopback-only. It logs the caller address before asking the native WebView to navigate to its last healthy URL; only the bundled app origin or configured development origin can replace that saved URL. The navigation log names the `http_route` trigger, action, URL, and success state. The recovery thread and page-load hook use the same navigation path with their own trigger names.

**`window.__TUIC__` bridge** — runtime introspection API:

| Method | Description |
|--------|-------------|
| `stores()` | List all registered store snapshot names |
| `store(name)` | Get a store snapshot by name |
| `plugins()` | All plugin states (legacy) |
| `plugin(id)` | Single plugin state with manifest (legacy) |
| `pluginLogs(id, limit?)` | Plugin log entries (legacy) |
| `terminals()` | All terminal states (legacy) |
| `terminal(id)` | Single terminal state (legacy) |
| `agentTypeForSession(sid)` | Agent type lookup (legacy) |
| `activity()` | Activity center sections/items (legacy) |
| `logs(limit?)` | App log entries (legacy) |

**Registered stores** (via debug registry): `github`, `globalWorkspace`, `keybindings`, `notes`, `paneLayout`, `repositories`, `settings`, `tasks`, `ui`. New stores self-register — see `src/stores/debugRegistry.ts`.

**Adding a new store snapshot** — 2 lines at the end of the store file:
```ts
import { registerDebugSnapshot } from "./debugRegistry";
registerDebugSnapshot("storeName", () => ({ /* fields to expose */ }));
```

### MCP Tool: `session` Atomic Submission

`session action=submit session_id=<id> input=<command>` is the managed-agent
command surface. It accepts only a confirmed-idle agent with an empty composer,
never queues, and keeps the PTY writer locked across Ctrl-U, a 50 ms gap, the
text (bracketed paste for multiline input), a second 50 ms raw-mode scheduling
gap, and Enter. Ctrl-U travels alone because Claude Code strips it from a long
text it treats as a paste and then refuses the Enter. The existing
`InputLineBuffer`, slash-mode tracking, submitted-input lifecycle, and
`turn_epoch` advance exactly once after the full write.

An ego empty composer painted after the last edit and quiet for 1.5 seconds
supersedes stale tracked input. Retained drafts and pre-edit screens still block.
Full local `session action=list` rows include `queued_commands` when entries
are parked, including server wakes, and `queue_blocker` when a current composer
gate prevents delivery. Ready observations retry existing queues and mail wakes;
rejected submits are never silently added to a queue.

`session action=suspend session_id=<id>` asks the UI to end a tab's PTY and agent
while keeping the tab restorable like after a restart (the user resumes it from
the tab). It is refused with `Cannot suspend: <reason>` while the agent is
working, a question awaits input, compose commands are queued or a plain shell is
busy, for a session that already exited, and for the caller's own session. The tab
performs the suspend after the `session-suspend-requested` event and answers through
`session_suspend_response` (`POST /mcp/suspend-response`, `{request_id, ok, reason}`);
the MCP call returns that verdict: `{ok:true}`, or `Cannot suspend: <reason>` when the
tab refuses. It returns an error when no UI is attached (headless with no open event
stream) and when no tab answers within 20 s. Localhost clients only.

`session action=keep_open session_id=<id> enabled=<bool>` controls automatic
idle closure for a managed child. `agent action=spawn keep_open=true` sets the
same mark at creation, and `agent action=send keep_open=<bool>` changes it while
delivering a message. These actions reject a user-created session.

The handler then waits internally for child output beyond the offset captured
immediately before Enter. One response returns:

| Field | Meaning |
|-------|---------|
| `submission_id` | UUID for this call |
| `submitted` / `write_state` | Whether the complete framed write finished; `uncertain` means retry may duplicate bytes |
| `acknowledged` | Child terminal output moved after Enter |
| `retry_safe` | True only when no byte was written |
| `turn_epoch` | Epoch advanced by the shared input FSM |
| `composer_state` | Composer safety verdict: `cleared`, `partial`, `empty`, or `unknown`; normally from `InputLineBuffer`, with fresh, settled native ego empty-screen reconciliation |
| `acknowledgement` / `reason` | Terminal-movement evidence or the precise rejection/timeout |
| `detail` | For pre-write rejections, a human-readable cause and corrective action; unknown agent identity includes the current foreground process |

Direct submits may answer an idle conversational question, including a
`progress blocked` report. That report is not a native dialog: its ranked
`progress-blocked` ownership permits an explicit command while server notices
remain parked. Ordinary low-confidence prose questions also permit submit.
Native selectors, approvals and free-text dialogs still return `awaiting_input`;
these refusals include `prompt_kind` (`choice`, `approval`, `free_text`, or
`question`). Explicit human replies retain their separate busy-dialog path.

The acknowledgement does not claim semantic application acceptance or task
success. Default acknowledgement timeout is 3,000 ms; callers may request
`timeout_ms`, clamped to 250–10,000 ms. `ack_timeout`, session exit after a
complete write, a superseding epoch, and an uncertain write all set
`retry_safe:false`. Partial composers, confident dialogs, busy/unconfirmed
agents, and older queued commands reject before the first byte. A peer that
arrives after the claim queues behind it; a peer that wins first makes `submit`
reject. Slash commands, including `/clear`, follow the same receipt contract.

`session action=input` remains the write-only compatibility surface. For an
identified agent, text with `special_key=enter` holds one PTY writer lock and
separates the text and CR with the agent Enter gap; Codex/OpenCode also use
the managed injection framing. Shell sessions and other keys retain raw pair
writes. Its `ok:true` proves PTY write only; it may prefill a composer or send an interactive key and
never returns a submission receipt. Never split command text and Enter across
two calls.

### MCP Tool: `session` Output

The `session` tool's `action=output` strips ANSI escape codes by default, returning clean text suitable for AI consumption. Pass `format="raw"` to preserve escape sequences (e.g. for terminal rendering). For managed peers, task results travel through `agent action=send`; raw session output is only the anomaly fallback when a child failed to send its result. The `action=list` response includes per session: `foreground_process`, `shell_state`, `agent_state`, `tuic_session`, and `is_caller`. `is_caller=true` identifies the managed PTY that owns the current MCP connection so an orchestrator does not close itself; it compares the caller's identity against the PTY that identity is bound to, not against the PTY id. `tuic_session` is the stable identity the tab persists, so a caller can address a session across restarts. `child_pid` and `foreground_pgid` are no longer serialized — no MCP action accepts a raw pid, so they only enlarged every list response. `background_work` and `standby` appear only when true. Optional values such as alias, display name, cwd, worktree data, process identity, and agent state are omitted when absent rather than serialized as `null`; `status` follows the same omission rule.

`Global overview: session action=list` — one call; no per-session `status` fan-out.

`shell_state` is observed PTY activity (`busy` or `idle`); it is omitted before
the first lifecycle observation rather than treating a newly spawned agent as
idle. It is not task completion.
For detected agents, `agent_state` is `starting`, `working`, `awaiting_input`,
`idle`, or `completed`. `background_work=true` keeps `agent_state=working` while
a meaningful agent descendant is alive even when `shell_state=idle` and the
composer is ready; persistent integration helpers are excluded. Completion
requires the explicit end-of-task
`suggest: [ ... ]` protocol marker. A quiet ready prompt without that marker
remains `idle`. Spawned-agent lifecycle mail uses `completed` for the same
marker and reserves `idle` for an unclassified ready state.

| Param | Default | Description |
|-------|---------|-------------|
| `limit` | `50` | Max scrollback rows, or source bytes for raw output. Whole logical lines and UTF-8 codepoints can extend a page beyond the limit |
| `format` | (text) | `"raw"` preserves ANSI escape codes |
| `since_cursor` | (none) | Forward page from a previous cursor (text: scrollback rows; raw: source bytes) |
| `from_line` | (none) | Absolute text scrollback row; omit to read the tail |
| `from_byte` | (none) | Absolute raw source-byte offset; omit to read the tail |

`session action=input` and HTTP `POST /sessions/:id/write` share the same raw PTY
bookkeeping: each write stamps `last_input_ms` and feeds the `InputLineBuffer`
so slash-mode tracking stays identical for MCP and remote web clients. When a
combined text + Enter request targets a prefill-only agent such as Codex or
OpenCode, MCP uses the legacy framed injection sequence (Ctrl-U, bracketed paste
for multiline text, a flushed scheduling gap, then CR). Other text/key pairs,
including Claude's established input path, retain raw pair semantics. This
legacy combined form remains write-only; new managed automation uses
`action=submit` for the bounded receipt.

Orchestrated PTYs accept an optional `pty_description` field on
`session action=submit` and `session action=input`. It is independent from
`last_prompt`: the former is a
short description of the assigned work written by the orchestrator, while the
latter is the last substantial user prompt submitted to the agent. Omit the
field to keep the current description; pass a string to replace it, or `null`
/ an empty string to clear it. `agent action=spawn` accepts the same field for
the initial task. Updates are emitted as `pty-description-changed` over both
Tauri events and `/events` SSE.

**Delta reads:** The non-raw output path returns a `cursor` field (monotonic scrollback position). Pass `since_cursor` on subsequent calls to receive only new lines since that position, avoiding full re-reads. The `total_written` field is kept alongside `cursor` for backwards compatibility. When `since_cursor` is provided, screen rows are excluded — only scrollback log lines are returned.

**Retained-output paging:** Every window reports `start_offset`, `oldest_offset`,
`has_more`, `next_cursor` (null at the end), and `truncated`. A truncated response
includes `continuation` with the exact `session action=output` request for the
next page, or for older retained output when the default tail omitted history.
For text, start with `from_line=oldest_offset` and follow `from_line=next_cursor`.
The legacy absolute/tail `cursor` remains the total scrollback position; use
`next_cursor`, not that snapshot cursor, to page. Delta `cursor` stops at the
returned page boundary. For raw output, start with `format=raw,
from_byte=oldest_offset` and follow `from_byte=next_cursor`; `since_cursor` also
accepts source-byte positions. Raw `cursor` is the returned page end and
`total_written` is the total original byte count, independent of redaction.
Explicit raw pages mask sensitive bytes with `*` using the complete retained
ring and terminal context before slicing, so even one-byte pages cannot
reconstruct a secret. Clean absolute and delta pages also discover secrets from the retained terminal
context, so a page containing only a multiline private key body stays masked,
including when its footer is still on screen and its header is in scrollback.
Tail snapshots retain the existing `[REDACTED]` format.
UTF-8 starts round down and ends extend to whole codepoints. Invalid PTY bytes
use lossy decoding without changing source-byte cursors. `data_length` counts
returned UTF-8 bytes, which can differ from the source range.

Buffers remain the only output store: text retains its configured scrollback;
raw retains up to 2 MiB. Output appended after a read is available on later
pages; this is not a frozen snapshot. A cursor older than `oldest_offset` starts
at the oldest retained position and adds `missed_count` (rows or source bytes).
Evicted output cannot be fetched. Exhausted/future offsets return an empty page.
The HTTP `format=mcp|mcp_raw` and remote MCP proxy use this same serializer.
Remote continuation notes also name the required `connection_id`.


### MCP Tool: `repo` — Worktree Create (Claude Code Agent Hint)

MCP `repo action=worktree_create` uses the same creation path as HTTP
`POST /worktrees`, including `base_repo` validation, stale-worktree recovery,
cache invalidation, `worktree-created` SSE/Tauri events, setup-script result
reporting, and best-effort copy-on-write warming of Git-ignored directories.
Every created workspace is a linked worktree: Git refs and objects remain
shared with the parent, while `instructions.warm_artifacts.warmed_directories`
reports how many ignored directories arrived warm. Parent tracked changes are
not copied. See `docs/api/http-api.md` § Create Worktree; both transports call
the same shared core.

The lifecycle event includes `creator_session` resolved from the MCP binding,
never from a caller-supplied session id, and the requested `spawn_session` flag.
Without a spawn request, the same-repository creator is reconciled from its
reported cwd. A root coordinator does not move into a worktree merely because it
created it. Other tabs and the active workspace selection stay in place.

When the MCP client identifies as Claude Code (detected via `clientInfo.name` at initialize time), the `repo action=worktree_create` response includes an additional `cc_agent_hint` field:

```json
{
  "worktree_path": "/path/to/repo__wt/feature-branch",
  "branch": "feature-branch",
  "cc_agent_hint": {
    "worktree_path": "/path/to/repo__wt/feature-branch",
    "suggested_prompt": "Work in the worktree at `/path/...`. Use absolute paths for ALL file operations..."
  }
}
```

This works around Claude Code's inability to change its working directory mid-session. The hint tells CC to spawn a subagent that uses absolute paths for all file operations (Read, Edit, Glob, Grep) and `cd <path> && ...` for shell commands.

Non-Claude Code MCP clients do not receive this field.

### MCP Tool: `repo` — Worktree List

MCP `repo action=worktree_list` returns each worktree's `branch`, `path`,
`kind`, optional `warm_artifacts`, and `lifecycle_status`. `lifecycle_status`
uses the same classifier as the sidebar and `repo action=worktree_lifecycle`.
Inventory and per-checkout inspection run on the blocking pool. Listing reuses
its inventory and the sidebar's ref-keyed integration proofs and cached GitHub
answers, instead of repeating unchanged integration subprocesses for every
checkout. GitHub evidence is cache-only on this request: a cache miss keeps the
conservative local classification, and normal monitoring can populate the existing
definitive-answer cache. Listing never starts a GitHub network lookup; this avoids
waiting on API pagination behind the native CLI response deadline. Fresh Git and
filesystem inspection can still take time, so this is not a hard wall-clock bound.
Dirt, fingerprints and submodule commit counts remain fresh; the
response fields are unchanged. If checkout HEAD differs from its branch ref,
listing classifies that HEAD freshly. Removal preflights remain uncached:

```json
{
  "feature-branch": {
    "branch": "feature-branch",
    "path": "/path/to/repo__wt/feature-branch",
    "kind": "worktree",
    "lifecycle_status": {
      "dirty_files": 0,
      "missing_checkout": false,
      "commit_status": "merged",
      "removal_safety": "safe"
    }
  }
}
```

`commit_status` is one of `merged`, `unmerged`, `in_sync`, or `unknown`.
`removal_safety` is one of `safe`, `requires_force`, or `unknown`. Both fall
back to `unknown` if inspection fails (`lifecycle_status.error` is set in that
case). This makes the same merged/dirty verdict `worktree_lifecycle` answers
per-branch available for every worktree in one call, so a caller does not
need to recompute merge state with its own git script.

### MCP Tool: `repo` — Worktree Remove

MCP `repo action=worktree_remove` returns `{ "ok": true, "warnings": [...] }` on full success. The warnings come from the same removal preview as desktop and HTTP, including branch history, live sessions, and local file counts. It
runs on the blocking pool and the local MCP bridge allows up to 305 seconds for
the response, covering linked worktrees with large ignored build artifacts.
Non-forced removal refuses staged, unstaged, or untracked work. The optional
MCP `force` boolean defaults to `false`; `true` is the explicit,
confirmation-gated authority to discard that worktree-only state. It does not
override a worktree lock or authorize deletion of unmerged commits. The
optional `delete_branch` flag defaults to `true` for a normal removal and
`false` when `force=true`; an explicit `delete_branch=true` still runs the
branch safety proof and may return a branch-retained warning. Unlocking a
locked worktree requires the separate `override_lock=true` flag and user
confirmation. Only an explicit lock override sends Git two `--force` flags.
Use `repo action=worktree_lifecycle` to obtain the fresh verdict and commit counts. The required `expected_fingerprint` binds force to that lifecycle snapshot; a
changed checkout status, HEAD, or submodule ref then stops removal. Initialized
submodule refs are preserved before Git removes the worktree.

When `delete_branch=true` and safe branch
deletion fails after a linked worktree is removed, the action still succeeds
with `branch_delete_warning` populated so clients can report that the worktree
was removed but the branch was kept.

### MCP Tool: `repo` — Branch Integration Proofs

`repo action=branch_integrations path=<repo>` returns an array for all local
branches, including branches without a worktree. `repo action=branch_integration
path=<repo> branch=<name>` returns one entry. Each includes:

- `branch`, `tip`, `default_branch`, `commit_status`, `integrated`, and `proof`;
- `archive_required`, `archived`, and `archive_ref` (`refs/archive/<branch>`);
- `worktree_paths` (including the main checkout when applicable), and `error`
  when classification cannot establish a verdict.

Proofs use the same classifier as workspace lifecycle, sidebar refresh, branch
panel and safe deletion. `ancestry`, `integration_ancestry`, `patch_equivalence`,
`noop_merge`, and `squash_message` are structural evidence. A squash message
must list every branch subject, either directly or as GitHub `* subject`
bullets, and a clean virtual merge must also leave the target tree unchanged.
`github_pr` retains the verified fetched-head proof. The `in_sync` status means
no own commits need integrating; it returns `integrated: false` and `proof: null`,
not a positive merge proof. The UI still excludes same-tip branches from its
merged list.

`content_superset` is a heuristic: exact same-subject twin patches ignoring
only context and hunk coordinates, or 100% of added lines present with their
multiplicity in the same regular files at the target tip. The latter rejects
deletions, changed modes, binary files and incomplete final lines. Neither
subject matches alone nor a partial percentage proves integration. An archive
at the exact current tip is required before deleting a branch with this proof.
`integrated` does not mean a checked-out or protected branch can be deleted.
Unknown verdicts never authorize deletion. The query itself changes no refs,
index or working files; Git virtual merges may write unreachable Git objects.

Cleanup tools must query this MCP surface instead of duplicating Git checks.
Refresh the query after archiving; deletion performs a fresh safety check.

### MCP Tool: `repo` — Local Branch Delete

`repo action=branch_delete` takes `path` and `branch` and returns
`{ "ok": true, "proof": "..." }` on success. It deletes only the local branch
ref; it does not remove a worktree or touch a remote ref. The branch must be
absent from every checkout and must not be the current integration or default
branch. The shared integration proofs described above apply. Content-based proofs
require an archive ref at the current tip (the primary `refs/archive/<branch>`
or its tip-suffixed variant). When the proof is `archived`, the response also
includes `archive_ref`, captured before deletion and naming the ref that holds
the deleted tip. An archived unmerged tip
also remains deletable through the existing `archived` recovery rule; that
does not change its integration verdict. Merge resolution changes are never
proved by `git cherry` or a same-subject twin comparison alone. The final
delete compares the ref against the proved tip and refuses a moved ref.

## Upstream MCP Proxy

TUICommander can proxy upstream MCP servers (stdio or HTTP) and aggregate their tools into its own `tools/list` response. Configuration lives in `mcp-upstreams.json`.

### Upstream configuration save contract

IPC `save_mcp_upstreams` and HTTP `PUT /mcp/upstreams` both accept
`{ base, config, credentials? }`: the snapshot the caller loaded, its desired result, and optional write-only secrets for newly added servers. The
backend indexes servers by stable `id`, derives the semantic base-to-desired
delta, and applies that delta to the latest on-disk configuration inside the
cross-process `ConfigFile` lock. It validates and atomically persists the merged
result while still holding that lock instead of replacing the file with a stale
UI snapshot.

Add-time credentials identify their new server by `server_id`. The locked merged config is validated before any vault writes. Existing vault entries are never overwritten by an add; failed partial writes or config persistence roll back new entries. The standalone credential endpoint defaults to refusing replacement and accepts `replace: true` for an explicit edit.

Absence has explicit semantics relative to `base`: omitting a former server from
`config` removes that ID, and omitting its former optional `auth` field clears
the auth value. Unchanged fields are not part of the delta, so concurrent edits
survive; in particular, a stale UI save cannot erase OAuth/DCR auth written
concurrently for an otherwise unchanged upstream. Concurrently added servers
also survive unless the caller independently adds the same ID, which is rejected
as a conflict.

Bearer, OAuth and secret-header vault entries store the intended MCP origin with the secret in one write. The HTTP credential resolver checks that binding before every request and before OAuth refresh; a mismatch sends no request and reports an origin error. Entries saved before origin binding require re-saving or authorization. Config saves cannot rebind existing credentials, even if auth metadata is cleared or an ID is replaced. The Settings form keeps its early origin-change rejection for a clear message: add an upstream with a different name for a different provider. Same-origin path edits retain auth metadata. OAuth refresh preserves the MCP origin captured at authorization, independently of the authorization server and RFC 8707 resource URLs.

Persistence returns the exact configuration immediately before and after the
locked mutation. Once the lock is released, `apply_config_diff` uses that exact
pair to disconnect removed or changed upstreams and connect added or changed
ones, so the live registry hot-reloads precisely what the atomic write changed.

The desktop boot thread owns the always-on Unix-socket/named-pipe listener and
its one-time background tasks for the lifetime of the process. A configuration
save may stop and replace the TCP listener, but the boot runtime remains parked
after that shutdown so dropping it cannot silently kill local bridge IPC.

PTY children receive `TUIC_SOCKET` from the server's actual bound Unix socket
path. This includes named `tuic-remote --instance` sockets and collision
fallbacks. `tuic-bridge` honors this environment override before instance-based
socket discovery, keeping child MCP calls on the spawning server.

### Stdio transport

`StdioMcpClient` spawns a child process and communicates via newline-delimited JSON-RPC over stdin/stdout. The handshake is: `initialize` → `notifications/initialized` → `tools/list`.

**RPC id-matching.** The `rpc()` method matches responses by JSON-RPC `id`, skipping any server notifications (messages without an `id` field) that arrive between request and response. This prevents silent "0 tools" when a server emits `notifications/tools/list_changed` or log messages during the handshake.

**Tilde expansion.** All user-supplied paths (`command`, `args`, `cwd`) are expanded via `crate::cli::expand_tilde()` before being passed to `std::process::Command`. This applies globally across the codebase — PTY, agent spawn, headless prompts, worktree scripts, plugin exec, and file validation all expand `~` to `$HOME`.

### HTTP transport

`HttpMcpClient` communicates via Streamable HTTP (POST to the server URL, `mcp-session-id` header for session affinity).

**Bearer credential generations.** `resolve_bearer()` re-reads the credential on
every request attempt so a completed re-authorization takes effect immediately;
the credential vault already caches the decrypted value process-wide, so this
does not repeat the OS keychain prompt. A 401 recovery passes the exact bearer
rejected by the server into the serialized refresh check. If storage now holds a
different valid generation, that credential is retried as-is; only the rejected
or invalid generation is refreshed at the authorization server.

### Health checker

A background task runs every 60s (`HEALTH_CHECK_INTERVAL`) and calls `tools/list` on every `Ready` upstream. Failures feed a circuit breaker (3 consecutive failures → backoff starting at 1s, capped at 60s, max 5 retries before permanent `Failed`). Recovery from `CircuitOpen`, `Connecting`, or `Failed` is attempted on each tick.

### Diagnostics

Both transports log `warn!` when `tools/list` returns a response without `result.tools` — making "0 tools" diagnosable instead of silent.

## OAuth 2.1 Upstream Authentication

When an upstream MCP server requires OAuth instead of a static Bearer token, TUICommander runs a full RFC 9728 (Protected Resource Metadata) + RFC 8414 (Authorization Server Discovery) flow with PKCE S256.

### Configuration

`UpstreamMcpServer.auth` is an enum:

```rust
enum UpstreamAuth {
    Bearer  { token: String },
    OAuth2  {
        client_id: String,
        scopes: Vec<String>,
        authorization_endpoint: Option<String>,  // None → discover
        token_endpoint: Option<String>,          // None → discover
    },
}
```

Missing endpoints trigger metadata discovery: the proxy issues an unauthenticated probe, follows the `WWW-Authenticate: Bearer resource_metadata=<url>` challenge to fetch `ProtectedResourceMetadata`, then resolves the authorization server's `.well-known/oauth-authorization-server` (falling back to OIDC `.well-known/openid-configuration` when required).

### Error → flow transition

`src-tauri/src/mcp_proxy/http_client.rs` emits a typed error:

```rust
enum UpstreamError {
    NeedsOAuth { www_authenticate: String },
    AuthFailed,
    Other(String),
}
```

A `NeedsOAuth` on any request transitions the upstream registry to `needs_auth`. The MCP page in Settings (Upstream MCP Servers) surfaces an *Authorize* button that calls `start_mcp_upstream_oauth`. Auto-triggered OAuth is gated behind explicit user consent (the confirm dialog shows the AS origin so the user can refuse an Authorization Server mix-up attempt).

**Off-domain authorization servers are never blocked.** MCP gateways, corporate proxies and hosted IdP tenants routinely serve AS metadata whose `issuer` and endpoints point at a different registrable domain than the MCP server — RFC 8414 §3.3 says the issuer must match the discovery URL, but refusing on that basis makes legitimate servers unusable. Discovery logs a warning on an issuer mismatch and continues; `start_mcp_upstream_oauth` returns `cross_domain_as: true` when the AS is off-domain, and the consent dialog switches to a `warning` kind naming the origin. The decision belongs to the user, not to a hard-coded gate.

### Flow

1. **Start** — `start_mcp_upstream_oauth(name)` generates a PKCE verifier/challenge (S256), mints an opaque `state`, records the pending flow in a DashMap keyed by state, and returns the authorization URL + AS origin. The upstream moves to `authenticating` only *after* the flow is recorded — the status must never claim "Awaiting authorization…" for a flow that does not exist.

   **Flows are not serialized.** Each one owns its `state` nonce, PKCE verifier and callback port, so concurrent authorizations share nothing. An earlier design held a single-permit semaphore for the whole browser round-trip: a second *Authorize* click then blocked inside `start_flow` for the full 5-minute timeout with no browser, no dialog and no error, and `cancel_mcp_upstream_oauth` could not release it because the queued flow had never reached the pending map. Do not reintroduce a shared permit here.
2. **Consent UI** — The frontend opens the URL via `tauri-plugin-opener` after user approval. The status bar and the Settings MCP page show "Awaiting authorization…".
3. **Callback** — The AS redirects to `tuic://oauth-callback?code=…&state=…`. The OS routes the deep link to the desktop app (`src-tauri/src/mcp_oauth/mod.rs` — `DEEP_LINK_SCHEME = "tuic://oauth-callback"`). The deep-link handler calls `mcp_oauth_callback(code, oauth_state)`.
4. **Exchange** — `TokenManager` posts code + PKCE verifier to the token endpoint, receives `{ access_token, refresh_token?, expires_in? }`, serializes into `OAuthTokenSet`, persists to the OS keyring (`mcp_upstream_credentials.rs` — structured JSON format with `"type": "oauth2"`), and transitions upstream to `connecting`.
5. **Refresh** — `TokenManager` is shared across every `HttpMcpClient` refresh path (unified per upstream); a semaphore serializes concurrent refresh attempts to defeat thundering-herd. `expires_at` uses a 60 s margin; `None` means "no known expiry — do not treat as expired". A 401 recovery carries the exact bearer rejected by the server into the serialized refresh check. If an authorization exchange wrote a different valid credential between the request and recovery, that generation is retried as-is instead of being immediately refreshed or rotated; only the still-rejected or an invalid generation reaches the token endpoint.

### Callback server lifetime

The localhost callback server outlives the flow it serves — flow timeout plus a 120 s grace period (`CALLBACK_SERVER_GRACE` in `mcp_oauth/commands.rs`). A *successful* callback still closes it promptly, 2 s after the response is served.

It used to shut down as soon as the flow left the pending map. A user who spent more than five minutes at the identity provider — MFA, password manager, account picker — then redirected back to a closed port and got the browser's own "can't connect to the server" page, with nothing saying the request had expired. Outliving the flow means `handle_callback` can answer a late redirect with a page naming the reason and the retry step.

### Expiry sweep

A background sweep (`spawn_cleanup_task`) drops pending flows past the 5-minute timeout and returns each one's upstream name so the registry can `rollback_authenticating` it back to `needs_auth`. Without that rollback an abandoned flow left its upstream on `authenticating` permanently, with no path to a retryable state.

### Cancel

`cancel_mcp_upstream_oauth(name)` drops the pending flow entry and resets the upstream status to whatever it was before the attempt (`disconnected` / `failed` / `ready`).

### Deep-link scheme

| Scheme | Purpose |
|--------|---------|
| `tuic://oauth-callback?code=…&state=…` | OAuth 2.1 authorization code return path for upstream MCP servers |

Registered at boot via Tauri's single-instance + deep-link plugins. The frontend listener routes callbacks to `mcp_oauth_callback` without exposing the code to the WebView console.

### Threat model

OAuth callbacks arrive on a loopback HTTP server bound to `127.0.0.1:0` (OS-assigned port), spawned per flow by `start_mcp_upstream_oauth`; the `tuic://` deep link is the manual fallback. Neither path is reachable from the network, so there is no adversary position from which a remote attacker can probe the pending-flow map, and state comparison uses a direct DashMap lookup (no constant-time compare).

## Inter-Agent Messaging

The `agent` tool's messaging actions (`register`, `list_peers`, `send`, `inbox`) enable coordination between multiple AI agents connected to TUICommander.
There is no separate `swarm` action; orchestration composes the `agent` and `session` primitives.

For `agent action=spawn`, `prompt` is always delivered. The per-agent
`prevent_alt_screen` setting controls the screen flag on every launch path,
including MCP spawn; there is no per-spawn screen override. MCP spawn rejects
the removed `allow_alt_screen` and `allowAltScreen` parameters.
`skip_trust_dialog` defaults to true for Claude and Codex MCP children. It is a per-agent setting, not an MCP parameter. Codex receives `-c projects."<canonical cwd>".trust_level="trusted"` for this launch, including when a custom launcher forwards its arguments; Claude's managed PTY answers only its exact startup trust question while **No, exit** remains selected. User-opened terminals and saved CLI trust files are unaffected.

Caller-supplied `args` that contain `{prompt}` remain authoritative and receive direct substitution.
Flags-only `args` keep their order; normal CLIs receive the prompt as the final
positional argument, while prefill-only interactive TUIs receive it through the
deferred PTY-injection path after their ready prompt appears.
Configured run-config argv retains its established authoritative behavior:
`{prompt}` is substituted where authored, otherwise the prompt is appended as
the final positional argument rather than converted to deferred PTY delivery.
Structured `model` is composed with `args`; a matching run config can supply a
default `model`, overridden by the spawn parameter. Existing `--model` in
run-config `args` stays authoritative and conflicts with an explicit model
parameter. Literal `codex` selects the persisted default run configuration,
including its editable approval-bypass argument. Only direct interactive Codex
defaults defer the task through PTY injection; wrappers and subcommands retain
run-config positional or placeholder task delivery. A Codex `exec`, `e`, or
`review` subcommand is recognized only as the first positional argument before
`--`, after skipping root options and their values. Profiles or models named
`review`, `exec`, or `e` keep interactive task submission. Direct executable identity
also selects Codex parser state when `agent_type` is omitted or disagrees.
Composition never restores a removed bypass. Wrapper configs receive
`launch_warning` because TUIC cannot validate their internal Codex flags.

The optional `env` map uses the same field name and string values as HTTP
`POST /sessions/agent` and desktop IPC spawn. Its values override run-config
environment values. TUIC applies `TUIC_SESSION` and `TUIC_PARENT` afterward,
so callers cannot replace peer identity. Environment values are redacted from
spawn logs.
Managed Claude peers default to `CLAUDE_CODE_TMPDIR=$HOME/Gits/.tmp/claude/`; TUIC creates this directory before spawn and preserves an explicit inherited, run-config or caller value.

`name` optionally assigns a non-empty peer and PTY display name at spawn time, pinned against later `intent:` and OSC titles.
The parent-assigned name is stored before prompt delivery, returned in the spawn
response, exposed as `name` by `agent action=list_peers` and as `display_name`
by `session action=list`, and preserved when the child later auto-binds its MCP
connection. This avoids making identity depend on the child successfully
executing a registration instruction in its initial prompt.
The session list's `alias` remains a separate repo-derived short address and is
not replaced by the display name.

The optional `pty_description` field on `agent action=spawn` populates the
orchestrator-owned task description shown above the PTY. When the caller's
orchestration schema cannot supply that field, spawn derives display-only
metadata from the normalized task prompt (capped at 160 characters); an
explicit string still wins, while `null` or an empty string explicitly keeps
the new PTY descriptionless. The inference never changes prompt delivery or
agent-specific launch semantics. Later `session action=input` calls may update
the same field without adding another command to the MCP surface.

Every managed child is registered server-side and receives an inbox immediately,
even when the caller has no bound peer identity. Spawn binds the child's PTY
to its own `$TUIC_SESSION`, so session-list rows show the child identity before
its first MCP connection or register call. A registered parent additionally
creates the bidirectional relationship: the child prompt receives its parent ID
and send instruction, while the spawn response returns `parent_session_id`. An
unregistered caller gets no `parent_session_id` and a warning instead of a false
two-way guarantee. `communication_ready`, `send_to` and `peer_registered` are gone:
the first two restated `parent_session_id`, and the third restated that TUIC always
registers a managed child.
The spawn still records the caller's MCP session as a pending parent: a later
`register` call links existing children to the stable parent UUID and migrates
any lifecycle notifications emitted before registration.

Deferred initial prompts use a one-shot internal watchdog. Successful PTY
submission removes the marker silently. A prompt still pending after 30 seconds
emits one `prompt_delivery_failed` message to the parent; there is no success
event, delivery polling, or public delivery-state machine.

Ordinary managed-agent PTY injection is allowed only when the recipient is idle,
its composer buffer is empty, and no confident question or approval is active.
Busy workers and recipients with partially typed input keep the message queued.
Clearing or submitting the composer rechecks the queue.

An orchestrator role is declared explicitly with
`agent action=register orchestrator=true` and removed with `orchestrator=false`; spawning a child never
infers or permanently grants the role. The declaration is returned by `register`
and `list_peers`. A `list_peers` entry carries `tuic_session`, `name` and
`orchestrator`, plus `alias` and `session_id` when the peer owns a live terminal —
the two fields that make an answer from `list_peers` directly usable as a `to` or a
`session_id`. `registered_at` and `mail_wake` are no longer per-entry, and the
top-level `count` is gone: the array's own length already reports it. Its routing is intentionally stricter: every peer and
child-lifecycle message remains in the authoritative inbox, and peer payloads
never enter its channel, active turn, pending-injection queue, or composer. An
active `agent wait` owns delivery and suppresses terminal wake. Without a waiter,
canonical `idle` or `completed` lifecycle may submit the payload-free notification
`[TUIC] message available — read it with: agent action=inbox`. A derived `working`
state may submit the same notice only when the existing composer-safety gate proves
the shell is idle, readiness is confirmed, and no question or partial input owns the
composer. This covers reconciled background descendants without weakening the
gate for an active turn. A pending process probe still waits for its existing
settlement retry before a wake is eligible.

One exception, and it is narrow: when *every* message in the reserved window is a
server-authored lifecycle notification (`tuic-auto-*`), the notice types those
events instead of pointing at them — `[TUIC] child agent 8c261794 is now idle;
child agent 8c261794 exited (exit 0)` — and acknowledges its own window, so the
orchestrator owes no `inbox` call for it (`delivery_path`
`lifecycle_summary_and_inbox`). A lifecycle payload is a state name generated by
TUICommander itself, so this exposes nothing a peer authored. A single peer
message in the window disqualifies the whole group back to the generic notice:
a partial summary would satisfy the reader and silently bury the rest. The
summary also falls back when it would exceed 240 characters. Mail that coalesces
*while* the summary is being typed falls outside the acknowledged window and
earns its own notice. Busy or otherwise non-quiescent working sessions,
awaiting-input, starting, missing, and unknown state fail closed to inbox-only.
Mail received while working remains eligible and is re-evaluated at the next
authoritative idle/completed transition. One pending wake covers later unread
mail through its logical inbox cursor. Inbox and successful wait observations
acknowledge the same cursor atomically with their snapshot, including an empty
snapshot, so a read cannot race a delayed wake assignment. A generic notice never
hides the underlying payload from a later wait. PTY I/O happens outside the
delivery gate. An ambiguous payload-free notice expires after five seconds and
may be retried once after the managed lifecycle reconfirms readiness. A second
uncertain result exhausts that unread-mail group's wake budget: later expiry or
idle/completed reevaluations, including newly coalesced mail, remain inbox-only
until a successful inbox or wait observation acknowledges the group. An attempt
that writes no bytes remains `NotStarted` and does not enter an automatic retry
loop: it exhausts the budget for that lifecycle, so nothing reclaims a composer
that just refused the claim. A *new* BUSY→IDLE edge re-arms the budget before it
chases the outstanding notice, because the lifecycle that refused is not the one
being retried — without that, one draft in the composer, one open question or one
unconfirmed idle would silence the wake for the rest of the session and the mail
would never be announced. A notice already in flight keeps its attempt; the
re-arm cannot take it.

`register` also returns `mail_wake`. Its only current non-`none`
value is `managed_pty_lifecycle`, derived from a live TUIC-managed PTY rather than
claimed by the caller. Headerless/external orchestrators have a mailbox and MCP/SSE
transport but no authoritative model lifecycle or host wake adapter capable of
starting a turn, so they honestly report `mail_wake: "none"` and must use
`agent wait`/`inbox`. MCP activity and SSE presence are not treated as idle proof.

The final injection decision atomically claims `idle -> busy`, closing the race
between observing a ready screen and writing to the PTY. Idle is published before
the queued-message flush, and each idle transition submits at most one queued
message; remaining messages wait for later turns. This keeps backend state and UI
events ordered and prevents lifecycle reports from overwriting an active composer.
Peer messages and Compose commands occupy one typed FIFO, so neither producer can
overtake an earlier accepted entry. The Compose queue count and clear operations
select only user-command entries; clearing them retains peer messages and their
relative order.

### Raw PTY Capture Diagnostics

`POST /diagnostics/capture` controls the off-by-default raw-stream tap used for
agent-state regression evidence. `{ "enabled": true, "session_id": "<id>" }`
records one session; omitting `session_id` records all sessions, and
`{ "enabled": false }` stops it. `GET /diagnostics/capture` returns the active
filter, output directory, and byte count per opened session. A new enable starts
fresh files, and each `<config dir>/captures/<session-id>.tcap` file is capped at
512 KiB. Records preserve direction, original read/write boundaries and monotonic
timestamps; legacy `.raw` fixtures remain readable as output-only captures.
Set `TUIC_CAPTURE_DIR` to an absolute path before starting the server to place
captures there instead. A relative value rejects activation with an `error`
field and leaves capture disabled.

Capture must be enabled before reproduction. `/sessions/:id/output` is not a
fixture-acquisition fallback: its bounded ring can lose a one-shot marker and its
JSON string is lossy UTF-8. Negative and positive captures belong in
`src-tauri/src/fixtures/agent_prompts/` and are replayed through the same raw plus
rendered-row composition as the reader thread.

The stdio bridge reads each IPC HTTP response through its declared
`Content-Length` rather than waiting for connection EOF. A single transport error
does not discard the current MCP identity; subsequent authenticated calls refresh
the session-to-terminal binding. Ordinary calls keep a ten-second read deadline;
direct and collapsed wait calls derive it from the clamped requested timeout plus
a five-second transport margin.

Focused `ui action=tab` requests using `tuic://open` or `tuic://edit` switch to
the registered repository that owns an absolute target path before activating
the native file tab. This keeps repo-scoped tabs visible in the tab bar instead
of rendering their content under an unrelated active repository. Background
requests (`focus=false`) do not change repository context.
Absolute files outside a registered repository and HTML/URL tabs use the MCP
caller's registered repository as their tab scope, even when another repository
is visible. A focused request switches to that repository; `focus=false`
preserves the visible repository. If the caller has no registered repository,
these tabs fall back to the currently active repository.
These tabs remain in the existing tab stores while another repository is
selected. Unpinned tabs reappear when the opening repository is selected again;
pinned MCP tabs remain visible across repositories. Unpinning restores the
opening repository scope.
Native Markdown tabs opened through `ui action=tab` survive UI document reloads
within the same window, including native recovery without a working unload
handler. The frontend snapshots tab changes synchronously and restores their
target, scope, pin state and selected tab from session storage; reopening the
same MCP id updates that tab.
Closed tabs are not restored. This does not persist tabs across app restarts.

Native file tabs use the MCP `id` as their identity, so distinct ids do not
collapse onto one file-path tab and repeating an id updates its target. The tab
bar also keeps repo-scoped tabs visible when a repository has no active workspace.

### Protocol

0. **Auto-identity** *(no call needed)*: TUICommander's Codex MCP entry explicitly whitelists
   `TUIC_SESSION` through `env_vars`; other supported clients inherit it from the agent PTY.
   `tuic-bridge` sends the value as the `x-tuic-session` header on the initialize `POST /mcp`. The server
   validates the UUID and binds the MCP session to that tuic session (`apply_initialize_identity`
   → the shared locked live-owner policy), auto-registering the peer. `agent action=register`
   becomes an optional rename. A client that scrubs the bridge's environment (ego launches stdio MCP
   servers with only its config's `inherit_env`) sends no header; on initialize the server then walks the
   `x-tuic-client-pid` ancestry for the nearest process holding a well-formed `TUIC_SESSION`
   (`identity_from_client_ancestry`) and binds that. An asserted header always wins. The same MCP session may refresh its binding, and a fresh session
   may reclaim a stale owner; a subscribed or recently active owner is not replaced but is joined,
   so a second bridge in the same PTY becomes routable instead of being locked out. An existing
   peer's display name is preserved. The bridge's eager initialize and the downstream client's
   proxied initialize reuse the same existing `mcp-session-id`; this prevents the bridge's own live
   SSE stream from being mistaken for a competing identity owner. External bridges without
   `$TUIC_SESSION` are not auto-bound at initialize.
1. **Register**: optional rename/project/role update for an auto-bound peer. Pass
   `orchestrator=true` to declare the role or `false` to remove it; omission
   preserves the current declaration. A headerless external
   caller may omit `tuic_session`; the server generates an MCP-scoped UUID that remains stable for
   that connection and does not create a PTY. Supplying an explicit UUID preserves identity across
   reconnects and retains the live-owner takeover guard, which now refuses only callers that hold
   no route to the identity — a bridge already joined to it is renaming, not taking over.
   When the announced UUID differs from the one already bound to the MCP session, the two are
   ranked by whether they resolve to a live PTY rather than merely compared: an identity backed by
   a terminal outranks one that is not. A caller that registered an invented UUID may therefore
   repair itself by announcing its real `$TUIC_SESSION`, while the reverse — wandering off a
   terminal-backed identity onto a fabricated one — is refused with an error naming the identity to
   use. Two identities that both lack a terminal keep the original "already bound to a different
   peer identity" rejection. A repaired identity carries over any mail buffered under the abandoned
   one and retires it, so `list_peers` stops advertising an address that can never be reached. The
   retire and the recipient check inside `send` share one identity lock, so a message aimed at the
   abandoned identity either arrives before the retire and is carried over with the rest of the
   inbox, or arrives after it and is refused with "is not registered" — it is never buffered under
   an address that is deleted a moment later.

   That carry-over only has an *implicit* trigger when the same protocol session rebinds. A caller
   that reconnects and registers a brand-new UUID arrives with no link to its old identity, so it
   must name it: `register replaces=<old_uuid>`. Identity is never inferred from a name or project —
   it decides who may read whose mail. The response reports the outcome instead of staying silent:
   `superseded_identity` plus `mail_migrated`, and — when the superseded identity still owns a live
   PTY — `mail_stranded` and an `identity_warning`. That last case deliberately moves nothing: an
   identity with a terminal is a reachable peer, and taking its inbox would strand a working agent.
2. **Discover**: `agent action=list_peers` returns all registered peers (filterable by path).
3. **Send**: `agent action=send to=<address> message="..."` buffers to the recipient's inbox.
   `to` takes the peer's `tuic_session`, its registered `name` (as `list_peers` prints it;
   two peers with one name are refused with the candidates), the id of the
   PTY it runs in, or that terminal's alias. `delivered` is the verdict and
   `delivery_path` is the single source of truth for the route and distinguishes SSE,
   terminal-or-queued, waiter, subscribed ACP inbox resource, generic/coalesced
   orchestrator wake, and inbox-only
   delivery. It replaced `delivered_via_channel` on this response, which reported only
   the SSE sub-route yet read as a delivery verdict — `false` next to a `delivered:true`
   and a confirming `delivery_path` was pure ambiguity. The field remains on the stored
   `AgentMessage` as in-memory forensics but is `#[serde(skip)]` — it reaches no MCP
   response at all, including `inbox`/`wait`. Emitting it to the recipient repeated the
   same trap: it is `false` precisely when a waiter or the terminal carried the message,
   and the recipient reading it is already holding the message it describes. The route is
   `delivery_path` for the sender and the `agent_msg` tracing line for the operator. When the recipient is a
   real managed PTY, `recipient_state` contains only its current `shell_state` and `agent_state`;
   external generated peers omit `recipient_state`.
4. **Receive** — the following paths surface buffered mail:
   - **Interrupting mail (explicit opt-in)**: `agent action=send urgency="interrupt"`
     buffers the message first. For Claude Code, Codex and ego, a safe working
     session receives one bare Esc. TUIC waits up to **10 seconds** for independently
     confirmed idle, then submits the full message as one input through the shared
     agent submit writer. An already idle session skips Esc. This explicit request
     also applies to registered orchestrators. Normal and urgent behavior is unchanged.
     A dialog/approval, draft, unknown foreground owner or uncertain earlier input
     prevents injection. A dialog or draft appearing during the wait aborts it.
     Unsupported agent types, missing PTYs and idle timeouts fall back to the existing
     urgent notice/inbox paths; a dialog or draft keeps the composer untouched.
     The response adds `interrupt_delivered` and `interrupt_fallback_reason`
     (for example `unknown_agent_type`, `dialog_open`, `composer_has_user_text`,
     `idle_timeout`, or `waiter_owns_delivery`). `delivery_path=interrupt_payload_and_inbox`
     means the complete input was written, not that the model accepted or acted on it.
     An active waiter receives the buffered body without an interrupt. A partial
     payload write is reported as `write_uncertain` and is not followed by a notice
     or automatic replay. **Risk:** an interrupted tool can leave partial work, and
     its child processes may continue; this is not a rollback or a process-tree kill.
   - **Urgent notice**: `agent send urgency="urgent"` retains the peer body only
     in the inbox. A busy managed Claude Code, Codex or ego session with a safe,
     empty composer gets a payload-free notice through the guarded PTY writer.
     The writer sends Enter, which Claude/Codex queue until the next tool
     boundary. ego also durably queues plain Enter while busy and delivers it
     after the running tool batch; Ctrl+Enter and Escape are not used. It does not interrupt the current tool. A draft, confident
     dialog, unknown agent type or exited PTY prevents this submission. The
     notice names the sender by validated UUID, never by a peer-controlled
     display name. The sender receives `urgent_delivered=true` for a notice
     write, coalesced unread notice, active waiter, or already observed inbox;
     `false` includes `urgent_fallback_reason` for the queued fallback. The
     receipt cannot prove the model acted. One unread notice per sender and
     recipient covers further urgent mail until inbox observation clears it.
     An unfinished notice reservation expires after 30 seconds so an abandoned
     writer cannot suppress later urgent sends. Completion of the expired
     reservation cannot settle a newer message's reservation; this lease does
     not retry the earlier payload or claim that the model consumed it.
     The recorded CLI contract, including unused immediate-send gesture
     findings, is in `src-tauri/src/fixtures/agent_mail/urgent_cli_probe_2026_09_27.json`.
   - **Channel push**: `notifications/claude/channel` is available to external Claude Code clients with an active SSE stream and no managed PTY. It is a best-effort notification; the inbox remains authoritative.
   - **ACP inbox resource**: a terminal-less peer with a live MCP-over-ACP connection subscribed to `tuic://inbox` receives `notifications/resources/updated` and can read its mail. Normal and urgent `agent send` report `delivered=true` and `delivery_path=acp_inbox_resource` when no waiter or other route owns the message; urgent send also reports `urgent_delivered=true`. This includes peers registered as orchestrators. After disconnect or without the subscription, the response reports `inbox_only` unless another route owns the message. An idle ego may also be woken by the ACP host.
   - **Normal PTY notice**: an ordinary managed agent receives a payload-free `agent action=inbox` notice when its composer is safe, or on its next safe idle transition. A busy turn, question, or partial draft is left untouched. Managed Claude peers use this path even with an active SSE stream, because a channel push during one turn cannot start a later turn for unread mail. An idle/completed orchestrator receives the coalesced inbox wake described above; so does a confirmed-ready, empty composer whose task state remains working only because of background work. A busy, questioning, or partially typed orchestrator is never queued or steered by normal mail.
   - **Inbox poll**: `agent action=inbox` — always the authoritative store.
5. **Wait** *(prefer over polling)*: `agent action=wait` blocks until new mail;
   `session action=wait session_id=<id> until=idle|exited` blocks on a peer's lifecycle. The default
   is 60 seconds and the advertised cap is 300000 ms — a request at or above that runs as 295000 ms.
   The 5-second margin exists because a client aborts its own `tools/call` on its own deadline, and
   at least one shipping client (Codex) uses exactly 300s: a wait running the full cap would answer
   right on that deadline and return a client-side error instead of `{timed_out:true}`, making the
   advertised maximum unusable. The clamp is deliberately client-agnostic — a per-client table would
   need maintaining against every client release, and ending 5s early is invisible to the caller.
   Agent-wait success preserves
   `{met,timed_out,new_messages}` and directly includes every retained fresh message (up to the
   100-message inbox capacity) plus `next_since`, in chronological order. Per-recipient logical
   unix-millisecond cursors make equal-clock-millisecond bursts safe.

   **The cursor is kept server-side.** `since` is optional on both `wait` and `inbox`: omitting it
   resumes from the caller's stored read position (`agent_read_cursor`), passing it overrides, and
   `since=0` stays the deliberate "replay everything" escape hatch — a replay never rewinds the
   stored cursor. `next_since` is now returned on *every* response, timeout included, falling back
   to the stored position when the batch is empty. Previously it was omitted whenever there were no
   messages, which left a timed-out waiter with `since=0` as its only recoverable value and made it
   reload the whole history on the next call. Wait never consumes the
   authoritative inbox; unread FIFO evictions are reported by `missed_count` on inbox reads.
   Both wait actions sleep on inbox or per-session lifecycle events; they do not run an internal
   polling loop. `session action=wait` resolves in three steps, in this order:

   1. **Already met?** Answer straight away. `until=exited` reads the exit-code tombstone, which
      outlives the session: `mark_session_exited` records the exit code and *then* drops the
      `sessions` entry, so a just-reaped child satisfies the wait while failing the liveness check
      below. Checking liveness first turned the one outcome `until=exited` exists to report into
      `Unknown session`. This step subscribes to nothing.
   2. **Live session?** Otherwise `session_id` is validated against the live session registry and an
      id that is not a real session gets `{"error": "Unknown session …"}` immediately — subscribing
      creates the per-session broadcast channel for whatever id it is given, and teardown only reaps
      ids that were real sessions.
   3. **Subscribe, then re-read state**, so a transition landing between step 1 and the subscription
      is still seen and no wake is lost.

Low-risk response compaction also omits an absent peer `path` from `list_peers` and an absent
`parent_session_id` from standalone spawn responses. Proxied upstream tool payloads are unchanged.

Blocking waits and terminal wake-up use a per-recipient delivery lease. Each
message is atomically assigned to at most one wake-up owner: an active waiter,
or PTY delivery. An external-client channel push does not take the lease: it is a best-effort
notification, so the message stays available and a later
`agent action=wait` or `inbox` returns it once more. Dedupe on
`meta.message_id`. The deadline path performs its final inbox check while
releasing the lease, and cancellation hands unobserved waiter-owned messages
back to terminal delivery. This removes both duplicate inbox+terminal turns and
the missed-wake race at the wait timeout boundary; inbox visibility itself is
unchanged and remains backward compatible.

If a terminal-owned message cannot be delivered before a PTY disappears, the
server re-queues that same message with a fresh logical cursor while preserving
its `meta.message_id`. This lets a later omitted-`since` wait recover it even if
it had already returned newer mail; recipients deduplicate the replay by
`meta.message_id`.

The inbox retains up to 100 messages per recipient. A new server-authored
lifecycle notice replaces the older notice for the same child and `type`, except
an `awaiting_input` state change: each separate question stays available to the
parent. The newest coalesced state stays at the end of the inbox, and replacement
does not increase `missed_count`. Other messages keep FIFO order. Every send
succeeds once the recipient is valid; at capacity without a matching notice,
the oldest retained message is evicted. `missed_count` on the next inbox read
reports evictions of unread mail; replacement and reclaiming mail already read
do not increase it.
An inbox read returns the oldest messages after `since` first. With no `limit`,
it returns all retained fresh mail (up to 100). With `limit`, the server clamps
the page size to 1–100 and returns `has_more=true` while newer unread mail
remains. `next_since` advances only through the returned page; omit `since` on
the next call to continue from the stored cursor, or pass `next_since` explicitly.
Reading a queued terminal message settles its delivery claim; when no pending
terminal-owned mail remains, the queued generic wake is removed. An evicted
message also releases its delivery claim and any urgent notice reservation that
no longer covers retained mail.

The server never infers orchestrator role from child spawn, peer name, prompt, MCP
activity, or SSE presence. Registration is the sole declaration seam. Wake
capability remains server-derived: without a live managed PTY and its canonical
lifecycle, an explicitly declared external orchestrator is inbox/wait-only.

Spawned peers additionally auto-post a `state_change` (`idle` / `completed` / `exited`) to the
parent's inbox. They use the same waiter-or-generic-wake orchestrator routing and never inject the
state payload into the parent composer. These notifications carry state only, never task
output. Each child must send its result or blocker with `agent action=send`; `session action=output`
is reserved for diagnosing the anomaly where that result message never arrived.

### Channel Push Delivery

An external Claude Code client with an active SSE stream (`GET /mcp`) receives `notifications/claude/channel` JSON-RPC notifications. A managed peer with a PTY uses the payload-free terminal wake, including when it is working. Registered orchestrators also keep peer payloads in the inbox. For messages over 200 bytes, the SSE `params.content` is a pointer under 300 bytes: the same inbox wake line as a PTY peer, followed by the validated sender UUID, message ID, byte count, and up to 80 UTF-8 bytes from the first line with control characters removed. The full body remains in the inbox. Messages of 200 bytes or less may be sent inline. The peer-controlled display name is never used in SSE notice text:

A channel notification is transport delivery, not proof that the recipient read the message. It stays unowned in the delivery lease, so the recipient's next `agent action=wait` still returns it. Managed peers instead reserve a terminal wake until the inbox read cursor passes the message; the notice never contains peer payload text.

```json
{
    "jsonrpc": "2.0",
    "method": "notifications/claude/channel",
    "params": {
        "content": "[TUIC] message available — read it with: agent action=inbox\nfrom 550e8400-e29b-41d4-a716-446655440a01 id 076546d8-80b0-4fa1-965f-1e31366e3506 10240 bytes: Large report",
        "meta": { "from_tuic_session": "550e8400-e29b-41d4-a716-446655440a01", "message_id": "076546d8-80b0-4fa1-965f-1e31366e3506" }
    }
}
```

This requires the client to be launched with `--dangerously-load-development-channels server:tuicommander`. The server declares `experimental.claude/channel` in its capabilities. Spawned Claude Code agents get this flag automatically.

### Limits

- Max message size: 64 KB
- Inbox capacity: 100 messages per agent (FIFO eviction; sends remain accepted)
- Peer registrations cleaned up on MCP session delete and TTL reap, except where
  the identity is still addressable — see below

### Identity outlives its transport

The 1h TTL reaper evicts an MCP protocol session nobody has used for an hour. A
peer identity is a different thing: it is the address other agents `send` to,
and `refresh_mcp_session` re-asserts it on the owner's next request. The reaper
rechecks `last_activity` before deleting each selected session. That check and
route cleanup share the identity lock with a refresh that must recreate missing
metadata; a session refreshed after the sweep's snapshot remains intact.
When a session is reaped, the server also drops its routing entry, reverse
route, and message broadcast sender; if another bridge still serves the
identity, it becomes the delivery owner. These small per-session allocations
must not survive a reap.
They do not account for the 27.3 GB malloc growth observed during the 2026-09-28
initialize storm; that allocation source and its triggering ego operation loop
remain under investigation. The reaper used to delete both session and identity
together, which broke agents that were still running:
`last_activity` only moves on an MCP request, so an agent that spends more than
an hour on one turn without calling a TUIC tool had its address deleted while it
was mid-turn. Its children's handoffs then failed with `Recipient '<uuid>' is not
registered`, and for a headerless caller that failure is permanent — re-register
mints a fresh UUID and nothing tells the child what it is.

The reaper therefore keeps an identity that is still addressable:

- it owns a live PTY (`live_pty_for_peer`), or
- a live session still records it as its parent (`session_parent`).

Both are bounded by live sessions, so retention cannot grow without bound; an
identity with neither is genuinely unreachable and is still evicted. The
retained ids are logged at `info` on each reap. The rule is
`AppState::peer_identity_is_reapable`, applied by
`evict_peers_for_reaped_mcp_session`.

## Authentication

When remote access is enabled:
- Basic Auth with username/password
- Password stored as bcrypt hash in config
- Session token, relay token, and VAPID private key stored in the OS keyring-backed credential vault
- Applied to all endpoints
- Mobile app: unauthenticated page navigations redirect to `/mobile/login`, which posts to `POST /auth/login` (same rate limit and bcrypt admission as Basic); cookie is sliding, 30 days by default — see `docs/api/http-api.md` "Authentication"

When MCP-only (localhost):
- No authentication required
- Localhost binding only

## Security Model

- **Default:** Local IPC (Unix socket or Windows named pipe), with filesystem/user access controls and no HTTP credentials. The TCP listener is opt-in.
- **HTTP authentication:** Every protected TCP request needs the existing URL token, session cookie or Basic Auth, including loopback and LAN clients. The legacy `lan_auth_bypass` preference no longer bypasses HTTP authentication. Login assets and CORS preflight are public; the headless health probe remains public.
- **Request boundary:** Before authentication, every TCP request validates one Host authority (localhost, loopback/private literal IP, actual local interface IP, or the detected Tailscale FQDN). Missing, duplicate or foreign Host is rejected with 403. An Origin must be an exact bundled WebView/Vite origin or the HTTP/HTTPS origin of that validated Host. Foreign and opaque origins are rejected with 403 even with valid credentials. Cross-site browser requests are refused except from the explicit bundled/development origins. CORS uses the same origin policy and never a wildcard. Native clients without Origin still authenticate. Daemon Tailscale detection refreshes every 30 seconds through the desktop's shared state-publication path; the Host guard reads the current state for every request, so late startup, node renames and daemon stops update trust without a server restart. With detection completing within its 500 ms budget, acceptance after Tailscale becomes ready takes at most 30.5 seconds (plus scheduler delay). A timed-out detection publishes an unavailable state and retries on the next cycle; repeated CLI failure/timeouts cannot guarantee acceptance. Daemon TLS remains controlled by its explicit manual/off configuration.
- **Compression:** Gzip and Brotli via `CompressionLayer` (responses >860 bytes, auto-negotiated). SSE and WebSocket excluded by `DefaultPredicate`
- **No TLS:** Intended for local network use; use SSH tunnel for remote
- **Loopback-only session actions:** `session create`, `submit`, `input`, `kill`, `close`, `pause`, and `resume` are restricted to loopback connections — a non-loopback (remote/LAN) MCP client cannot pause/resume sessions, write to PTYs, or spawn/destroy sessions (those remain read-only: `list`, `output`, `status`)
- **Remote `/fs/read-editor*` cap:** Remote clients receive the standard 10 MB file-read cap on `/fs/read-editor` and `/fs/read-editor-external`, not the 250 MB local cap (`MAX_EDITOR_LARGE_FILE_SIZE`). The local (loopback) router routes these paths to the large-cap handler; the remote router routes them to the standard-cap handler to avoid OOM/latency over metered links (see `build_remote_router` in `src-tauri/src/mcp_http/mod.rs`)
- **Markdown link resolution parity:** `POST /fs/resolve-markdown-link` and the `resolve_markdown_link` Tauri command call the same Rust resolver. It returns only target metadata, rejects UNC paths before probing, and preserves local symlink targets outside the registered root.
- **Mobile Markdown images:** `GET /fs/markdown-image?repoPath=...&file=...` serves image files up to 10 MiB through the shared authenticated router. It canonicalizes the selected repository and requested file, then refuses a file outside that repository, including escapes through symlinks.
- **Traversal gate on absolute-path fs routes:** `/fs/read-external`, `/fs/read-editor-external`, `/fs/write-external`, `/fs/copy-abs`, `/fs/move-abs` and `/fs/transfer` (its `destDir`) share `deny_unless_in_roots`, which rejects `..`, NUL and relative paths before the lexical `Path::starts_with` containment check. Without that first layer, `/repo/../../etc/passwd` passes containment by components while the OS resolves it outside the repo. These routes are in `shared_routes()`, so they are reachable from the remote router too. Paths are intentionally not canonicalized — symlinks placed inside a registered repo are an accepted design decision
- **Anti-hijack guard on `agent register`:** A non-loopback caller cannot register as an existing live TUIC session — the `register` action (along with `list_peers`, `send`, `inbox`) is restricted to loopback connections, preventing a remote client from injecting messages into another agent's context (see `mcp_transport.rs`)

## Browser Mode Integration

The frontend's `transport.ts` maps all Tauri commands to HTTP endpoints:

```typescript
// In browser mode:
invoke("create_pty", { config }) → POST /sessions { config }
invoke("get_repo_info", { path }) → GET /repo/info?path=...
```

PTY output in browser mode uses WebSocket instead of Tauri events.

## Mobile Transport

The mobile companion UI (`/mobile`) uses the same HTTP/WebSocket infrastructure as the desktop browser mode:

- **Session polling**: `GET /sessions` every 3s, enriched with `SessionState` (question, rate-limit, busy, agent type)
- **Real-time events**: SSE via `GET /events` for session create/close notifications
- **Live output**: WebSocket to `/sessions/{id}/stream` with JSON framing (`output`, `parsed`, `exit`)
- **Input**: `POST /sessions/{id}/write` sends text to PTY (used by quick-reply chips and command input)
- **History**: `GET /sessions/{id}/output?format=text` fetches initial ANSI-stripped output buffer
- **Mirrored sessions**: the desktop server routes session-scoped HTTP calls and WebSockets to the connected owning daemon. It replaces the phone's credential with the owner connection token and returns 503 when that connection is unavailable. The daemon itself serves only local PTYs.
- **Progress projects**: authenticated `GET /progress/projects` reads the local journal's project names for the phone's selector.
- **Activity**: `GET /config/activity` returns the persisted array; the Activity tab hydrates that array when opened.

The mobile entry point shares `transport.ts` and `invoke.ts` with the desktop — no mobile-specific transport code.

## Secret form security boundary

`secret` supports `request`, `run` and `remove`; no action reads values. The
backend owns the zeroizing store and process spawn. Values are shared by name
across that backend's callers, not keyed by MCP connection or TUIC session;
MCP disconnect/reconnect leaves them intact. `remove` returns the requested
`names` and `status: "removed"` after idempotent deletion, even for absent names.
Older builds returned `missing` to describe the post-deletion state, not a lookup
failure. `run` still returns `missing` when a requested value is unavailable.
Native-window bootstrap
privately distributes a per-form capability. Browser entry uses
`/index.html#/secret-form?nonce=<capability>` on the existing application router,
with the same authentication and transport policy. Requests require a desktop
host. There is no separate server, TLS detection or origin isolation: an
application-origin service worker can observe entry, and HTTP is not restricted
to loopback by this feature. Boss accepted these limits on 2026-10-03.

One shared pre-call gate blocks native `ui`/`debug`, direct upstream
`tools/call`, and `call_tool`-wrapped inspection while a form is open. Results
of already-running inspection calls are not withheld. The HTTP debug-JS handler
also checks the form gate. The form mounts through the common frontend entry
without starting App/debug/logging/terminal initialization. Run captures pipes,
caps output and masks before serialization; it never writes raw output to
logging or PTY paths. Consent templates match exact argv, names and directory.

## Configured remote MCP ownership

The desktop's native MCP session list includes the Rust remote mirror. Remote rows carry
`connection_id` and `address` (`connection_id/session_id`). Session output and submit
resolve that owner and use its configured HTTP URL and token. Output uses the daemon's
native MCP cursor, redaction and exited-session contract through `format=mcp` or
`format=mcp_raw`; submit uses the authenticated semantic submit endpoint.

Peer discovery includes local and connected remote registries. Use a returned peer
`address`, or pass `connection_id` with `to`. `local/id` addresses the desktop.
The desktop opens an authenticated `/mcp/peer` WebSocket to each configured daemon.
This duplex mail link carries register, list_peers, send, inbox and wait only; process
creation is excluded. A daemon sends to the desktop or another daemon through the hub.
The authenticated link determines sender provenance. Destination delivery reuses native
inbox and wake handling, preserving the message body in the inbox.

Local mail survives hub loss. Cross-host failure names the connection; uncertain
acknowledgements must not be retried blindly. Lifecycle mail retains its message identity
in the bounded native outbox until acknowledged. Reconnect retries those notices without
duplicating an already retained destination message.

Peer handshakes serialize per configured connection, so a mute daemon cannot hold
mail calls to another host behind its network deadline. Session targets reject empty
ids/prefixes before owner selection. Forwarded notice deduplication survives inbox
reads: each registered recipient keeps a FIFO ring of the last 100 forwarded
message ids and fingerprints, shared across senders, without retaining message
bodies. A retained id with a different sender or body is rejected as an identity
collision. Recipient unregister removes that recipient's ring; sender retirement
does not. Beyond the last 100 ids, a retry may be delivered twice, including when
another sender's burst pushes its id out of the ring. There are no global budgets,
host quotas or sender eviction rules.

Disconnect retires that host's existing shadows synchronously, independently
of a pending handshake or a later reconnect generation. The registered-recipient
check and enqueue remain atomic with recipient retirement. This is a bounded
replay horizon, not unbounded or restart-persistent exactly-once delivery.

## Telegram adapter groundwork

The headless `telegram` tool exposes `register`, `unregister`, `begin`, `activity`, `finish` and `send` through the normal MCP registry and dispatch. Registration uses the caller's MCP-bound TUIC identity, replaces the previous agent with one native mail and is never persisted. MCP session end, PTY close or foreground-agent exit retires it. Without an explicit owner, the latest-connected live AI Chat peer is the fallback for inbound text and may use the reply tools. Explicit registration always wins. Connecting another AI Chat changes only future inbound delivery; existing requests keep their original reply peer. Request-specific reply tools require that peer’s MCP binding. Explicit registration takeover revokes pending requests. Other callers must register before outbound actions. Authorized text with neither a registered owner nor a live AI Chat peer gets "No agent is listening. Ask an agent in TUICommander to register for Telegram." and is dropped; strangers stay silent. Native stable-ID inbox/wake delivery remains authoritative. See [the Telegram design](../design/telegram-channel.md).

Native remote health, authentication, session-list, and SSE clients strip request URLs from reqwest errors before publishing them. Token-authenticated native HTTP requests send the existing `tui-session` cookie header rather than a query token; browser WebSocket query authentication is unchanged.

Workflow operator authority is selected by the host: verified HTTP credentials grant Human, unauthenticated loopback grants LocalApi. Request JSON cannot select the actor. Workflow policy writes and human decisions, plus administrative story transitions, require Human authority.

The daemon executor now owns recovery and duration timers under an OS run-database lock. Reads never recover live work, and a non-owner daemon refuses run mutations. Graph resume uses `resume_graph {execution_id,activation_id,resolution}` with an explicit pending activation; status-only resume cannot bypass graph position. Graph start controls, Agent effects and delivery policy remain unavailable until their later slices.

## Launch receipt read

`GET /sessions/{id}/prompt-receipt` returns the same live receipt as desktop `get_prompt_receipt`. MCP initialize captures the served instructions after peer auto-binding. Protocol metadata retains bounded responses during PTY registration; PTY metadata retains its copy after protocol reaping. Sessions without a captured launch brief still retain served MCP instructions and show an explicit launch-unavailable section. No additional store or settings reconstruction is used. See [HTTP API](../api/http-api.md#launch-instruction-receipts).

### Inbox consumption audit

Every successful `agent action=inbox` read emits an INFO tracing event with `source="agent_msg"`, `event="inbox_read"`, `caller_session_id` (the MCP protocol session), `caller_peer_id` (the bound TUIC peer), `inbox_owner` (that peer), and `message_ids` (a JSON array of returned ids). Empty reads emit an empty array. No message bodies are logged. The audit does not grant access to another peer's inbox: caller and owner remain bound by MCP registration. Read `/logs?source=agent_msg&level=info` and filter `event=inbox_read`.

Conversation-specific launch is available through `POST /acp/chat/open`; see the HTTP API guide. It creates an ACP peer, without a terminal tab.

### Agent-declared worktree placement

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
Only the caller’s tab moves to the declared workspace, without changing shell cwd.
Sibling tabs and an inactive selection stay unchanged. If the caller is active,
its selected workspace follows the same tab. Retrying the declaration is idempotent.

The stable `TUIC_SESSION` association is stored in the owning repository's
`declaredWorktrees` map using the existing locked repository delta. Any saved
caller snapshot moves to the target workspace; sibling snapshots are preserved.
After a restart or WebView reconnect, HTTP `GET /sessions` and IPC
`list_active_sessions` return the declared `worktree_path` and `worktree_branch`
while `cwd` remains the reported shell directory. The frontend restores tab
placement from the persisted stable `TUIC_SESSION` declaration across registered
repositories, independent of the current cwd-derived owner, when its workspace id
and checkout path still match. Declaration does not acquire
worktree cleanup ownership.
A concurrent repository edit can return a configuration conflict; retry the
declaration after refreshing rather than overwriting that edit.

The association owns display placement during later shell cwd notifications,
reconnect and saved-tab restore. A removed or replaced checkout invalidates it.
Worktree removal checks each terminal’s current cwd, not its declaration or sidebar
membership.

**Intentional request transport exception:** `declare_worktree` is an MCP-only
action, callable over the existing HTTP `POST /mcp` route with a bound caller.
There is no Tauri command or `COMMAND_TABLE` entry: window IPC has no managed
agent caller binding. Its push event is dual-emitted over Tauri and `/events` SSE,
and session-list response fields are identical over IPC and HTTP. Schema and
serialization regressions cover these shared contracts.

### MCP Tool: `automations`

Saved run configurations are validated against the same named-profile lookup used
by the runner. Names are case-insensitive; a bare agent type such as `claude`
is not a saved profile unless a profile has that name. Create, definition
updates and re-enabling reject an unresolved name with a repair hint listing
valid saved profiles. Existing list rows retain invalid definitions, but expose
`run_config_error` and return `next_run_ms: null` until repaired. A profile can
be removed after saving; launch checks it again and records a failed run.

Failed runs send the bound creator one `automation_run_failed` inbox message
with run/definition ids and reason, including pre-spawn failures. The existing
peer mail route supplies payload-free wakes; SQLite claims the `owner` channel
before delivery, so repeated publication and restarts do not resend it. A
missing creator registration leaves delivery unconfirmed; bell behavior stays
unchanged for definitions without a creator.

The native `automations` tool accepts the same top-level action object as
`automation_action` IPC and the `input` in `POST /automations/action`.
Actions include `list`, `get`, `create`, `update`, `pause`, `resume`, `delete`,
`run_now`, `run`, `list_runs`, `summary`, `preview`, `preview_definition` and `preset`.
See the HTTP API Automations table for fields and results. List returns rows
with `definition`, `next_run_ms` and `last_status`; create/update return the saved
definition. Update accepts a full definition (optional id), or id and enabled.
Errors use `{ "error": "..." }` with the identical backend validation text.

A bound agent can create a one-shot in one call:

```json
{"action":"create","definition":{"name":"Recheck","prompt":"Check again","in_secs":3600,"run_config":"Your saved profile"}}
```

The host resolves the elapsed delay from its own UTC clock. It defaults repository
from this caller's live cwd, ACP root or registered project, never another tab.
`run_config` must be explicit: current caller metadata does not retain the actual
saved account/model profile. The host never substitutes the agent type's default
profile. An omitted profile returns `{error,code:"caller_profile_unresolved",field:"run_config"}`.
Generated IDs combine the name with a SHA-256 hash of caller identity and the
original request before defaults and clock resolution. Repeating that request
returns the first saved occurrence, including when caller cwd has changed or is
no longer available. Concurrent retries share one locked insertion. To create a
separate identical reminder, supply a new explicit `id`. Explicit-ID duplicates
still fail. Other defaults are existing workspace,
enabled=true, grace_secs=43200, overlap=skip and max_duration_secs=3600.
Create returns the saved fields plus `next_run_ms` and the resolved
`once_local`/`timezone`; no date, list or get call is needed. `in_secs` must be a
positive integer and cannot accompany cron or `once_local`. The stored `once_at`
preserves the exact UTC instant across DST folds. Preserve it for unchanged
schedule edits, or clear it when changing the schedule. Computed `next_run_ms` is
read-only and is ignored when a reply is passed to update or preview. The MCP
schema declares this field as a read-only nullable integer so a saved reply also
passes the closed-object update/definition-preview property boundary.

MCP creation requires a bound agent session. The host sets `created_by_session`;
input cannot impersonate a creator, and updates preserve the original creator.
Pause/resume edit only `enabled` under the store lock. Deleting a definition
retains run history. `run_now` uses the same runtime owner and refusal rules as
HTTP/IPC. All adapters call `automations::api`; MCP does not parse a second
action enum or implement scheduler logic. The MCP endpoint is `POST /mcp`.

`tuic automations '<action JSON>'` (or `tuic automations -` for stdin) forwards
unchanged arguments over HTTP-over-IPC MCP into this same API. CLI failures
print the backend message on stderr and exit nonzero; successes print JSON.

### Automation HTTP/IPC adapter

The shared desktop/headless router exposes `POST /automations/action` with
`{input:{action:...}}`. `automations::api::execute` is also the Tauri command
implementation; stores, schedule rules, aggregates and execution ownership stay
in Rust. `COMMAND_TABLE.automation_action` maps the same input without renaming
fields. Route snapshots and registered-route PATCH probes cover this route.
MCP and CLI use this same action parser, runtime owner and result contract.
HTTP and MCP/CLI validate static automation inputs with the same advertised
schema before dispatch. HTTP returns status 400 with the identical structured
`invalid_arguments` payload, including bounded, value-masked violations and
`effect=not_started`. Semantic application error messages still match IPC
rejections and MCP/CLI errors; IPC retains its string-error signature.
MCP additionally requires a bound creator identity for creation.

Automation precheck context (#1631-45bf): definitions can opt in with
`precheck.inject_output: true` (default false). Proceeding checks prefix the
literal prompt with delimited, escaped untrusted stdout capped at 16 KiB;
truncation is marked and stderr is excluded. Empty output and manual bypasses
inject nothing. The run ledger records `precheck_context: {injected, bytes}` for
the prepared launch prompt, counting escaped stdout bytes before framing/marker.

PR line review routes (`/repo/pr-line-threads`, `/repo/pr-open-threads`, `/repo/pr-thread-comments`, `/repo/pr-thread-resolved`, `/repo/pr-thread-reply`, `/repo/pr-draft-review`) share account resolution and backend behavior with their desktop commands.

The PR action routes (`/repo/pr-action-state`, `/repo/pr-auto-merge`, `/repo/pr-merge-queue`, `/repo/pr-merge-or-enqueue`, `/repo/pr-rerun-failed`) share account-aware backend logic with IPC. Successful rerun means accepted, not completed.

## Core accounts owner surface

IPC `account_action` and HTTP `POST /accounts/action` share one Rust service and presence-only replies. Both HTTP routers require loopback or authenticated API access. The Settings MCP adapter can consume the owner service/schema after integration; account actions are not a new standalone MCP tool or an unrestricted plugin invoke grant. See [Shared account registry](github.md#shared-account-registry).


Automation worktree cleanup (#1623-8b2a): Automation runs retain automation/run IDs and the exact created workspace path and ID in their saved snapshot, including after definition deletion. Worktree cleanup is explicit and limited to final runs. The backend resolves the saved identity and rechecks shared Git integration/archive proof, dirt, submodule commits and live sessions; it never forces removal or closes sessions. Shared `automation_action` actions `worktree_preview` and `cleanup_worktree` accept a saved run `id` on IPC, HTTP, MCP and CLI.
Automation templates (#1622-be11): the shared automation action accepts
**templates** (catalogue) and **template** (draft). Draft inputs are template_id,
repository, run_config and optional timezone (host local by default).
Drafts have fresh ids and start disabled; no definition or run is created until
an explicit create action. These actions match across HTTP, IPC, MCP and CLI.

### Design Mode lifecycle tool

The `ui` tool exposes `design_start`, `design_stop`, `design_status`, `design_reload`, `design_navigate`, `design_screenshot`, `design_errors` and `design_eval`. These actions delegate to the existing Design Mode owner: the caller must own the armed repository for browser commands, remote hosts still require an extension host grant, and JavaScript still requires its optional host grant. A saved `design_mode` denial migrates to the `design` capability and still blocks these actions; disabling `ui` blocks all UI actions. The separate `design_mode` tool is removed without an alias. Start accepts `session_id`, optional `browser` (`chrome` or `edge`) and optional absolute HTTP(S) `url`. Session references resolve like the session tool. Stop accepts `repo_path` from start/status. Reload takes the same `repo_path` and bumps a per-repository reload sequence; the extension reloads the reviewed tab once on its next heartbeat. Navigate takes `repo_path` and `url` (HTTP(S) on `localhost`, `127.0.0.1` or `[::1]`, no credentials); the heartbeat reply then carries `navigate: {seq, url}` and the extension opens the URL in the reviewed tab once per sequence. The sequence survives a re-arm but the URL does not, so a new Start never replays an old navigation. Reload and navigate answer "Design Mode is not running for this repository" when nothing is armed. The extension posts `heartbeat` to `/design-mode/extension` every 5 s (reply `{armed, reloadSeq, navigate?}`); an armed mode is stopped 60 s after the last heartbeat, or 10 min after Start when none arrived. The adapter uses the same lifecycle core as HTTP and IPC; headless builds return an explicit desktop-host-required error. URL-aware start is the default coordinator path for layout work.

The GitHub settings owner adapter supplies typed device-login, Enterprise account,
credential, label and binding actions for the later settings transport cutover.
It is an internal service boundary; this adapter step registers no new MCP tool,
HTTP route or IPC command. Its reads and results contain credential presence only.

### Internal Ego settings adapters (#1775-330c)

`settings::ego::EgoSettingsStore` targets `profile` with the configured profile ID,
or `machine` for the user base. It captures trusted executable, workspace and
profile together per operation, clears ambient `EGO_PROFILE`, and invokes only
`config capabilities/snapshot/batch`, `models --json` without refresh, and `auth status`.
The model slug selects the provider. Managed DTOs discard unknown owner fields;
credential type/expiry and process diagnostics never enter settings replies,
effects or audit data. Credentials are read-only set/unset markers.

Managed keys (`model`, `roots`, `network`, `exec_paths`) are atomic leaves.
Null/reset sends an owner unset; explicit empty arrays stay explicit. A patch is
one `config batch --if-revision` against the opaque revision from `config snapshot`.
This protects against external CLI writers as well as host requests. Atomicity is
limited to the selected Ego user document, never other settings stores. Owners
without advertised atomic batch and compare-and-swap support fail closed.
Base-only provider headers and credential commands have typed write-only auth
actions (`EgoAuthAction`) and presence-only catalogue fields. Auth actions always
select the base owner, independent of the launch profile. Profile-target reads
of these machine-owned auth paths return `not_found`, not a false unset status. No read executes a
credential command or attempts to recover an owner-redacted value. Failed
readback after a successful write reports `persisted: true` and `nextAction: get`.

Launch defaults (`ego_executable`, `ego_profile`, `ai_chat_workspace`) stay in the
`app` store and its `AppSettingsStore` owner. The adapter is internal; settings
transport registration and executable confirmation belong to later plan steps.
The Providers tab shows the base model when the configured profile has none, as
Ego resolves it, and creates a missing AI Chat workspace like ACP does. Existing Ego IPC/HTTP commands retain
identical shapes; model reads/writes now honor configured profile/workspace.

Tasks brief preparation uses `POST /tasks/start-work-prompt`, the HTTP twin of
`build_start_work_prompt`. Both accept `repoPath` and `input` with key, nullable
URL, title and body. The shared Rust implementation reads the repo-contained
`.tuic-start-work.md` and sanitizes item fields before substitution. The frontend
passes the result to the provider; host delivery still wraps and caps it.

## Tasks items through the repo tool

`repo action=work_item` reads the item linked to the caller's current Git worktree branch in `workitem_links.json`, searching all registered paths sharing its canonical repository root. Explicit targets are `reference: {providerId, accountId, id}`, `url`, or `key` with `provider_id` and `account_id`. URL lookup uses registered plugin resolvers. Key lookup uses the selected provider's search and requires an exact key match; repeated cursors and ambiguous matches fail. Use a reference or URL when the provider cannot find a key through search.

The response has `content`, with a JSON detail inside `<untrusted-content source="work item">` and its closing tag. Detail includes the first comments page. Pass `cursor` from `comments.nextCursor` to read the next comments page, with `limit` (1–100, default 50). All provider text, including metadata, stays inside the block. JSON escapes angle brackets so item text cannot forge a closing tag.

`repo action=work_item_act` accepts the same target and `operation: {kind: "comment" | "transition", value: string}`. Transitions use provider action IDs. Writes fail before provider dispatch unless `mcp_work_item_writes` is true; it defaults to false. The setting is checked again before a frontend claims the write. An action result contains the updated detail in the same block.

Rust owns target selection, input/reply validation and opt-in. A typed `work-item-provider-request` event reaches the existing frontend registry through desktop events and SSE. The frontend atomically claims a request with `work_item_provider_bridge`, calls the existing provider port and completes the reply. Multiple clients cannot dispatch the same request twice. Claims and completions have matching IPC and HTTP transports (`POST /work-items/provider-bridge`). A running frontend with the provider loaded is required; a headless daemon alone has no JS providers. Requests time out after 30 seconds. A dispatched write may have completed when the reply times out: read before retrying.

Credentials stay in Rust's existing account-authenticated HTTP path, which redacts raw tokens and Basic encodings before provider JavaScript receives a response. MCP accepts only the Tasks DTOs and suppresses raw provider errors; it never serializes account objects or configuration.

### Background tray and menu-bar icon

The desktop `close-dialog` event and its SSE copy include `background_available: boolean`. The desktop Ask dialog uses it to omit Background when the tray is disabled or unavailable. Browser clients continue to ignore native close prompts; tray actions add no public request surface.

When the desktop WebViews are deliberately unloaded in background mode, `ui` actions return `{"error":"webview_unloaded"}` immediately. This does not affect PTYs or remote terminal streams. SSE emits `background-mode-changed` with `state` equal to `hidden`, `unloaded` or `visible`.

Design Mode browser commands (`navigate`, `reload`, `screenshot`, `errors`, `eval`)
return correlated native-port results with a ten-second timeout. Direct MCP tool
lists, meta search and schema lookup filter Design Mode by the armed terminal
session; cached-tool dispatch checks the caller and repository again. Start remains
callable by its known tool name before arming. Arm/Stop broadcasts
`notifications/tools/list_changed`; live client refresh remains a verification item.

### Settings proposal confirmation (#1786-9cbd)

`ui(action=confirm, proposalId=...)` accepts an opaque proposal from the shared
settings service. The host supplies the title and redacted diff; supplied
`title`, `message` and `confirmed` cannot approve or replace it. The existing
`mcp-confirm` event reaches desktop and SSE clients, and the existing human
response endpoint/IPC command settles the first answer. A pending dialog uses the
remaining portion of the proposal's absolute 300-second lifetime. Cancelling its
handler dismisses the question and revokes the proposal. Ending an MCP protocol
session revokes its proposals and grants. Headerless confirmation requires a
server-issued connection identity.

The service boundary rejects remote MCP writes regardless of peer registration.
Local MCP enablement, HTTP local-or-authenticated access and native OS-user trust
remain distinct host-issued contexts. The replacement settings tool and HTTP/IPC
routes are not published by the policy foundation; cutover wires those contexts
into the same service rather than duplicating policy.


Durable session wake is shared by MCP submit and peer mail, IPC `resume_session`, and authenticated/local `POST /sessions/wake`. MCP list/status include retained tabs as `state:"hibernated"`. Mail remains durable in the inbox if wake fails; the response includes `resume_error`.

### Account-scoped GitHub review and action ports

`host.githubReviewThreads(account, reference, after)` returns the core B6 review
thread page, including head SHA, bot authors and parsed findings.
`host.githubPrAction(account, reference, request)` calls core B7 policy.
Both require `net:http` and an on-disk manifest declaring `providers: ["github"]`.
The reference must identify a GitHub pull request in the selected account. Core
loads that exact account credential, never a local repository default. Cross-account
references fail before credential lookup. No token is returned to the plugin.

Requests use `action`: `state`, `auto_merge` (`enabled`, optional `mergeMethod`,
`expectedHeadSha`), `queue` (`enqueued`, optional `expectedHeadSha`), `merge`
(`mergeMethod`, `expectedHeadSha`), `rerun` (`runId`), or `review` (`draft` with
`headSha`, `body`, `event`, `comments`). Existing core reviewed-head validation,
merge queue policy and shared request budgets apply unchanged. The desktop command
`plugin_github_action` and `POST /api/plugins/{plugin_id}/github` accept identical
`{account, reference, request}` payloads (IPC also supplies `pluginId`).
The GitHub Work runtime binds both ports when it registers each configured account.
No live write or desktop-load verification is implied by recorded-fixture tests.

### Settings cutover (#1787-4778)

`GET /config` and `PUT /config` are registered once in `shared_routes()` for browser Settings hydration, non-saving refresh and legacy owner saves. Their authentication and secret-redacted readback are unchanged.

`POST /settings/action` is registered once in `shared_routes()`, so desktop and headless daemons use the same handler, authentication policy and value-free Settings event publisher. Upstream OAuth startup requires a desktop host; the headless owner returns `unavailable` before upstream lookup or network access.

`settings` replaces the removed MCP `config` tool without an alias. Its schema is generated from Rust action DTOs. `settings_action(request)` and `POST /settings/action` use the same envelope and owner services. Describe has optional area/store/id filters and cursor pagination (1–128 fields). Reads and edits use an explicit target; edits require expectedRevision. Prompts remain available through the stable-ID `prompts` store; get `/prompts` lists id/label/text/pinned records, while keyed leaf reads and create/remove patches retain stable identities. Typed workflows use perform with an explicit machine target and a store-selected operation DTO for github_accounts, ego (also expectedRevision), upstreams, connections, circleci, telegram and voice in dictation builds. Targeted describe/refresh uses target/store/paths through the registered owner resolver. Connection credential reads use /{id}/credential and return separate password/pairingToken/relayPairing presence; the `set_relay_pairing` action stores the bundle printed by `tuic-remote --relay-pair` after parsing it (an unparsable bundle is `out_of_range`). Credential writes retire only that runtime and reconnect it only when it was active; an explicit later disconnect retires the queued reconnect. Explicit refresh for ego refreshes the configured owner's provider catalogue; describe never performs a network refresh.

MCP access requires settings_mcp_enabled, enabled only by trusted UI; legacy config enablement grants no settings authority. MCP mutations remain loopback-only. HTTP retains local-or-authenticated access and IPC retains native host trust. Exposure proposals use ui confirm proposalId and an exact replay with approval. Secrets remain write-only. Owner transactions and perform remain unavailable while the native inventory gap ledger is nonempty; describe reports inventoryComplete.

Gated HTTP and IPC edits return `confirmation_required` with `proposalId` and do not write.
Replay the exact original edit with `approval: "<proposalId>"` and `confirm: true`.
The same request opens the existing all-client human confirmation dialog, waits for the
answer, and, only after approval, commits the exact edit under its original caller.
A decline, expiry, changed edit, caller mismatch or reused approval cannot commit.
HTTP proposal identity uses the source IP, not the ephemeral TCP port; every request
still requires local or authenticated access. MCP callers can continue to use
`ui action=confirm proposalId=...` followed by an exact replay with `approval`.
Tunnel forward lists, including empty replacements, require confirmation like tunnel ports.

Plugin HTTP fetch and cancel routes use the shared `plugin_http` implementation. The optional `requestId` is scoped to the plugin; cancellation drops the entire request future, including authentication preparation, headers and streamed response body.

`session action=force_retry` is a blocking, localhost-only terminal control.
It resolves the normal session address, requires a live ego recovery snapshot
and an empty tracked composer, serializes input and submits `/retry` once via
the standard agent writer. It does not queue or attach ACP. Remote-qualified MCP
session control remains limited to the existing supported remote actions; the
browser status-bar control uses its owning daemon's normal PTY write transport.

### Telegram request streaming (1959-162a)

Telegram replies now use TUIC-driven receipt (👀), working (👨‍💻), waiting (🤔), answered (👍) and failed (👎) reactions. An ephemeral native draft shows Italian status lines or unapproved agent previews; it refreshes while work continues and the final reply replaces it. Acknowledgement of work requires inbox Read and a new recipient turn. PTY intent/activity and AI Chat assistant chunks share request correlation; finish remains authoritative after draft retirement. Telegram-mail-woken AI Chat turns automatically send their settled assistant text if finish has not answered. Queue and no-reply notices occur at ten minutes and sixty seconds after Read respectively. Restart keeps no request history. Phone rendering requires a live check after the backend restart.

### Settings startup contract

See [Settings startup workflows](settings-startup.md) for discovery, target/revision ownership, device-login intervals, human confirmation and partial-persistence recovery. Native `inventoryComplete` does not prove schema-only authentication onboarding or UI parity. Typed `list_accounts` and Ego `begin_login`/`login_status` use the same owner facade. Login launches the configured process directly in a tracked human terminal, not an arbitrary-command or ACP fallback; status separates process lifetime from credential readback.

### CircleCI credential settings (#1991-bd88)

The machine-scoped `circleci.token` descriptor is write-only. Read
`{action:"get",target:{kind:"machine"},store:"circleci",paths:["/token"]}`
through MCP `settings`, IPC `settings_action`, or HTTP `POST /settings/action`.
Readback returns only `stored.status` and `effective.status`: `set`, `unset`, or
`unavailable`. It describes the stored TUIC token, not environment or CLI fallbacks.
Use `{action:"perform",target:{kind:"machine"},store:"circleci",operation:{action:"set_token",token:"..."}}`
or `operation:{action:"clear_token"}`. Both actions use the existing CircleCI
owner; replies contain only token presence. Clearing the stored token does not
remove `CIRCLE_TOKEN` or the CircleCI CLI configuration. The Git & GitHub page
continues to show the effective token source. No transport returns the token.

### Machine launch instruction previews

`GET /config/defaults` and `POST /config/defaults` are shared machine routes on
both the desktop server and `tuic-remote`. The POST previews draft agent settings
without saving them and uses the same authentication as other machine config routes.

Launch-capable Claude, Codex, Grok, OpenCode, Goose, pi and ego clients receive playbook obligations from generated launch files, not again from MCP initialize instructions. MCP keeps tool discovery and live repository/session/peer state. Unrecognized clients retain the existing MCP marker, submit, worktree and progress fallback.

### Idempotent managed launch

Prefer `agent action=launch` for a managed worker that needs a selected/new workspace and a durable retry identity. One request composes worktree allocation, named spawn, parent linkage and keep-open policy; unkeyed `spawn` remains compatible. The shared Rust boundary also serves `POST /agents/launch` and `managed_agent_launch` IPC. See [the wire contract](../api/http-api.md#durable-managed-agent-launch). Read by `launch_id` without sending a new prompt.

A SQLite journal reserves identity before side effects and uses revision compare-and-swap to prevent concurrent checkpoint owners from both creating resources. An app-scoped async mutex serializes launch mutations; filesystem/process work remains on the existing allocation/spawn paths. A lost reply reuses the child. An uncertain interrupted side effect blocks instead of retrying or changing checkout. Native session/prompt state determines delivery and readiness, never output movement alone. The existing spawn name pinning and managed idle-close exemption are reused. Routing, account choice and local brief rules are not TUIC policy.

Native automation and workflow run bindings also use this transaction through `managed_launch`: automation run IDs and workflow attempt IDs provide stable keys, with an in-process run-owner scope when no peer parent exists. No synthetic peer is registered. Run-owned children use the same pinned-name and explicit keep-open path as MCP children; the existing run controller still owns cancellation and completion. The unkeyed public `spawn` compatibility action continues to use the same low-level PTY spawn implementation. The existing 30-second deferred-prompt watchdog also applies to parentless run-owned children: a timeout reports blocked, retains the queued prompt and never creates a replacement.

### Compact inventories and byte-bounded response snapshots

`session action=list session_id=<address>` resolves one session through the same
address resolver as other session actions. `agent action=list_peers to=<address>`
uses the send recipient resolver; `connection_id` or `connection/peer` selects a
remote owner. Missing or ambiguous targets return errors, not empty success.
Both inventories accept `compact=true`. Rows keep identity/address fields,
`state`, `activity_age_ms`, and `omitted_fields`. Unknown state is `"unknown"`;
unknown activity age is `null`, never zero. The omitted-field list names removed
fields. A failed remote inventory remains an error or `connection_errors`, not
proof that no peers exist. Activity age comes from the backend session snapshot;
a peer without a live terminal has unknown state and age.

For `agent action=inbox` and `session action=output`, set
`byte_budget=4096` (integer range 1024–65536) to request an immutable snapshot:

```json
{"encoding":"json-text","chunk":"...","offset":0,"total_bytes":49162,"continuation":"opaque token","omitted_fields":[],"expires_after_seconds":300}
```

The budget counts **UTF-8 bytes of the compact JSON page object**, including its
escaped `chunk` and all page-envelope fields. It excludes JSON-RPC, MCP content
blocks, HTTP headers, and transport framing. Even one oversized item is split;
UTF-8 codepoints stay whole. Concatenate decoded `chunk` strings in page order,
then parse the resulting JSON to obtain the original response. No fields or
retained text are removed. `total_bytes` and `offset` refer to that reconstructed
JSON text, not the escaped page object. `continuation=null` marks the final page.

Repeat the same action, target and connection with the returned `continuation`.
A continuation defaults to a 4096-byte budget if `byte_budget` is omitted. Tokens
are bound to the inbox owner or output caller and operation/target; changed
window arguments do not recapture the snapshot. Repeating a page is safe.
Snapshots expire 300 seconds after capture, on restart, or when the shared cache
needs space (128 snapshots, 32 MiB of serialized snapshot text). Invalid,
foreign, expired or evicted tokens fail explicitly with `restart_required` when
applicable. Start a new read; do not treat the error as an empty result.

The inbox read cursor advances only when the final chunk is returned. A partial
or abandoned read does not consume mail or its eviction count. Final-page replay
does not acknowledge later evictions again. Explicit `since=0` remains replay,
and cursor advancement remains monotonic. Concurrent new mail stays unread;
concurrent terminal output does not alter an existing snapshot. Inbox retention
loss is still reported as `missed_count`; expired terminal retention is still
reported by the existing output-window metadata. A captured snapshot remains
readable even if its source ring rolls over. The byte bound cannot recover data
already lost before capture.

Without `byte_budget` or a token, existing response shapes and cursor behavior
stay unchanged. Errors remain errors; an error detail that itself exceeds the
budget becomes `response_error_exceeds_byte_budget` with an explicit omission.
The shared native handlers own validation and paging. MCP, the IPC MCP bridge,
and HTTP `format=mcp|mcp_raw` use those handlers; the remote output proxy forwards
byte budgets and tokens without rewriting the bounded envelope.

Fixture provenance: `src-tauri/src/fixtures/mcp-output-2005.txt` captures the
49,162-byte response cited in the 2026-10-17 MCP usage audit, coordinator transcript
result line 168437. It is historical terminal text, not a current-state assertion.
`src-tauri/src/fixtures/mcp-inbox-2005.txt` captures the audit's 49,609-byte inbox
response, private TUIC transcript result line 3239. The regression also covers
multibyte and escaped oversized inbox content.

#### Review clarifications: loss watermarks, wait and local continuation scope

All inbox readers use one delivery-gate acknowledgement boundary. A captured
byte snapshot records the monotonic eviction watermark at capture time. Final
completion acknowledges only the previously unacknowledged part of that range.
Two overlapping snapshots cannot subtract the same historical missed count
from later losses. An ordinary inbox read advances that same watermark; there
is no separate byte-page read cursor or client acknowledgement protocol.

`agent action=wait` is **not byte-bounded**. Supplying `byte_budget` or
`continuation` to wait returns `wait_byte_budget_unsupported` before registering
a waiter or consuming mail. Use byte-budgeted inbox reads for bounded payloads.
The existing wait claims and acknowledges mail before its response is returned.
Adding deferred completion to it would create another acknowledgement model;
wait paging must instead use the shared subscription/ack reconciliation tracked
by #2004-4760. This story does not claim to reduce the audit's 14,763-byte wait
response maximum.

A daemon must support the byte-page envelope to serve a byte-budgeted remote
output request. The desktop proxy rejects unpaged legacy-daemon responses and
pages that exceed the requested compact-JSON budget. It does not pass through
an oversized legacy body merely because `exited` is present. Update an
incompatible daemon; do not retry without a budget unless unbounded output is
actually desired.

Full session inventory exposes known `last_activity_ms` from the backend
snapshot. Compact projection derives `activity_age_ms` from that timestamp;
a missing or zero timestamp remains unknown. An exact registered local peer
name is resolved before interpreting a slash as a connection qualifier. With
an explicit connection selected, a daemon-local slash-containing name remains
a name unless it carries that same connection prefix.

Inbox tokens are scoped to the bound peer identity. Local native MCP output tokens
are scoped to their MCP session and target; proxied remote output uses the
owning daemon's HTTP scope. HTTP MCP-compatible output and the
IPC MCP passthrough intentionally share the local `ipc` continuation scope for
the same target: either local-user transport can resume a token issued by the
other. Tokens are opaque snapshot references, **not authorization credentials**;
normal endpoint admission and the local OS-user trust boundary still apply.

### Frame workload dump

The loopback-only `debug action=frame_workload_dump` schedules the enabled frontend recorder dump. Like `invoke_js`, read `eval_js` logs for completion; submission does not prove frontend liveness. `POST /debug/frame-workload` and desktop `save_frame_workload` share the bounded writer. See [stall evidence](stall-journal.md).

## Runtime call outcome audit (#2010-30a7)

Gated by the persisted `mcp_call_audit_enabled` setting (#2136-dc79): default `false` in release builds, `true` when `cfg!(debug_assertions)`. While off, `tools/call` ingress records nothing and `GET /mcp/audit` (IPC `get_mcp_audit`) returns `{"enabled":false,"records":[]}`; retained rows are not returned. Each row also carries `outcome` (`replied`, `error`, `cancelled`), which describes the reply, never the effect. Settings > MCP > Call audit shows the rows; it fetches on open and on Refresh only.

`GET /mcp/audit` returns at most 100 newest completed/cancelled `tools/call`
observations from an instance-local 1024-record memory ring. `retained`, `evicted`,
`capacity` and `limit` describe retention. Restart clears the ring. HTTP and ACP
both enter `mcp_post`, so a collapsed call is one runtime ingress record, not an
extra inferred hop. Existing inbox-read audit records are unchanged.

Each row has a server `call_id`, optional UUID `client_call_id` and
`parent_operation_id`, catalogue-recognized native, discovery or connected-upstream `operation`, optional resolved
`target` (`kind`, `id`, `name`), optional SHA-256 `target_digest`, start time, `response_text_bytes`, `application_error`,
`transport_error` and `effect`. Unrecognized tool names are digested. Known runtime session/peer identities may be retained; arbitrary target arguments are not. Arguments,
response bodies, arbitrary metadata and error messages are never retained.
The target digest covers the first string target among `to`, `session_id`, `path`,
`worktree_path`, `id`, including the field name and a NUL separator. It is a grouping
key, not authorization or anonymization against a known-value dictionary.

Clients may supply UUIDs in `params._meta["tuic/call_id"]` and
`params._meta["tuic/parent_operation_id"]`; invalid values are omitted. Use a new
call UUID per attempt and a common parent UUID for one intent. Different target
hashes in a batch are distinct operations, not evidence of retries. Neither a
shared parent nor a repeated target proves an attempt is a retry.

Text cost is the sum of UTF-8 byte lengths of top-level MCP `content` blocks whose
`type` is `text`, after response formatting. It excludes JSON escaping, envelopes,
resources, images and transport overhead. A native `error:null` is not an
application error; upstream `isError` is preserved. Error text is not copied.
Effects classify the handler observation: `read_only` for a recognized read-only action,
`completed` for a successful handler return, `failed` for an application-error reply,
and `not_started` for an explicit preflight refusal, including private-secret-form
inspection refusals on native, direct-upstream and wrapped-upstream calls. Explicit handler effect contracts
remain authoritative. `completed` does not mean a submitted command finished, queued
speech was heard, a remote job finished, or the client received the reply. Native
handler interruption and upstream failures after dispatch remain `unknown`; the
proxy carries a typed effect so pre-send policy/configuration refusals are not confused
with a lost upstream reply. No classification parses error prose.
A cancelled dispatch has unknown application outcome, no observed response bytes,
`transport_error=cancelled_before_reply` and unknown effect. A constructed reply
has `transport_error=unobserved_after_reply_construction`: this does not prove
client delivery or absence of a lost reply, and is never rollback evidence.

The projection counts actual MCP ingress, not all Codex model custom calls.
Client-side correlation/captured traces are needed to identify Codex attempts that
never reached MCP or whose replies were lost. Do not evaluate transcript JavaScript
to derive call counts. No OTel exporter is enabled by this feature.

The [partial live recording](../../src-tauri/src/mcp_http/fixtures/outcomes-partial-2010.md)
records a real ego-selected call with a server-retained client UUID, an application
refusal, and a client disconnect over local IPC. It excludes response bodies and
does not complete the four-outcome fixture: real client permission denial and
rendering success still require recording. A successful speech-status read is
not rendering evidence.

## Managed stateless reconnect context (#2008-5317)

A modern `tools/call` without `mcp-session-id` can recover mail/project routing
from an exact host-issued `x-tuic-session` only when that peer has a live PTY or
ACP root. The internal `managed-call:<peer>` routing key is not a wire protocol
session: the response does not introduce a legacy session header. Existing
binding ownership, sibling-bridge policy and replacement migration remain the
only identity mutation seams. Project resolution still follows the live binding;
it never uses the focused repository, a display name or an invented path.

An unknown/headerless peer receives `code=identity_context_required`,
`effect=not_started`, `retryable=false` and bounded repair guidance. Managed
callers must reconnect using host context; external callers initialize and then
register on that protocol session. `repair.max_attempts=1` and
`retry_after_context_change=true` prohibit an unchanged retry policy; the client
must enforce that bound. Server diagnostics alone do not prove ego compliance.
An external caller without a project still receives the Progress core's explicit
`project_required` error, never a guessed project.

## Durable independent mail subscriptions

`agent action=subscribe subscription_id=pump filter=actionable` creates a reader
owned by the bound peer identity. Repeating it resumes the same reader. Use a
separate ID for each pump or watcher. The filter is immutable: `actionable`
(default) excludes routine TUIC lifecycle/idle notices; `all` includes them;
`exit` selects exits, including failed exits; `failure` selects nonzero exits
and prompt-delivery failures. Peer messages, including `BLOCKED` and `RESULT`,
remain actionable regardless of their wording. Unknown system notices fail open
as actionable. No model-side lifecycle classification is needed.

Read with `agent action=inbox|wait subscription_id=pump [limit=1..100]`.
The reply contains `messages`, `count`, `cursor`, `acknowledged_cursor`,
`missed_count`, and `has_more`. A filtered wait returns only for matching mail
or matching retention loss. Routine traffic does not end the wait. For a
continuous background consumer, use the HTTP event stream below or subscribe
to the ACP resource; the consumer does not need a model-driven wait loop.
Subscription batches do not use the legacy byte pager: `byte_budget` and
`continuation` are rejected before either reader is consumed. Use `has_more`
after explicit ACK to request the next bounded batch. Wait retains its separate
lease path and also rejects byte paging.

After processing a batch, call `agent action=ack subscription_id=pump
cursor=<returned cursor>`. Reads do **not** acknowledge. A pending batch's message
range is frozen until acknowledgement, so a retry cannot expand it to include
new results. A changed limit takes effect after the pending batch is acknowledged. Acknowledgement is durable and idempotent. Read again after ack
when `has_more` is true. A reconnect under the same peer identity and reader ID
resumes after its acknowledged cursor. Reclaim the same explicit peer UUID when
registering a new headerless MCP connection. Changing UUID does not migrate
subscription cursors; legacy `replaces` still migrates the legacy inbox only.

Delivery is **at least once until acknowledgement**, not exactly-once execution.
If processing succeeds but ack does not reach the server, the retained batch is
replayed. Deduplicate external effects by message ID. Acknowledged results do
not replay. Concurrent readers must use distinct subscription IDs; sharing an ID
means sharing its acknowledgement. Subscriptions never acknowledge another
subscriber or advance the legacy inbox cursor. Explicit subscription ACKs mark
exact matching live message IDs as observed under the same recipient delivery
gate as legacy snapshot ACKs. This prevents a later FIFO eviction from counting
already-delivered mail as lost. It does not acknowledge the legacy loss watermark;
only the legacy snapshot ACK settles that snapshot's loss count. Filtered gaps
remain unobserved and cannot clear unrelated wake evidence.

The SQLite journal (`<config dir>/mail-subscriptions.db`, with WAL sidecars)
retains the newest **1000 messages per recipient** across process restart. This
is a message-count bound, not a byte budget or an unlimited result archive.
Every common inbox insertion is journaled before its change notification,
including forwarded peer mail. Retention is independent of the legacy
100-message FIFO and lifecycle coalescing. `missed_count` counts matching
unacknowledged events evicted since the reader started or last acknowledged the
gap. Filtered routine evictions do not wake an actionable reader. A new reader
starts at the oldest retained position and does not claim historical losses
before its creation. Acknowledging the returned gap cursor clears the reported
loss. Loss accounting can be conservative when an ACK is delayed until its
offered batch is below the retention floor: already-processed evictions can
remain in `missed_count` until a later gap ACK clears the aggregate. This does
not discard additional mail. A journal append failure is logged and makes subscription reads fail
explicitly for the lifetime of that process rather than claiming an empty inbox.
Legacy in-memory mail delivery remains available.

Each peer may keep 32 reader IDs (1–64 ASCII letters, digits, `_` or `-`).
`agent action=unsubscribe subscription_id=pump` removes a reader, not retained
mail. The journal and cursors are not removed by MCP disconnect or peer
retirement. Legacy calls without `subscription_id` retain their existing
`since`/`next_since` semantics and notifications; `since` is rejected for a
subscription. No new Tauri command is needed: peer actions still require the
MCP identity binding, including calls through the IPC MCP bridge.

### Continuous HTTP and ACP notifications

- `POST /agent/subscriptions/action`: accepts the same `subscribe`, `inbox`,
  `ack`, and `unsubscribe` payload as the agent tool.
- `GET /agent/subscriptions/{id}/events`: SSE `mail` events carry bounded batches
  with `id: <cursor>`; keep-alive comments carry no mail. A connection emits an
  unacknowledged batch once. Ack through the action endpoint advances it to the
  next batch. Reconnect replays an unacknowledged retained batch. `Last-Event-ID`
  does not acknowledge processing; only explicit ack does. Stream errors emit
  `event: error` and close. Both endpoints require loopback, an enabled `agent`
  tool, and `Mcp-Session-Id` bound to the reader's peer. Body fields cannot select
  another peer. They use the same service and validation as MCP tools.
- On MCP over ACP, subscribe/read resource
  `tuic://inbox/subscriptions/<id>` after creating the reader. It sends
  `notifications/resources/updated` only for a new actionable batch cursor or a
  journal error. It also filters the idle-ego wake; routine traffic sends neither
  a resource update nor a wake. Ack uses the agent tool. The
  wake cursor is consumed only after a turn is accepted; a busy/skipped attempt
  stays eligible when later mail arrives at the same frozen cursor. Selecting a durable
  inbox resource replaces the legacy inbox as the idle-wake source on that ACP
  connection. Explicitly subscribing to `tuic://inbox` still requests its legacy
  unfiltered resource notifications.
- HTTP MCP `/mcp`, its SSE/streamable response transport, and the local IPC MCP
  bridge expose the same agent actions. The new HTTP SSE endpoint supplies
  continuous durable-mail notifications; ordinary GET `/mcp` and the legacy
  `tuic://inbox` resource are not silently reinterpreted as filtered readers.

Controlled replay fixture: the audit's four captured idle notifications yield
four legacy lifecycle deliveries and zero actionable deliveries. A subsequent
RESULT produces one matching batch per independent acknowledged reader. This
measurement is not a claim about product-wide token or latency savings.

`session action=input` also accepts only `pty_description` (string or null).
This updates or clears the context-bar description without sending PTY bytes.

Managed `agent action=launch` uses the same private pre-dispatch static contract as other native actions. Discovery describes the disjoint receipt-read and creation forms, workspace alternatives, and nested spawn argument types; the server enforces exclusivity. Invalid calls return structured `invalid_arguments` with `effect=not_started` before the launch journal or PTY is touched.

## Client tool identity

Server instructions use TUIC protocol names as shorthand. Claude Code exposes the configured `tuicommander` server tool as `mcp__tuicommander__agent`; Codex 0.163.0 exposes namespace member `mcp__tuicommander.agent`. A different configured server alias changes that prefix. These are not the client native `Agent` tool. Use the actual exposed schema; a bare `tool_name=agent` is valid inside TUIC lazy `call_tool`, not as a guessed direct client call.

The recorded client names and localhost-only capture script are in `src-tauri/src/mcp_http/fixtures/tool-names-2108.*`. The probe advertises one MCP tool and captures only outgoing tool names, with isolated homes and dummy keys; it rejects the model request without a vendor call. This records client qualification, not a production TUIC server handshake.

### Remote session closure

MCP `session action=close` accepts a remote `connection_id` or connection-qualified session reference. The host resolves aliases on the owning daemon and sends authenticated `DELETE /sessions/{id}`. The caller's own PTY remains protected from self-close, and non-loopback MCP callers cannot invoke destructive close actions. A disconnected connection reports an error; it never falls back to a local session. HTTP DELETE and local MCP close use the same teardown, session-closed events and retained output tombstone. Repeating a close of a retained tombstone succeeds. Explicit close remains user-directed and is not limited to abandoned shells.

### Detached command identity boundary

Native `tuic bg` removes `TUIC_SESSION` and `TUIC_PARENT` from the command
environment and sets `TUIC_BG_COMMAND=1`. Headerless MCP identity recovery
checks each process up to its 16-hop bound and stops at this marker before
reading an identity or reaching the foreground runner. This also covers a
bridge whose agent removed the marker from the bridge environment. Ordinary
foreground ego ancestry recovery and explicit identity headers are unchanged.
The detached runner keeps its caller identity for completion queueing and mail.
This is an identity boundary, not a security sandbox or a delivery-policy bypass.

### Session usage reads

`GET /sessions/{id}/usage` and `get_session_usage` share `session_usage.rs`.
The owner check precedes all local process/cache/provider reads. Connected
remote PTYs forward to the owning daemon; an absent/failed route returns
`remote session` unavailability. No local-account fallback is allowed. The
frontend transport table also supports the normal session-owner routing.

### TLS readiness

`get_mcp_status` and `GET /mcp/status` expose `tls_active`. The loaded dual-protocol
serving task owns a strong lifetime token; AppState holds only a weak token.
Provisioning failure, no TCP listener, task completion, and cancellation do not
leave HTTPS active. Replacing the token prevents an old task's exit from clearing
a replacement listener. Tailscale detection retains its separate capability
state for provisioning and Host trust. Settings and connect URLs use live TLS
readiness, not that capability. The CLI provisioning fallback reads each PEM
stream from stdout, because the sandboxed macOS app CLI cannot write into the
caller's temporary directory.


### Append-only story notes

`add_note` and `list_notes` use the same shared story service on all transports. Notes retain backend actor/time and the subject revision without advancing it. Optional `{runId, commandId}` receipt references are validated against the same project and plan. See [Native stories in the HTTP API](../api/http-api.md#append-only-story-notes) for fields, replies, bounds, and append-only semantics.
