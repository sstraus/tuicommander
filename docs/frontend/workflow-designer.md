# Neutral Workflow Designer

The Designer edits project-owned neutral drafts. It is not a second workflow
runtime and does not infer execution authority from subjects, references or labels.

## Workspaces

- **Design:** the left palette lists installed catalog descriptors, built-in
  templates, drafts, neutral controls and the project's published workflows.
  **Add as subflow** places a published revision on the graph as one activity:
  the backend `save_subflow` action registers it with its exact content pin and
  interface, or the existing operation for that revision is reused. The Designer
  never computes a digest. Search filters catalog activities by
  namespace, name or purpose. The central canvas draws actual named control
  routes. Select a node to edit it in the right inspector.
- **Inputs & returns:** add, rename and remove typed root fields, edit their
  type/requiredness, map return sources and declare the optional or required
  subject. Renaming or removing a return field moves or drops its return mapping.
  A mapping picks the activity by its node title and the field from the
  descriptor's declared outputs (or the declared workflow inputs); free text
  remains only where nothing is declared. Existing defaults remain intact;
  advanced editing can change them.
- **Preview & runs:** supply sample inputs/subject for the backend's pure intent
  preparation, start a run, or read a retained run. Preview neither starts a run
  nor persists a draft. Unknown values and unavailable preparation are not
  simulated success. **Start run** always starts the latest published revision,
  with typed inputs and explicit limits; edits made after that publication are
  not part of the run. The run summary shows status, current node and returned
  values; the exact snapshot and events stay in a disclosure.
- **Advanced definition:** the complete backend definition editor, pinned
  publication inspection/cloning, trigger bindings and correction-region tools.
  JSON is validated by the same backend before replacing the current definition.

## Graph editing

Select a source node, choose an outgoing route above the canvas, then click a
destination node. The pending connection names the source and port. Cancel it
with the button or Escape in the canvas. The inspector offers the same operation
through named destination selectors; Unconnected removes only that port's edge.
An invalid imported target remains visible as Missing destination for repair.
Changing destinations preserves sibling routes, instance configuration, typed
bindings, scopes and editor labels.

Fit graph shows the topology; zoom controls and the scrollable canvas allow
closer inspection. Nodes on a common control layer occupy separate lanes. A
cyclic, disconnected or missing-target draft still renders for inspection;
rendering does not certify validity. Layout, selection and viewport state are not
saved as executable semantics. Opening another draft resets pending connections.
No drag/drop or pointer-move behavior is added by this workspace.

Activity ports come from the exact namespace/name/version descriptor. Neutral
conditions retain true/false/unknown, scopes their existing complete/blocked or
complete/exhausted/unknown/failed routes, and ends their named outcome. Neither a
route label nor a presentation label changes the backend contract.

## Persistence and validation

Save retains editor labels beside the same neutral definition with expected
revision fencing. Unsaved drafts cannot be replaced by another selected draft,
blank draft or template. Publication still saves no pending edits: it requires a
saved draft and backend validation before publishing. Catalog refresh updates
metadata without replacing unsaved authoring state.

Desktop IPC, HTTP and MCP retain the existing shared neutral action service.
This presentation change introduces no new endpoints or client-side scheduler.

## Current authoring boundaries

The generic catalog and complete definition remain available; this is not a
fixed business-process wizard. Conditions, collections, loops and fork/all-join
scopes are authored natively in the scope inspector, and a scope node opens a
read-only topology of its declared bodies (a view of declarations, not a second
control graph). Recursive object/projection mappings use native source forms;
root and return schemas support add, rename and remove with return mappings
following the change. Published workflows can be added as subflows from the
palette; the backend `save_subflow` action pins the exact publication.
Advanced definition stays available for anything outside these forms.

Scope body ownership: every activity instance has exactly one owner, an ordinary
activity node or one scope body/branch. Body pickers offer only unowned
instances plus the scope's own current body, a new scope never defaults to an
ordinal held by another owner, and choosing a new catalog body for a collection
or loop replaces its body instance in place (a loop's carried results are then
cleared and must be bound again) so no orphan instance is left for the backend
to reject. Selecting a different existing instance from the picker can leave the
previous one unowned; it is offered again until it is assigned.

A conservative return warning is not control-path dominance proof. Backend
validation remains authoritative for types, reachability, cycles, scopes,
required inputs, bounds and publication pins. Side-effect-free preview is not a
live execution test or an approval.

## Live runs and approvals

**Preview & runs** lists the project's active and recent runs, newest first.
Select a row to inspect it; no run ID entry is needed. Each row names the workflow,
publication revision, current node, status and update time. Waiting for approval
is distinct from running. The read-only run graph uses the retained publication,
including its labels, and marks current, completed, failed and waiting nodes with
text and visual states. Node history comes from complete verified backend replay,
so paging the raw events does not erase graph progress. `workflow-run-changed`
refreshes the list and selected inspection; reconnect refreshes both.

An unanswered workflow approval opens a real dialog even with the Designer
closed. Its question and Markdown evidence accompany **Approve** and **Decline**.
**Later** leaves it in the attention inbox and the pending approval count.
An attention toast opens the same dialog; sound follows notification settings.
Desktop and mobile shells share this host. A closed mobile client receives the
existing mobile push notification and restores pending approvals when opened.
The first accepted answer wins through the existing sequence/revision-fenced
command. Other clients retract the dialog, count, bell item and toast on the next
run event; expired, superseded and terminal-run questions are not presented.

The run selection survives workspace tab changes. Bursts of progress events coalesce run-list refreshes over 100 ms. The list retains every projected active run and the latest 50 terminal runs; unreadable histories have an individual unavailable row. Typed operator approvals disclose the exact retained request as well as their Markdown question and details.
