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

**View run** focuses the owning live terminal when it is present in this client. If the terminal is absent, suspended, exited or belongs to another daemon, the dialog shows saved stdout/stderr, truncation warnings, the recorded workspace and run configuration. A missing or removed workspace is not recreated. Viewing a run never starts another run. History and output retention remain backend-controlled. New PTY run snapshots remove terminal escapes (including colours and cursor moves) before saving up to the newest 256 KiB of UTF-8 text, so a long log does not discard the final report. This is plain text, not a terminal screen replay. Older saved output is not rewritten.

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

### Manual configuration edits

You can edit the selected instance's `automations.json` with an external editor.
Reads and edits reject incomplete or invalid documents without renaming or
replacing them. If a read overlaps an in-place save, it can report an error;
after the save finishes, Refresh or the next runtime tick reads the completed
file. A genuinely invalid document continues to report an error until you repair
it. TUIC does not replace it with an empty configuration.

### GitHub issue intake (allowlisted, off by default)

`automations.json` accepts an `event_intake` block. It stays off until `repositories` lists at least one `owner/name`:

```json
"event_intake": { "repositories": ["owner/name"], "label": "tuic-run", "definition_id": "<definition id>" }
```

After acquiring executor ownership, the daemon starts the shared GitHub poller for the allowlisted repositories without a UI call. Each entry must resolve to a registered, unambiguous github.com repository on that daemon; otherwise its logs show an **Event intake configuration error** with the repository name. Each failing entry is reported separately; valid entries continue polling. The runtime re-reads configuration on its existing 30-second tick and on repository/settings changes. Intake requests all open issues regardless of a client's issue filter or subscribed paths. If a repository belongs to both subscriptions, one GraphQL request selects unfiltered intake issues and separately filtered client issues. The sidebar and SSE snapshots retain the client filter, including disabled issues; internal intake snapshots never reach clients. Client stop releases only the client subscription while intake remains enabled. Removing the allowlist removes the intake subscription; normal client polling remains unchanged. An open issue that carries the label starts the named, enabled definition once per issue and label; the key is stored in the run ledger and survives restarts. If an open run or the concurrency limit refuses it, a later 30-second tick admits it. The agent receives only the issue URL, never its title or body.

When such a run completes in an isolated workspace (`new_per_run`), TUIC creates a draft pull request for the workspace branch through its GitHub API layer, using the TUIC GitHub login. The agent must push that branch itself. Publication re-checks the allowlist and that the workspace remote is the admitted repository; no other repository is ever written.

Downgrading TUICommander after an event run exists is not supported: an older build cannot read `Event` runs or the `event_intake` block.


### Optional triage before labelled intake

Add `triage` entries inside `event_intake` in the owning daemon's
`automations.json`. An omitted or empty array keeps triage off:

```json
"event_intake": {
  "repositories": ["owner/name"],
  "label": "tuic-run",
  "definition_id": "implement-issue",
  "triage": [{
    "repository": "owner/name",
    "definition_id": "triage-issue",
    "policy_version": "1",
    "read_only_attested": true,
    "rejection_label": "triage-rejected"
  }]
}
```

Create both definitions first. The triage definition must be separate from the
implementation definition, use an existing workspace, and have no precheck.
Its prompt contains your acceptance policy. Its saved `run_config` must name an
operator-attested read-only profile without push credentials. Its automatic cron
schedule is suppressed while it is referenced by a triage entry. Keep the
triage definition enabled to admit issues. Configure the labels in GitHub before
enabling the policy. The rejection label must differ from the intake label.

**This is operator attestation, not an isolation guarantee.** TUIC does not remove
or enforce the absence of host git/gh credentials, inherited environment access,
filesystem writes or MCP write authority for the evaluator. Do not set
`read_only_attested` unless you accept this trust contract and have configured the
external profile accordingly. Recheck the attestation when the profile changes.
Enforced ego isolation is future story 2450-6845.

The daemon fetches all open issues when any triage entry is enabled in the
configuration. Admission remains per repository. Before any triage or subsequent
implementation run in that repository, the issue must have GitHub author
association `OWNER`, `MEMBER` or `COLLABORATOR`. Missing or other associations fail
closed. Each eligible issue gets one triage run per repository, issue number and
policy version, even if it already has the intake label. Existing overlap and
concurrency limits leave capacity-refused issues available for a later poll.
Change `policy_version` to evaluate previously decided issues again; changing only
the definition id does not reset the identity.

The managed evaluator receives the issue URL, the policy prompt and its run id.
It must treat the issue text as data and must not edit files, execute repository
code, push, comment or label. It reports with the bound MCP tool:

```json
{"action":"triage_decision","id":"<run id>","decision":{"outcome":"rejected","reason":"Outside the repository policy"}}
```

Allowed outcomes are `accepted`, `rejected` and `needs_input`. Only the run's bound
managed PTY can report. A report completes the run; identical retries return the
same decision, and a conflicting decision is rejected. Terminal output and
`progress done` are not decision reports. A run that ends or is interrupted before
reporting records `needs_input` and cannot start implementation. Decisions are in
`run`/`list_runs` as `triage_decision`, and the existing run reason shows the result.
Triage records are retained beyond the normal 90-day execution-history retention
because later admission needs the durable decision.

The host separately rechecks the current policy, allowlist and resolved workspace
GitHub remote. Acceptance adds the intake label; the existing labelled intake then
starts implementation and retains its existing draft-PR path. Rejection posts one
reason comment, then adds the rejection label; it does not close the issue.
`needs_input` performs no GitHub write. Rejected and undecided issues cannot bypass
triage by carrying the intake label.

Each comment/label attempt is durably claimed before the request, including across
restart. Successful attempts are `confirmed`. A crash or ambiguous response is
`unknown` and is **not automatically retried**: it may already have arrived. An
unknown comment also stops automatic rejection-label publication. Inspect the run's
`triage-comment` / `triage-label` GitHub delivery records in the owning daemon's
`automation_runs.sqlite3` and reconcile the issue on GitHub before any manual
repair. This prevents duplicate automatic comments, but it does not guarantee
successful delivery during an outage. Do not change the policy version merely to
retry an uncertain publication. Removing the policy prevents pending publication.
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

### Opt-in reviewed issue workflows

To require independent review and checks before draft publication, set an explicit
published story workflow on the owning daemon:

```json
"event_intake": {
  "repositories": ["owner/name"],
  "label": "tuic-run",
  "definition_id": "issue-automation",
  "workflow": { "definition_id": "published-story-workflow", "definition_revision": 1 }
}
```

The workflow must belong to the automation's registered project and have configured
deterministic checks. Use `new_per_run`: the existing automation workspace and
precheck are retained, then that isolated workspace is bound to the native workflow
before any worker starts. TUIC creates one native plan and GitHub-origin story for
the event, with the prepared automation prompt as its problem statement (maximum
20,000 characters). The workflow's profiles select its implementer and independent
reviewer; the legacy automation `run_config` is not launched on this path.

The admitted run pins the workflow selection and records its native run/story IDs.
Removing or changing the setting cannot downgrade that run. The daemon follows
native workflow completion, not an implementer's `done` report. Workflow pauses
show as `needs_you`; the automation deadline cancels the bound workflow and stops
its owned sessions. Existing configurations without `workflow` keep their previous
execution and publication behavior. There is no automatic workflow default.

Before creating the draft, TUIC reuses the native independent review and configured
check gate against the current clean artifact, then reads the branch from GitHub
and requires its pushed SHA to match that artifact. Missing or stale evidence
refuses publication and records the reason in automation history. The draft body
includes the independent review summary and findings. The implementer must still
push its branch; this path does not merge or approve a GitHub PR. As with any
branch-addressed GitHub PR creation, the remote SHA read and PR creation are separate
API operations; do not let another writer move the branch during publication.

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

## Incoming PR reviews (review-only)

On the owning host, add a separate block to the selected instance's `automations.json`:

```json
"pull_request_reviews": {
  "repositories": ["owner/name"],
  "definition_id": "<enabled automation definition id>"
}
```

An empty repository list disables PR review intake. This list is independent of
`event_intake`; enabling issue intake does not authorize PR reviews. Repositories
must be registered on that host with an unambiguous github.com account. The host
must have working ego configuration and permission to post GitHub reviews.
Confirm publication authorization before saving this opt-in: it permits unattended
COMMENT reviews on open, non-draft PRs in the allowlist, including fork PRs.

The selected definition supplies its name, enabled state and duration limit. Its
prompt, run profile, precheck and workspace settings are not executed. While bound
to this subscription it has no scheduled launches and Run Now is refused. Use a
separate definition for issue intake or ordinary agent automation. Every review
uses the registered repository path from the poller, not the definition's checkout.

The host admits one durable run per repository, PR number and GitHub head SHA,
even if the selected definition later changes. Open-run overlap and machine-wide
capacity limits still apply; the latest snapshot is reconsidered on each tick.
A new observed head revokes publication authority for an older result, including
when capacity delays the newer run. Bus lag drops snapshots until a poller resync.
Only observed heads can be admitted; this is not an exhaustive PR-history import.

The existing no-host-tool ACP one-shot adapter reviews an immutable base/head
comparison. The usual review file/line budgets and confidence filter apply. The
host rechecks the current head, open/draft state, registration and subscription,
then claims publication durably before one `POST pulls/{number}/reviews` with
`event: COMMENT` and `commit_id` equal to the GitHub SHA. The manual review panel's
diff hash is not used as a commit SHA. Findings appear in the review body, not as
inline review threads. No code repair, deterministic check execution, check-result
publication, branch push, worktree creation, PR creation, approval or merge occurs.

Saved run output and outcome use the existing automation history API on desktop
and HTTP. A review URL is retained in `outcome.pr_url`. Superseded run decisions are
retained but their results are not posted. A lost POST response, or restart during a publication
claim, becomes `publication_unknown` and is never retried automatically. Restart
interrupts unfinished ACP runs without replay. Inspect GitHub before taking any
manual recovery action. An older application cannot read the new trigger/config;
do not downgrade an instance after enabling this feature.
