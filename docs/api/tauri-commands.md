# Tauri Commands Reference

Planner Ideas use the same `StoryAction` envelope as plans and stories: `add_idea {text,source?}`, `list_ideas`, and `update_idea {idea_id,text}`. IPC `story_action_command`, HTTP `POST /stories/action?path=<project>` (`{action:<StoryAction>,sessionId?:string}`), CLI `tuic story`, and MCP `workflow action=story input=<StoryAction>` share the Rust owner. Add/update return `{type:"idea",value:<camelCase Idea>}`; list returns `{type:"ideas",value:[...]}`. The project sets `repoPath`; list also includes global Ideas, while update requires project ownership. Source defaults to the bound session id or `local-api`. Text is trimmed to a nonblank maximum of 16000 characters; source is nonblank with a maximum of 256. Writes use the strict locked `notes.json` owner and publish payload-free `ideas-changed` on desktop and `/events` SSE. UI `save_notes(base,config)` / `PUT /config/notes` applies per-id and per-field intent instead of replacing stale arrays. See [Ideas through the planner](../user-guide/native-stories.md#ideas-through-the-planner).

Native plans have a stored `revision`. `update_plan` takes `plan_id`, `expected_revision`, and `patch` with optional `title` and `source`. At least one field is required. The shared service validates both fields and commits them with one revision increment in one transaction; stale revisions leave the plan unchanged. Existing records start at revision 1. A metadata change changes the workflow planning and verification fingerprints, including plans with no stories. Source prose stays in the external document; the update does not move or rewrite that file. Source-changing edits are refused with a user error while a plan coordinator/planner attempt is running, including an unbound attempt or a paused run. Finish the coordinator or cancel its run first. Title-only and unchanged-source edits remain allowed. The same action is available through MCP, HTTP, desktop IPC, and `tuic story`.

## Scoped plugin settings

`plugin_settings_action(plugin_id: String, action: PluginAction)` returns catalogue descriptors, a redacted snapshot, or a commit reply. Actions: describe, get with paths, set/reset with path and expectedRevision, patch with operations and expectedRevision. The backend binds the plugin owner and config file; unloaded or schema-less plugins are unavailable. HTTP twin: `POST /api/plugins/{id}/settings`.

Slice F exposes `start_graph {target:{type:story|plan,id},expected_revision?,definition_id,definition_revision,request_id,limits?}` through the existing owning-daemon service. Story starts require the current native revision and pin the selected publication; the request ID is bound to its payload. Omitted limits use Rust defaults. Plan dispatch remains unavailable in this build and its start control says so. The `workflow action=run` MCP entry uses an inline schema generated from `RunAction` and the public `RunCommand` variants; it returns the same scoped snapshots and cursor-ordered events as IPC/HTTP. Run history shows pinned activations, decisions/evidence, repair counters, pause targets and complete event payloads across pages. Graph recovery uses `resume_graph {execution_id,activation_id,resolution}` after answering pending input; pause and cancel use the existing sequence-fenced commands. Legacy runs offer inspection and cancellation in the UI. No new persistence or client scheduler is added.

## Workflow runs

| Command | Parameters | Result | Description |
|---|---|---|---|
| `workflow_run_action` | `project, action` | tagged `RunReply` | Starts, reads, lists plan runs, pages events, executes pinned checks, records checked integrations or canonical recertifications, or commands a durable project-scoped run. See [Workflow runs](../backend/workflows.md#durable-runs). |

## Workflow definitions

| Command | Parameters | Result | Description |
|---|---|---|---|
| `workflow_definition_action` | `project, action` | tagged `WorkflowReply` | Reads, edits, validates, and publishes project-scoped workflow definitions; `update_closure` sets the approval policy and `update_checks` pins executable checks. See [Workflow definitions](../backend/workflows.md). |

## Native stories

Native stories expose `qa` for work that waits on manual checks. A criterion starts with `[MANUAL]`, or is marked through `set_criterion_manual {index,manual}`. Active work automatically enters QA when all unchecked criteria are manual; `submit_qa` explicitly requests that state. `reject_qa` returns QA to In progress; `approve` requires every criterion checked. QA keeps plans Active and dependencies blocked. All transitions retain expected-revision checks. See the [native stories guide](../user-guide/native-stories.md#manual-qa).

### Reversible native record archival

All native story transports share `archive_story`, `unarchive_story`, `archive_plan`, and `unarchive_plan`. Story actions take `story_id`; plan actions take `plan_id`. Each requires `expected_revision` and returns the updated record with its incremented `revision` and `archived` flag. Stale revisions fail without a write. Archival preserves status, criteria, dependencies, and child archive flags; it never deletes records or cascades to children.

`list_plans` and `list_stories` default to `archived: false`. Set `archived: true` to select archived records. The story filter combines with the other filters and is bound into pagination cursors. `filtered_plan_view` also accepts camel-case `filter.archived` (default false), which selects archived list ids and board columns without changing the active-plan state calculation. Direct reads remain available. Default plan views and workflow dispatch omit archived stories; archived plans and stories cannot start manual or workflow work. Restore the parent plan before starting its stories.

Both archive and restore refuse claimed stories and plans that contain a claimed story. An active or paused plan-root workflow owns the plan's story set. A story-root workflow owns its target and execution stories, not unrelated siblings. Cleanup of an owned story or its parent plan is refused until the run ends. The workflow writer lock remains held through the archive transaction, so a concurrent run cannot start between the ownership check and the write.

`story_archive_history` (`story_id`) and `plan_archive_history` (`plan_id`) return `{type:"archive_history",value:[{revision,archived,actor}]}` in revision order. Events commit atomically with the record and retain caller provenance. Existing status transition history remains separate. Consecutive archive-only revisions preserve accepted-work integration receipts; a work change or a moved canonical Git ref still invalidates them. Example: `tuic story '{"action":"archive_story","story_id":"<id>","expected_revision":1}' --project /absolute/project`.


| Command | Parameters | Result | Description |
|---|---|---|---|
| `story_action_command` | `project, action, sessionId?` | tagged `StoryReply` | Creates, reads, amends, claims, transitions, or removes a cancelled dependency from native stories. `update_story` takes a revision-checked typed patch for unclaimed Backlog/Ready stories without run ownership; metadata, criteria and dependencies commit atomically. `list_plan_sources` discovers project Markdown plans and `add_plan_source` records one with its document title; both are also available through HTTP and MCP. `plan_view` returns a Rust-derived plan summary and transitive `abandoned` indicators; `transition_history` returns actor provenance. Removal requires a Backlog dependent, a direct WontFix prerequisite, and the current revision. The backend checks project ownership and claim session identity. See [HTTP API](http-api.md#native-stories). |
| `story_capabilities` | none | `true` | Infallible capability probe used before loading the dialog. Its absence means the running desktop backend predates native stories. |

`list_stories` shares optional top-level `state`, `priority`, `story_type`, `limit` (1–100), and `cursor` across all transports. Without a limit it returns every matching story; limited responses retain the `value` array and add `next_cursor` only when more matches remain. Cursors require a limit and the same plan/filters. See the HTTP API for bounds and live-read semantics.

`filtered_plan_view` uses the same service on IPC, HTTP, MCP, and CLI. It takes `plan_id` and `filter` (camel-case `visibility`, `storyType`, `untypedOnly`), and returns the full plan with filtered list ids and per-status board ids. Optional `storyType` on create input and persisted stories is feature, fix, refactor, chore, or task; omitted types stay untyped. See the HTTP API for defaults and combined-filter rules.

## Project Progress

| Command | Parameters | Result | Description |
|---|---|---|---|
| `report_progress_event` | `project, report` | `{id}` | Appends one `done` or `blocked` entry through the same core as MCP and HTTP. |
| `progress_list` | `project, input.blockedOnly?, input.ptyId?, input.limit?, input.cursor?` | `ProgressList` | A newest-first page (default 8, maximum 100), matching total, next cursor, available PTY IDs, and stored last-visit mark. |
| `progress_projects` | none | `string[]` | Journal projects ordered by their most recent entry, for clients without an active desktop repository. |
| `progress_delete` | `project, input.ids` | `{deleted}` | Deletes entries by id, scoped to the project — one project cannot delete another's. |
| `progress_flow` | `project, input.ptyId?` | `ProgressFlow` | The journal as a delegation sequence: participants and ordered hand-off events (see `docs/api/http-api.md` → Project Progress). |
| `progress_flow_detail` | `input.ptyId, input.agentId, input.part` | `{text}` | Full redacted prompt or report of one subagent arrow, fetched on demand. |
| `progress_mark_viewed` | `project, ptyId?` | `{lastViewedMs}` | Moves the selected PTY's or repository aggregate's last-visit mark to now. Deliberately not an MCP action: it is the reader's, not the agent's. |

The journal is append-only. There is no pause, clear, correction or export
command: an entry is written once and either kept or deleted. `intent` entries
are written by TUIC from the agent's `intent:` marker and cannot be reported.
Every stored entry has secret-shaped text and step values redacted; agent and
target names are redacted and capped at 80 characters.

All commands are invoked from the frontend via `invoke(command, args)`. In browser mode, these map to HTTP endpoints (see [HTTP API](http-api.md)).

Grid-frame channels and `terminal_styled_rows` share the optional
[`TCX1` cell-text trailer](../frontend/canvas-terminal-audit.md#cell-text-extension-tcx1)
with HTTP/WS. Complete cell text includes retained zero-width characters.
`terminal_search_buffer` returns UTF-16 string ranges; `terminal_search` retains
grid-cell coordinates. Both match the stored sequence without normalization.

## PTY Session Management (`pty.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `create_pty` | `config: PtyConfig` | `String` (session ID) | Create PTY session |
| `create_pty_with_worktree` | `pty_config, worktree_config` | `WorktreeResult` | Create worktree + PTY |
| `write_pty` | `session_id, data` | `()` | Write to PTY |
| `write_pty_parts` | `session_id, parts: Vec<String>` | `()` | Write several inputs under one writer lock. The parts stay separate on purpose: post-write bookkeeping runs once per part, and it is not a function of the joined bytes (a lone `/` opens slash mode, an exact option key answers a choice prompt) |
| `enqueue_agent_command` | `session_id, text, idempotency_key?, expected_draft?` | `{ accepted, typed, queued }` | Queue for the next idle window; optional `expectedDraft` validates and clears the live composer first (errors after clearing are uncertain; do not retry automatically); optional key (1–128 UTF-8 bytes, wire name `idempotencyKey`) recognizes the last 128 accepted keys per live PTY without resubmitting; acceptance is not a model-turn receipt |
| `clear_queued_agent_commands` | `session_id` | `usize` | Drop every queued command; returns how many |
| `list_queued_agent_commands` | `session_id` | `[{ id, text }]` | The queued user commands in delivery order; peer messages excluded |
| `remove_queued_agent_command` | `session_id, command_id` | `bool` | Drop one queued command by id; false when it already drained |
| `resize_pty` | `session_id, rows, cols` | `()` | Resize PTY; alternate-screen resizes preserve primary-log continuity |
| `pause_pty` | `session_id` | `()` | Pause reader thread |
| `resume_pty` | `session_id` | `()` | Resume reader thread |
| `close_pty` | `session_id, cleanup_worktree` | `()` | Close PTY session |
| `can_spawn_session` | -- | `bool` | Check session limit |
| `get_orchestrator_stats` | -- | `OrchestratorStats` | Active/max/available |
| `get_session_metrics` | -- | `JSON` | Spawn/fail/byte counts |
| `list_active_sessions` | -- | `Vec<SessionInfo>` | List all sessions with `display_name_is_custom`, `display_name_from_spawn`, `is_remote`, optional live `tuic_session`, the optional resolved `parent_session`, and the same optional lifecycle `state` (`shell_state`, `agent_state`, `background_work`, `queued_commands`) returned by `GET /sessions` — one builder serves both. Sessions running on a connected remote machine are in the list too, each carrying `connection_id`; a local row has none (#791-055e) |
| `list_worktrees` | -- | `Vec<JSON>` | List managed worktrees |
| `get_session_foreground_process` | `session_id` | `JSON` | Get foreground process info |
| `get_kitty_flags` | `session_id` | `u32` | Get Kitty keyboard protocol flags for session |
| `get_prompt_receipt` | `session_id` | `PromptReceipt` | Read captured launch sections and observation gaps; async/blocking worker |
| `get_last_prompt` | `session_id` | `Option<String>` | Get last user-typed prompt from input line buffer |
| `get_shell_state` | `session_id` | `Option<String>` | Get current shell state ("busy", "idle", or null); agent-specific semantic Working markers can repair a transient false-idle state |
| `has_foreground_process` | `session_id: String` | `bool` | Checks if a non-shell foreground process is running |
| `get_pty_capture` | -- | `JSON` | Raw PTY capture tap state: enabled, session filter, directory, bytes per session. Browser parity: `GET /diagnostics/capture`. |
| `set_pty_capture` | `enabled: bool, session_id: Option<String>` | `JSON` | Start/stop recording raw PTY bytes to `<config dir>/captures/<id>.tcap` or the absolute `TUIC_CAPTURE_DIR` override; a relative override returns an error and leaves recording disabled. Starting begins a fresh file. Surfaced as **Capture Session** in the tab context menu under `isPerfDebug()`. Browser parity: `POST /diagnostics/capture`. |
| `set_session_name` | `session_id, name, is_custom?` | `()` | Set a session display name and whether it represents an explicit user rename |
| `get_input_buffer_content` | `session_id` | `String` | Get the current content of the input line buffer (what the user is typing). Used by plugins with `pty:read` capability. |
| `terminal_get_selection_text` | `session_id, start_row, start_col, end_row, end_col, history_base?` | `Result<String, String>` | Read a scrollback-aware selection, join soft-wrapped rows, and remove coherent Claude visual gutter runs. Optional frame `history_base` rebases grid-relative rows atomically against history eviction; evicted endpoints are rejected. Browser parity: `GET /sessions/:id/terminal/selection-text` (`historyBase` query parameter). |
| `get_process_stats` | -- | `Vec<ProcessStat>` | CPU% and RSS memory for TUIC and all child process trees |
| `subscribe_terminal_grid` | `session_id, channel: Channel<Response>, visible: bool` | `u64` (epoch) | Register the grid-frame channel and initial visibility for the calling WebView and install a fresh delivery gate (counting from zero). Navigation or destruction releases only that WebView's subscriptions. Returns the subscription epoch the client must carry on `ack_terminal_frame` and `unsubscribe_terminal_grid`. Frames are **raw bytes**, not JSON. Browser parity: `WS /sessions/:id/stream?format=grid` |
| `ack_terminal_frame` | `session_id, epoch: u64, received: u64, visible?: bool` | `()` | Report the total number of frames this client has received and optionally update epoch-scoped desktop visibility. Hidden subscriptions receive no frames. The gate opens when the echo catches up with what was sent, which is what tells a fresh ack from a late one for an abandoned frame. An ack whose epoch is not the live subscription's is dropped. A receipt never marks the grid dirty or generates a frame; actual pending damage and recovery debt are retained by the ticker. Browser parity: none — the WS path uses sequence numbers instead |
| `unsubscribe_terminal_grid` | `session_id, epoch: u64` | `()` | Tear down the grid channel and its gate. The pending scroll target is NOT torn down — it belongs to the session, so an attached browser keeps scrolling after the desktop terminal closes. A non-matching epoch is ignored: a remount subscribes before the outgoing instance unsubscribes, and honouring the stale call would blank a mounted terminal. Browser parity: closing the WS |
| `chat_view_snapshot` | `session_id, epoch: Option<u64>, from_seq: Option<u64>` | `ChatViewSnapshot` | Chat view of a Claude or Grok terminal: transcript entries from the bound session file as ACP updates, with a bounded log, epoch and `reset` flag. Errors `not_bound: <reason>` for a terminal with no bound Claude or Grok agent. Browser parity: `GET /sessions/:id/chat-view` |
| `terminal_styled_rows` | `session_id, start, count` | `Result<Response, String>` (packed bytes) | A range of styled rows by absolute index, filling the client-side scroll cache. Raw bytes for the same reason as grid frames. Browser parity: `GET /sessions/:id/terminal/styled-rows` (`application/octet-stream`) |

`terminal_scroll_info`, `terminal_search`, `terminal_search_buffer`,
`terminal_get_row_text`, `terminal_get_logical_line`, and `terminal_get_lines`
use `pty::vt_try_read`, so each returns `Result<T, String>` rather than a bare `T`.
The `Err` arm means the pool task itself failed; a session that is gone is still
the old default (or a 404 over HTTP). See
[`docs/backend/command-threading.md`](../backend/command-threading.md).

MCP `session action=submit` deliberately has no new Tauri command. It is a
request-scoped orchestration contract in `mcp_transport.rs`: it reuses the PTY
injection claim, writer, input FSM, and output ring, then keeps the MCP response
open for a bounded terminal-movement receipt. Desktop `write_pty` and
`write_pty_parts` remain raw input primitives and make no acknowledgement claim.
Raw input returns an uncertain-delivery error after two seconds rather than
waiting indefinitely on a blocked writer. The write can still complete; do not
retry automatically. Session close does not wait on the input writer.

## Design Mode (`design_mode/tauri_commands.rs`)

The desktop commands and their HTTP equivalents control the same per-repository
Chrome inspector. A start is bound to the named agent session; a second start
for the repository rebinds its existing Chrome window. A grab is inserted into
that session's input draft without submitting it.

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `start_design_mode` | `sessionId: String, browser?: String, url?: String` | `DesignModeStatus` | Start or rebind an agent terminal; optional HTTP(S) `url` opens directly, otherwise uses discovery/onboarding |
| `stop_design_mode` | `repoPath: String` | `DesignModeStatus` | Stop inspection for a repository |
| `get_design_mode_status` | -- | `Vec<DesignModeStatus>` | Read the active per-repository modes |

Command responses use `{ repoPath, sessionId, status }`, with `status` equal
to `armed` or `stopped`. The backend also emits `design-mode-changed` with a
snake_case event payload `{ repo_path, session_id, status }`; browser clients
receive it on `/events`.

## Generators (`generators.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `generate_value` | `generator_id, options` | `GeneratedValue` | Generate a secure random value (password, uuid_v4, uuid_v7, ulid, cuid2, jwt_secret, totp_secret, nano_id, slug, ed25519_keypair) |

## Git Operations (`git.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `get_repo_info` | `path` | `RepoInfo` | Repo name, branch, status |
| `get_git_diff` | `path` | `String` | Full git diff |
| `get_diff_stats` | `path` | `DiffStats` | Addition/deletion counts |
| `get_changed_files` | `path` | `Vec<ChangedFile>` | Changed files with stats |
| `get_file_diff` | `path, file` | `String` | Single file diff |
| `get_gutter_changes` | `path, file, scope?` | `Vec<GutterChange>` | Per-line editor gutter/scrollbar change markers (diff parsed in Rust); empty for an untracked file |
| `get_git_branches` | `path` | `Vec<JSON>` | All branches (sorted) |
| `get_recent_commits` | `path` | `Vec<JSON>` | Recent git commits |
| `get_git_remotes` | `path` | `Vec<{name,url}>` | Every remote; credentials in URLs removed (`workitem_links.rs`) |
| `list_workitem_links` | — | `{repoPath: {branch: WorkItemLink}}` | Work-item links of worktree branches (`workitem_links.rs`) |
| `link_workitem` | `repoPath, branch, link` | `()` | Link a worktree branch to a work item; url must be http(s) |
| `unlink_workitem` | `repoPath, branch` | `bool` | Remove a link; true when one existed |
| `rename_branch` | `path, old_name, new_name` | `()` | Rename branch |
| `check_is_main_branch` | `branch` | `bool` | Is main/master/develop |
| `get_initials` | `name` | `String` | 2-char repo initials |
| `get_merged_branches` | `repo_path` | `Vec<String>` | Branches merged into default branch |
| `get_repo_summary` | `repo_path` | `RepoSummary` | Aggregate snapshot: worktree paths + merged branches + per-path diff stats in one IPC |
| `get_repo_structure` | `repo_path` | `RepoStructure` | Fast phase: worktree paths + merged branches only (Phase 1 of progressive loading) |
| `get_repo_diff_stats` | `repo_path` | `RepoDiffStats` | Slow phase: per-worktree diff stats, last commit timestamps, and `workspace_statuses` keyed by opaque workspace id (Phase 2 of progressive loading) |
| `run_git_command` | `path, args` | `GitCommandResult` | Run allowlisted git command (success, stdout, stderr, exit_code) |
| `get_working_tree_status` | `path` | `WorkingTreeStatus` | Full porcelain v2 status: branch, upstream, ahead/behind, stash count, staged/unstaged entries, untracked files |
| `update_from_base` | `path, branch_name, strategy?` | `String` | Fetch configured base ref and rebase or merge the branch onto it. Conflict cleanup reports `(aborted)` only after abort succeeds; abort failure includes manual recovery guidance. |
| `git_stage_files` | `path, files` | `()` | Stage files (`git add`). Path-traversal validated |
| `git_unstage_files` | `path, files` | `()` | Unstage files (`git restore --staged`). Path-traversal validated |
| `git_discard_files` | `path, files` | `()` | Discard working tree changes (`git restore`). Destructive. Path-traversal validated |
| `git_commit` | `path, message, amend?` | `String` (commit hash) | Commit staged changes; optional `--amend`. Returns new HEAD hash |
| `get_commit_log` | `path, count?, after?` | `Vec<CommitLogEntry>` | Paginated commit log (default 50, max 500). `after` is a commit hash for cursor-based pagination |
| `get_stash_list` | `path` | `Vec<StashEntry>` | List stash entries (index, ref_name, message, hash) |
| `git_stash_apply` | `path, index` | `()` | Apply stash entry by index |
| `git_stash_pop` | `path, index` | `()` | Pop stash entry by index |
| `git_stash_drop` | `path, index` | `()` | Drop stash entry by index |
| `git_stash_show` | `path, index` | `String` | Show diff of stash entry |
| `git_apply_reverse_patch` | `path, patch, scope?` | `()` | Apply a unified diff patch in reverse (`git apply --reverse`). Used for hunk/line restore. `scope="staged"` adds `--cached`. Patch passed via stdin (no temp files). Path-traversal validated |
| `get_file_history` | `path, file, count?, after?` | `Vec<CommitLogEntry>` | Per-file commit log following renames (default 50, max 500) |
| `get_file_blame` | `path, file` | `Vec<BlameLine>` | Per-line blame: hash, author, author_time (unix), line_number, content |
| `get_branches_detail` | `path` | `Vec<BranchDetail>` | Rich branch listing: name, ahead/behind, last commit date, tracking upstream, merged status |
| `delete_branch` | `path, name, force` | `()` | Delete a local branch. `force=false` uses the shared integration proof and compares the ref with its proved tip; content-based proof requires an exact-tip archive. `force=true` preserves the exact tip at `refs/archive/<branch>` before deleting, archives at `refs/archive/<branch>-<sha7>` when that name holds a different tip, and cannot delete a checked-out branch. Refuses to delete the current branch or default branch |
| `create_branch` | `path, name, start_point, checkout` | `()` | Create a new branch from `start_point` (defaults to HEAD). `checkout=true` switches to it immediately |
| `get_recent_branches` | `path, limit` | `Vec<String>` | Recently checked-out branches from reflog, ordered by recency |

## Commit Graph (`git_graph.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `get_commit_graph` | `path, count?` | `Vec<GraphNode>` | Lane-assigned commit graph for visual rendering. Default 200, max 1000. Returns hash, column, row, color_index (0–7), parents, refs, and connection metadata (from/to col/row) for Bezier curve drawing |

## GitHub Authentication (`github_auth.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `github_start_login` | — | `DeviceCodeResponse` | Start OAuth Device Flow, returns user/device code |
| `github_poll_login` | `device_code` | `PollResult` | Poll for token; saves to keyring on success |
| `github_logout` | — | `()` | Delete OAuth token from keyring, fall back to env/CLI |
| `github_auth_status` | — | `AuthStatus` | Current auth: login, avatar, source, scopes |
| `github_disconnect` | — | `()` | Disconnect GitHub (clear all tokens from keyring and env cache) |
| `github_diagnostics` | — | `JSON` | Diagnostics: token sources, scopes, API connectivity |

## GitHub Integration (`github.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `get_github_status` | `path` | `GitHubStatus` | PR + CI for current branch |
| `get_ci_checks` | `path` | `Vec<JSON>` | CI check details |
| `get_pr_review_threads` | `path, pr_number` | `{bot, human, ego}` | Unresolved review threads of one PR, split bot vs human vs ego (first comment carries `<!-- tuic:ego-review -->`) |
| `get_repo_pr_statuses` | `path, include_merged` | `Vec<BranchPrStatus>` | Batch PR status (all branches) |
| `approve_pr` | `repo_path, pr_number` | `String` | Submit approving review via GitHub API |
| `update_pr_branch` | `repo_path, pr_number, expected_head_sha` | `()` | Update a BEHIND PR branch from its base, pinned to the shown head |
| `close_pr` | `repo_path, pr_number` | `()` | Close a PR without merging |
| `merge_pr_via_github` | `repo_path, pr_number, merge_method, expected_head_sha` | `String` | Merge PR via GitHub API, pinned to the reviewed head |
| `get_all_pr_statuses` | `path` | `Vec<BranchPrStatus>` | Batch PR status for all branches (includes merged) |
| `get_pr_diff` | `repo_path, pr_number` | `{diff, headSha}` | Get PR diff content and the head it was read at |
| `get_merged_prs` | `repo_path, since_tag?` | `Vec<MergedPr>` | Merged PRs via GraphQL, optionally since a tag's date |
| `start_conflict_assist` | `repo_path, pr_number` | `ConflictAssistResult` | Worktree on PR head + rebase onto base; reports verified/unverified clean or conflicts, base provenance/warning, and agent prompt (push gated, never auto-merge) |
| `fetch_ci_failure_logs` | `repo_path, branch, check_url?, head_sha?` | `String` | Fetch failed-job logs for the local branch head, or select a check at the supplied PR head SHA; failing checks come from the paginated commit rollup and Actions logs from REST using the repository account on github.com or GHE; one 60-second sweep deadline |
| `circleci_token_status` | — | `CircleCiTokenStatus` | Report token presence and source without returning the token |
| `circleci_set_token` | `token` | `()` | Store a read-only CircleCI token |
| `circleci_delete_token` | — | `()` | Remove the stored CircleCI token |
| `check_github_circuit` | `path` | `CircuitState` | Check GitHub API circuit breaker state |

## Review, Changelog and Improvement Scan (`pr_review.rs`, `changelog.rs`, `improvement_scan.rs`)

Each of the first three is one unattended `acp::oneshot` turn (#795-320b): the
whole input goes inline, ego is offered no host tools, and every permission
request is refused. No API key is stored and no provider HTTP call is made from
TUICommander; which model runs is ego's own configuration. An ego that cannot be
reached is an error carrying ego's own sentence, never an empty result.

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `run_pr_review` | `repo_path, pr_number` | `PrReviewResult` | Review a PR's diff. Findings under the confidence threshold (default `0.7`, `TUIC_REVIEW_CONFIDENCE_THRESHOLD` overrides) are dropped before the result is built. Emits `review-progress` |
| `generate_changelog` | `repo_path, since_tag?` | `ChangelogResult` | Changelog over the merged PRs since a tag. `{ markdown, json }`; `json` is `null` when ego answered in prose only |
| `run_improvement_scan` | `repo_path, focus` | `ImprovementScanResult` | Up to five proposals for `refactor`, `testing` or `perf`. Emits `proposals-ready` |
| `create_issue_from_proposal` | `repo_path, proposal` | `CreatedIssue` | File one proposal as a GitHub issue through account-aware REST. No model call |

## Worktree Management (`worktree.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `create_worktree` | `base_repo, branch_name, create_branch?, base_ref?` | `{ status: "ok", name, path, workspace_id, branch, base_repo, instructions }` | Create a linked worktree and start warming in the background. `instructions.warm_artifacts.status` is `pending`, matching HTTP/MCP. Parent tracked changes are not carried over. |
| `remove_worktree` | `repo_path, workspace_id, delete_branch?, force?, override_lock?, expected_fingerprint?, confirm_missing_checkout?` | `{ branch_delete_warning?: string, branch: string, removal_rule: string }` | Remove a linked worktree by `workspace_id`. Branch deletion defaults to true, or false when force is true. Without force, a clean checkout and submodules with no Git operation in progress are required even when the branch is kept. `force` permits discarding dirty files but does not skip branch proof or override a lock; `override_lock` is separate. A live checkout requires `expected_fingerprint` for force removal. A missing registered checkout instead requires `force` and explicit `confirm_missing_checkout`; its absence is rechecked before cleanup and module refs are preserved before pruning. Branch deletion uses the same integration proof as MCP, including no-op merges, corroborated squash messages and archived content heuristics, alongside ancestry, patch equivalence and verified merged GitHub PRs fetched over account-aware, fully paginated HTTP GraphQL; it uses the captured OID and warns if the proof fails or the ref moves. Keeping the branch skips only branch proof. Before Git removes a live checkout, owner write permission is restored inside it so read-only ignored artifacts cannot block deletion. A leftover unregistered directory is reported with its path. `removal_rule` names the matching rule. |
| `delete_local_branch` | `repo_path, branch_name, workspace_id, keep_worktree?` | `()` | Delete a local branch and dispose of the workspace `workspace_id` names. Two identifiers because there are two objects: `branch_name` is the ref to delete, `workspace_id` the checkout holding it. Refuses to delete the default branch, and refuses when the resolved workspace is on a different branch than the one asked for. Uses safe `git branch -d` |
| `check_worktree_dirty` | `repo_path, workspace_id` | `bool` | Check if the workspace `workspace_id` names has uncommitted changes. Addressed by id because the answer gates an irreversible cleanup — a sibling on the same branch being clean must never authorise destroying this one. Returns false if the id resolves to no checkout. When git cannot answer (the `worktree list` or `status` call fails) it returns an **error**, never `false` — callers that gate a destructive action must see the failure |
| `get_worktree_paths` | `repo_path` | `HashMap<String, { branch, path, kind: "worktree", warm_artifacts }>` | Every linked worktree of a repo, keyed by workspace id. `warm_artifacts.status` is `pending`, `done`, or `failed`. |
| `get_worktrees_dir` | -- | `String` | Worktrees base directory |
| `generate_worktree_name_cmd` | `existing_names` | `String` | Generate unique name |
| `list_local_branches` | `path` | `Vec<String>` | List local branches |
| `checkout_remote_branch` | `repo_path, branch_name` | `()` | Check out a remote-only branch as a new local tracking branch |
| `detect_orphan_worktrees` | `repo_path` | `Vec<String>` | Detect worktrees in detached HEAD state (branch deleted) |
| `assess_orphan_cleanup` | `repo_path` | `Vec<{ path, safe, reason?, live_sessions? }>` | Classify every orphan for automatic removal using tracked/untracked status, branch reachability and live sessions in the checkout. |
| `begin_orphan_cleanup` | `repo_path, paths` | `()` | Register the open Ask dialog for an agent answer. |
| `pending_orphan_cleanup_answer` | `repo_path` | `bool?` | Read the pending answer, or `null` while unanswered. |
| `clear_orphan_cleanup` | `repo_path` | `()` | Clear the pending answer when the dialog closes. |
| `remove_orphan_worktree` | `repo_path, worktree_path, safe_only?, confirmed_sessions?` | `()` | Remove a registered detached orphan by filesystem path. With `safe_only`, recheck clean status, branch reachability and live sessions immediately before removal. Without it, refuse when a live session in the checkout is not in `confirmed_sessions` (the session ids the user saw). |
| `switch_branch` | `repo_path, branch_name` | `()` | Switch main worktree to a different branch (with dirty-state and process checks) |
| `merge_and_archive_worktree` | `repo_path, branch_name, workspace_id, target_branch, after_merge, force?, expected_fingerprint?` | `MergeArchiveResult` | Merge worktree branch into base and archive or delete. A pre-flight counts commits and checks the exact checkout. For `archive` or `delete`, an unverified or dirty checkout or one with a live session returns `action: "needs_confirmation"` **before merging**. Re-call with `force: true` and the confirmed lifecycle `expected_fingerprint` to proceed; a changed fingerprint aborts. A locked checkout is not archived. If conflict cleanup abort fails, the error includes the manual abort command. |
| `finalize_merged_worktree` | `repo_path, workspace_id, action, force?, expected_fingerprint?` | `MergeArchiveResult` | Clean up after a completed merge. Uses the same lifecycle review as one-click cleanup and also requires merged commit status before automatic cleanup. Without `force`, a dirty, unverified, or live checkout returns `action: "needs_confirmation"` (`merged: true` — only cleanup stopped). Force requires the confirmed lifecycle `expected_fingerprint` and rejects changed state. A lock stops archiving. Delete may include `branch_delete_warning` if safe branch deletion kept the branch. |
| `get_workspace_lifecycle` | `repo_path, workspace_id` | `{ dirty_files, untracked_files, live_sessions, warnings, missing_checkout, dirty_fingerprint?, submodule_unpushed_commits, commit_status, removal_safety, error? }` | Fresh workspace-id-addressed removal preflight. `missing_checkout: true` identifies a registered checkout whose directory is gone; it has no dirty fingerprint and requires force confirmation. `dirty_files` counts files a removal discards (`null` when unavailable — not `0`); `commit_status` is `unmerged`, `in_sync`, `merged`, or `unknown`; `removal_safety` is `safe`, `requires_force`, or `unknown`. Unknown never authorizes removal. HTTP twin: `GET /worktrees/lifecycle`. |
| `list_base_ref_options` | `repo_path` | `Vec<String>` | List valid base refs for worktree creation |
| `run_setup_script` | `script, cwd` | `JSON` | Run a setup script through `sh -c` / `cmd /C` in `cwd`; returns exit code and captured output |
| `generate_clone_branch_name_cmd` | `base_name, existing_names` | `String` | Generate hybrid branch name for clone worktree |

## Configuration (`config.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `load_app_config` | -- | `AppConfig` | Load app settings |
| `save_app_config` | `base, config` | `()` | Save app settings |
| `load_notification_config` | -- | `NotificationConfig` | Load notifications |
| `save_notification_config` | `base, config` | `()` | Save notifications |
| `show_native_notification` | `title, body, target` | `()` | macOS desktop-only alert that retains the Notification Center click and emits `native-notification-click` for a terminal or Progress target; browser/remote clients cannot call it |
| `load_ui_prefs` | -- | `UIPrefsConfig` | Load UI preferences |
| `save_ui_prefs` | `base, config` | `()` | Save UI preferences |
| `get_config_defaults` | `preview?: LaunchPreview` | `ConfigDefaults` | Read-only defaults and resolved launch instruction previews; optional draft agent config and global marker flags |
| `load_repo_settings` | -- | `RepoSettingsMap` | Load per-repo settings |
| `save_repo_settings` | `base, config` | `()` | Save per-repo settings |
| `check_has_custom_settings` | `path` | `bool` | Has non-default settings |
| `load_repo_defaults` | -- | `RepoDefaultsConfig` | Load repo defaults |
| `save_repo_defaults` | `base, config` | `()` | Save repo defaults |
| `load_repositories` | -- | `JSON` | Load saved repositories |
| `save_repositories` | `config` (`mutationVersion: 1` keyed delta) | `()` | Apply repository/group/order/active-selection changes to the latest locked document; same-record conflicts are returned to the caller |
| `list_stale_temp_repository_candidates` | -- | `StaleTempCandidate[]` (`{path, displayName}`) | Read-only preview of rows classified as stale-temp ghosts (#763-d219); never mutates |
| `repair_stale_temp_repositories` | `paths` (`string[]`) | `StaleTempRepairSummary` (`{removed, backupPath}`) | Re-validates every path against the classifier on the current on-disk document, refusing the whole request if any no longer matches, then removes the validated rows in one transactional write after backing up the pre-repair document |
| `load_prompt_library` | -- | `PromptLibraryConfig` | Load prompts |
| `save_prompt_library` | `base, config` | `()` | Save prompts |
| `load_notes` | -- | `JSON` | Load notes |
| `save_notes` | `base, config` | `()` | Save notes |
| `save_note_image` | `note_id, data_base64, extension` | `String` (absolute path) | Decode base64 image, validate ≤10 MB, write to `config_dir()/note-images/<note_id>/<timestamp>.<ext>` |
| `delete_note_assets` | `note_id` | `()` | Remove `note-images/<note_id>/` directory recursively (no-op if missing) |
| `get_note_images_dir` | -- | `String` | Return `config_dir()/note-images/` absolute path |
| `load_keybindings` | -- | `JSON` | Load keybinding overrides |
| `save_keybindings` | `base, config` | `()` | Save typed action/key overrides; reject invalid records before persistence |
| `load_agents_config` | -- | `AgentsConfig` | Load per-agent run configs, including optional model defaults, and `prevent_alt_screen` overrides |
| `save_agents_config` | `base, config` | `()` | Save per-agent run configs, including optional model defaults, and `prevent_alt_screen` overrides |
| `get_agent_native_status_signals` | `agent_type` | `bool` | Read the default-on Claude/Codex launch-scoped status setting |
| `set_agent_native_status_signals` | `agent_type`, `enabled` | `()` | Change launch-scoped status injection for future sessions |
| `load_activity` | -- | `ActivityConfig` | Load activity dashboard state |
| `save_activity` | `base, items` | `()` | Save activity dashboard state |
| `load_repo_local_config` | `repo_path` | `RepoLocalConfig?` | Read `.tuic.json` from repo root; returns null if absent or malformed |
| `save_repo_local_config` | `repo_path` | `()` | Write the repo's **effective resolved** worktree/branch settings (global defaults + per-repo overrides) to `.tuic.json` at its root (committable, team-shareable). Preserves fields already in the file (e.g. `mcp_upstreams`); never writes script fields |

## SSH Tunnels (`tunnels/tauri_commands.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `list_tunnel_profiles` | -- | `Vec<TunnelProfile>` | Load all tunnel profiles (global + per-repo merged) |
| `save_tunnel_profile` | `profile: JSON` | `String` (profile ID) | Create or update a tunnel profile. Auto-generates UUID if `id` is empty. Validates before saving |
| `delete_tunnel_profile` | `id` | `bool` | Delete a tunnel profile by ID. Stops the tunnel if running |
| `start_tunnel` | `id` | `String` | Start a tunnel by profile ID. Loads the profile, validates, and spawns the SSH process |
| `stop_tunnel` | `id` | `()` | Stop a running tunnel by profile ID |
| `list_active_tunnels` | -- | `Vec<JSON>` | List all active tunnels with ID, status, and started_at |
| `get_tunnel_status` | `id` | `JSON` | Get the current status of a specific tunnel (starting, connected, reconnecting, stopped, error) |
| `list_ssh_config_hosts` | -- | `Vec<String>` | Parse `~/.ssh/config` and return all non-negated, non-wildcard Host entries |
| `list_discovered_ssh_hosts` | -- | `DiscoveredSshHosts` | List non-wildcard `~/.ssh/config` aliases and plain `~/.ssh/known_hosts` names deduplicated by resolved host and port; hashed known_hosts entries are counted in `hashed_count` |
| `probe_discovered_ssh_host` | `target, port?` | `SshHostStatus` | Probe one discovered entry (known_hosts entries use `StrictHostKeyChecking=yes`); refuses a host not in the discovered list |
| `probe_ssh_config_hosts` | -- | `Vec<SshHostStatus>` | Probe the `~/.ssh/config` aliases (known_hosts names excluded) with bounded concurrency and classify shell, no-shell, authentication-failed and unreachable results |
| `get_tunnel_audit` | `id, limit?` | `Vec<JSON>` | Query audit log events for a tunnel (default limit 20). Returns timestamp, kind, and extracted message |
| `list_ssh_agent_keys` | -- | `SshAgentInfo` | Detect SSH agent type (1Password, Secretive, GPG, generic) and list loaded keys via `ssh-add -l` |

## Remote Connections (`remote_connection.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `list_remote_connections` | -- | `Vec<RemoteConnection>` | Load every configured remote machine from `connections.json` |
| `save_remote_connection` | `base, connection` | `()` | Create with null base or update a remote machine from its loaded snapshot; merges changed fields into the latest record under the config file lock |
| `delete_remote_connection` | `id` | `()` | Tear down and delete a remote machine, both vault credentials, and its ephemeral daemon best-effort; an installed service is left for explicit uninstall |
| `set_remote_connection_relay_pairing` | `id, bundle` | `()` | Parse and store the relay pairing bundle from `tuic-remote --relay-pair` in the OS credential vault, or forget it when `bundle` is empty. Same owner and value-free `connections` Settings change as the HTTP route |
| `set_remote_connection_password` | `id, password` | `()` | Store the Basic Auth password in the OS credential vault, or forget it when `password` is empty. Never written to `connections.json`. Publishes a value-free `connections` Settings change only after vault persistence, through the same owner as HTTP |
| `remote_connection_password_exists` | `id` | `bool` | Whether a password is stored. The password itself is never readable — this and the token exchange are the only answers given about it |
| `fetch_remote_connection_token` | `id, baseUrl, username` | `String` | Trade the stored password for the daemon's in-memory session token over `GET /api/auth/session-token`. Runs in the backend so the password never reaches the WebView. Re-run on every connect: the daemon mints a new token on restart |
| `install_remote_daemon` | `id` | `()` | Stage the matching daemon and install/start a systemd user unit or launchd agent, then persist `deploy = installed` |
| `uninstall_remote_daemon` | `id` | `()` | Stop and remove the systemd/launchd service files and persist `deploy = on_connect` |

## Remote Connection Runtime (`remote_runtime.rs`)

`remote_connection.rs` above says what a connection *is*; these say whether it is
up. The state machine — health probe, token exchange, status poll, SSH tunnel and
base URL — runs in Rust so that backend tasks, not only the WebView, can reach a
remote daemon.

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `connect_remote_connection` | `id` | `()` | Bring a connection up. SSH connections may deploy a matching loopback-only daemon first, then authenticate with the vault pairing token; direct and unmanaged connections use the stored password exchange. Idempotent while connecting or connected, so a double click opens one tunnel. Every transition is announced as a `remote-connection-status` event |
| `disconnect_remote_connection` | `id` | `()` | Stop the status poll, forget the token, stop the tunnel |
| `remote_connection_statuses` | -- | `Vec<RemoteConnectionStatus>` | Live status of every connection. `base_url`, `token` and `protocol_version` are present only while connected; `update_in_progress` is true during a manual or unattended update. A disconnected machine has no route to hand out |
| `prepare_remote_update` | `id` | `UpdatePreview` | Select the release or matching local daemon binary and report both build identities and the live session count |
| `update_and_restart_remote` | `id, confirmedSessions, expectedSha256` | `UpdatePreview` | Check the confirmation, update by Direct upload or SSH deployment, and verify the new build after reconnect |

## Agent Detection (`agent.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `detect_agent_binary` | `binary` | `AgentBinaryDetection` | Check binary name in PATH or an exact absolute path, version, and cached `supports_no_alt_screen` help probe |
| `prepare_agent_launch_args` | `agent_type, binary_path, args` | `Vec<String>` | Add a supported native-scrollback flag according to the agent setting; probes in a blocking worker with a deadline. HTTP parity: `POST /agents/launch-args` |
| `detect_all_agent_binaries` | `binaries` | `HashMap<String, AgentBinaryDetection>` | Detect the named binaries in parallel, path only (no version or screen-capability lookup) |
| `detect_claude_binary` | -- | `String` | Detect Claude binary |
| `detect_installed_ides` | -- | `Vec<String>` | Detect installed IDEs |
| `open_default_app` | `target: String, kind: "url" \| "path"` | `()` | Host-only default application open; allows http/https/mailto/tel URLs, checks path metadata, returns after spawn and reaps the launcher through `tuic_core::child_reaping` |
| `open_in_app` | `path, app, line?, col?` | `()` | Open path in application; `line`/`col` are used only by editors that support them |
| `spawn_agent` | `pty_config, agent_config` | `String` (session ID) | Spawn agent in PTY |

## Agent Session Discovery (`agent_session.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `discover_agent_session` | `agent_type, cwd, claimed_ids, agent_pid, env_overrides` | `Option<{ sessionId, launchCommand, provider }>` | Discover the agent's session UUID for session-aware resume. IPC and HTTP run filesystem and process reads on the blocking pool. Ego bindings are cached by sessions root and pid, with one-lease pid and writer-lock validation; a miss scans for the latest matching heartbeat. Claude and grok resolve it exactly from their pid→session registry when `agent_pid` is known; every other agent (and any Claude/grok too old to publish one) falls back to the newest unclaimed session file, which cannot tell two tabs in one folder apart. `launchCommand` is the command the live process really runs, rebuilt from its argv and env (`CLAUDE_CONFIG_DIR=… claude --dangerously-skip-permissions`) — a shell alias is expanded before `exec`, so it is the only record of which config dir holds the session. `null` for agents with no verified session-flag list, and on Windows, where argv is unreadable. `provider` is the model provider the session runs on, for ego only: the first segment of `model` in the pid-leased `<EGO_HOME>/sessions/<id>/manifest.json`; the sidebar shows it as tooltip text (`null` when unreadable). The session always uses the ego icon, regardless of provider |
| `verify_agent_session` | `agent_type, session_id, cwd, agent_pid, env_overrides` | `bool` | Verify that the session file exists in the agent's selected profile. A live PID can supply process environment; saved launch environment selects the profile after restart. HTTP parity: `POST /agents/verify-session` |

## Panel Windows (`panel_window.rs`)

Detaching a panel into its own OS window. Used by the AI Chat panel, the
Activity Dashboard and the Git panel.

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `open_panel_window` | `panel_id, title?, params?, width?, height?` | `()` | Open (or focus) a detached panel window. `panel_id` becomes the window label prefix (`panel-{id}`). URL: `/?mode=panel&panel={id}&{params}`. Emits `panel-window-closed { panelId }` on destroy |
| `close_panel_window` | `panel_id` | `()` | Close a detached panel window by ID |
| `focus_main_window` | — | `()` | Bring the main window to foreground (used by detached panels after cross-window actions) |

## The embedded AI engine is gone (#784-0aec)

The embedded ReAct loop, provider registry and model-key storage were removed. `ego` supplies intelligence over ACP — see **ACP client for ego** below. PR review, changelog generation and improvement scanning use unattended ego turns.

`ai_agent/` retains only `tui_detect.rs`, which classifies terminal activity. The write-only command knowledge store and its persistence task were removed in #2115-1333. No knowledge accessor command remains.

## MCP OAuth 2.1 (`mcp_oauth/commands.rs`)

OAuth 2.1 authorization for upstream MCP servers. Full RFC 9728 (Protected Resource Metadata) + RFC 8414 (Authorization Server Discovery) flow with PKCE S256. Completion via the `tuic://oauth-callback` deep link.

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `start_mcp_upstream_oauth` | `name: String` | `StartOAuthResponse` | Begin an OAuth flow for the named upstream. Transitions status to `authenticating`, returns the authorization URL + AS origin for the consent dialog. PKCE challenge is generated and stored per pending flow |
| `mcp_oauth_callback` | `code: String, oauth_state: String` | `()` | Consume the `tuic://oauth-callback?code=…&state=…` deep link. Exchanges the code for tokens, persists `OAuthTokenSet` to the OS keyring, transitions upstream to `connecting` |
| `cancel_mcp_upstream_oauth` | `name: String` | `()` | Abort an in-flight OAuth flow. Drops the pending entry and resets upstream status |

## MCP Upstream Proxy (`mcp_upstream_config.rs`, `mcp_upstream_credentials.rs`)

Commands for managing upstream MCP servers proxied through TUICommander's `/mcp` endpoint.

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `load_mcp_upstreams` | -- | `UpstreamMcpConfig` | Load upstream config from `mcp-upstreams.json` |
| `save_mcp_upstreams` | `base: UpstreamMcpConfig, config: UpstreamMcpConfig, credentials?: {server_id, token, header?}[]` | `()` | Add-time credentials are validated and created under the config lock; failures roll back new vault entries. Apply the caller's ID-keyed base-to-config delta to the latest locked `mcp-upstreams.json`, validate it, and hot-reload the exact persisted change. Removing a server or its optional `auth` field is an explicit deletion; unrelated concurrent changes are preserved |
| `reconnect_mcp_upstream` | `name: String` | `()` | Disconnect and reconnect a single upstream by name. Useful after credential changes or transient failures |
| `get_mcp_upstream_status` | -- | `Vec<UpstreamStatus>` | Get live status of all upstream MCP servers. Status values: `connecting`, `ready`, `circuit_open`, `disabled`, `failed`, `authenticating`, `needs_auth` |
| `save_mcp_upstream_credential` | `name: String, token: String, url: String, header?: {name, credential_ref}, replace?: boolean` | `()` | Refuse to overwrite an existing entry unless `replace: true` explicitly requests an edit. Store a Bearer token or scoped header secret with its intended HTTP origin in the OS credential vault |
| `delete_mcp_upstream_credential` | `name: String, header?: {name, credential_ref}` | `()` | Remove a Bearer/OAuth token or a scoped custom header secret (idempotent) |

### UpstreamMcpConfig schema

```typescript
interface UpstreamMcpConfig {
  servers: UpstreamMcpServer[];
}

interface UpstreamMcpServer {
  id: string;              // Unique UUID, used for config diff tracking
  name: string;            // Namespace prefix — must match [a-z0-9_-]+
  transport: UpstreamTransport;
  enabled: boolean;        // Default: true
  timeout_secs: number;    // Default: 30 (0 = no timeout, HTTP only)
  tool_filter?: ToolFilter; // Optional allow/deny filter
}

type UpstreamTransport =
  | { type: "http"; url: string }
  | { type: "stdio"; command: string; args: string[]; env: Record<string, string> };

interface ToolFilter {
  mode: "allow" | "deny";
  patterns: string[];  // Exact names or trailing-* glob prefix patterns
}
```

### Upstream status values

The live registry exposes status via SSE events (`upstream_status_changed`). Valid status strings:

| Value | Meaning |
|-------|---------|
| `connecting` | Handshake in progress |
| `ready` | Tools available |
| `circuit_open` | Circuit breaker open, backoff active |
| `disabled` | Disabled in config |
| `failed` | Permanently failed, manual reconnect required |

## Agent MCP Configuration (`agent_mcp.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `get_agent_mcp_status` | `agent` | `AgentMcpStatus` | Check MCP config for an agent |
| `install_agent_mcp` | `agent` | `String` | Install TUICommander MCP entry |
| `remove_agent_mcp` | `agent` | `String` | Remove TUICommander MCP entry |
| `get_agent_config_path` | `agent` | `String` | Get agent's MCP config file path |
| `get_mcp_bridge_info` | — | `McpBridgeInfo` | Bridge path + ready-to-paste JSON config snippet |

## Prompt Processing (`prompt.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `extract_prompt_variables` | `content` | `Vec<String>` | Parse `{var}` placeholders |
| `process_prompt_content` | `content, variables` | `String` | Substitute variables |
| `resolve_context_variables` | `repo_path: String` | `HashMap<String, String>` | Resolve git context variables (branch, diff, changed_files, commit_log, etc.) for smart prompt substitution. Best-effort: variables that fail are omitted |

## Smart Prompt Execution (`smart_prompt.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `execute_headless_prompt` | `command: String, args: Vec<String>, stdin_content: Option<String>, timeout_ms: u64, repo_path: String, env: Option<HashMap<String,String>>` | `Result<String, String>` | Spawn a one-shot agent process in argv form (no shell — metacharacters in args are literal). Prompt content piped via stdin. Timeout capped at 5 minutes |
| `execute_shell_script` | `script_content: String, timeout_ms: u64, repo_path: String` | `Result<String, String>` | Execute shell script content directly via platform shell (sh/cmd). No agent involved — runs the content as-is. Captures stdout. Timeout capped at 60 seconds |

## Claude Usage (`claude_usage.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `get_claude_usage_api` | `sessionId?` | `UsageApiResponse` | Fetch rate-limit usage for the Claude session's credential profile; omitted session selects the default profile |
| `get_claude_usage_timeline` | `scope, days?` | `Vec<TimelinePoint>` | Hourly token usage from session transcripts |
| `get_claude_session_stats` | `scope` | `SessionStats` | Aggregated token/session stats from JSONL transcripts |
| `get_claude_project_list` | -- | `Vec<ProjectEntry>` | List project slugs with session counts |
| `get_codex_usage_api` | -- | `CodexUsageApiResponse` | Fetch rate-limit usage through the official Codex App Server |
| `get_codex_usage_stats` | -- | `CodexStatsResponse` | Fetch daily token history and supported lifetime stats through the Codex App Server |
| `get_grok_usage_api` | -- | `GrokUsageApiResponse` | Fetch billing-period usage through Grok Build's `_x.ai/billing` ACP extension |
| `set_terminal_theme_colors` | `foreground: [u8;3]`, `background: [u8;3]`, `cursor: [u8;3]` | `()` | Publish the resolved terminal theme so the emulator can answer OSC 10/11/12 colour queries |

`scope` values: `"all"` (all projects) or a specific project slug. `days` defaults to 7.

Uses incremental parsing with a file-size-based cache (`claude-usage-cache.json`) so only newly appended JSONL data is processed on each call. The cache is persisted across app restarts.

## Voice Dictation (`dictation/`)

These commands stay in the root `dictation/commands.rs` adapter; their audio and speech operations use `tuic-dictation`. The command names and payloads are unchanged by the crate split.

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `start_dictation` | `source?` (`"fn"`, `"hotkey"`, `"ui"`) | `()` | Start recording; Fn starts are refused after native key release |
| `stop_dictation_and_transcribe` | -- | `TranscribeResponse` | Stop + transcribe. Returns `{text, skip_reason?, duration_s, truncated_s}`; a capture without sustained speech returns `no sustained speech` before Whisper, a final transcription skip keeps its specific reason, an empty successful result says `no speech detected`, and `truncated_s` counts captured audio lost before transcription |
| `inject_text` | `text` | `String` | Apply corrections |
| `get_dictation_status` | -- | `DictationStatus` | Model/recording status plus normalized `audio_level` (0–1) |
| `get_model_info` | -- | `Vec<ModelInfo>` | Available models |
| `download_whisper_model` | `model_name` | `String` | Download model |
| `delete_whisper_model` | `model_name` | `String` | Delete model |
| `get_speech_assets` | -- | `Vec<SpeechAssetInfo>` | The ONNX runtime, the speech languages, then every catalogue voice (`kind: "voice"`, with `language` and `voice`), each `absent`/`downloading`/`incomplete`/`ready` |
| `download_speech_asset` | `asset` | `String` | Download and install, verifying every pinned sha256. `asset` is an id from the catalogue allowlist |
| `cancel_speech_download` | `asset` | `String` | Abandon a download in flight |
| `delete_speech_asset` | `asset` | `String` | Unload the engine, then remove the files |
| `get_speech_voices` | `language` | `Vec<VoiceChoice>` | The voices a language (Whisper code) can speak with now: `{ id, source }` with `source` `"default"`, `"downloaded"` or `"user"`; empty while the language is not fully downloaded |
| `import_speech_voice` | `language`, `name`, `dataBase64` | `String` | Store a user voice file (base64) at `<speech>/user-voices/<language>/<name>.safetensors`. Refused with the reason for a bad name, a catalogue voice name, more than 64 MB, a file that is not safetensors, or a voice that does not fit the language's model |
| `delete_speech_voice` | `language`, `name` | `String` | Remove a user voice file. Absent is success |
| `preview_speech_voice` | `language`, `voice`, `text` | `()` | Speak `text` (max 200 characters) in `voice` on this machine's speaker with the saved loudness. Needs no hands-free conversation and does not change `speech_voice`. Refused while a hands-free reply is queued, rendering or playing |
| `speak_reply` | `text`, `turn?` | `SpokenReply` | Queue one spoken reply, max 2000 characters. Returns `state: "queued"` — never `"finished"`; poll `get_speech_status` with the id. `turn` refuses a reply written for a turn the user talked over |
| `stop_speech` | -- | `SpeechStatus` | Stop now, drop the queue, open a new turn. Returns the status so the caller learns that turn |
| `pause_speech` | -- | `SpeechStatus` | Hold the reply where it is; `paused` true, `speaking` false. No-op with nothing playing |
| `resume_speech` | -- | `SpeechStatus` | Continue a reply the user held |
| `get_speech_status` | `utterance?` | `SpeechStatus` | Whether anything can be spoken, and optionally what became of one reply. An id no longer remembered reports `state: "unknown"`. `language` is the conversation's language — empty under `auto` before the first turn, which is the one state in which nothing can be spoken |
| `get_correction_map` | -- | `HashMap<String,String>` | Load corrections |
| `set_correction_map` | `map` | `()` | Save corrections |
| `list_audio_devices` | -- | `Vec<AudioDevice>` | List input devices |
| `get_dictation_config` | -- | `DictationConfig` | Load saved config. With an activation phrase, the hands-free runtime applies at least 5000 ms of hold-back even when the saved `hands_free_hold_back_ms` is shorter; hands-free status reports the effective value |
| `get_hands_free_default_notice` | -- | `string` | The built-in hands-free start notice, sent while `hands_free_start_notice` is empty |
| `set_dictation_config` | `base, config` | `()` | Save only the changes between the loaded base and edited config. A changed `language`, `speechCommand` or `speech_voice` also drops the voice built for the previous one, cancelling what it was speaking; hands_free_spoken_replies=false also cancels it and refuses new speech while dictation stays armed. Edge 401/403 rejection makes speech unavailable for five minutes. Other fields leave the voice alone. `hands_free_listening` (default true, read at arm time only; false arms a replies-only conversation with no microphone, and arming with it and `hands_free_spoken_replies` both false is refused). `hands_free_earcons` (default true) turns the hands-free earcons off; only the frontend reads it. `hands_free_notify_model` (default true) is read at arm time only — turning it off mid-conversation does not cancel the end notice the model is already owed. `hands_free_start_notice` (default empty = built-in text) replaces the start notice, also at arm time only, folded to one line |
| `check_microphone_permission` | -- | `String` | Check macOS microphone TCC permission status |
| `open_microphone_settings` | -- | `()` | Open macOS System Settings > Privacy > Microphone |

## Native file dialogs (`native_dialog.rs`)

Desktop-only, therefore `INTENTIONALLY_UNMAPPED` in `transport.ts`: the panel
browses the **host's** filesystem, and a remote client uses the in-app file
browser instead.

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `pick_path` | `kind ("file"\|"files"\|"folder"\|"save"), title?, defaultPath?, fileName?, filters?` | `Option<Vec<String>>` | Open a native file/folder/save panel. `null` when the user cancels; always an array otherwise, even for a single pick. `Err` — never a crash — when the panel cannot be built |

Call it through `src/utils/nativeDialog.ts` (`openDialog`/`saveDialog`), not
directly, and **do not** import `open`/`save` from `@tauri-apps/plugin-dialog`.
The plugin builds `NSOpenPanel`/`NSSavePanel` inside its own main-thread closure,
so when AppKit's window-server link is interrupted — the state the Mac wakes into
after standby — the binding's NULL check panics on the main thread and the
process dies with every live PTY session. `native_dialog.rs` owns that closure so
the unwind is caught and returned as an error. The plugin's `confirm`/`message`
are unaffected (they build `NSAlert`) and stay as they are.

## Filesystem (`fs.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `resolve_terminal_path` | `cwd, candidate` | `Option<ResolvedFilePath>` | Resolve one terminal path candidate against `cwd`; `null` on a miss |
| `resolve_terminal_paths` | `cwd, candidates` | `Vec<Option<ResolvedFilePath>>` | Batched form, answered **positionally**: entry `i` is the result for `candidates[i]`. One IPC round-trip per terminal screen instead of one per candidate |
| `resolve_markdown_link` | `root, currentFile, href` | `MarkdownLinkTarget` | Decode and resolve a rendered Markdown link relative to its file; return a heading, file, missing path, or blocked network path. Runs filesystem work on the blocking pool |
| `list_directory` | `repoPath, subdir` | `Vec<DirEntry>` | List directory contents; links add `symlink_target` (link text) and `symlink_target_kind` (`directory`, `file`, `unavailable`), with size 0 |
| `get_home_directory` | — | `String` | Return this machine's home directory for local or remote browsing |
| `fs_read_file` | `path` | `String` | Read file contents |
| `write_file` | `path, content` | `()` | Write file |
| `write_file_if_unchanged` | `repo_path, file, expected, content` | `bool` | Write only when the file still equals `expected`; `false` = changed on disk, nothing written (mobile Markdown review) |
| `create_directory` | `path` | `()` | Create directory |
| `delete_path` | `path` | `()` | Delete file, directory, or symbolic link; links are removed without following their targets |
| `rename_path` | `src, dest` | `()` | Rename/move path |
| `copy_path` | `src, dest` | `()` | Copy file or directory |
| `copy_path_abs` | `from, to` | `()` | Copy a file by absolute paths (cross-repo paste). Rejects directories. |
| `move_path_abs` | `from, to` | `()` | Move a file by absolute paths (cross-repo cut+paste); copy+remove fallback across filesystems. |
| `fs_transfer_remote_paths` | `connectionId, destDir, paths, allowRecursive` | `TransferResult` | Sender-side coordinator for copying local OS paths onto a connected remote repository using its existing endpoint/token. Streams bounded archives; preserves recursion confirmation and skips existing names. Desktop IPC only (`INTENTIONALLY_UNMAPPED`): data leaves the machine; Finder source paths cannot be gated to registered roots; HTTP token holders must not trigger exfiltration. |
| `fs_transfer_paths` | `destDir, paths, mode ("move"\|"copy"), allowRecursive` | `TransferResult { moved, skipped, errors, needs_confirm }` | Move/copy OS paths into a destination directory. Skips silently on name conflicts; returns `needs_confirm=true` (no-op) when a source is a directory and `allowRecursive=false`. Used by the drag-drop handler when dropping files onto a folder in the file browser. |
| `add_to_gitignore` | `path, pattern` | `()` | Add pattern to .gitignore |
| `search_files` | `path, query` | `Vec<SearchResult>` | Search files by name in directory |
| `search_content` | `repoPath, query, searchId, caseSensitive?, useRegex?, wholeWord?, limit?` | `()` | Full-text content search; streams results progressively via `content-search-batch` events, each echoing `searchId`. Binary files and files >1 MB are skipped. Supports cancellation. |
| `search_content_all` | `query, searchId, caseSensitive?, limit?` | `()` | Cross-repo BM25 content search over every ready index; streams via the same `content-search-batch` events with each match tagged `repo_path` and every batch echoing `searchId`. Only repos whose index is built participate (depends on Content Indexing strategy). Shares the cancellation slot with `search_content`. |
| `warm_content_index` | `repoPath` | `()` | Build a repo's content index in the background, invoked on repo switch. **Strategy-gated:** under `disabled` or `active_only` it returns without scheduling anything, so a repo switch cannot index behind a setting that asked it not to. The `POST /fs/warm-index` route applies the same gate. |

`search_content_all` reports why a repo produced nothing, so the UI can tell
"no match" apart from "not searched yet". Every `ContentSearchResult` and every
`content-search-batch` carries `repos_searched`, `repos_pending`, and
`repos_indexing` — the last being the subset of `repos_pending` with a build
actually in flight. Only that subset justifies telling the user to retry; the
rest are waiting on a scheduling event that may never come.

For both commands, every `ContentMatch.match_start`/`match_end` pair is a
zero-based, end-exclusive UTF-16 code-unit range within `line_text`, matching
JavaScript `String.slice` semantics for ASCII, accented text, and non-BMP emoji.

## Plugin Management (`plugins.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `list_user_plugins` | `pluginIds?: string[]` | `Vec<PluginManifest>` | List valid plugin manifests with backend-derived optional `contentSha256` |
| `get_plugin_readme_path` | `id` | `Option<String>` | Get plugin README.md path |
| `read_plugin_data` | `plugin_id, path` | `Option<String>` | Read plugin data file |
| `write_plugin_data` | `plugin_id, path, content` | `()` | Write plugin data file |
| `delete_plugin_data` | `plugin_id, path` | `()` | Delete plugin data file |
| `install_plugin_from_zip` | `path` | `PluginManifest` | Install from local ZIP |
| `install_plugin_from_url` | `url` | `PluginManifest` | Install from HTTPS URL |
| `uninstall_plugin` | `id` | `()` | Remove plugin and all files |
| `install_plugin_from_folder` | `path` | `PluginManifest` | Install from local folder |
| `register_loaded_plugin` | `plugin_id` | `()` | Register a plugin as loaded (for lifecycle tracking) |
| `unregister_loaded_plugin` | `plugin_id` | `()` | Unregister a plugin (on unload/disable) |
| `set_plugin_output_watchers` | `client_id`, `seq`, `watchers: [{ id, pattern, flags }]` | `{ applied, rejected }` | Replace the OutputWatcher set of one frontend — the patterns the PTY reader thread matches lines against. The frontend pushes its whole set on every add or remove; sets are per `client_id`, and `seq` orders the mutations so a stale sync answers `applied: false` and changes nothing. Tauri IPC sets are released when their WebView navigates or closes. Browser clients have no reliable disconnect signal, so they re-send every 30 s to recover from bounded eviction and keep live sets recent. `rejected` lists the ids the Rust `regex` crate cannot compile (lookaround, backreferences, a negated class escape inside a character class); those watchers keep matching in the WebView, which then receives every line. |

## Plugin Filesystem (`plugin_fs.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `plugin_read_file` | `path, plugin_id` | `String` | Read file as UTF-8 (within $HOME, 10 MB limit) |
| `plugin_read_file_base64` | `path, max_bytes?, plugin_id` | `String` | Read file bytes as base64 within `$HOME`; default 10 MiB, positive custom budgets clamped to 512 MiB |
| `plugin_write_file_base64` | `path, content, max_bytes?, plugin_id` | `()` | Atomically replace a file from base64 bytes within `$HOME`; default 10 MiB, positive custom budgets clamped to 512 MiB |
| `plugin_read_file_tail` | `path, max_bytes, plugin_id` | `String` | Read last N bytes of file, skip partial first line |
| `plugin_list_directory` | `path, pattern?, plugin_id` | `Vec<String>` | List filenames in directory (optional glob filter) |
| `plugin_watch_path` | `path, plugin_id, recursive?, debounce_ms?` | `String` (watch ID) | Start watching path for changes |
| `plugin_unwatch` | `watch_id, plugin_id` | `()` | Stop watching a path |
| `plugin_write_file` | `path, content, plugin_id` | `()` | Write file within $HOME (path-traversal validated) |
| `plugin_rename_path` | `src, dest, plugin_id` | `()` | Rename/move path within $HOME (path-traversal validated) |

## Plugin HTTP (`plugin_http.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `plugin_github_action` | `plugin_id, account, reference, request` | `Value` | Account-scoped B6/B7 bridge; requires net:http, manifest github provider consent and matching PR reference |
| `plugin_http_fetch` | `url, method?, headers?, body?, account?, plugin_id, request_id?` | `HttpResponse` | Make HTTP request (validated against the on-disk manifest `allowedUrls`; with `account`, Rust injects that account's credential under the account host rule) |
| `plugin_http_cancel` | `plugin_id, request_id` | `()` | Cancel a plugin-scoped HTTP request, including its response body. |

## Code Intelligence / MDKB (`mdkb_commands.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `mdkb_status` | — | `MdkbStatus` | Check if mdkb binary is available and daemon connected |
| `mdkb_outline` | `repo_path, file_path` | `Vec<OutlineSymbol>` | Get symbol outline (functions, types) for a file |
| `mdkb_goto_definition` | `repo_path, file_path, line, col?` | `DefinitionLocation?` | Find definition of symbol at position |
| `mdkb_references` | `repo_path, symbol_name` | `Vec<ReferenceLocation>` | Find all callers of a symbol via code_graph |
| `mdkb_code_find` | `repo_path, name, kind?` | `CodeFindResult` | Exact symbol lookup by name, optionally filtered by kind. `symbols` is capped by mdkb; `total` is the unclamped match count and `capped` says whether rows were held back (both `null` when mdkb sent no count) |
| `install_mdkb` | — | `String` | Download and install mdkb binary |
| `uninstall_mdkb` | — | `()` | Remove mdkb binary (errors for homebrew/cargo installs) |

**Line numbers are 1-based on this boundary.** mdkb stores symbol ranges 0-based
but takes a 1-based `line` as *input* to `symbol_at_position`. `mdkb_commands::editor_line`
shifts every response so `line`/`line_start`/`line_end` reaching the frontend
match CodeMirror's `doc.line(n)`. Request args (`mdkb_goto_definition`'s `line`)
are already 1-based and pass through unchanged.

**Each mdkb method has a different response shape** — do not assume "a JSON array
of symbols":

| mdkb hook method | `result` shape |
|---|---|
| `symbols_in_file` | `text` = stringified **bare array** |
| `symbol_at_position` | `text` = stringified **object**, or the literal `"null"` |
| `code_find` | `text` = stringified **envelope** `{total, showing, symbols}` |
| `code_graph` | `text` = **prose for agents**; the symbols are a separate `symbols` array on `result` |

`code_graph`'s `symbols` field was added after mdkb 3.7.17. Against an older
daemon the field is absent and the call fails loudly — "no callers" is `symbols: []`,
so an absent field can only mean a stale daemon and must never be reported as an
empty result.

## Plugin CLI Execution (`plugin_exec.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `plugin_exec_cli` | `binary, args, cwd?, plugin_id` | `String` | Execute whitelisted CLI binary, return stdout. Allowed: `mdkb`. 30s timeout, 5 MB limit. |

## Plugin Credentials (`plugin_credentials.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `plugin_read_credential` | `service_name, plugin_id` | `String?` | Read credential from system store (Keychain/file) |

## Plugin Registry (`registry.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `fetch_plugin_registry` | -- | `Vec<RegistryEntry>` | Fetch remote plugin registry index |

## Watchers

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `start_head_watcher` | `path` | `()` | Watch .git/HEAD for branch changes |
| `stop_head_watcher` | `path` | `()` | Stop watching .git/HEAD |
| `start_repo_watcher` | `path` | `()` | Watch .git/ for repo changes |
| `stop_repo_watcher` | `path` | `()` | Stop watching .git/ |
| `start_dir_watcher` | `path` | `()` | Watch directory for file changes (non-recursive) |
| `stop_dir_watcher` | `path` | `()` | Stop watching directory |
| `set_hot_repos` | `paths: Vec<String>` | `()` | Set repos with active terminals (cold repos get throttled watchers/polling) |

## System (`lib.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `load_config` | -- | `AppConfig` | Alias for load_app_config |
| `save_config` | `base, config` | `()` | Alias for save_app_config |
| `hash_password` | `password` | `String` | Bcrypt hash |
| `list_markdown_files` | `path` | `Vec<MarkdownFileEntry>` | List .md files in dir |
| `read_file` | `path, file` | `String` | Read file contents |
| `get_mcp_status` | -- | `JSON` | MCP server status including live `tls_active` (not Tailscale capability), plus unfiltered `native_tools: [{name, summary, description}]` Settings inventory (no token — use `get_connect_url` for QR) |
| `get_mcp_audit` | -- | `JSON` | Body-free MCP tools/call outcome audit (same body as `GET /mcp/audit`); `{enabled:false,records:[]}` while `mcp_call_audit_enabled` is off |
| `suspended_session_action` | `request` | JSON | Rust-owned durable suspend/resume, metadata sync, list, forget and verified resume resolution. HTTP: `POST /sessions/suspended`; actions and shapes documented in HTTP API |
| `mcp_confirm_response` | `request_id, confirmed` | `()` | Answer a pending `ui(action=confirm)`. HTTP: `POST /mcp/confirm-response`. Every client is shown the same request and the first answer wins, so an unknown or already-answered id is a no-op, not an error |
| `get_connect_url` | `ip` | `String` | Build QR connect URL server-side (token stays in backend) |
| `check_update_channel` | `channel` | `UpdateCheckResult` | Check beta/nightly channel for updates (hardcoded URLs, SSRF-safe) |
| `clear_caches` | -- | `()` | Clear in-memory caches |
| `get_local_ip` | -- | `Option<String>` | Get primary local IP |
| `get_local_ips` | -- | `Vec<LocalIpEntry>` | List local network interfaces |
| `regenerate_session_token` | -- | `()` | Regenerate MCP session token (invalidates all remote sessions) |
| `fetch_update_manifest` | `url` | `JSON` | Fetch update manifest via Rust HTTP (bypasses WebView CSP) |
| `read_external_file` | `path` | `String` | Read file outside repo (standalone file open) |
| `get_relay_status` | -- | `JSON` | Cloud relay connection status |
| `get_tailscale_status` | -- | `TailscaleState` | Tailscale daemon status (NotInstalled/NotRunning/Running with fqdn, https_enabled) |

## Global Hotkey

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `set_global_hotkey` | `combo: Option<String>` | `()` | Set or clear the OS-level global hotkey. There is no getter: the current combo is the `global_hotkey` field of `AppConfig`, read through `load_config`. |

## App Logger (`app_logger.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `push_log` | `level, source, message` | `()` | Push entry to ring buffer (survives webview reloads) |
| `get_logs` | `level?, source?, limit?` | `Vec<LogEntry>` | Query ring buffer with optional filters |
| `clear_logs` | -- | `()` | Flush all log entries |

## Notification Sound (`notification_sound.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `play_notification_sound` | `sound, volume, device?` | `()` | Play a Rust rodio notification sound (`question`, `completion`, `error`, `warning`, `info`, or `attention`) on `device`, or the system default; every tone releases to zero and drains a short silent tail before the output stream closes |
| `block_sleep` | -- | `()` | Prevent system sleep |
| `unblock_sleep` | -- | `()` | Allow system sleep |

## ACP client for ego (`acp_commands.rs`)

Drives an [ego](https://github.com/sstraus/ego) agent over the Agent Client
Protocol. Every command is a one-line pass-through to `AcpClientManager`; each
has an identical HTTP route (see `docs/api/http-api.md`) so a browser or the
PWA gets the same answers, including the same error bodies.
The shared event stream sends `turnFailed {message, state}` when an accepted
prompt later fails; the event envelope identifies its session and turn.

The binary this launches is **not** an argument. It comes from the
`ego_executable` setting, read at each connect, so no caller over IPC or HTTP
can choose what the host runs.

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `acp_workspace_root` | — | `String` | The directory AI Chat runs in, from `ai_chat_workspace` (empty = home directory of this host). A missing folder is created; an unusable one is refused with a message naming the setting |
| `acp_connect` | `root` | `AcpConnectionSnapshot` | Launch the configured ego in `root` and initialize a connection |
| `acp_reconnect` | `connectionId, root` | `AcpConnectionSnapshot` | Settle the old connection and start a new generation |
| `acp_disconnect` | `connectionId` | `AcpConnectionSettlement` | Ask the child to exit; the snapshot is retained |
| `acp_kill` | `connectionId` | `AcpConnectionSettlement` | Kill the child without asking |
| `acp_connection_snapshot` | `connectionId` | `AcpConnectionSnapshot` | State, capabilities, attachments, and the journal's readable range |
| `acp_subscribe` | `connectionId, afterSequence, channel` | `()` | Stream `AcpStreamFrame`s over a dedicated Channel. The browser equivalent is a WebSocket, not an HTTP route |
| `acp_session_new` | `connectionId, authority` | `AcpAttachmentSnapshot` | `session/new` |
| `acp_session_list` | `connectionId, cwd?, cursor?` | `ListSessionsResponse` | `session/list` |
| `acp_session_load` | `connectionId, sessionId, authority` | `AcpAttachmentSnapshot` | `session/load` |
| `acp_session_resume` | `connectionId, sessionId, authority` | `AcpAttachmentSnapshot` | `session/resume` |
| `acp_session_fork` | `connectionId, sessionId, authority` | `AcpAttachmentSnapshot` | `session/fork` |
| `acp_session_delete` | `connectionId, sessionId` | `()` | `session/delete` — the session is gone for good |
| `acp_session_close` | `connectionId, sessionId` | `()` | Detach without deleting |
| `acp_session_prompt` | `connectionId, sessionId, prompt, viewedRepo?` | `AcpTurnId` | Start a turn or accept it into the server-owned FIFO when one is running. `viewedRepo` is sent as `_meta.tuicommander/viewedRepo`, a hint for that turn only |
| `acp_session_cancel` | `connectionId, sessionId` | `()` | Cancel the running turn |
| `acp_queued_prompt_cancel` | `connectionId, sessionId, turnId` | `()` | Remove one queued prompt before it reaches ego |
| `acp_session_set_config_option` | `connectionId, sessionId, configId, value` | `Vec<SessionConfigOption>` | Set one option; the agent returns the whole resulting set |
| `acp_turn_pause` | `connectionId, sessionId, requestId` | `EgoHoldResponse` | `_ego/pause`. `state` may be `pending` — the hold has not landed yet |
| `acp_turn_resume` | `connectionId, sessionId, requestId` | `EgoHoldResponse` | `_ego/resume` |
| `acp_session_compact` | `connectionId, sessionId, requestId` | `EgoCompactResponse` | `_ego/compact`. A `durability_uncertain` publication is never retry-safe |
| `acp_pending_interactions` | `connectionId` | `Vec<AcpPendingInteraction>` | Questions the agent is waiting on, for a client that was not listening when they were asked |
| `acp_respond_permission` | `connectionId, requestId, outcome` | `AcpInteractionSettlement` | Answer a `session/request_permission` |
| `acp_respond_elicitation` | `connectionId, requestId, action` | `AcpInteractionSettlement` | Answer a `session/create_elicitation` |
| `acp_one_shot_prompt` | `root, prompt` | `EgoTurn` | One unattended turn: launch, prompt, shut down. No connection id — it owns the whole lifetime |

Errors are an `AcpClientError` — `code`, `message`, `connectionId`,
`sessionId`, `operation`, `retryable` — identical on both transports.

`acp_one_shot_prompt` is the odd one out and deliberately so. It is what a Smart
Prompt in `api` mode runs on, with no panel open and nobody watching, so its
session is opened with **no MCP server** and every permission request and
elicitation is refused the instant it arrives. `EgoTurn` is
`{ text, stopReason, declined }`; `declined` separates "ego had nothing to say"
from "ego wanted a tool this mode cannot grant". The turn is abandoned after
300s, a server-side constant rather than an argument.

## ego command line (`ego_cli.rs`)

Reads and writes **ego's** configuration by running ego, for the Settings → AI
Chat page. TUIC stores no API key and makes no provider HTTP call; the one
network call is `ego models --refresh`, which ego makes, and only when asked.

Four rules hold this surface down:

- The binary is the configured `ego_executable`, read per call. No argument
  names a program.
- There is no key parameter. Dedicated operations write only `model`, `roots`
  and `network`, so a caller cannot reach `sandbox` or
  `permissions.judge` at all.
- A model value may not look like a flag, carry whitespace or a control character, or
  contain `"` or `\` — the last two would break out of ego's TOML string.
- A failure carries what ego printed.

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `ego_providers` | `refresh?` | `EgoProviders` | `config ls --profile=<configured> --json` + `models --json` + `doctor --json`, joined in the configured AI Chat workspace. The profile flag is omitted for the base. `refresh: true` adds `--refresh`, the only call that reaches a provider |
| `ego_set_default_model` | `model` | `EgoProviders` | `config set --profile=<configured> -- model="<slug>"` in the configured AI Chat workspace (profile omitted for the base), then a fresh read — the answer is what ego persisted, not what was sent |
| `ego_perimeter` | -- | `PerimeterView` | Stored `config ls --json` plus `config ls --effective --json` in the AI Chat workspace/profile |
| `ego_set_perimeter_roots` | `roots: {rootDir, rootAccess, readAllowlist, writableDirs}` | `PerimeterView` | Encode a typed `roots=[...]` assignment, run `config set --`, then read back |
| `ego_set_perimeter_network` | `enabled: bool` | `PerimeterView` | `config set -- network="on"` or `network="off"`, then read back |

Perimeter operations select `--profile` from `ego_profile` and cwd from
`ai_chat_workspace` (empty means host HOME); that directory must exist. They
remove ambient `EGO_PROFILE` so the preview matches the configured ACP profile.
Path strings are TOML-encoded in Rust, never written to the TOML file by TUIC.
Each allowlist is newline-delimited; the primary root is one path. `rootAccess`
is `read` or `read-write`. Paths must be absolute or start with `~/`; limits are
128 entries and 64 KiB of path input. Empty fields encode explicit `roots=[]`.
The response contains the edit fields, `networkEnabled`, selected `profile`,
`effective` (ego's original snake_case JSON schema), `execEnforcement`
(`enforcedByOs`, `promptOnly`, `notChecked`), and a display-ready `preview`.
Capability evidence remains exactly what ego reports: a measured string array
or `not_checked`, with optional `capabilities_reason` and `probe_evidence`.
Rust classifies the exec badge: `ro` needs `read_scoped` + `write_denied`,
`workspace` needs `read_scoped` + `write_scoped`, and offline adds `no_ip_network`.
An empty/partial measured set is Prompt only; unknown evidence is unverified.
The measured form needs its probe reference. `off` + online is Prompt only,
never a vacuous OS-enforcement success. Headless inspection currently has no
measurement, so it reports not checked and its reason.

All three reads must succeed. A partial answer would render as a tab silently
missing one of the three things it exists to show.

Errors are an `EgoCliError` — `code` (`notConfigured`, `invalidInput`,
`launchFailed`, `commandFailed`, `unreadableOutput`), `message`, `command`,
`stdout`, `stderr`, `exitCode` — identical on both transports. `command` is
spelled by file name only, and the captured output is clipped to 4000
characters on a character boundary.

## Private secret form commands

`secret_form_bootstrap` takes no arguments and checks the native caller window
against the backend-created private window. It is intentionally unmapped: an
HTTP client must already possess the privately delivered capability.
`secret_form_submit {submission}` checks the same native identity and uses the
same schema and consumption logic as `POST /secrets/forms/submit`.

Workflow definition and human-decision IPC commands use host Human authority. HTTP equivalents require verified credentials for that authority; local address admission grants LocalApi only. Native story plan_state and plan_view use current integration receipts for workflow-owned plans.

### Stored terminal marker coordinates

OSC 133 event `line` and hook-generated `UserInput.line` use all-time rows. `terminal_scroll_to`, `terminal_get_lines` and search results keep their retained-grid coordinates; callers subtract the current frame `historyBase` when using stored markers.

### Telegram Settings

- `telegram_settings`: safe settings snapshot (`enabled`, `token_set`, `bot_alias`, `registered_agent_name` (nullable), decimal-string `chats`, `connected`, `last_error`, `last_message_time`). Never returns a token or message text.
- `telegram_setup { change }`: `change.action` is `token` (password `token`, checks `getMe`), `pair` (returns `code`, `expires_in_seconds`), `add_chat`/`remove_chat` (`chat_id`), or `configure` (`enabled`). Errors are typed safe Telegram categories.

The daemon executor now owns recovery and duration timers under an OS run-database lock. Reads never recover live work, and a non-owner daemon refuses run mutations. Graph resume uses `resume_graph {execution_id,activation_id,resolution}` with an explicit pending activation; status-only resume cannot bypass graph position. Graph start controls, Agent effects and delivery policy remain unavailable until their later slices.

`acp_session_fork` accepts optional `atMessageId`, with the same behavior as the ACP HTTP fork route. Capability snapshots expose `forkAtMessage` separately from tip fork support.

`acp_session_list` uses the same complete ancestry projection as HTTP: original ego lineage plus `_meta.tuicommander.lineageDepth` and disabled deleted-parent placeholders.

`workflow_run_action` also accepts the read-only `incidents` action with `run_id`. It returns the same project-scoped camelCase incident entries as `POST /workflows/run/action`; see the HTTP API run incident projection.

### `acp_chat_open`

Arguments: `{ request: { sessionId?, profile?, workspace?, executable? } }`. Returns `{ connection, sessionId, launch, replayed }`. Creates a conversation with its own durable launch options, or reopens the saved `sessionId`. HTTP equivalent: `POST /acp/chat/open` with the request object as the body. Global ACP connect defaults remain unchanged.

MCP `session action=declare_worktree` is intentionally not a Tauri command. It
requires a bound managed caller, which window IPC does not supply. Call it via
HTTP `POST /mcp`. Its `session-worktree-declared` push is also emitted to Tauri;
`list_active_sessions` and HTTP `GET /sessions` report the persisted declaration
in their existing worktree fields. See [MCP backend](../backend/mcp-http.md).

### Desktop text clipboard

| Command | Parameters | Returns | Description |
|---------|------------|---------|-------------|
| `write_clipboard_text` | `text: String` | `()` | Write UTF-8 text to the local OS clipboard through arboard. Errors reject the call so copy actions show failure. |
| `read_clipboard_text` | — | `String` | Read UTF-8 text directly from the local OS clipboard without the macOS Web Clipboard Paste pill. |

Both commands require the `desktop` feature and run blocking clipboard operations
on worker threads. They have no focus or user-gesture requirement. The clipboard
owner stays alive between calls and is released on application exit. They are
`INTENTIONALLY_UNMAPPED`: browser clients use `navigator.clipboard` on their own
machine; HTTP clients cannot read or write the host pasteboard.

### Automations

`automation_action(input: serde_json::Value) -> Result<serde_json::Value, String>`
is an async command that shares parsing, definition edits, history, schedule
previews/presets, UTC run summaries and owner-guarded Run Now with
`POST /automations/action`. Invoke with `{input:{action:"list"}}`; response values
and application error messages match HTTP. See the HTTP Automations action table
for every field and reply. Blocking storage/schedule work runs off the main
thread. Run Now executes only on the addressed backend; Once and cron definitions both run.

`automation_action` also accepts `{input:{action:"run",id:"<run id>"}}` to fetch a canonical saved run for notification navigation. Native automation click targets are `{kind:"automation",id:"<run id>"}` on macOS.
Design Mode v2: `start_design_mode(sessionId, browser?: "edge" | "chrome", url?: string)` opens the dedicated browser profile and installs its native host. HTTP `POST /design-mode/start` accepts the identical camelCase body. Stop/status and the terminal badge retain their existing shapes. The browser opens on the backend host.

Automation precheck context (#1631-45bf): definitions can opt in with
`precheck.inject_output: true` (default false). Proceeding checks prefix the
literal prompt with delimited, escaped untrusted stdout capped at 16 KiB;
truncation is marked and stderr is excluded. Empty output and manual bypasses
inject nothing. The run ledger records `precheck_context: {injected, bytes}` for
the prepared launch prompt, counting escaped stdout bytes before framing/marker.

### PR line review commands

- `get_pr_line_threads(repoPath, prNumber, after?)`: `{headSha, threads, hasNextPage, endCursor}`. Threads retain GitHub `line`, `originalLine`, `diffSide`, `isOutdated`, `isResolved`, and `comments` connection; every comment author carries `bot` (from the GraphQL actor type), and each thread carries a nullable `finding` parsed from its first comment by `tuic_git::review_bots` (`tool`, `title`, `severity`, `severityLabel`, `category`, `suggestion`, `agentPrompt`, `addressedIn`, `externalId`; untrusted text).
- `get_pr_open_threads(repoPath, prNumber)`: the `{pr_open_threads}` smart-prompt variable. A string: a preface, then a fenced untrusted block with one line per unresolved, non-outdated thread (`- path:line · tool · severity · title · url`, most severe first, at most 30 plus `+N more open threads not listed`). Each value is flattened to one line without control characters, and the fence is longer than any backtick run in the data.
- `get_pr_thread_comments(repoPath, threadId, after?)`: GitHub comments connection.
- `set_pr_thread_resolved(repoPath, threadId, resolved)`: confirmed thread.
- `reply_pr_thread(repoPath, threadId, body)`: posted comment.
- `submit_pr_draft_review(repoPath, prNumber, draft)`: posted review. Draft `{headSha, body, event, comments}` accepts COMMENT, APPROVE, REQUEST_CHANGES; request changes needs a body. Inline comments retain path/line/side/body.

### PR action commands

- `get_pr_action_state(repoPath, prNumber)`: `{id, headSha, baseRefName, state, isDraft, autoMergeRequest, mergeQueueEntry, queueRequired}`. Queue entries retain id/position/state; auto-merge requests retain enabledAt/mergeMethod.
- `set_pr_auto_merge(repoPath, prNumber, enabled, mergeMethod?, expectedHeadSha?)`: confirmed GraphQL payload. Enabling needs a valid merge/squash/rebase method and reviewed head.
- `set_pr_merge_queue(repoPath, prNumber, enqueued, expectedHeadSha?)`: confirmed GraphQL payload with mergeQueueEntry. Enqueue needs reviewed head; dequeue does not.
- `merge_or_enqueue_pr(repoPath, prNumber, mergeMethod, expectedHeadSha)`: `{action:"enqueued", mergeQueueEntry}` or `{action:"merged", sha}`. Queue-required branches enqueue.
- `rerun_failed_pr_jobs(repoPath, runId)`: `{accepted:true, runId}` after GitHub accepts the failed-job rerun, not a claim the new run finished.

`post_pr_review` and `submit_pr_draft_review` require a nonempty body for REQUEST_CHANGES and reject unsupported event values.

## Core account command

| Command | Arguments | Result | HTTP equivalent |
|---|---|---|---|
| `account_action` | `request: AccountAction` | Metadata/presence-only action reply | `POST /accounts/action`, body = request |

The async command delegates to the same owner service as HTTP; disk/vault work runs on the blocking pool. See [the account action table](../backend/github.md#shared-account-registry).

### Private account credential entry

`account_secret_form {request}` opens the existing isolated native secret window.
The request is `{action:"add",account:{provider,host,kind,label,username?}}` or
`{action:"rotate",id}`; it accepts no credential. The reply contains only
`{names:[],status:"stored"|"declined"|"failed"}`. `secret_form_bootstrap` and
`secret_form_submit` require the active native window identity. Account forms
do not expose a mobile capability or accept HTTP bootstrap/submission.


Automation worktree cleanup (#1623-8b2a): Automation runs retain automation/run IDs and the exact created workspace path and ID in their saved snapshot, including after definition deletion. Worktree cleanup is explicit and limited to final runs. The backend resolves the saved identity and rechecks shared Git integration/archive proof, dirt, submodule commits and live sessions; it never forces removal or closes sessions. Shared `automation_action` actions `worktree_preview` and `cleanup_worktree` accept a saved run `id` on IPC, HTTP, MCP and CLI.
Automation templates (#1622-be11): the shared automation action accepts
**templates** (catalogue) and **template** (draft). Draft inputs are template_id,
repository, run_config and optional timezone (host local by default).
Drafts have fresh ids and start disabled; no definition or run is created until
an explicit create action. These actions match across HTTP, IPC, MCP and CLI.

### History-anchored viewport scrolling

`terminal_scroll_to_anchor` accepts `{ sessionId: string, anchor: number | null }` and returns `()`. The all-time row includes evicted rows, matching `historyBase + historySize - displayOffset`. Null follows output; a trimmed anchor clamps to the oldest retained row. HTTP uses `POST /sessions/:id/terminal/scroll-to-anchor` with the same anchor value.

### Desktop close policy

- `request_quit()` requests real quit regardless of `app.close_behavior`, with confirmation if enabled and terminal sessions exist.
- `ack_background_snapshot(request_id: string)` acknowledges a completed persisted terminal snapshot requested by `snapshot-now { request_id }`. Native lifecycle only; intentionally unmapped over HTTP. The desktop waits at most two seconds before hiding.
- `resolve_close(choice: "background" | "quit" | "cancel", remember: bool)` answers the pending Rust-owned close dialog. Background and Remember apply only to Ask. A failed save leaves the dialog pending.
- These native lifecycle commands are deliberately in `INTENTIONALLY_UNMAPPED`; remote clients cannot hide or quit the host. `close-dialog` carries `{kind: "ask" | "confirm_quit", sessions: number, background_available: boolean}` on desktop events and `/events` SSE.

### Tasks start-work brief

| Command | Arguments | Returns | Description |
|---------|-----------|---------|-------------|
| `build_start_work_prompt` | `repoPath, input: {key, url: string or null, title, body}` | `String` | Read `.tuic-start-work.md`, substitute sanitized fields once, or preserve the default brief when absent. HTTP: `POST /tasks/start-work-prompt`. Delivery applies the existing untrusted wrapper and 12,000-character cap. |

### Tasks provider bridge

`work_item_provider_bridge(input)` claims or completes one Rust-issued Tasks provider request. It uses the same input and reply as `POST /work-items/provider-bridge`; see the HTTP API reference. The command only moves requests and replies; the existing provider port owns tracker calls.

`project_work_items` / `POST /tasks/project`: accepts `{items, fields, query}` using the work-item DTOs. Returns `WorkItemGroupedPage` with the filtered unique count, no cursor and one unnamed group when no grouping is selected. Pure Rust projection, maximum 1,000 collected items, no provider access or persisted state.

Settings policy foundation (#1786-9cbd): `mcp_confirm_response` also settles the
existing desktop/mobile dialog for a backend-built settings proposal. It grants
only that connection/target/revision/content-bound proposal once, within its
absolute five-minute lifetime. No new settings IPC command is registered by this
step; the replacement settings transport is a separate cutover.

## Open desktop Markdown metadata

| Command | Arguments | Result | HTTP twin |
| --- | --- | --- | --- |
| `sync_open_markdown_tabs` | `tabs: {id,repoPath,filePath,fsRoot?}[]` | `void` | `PUT /config/open-markdown-tabs` (loopback only) |
| `list_open_markdown_tabs` | `cwd: string` | `{id,title,relative_path,absolute_path,fs_root}[]` | `GET /repo/open-markdown-tabs?cwd=...` |

The read command runs configuration I/O on the blocking pool. Publication stores
only metadata in memory and invalidates consumers with `open-markdown-tabs-changed`.
The list is scoped to registered repository/workspace ownership.

`design_mode_browser_command(repo_path, action, args)` executes an immediate
Design Mode native-port browser command. HTTP parity: `POST /design-mode/command`.
See the HTTP reference for action arguments and typed error prefixes.

### Tasks comment comparison

`compare_work_item_comments` / `POST /tasks/compare-comments` accepts `{detail, previous, enabled, followed}` and returns `{snapshot, notify}`. `detail` is the unchanged WorkItemDetail; `previous` is null or `{latest, ids, count}` (latest is a UTC timestamp in milliseconds). The pure Rust comparison has no provider IO or persistence. First observations, disabled/unfollowed inputs, older comments and edits are silent. GitHub full comment counts cover normal comments outside the first page. Desktop and HTTP use the same implementation.


`resume_session(tuic_session)` returns `{ok:true,session_id}` through the shared Rust wake path. HTTP parity: `POST /sessions/wake` with `{tuicSession}`. Live targets are idempotent; failed resume retains the suspended record.

### Guarded automated agent input

`send_agent_input_if_empty { sessionId, text } -> boolean` submits only to a confirmed-idle agent with no awaiting question, tracked draft, or queued command. Checks, split writes, and input bookkeeping share the per-session input transaction with raw UI and HTTP input. HTTP: `POST /sessions/{id}/input-if-empty { text }`.

## Shared settings action (#1787-4778)

`settings_action(request: JSON) -> JSON` uses the same Rust-generated envelope as POST `/settings/action` and MCP settings. Native caller authority is host-issued, never deserialized from the request. Typed owner workflows use the same envelope and explicit machine target. Owner transactions remain inventory-gated.

Gated HTTP and IPC edits return `confirmation_required` with `proposalId` and do not write.
Replay the exact original edit with `approval: "<proposalId>"` and `confirm: true`.
The same request opens the existing all-client human confirmation dialog, waits for the
answer, and, only after approval, commits the exact edit under its original caller.
A decline, expiry, changed edit, caller mismatch or reused approval cannot commit.
HTTP proposal identity uses the source IP, not the ephemeral TCP port; every request
still requires local or authenticated access. MCP callers can continue to use
`ui action=confirm proposalId=...` followed by an exact replay with `approval`.
Tunnel forward lists, including empty replacements, require confirmation like tunnel ports.

### Audio choice availability (#1908-c327, #1909-0d24)

Existing input/output device enumeration returns the same device objects on success, including a successful empty array. Backend enumeration failure now returns an error rather than an empty array. Internal settings choice refresh reuses those owners; cache-only describe and typed voice asset actions are not new public routes in this change. The settings transport cutover remains separate.

`force_retry_provider_recovery_command(sessionId)` submits `/retry` once through the
shared ego recovery transaction. It refuses a non-empty tracked composer and returns
the error to the caller. HTTP equivalent: `POST /sessions/{id}/force-retry`.

## Settings startup ownership

`settings_action` shares its envelope and recovery behavior with HTTP and MCP; native caller trust does not bypass owner CAS or human confirmation. [Settings startup workflows](../backend/settings-startup.md) describes GitHub `list_accounts` and configured Ego `begin_login`/`login_status`, including owner revision checks, a human-operated terminal and credential readback distinct from process completion.

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

### Managed launch transaction

`managed_agent_launch(request: object) -> object` creates or reads a durable managed-child launch. It shares validation, journal and execution with `POST /agents/launch` and MCP `agent action=launch`. See [HTTP API](http-api.md#durable-managed-agent-launch) for the request, receipt and retry contract. The explicit registered parent, stable name and keep-open policy are set before the prompt can run.

MCP passthrough calls use the same native inbox/output byte-budget validator and
snapshot pager as `POST /mcp` and HTTP MCP-compatible output; no separate IPC
paging implementation is added. See [the paging contract](../backend/mcp-http.md#compact-inventories-and-byte-bounded-response-snapshots).

Output snapshot references issued through IPC MCP passthrough and HTTP
MCP-compatible output intentionally share local `ipc` scope for the same
target. They are not authorization tokens; endpoint admission is unchanged.

### Frame workload evidence

`save_frame_workload(recording: String) -> Result<String, String>` writes bounded schema-v1 live grid workload JSON through the same serialized blocking-pool writer as `POST /debug/frame-workload`. Returns the instance-local capture path. It does not enable capture.

Dictation corrections are also available through `settings_action` with machine
target and store `corrections`: discover `/replacements` records keyed by `from`,
create/remove records and edit replacement `to` values with an owner revision.
`set_correction_map(map)` retains its public arguments and shared TextCorrector
persistence; its host-injected state publishes a value-free change after saving.

The upstream credential save/delete commands retain their public arguments;
host-injected state routes both through the shared value-free Settings publisher
only after owner persistence. Their HTTP equivalents use the same adapters.

Progress reporting rejects text longer than 500 raw Unicode scalar values before trimming, consistently with HTTP and the native MCP schema (#2007-59f6).

`github_diagnostics` includes `repo_access_errors` grouped by account, host and
owner, with per-repository reason (`not_found`, `forbidden` or `unknown`) and
exact message, plus an OAuth app authorization URL for every github.com OAuth group. IPC and HTTP share this DTO.

`github_diagnostics` accepts optional `refresh: boolean` (default false). True
clears all accounts' NOT_FOUND cooldowns before returning diagnostics; HTTP uses
`POST /github/diagnostics` for this explicit mutation and GET for ordinary reads.

### Detached checkout inventory

`get_repo_structure` and `get_repo_summary` include `detached_worktrees: Array<{path, head, short_head, directory_name, state: "detached"}>`, matching HTTP. `get_worktree_paths` retains its branch-workspace map response. Detached records have no workspace ID and cannot be used in branch actions.

### Session usage

| Command | Arguments | Result |
|---------|-----------|--------|
| `get_session_usage` | `sessionId: string` | Session-owned usage snapshot; same shape as `GET /sessions/{id}/usage` |

This async command runs outside the macOS main thread. Rust selects the owner,
profile and exact ego ledger row. Missing usage is an explicit unavailable
snapshot, not a zero quota or cost. No messages-generation fallback is used.

`get_repo_structure` and `get_repo_summary` also return `worktree_diagnostics: Array<{path, branch: string | null, state: "missing" | "duplicate_branch", guidance}>`, identical to HTTP. Diagnostics are read-only and have no workspace ID. Discovery does not prune unavailable registrations.

Unavailable detached registrations are excluded from automatic orphan cleanup in both Ask and On modes, including the Ask countdown. They remain repair diagnostics until restored or explicitly removed. Safe-only removal rechecks availability, so a directory moved after assessment cannot be pruned automatically. Explicit user-confirmed removal remains available.


### Append-only story notes

`add_note` and `list_notes` use the same shared story service on all transports. Notes retain backend actor/time and the subject revision without advancing it. Optional `{runId, commandId}` receipt references are validated against the same project and plan. See [Native stories in the HTTP API](http-api.md#append-only-story-notes) for fields, replies, bounds, and append-only semantics.
