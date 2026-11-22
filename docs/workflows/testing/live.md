# Workflow live suite (L3)

An opt-in suite. It runs against real endpoints and never passes by mock. A test
without its credential or endpoint reports `skipped` or `unavailable` and names
the missing variables. Run it with `scripts/workflow-live.sh`.

## Run

```sh
scripts/workflow-live.sh          # run here
scripts/workflow-live.sh --list   # list the selected tests
WF_LIVE_RUNNER="build-slot.sh --remote" scripts/workflow-live.sh   # on rb
```

Use rb when the local network drops (story 018 worklog: the live GitHub run failed
here with `Network is down` and passed on rb). Credentials come only from the
process environment or from `RB_SECRET_ENV_FILE` (sourced, never printed).

Evidence: `<checkout>/.tmp/wf-live/<stamp>/evidence.json` (`WF_LIVE_OUT_ROOT` overrides the root) with `head`, `suite_exit`,
`model_runs_used_total` and, per test, `test`, `result`, `run_id`, `cost`, `reason`.
`results.jsonl`, `suite.log` and `github.log` are next to it.

Results: `passed`, `failed`, `skipped` (a model test lacks a variable),
`unavailable` (a connector lacks a variable), `probe_only` (a connector account
answered a read-only request; no workflow touched it). A panic before a result is
`failed`.

## Tests

| Test | Needs | Real effect |
|---|---|---|
| `live_a_scheduled_text_reaches_a_real_model_condition_and_approval` | `TUIC_LIVE_EGO` (absolute path), `TUIC_LIVE_EGO_PROFILE`, `TUIC_LIVE_BUDGET_FILE`, `OPENROUTER_API_KEY` | Scheduled trigger, real model through ego ACP, typed condition, exact local approval. 1 model run. |
| `live_c_plan_delivery_commits_the_bytes_a_real_agent_wrote` | `TUIC_LIVE_EGO`, `TUIC_LIVE_BUDGET_FILE`, `OPENROUTER_API_KEY` | Plan and story delivery with real `ego run` as implementer and reviewer; the commit holds `delivered.txt`. 2 model runs. |
| GitHub issue/PR trigger | `GITHUB_E2E_TOKEN` | Runs the existing `parity_github_live` test (sandbox `sstraus/tuic-e2e-sandbox` only): 1 issue created, admitted, run to completion, then closed; the open PR is replayed. The script reports `unavailable` without the token. |
| `live_jira_…`, `live_gitlab_…`, `live_telegram_…` | `JIRA_LIVE_SITE/EMAIL/TOKEN/PROJECT`, `GITLAB_LIVE_HOST/TOKEN/PROJECT`, `TELEGRAM_LIVE_BOT_TOKEN/CHAT_ID` | None. Without all variables: `unavailable: missing …`. With them: one read-only account probe, `probe_only`. The suite has no write authority for these systems. |

The model default is `openrouter/anthropic/claude-haiku-5.5` (`TUIC_LIVE_MODEL`
overrides it).

## Cost guard

The suite may spend 3 real model runs in total. `TUIC_LIVE_BUDGET_FILE` (default
`<checkout>/.tmp/wf-live/model-runs.count`; point it at one shared file to count across checkouts) counts runs across processes. Test A spends
one before its model call. Test C wraps both agent commands in a counter that
refuses a run above the budget, so a rejected review cannot start a fourth run.
A failed run still counts. Delete the file only with new authority.

## Limits

- Test C uses the plan and story delivery workflow with `ego run`, not ACP: the
  converted `resolve_plan` template and its ACP harness were deleted with the
  legacy engine.
- The script runner sanitizes the child environment. If `ego run` does not see
  its provider key there, test C fails with the agent's error; that is a real
  finding about the delivery workflow, not a test defect.
