# Design: Issue workflow kind, GitHub issue effects and label-poll binding (#2770-aae5)

Status: approved by the coordinator 2026-11-05 (D1 two effect nodes, D2 const `[]` allowlist
changed only by an approved commit, D3 split into three stories). Section 8 records the decisions.
Base: main 7b3ae514cf. Source of the behaviour: `plans/orchestrator-as-workflow-implementation.md`
section 2.5, WF14 and the `issue-delivery@N` canvas. This note fixes the code shape. It does not
change the approved behaviour.

## 1. Model: copy the WF13 pull-request shape

WF13 shipped a kind, a typed effect node, a trigger source, a runtime driver behind ports and a
staged migration. The issue path uses the same five parts.

| Part | WF13 (exists) | 2770 (new) |
|---|---|---|
| Kind | `WorkflowKind::PullRequest` | `WorkflowKind::Issue` (`"issue"`) |
| Subject | `PrSubject` (repository, number, head) | `IssueSubject` (repository, number, requirements digest, admission generation, reporter) |
| Effect node | `NodeKind::GitHubReview { event }` | `NodeKind::GitHubIssue { effect }` and `NodeKind::GitHubPublication { action }` (section 2) |
| Trigger | `TriggerSource::PullRequestPoll` | `TriggerSource::IssueLabelPoll` with a required `label` |
| Driver | `run/runtime/pull_request.rs`, `PrPorts` | `run/runtime/issue.rs`, `IssuePorts` |
| Templates | `pr_review` built-in | `issue_triage` and `issue_delivery` built-ins |
| Migration | `automations/pr_reviews/migration.rs` | `automations/event_intake/migration.rs` |

The store CHECK on `workflow_definitions.kind` gains `'issue'` through the same table rebuild that
`store/pull_request.rs` used for `'pull_request'`.

## 2. Nodes

### 2.1 `IssueTriage` (judge-like evaluator)

`NodeKind::IssueTriage` runs the read-only triage evaluator through an `IssueActivities` port,
exactly as `GitHubReview` uses `ReviewActivities`. It returns the existing `TriageDecision`
(`automations/triage.rs`). Edge outcomes: `accepted`, `rejected`, `needs_input`. The reason text
keeps the existing 1..4000-byte rule. The evaluator stays read-only (the existing
`read_only_attested` floor).

### 2.2 `GitHubIssue { effect }` (the github-issue effect node)

Host-owned issue writes. `effect` is one of:

- `rejection_comment`, `question_comment`, `evidence_comment` or `status_comment`;
- `accepted_label` or `rejection_label`.

As shipped in 2821 this is one flat string enum, not a tagged object with a nested purpose: the
`workflow` schema sits at its 65,536-byte search budget, and the flat form keeps it under.

Edge outcomes: `published`, `not_published`, `uncertain`, as for `GitHubReview`. It reuses
`triage.rs` `send_once`: one retained intent per effect key, lookup-only reconciliation, and no
second POST after an unknown delivery. Comment text comes from the pinned definition. Labels come
from the trigger binding, never from the agent.

### 2.3 `GitHubPublication { action }`

The delivery tail writes to git and to pull requests, not to the issue, so it is a separate node.
`action` is one of `push`, `create_draft_pr` or `mark_ready`. Edge outcomes: `published`,
`refused` and `uncertain`.

- It reuses `automations/publish.rs`: reviewed native approval, exact pushed SHA, saved
  publication intent and lookup-only recovery.
- `create_draft_pr` first checks the unattended-publication allowlist (section 5). An unnamed
  repository gives `refused` with no network call.
- `uncertain` after `push` or `create_draft_pr` blocks the next action. The graph can route only
  to a status comment and End. This is a validation rule, not a template convention.
- `mark_ready` re-reads the PR head and refuses when it differs from the approved head.

Why two nodes and not one: issue writes and repository writes have different authority. The
allowlist applies only to publication. One node with five actions would carry fields that half of
its actions refuse.

### 2.4 Intake bridge

The `Accepted` arm uses the existing intake bridge to create one native story, idempotently
(`event_workflow.rs` `start`), and dispatches the pinned delivery definition once. There is no
Create Stories node in an issue graph, as in the plan.

## 3. Trigger binding and admission

`TriggerSource::IssueLabelPoll` maps to `WorkflowKind::Issue`. `TriggerBindingInput` gains
source-specific options:

```rust
pub enum TriggerOptions { PullRequestPoll, IssueLabelPoll { label: String, rejection_label: String,
    needs_input_comment: bool, policy_version: String } }
```

Admission reuses `event_intake.rs` without a second classifier:

- the trusted repository from the issue URL;
- open state, the label still present when re-read, and OWNER/MEMBER/COLLABORATOR author;
- the 2522 digest generation: a changed requirements digest on a returned issue admits exactly
  one new generation; an unchanged poll admits none; a label removed and added again changes
  nothing.

The shared poller keeps one consumer per subscription. The binding is created disabled. Enabling it
belongs to the WF17 cutover, as for PR reviews.

## 4. Migration (lossless, staged, single consumer)

`import_issue_intake(workflows, config, runs, projects)` copies `import_pr_reviews` and writes the
same receipt types (`ImportReceipt`, `SubscriptionReceipt`, `FieldReceipt`, `HistoryEntry`).

| Legacy field | Goes to | Disposition |
|---|---|---|
| `event_intake.repositories[]` | one binding per repository | `staged_disabled` |
| `event_intake.label` | `IssueLabelPoll.label` | `exact` |
| `event_intake.definition_id` | retained reference on the binding | `retained_reference` |
| `triage[].definition_id`, `policy_version`, `read_only_attested`, `rejection_label` | binding options plus the triage node input | `exact` |
| `triage[].needs_input_comment` (default false) | binding option; false keeps NeedsInput no-write | `exact` |
| `event_intake.workflow` (pinned story workflow) | the delivery definition pin | `exact` |
| a referenced definition's prompt, profile, schedule, precheck, duration | retained reference, never copied over user edits | `retained_reference` |
| any field without an exact equivalent | nothing | `blocked`, with the reason in the receipt |

- History: `Event` and `Triage` runs import as `HistoryEntry`, never as runnable starts.
- Uncertain deliveries are kept as `not_retried`: a comment, a label or a PR whose delivery is
  `Unknown`.
- Idempotence: a second import of the same config returns `unchanged` and leaves the bindings as
  they are.
- Rollback: disable the binding after reconciling native receipts. It never replays a POST and
  never re-arms a consumed event key.

## 5. Unattended PR creation allowlist

The list is empty: `[]`. The plan forbids inferring it from a path or from the plan approval.
Proposal: a const `UNATTENDED_PR_REPOSITORIES: &[&str] = &[]` in the publication gate. Changing
it needs a code change that Boss approves in review. Alternative: an owner-confirmed setting.
Decision D2 below.

## 6. Tests (named bugs; RED where the plan names them)

- Templates and nodes:
  - `insufficient_issue_comments_without_spawning_implementer`;
  - `duplicate_template_cannot_bypass_publication_safety`;
  - `issue_triage_and_delivery_templates_run_end_to_end_on_fixtures`.
- Migration: `automation_template_migration_preserves_settings_and_unknown_effects` and
  `issue_import_is_idempotent_and_rollback_replays_no_post`.
- Delivery tail:
  - `repair_exhaustion_comments_and_creates_no_pr`;
  - `push_failure_or_uncertainty_blocks_pr_and_records_comment`;
  - `uncertain_pr_creation_comments_without_second_post`;
  - `developer_approval_executes_ready_action_on_current_head`;
  - `unnamed_repository_rejects_unattended_pr_creation`.
- The plan says to "preserve" `developer_approval_before_pr_exists_is_refused` and
  `lost_pr_create_response_keeps_publication_uncertain`. Neither exists in the code at
  7b3ae514cf, so part (c) adds both.

Fixtures: the existing fake `PrApi` replies from 2522. A live capture needs a named sandbox
repository, which nobody has approved, so the evidence stays fixture-only.

## 7. Risks

- **Schema budget.** The workflow tool schema has to fit the 65,536-byte `search_tools` budget.
  2521 broke it with one kind and one node (story 2522 worklog, 2026-11-03). This design adds a
  kind, three nodes and trigger options. Measure the advertised `workflow` schema after the first
  node lands, and stop if it exceeds the budget.
- **Size.** This is WF13 size twice over: a kind, three nodes, a driver, two templates, a
  migration, a delivery tail and fixtures. The rb build cost is many runs of the workflows
  module.

## 8. Decisions (coordinator 2026-11-05)

Decided: D1 two effect nodes; D2 a const `[]` allowlist changed only by an approved commit; D3 the
three-story split below, with (b) depending on (a) and (c) on (b). Every slice tests the 65,536-byte
workflow schema budget. 2770 stays the umbrella story.

The options as proposed:


- **D1:** two nodes (`GitHubIssue` and `GitHubPublication`), as above, or one `GitHubIssue` node
  with five actions. Recommendation: two.
- **D2:** allowlist as a const changed only by an approved commit, or as an owner-confirmed
  setting. Recommendation: the const, because the plan treats the list as Boss authority, not as
  configuration.
- **D3:** split 2770 into three stories with one branch each:
  - (a) kind, nodes, trigger and driver, plus the `issue_triage` template;
  - (b) migration;
  - (c) the `issue_delivery` template with the delivery tail.

  Recommendation: split. (a) is the only part without a dependency on the others, and each part
  needs its own critic.

## 9. Part (c) under option C (coordinator 2026-11-05, story 2823-cf79)

The delivery tail runs inside the delivery-candidate **story** definition, where the
workspace, the reviewed commit and the review already live. The issue graph triages and
hands off. There is no cross-run wait.

### 9.1 Hand-off node

`NodeKind::IssueHandoff { story_template_id, story_revision }`, valid only in issue
graphs. It replaces nothing: it is the `accepted` successor in an `issue_delivery`
graph.

1. It reuses the intake bridge (`automations::event_workflow::start`): it creates one native
   plan and story (origin `GitHubIssue`) and starts the pinned story definition in a new
   worktree through `start_graph_in_worktree`.
2. The request id is `issue:<issue run id>:<generation>`, so a repeated hand-off returns
   the same story run.
3. The story run pins the issue run's `IssueSubject` (`RunSnapshot.issue_subject`).
4. Outcomes are `dispatched`, `refused` and `uncertain`. The issue run ends after the
   hand-off.

### 9.2 Issue nodes in story graphs

`GitHubIssue` and `GitHubPublication` become valid in story graphs too. Two guards apply:

- **Publication:** a story definition that contains them is *issue-bound*. The rule is
  derived from the graph, so no new field is needed. The multi-hop uncertain rule (9.4)
  applies to it as it does to an issue graph.
- **Start and runtime:** `start_root` refuses an issue-bound story definition without an
  issue subject. Only the hand-off supplies one. The driver also refuses an issue node in
  a run without `issue_subject`: it records a refused receipt with no network call
  instead of returning an error.

### 9.3 Publication actions on the candidate

- **Candidate:** the story run's worktree, branch, the reviewed commit (`PublicationReview`)
  and the canonical base.
- **Allowlist:** `UNATTENDED_PR_REPOSITORIES: &[&str] = &[]` returns, consulted by every
  action before any network call (decision D2).
- **`push`:** pushes the exact reviewed commit to the run branch.
  - A definite refusal gives `refused`.
  - A transport loss gives `uncertain`, then a lookup-only check that the remote branch
    equals the commit. If they are equal the outcome is `published`; otherwise it stays
    `uncertain`.
- **`create_draft_pr`:** first runs the lookup `find_pr` (same head, base and repository).
  - A found PR gives `published`, with no POST.
  - Otherwise one `create_pr_with` POST. A lost reply means a second lookup only, then
    `uncertain` with a status comment. A second POST never happens.
- **`mark_ready`:** only after a confirmed draft PR and a revision-fenced developer Ask
  that is approved on the current head. It re-reads the head and refuses on a mismatch.
  An uncertain result is reconciled by reading the PR state, never by replaying the
  request.

### 9.4 Multi-hop uncertain rule

The outcome `uncertain` of `push` or `create_draft_pr` may reach only `GitHubIssue`
`status_comment`, `Notify` and `End` on every path after it. No `GitHubPublication`,
agent or other effect may follow. This closes critic F2 of 2821, which checked only one
hop.

### 9.5 Tests

- The named RED tests from section 6.
- `developer_approval_before_pr_exists_is_refused` and
  `lost_pr_create_response_keeps_publication_uncertain`, which the plan calls "preserve"
  but which do not exist yet.
- `issue_bound_story_refuses_to_start_without_issue_subject`.
- `uncertain_publication_cannot_reach_a_later_write_through_a_status_comment`.
