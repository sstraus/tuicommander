# Workflow e2e on the 2.0.1 candidate — 2707-619d

This supplements [workflow-e2e-candidate-2026-11-06.md](workflow-e2e-candidate-2026-11-06.md).
It does not erase its trials.

## Pin and build

Application pin: `18cac596439917e896486acd5f61b2799df3e665` (main with the
2.0.1 version bump and the fix for 2883-c897; the branch adds only the story
start commit). Native binary built with `make agent-test` through build-slot and
the mbx shim, instance `agent-test-2707`, SHA-256
`9a2ca168656cf160817d7b81ea68312442862bd1affd3e1eb0a159a025901fda`.
Models: ego `openai-codex/gpt-6-luna`, reviewer Claude `haiku` (Boss
2026-11-04: cheap models only). Budget: four hours (`budget-6.json`).
Command per trial: `scripts/workflow-e2e.sh --isolated --candidate <pin>
--manifest <input.json> --evidence <dir> --case <family>`, driven serially by
`batch-6.py`. Trial artifacts are under `/Users/stefano.straus/Gits/.tmp/e2e-2707c`
(`batch-6.log`, `batch-6b.log`, `batch-6c.log`, `<trial>/result.json`,
`<trial>-daemon-stops.json`). The reviewer runtime and ego runtime were copied
there with this worktree's `tuic-bridge` path (the 2026-11-06 copy pointed at a
removed worktree).

## Results

| Trial | Variant | Result | Cause |
|---|---|---|---|
| E01-cand-1 | automatic closure | PASS | |
| E01-cand-2 | human closure | PASS | |
| E20-cand-1 | | PASS | |
| E02-cand-1 | | FAIL | Model: the ego author wrote `wrong` without the newline that the harness asserts (`b'wrong\n'`) |
| E03-cand-1 | | FAIL | Environment: another agent's test daemon started during the trial |
| E03-cand-2 | | FAIL | Environment: two foreign test daemons stayed alive |
| E05-cand-1 | | PASS | |
| E07-cand-1 | | FAIL | Environment: a foreign test daemon started during the trial |
| E07-cand-2 | | FAIL | Model: the ego author ran `git commit` in the primary checkout, not in its worktree |
| E22-cand-1 | | PASS | |
| E24-cand-1 | | PASS | |
| E02-cand-2 | | PASS | rerun after E02-cand-1 |
| E03-cand-3 | | PASS | rerun on a quiet machine |
| E03-cand-4 | answer after ACP idle | PASS | rerun on a quiet machine |
| E07-cand-3 | | PASS | rerun after E07-cand-2 |

Final state: E01 (both closures), E02, E03 (both timings), E05, E07, E20, E22
and E24 each have a passing trial on this pin. Each PASS covers the implemented
assertions only; every family status stays INCOMPLETE. Every trial has an owned
stop proof with no surviving process. The first pass took 21:00 to 22:32
(about 1 h 32 min, of which about 55 min waited for foreign test daemons); the
reruns took 22:34 to 23:07.

## Failure diagnosis

- **Foreign test daemon (E03-cand-1/2, E07-cand-1).** `background_refusal` in
  `src-tauri/src/mcp_http/launch_transaction_close.rs` refuses an owned close
  while any process named `tuicommander-agent-test` other than the serving
  daemon exists on the machine. `close-test-app-inventory.json` of each failed
  trial lists a daemon of another worktree (feat-2900, chore-2881). The
  harness waits for such daemons only before a trial starts. The guard is
  deliberate and conservative; no product defect is claimed. A passing
  evidence run needs a machine with no other test daemon.
- **E02-cand-1.** `lifecycle.py` requires the intentionally wrong artifact to
  be exactly `wrong\n`; the model wrote `wrong`. Model variance against a strict
  oracle, not a product fault; the rerun passed.
- **E07-cand-2.** The ego host context lists the primary repository path. The
  model used it as `cwd` for `git add` and `git commit`, so the commits landed on
  `main` of the primary checkout while its worktree stayed on the dispatch
  commit. `completion.rs:31` correctly rejected the report ("no new commit since
  dispatch") and the model asked the operator. The harness correctly stopped on
  the unexpected question. The rerun passed.

## Preservation rows

No row is promoted. A row moves from `required_not_operationally_verified` only
with complete compatible evidence for that row (`workflow-e2e.md`, last
paragraph; `workflow-e2e-runs.md`). These trials implement bounded subcases, so
every family remains INCOMPLETE and no row has its full oracle set.

## GitHub and Fabbrica

Out of scope for 2.0.1 (Boss 2026-11-04, open-asks A6). Criterion 7 of
2707-619d duplicates the rejected criterion 4 and is rejected.
