# Automations

The backend Once runtime starts on desktop and headless hosts. It records
precheck decisions before launching the saved agent profile with the literal
prompt. Precheck output is saved in history. It is not added to the prompt unless
the definition opts in with `precheck.inject_output: true`.
Recurring cron definitions dispatch once per due occurrence and Run Now works for
them. Overlap `skip` still refuses an occurrence while the previous run is open.

Open the command palette and select **Automations**. This dialog manages runs
on this machine. It does not dispatch onto a different saved remote connection.

Select **New automation**, enter a name, repository path, select a saved run config
and enter a literal prompt. Choose an existing checkout or a new worktree
per run with a base branch. The agent uses its run config's permissions.

Saved run configurations are validated against the same named-profile lookup used
by the runner. Names are case-insensitive; a bare agent type such as `claude`
is not a saved profile unless a profile has that name. Create, definition
updates and re-enabling reject an unresolved name with a repair hint listing
valid saved profiles. Existing list rows retain invalid definitions, but expose
`run_config_error` and return `next_run_ms: null` until repaired. A profile can
be removed after saving; launch checks it again and records a failed run.

On desktop startup, backend validation reports unresolved definitions in the
Messages bell without opening Runs. Renaming or deleting a profile triggers the
same check as soon as its settings save completes. **Fix** opens the current
definition and focuses the profile picker. Select a saved profile and **Save**;
TUIC never substitutes an agent default or starts a replacement run automatically.

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

An agent-created run that ends `unknown` (for example the agent went idle after a provider error) sends the creator one `automation_run_unknown` message with `run_id`, `session_id` (the session to resume) and `last_error` (the last output line mentioning an error).

A failed agent-created run also sends one `automation_run_failed` inbox message
to its recorded creator, including the run id, definition id and failure reason.
This includes failures before a session or task exists and does not depend on
sound preferences. Mail uses the normal payload-free peer wake route. The
recipient must still be registered; acceptance means queued, not read. The
durable owner-channel claim prevents duplicate or restart replay. Definitions
created by HTTP/IPC without a creator still use the bell and configured alerts.

A live transition to `needs_you` or `failed` adds a Messages bell item, including
the failure reason even when the creator session is closed. Select
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

The dialog addresses the daemon that owns the active repository when you open it (or this machine for a local repository). Switch repository and reopen to view another owner. Select **Runs** to view all persisted runs, including runs of deleted automations. The 24h and 7d counts come from the backend and use elapsed UTC windows, not local calendar days. Identical failures (same definition, repository, profile and reason) share one row
with an occurrence count and the newest run as representative. **Fix** opens the
saved definition's profile picker, not an editable copy of historical settings.
Deleted definitions retain read-only history. **Previous runs** and **Next runs**
page through grouped rows; summaries still count individual runs. Each row retains its original automation name, repository and status. `needs_you`, `unknown` and `interrupted` are distinct; none implies success.

**View run** focuses the owning live terminal when it is present in this client. If the terminal is absent, suspended, exited or belongs to another daemon, the dialog shows saved stdout/stderr, truncation warnings, the recorded workspace and run configuration. A missing or removed workspace is not recreated. Viewing a run never starts another run. History and output retention remain backend-controlled. New PTY run snapshots save plain text from the session’s existing rendered terminal grid, including retained scrollback and the current screen. Cursor repaints replace earlier spinner frames instead of saving them as output. The newest 256 KiB of UTF-8 text is kept, so a long log does not discard the final report. Plain-command stdout capture is unchanged. Older saved output is not rewritten.

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

Use Left/Right or Home/End on the Definitions and Runs tabs. New automation, Refresh and search share a toolbar. The dialog opens the first automation unless a specific definition, run or new draft is already requested. Refresh does not replace unsaved edits.

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

### Manual configuration edits

You can edit the selected instance's `automations.json` with an external editor.
Reads and edits reject incomplete or invalid documents without renaming or
replacing them. If a read overlaps an in-place save, it can report an error;
after the save finishes, Refresh or the next runtime tick reads the completed
file. A genuinely invalid document continues to report an error until you repair
it. TUIC does not replace it with an empty configuration.

#### Durable event outcomes

The `run` and `list_runs` actions return `outcome` independently of execution
status: `decision`, `pr_url`, `comment_url`, `failure_reason` and
`publication_intent` (repository, head, base and issue number). Existing records
without an outcome remain readable. A completed agent is not proof of a
published PR. No comment publisher is added; `comment_url` remains null.

Before publication, TUIC saves the exact intent and claims it once. It looks up
all PR states for the same repository/head/base before creation and after a lost
response. On restart it reconciles saved pending/unknown intents by lookup only,
even if the worktree was removed. An unavailable, incomplete or empty lookup
leaves `publication_unknown` with a reason; TUIC never automatically repeats an
uncertain POST. Inspect the repository and saved intent before operator action.
Historical runs from older builds are not automatically republished.

Filtered issues with a valid GitHub issue identity, duplicate admissions,
disabled definitions and capacity refusals retain skipped history rows and
reasons. Repeated observations of the same event/reason update one row's
`updated_ms`, not one row per poll. These rows do not consume the admission key;
a later eligible issue or free slot can still be admitted. Invalid issue URLs
have no trusted event identity and are ignored. An empty intake allowlist stays
disabled and performs no evaluations. History retention also applies to these
rows; summaries count rows, not poll observations. The saved run API exposes
outcomes; this change does not add outcome controls to the Automations dialog.

#### Durable event outcomes

The `run` and `list_runs` actions return `outcome` independently of execution
status: `decision`, `pr_url`, `comment_url`, `failure_reason` and
`publication_intent` (repository, head, base and issue number). Existing records
without an outcome remain readable. A completed agent is not proof of a
published PR. No comment publisher is added; `comment_url` remains null.

Before publication, TUIC saves the exact intent and claims it once. It looks up
all PR states for the same repository/head/base before creation and after a lost
response. On restart it reconciles saved pending/unknown intents by lookup only,
even if the worktree was removed. An unavailable, incomplete or empty lookup
leaves `publication_unknown` with a reason; TUIC never automatically repeats an
uncertain POST. Inspect the repository and saved intent before operator action.
Historical runs from older builds are not automatically republished.

Filtered issues with a valid GitHub issue identity, duplicate admissions,
disabled definitions and capacity refusals retain skipped history rows and
reasons. Repeated observations of the same event/reason update one row's
`updated_ms`, not one row per poll. These rows do not consume the admission key;
a later eligible issue or free slot can still be admitted. Invalid issue URLs
have no trusted event identity and are ignored. An empty intake allowlist stays
disabled and performs no evaluations. History retention also applies to these
rows; summaries count rows, not poll observations. The saved run API exposes
outcomes; this change does not add outcome controls to the Automations dialog.

## Returned issues and requirements changes

Intake polls fetch each issue's body and keep only a digest of its title and body.
A triage verdict is published only if the issue still has that digest; an edit after
admission retires the verdict without writing anything. An issue returned by triage
(rejected or needs input) is admitted once more, as the next generation, when it
still carries the intake label and its digest differs from the returned one.
Removing and re-adding the label alone changes nothing. A triage policy with
`needs_input_comment: true` posts one question comment for a needs-input verdict;
without it, needs-input still writes nothing.

### Close completed run tabs

Set `close_on_finish: true` on an automation definition, or select **Close the run
tab after confirmed completion** in the Automations editor. The optional boolean
defaults to `false`. Create and update accept the same field over MCP, HTTP and
desktop IPC, and store it in `automations.json` and the immutable run snapshot.

Only a positively confirmed `completed` run closes its recorded session through
the existing session-close path and tab countdown. Failed, unknown, timed-out,
interrupted and `needs_you` runs do not trigger this option. Idle is not completion.
The option never removes a worktree or branch and never closes sibling sessions
in an existing checkout. Worktree removal remains a separate explicit
`cleanup_worktree` action with its existing safety checks.

## Timezone selection

The Timezone dropdown filters zone names while open. It includes the names accepted
by the backend, including aliases and fixed-offset zones. New automations select
the host-local zone returned by schedule preview, not the browser zone. A saved
zone outside the list remains selected until you choose another zone.
