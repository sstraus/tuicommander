# Native plans and stories

The selected plan has an **Edit plan** control in the Plans column. It edits title and document/link with Save and Cancel. Save sends only changed fields using the revision captured when editing started. A conflict keeps the draft and requires Cancel and reopening Edit plan to load the latest version; an active-coordinator refusal keeps the source draft and displays the backend remedy. Successful saves refresh the plan list.

Native plans have a stored `revision`. `update_plan` takes `plan_id`, `expected_revision`, and `patch` with optional `title` and `source`. At least one field is required. The shared service validates both fields and commits them with one revision increment in one transaction; stale revisions leave the plan unchanged. Existing records start at revision 1. A metadata change changes the workflow planning and verification fingerprints, including plans with no stories. Source prose stays in the external document; the update does not move or rewrite that file. Source-changing edits are refused with a user error while a plan coordinator/planner attempt is running, including an unbound attempt or a paused run. Finish the coordinator or cancel its run first. Title-only and unchanged-source edits remain allowed. The same action is available through MCP, HTTP, desktop IPC, and `tuic story`.

Native stories are managed directly in TUICommander. Plans, stories, criteria, dependencies, and revisions are stored in its configuration directory. A plan names one project and a source document; its state is derived from its stories.

Open a project, then select **Plans and Stories** in the toolbar. Choose **New plan** to see Markdown plans in the project's `plans/` and `.claude/plans/` directories. Select a plan; its title comes from its front-matter `title` or first Markdown heading. Files in nested directories such as `plans/archive/` are excluded. Choose **Refresh** after an agent creates a plan file. **Add from path or link** remains available for another document; a web link needs a typed title because its document is not read locally. Add stories with acceptance criteria, priority, an optional story type, and optional relative file paths. Select a story to inspect its criteria, dependencies, and status. To work without an agent terminal, choose **Start work**, check criteria as they are met, then **Submit for review**. Approve the story or request changes; blocking and "Won't fix" are explicit manual actions. A dependency can be added while a story is Ready or Backlog. The dialog works in both desktop and browser mode.

Story details show the persisted origin, plan step, revision and claiming session. A live terminal name appears beside the stable session ID; closed or unavailable terminals retain the ID. Transition history shows committed commands, revisions and actors in backend order, with Refresh history and explicit loading, error and empty states. New story offers Native or Plan step origin; a plan-step story requires a step label.

The dialog keeps the same viewport-bounded size when you switch tabs or select a story or plan. Long content scrolls inside its columns. On desktop, choose **Open in separate window** in its header to detach it into a resizable window. Choose **Bring back to main window** to return it; closing the detached window also restores the dialog. Browser mode keeps the dialog inline.

Use **Status** to show all, open, or closed stories. Open includes every native status except **Done** and **Won't fix**. Combine it with **Story type**: feature, fix, refactor, chore, task, or untyped. Existing stories and creation requests that omit a type remain untyped; no type is inferred from origin or title. These filters and the **List / Kanban** switch remain selected for the application session, including when the dialog is detached and brought back. Detached windows receive the current controls in their panel parameters and return changes through the shared panel-action channel. The board has one column for each native status and uses the same filtered stories as the list. Select a card to inspect its story; the native board does not change status by drag and drop. It follows the Wiz Kanban column and card layout, but is a core Solid view with native story data. The separate Wiz plugin is unchanged.

The close button receives keyboard focus when the dialog opens. If the running backend predates native stories, the dialog asks you to restart TUICommander to load the newer backend.

Before loading plans, the dialog checks whether the running backend has native story support. A missing capability prompts a restart; a story action error remains visible as that action's error, shown as the plain message (for example `criteria are incomplete: unchecked indexes 1` or `plan document not found: <path>`), not as the raw HTTP response. Workflow Designer: **Publish** is disabled while the draft has no edit since its last publication.

"Won't fix" cancels a story without delivering its output. It never satisfies a dependency or releases a dependent story for work. The Rust service marks a story **abandoned** when it or any dependency path reaches a cancelled story; the dialog renders that label. A human can remove a direct cancelled prerequisite from a Backlog story; this changes that story's requirements and makes it Ready only when every remaining prerequisite is Done. A separate Blocked story is never automatically unblocked. A plan remains Active while an unfinished dependent remains. A nonempty plan becomes Done when every story is either Done or Won't fix; an empty plan stays Draft. The service supplies the cancellation count and **All cancelled** flag to the dialog.

Select a Ready, unclaimed story and choose **Start story workflow**. Select an immutable published workflow revision, then choose **Start published workflow**. Unpublished drafts cannot be selected. Start errors appear in the dialog. This dialog has no plan workflow start button. Published plan workflows can start through the owning daemon API with a plan target.

Select **Run history** for the selected plan to inspect persisted workflow runs. A run shows its status, story and attempt counts, and ordered events. Use **Load more events** to continue beyond the first page. The timeline follows run change notifications while the dialog is open; **Refresh** reloads the list of runs. Manual story work does not require a run. Graph runs also show their pinned node activations, decisions with evidence, repair counts and pause reasons/targets. Expand **Event details** for the complete recorded payload. Use **Pause run** or **Cancel run** for an active graph. For recovery, answer any pending input, select the execution and activation, write an explicit **Resolution**, then choose **Resume run**. The backend refuses unresolved effects or invalid recovery targets. Legacy runs can be inspected and cancelled, but the UI cannot resume them.

Use `tuic story '<JSON action>' --project /absolute/project` to call the story service. For example:

```sh
tuic story '{"action":"create_plan","title":"Release","source":"plans/release.md"}' --project /absolute/project
tuic story '{"action":"list_plans"}' --project /absolute/project
tuic story '{"action":"list_plan_sources"}' --project /absolute/project
tuic story '{"action":"add_plan_source","source":"plans/release.md"}' --project /absolute/project
```

The result is JSON with a `type` and `value`. Supply the returned plan ID when creating a story:

```sh
tuic story '{"action":"create_story","input":{"planId":"PLAN_ID","title":"Implement","criteria":["Behavior verified"],"priority":1,"origin":{"type":"native"},"fileScope":[]}}' --project /absolute/project
```

## Amend a story

`update_story` takes `story_id`, `expected_revision`, and a camel-case `patch`. Omitted fields stay unchanged. The patch accepts `title`, `priority` (1–3), `fileScope`, `origin` (including a plan step), nullable `storyType` (`feature`, `fix`, `refactor`, `chore`, `task`), optional `problem` and `risk` (`low`, `medium`, `high`), ordered `criteria`, and the final `dependencies` set. Null clears `storyType`, `problem` or `risk`; old story records default both to absent. Unknown fields, invalid values, cross-plan dependencies, duplicates, cycles and stale revisions are rejected without partial story writes. Only unclaimed Backlog/Ready stories without active or paused workflow ownership can be amended. QA, like In progress, is not editable: manual verification must stay bound to its requirements. Unique byte-identical criteria retain their checks after reorder; changed, new or ambiguous duplicate criteria become unchecked. A priority-only patch preserves all checks. Amendments do not rewrite append-only story notes: a note keeps the subject revision recorded when it was added, even after a later amendment. Dependency changes recompute readiness. Planning and execution fingerprints include the amended requirements/revision, so existing evidence cannot certify changed work.

```sh
tuic story '{"action":"update_story","story_id":"STORY_ID","expected_revision":1,"patch":{"priority":3}}' --project /absolute/project
```

In StoriesDialog, select an eligible story and choose **Edit**. One form edits the title, priority, type, origin/plan step, problem, risk, ordered criteria, file scope and dependencies. Each criterion has its own textarea, with Add, Remove, Up and Down controls. Editing or moving one entry preserves every other entry exactly, including embedded newlines and indentation. File scope uses one path per line. **Save** sends only changed fields with the revision captured when Edit opened; **Cancel** discards the draft. The form hides lifecycle controls while editing. A revision conflict keeps the draft visible and never retries against a newer revision. Cancel and reopen Edit to load the latest record before preparing a new amendment. Workflow ownership is checked by the backend on Save, even when Edit was available when the form opened.

Read `get_story` first and use its current revision. After a revision conflict, reload the story before preparing another amendment. `criteria` and `dependencies` are final lists, not incremental commands. The existing `remove_dependency` action keeps its narrower cancelled-prerequisite rules; use `update_story` for a full requirement amendment before work starts. Plan ownership cannot be moved by this patch.

Changes to an existing story take its current `revision` as `expected_revision`; a stale request is rejected. The `start_manual` transition moves a Ready story into progress without a PTY claim. A terminal `claim` instead takes `--session-id` with a live PTY ID from the same project. Closing that tab releases its claim. A session-bound agent can perform the same transitions as other trusted callers. `transition_history` shows who performed each committed transition, including approval. Any trusted caller can approve after checking the acceptance criteria. Sessionless local HTTP records `local_api` provenance; a valid token records `human` provenance. These labels describe the transport, not a verified person. The desktop and browser dialogs both offer approval.

To remove a cancelled prerequisite, any trusted caller, including a session-bound agent, can send `{"action":"remove_dependency","story_id":"DEPENDENT_ID","dependency_id":"CANCELLED_ID","expected_revision":2}`. Use the dependent story's current revision as `expected_revision`. The dependent must be in Backlog and must already list the prerequisite as a dependency. The prerequisite must belong to the same plan and be Won't fix. Stale revisions, non-Backlog dependents, missing dependencies, and prerequisites in another plan or any state other than Won't fix are rejected.

Caller identity does not restrict dependency removal. Managed MCP calls, the Plans and Stories dialog, and CLI or HTTP calls use the same state and revision rules within their project scope. A managed caller does not need to omit its session identity to remove a dependency.

The same records are available through desktop IPC, guarded HTTP, and the `workflow action=story` MCP entry. `Done` records an approval with actor provenance. In a manual plan with no workflow run, approval immediately releases dependents whose other prerequisites are Done. Once a workflow run owns the plan, a dependent remains held until its accepted prerequisite has a current integration receipt: the checked source commit must be merged into the canonical branch and TUIC must pass the published checks on the merge result. A later unrecorded ref or tree change invalidates that proof. Import, export, and external sync are not part of this feature.

API and CLI callers can filter `list_stories` by native `state` (including `qa`), `priority`, and `story_type`. Add `limit` (1–100) to page the results, then send the returned `next_cursor` as `cursor` with the same plan and filters. The `value` field stays an array; no cursor means the final page. Omitting `limit` preserves a complete filtered list. For example: `tuic story '{"action":"list_stories","plan_id":"<id>","state":"ready","priority":1,"story_type":"fix","limit":25}' --project /absolute/project`. Dispatch order is not affected.

## Manual QA

The dialog shows QA in the story list and its own Kanban column. Select **QA only** in the Status filter. In story details, check the manual criteria, then select **QA passed** or **QA failed**. **Submit for QA** is available during development or review; the backend validates its criteria.

Use the **QA** state when development is complete and only human checks remain. Prefix a manual criterion with `[MANUAL]` when you create the story, or send a transition command such as `{"set_criterion_manual":{"index":1,"manual":true}}`. Set `manual` to `false` to remove the marker. Indexes start at zero. The marker is part of the criterion text, not a second checklist.

Starting work, claiming a story, checking or unchecking a criterion, changing its manual marker, and submitting review evaluate QA readiness in the backend. An In progress or Review story enters QA when at least one criterion is unchecked and every unchecked criterion is manual. An open non-manual criterion prevents QA. An all-manual story enters QA when work starts. This automatic transition shares the triggering revision and transaction and releases its terminal claim.

Use `"submit_qa"` to move an In progress or Review story to QA explicitly. It requires at least one manual criterion and no unchecked non-manual criteria. In QA, check each manual criterion after it passes, then send `"approve"` to reach Done. Approval refuses any unchecked criterion. Send `"reject_qa"` after a failed check to return to In progress. Unchecking a non-manual criterion or removing the manual marker from an unchecked criterion also returns the story to In progress. All commands require the current `expected_revision`.

If a workflow starts a Ready story whose checks are all manual, the story enters QA and the run pauses before it reserves an implementer spawn or creates a worktree. While QA remains pending, resuming the run does not launch an implementer. Fail QA back to development when implementation changes are required.

QA is not Done: it keeps a plan Active and does not release dependent stories. Story lists return `status:"qa"`; the project summary reports a separate `qa` count. Run history incidents identify QA stories with source `story_qa`, including after an agent attempt completes, and describe the next human action. Existing stored story documents, revisions and checks are retained without a JSON rewrite when the status constraint is upgraded.

## Which caller can do what

| Caller | Scope | Reads | Story actions (claim, check criteria, submit review, add dependency, create, approve) | Administrative actions (reject, block, unblock, won't fix, start manual, remove dependency) |
|---|---|---|---|---|
| `workflow action=story` MCP entry in a managed session | The session's own registered project only | Yes | Yes | Yes |
| Plans and Stories dialog | The open project | Yes | Yes | Yes |
| `tuic story ... --project /abs/path` (HTTP `POST /stories/action?path=...`, loopback or authenticated) | Any project named by `--project` | Yes | Yes | Yes (actor provenance follows the session or credentials) |

An orchestrating session in another project cannot read or approve this repo's plan through its `workflow action=story` MCP entry: the tool resolves the project from the calling session, so a plan id of another project is refused with `plan does not belong to project`. It can do both from a shell with `tuic story '{"action":"plan_view","plan_id":"..."}' --project /abs/path/of/the/repo` and, for review, `{"action":"transition","story_id":"...","expected_revision":N,"command":"approve"}`. That path carries no session identity, so it records local API provenance unless credentials are supplied; as stated above, this is a workflow convention, not a security boundary. No cross-project MCP path exists, and none was added.

## MCP tool schema

Send the service action under `input`, for example `{"action":"story","input":{"action":"list_plans"}}`. The `workflow action=story` variant publishes the full `StoryAction` JSON Schema (actions, fields, types, enums, the `origin` tag shape, priority range 1 to 3) in its `inputSchema`. It is generated from the Rust types, so it follows the code. Inside `input` the story fields are camelCase (`planId`, `fileScope`) while the action fields are snake_case (`story_id`, `expected_revision`).

Actor identity is tracking only and never restricts an action. Localhost remains trusted, including local token exchange. Administrative story decisions use the same state and revision rules for human, local API and managed callers.

A workflow plan reaches Done after all stories are approved and their integration checks are current. Moving the canonical branch or committing new work returns the plan to Active until those checks are recertified. Manual plans continue to finish through approval.

The graph runtime is being delivered in stages. The first stage stores replayable graph positions and requires explicit pause destinations and final checks for executable definitions, but does not automatically run the Designer's graph. Existing pre-contract runs remain available for inspection and cancellation; resuming one requires starting a new run instead. New record-only runs retain the existing explicit command controls during rollout. Fork/all-Join settings and automatic execution are not yet Designer capabilities.

The daemon executor now owns recovery and duration timers under an OS run-database lock. Reads never recover live work, and a non-owner daemon refuses run mutations. Graph resume uses `resume_graph {execution_id,activation_id,resolution}` with an explicit pending activation; status-only resume cannot bypass graph position. Graph start controls, Agent effects and delivery policy remain unavailable until their later slices.

## Workflow graph rollout

The internal daemon executor follows published serial Agent, Judge, Loop, Pause, Notify and exclusive Join nodes. Each Agent step uses a run profile: one named `sol` (implementation, coordination, planning) or `sonnet` (review, validation) when saved, otherwise your default run profile in Settings > Agents (or the only one saved). A default profile that passes a permission-skipping flag such as `--dangerously-bypass-approvals-and-sandbox` is refused: save a profile named `sol` or `sonnet` to choose it for workflows. The Start dialog names a missing profile before the run starts and disables Start. Failed effects pause the run with a repository progress notice; Run history shows the cause, and Resume run offers the pending activation once the cause is fixed. Manual pause time does not consume active duration. Resume resolves the pinned target without resetting repair limits.

The Designer refuses new publications containing unsupported Gate or plan create/dispatch/coordinator nodes. Keep those graphs as drafts. Existing built-in publications remain readable; public graph start controls and full Resolve plan execution are still in development. A completed graph does not approve or merge a story by itself.

### Designer actions without a UI control

These actions exist in the backend and have no Designer or dialog control. Two surfaces reach them:

- **HTTP and desktop IPC only:** `POST /workflows/definition/action` and `workflow_definition_action`. The MCP `workflow` tool has no definition action, so agents cannot call these.
- **HTTP, desktop IPC and MCP:** `POST /workflows/run/action` and `workflow_run_action`. The MCP `workflow action=run` takes the same run actions.

| Action | Surface | Designer limit |
|---|---|---|
| Remove one edge | `update_draft` with the full new graph (HTTP/IPC only) | **Remove node** drops that node's edges. **Connect** replaces an edge with the same source and outcome. No control removes a single edge. |
| Agent role and capabilities | `update_draft` (fields of an Agent node; HTTP/IPC only) | The palette preset sets both when you add the node. The inspector edits only the prompt template. |
| Closure policy | `update_closure` (HTTP/IPC only) | No control. New drafts use `human`. |
| Required checks | `update_checks` (HTTP/IPC only) | No control. |
| `execute_check` | run action (HTTP, IPC, MCP) | No control. Runs one pinned check in the story worktree. |
| `record_integration` | run action (HTTP, IPC, MCP) | No control. Records a completed merge. |
| `recertify_canonical` | run action (HTTP, IPC, MCP) | No control. |


### Workflow story policy (slice D)

The daemon runs pinned deterministic checks before accepting an independent reviewer approval. Native approval history preserves the reviewer actor and revision; the run retains the reviewed artifact and check receipts. A workflow implementer cannot approve its story after exit or claim release. Integration remains an explicit operator action.


### Workflow plan dispatch (slice E)

The owning daemon executes pinned plan coordinator and Create Stories visits through the existing proposal effects. Disjoint story children share a bounded project wave; overlapping or unknown scopes wait. A dependency starts only after its accepted revision has a current explicit Git integration receipt. Done alone and WontFix never release it. Approved children wait for an operator merge. The final plan Judge runs the pinned canonical checks and binds verification to the current plan fingerprint. Failed checks route through a bounded replan with a fresh coordinator attempt. Public start/history controls and parallel graph branches follow in later slices.

### Incidents and next steps

Select a plan, open **Run history**, and select a run. **Incidents and next steps** shows recorded failures, interruptions, unanswered input, and the next operator action. Each entry identifies its run and, where available, story, attempt, node, session and task. Report summaries and input questions come from workflow reports. Active bound attempts can also show session exit codes, awaiting-input state, pending initial-prompt delivery failures and failed, cancelled or input-required task records. A working session alone is not classified as stuck.

Suggestions do not run commands or retry work. Use the existing input form or **Resume run** when ready. A timed-out initial prompt remains in the existing delivery queue: inspect the session before sending it again. **Refresh incidents** reloads transient session/task evidence; run changes refresh the section automatically. Report outcomes persist with the run, while session/task evidence exists only while retained. A paused run without retained cause says so. Agents not bound to the selected native run are outside this view.

## Notes through the API and CLI

Append commentary with `tuic story '{"action":"add_note","story_id":"STORY_ID","expected_revision":1,"text":"Verified the observable effect"}' --project /absolute/project`. Read it with `{"action":"list_notes","story_id":"STORY_ID"}`. The backend records the author provenance, time, and subject revision. Notes cannot be edited or removed, and adding a note does not change the story revision or invalidate its workflow evidence.

An optional `receipts` array holds up to 16 `{runId, commandId}` references to persisted workflow receipts in the same project and plan. These are references, not copied results or proof of current approval. Note text is required and limited to 16,000 characters. The same actions are available through `workflow action=story` MCP, HTTP, and desktop IPC. Select a story and use **Notes and evidence** to read its trail. Each entry shows author provenance, time, the revision it was written against, and any workflow receipt references. Enter text in **Add note** and choose **Append note**. Errors keep the draft; a confirmed append clears it. Use **Refresh notes** to load a note added by an agent or another window. Selecting another story starts a separate empty draft.

### Reversible native record archival

In Plans and Stories, **Archive plan** and **Archive story** move the selected record out of the default list. Use **Archived plans** or **Archived stories** to select archived records, then choose **Restore plan** or **Restore story**. The toggles are independent, so archived stories remain reachable inside active plans. Archived detail stays readable; restore before starting work. Backend refusal messages remain visible when a claim or active run owns the record.

All native story transports share `archive_story`, `unarchive_story`, `archive_plan`, and `unarchive_plan`. Story actions take `story_id`; plan actions take `plan_id`. Each requires `expected_revision` and returns the updated record with its incremented `revision` and `archived` flag. Stale revisions fail without a write. Archival preserves status, criteria, dependencies, and child archive flags; it never deletes records or cascades to children.

`list_plans` and `list_stories` default to `archived: false`. Set `archived: true` to select archived records. The story filter combines with the other filters and is bound into pagination cursors. Direct reads remain available. Default plan views and workflow dispatch omit archived stories; archived plans and stories cannot start manual or workflow work. Restore the parent plan before starting its stories.

Both archive and restore refuse claimed stories and plans that contain a claimed story. An active or paused plan-root workflow owns the plan's story set. A story-root workflow owns its target and execution stories, not unrelated siblings. Cleanup of an owned story or its parent plan is refused until the run ends. The workflow writer lock remains held through the archive transaction, so a concurrent run cannot start between the ownership check and the write.

`story_archive_history` (`story_id`) and `plan_archive_history` (`plan_id`) return `{type:"archive_history",value:[{revision,archived,actor}]}` in revision order. Events commit atomically with the record and retain caller provenance. Existing status transition history remains separate. Consecutive archive-only revisions preserve accepted-work integration receipts; a work change or a moved canonical Git ref still invalidates them. Example: `tuic story '{"action":"archive_story","story_id":"<id>","expected_revision":1}' --project /absolute/project`.

## Ideas through the planner

Ideas are separate from append-only story notes. They use the Ideas panel's `notes.json`, not the story database. No plan is required.

```sh
tuic story '{"action":"add_idea","text":"Investigate a smaller build","source":"ego"}' --project /absolute/project
tuic story '{"action":"list_ideas"}' --project /absolute/project
tuic story '{"action":"update_idea","idea_id":"note-ID","text":"Investigate the incremental build"}' --project /absolute/project
```

The same inputs work under `workflow` with `action:"story"` and `input` set to the JSON action. MCP uses the calling session's project. CLI and HTTP use the named project. `add_idea` records this project as `repoPath`; `list_ideas` includes that project's Ideas and global Ideas, just like the panel. `update_idea` can change only text on an Idea owned by that project. It preserves the id, timestamp, source, image list, used state and repository assignment. Global Ideas are visible but cannot be updated through a project-scoped action.

Text is trimmed and must contain 1–16000 characters. Optional `source` must contain 1–256 characters after trimming. Without it, the source is the bound session id, or `local-api` for a sessionless request. Replies are `{type:"idea",value:Idea}` for add/update and `{type:"ideas",value:Idea[]}` for list. Idea records have camelCase fields: `id`, `text`, `createdAt` (Unix milliseconds), `repoPath`, `repoDisplayName`, `usedAt`, `images` and `source`.

All mutations use the backend's locked notes writer. An `ideas-changed` desktop/SSE event reloads the panel without saving the read back. Local edits finish before reload; stale UI snapshots apply only their changed ids and fields to current disk state. They cannot remove unseen agent additions or restore a remotely deleted Idea. Concurrent text edits use the last committed text intent.
