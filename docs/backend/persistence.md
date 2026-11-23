# Persistence

**Module:** `src-tauri/src/db/` (`mod.rs`, `tuic.rs`, `legacy.rs`, `retention.rs`, `retention/files.rs`, `retention/instances.rs`)

TUIC keeps its SQLite data in the app config directory (`config::config_dir()`,
the same directory as `config.json`), never in a repository. Stories 3104-2fd1,
3105-d266 and 3106-327c (2026-11-21) set the layout below.

## Rules

- Durable, low-write data lives in **one** file, `tuic.sqlite3`.
- Only a high-volume append log gets its own `<name>.sqlite3`.
- Every database file uses the `.sqlite3` extension.
- Every store opens its file through `db::open` (WAL, 5 s busy timeout,
  foreign keys, a held idle connection that keeps the WAL alive, and
  `auto_vacuum=INCREMENTAL` on a new file) or `db::open_with`, which also
  applies the file's versioned migration list once per process. A read-only
  probe uses `db::open_read_only`.
- A schema change appends a migration to the file's `Schema`; a shipped
  migration is never edited. A file whose `user_version` is newer than the
  build's list is refused, never written.
- Every store has a retention rule for closed and dead records (below), and a
  new store must add one.
- Nothing reads across files today. A future cross-file read uses `ATTACH`,
  never a second copy of the data.

## Files

| File | Owning modules | What it saves | Write rate | Size before 2026-11-21 |
|---|---|---|---|---|
| `tuic.sqlite3` | `stories/store*`, `workflows/store.rs`, `workflows/{activity_preset,reusable_operation,neutral_publication,trigger_bindings}.rs`, `workflows/neutral_api/drafts.rs`, `mcp_http/launch_transaction.rs`, `mcp_http/mcp_usage_rollup.rs`, `work_item_visits.rs` | Native plans and stories, workflow definitions, managed launch records, daily MCP usage | Per user action; MCP usage every 30 s | 2.8 MB across the 4 old files |
| `progress.sqlite3` | `progress/store.rs` | Progress journal entries, views, contextual replies | Every agent progress line | 61 MB |
| `automation_runs.sqlite3` | `automations/store*` | Automation run ledger, schedule cursors, notification deliveries | Every run transition | 24 MB |
| `workflow_runs.sqlite3` | `workflows/run/*` | Neutral workflow runs, events, effect resources, native batch and policy ledgers, trigger state | Every run event | 0.2 MB |
| `mail.sqlite3` | `mail_subscriptions.rs` | Durable peer-mail journal and subscriptions | Every peer message | 33 MB |
| `tunnel_audit.sqlite3` | `tunnels/audit.rs` | SSH tunnel lifecycle events | Every tunnel state change | 4 MB |
| `provider_cache.sqlite3` | `provider_gateway/l2_sqlite.rs` | zstd-compressed provider response bodies and validators (L2 of the gateway cache) | Write-behind batch every 2 s | new |

`tuic.sqlite3` holds one migration list, `db::TUIC_SCHEMA` in `db/tuic.rs`.
Each log file declares its own `Schema` beside its store.

`provider_cache.sqlite3` is not an append log: it is a disposable, high-volume
cache with constant churn and rows of up to 2 MiB, so it stays out of the
durable `tuic.sqlite3`. It is the one file whose `user_version` mismatch is
resolved by deleting the file (the content is refetchable) instead of refusing
it. It never stores `Authorization`, cookie or token headers, nor a URL that
carries a credential. Story 3131-92ad (2026-11-22).

### Retired files

No data was migrated (Boss, 2026-11-21). At startup `AppState::new` calls
`db::legacy::remove_retired_files`, which deletes these files with their
`-wal`, `-shm` and `-journal` companions, logs the list once, and creates
`tuic.sqlite3`:

- always: `stories.sqlite3`, `workflows.sqlite3`, `managed-launches.sqlite`,
  `mcp_usage.sqlite3`, `mail-subscriptions.db`, `tunnel_audit.db`, and every
  `*.legacy-workflows.bak` copy, and the unused `ai-sessions/` directory;
- only while `tuic.sqlite3` does not exist yet: `progress.sqlite3`,
  `automation_runs.sqlite3`, `workflow_runs.sqlite3` (same name, old layout).

### Files TUIC does not own

Excluded from these rules and never opened through `db::open`:

- agent session stores such as Claude's `sessions.db`, read by
  `agent_session.rs`;
- plugin data a plugin writes through `plugin_fs.rs` (any name it chooses);
- the mdkb index, owned by the mdkb daemon.

## Tables and retention

"Read after close" says whether anything reads a record once it is closed.
Every table below has at least one production reader (audited 2026-11-21 by
`FROM`/`JOIN` sites outside tests), so no table or column was dropped. Two
pieces of unread code were removed instead: the tunnel audit time-range query
and the tunnel retention-on-connect.

### `tuic.sqlite3`

| Table | Saves | Read after close | Rule |
|---|---|---|---|
| `plans`, `stories`, `story_notes` | Native plans, stories, notes | Yes: story UI and API | Keep (user data) |
| `story_transitions`, `archive_history` | Transition and archive provenance | Yes: `transition_history`, `archive_history` API | Keep |
| `story_ingest` | mdkb projection revisions and tombstones | Yes: ingest sweep | Keep (one row per story) |
| `workflow_story_proposals`, `workflow_plan_proposals`, `external_story_*` | Workflow-proposed stories, external adapter state | Yes: idempotency on retry | Keep |
| `activity_presets`, `reusable_operations`, `neutral_workflow_drafts`, `neutral_workflow_presentation`, `neutral_workflow_publications`, `neutral_trigger_bindings` | Workflow definitions | Yes | Keep (user data) |
| `launches` | Managed launch identity, digest and stage (`updated_ms`) | Yes: retry with the same `launch_id` | Delete `retention.launch_days` (30) after the last update once the stage is `created` or `blocked`; other stages are open |
| `mcp_usage_daily` | Per-day MCP call counts | Yes: Settings > MCP > Usage (7 days) | Delete days older than `retention.mcp_usage_days` (90) |
| `work_item_visits` | Last visit of each issue or PR panel: comment count and time (migration 2) | Yes: the next visit's "N new" mark | Delete visits older than `retention.work_item_visit_days` (90); the next visit then marks nothing new |

### Logs

| File / table | Saves | Read after close | Rule |
|---|---|---|---|
| `progress` `entries` | Progress lines | Yes: Progress panel paging | Delete after `retention.progress_days` (30), except a `blocked` entry that is still the newest of its terminal (an open question) |
| `progress` `reply_context`, `progress_replies` | Question context and reply receipts | Only with their entry | Deleted with their entry |
| `progress` `pty_views`, `project_views` | Last-viewed markers | Yes: unread divider | Delete when not viewed within the window (project rows only once the project has no entries) |
| `automation_runs` `automation_runs`, `automation_deliveries` | Run snapshots and notification receipts | Yes: history and summary | Delete runs finished more than `retention.automation_run_days` (90) ago; open statuses (`reserved`, `prechecking`, `running`, `needs_you`) are never deleted; deliveries cascade |
| `automation_runs` `automation_cursors` | Last consumed occurrence per automation | Yes: prevents replaying a consumed schedule | Keep (one row per automation) |
| `workflow_runs` (all tables) | Neutral run events and native ledgers | Yes: run replay derives status from events | Keep: a run's closed state is derived, not stored, and the file is small |
| `mail` `mail_events`, `mail_heads`, `mail_subscriptions` | Peer mail per recipient, newest 1000 | Yes: subscription inbox | A recipient is dead when it received nothing for `retention.mail_days` (14) and none of its subscriptions has unacknowledged mail; all its rows are deleted |
| `provider_cache` `l2_entries` | Provider response bodies, validators, TTL state | Yes: the L1-miss read path | Self-managed by `tuic_gateway::l2` (not the daily job): at start and every 5 min, delete rows unused for 7 days, then least recently used rows until the stored bytes fit 256 MiB; `incremental_vacuum` after; an account's rows are deleted on logout |
| `tunnel_audit` `tunnel_events` | Tunnel events | Yes: Tunnels panel (last 20) | Delete after `retention.tunnel_audit_days` (30) |

## Files in the data directory

Every file rule deletes through `db::retention::files::remove`, which refuses a
path that is not strictly inside the data directory (`config::config_dir()`),
also after resolving symlinks in its parents; a symlink is deleted, never its
target. Story 3106-327c (2026-11-21).

| Path | Saves | Rule |
|---|---|---|
| `instances/*` | Config directory of an isolated instance (`make agent-test`, `make test`, checks) | Delete when no process holds its `instance.lock` and its newest top-level change is older than `retention.instance_days` (3). The daily instance is the data directory itself, never a child of `instances/`. The instance's keychain entry is left alone |
| `agent-capabilities/<identity>.json` | Fixed capability set of a spawned agent | Delete when the identity is not live or suspended and the file is older than `retention.session_file_days` (30). Loading a record for a restored tab refreshes its age. A missing record grants no `operator`, so a deletion can only narrow access |
| `bg-wakes/<session>.json` | Background-runner wake status of a session | Same rule as `agent-capabilities/` |
| `captures/` | PTY `.tcap` captures and frame workload recordings | Delete after `retention.capture_days` (14) |
| `mcp-bridge/<sha256>/` | Installed bridge revisions agent configs point at | Keep the current revision (the copy of the bundled bridge, else the newest valid copy) and any revision touched within `retention.bridge_revision_days` (7); delete the rest. A config root TUIC does not discover (a private `CLAUDE_CONFIG_DIR`) can still name a deleted revision; 0 days keeps every revision |
| `ai-sessions/` | Nothing: its writer was removed in a961a079f5 and nothing reads it | Deleted at startup by `db::legacy` |
| `design-mode/<repo hash>/` | Design-mode browser profile, one per repository (logins, cookies) | Kept: deleting it signs the user out of the pages they design against |
| `note-images/<note>/` | Images pasted into notes | Kept: user data, deleted with its note |

"Live" means a PTY session, a bound `TUIC_SESSION` identity or a capability set
held in memory, plus every entry of `suspended-sessions.json`. The startup run
skips `agent-capabilities/` and `bg-wakes/`, because restored tabs have not
respawned yet; the daily runs apply them. When `suspended-sessions.json`
cannot be read, those two rules are skipped.

## Cleanup job

`db::retention::start` runs at startup (desktop and headless boot, once the
`AppState` exists) and every 24 hours on a `db-retention` thread. It reads
`retention` from `config.json` each time, applies every database rule in its
own transaction, runs `PRAGMA incremental_vacuum` and a truncating WAL
checkpoint on a file it shrank, skips files that do not exist, then applies the
file rules above. It logs one line per store: `Cleanup removed N from <store>`.
A rule set to 0 days is disabled. Rules: `db/retention.rs`,
`db/retention/files.rs`, `db/retention/instances.rs`; tests:
`db/retention/tests.rs`, `db/retention/files_tests.rs`.
