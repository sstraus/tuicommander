# Command threading

Where a `#[tauri::command]` runs, and how to decide for a new one.

## The rule

A command declared as a plain `fn` gets `ExecutionContext::Blocking` and runs
**inline in the IPC handler**. On macOS that is the main thread, so anything slow
in it freezes the WebView — not "makes the app feel slow", freezes it, cursor and
all, until the call returns.

A command declared `async fn` runs on the Tokio executor instead. That takes it
off the UI thread, but an `async fn` full of `std::fs` calls still parks a Tokio
worker for the whole operation. Blocking work needs `spawn_blocking` **as well**.

So there are three placements, and the choice is not a style preference:

| Shape | Runs on | Use for |
|---|---|---|
| `fn` | IPC thread (macOS main thread) | Work bounded by a few µs: a cached getter, an atomic load, a pure transform |
| `async fn` | Tokio executor | Work that awaits, or that is bounded and short |
| `async fn` + `spawn_blocking` | Blocking pool | Filesystem, subprocesses, locks that a busy thread may hold, anything unbounded |

`fs::spawn_blocking_fs` is the helper for the third row. It flattens the
`JoinError` into the closure's own `Result<T, String>`, so the caller sees the
error the operation actually produced rather than an opaque join failure.

## IPC and HTTP must agree

Every command has an HTTP twin. Both transports drive the *same* work, so both
must make the same threading decision — otherwise the same operation blocks a
Tokio worker over HTTP while running fine over IPC, or vice versa, and the
divergence is invisible until someone profiles the transport nobody tested.

The way to keep them in step is structural, not disciplinary: **the HTTP route
calls the command**, and the command owns the `spawn_blocking`. A route that
reaches past the command into a `*_impl` has opted out of the guarantee and needs
a comment saying why.

The terminal grid reads are the one place that cannot follow that shape. Their
commands are `#[cfg(feature = "desktop")]` and the routes also compile into the
headless `tuic-remote` binary, so the route physically cannot call the command.
There the shared unit moves one level down: both call `pty::vt_try_read`, which
owns the `spawn_blocking`. Same guarantee, one layer lower.

Before story 607-f483 the two had drifted in both directions: `search_files`
offloaded over HTTP but not over IPC, while `search_content_all` offloaded over
IPC but not over HTTP.

## Audit (2026-08-18)

### Remote update (2026-09-26)

`prepare_remote_update` and `update_and_restart_remote` are async commands.
Their HTTP routes call the same `remote_update` service. Binary reads and writes
use Tokio files and bounded streaming; network probes, asset retrieval, upload
and restart verification await without blocking the IPC main thread.

### Native stories (2026-09-25)

`story_action_command` is `async fn` and offloads SQLite and ownership resolution with `spawn_blocking`. The headless-compatible `/stories/action` route offloads the same shared Rust service independently; the MCP `story` tool runs it through the existing blocking handler. This keeps disk I/O off the macOS IPC thread and Tokio workers.

`workflow_definition_action` and its HTTP twin use `spawn_blocking` around graph validation and SQLite publication for the same reason.
`workflow_run_action` and its HTTP twin also use `spawn_blocking` for SQLite run transitions and event paging.

391 `#[tauri::command]` declarations, 222 of them syntactically sync. Most are
correctly sync — cached getters, atomic reads, in-memory state. What follows is
the part of the inventory that matters, so this class of bug does not regrow
unnoticed.

### Moved off the UI thread in this story

`fs.rs`: `write_file`, `write_file_if_unchanged`, `create_directory`, `delete_path`, `rename_path`,
`copy_path`, `copy_path_abs`, `move_path_abs`, `add_to_gitignore`,
`fs_read_file`, `list_directory`, `search_files`.
`lib.rs`: `read_file`, `read_editor_file`, `read_external_file`,
`read_editor_file_external`, `write_external_file`.
`ai_agent/tools.rs`: `read_file`, `write_file`, `edit_file`, `list_files`,
`search_files`, `search_code` — routed through `blocking_fs_tool()`, which is a
table rather than six match arms precisely so a test can assert the routing.

`list_directory` is the one that looks harmless and is not: it runs
`git status --porcelain` as a subprocess for the requested subdir.

`pty.rs`, the terminal grid reads: `terminal_styled_rows`,
`terminal_get_block_rows`, `terminal_scroll_info`, `terminal_search`,
`terminal_search_buffer`, `terminal_get_row_text`, `terminal_get_logical_line`,
`terminal_get_selection_text`, `terminal_get_lines`, `terminal_get_cursor_line`,
`terminal_hyperlink_at`, `terminal_hyperlink_span` and `read_vt_log` — all
through `pty::vt_try_read`, and their HTTP twins in `mcp_http/session.rs` with
them. `read_vt_log` is the one worth naming: it has no frontend caller yet, so
it survived the first pass of the audit purely by being unused, and being
registered in the handler is all it takes to be reachable.

Every one of them moved, including the single-row reads. The cost that matters
is not the work the closure does, it is the wait for the vt mutex, which the PTY
reader holds through a whole `serialize_dirty_rows`. A one-cell read waits
exactly as long as a whole-scrollback search, so a line drawn between "cheap"
and "expensive" reads would only rot.

Their return type changed from `T` to `Result<T, String>`, which is what an
`async fn` borrowing `State<'_, _>` requires. An `invoke` that used to always
resolve can now reject, on a failed blocking-pool task.

That is not a new failure mode — `transport.ts` throws on any non-2xx, so the
browser path has always had a rejection to handle — but three frontend callers
turned out never to have handled it, and desktop had been hiding that:

- `CanvasTerminal.tsx`, the debounced search refresh, fire-and-forget from a timer
- `CommandOverview.tsx`, `getCommandText(...).then(setCommandText)` with no catch
- `useTerminalContextMenus.ts`, "Copy Block Output" — it awaits, but `ContextMenu`
  invokes the async action and discards the promise

Each now catches and logs. `terminal_search_buffer` was already safe: its only
consumer aggregates with `Promise.allSettled`.

`plugin_pty.rs`: `plugin_read_session_output` went the same way. It was already
`async`, so it was never on the IPC thread — but its body took the same vt mutex
inline, which parks a Tokio worker instead. It is the clearest example of why
row two of the table above is not enough on its own. Its capability check stays
on the caller's thread deliberately: a plugin without `pty:read` should be
refused without occupying a pool slot.

The cold-start follow-up (`606-fe99`) moved the splash-gating config loaders to
async wrapper commands in `lib.rs`: repositories, UI preferences, notifications,
repo settings/defaults, prompt library, notes, activity, keybindings, agent
config, and provider registry. The wrappers use unique Rust names and preserve
the existing IPC names with `#[tauri::command(rename = "...")]` while running the
file-locked loaders on the blocking pool. `load_config` is also async but reads
the cached `AppState`, so it
does not need a blocking hop. This makes the frontend `Promise.allSettled`
hydration genuinely concurrent rather than a queue of synchronous IPC handlers.

The same follow-up made `detect_all_agent_binaries` async with one blocking-pool
task per non-empty binary, and moved `detect_orphan_worktrees` onto the blocking
pool. Its HTTP route now awaits the command directly, so both transports share
the placement. The startup CLI version probes and atomic replacement also run in
a detached blocking task instead of inside Tauri `setup`.

Agent binary detection and terminal launch-argument preparation also use async
commands that offload CLI lookup, version and bounded `--help` probes to the
blocking pool. Their HTTP counterparts await the same commands, leaving Tokio
workers free while a configured CLI takes time to answer.
Each `--help` probe owns a process group on Unix or a Job Object on Windows,
so timeout cleanup includes descendants of a launcher script. A timeout is
cached as unsupported for that binary's path, size and modification time;
quick inconclusive exits remain retryable after a short cooldown.

### Known gaps, with reasons

| Command | Why it is still where it is |
|---|---|
| `fs_transfer_remote_paths` (`remote_transfer.rs`) | Async IPC/HTTP coordinator. Source enumeration, tar production and receiver extraction run on the blocking pool; HTTP and archive spooling are streamed asynchronously. |
| `fs_transfer_paths` (`fs.rs`) | Still sync, so its recursive directory copy runs on the main thread. It is the backend of a drag-drop, and the D&D surface needs Boss's approval before it is touched. Conversion is mechanical when that comes — see the `DEFERRED` note at the site. |
| `resolve_terminal_path` (`fs.rs`) | A single `canonicalize` + `is_dir`. Microseconds on a local disk; a stale network mount could stall it, which is a real but unobserved risk. Its batched sibling `resolve_terminal_paths` — the one a terminal screen actually calls, with tens of candidates — **is** on the blocking pool, so the risk that scaled with candidate count is gone. |
| `resolve_markdown_link` (`fs.rs`) | `async fn` using `spawn_blocking_fs` for canonicalization of the source, target and root. UNC paths are rejected before any filesystem probe. |
| `warm_content_index` (`fs.rs`) | `warm_index` is an in-memory config read plus a map entry plus a spawn — the build itself already runs in the background. It fires on every repo switch, so it stays cheap by design: the `index_strategy` gate and `ensure_index`'s dedup both short-circuit before any work. |
| `set_ansi_colors` (`pty.rs`) | Locks *every* vt buffer in a loop on the IPC thread, so the stall grows with session count. Same reordering objection as the row below, and it fires once, when the user picks a theme. |
| Terminal grid *mutations* (`pty.rs`) | `terminal_scroll`, `terminal_scroll_to`, `terminal_request_frame`, `terminal_exit_alt_screen` still take the vt lock inline. Same stall as the reads, but not the same safety: two `spawn_blocking` hops for one session can run in either order, and `terminal_scroll_to(line)` is absolute, so reordering lands the viewport on the wrong line. They need the coalescing `terminal_scroll_to_offset` already has — which is also why they are the cold path, since the wheel and the scrollbar drag go through the offset command and never touch this lock. |
| Remaining commands in `worktree.rs`, `tuic_cli.rs`, `tunnels/`, `dictation/`, `plugins.rs`, `agent.rs` | Sync commands still run git subprocesses, keyring calls, hardware enumeration and recursive deletes. `detect_orphan_worktrees`, `detect_all_agent_binaries` and the four named in the 2026-09-12 sweep below are no longer in this set; the other commands remain a starting list for the next sweep. |

## Post-merge cleanup (2026-09-12)

The dialog froze the window for the whole run, and the reason was the drift this
document predicts: **every HTTP twin was already on the blocking pool while the
IPC command was a plain `fn`.** Four commands moved to `async fn` +
`spawn_blocking`, one per step of the dialog:

| Command | What it does inline |
|---|---|
| `switch_branch` (`worktree.rs`) | stash + checkout, and it can wait on the index lock |
| `delete_local_branch` (`worktree.rs`) | `git branch -d`, cascading into a worktree removal |
| `finalize_merged_worktree` (`worktree.rs`) | moves a directory (archive) or deletes one |
| `close_pty` (`pty/commands.rs`) | two 100 ms `sleep` deadlines waiting for the child, then an optional recursive delete |

`close_pty` is the one to keep in mind: the cost is per session and the cleanup
closes a workspace's terminals in a loop, so the freeze grew with the number of
tabs on the branch. `run_git_command` — pull, push, stash pop — was already
async and correct.

Two source-scan tests lock it in, in the shape `lib.rs` already uses for the
splash-gating loaders: `post_merge_cleanup_commands_never_run_on_the_ipc_thread`
(`worktree.rs`) and `close_pty_never_runs_on_the_ipc_thread` (`pty/tests.rs`).
Each asserts the `async fn` signature **and** a `spawn_blocking` in the body,
because row two of the table above is not enough on its own.

## Stale-temp repository repair (2026-09-13)

`list_stale_temp_repository_candidates` and `repair_stale_temp_repositories`
(`config.rs`, story 763-d219) are plain sync `fn`, matching `save_repositories`
exactly rather than the async splash-gating loaders above: both do the same
class of work `save_repositories` already does inline on the IPC thread — a
`ConfigFile` file-locked JSON read (and, for repair, a write) — and neither is
part of the parallel boot hydration `606-fe99` moved off it. They are also
low-frequency, user-explicit actions (a preview shown once, a repair confirmed
once), not something that fires per-keystroke or per-frame. Left inline as a
deliberate, documented choice rather than converted preemptively; revisit
together with `save_repositories` if repository writes are ever swept onto the
blocking pool.

## Managed workspace removal (2026-09-12)

MCP `repo worktree_remove` used to call the blocking removal core directly from
its async native-tool handler. Removing a linked worktree can recursively
delete large ignored build artifacts, so it may take tens of seconds. The
native MCP path now uses `spawn_blocking`, matching both the
Tauri command and HTTP route. The bridge gives create and remove the same 305 s
workspace-operation response deadline; its ordinary 10 s deadline cannot report
a removal failure while the backend is still deleting the workspace.

## Workspace lifecycle preflight (2026-09-13)

`get_workspace_lifecycle` is `async fn` and moves its Git status, ref mirror,
rev-list, and merge-base work to `spawn_blocking`. Its HTTP twin does the same.
Repository-wide lifecycle snapshots run inside the existing progressive Phase 2
blocking tasks and monitoring semaphore; no Git subprocess runs on the WebView
IPC thread.

### Comments that asserted a cost the code did not have

Three were found and corrected. They are recorded because a wrong comment is
worse than no comment — each one had already talked a reader out of checking:

- `mcp_http/fs_routes.rs` — "a single `read_dir` + sort, which completes in
  microseconds". It runs `git status` as a subprocess.
- `mcp_http/fs_routes.rs` — the BM25 path is an "in-memory query (fast, stays on
  the executor)". Its grep phase opens up to 50 files.
- `fs.rs` — "BM25 phase: get top-ranked files (~1ms)". Nothing bounds it; the
  cost is proportional to the index.

Private secret entry: `secret_form_bootstrap` is a short synchronous identity
check; `secret_form_submit` is async. MCP request/consent waits on a oneshot
without holding the blocking pool. Run uses Tokio child pipes; no PTY or shell.

`get_prompt_receipt` is async and uses `spawn_blocking` for the metadata read and MCP receipt adoption. It performs no file reads on the IPC path; explicit instruction file snapshots happen at launch.

### ego perimeter commands (2026-10-06)

`ego_perimeter`, `ego_set_perimeter_roots`, and `ego_set_perimeter_network`
are async IPC handlers. IPC and HTTP call the same `ego_cli::perimeter` core,
which awaits `tokio::process::Command` and reads ego's JSON. TUIC performs no
configuration-file rewrite or provider request on this surface.

### Native clipboard (2026-10-10)

`write_clipboard_text` and `read_clipboard_text` are desktop-only async commands.
They create and access arboard inside `spawn_blocking`, including the lock that
serializes clipboard operations. The managed clipboard stays alive between calls
so Linux retains ownership of copied text. Clipboard and worker failures return
`Err(String)`. Browser clients use `navigator.clipboard`; these commands have no
HTTP routes because remote clients must not access the host pasteboard.

Automation `automation_action` is async. Shared Rust parsing routes Run Now to
the owner-guarded runtime; definition, history, aggregate and preview work uses
`tokio::task::spawn_blocking`. HTTP calls the same boundary.
