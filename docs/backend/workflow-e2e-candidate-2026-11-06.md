# Workflow e2e on the 2026-11-06 candidate — 2707-619d

This supplements [workflow-e2e-main-pin.md](workflow-e2e-main-pin.md). It does
not erase its trials.

## Pin and build

Application pin: `049393bfdc2ac7af5f44766fb0cf02da00fe593d` (main `c40e8c4c3e`
plus the story start commit). It contains the fixes for 2771-4a2c (automatic
closure actor) and 2772-3526 (PTY close after restart). No `v2.0.1` tag exists.

The native binary was built with `make agent-test` through the mbx shim,
instance `agent-test-2707`, SHA-256
`a3c76a3fdf77cbd12fa962f3c2797e89e0f7f68ba07217f8002560b55ddbff8a`.
Models: ego `openai-codex/gpt-6-luna`, reviewer Claude `haiku` (Boss
2026-11-04: cheap models only). Budget: four hours (`budget-4.json`).
All trial paths are relative to `/Users/stefano.straus/Gits/.tmp/workflow-e2e-2707`
(`batch-4.log`, `batch-4-results.json`, `launch-plan-4.json`).

## Results

| Trial | Variant | Result | Cause |
|---|---|---|---|
| E01-live-12 | automatic closure | FAIL | Product: 2883-c897 |
| E20-live-9 | | FAIL | Product: 2883-c897 |
| E01-live-13 | human closure | FAIL | Product: 2883-c897 |

The batch was stopped after these three cases. The remaining local cases (E02,
E03 both timings, E05, E07, E22, E24) use the same report path, so they would
fail in the same way. Every case has an owned stop proof with no surviving
process (`<trial>-daemon-stops.json`).

An earlier E01-live-12 attempt did not start: `scripts/workflow-e2e.sh` was
committed with mode 100644. Commit `c200b1eeed` sets mode 100755.

## The ego implementer cannot report

In each trial the ego attempt wrote and committed its file in less than one
minute (for example E20-live-9 `4246333`, E01-live-12 `238e2c1`). It then never
sent its typed report, and the run reached its own limit: E01 paused at
`maxDurationSecs` 600, and E20 never reached its question.

The ego transcripts (`subscription-sessions/<session>/events.jsonl`) show the
cause. Every `search_tools`, `get_tool_schema` and `call_tool` for `workflow`
failed in ego with:

```
MCP execution failed: invalid MCP candidate: invalid collapsed inner tool schema: null is not of type "object"
```

A probe on the live candidate daemon, registered as orchestrator so that
`workflow` is listed, returned `"$defs": null` in the advertised `workflow`
`inputSchema` from both `get_tool_schema` and the `search_tools` `top_schema`.

Cause, from source: `advertise_schema` (`mcp_input_contract.rs`) replaces
`workflow.inputSchema.properties.input` with a plain object since
`aa66d78e9a` (2849-2441). `prune_unreferenced_defs` then removes every
definition and the `$defs` key. The type-notes pass that follows reads
`tool["inputSchema"]["$defs"].as_object_mut()`; serde_json `IndexMut` inserts
`Null` for a missing key. The 2026-11-03 pin predates `aa66d78e9a`, which is why
the same harness and models passed there.

## Rerun on the 2883-c897 candidate

Application pin: `853aa82e8e56856a6a84752d21ed4e64b49784c7` (this branch
merged with main `03e37118bc`, which contains the 2883-c897 fix). Native
binary, same instance and models as above, SHA-256
`f106e1a5f8ac704ee1f28946bc4aa129fd1abdf39cf98bc35532ec8182011ba3`.
Budget: four hours (`budget-5.json`). Trial artifacts: `batch-5.log`,
`batch-5b.log`, `batch-5-results.json`, `launch-plan-5.json`.

| Trial | Variant | Result | Cause |
|---|---|---|---|
| E01-live-14 | automatic closure | FAIL | Harness: stale reviewer bridge path |
| E20-live-10 | | PASS | |
| E01-live-15 | human closure | PASS | |
| E02-live-10 | | PASS | |
| E03-live-13 | | PASS | |
| E03-live-14 | answer after ACP idle | PASS | |
| E05-live-14 | | PASS | |
| E07-live-11 | | PASS | |
| E22-live-4 | | PASS | |
| E24-live-9 | | PASS | |
| E01-live-16 | automatic closure | PASS | |

Each PASS covers the implemented assertions only; the family status stays
INCOMPLETE. Every case has an owned stop proof with no surviving process.

The ego implementer now reports: in E01-live-14 the first attempt reached
`reported` with outcome `completed`. No ego transcript of this round contains
the `null is not of type "object"` error.

E01-live-14 failed in the harness, not the product. The reviewer runtime
(`team-runtime.py`, outside the repository) started `tuic-bridge` from the
removed worktree `test-2707-workflow-e2e`, so the reviewer exited at once and
the run paused. Round 4 never reached the reviewer, so it did not show this.
After the path was set to this worktree's bridge, E01-live-16 passed.

## GitHub and Fabbrica

On 2026-11-06 `GITHUB_E2E_TOKEN` still authenticates (`GET user` 200), and
`GET repos/sstraus/tuic-e2e-sandbox` still returns 404. E10–E14, E26 and the
Fabbrica cases E28–E35 stay blocked on the token's repository access.

No preservation row is promoted.
