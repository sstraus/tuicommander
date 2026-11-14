# Workflow cutover 019-683f: preparation before the operator boundary

> **Superseded (2026-11-14).** Story 019-683f is wontfix. Story 3034-365e deleted
> every legacy engine named below, its data and its UI, with no transfer, fence or
> rollback bridge. The automations prompt scheduler and the neutral engine remain.
> The one-time store migration and its backups are described in
> [Workflow definitions](workflows.md#legacy-engine-removed-3034-365e). The rest of
> this document is historical; the code paths it names no longer exist.

This document is the inspectable proposal of story 019-683f. It records what the
code does now and what each later step needs. **Nothing in it is executed.** No
trigger is transferred, no legacy run is drained, no store is migrated and no
legacy code is deleted. Each later step needs the operator approval in
[the approval question](#approval-question-for-the-operator). Orchestrator story
2530 forbids operational activation before 2026-11-15.

Line numbers refer to the tree of branch `feat/wer-019-cutover-prep`.

## Three engines consume triggers today

| Engine (`cutover_snapshot::Engine`) | What it consumes | Receipt namespace |
|---|---|---|
| `automations` | `automations.json`: `pull_request_reviews`, `event_intake` (label start and `triage`), prompt schedules | `automation_event_keys`: `@pr_review` (repository, PR, head), `@triage` (repository, issue, policy, generation), `<definition>` (repository, issue, label); scheduled cursor |
| `legacy_graph` | Kind bindings `workflow_trigger_bindings` (`pull_request_poll`, `issue_label_poll`) and manual plan runs; run by the role/kind graph scheduler | `workflow_pr_subjects` (repository, PR, head, base, policy); `workflow_issue_admissions` (repository, issue, digest) |
| `neutral` | `neutral_trigger_bindings` (GitHub, GitLab, scheduled) | `neutral_trigger_intents` (binding, resource, epoch generation) |

The existing operator cutover (`automations/cutover.rs:331` `cutover_pr_review`,
`:410` `cutover_issue_intake`, rollbacks at `:495` and `:568`) moves a repository
from `automations` to `legacy_graph` only. The kind-binding consumer skips a
repository that its legacy subscription still lists (`cutover.rs:25`,
`native_issues.rs:17`). **No fence exists between `neutral` and the two legacy
engines**: an enabled neutral binding consumes beside a legacy consumer of the
same repository.

## Criterion 1: the snapshot (implemented, read-only)

`automation_action {"action":"cutover_snapshot"}` on IPC and `POST /automations`
returns `CutoverSnapshot` (`src-tauri/src/automations/cutover_snapshot.rs`).
It writes nothing. It needs the automation runtime owner, like the other
cutover actions. It is not in the MCP `automations` action list.

- `definitions` (`workflows/migration/staged.rs`): every legacy definition of
  the registered projects with its digest and every converted draft staged from
  it (`neutral_migration_conversions`). `current` is false when the source
  changed after the conversion. `enabledBindings` lists enabled neutral
  bindings that start the converted draft or a publication it pins; it is empty
  while the draft stays inactive.
- `triggers`: one row per trigger key and family. Each consumer has its engine,
  revision, `enabled`, `consuming` (enabled and not fenced) and its receipt
  namespace. `owners` lists the engines that consume now.
- `legacyRuns`: open automation runs, finished PR-review, triage and event runs
  whose publication is not settled (`admitted`, `publication_pending`,
  `publication_unknown`), and running or paused legacy graph runs, each with the
  trigger that started it.
- `blockers`: a stale or already-started conversion, more than one consuming
  engine, a legacy-only trigger with no staged neutral binding, open legacy runs
  of a trigger, unsettled publications and open manual legacy runs.

Converted drafts are staged inactive by `migration::convert`
(`workflows/migration/convert.rs:84`, one transaction in `:388`
`stage_converted_draft`).
A neutral draft never runs; only a publication that an enabled binding or a
manual start selects runs. The snapshot reports a violation of that rule.

### Receipt namespace reconciliation (finding)

The legacy keys and the neutral keys are not comparable:

- A legacy PR key holds the head SHA; a legacy issue key holds the requirement
  digest or the label.
- A neutral GitHub generation is a local epoch counter per resource
  (`run/neutral_github_source.rs:160`), bumped when `relevant()` changes
  (`:235`: PR open/head/base; issue open/material digest/labels).
- When a neutral GitHub binding is enabled, its first complete poll records
  every open resource at its current epoch as `baseline`, not as a run
  (`run/neutral_github_admission.rs:55-67`). No legacy key is imported.

Consequences for a transfer to `neutral`:

1. A resource that the legacy owner already consumed at its current head or
   digest is not consumed again: the baseline covers it. No shared key is needed
   for this case.
2. A resource that changed after the last legacy poll and before the neutral
   baseline is baselined without a run. The legacy owner never saw it, and the
   neutral owner skips it. Owner replay (`run/neutral_triggers.rs:81`
   `replay_trigger`, `ReplayAuthority`) starts it explicitly.
3. A rollback from `neutral` to a legacy owner has **no bridge**. The existing
   rollbacks record `native:<binding>` keys for `legacy_graph` subjects only
   (`cutover.rs:599`, `:534`). A neutral-to-legacy rollback must first record
   every started neutral intent as a consumed legacy key; that code does not
   exist.
4. **Decisive code evidence:** `neutral_trigger_self_events` is created and
   read (`run/neutral_trigger_store.rs:26`, `:205` `trigger_suppressed`) but
   no production code writes it. A neutral run's own label write therefore
   bumps the issue epoch and emits a new `labeled` event. The critic of 2962
   named this hazard (`workflow-rework-landing-requirements-2026-11-12.md`,
   last section). The issue-trigger transfer stays blocked until the writer
   exists or the operator accepts the behaviour.

## Trigger-transfer plan (prepared, not executed)

One transfer moves one key and family from its current owner to its staged
neutral binding, once. Do it per repository and family; never as a batch.

1. **Snapshot before.** Call `cutover_snapshot` and save the reply. The row must
   have exactly one owner, a staged disabled neutral binding, no open legacy run
   and no unsettled publication. Any blocker for the row refuses the transfer.
2. **Fence the legacy owner.** `automations`: remove the repository from
   `pull_request_reviews` (`definitions.rs:150`) or from `event_intake` and its
   triage policy (`definitions.rs:173`). `legacy_graph`: disable the kind binding
   (`store/bindings.rs` `set_trigger_binding_enabled`). Nothing consumes the key
   after this write.
3. **Drain.** Wait until the snapshot shows no legacy run for the key. Runs drain
   under their existing owner. The neutral engine never takes a legacy run
   position (compatibility table: `historical_replay`).
4. **List the gap.** Read the open resources (one poll) and compare each current
   head or digest with the legacy keys. Every resource the legacy owner did not
   consume goes to the replay list of step 6.
5. **Enable the neutral binding** with the current overlap acknowledgement
   (`trigger_bindings.rs:89` `enable_neutral_trigger_binding`). The first
   complete poll records the baseline. The binding must start the publication of
   a `current` converted draft and supply the inputs that its conversion record
   names under `019:` (for example `convert_pr_review.rs:58`, `convert.rs:381`).
6. **Replay the gap** with owner replay, one resource at a time.
7. **Snapshot after.** The row has `owners == [neutral]` and no blocker.

Shared effect and delivery keys are needed only where two owners can act on
one resource. Steps 2 and 3 remove that window for PR review and issue triage.
They stay necessary for the rollback (finding 3) and, if the operator wants to
skip the drain, for every effect kind that a draining legacy run can still
write.

Prompt schedules in `automations.json` launch an agent prompt, not a workflow
(`scheduled_trigger.rs:73` lets neutral schedules share the same tick). No
converter exists for them. This plan leaves them under `automations` unless the
operator decides otherwise.

## Rollback and recoverability check (prepared, not executed)

Before the first transfer:

- Copy the three SQLite stores (workflow definitions, workflow runs, automation
  runs) and `automations.json` while the app is stopped. Restoring the copy is
  the recovery path for a store failure. It never replays an effect: effects live
  in the run ledgers, and an uncertain effect stays uncertain after a restore.
- Implement the neutral-to-legacy key bridge of finding 3 and give it a
  regression test: a rollback after a neutral run reviewed head H must not let
  the legacy owner review H again.

Rollback of one key, in this order:

1. Disable the neutral binding. Nothing new starts.
2. Let neutral runs of the key finish, or cancel them. A cancelled effect with an
   unknown outcome stays `uncertain` in the neutral ledger and is reconciled
   there (`neutral_effects.rs`); it is never replayed by the legacy owner.
3. Record every started neutral intent of the key as a consumed legacy key.
4. Re-enable the legacy consumer.
5. Snapshot after: `owners == [<legacy engine>]`, no blocker.

Check items for each rehearsal on a scratch store: the before and after
snapshots differ only in revisions and owners; no resource has a run in both
engines; no uncertain effect changes state during the rollback.

## Obsolete code to delete after the parity rows pass (prepared, not executed)

Criterion 7 gates every deletion. The affected rows are the 147 rows of
`workflow-baseline/capabilities.json` (105 capabilities, 42 screenshot
features). Every conversion record carries each of them as an open blocker
(`convert.rs` `open_blockers`, `preservation <id>`); the snapshot shows the
count per converted draft. A row passes with exact installed pins plus neutral
behaviour evidence, or with the original explicit exclusion. 018-bb68 proved
integrated parity A to E; it did not dispose rows one by one. **Today every row is
unresolved, so no deletion is allowed.**

Before each deletion, ask `mdkb code callers` for every symbol: the neutral
engine shares `RuntimeOwner` (`run/runtime.rs:44`) and some legacy modules.

| Area | Code | Keep for inspection |
|---|---|---|
| Role/kind graph scheduler | `run/runtime.rs:350` `run_actor`, `:678` `drive_turn`; legacy submodules `run/runtime/{continuation,effects,issue,issue_handoff,issue_publication,issue_ready,judge,plan,policy,pull_request,telegram}.rs` (`run/runtime.rs:3-15`); `run/store.rs:374` `start_plan`, `:395` `start_plan_pre_policy`; `run/store/runtime_start.rs:41-120` graph, binding and PR starts | `run/reducer.rs:5` `apply_event` and `run/graph.rs` replay of retained runs (historical inspection without an executor) |
| Kind bindings and their consumer | `workflows/store/bindings.rs` (`pull_request_poll`, `issue_label_poll`); `store/pr_subjects.rs`; `store/issue.rs` admissions; `automations/cutover.rs:25-292` native consumer and poll glue, `:331-621` legacy-to-kind cutover and rollbacks; `automations/native_issues.rs`; `automations/runtime.rs:181-183`, `:318-334` poll calls; `run/store/runtime_start.rs:496-512` | Binding rows as history until the stores are retired |
| Legacy node kinds | `workflows/definition.rs:6` `WorkflowKind`, `:51` `AgentRole`, `:63` `AgentExecution`, `:76` `NodeKind` as executable input | The serde types, to read retained publications and run events |
| Global policy UI | `src/components/StoriesDialog/StoriesDialog.tsx:80`, `:483`, `:757-810`, `:1590-1618` host routing policy preview and import; `src/components/WorkflowDesigner/ReviewPolicy.tsx`, `WorkflowInspector.tsx:6`, `:31-32`, `WorkflowDesigner.tsx:367`, `types.ts:5`, `:58` legacy evidence policy; backend `workflows/api.rs:16`, `:28`, `:32`, `:210-227` and `workflows/agent_policy.rs` | Imported policy pins in retained runs (`RunDefinitionPins.tsx`) |
| Legacy automations subscriptions | `automations/pr_reviews*`, `event_intake*`, `triage*` once no repository is listed | Run history |

The migration preview (`workflows/migration/mapping.rs`) reads the routing
policy. Delete the policy reader after the last conversion, not before.

## Criterion 8: compatibility decisions

`src-tauri/src/workflows/fixtures/cutover-compatibility.json` names one decision
for each legacy source and version: the seeded templates (unedited and edited),
user definitions, legacy run event contracts 2 to 6, the run store layouts, the
definition file format, `automations.json`, the preservation baseline and the
neutral publication format. A source or version that is not in the table is
refused. `migration_compatibility_table_states_the_decision_convert_makes_for_each_template`
checks the template rows against `convert`; the other rows name the existing
tests that enforce them.

## Approval question for the operator

See the RESULT of task t-019w for the exact question. It asks for approval of:
the per-key transfer procedure above (fence, drain under the existing owner,
baseline, replay of the listed gap); the implementation of the
neutral-to-legacy rollback bridge and of the self-event writer before any issue
transfer; and whether prompt schedules stay under `automations`.
