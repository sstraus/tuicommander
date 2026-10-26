# Automations

The backend Once runtime starts on desktop and headless hosts. It records
precheck decisions before launching the saved agent profile with the literal
prompt. Precheck output is saved in history. It is not added to the prompt unless
the definition opts in with `precheck.inject_output: true`.
Recurring cron definitions dispatch once per due occurrence and Run Now works for
them. Overlap `skip` still refuses an occurrence while the previous run is open.

Open the command palette and select **Automations**. This dialog manages runs
on this machine. It does not dispatch onto a different saved remote connection.

Select **New automation**, enter a name, repository path, configured run config
identifier and literal prompt. Choose an existing checkout or a new worktree
per run with a base branch. The agent uses its run config's permissions.

Saved run configurations are validated against the same named-profile lookup used
by the runner. Names are case-insensitive; a bare agent type such as `claude`
is not a saved profile unless a profile has that name. Create, definition
updates and re-enabling reject an unresolved name with a repair hint listing
valid saved profiles. Existing list rows retain invalid definitions, but expose
`run_config_error` and return `next_run_ms: null` until repaired. A profile can
be removed after saving; launch checks it again and records a failed run.

Choose Hourly, Daily, Weekdays or Weekly, set the time controls and select
**Apply cadence**. The backend converts the cadence to cron. You can also edit a
five-field cron expression directly. For a one-time run, choose **Once** and
enter **Once local time** in the named timezone. The backend resolves the
instant; the browser does not convert it through your browser timezone.
A completed one-time occurrence stays visible without another next run. Enter an IANA timezone such as
`Europe/Madrid` and select **Preview schedule**. An empty creation zone resolves
to the backend's local zone. Preview shows backend occurrences in that named
zone. The backend validates schedules and handles daylight-saving transitions.

Set a maximum duration and catch-up grace in seconds. Overlap always skips an
occurrence while a previous run remains open. An optional precheck runs before
scheduled launches: exit 0 allows the agent to run; other exits skip quietly.
Save the definition before running it.

To pass what the precheck found to the agent, enable **Include precheck stdout in
the agent prompt** in the Precheck section and save. The checkbox starts off and
can be changed for each definition. Turning the precheck off and on resets consent.
The same setting is `inject_output: true` in the precheck object through the
definition API or MCP, for example:

```json
{"command":"git status --short","timeout_secs":30,"inject_output":true}
```

The flag defaults to false. A successful scheduled precheck adds stdout before
the unchanged prompt in a `<precheck-output untrusted="true">` block, labeled as
data rather than instructions. XML delimiters are escaped. The escaped stdout
payload is capped at 16 KiB on character boundaries; `[output truncated]` marks
omitted data. Framing and this marker are outside the payload budget. stderr is
never included. Empty stdout, refused checks, absent checks and Run Now bypasses
do not add context. Run history includes `precheck_context.injected` and `bytes`
for the prepared launch prompt; bytes counts escaped stdout only, not framing
or the marker. A failed launch can retain this preparation receipt.

**Pause** stops scheduled launches; **Resume** enables them. **Run now** works
while paused, bypasses precheck, obeys capacity and overlap, and leaves the
scheduled cursor unchanged. A skip receipt is shown as a skip, not as a launch.
The list shows next-run time and last status; recent history shows saved run
statuses and reasons. **Delete** asks for confirmation and keeps saved history.

Errors stay visible and failed saves retain the draft. **Refresh** retries a
failed list load. Escape closes the dialog and Tab stays inside it.

## Needs-you and failure notices

A failed agent-created run also sends one `automation_run_failed` inbox message
to its recorded creator, including the run id, definition id and failure reason.
This includes failures before a session or task exists and does not depend on
sound preferences. Mail uses the normal payload-free peer wake route. The
recipient must still be registered; acceptance means queued, not read. The
durable owner-channel claim prevents duplicate or restart replay. Definitions
created by HTTP/IPC without a creator still use the bell and configured alerts.


A live transition to `needs_you` or `failed` adds a Messages bell item. Select
it to open the canonical saved run, including its status, output and owning
session. Editing uses the current saved definition, not the historical run snapshot.
The run stays available after its definition is deleted, with no definition editor. **Open owning
session** is disabled when the session no longer has a terminal tab.

Native alerts use **Notifications** → **Enabled** and the existing **Question**
or **Error** preference. Web Push also requires its existing service opt-in and
a subscribed device. Push opens the owning mobile session when one exists;
the message names the saved run. Telegram uses only the owning agent's existing
registration and allowlisted destination. Automations never register an agent
or select another conversation on its behalf. Without a matching registration,
Telegram remains quiet.

Each run/status/channel is claimed in SQLite before delivery. Repeated awaiting
signals, reconnects and restarts do not replay notices. `needs_you` is announced
at most once per run per channel, even if that run resumes and blocks again.
Precheck skips, success, unknown, interrupted and timeout outcomes stay in history
without these alerts. A failed or ambiguous send never changes execution status
or saved output. Delivery acceptance does not prove a notification appeared on
a device. The bell is live-only: disconnected clients can miss the event and
must inspect run history; boot never replays historical notices. On non-macOS
desktop platforms native alerts do not support click navigation; use the bell.
The dialog's backend integration is pending Step 8 (story 1617). Until that API
is installed, requests show a backend error. The injectable adapter is verified
independently; this guide does not claim an installed scheduler is available.

## Run history and saved output

The dialog addresses the daemon that owns the active repository when you open it (or this machine for a local repository). Switch repository and reopen to view another owner. Select **Runs** to view all persisted runs, including runs of deleted automations. The 24h and 7d counts come from the backend and use elapsed UTC windows, not local calendar days. **Previous runs** and **Next runs** page through history. Each row retains its original automation name, repository and status. `needs_you`, `unknown` and `interrupted` are distinct; none implies success.

**View run** focuses the owning live terminal when it is present in this client. If the terminal is absent, suspended, exited or belongs to another daemon, the dialog shows saved stdout/stderr, truncation warnings, the recorded workspace and run configuration. A missing or removed workspace is not recreated. Viewing a run never starts another run. History and output retention remain backend-controlled.

Agents and scripts can use MCP `automations` or `tuic automations '<action JSON>'`
for the same operations. List returns definition rows with next-run/last-status
metadata; create/update return the saved definition. Agent-created definitions
record the bound session. See the CLI guide for examples and the HTTP API
Automations table for fields.


In **Runs**, select **Inspect worktree cleanup** for a run that created a worktree. Review its recorded path and safety result, then select **Remove safe worktree and branch**. Cleanup retains the saved report and run provenance. Open runs, existing repositories, dirty or unmerged work, missing required archives, locked worktrees and live sessions are preserved. A stale preview cannot authorize unsafe removal.
### Built-in automation templates

New automation drafts offer weekday repository audit, release readiness, daily
change review and hourly maintenance review templates. Enter the repository and
run config, then select **Apply template**. Rust supplies the literal prompt,
cadence, limits and zone (the backend host's local IANA zone unless selected).
Review and edit the draft, then select **Save**. Templates start paused; applying
one never persists a definition or starts a run. **Resume** is a separate action.
Templates use the existing checkout with no precheck and do not change agent
permissions. Recurring cadence previews retain the existing runtime limitations.

### Reading the Automations dialog

Search for **Automations**, **scheduler** or **schedule** in the command palette to open the dialog.

Definition cards show a short repository name; hover it for the full path. Daily, weekly, hourly and every-N-minute schedules have plain-language labels. Other expressions retain their cron text. Next-run times come from the backend and use the definition's timezone.

Use Left/Right or Home/End on the Definitions and Runs tabs. New automation, Refresh and search share a toolbar. **Select first automation on open** is enabled by default; clear it to select definitions manually. This preference is saved for the current browser/WebView. Refresh does not replace unsaved edits.

### Ask an agent to schedule a one-time check

A bound agent can call `automations` once with
`{"action":"create","definition":{"name":"Recheck","prompt":"Check again","in_secs":3600,"run_config":"Your saved profile"}}`.
The host uses its own clock and this caller's repository. Specify the actual
saved agent profile in `run_config`; the host does not infer an account/model
profile from agent type. It returns the saved schedule and next-run timestamp;
no clock or list lookup is needed. Retrying the identical request from the same
caller returns the original reminder without resetting its delay. Supply a new
explicit id to schedule a separate identical reminder. Elapsed delays remain
exact across daylight-saving changes.

### Guided GitHub event configuration

For step-by-step policy setup, use the three [event configuration Smart Prompts](event-driven-templates.md), not the scheduled draft templates above. The guides verify the owning daemon, agent/reviewer profiles, GitHub auth, skills and supported intake/publication APIs. Missing capabilities stop the interview; no event subscription is created by applying a scheduled template.

### GitHub issue intake (allowlisted, off by default)

`automations.json` accepts an `event_intake` block. It stays off until `repositories` lists at least one `owner/name`:

```json
"event_intake": { "repositories": ["owner/name"], "label": "tuic-run", "definition_id": "<definition id>" }
```

The existing GitHub poller supplies the issues, so its issue filter must be `all` for the repository. An open issue that carries the label starts the named, enabled definition once per issue and label; the key is stored in the run ledger and survives restarts. If an open run or the concurrency limit refuses it, a later 30-second tick admits it. The agent receives only the issue URL, never its title or body.

When such a run completes in an isolated workspace (`new_per_run`), TUIC creates a draft pull request for the workspace branch through its GitHub API layer, using the TUIC GitHub login. The agent must push that branch itself. Publication re-checks the allowlist and that the workspace remote is the admitted repository; no other repository is ever written.

Downgrading TUICommander after an event run exists is not supported: an older build cannot read `Event` runs or the `event_intake` block.
