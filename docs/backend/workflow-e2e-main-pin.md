# Workflow e2e on the main pin — 2707-619d

This supplements the [original ledger](workflow-e2e-runs.md), the
[refreshed ledger](workflow-e2e-reruns.md) and the
[extension](workflow-e2e-extension.md). It does not erase their trials.

## Pin and build

Application pin: `05632abeb5c438d4dcb19168110ef02adc98630e` (main on 2026-11-03).
No `v2.0.1` tag exists. Main is the release line, and it contains the fixes for
2716-944f (automatic-closure evidence gate), 2720-89e2 (close after restart),
2723-2153 (early answer), 2725-539b (completed report artifacts) and
2743-9a94 (workflow schema budget). The earlier pin `ee8908602e` contains none
of them, so a rerun on that pin would only repeat known failures.

The worktree merged main at this pin; later edits touch only the harness and
documentation. The native binary was built with `make agent-test`
(build-slot and mbx), SHA-256
`ecead9c13687f743fecc327439d365dccf9b4325dbe1af2dd0281967e1e539d9`.
Build evidence: `candidate-refresh-3.log`, `candidate-refresh-3-health.json`.
Budget: three hours from 2026-11-03 under the parent brief (`budget-3.json`).

All trial paths are relative to `/Users/stefano.straus/Gits/.tmp/workflow-e2e-2707`.

## New variant: unattended automatic closure (E01)

`experiment.closure=automatic` selects `lifecycle.automatic_case`. The graph is
implement, independent review, Gate, Judge, with an evidence policy and a
deterministic bytes check. The harness makes no story transition and runs no
check: the daemon runs the check at the Gate, and the Judge closes the story.
Oracle: story `done`, run accepted at the current story revision, committed
`result.txt` bytes, a passing check receipt for HEAD, disjoint author and
reviewer sessions, and only `managed_session` actors in the native transition
history.

## Results

Serial batch on 2026-11-03, one isolated daemon per case (`batch-3.log`,
`batch-3-results.json`). PASS means the implemented assertions of that case, not
the complete family ledger.

| Trial | Variant | Result | Cause |
|---|---|---|---|
| E24-live-8 | QA | PASS | |
| E07-live-10 | adaptive | PASS | |
| E22-live-3 | workspace, restart | PASS | Close after restart works (2720-89e2) |
| E01-live-9 | human closure | PASS | |
| E02-live-9 | adaptive | PASS | |
| E03-live-11 | immediate answer | PASS | |
| E03-live-12 | `after_acp_idle` answer | PASS | Early answer settles (2723-2153) |
| E01-live-11 | automatic closure | FAIL | Product: 2771-4a2c |
| E20-live-7 | deadline, restart | FAIL at close | Product: 2772-3526 |
| E05-live-11 | | FAIL at close | Harness: close raced the PTY turn; fixed in `dd80351813` |
| E05-live-12 | | FAIL | ego offered no `effort` option inside the 30 s config budget |
| E03-live-10 | | FAIL at cleanup | Harness: cancel raced the live run; fixed in `e22fd7dfbf` |
| E01-live-10, E02-live-8 | | FAIL | External: the test window was closed and the app quit |

**E01-live-11 (automatic closure).** The implementer and the reviewer reported
completed, and the Gate passed the daemon check. The Judge then answered
uncertain, "Story must be submitted for independent review"
(`runtime/judge.rs`), and the run paused at hold. The story stayed in progress
with its criterion unchecked. In production code, no run step checks criteria
or submits the story: every such call is a test fixture or the operator story
API. So an unattended automatic run cannot close a story.

**E20-live-7.** The PTY `work` attempt started on the first daemon. After the
scripted restart, owned close refused it with "owned process inventory unknown"
(`pty_refusal`), although the stop record shows no owned PID alive. The ACP
attempts of the same run closed, so 2720-89e2 holds for ACP only.

**E05-live-12.** ACP attach at 13:19:14.93, pause at 13:19:56.33. The ego
session published no `effort` option before the 30 s budget in
`launch_transaction_acp_readiness.rs` ran out. E03-live-6 failed the same way.
The logs do not show whether ego was slow or never published the option, so no
fix story was opened.

**E01-live-10 and E02-live-8.** The unified log shows a mouse-up on the test
app's window while another app was in front, `windowShouldClose: prevented
close` (close behaviour `ask`), and process exit 1.5 s later. A person or an
automation closed the visible test window. The product did not quit by itself.

**Reruns.** E20-live-8 failed the same way as E20-live-7: both ACP attempts
closed, and the PTY attempt was refused with "owned process inventory unknown".
The defect is deterministic. E05-live-13 passed, close included, so the
`effort` failure of E05-live-12 is intermittent.

Local summary: all eight local families ran. E02, E03 (both answer timings),
E05, E07, E22, E24 and E01 with human closure pass. E01 with automatic closure
and E20 fail on the product defects 2771-4a2c and 2772-3526.

## Close guard and foreign test daemons

`background_refusal` (`src-tauri/src/mcp_http/launch_transaction_close.rs`)
reads the whole process table. Any `tuicommander-agent-test` process except the
serving daemon refuses an owned close. E02-live-7 and E03-live-9 failed only on
this, because of another agent's instance. `batch-3.py` now waits up to 30
minutes for no foreign test daemon before each case, and records the inventory
it saw. This guard is outside the scope of this story.

## GitHub and Fabbrica

On 2026-11-03 the `GITHUB_E2E_TOKEN` authenticated (`GET user` 200), but
`GET repos/sstraus/tuic-e2e-sandbox` returned 404. E13, E14 and the Fabbrica
cases E28/E32 stay blocked on the token's repository access.
