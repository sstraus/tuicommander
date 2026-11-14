# Automations — Scheduled Agent Runs

**Status:** `moved` → [implementation plan](../plans/automations-scheduler.md)
**Priority:** TBD
**Category:** Orchestration / Agents
**Date:** 2026-10-08

## Request

Map the features and UX of a scheduler for agent runs, find what TUIC already
has, and plan how TUIC can give the same features. No external code is
incorporated.

## Reference feature set

An automation is a stored prompt. A scheduler turns it into a coding-agent
terminal session on a cron schedule. The core without multi-host machinery is
about 3-5k LOC.

- **Definition:** name, prompt, agent, project, workspace mode (`existing`
  workspace or `new_per_run` worktree from a base branch, optional project
  setup), session reuse (only with `existing`), precheck command + timeout,
  schedule, enabled, missed-run grace (default 12 h), host.
- **Schedule:** 5-field cron (Vixie OR rule for day-of-month/day-of-week) and
  a restricted RRULE. UI presets: hourly, daily, weekdays, weekly, custom cron.
  A stored `timezone` that the occurrence math ignores would be a bug.
- **Tick:** 60 s poll in the main process, and the same service headless.
  Each tick takes only the latest due occurrence. Inside
  the grace window it runs once; older → `skipped_missed`. No replay of every
  missed occurrence.
- **Run:** claim the run durably (`pending` → `dispatching`, flush), re-check
  definition and destination, create or pick the workspace, precheck (exit 0 =
  go, anything else = `skipped_precheck`; manual "Run Now" skips the
  precheck), then spawn the agent in a background terminal, or submit the
  prompt into the last live `done` session when reuse is on.
- **Completion:** agent status `done` or PTY exit 0 → `completed`; non-zero →
  `dispatch_failed`; lost PTY stays `dispatched` and a watcher reconciles it.
  The reference has no run timeout and no overlap policy.
- **Statuses:** pending, dispatching, dispatched, completed, skipped_precheck,
  skipped_missed, skipped_unavailable, skipped_needs_interactive_auth,
  dispatch_failed. Identical consecutive refusals fold into one row.
- **Run record:** workspace, terminal pointers, output snapshot (256K chars),
  precheck result, token usage and estimated spend. Retention: 100 final runs
  per automation, because one state blob is rewritten on every save.
- **UI:** sidebar shortcut, full-page view. List (Name, Project, Host, Last
  run, Status; search, filters, context menu Run Now/Edit/Delete/Pause).
  Templates (weekday repo audit, release readiness, daily change review,
  hourly maintenance). Editor dialog with cadence picker and cron hints.
  Detail pane with run history (Run, Status, Workspace, Tokens, Spend) and
  "View run" → live pane, else resume workspace, else snapshot. Runs dashboard
  with 24 h / 7 d success and failure tiles. Worktree cards show "Created by
  automation"; sidebar filter "Hide automation-created".
- **CLI:** `automations list|show|create|edit|remove|run|runs`.

## What TUIC has

- **Removed:** `ai_agent/scheduler.rs` (`ai-cron.json`, `cron` crate, 30 s
  tick, skip if target busy, no catch-up, no history). Deleted with the
  embedded AI engine in `393ad4e05` (#784-0aec) because the engine was unused,
  not because the scheduler was wrong.
- **Spawn from the backend:** `launch_daemon_workflow_agent` and
  `create_daemon_workflow_worktree` (`mcp_http/mcp_transport.rs`) already
  create a worktree and spawn an agent through `handle_agent_with_parent_cwd`
  without an MCP client. A scheduler uses the same path.
- **Completion signals:** `tasks.rs` (in memory, 24 h TTL, completed/failed
  from PTY exit), `SessionStateChanged` / `PtyExit` on the event bus, the
  `progress` journal (`done`/`blocked`).
- **Durable patterns:** `workflows/run/` SQLite store with owner lock,
  restart reconciliation and reserve-then-spawn effects. `progress.sqlite3`.
  `ConfigFile<T>` with delta save for JSON config.
- **Boot:** `WorkflowRuntime::spawn` runs in both desktop and `tuic-remote`
  boot paths (`lib.rs`). A scheduler started next to it runs headless too.
- **Smart Prompts:** library in `prompt-library.json`; variable resolution is
  partly in the frontend, so a headless run cannot resolve all variables.
- **Precheck building block:** `smart_prompt.rs` shell execution with clean
  env and timeout.
- **Notifications:** native, web push, Telegram.
- **Plugins:** cannot own a scheduler (webview lifetime, no agent spawn, no
  worktree capability). The engine belongs in Rust.

## Ideas

1. **One run = one TUIC task.** Use the existing spawn path and task handle.
   A run is visible in the sidebar like any spawned agent; "View run" focuses
   the tab.
2. **Completion from TUIC signals, not from a timer:** `progress done` /
   `blocked`, agent idle with `suggest:`, PTY exit. A `blocked` run becomes
   "needs you" and notifies (Telegram/push).
3. **Precheck with output as context:** precheck stdout can go into the prompt
   (for example the list of failing PRs), not only exit code gating.
4. **Overlap policy and max duration** from the start:
   skip if the previous run of the same automation is still active; kill or
   mark `timed_out` after a limit.
5. **Run history in SQLite**, not in a JSON blob: no 100-run cap for
   performance reasons; retention by age.
6. **Worktree provenance:** a worktree created by a run carries the
   automation id, so cleanup ("delete merged automation worktrees") and the
   sidebar filter are possible.
7. **MCP `automation` tool** so an agent can create or trigger automations;
   CLI parity through the HTTP API.
8. **Templates** as built-in definitions.

## Critical points

- **Run only while TUIC runs.** Desktop app or `tuic-remote`. No new daemon.
  On a closed laptop, catch-up runs one occurrence inside the grace window.
- **Double firing.** Restart in the middle of a dispatch must not spawn twice:
  claim the run durably before spawn (reserve-then-spawn), and mark
  unfinished runs `interrupted` at boot without automatic retry.
- **Completion is inference.** Agent state detection is heuristic for some
  agents; a run can stay "running" forever. Max duration and a clear
  `unknown` final state are required.
- **Session reuse semantics.** Submit into a live PTY is fragile (agent
  busy, dialog open). Resume through the agent's own session id is a different
  feature. Recommend: no reuse in phase 1.
- **Unattended permissions.** An agent launched at 03:00 that asks for a
  permission blocks. The run config decides (for example
  `--dangerously-skip-permissions`); TUIC must show `awaiting` as "needs you".
- **Resources.** Several automations at the same time fight for CPU (cargo
  builds). A global concurrency cap is needed (`MAX_CONCURRENT_SESSIONS` is
  not enough).
- **Timezone and DST.** Use the `cron` crate with `chrono-tz`, or local time
  only, but say it. Never store a timezone that is ignored.
- **Worktree accumulation.** `new_per_run` creates one worktree per run;
  without cleanup they pile up.
- **IPC/HTTP parity** for every command, plus doc sync.

## Draft plan

Phase 1 — core (Rust):
1. `automations.json` through `ConfigFile<T>`: definition model.
2. Cron parsing and next-occurrence (`cron` crate or `croner`), validation,
   presets.
3. SQLite run store (`automation_runs.sqlite3`): runs, statuses, precheck
   result, task/session ids; boot reconciliation.
4. Scheduler loop started next to `WorkflowRuntime::spawn`: 30-60 s tick,
   grace window, overlap skip, global concurrency cap, sleep/wake via
   wall-clock gap.
5. Dispatcher: optional worktree, precheck, spawn through the existing
   backend spawn path, bind task id, completion from task/event bus, max
   duration.
6. HTTP + IPC commands (list, get, create, update, delete, run_now,
   list_runs), `COMMAND_TABLE`, parity tests.

Phase 2 — UI:
7. Automations dialog (action registry + command palette): list, editor with
   cadence picker, detail with run history, Run Now, pause.
8. Runs view with 24 h / 7 d tiles; "View run" focuses the tab or shows the
   output snapshot.
9. Notifications on failed / needs-you runs.

Phase 3 — extras (only on request):
10. MCP `automation` tool, templates, worktree provenance + cleanup, precheck
    output as prompt context, session reuse.

Out of scope: multi-host ownership, external cron managers, token
spend estimation (until a usage source exists per run).
