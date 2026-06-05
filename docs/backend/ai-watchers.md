# AI Watchers

Event-driven autonomous actions on terminal sessions. Watchers observe terminal state transitions and fire AI agent conversations when conditions are met.

## Architecture

```
event_bus (AppEvent::PtyParsed)
    │
    ▼
WatcherEngine::run()          ← subscribes to broadcast channel
    │
    ├─ shell-state: idle  ──► on_idle()   ──► evaluate Idle / CommandDone / Pattern / Unseen triggers
    ├─ shell-state: busy  ──► on_event()  ──► evaluate Busy triggers
    ├─ question           ──► on_event()  ──► evaluate Question triggers
    ├─ api-error/rate-limit ► on_event()  ──► evaluate Error triggers
    ├─ user-input         ──► on_user_input() ──► auto-pause all active watchers for session
    └─ SessionClosed      ──► detach + pause all watchers for session

event_bus (AppEvent::GitHubTransition)   ← emitted by github_poller
    │
    ├─ PrTransition::Pushed ─► on_pr_pushed() ─► evaluate PrPushed triggers (dedup by head_ref_oid)
    └─ PrTransition::Opened ─► on_pr_opened() ─► evaluate PrOpened triggers (once per PR appearance)
```

## Data Model

### WatcherRule

Persisted in `ai-watchers.json` (app config dir).

| Field | Type | Description |
|-------|------|-------------|
| `id` | `String` | UUID, auto-generated on create |
| `name` | `String` | Human-readable label |
| `session_id` | `Option<String>` | `None` = template (unattached), `Some` = active instance |
| `template_id` | `Option<String>` | Links instance back to its template |
| `trigger` | `WatcherTrigger` | When to fire (see below) |
| `instructions` | `String` | Prompt sent to the AI agent |
| `max_fires` | `u32` | Limit before auto-exhaustion (default: 50) |
| `fire_count` | `u32` | How many times this rule has fired |
| `cooldown_secs` | `u32` | Minimum seconds between fires (default: 10, min: 5) |
| `burst_threshold` | `u32` | Max fires within burst window before auto-pause (default: 5) |
| `burst_window_secs` | `u32` | Burst detection window (default: 60) |
| `status` | `WatcherStatus` | `active`, `paused`, `stopped`, `exhausted` |

### WatcherTrigger

| Variant | Evaluated in | Description |
|---------|-------------|-------------|
| `Idle` | `on_idle` | Terminal returns to idle (shell prompt) |
| `Busy` | `on_event` | Terminal enters busy state (command running) |
| `CommandDone { on_failure_only }` | `on_idle` | Command completed; optionally only on non-zero exit |
| `Question { confident_only }` | `on_event` | Question detected in terminal output |
| `Error` | `on_event` | API error or rate limit detected |
| `Unseen` | `on_idle` | Terminal is idle AND its tab is not visible |
| `Pattern { regex }` | `on_idle` | Regex matches against last 50 screen lines |
| `PrPushed { authored_by_others }` | `on_pr_pushed` | New commit pushed to an open PR (git-scoped via `repo_path`) |
| `PrOpened { authored_by_others }` | `on_pr_opened` | A brand-new PR was opened (git-scoped via `repo_path`) |

### Trigger Evaluation Paths

Triggers are evaluated in three distinct paths:

- **Idle path** (`on_idle`): Idle, CommandDone, Pattern, and Unseen are evaluated when the terminal transitions to idle. Unseen additionally checks `session_visibility` (tab visible flag from the frontend).
- **Event path** (`on_event`): Busy, Question, and Error fire immediately when their corresponding event arrives — they don't wait for idle.
- **GitHub path** (`on_pr_pushed` / `on_pr_opened`): PrPushed and PrOpened fire from `AppEvent::GitHubTransition` (emitted by `github_poller`), not the terminal paths. They are git-scoped to `repo_path`, apply the `authored_by_others` filter (skips PRs you authored, and skips when the GitHub viewer can't be resolved), and provision/reuse a worktree session to review the PR. `PrOpened` fires at most once per PR appearance (the poller suppresses the first-poll seed so pre-existing PRs don't fire); `PrPushed` dedups by `head_ref_oid` so it fires once per commit.

## Template / Instance Model

Watchers use a **template → instance** pattern:

1. **Template**: A rule with `session_id = None`. Created via the UI. Not active — serves as a blueprint.
2. **Instance**: Created by "attaching" a template to a terminal session. Clones the template with a new UUID, sets `session_id`, and starts in `active` status.

Detaching an instance clears `session_id`, resets `fire_count`, and pauses it. On session close, all instances for that session are automatically detached.

On app restart, all rules are detached and paused (session IDs don't survive restart).

## What the Agent Receives

When a watcher fires, it calls `start_conversation()` with this message:

```
## Watcher instructions
<the rule's instructions field>

## Terminal context
Last command: `<cmd>` (exit <code>), cwd: <path>
Output:
<sanitized output snippet from session_knowledge>

Screen (last N lines):
<last 50 lines from VtLogBuffer>

## Watcher fire #<count>/<max>
```

The conversation runs with `Autonomy::Autonomous` and a 10-step limit.

## Safety Guards

| Guard | Behavior |
|-------|----------|
| **Active conversation** | Skips if a conversation is already running on the session |
| **Cooldown** | Per-rule minimum interval between fires (default 10s) |
| **Burst detection** | Auto-pauses if fires exceed `burst_threshold` within `burst_window_secs` |
| **Max fires** | Transitions to `exhausted` status when `fire_count >= max_fires` |
| **User input** | Any user keystroke auto-pauses all active watchers for that session |

## Tauri Commands

| Command | Parameters | Description |
|---------|-----------|-------------|
| `watcher_create` | `name`, `session_id`, `trigger`, `instructions`, `max_fires` | Create template or instance |
| `watcher_list` | — | List all rules (templates + instances) |
| `watcher_delete` | `id` | Delete a rule |
| `watcher_toggle` | `id`, `enabled` | Pause/resume a rule |
| `watcher_attach` | `template_id`, `session_id` | Clone template as active instance |
| `watcher_detach` | `id` | Detach instance, reset fire count |
| `watcher_update` | `id`, `name?`, `trigger?`, `instructions?`, `max_fires?` | Edit a rule's fields |

## Frontend Events

| Event | Payload | When |
|-------|---------|------|
| `watcher-status` | `{ id, status, fire_count, session_id }` | On any status transition (fire, pause, exhaust, burst) |

## Key Files

| File | Role |
|------|------|
| `src-tauri/src/ai_agent/watcher.rs` | WatcherRule model, WatcherEngine event loop, trigger evaluation, CRUD, persistence |
| `src-tauri/src/ai_agent/commands.rs` | Tauri command handlers for watcher_* |
| `src-tauri/src/state.rs` | `watcher_engine` OnceLock in AppState, `session_visibility` DashMap |
| `src/components/WatcherManager/WatcherManager.tsx` | Template CRUD UI, attach/detach, edit form |
| `src/components/WatcherManager/WatcherManager.module.css` | Popover styles |
| Config: `ai-watchers.json` | Persisted rules (app config dir) |
