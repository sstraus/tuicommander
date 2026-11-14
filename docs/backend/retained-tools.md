# Effective policy and retained tools (WF16A)

This page is a contract, not a code module. TUICommander has no policy audit and no
retained-tool registry at runtime: no IPC, HTTP or MCP surface, no timer and no
scheduling. The operator's own tooling implements the contract below outside this
repository. The global routing authority of the operator does not change.

## Effective policy

A checked-in hook or rule file is source, not an installed policy. An audit of the
effective policy reads the installed locations of each supported harness and reports each
(harness, kind) row in one of three states:

| State | Meaning |
|---|---|
| `installed` | An installed file was read and holds the marker of the row. This proves that the text is installed, not that a running session loaded it. |
| `absent` | Every candidate location was read and none holds the marker. |
| `unknown` | No verified location exists for the harness, or a candidate could not be read. |

A report is healthy only when every row is `installed`. `unknown` never counts as healthy.
A file that exists is not enough when the harness loads that file through a different file:
the audit must find the loader that imports it.

Policy kinds:

| Kind | Rule |
|---|---|
| Global routing | One routing authority selects models and delegation for every harness. |
| Source verification | Search the knowledge index first, then verify in source. |
| Handoff | Write a handoff at session end. |
| Phase test policy | Run the tests of the changed behavior only; broader suites run once per batch. |
| No pattern-wide kill | Stop only the processes that the agent started; never kill by name pattern. |
| PATH protection | Do not change the shared tool path that other agents use. |
| Unowned deferral | A deferred item carries a dated inline note at its site. |
| Owned-tool failure | A failure of a tool is fixed at the owner of that tool. |

### Duplicated rules

The no-pattern-kill and PATH rules can appear as an enforcement hook and as instruction
text. The hook is the enforcement point; the text explains it. Unowned deferral is enforced
by a pre-commit gate. Owned-tool failure is a rule with one home at the owner of the tool.
WF16A moves none of them.

## Retained tools

A retained tool is an existing deterministic executable that stays available after the
coordinator pump is retired. Each entry of a retained-tool list declares:

| Field | Contract |
|---|---|
| Id | Unique, not empty. |
| Entry point | One executable path, relative to the checkout that owns it. |
| Invocation | Manual on request, or a durable timer. A timer needs a tool-run timer purpose, which the durable timer does not have today. |
| Prerequisites | Each one is checked before the run; a prerequisite that cannot be checked is `unknown`, never assumed. |
| Owner | The repository that owns the entry point. |
| Output receipt | What a run leaves behind (stdout text, files, exit code meaning). |

The list is short and bounded. No entry point, invocation or prerequisite depends on
retired scheduling (pump, agent planning, mail watch, open-work refresh, idle close,
unowned stop). Every invocation returns a receipt: `ran` with the exit code,
`entry_point_missing`, or `unavailable`. A missing entry point is a receipt, not a run.

A row without an executable entry point (for example, a usage document only) is not a
retained tool. Later plan steps must not cite such a row as preserved by WF16A.

## Upkeep convenience subflow (WF16E)

A retained upkeep tool that has a check and an apply mode can run inside a workflow
through the reusable Upkeep with approval subflow (`create_upkeep_draft`, see
[Workflow definitions](workflows.md#upkeep-with-approval-wf16e)). The subflow does not
replace the manual invocation, and apply runs only after a recorded approval.

## Test Janitor evidence comparison (WF16E)

| Field | Value |
|---|---|
| Id | `test-janitor-evidence` |
| Entry point | `.agents/skills/test-janitor/scripts/evidence.py` |
| Invocation | Manual on request, or the `evidence` command of the Test reduction with evidence subflow (`create_test_reduction_draft`, see [Workflow definitions](workflows.md#test-reduction-with-evidence-wf16e)) |
| Prerequisites | Two complete schema-v1 AWS mutation runs (before and after the reduction); Python 3 |
| Owner | The orchestrator repository |
| Output receipt | stdout JSON `baseline_kills`, `retained_kills`, `lost_kills`, `retention`, `mutation_only_tolerance_met`. Exit 0 valid with the tolerance met or N/A; exit 2 invalid or incompatible evidence (no stdout); exit 3 tolerance missed (receipt printed) |
