# Extended local workflow evidence — 2707-619d

This is the second, coordinator-authorized 120-minute extension, granted at
1793633291242 and ending at 1793640491242. The application pin remains
`ee8908602ea1d4fb6c19c204217495965aa5edfe`; native binary SHA-256 remains
`e1336a56e1e95c98e58e6c11c91f0331f505a70f03481efec8ddf510d917c89e`.
`git merge-base --is-ancestor b442c5282e ee8908602e` exits 0, recorded in
candidate-2709-ancestry.json: the 2709 owned-close fix is present.

All trial paths below are relative to
`/Users/stefano.straus/Gits/.tmp/workflow-e2e-2707`.
Every case retained the actual HTTP exchanges, immutable harness hashes, exact
command, application provenance, observed agent executable hash and result.
Only implemented assertions are described; all 147 preservation rows remain in
the approved inventory, and no complete row, family or release is promoted.

## Results that matter

| Case / variant | Trial | Assertions proved | Overall result |
|---|---|---|---|
| E05 clean integration plus dirty refusal | E05-live-10 | Real author commit; truthful explicit submission; independent typed approval; real checks; actual two-parent canonical merge; exact retry receipt and unchanged HEAD; unrelated committed sentinel retained; obligation published; owned agent close and daemon stop | PASS for implemented assertions, family INCOMPLETE |
| E02 rejection and repair | E02-live-7 | Actual wrong bytes committed; independent changes_requested; generation 2 committed correct bytes; second independent approved review; deterministic bytes check; accepted revision | FAIL at owned close: foreign test daemon present |
| E03 immediate answer | E03-live-7 | Real question and independent sibling; stale revision rejected; valid answer accepted once; exact same-command reply replayed | FAIL: continuation stayed pending because answer preceded ACP quiescence; tracked 2723-2153 |
| E03 after ACP idle | E03-live-9 | Actual ACP quiescence observed; real blue-answer continuation completed; unchanged seed bytes; independent sibling; stale refusal and same-command replay | FAIL at owned close: foreign test daemon present |
| E20 question deadline | E20-live-6 | Both real branches reached setup; revision 2 deadline armed; real daemon stopped/restarted; absolute deadline retained; exactly one timer fired; no completed question branch, implicit acceptance or integration | FAIL at historical ACP close; tracked 2720-89e2 |

E03's immediate variant remains a failed target. `experiment.answerTiming`
defaults to `immediate`; the separate `after_acp_idle` variant explicitly waits
for the actual attachment to be idle, its current turn settled, empty prompt
queue and no permission/elicitation seats. It does not cover or replace 2723.
Unknown timing variants fail admission. E03-live-8 used the same observable idle
setup before the manifest field was introduced; E03-live-9 records it explicitly.

Key proof files: E05-live-10/integration-observation.json,
E02-live-7/delivery-observation.json, E03-live-7/answer-idempotency-observation.json,
E03-live-9/question-acp-idle.json, E03-live-9/question-observation.json,
E20-live-6/question-observation.json. Read each result.json alongside those
subcase artifacts: a behavioral assertion artifact never overrides a cleanup FAIL.

## Setup and harness corrections retained as failed trials

| Trial | Observation and correction |
|---|---|
| E05-live-5 | Strict isolated MCP configuration exposed workflow tools and the real reviewer submitted typed approved. Judge correctly refused missing story submission. Added an explicit operator Pause before review; inspect actual bytes and submit only correct artifacts. |
| E05-live-6 | Real publication refused unreachable nodes. Added static conditional reachability for the Pause/resume route, following E07's proven graph shape. No worker launched. |
| E05-live-7 | Graph, independent review and checks completed; story already done. Removed a duplicate approve request. |
| E05-live-8 | Integration correctly refused dirty canonical metadata/sentinel before changing HEAD. Fixture now ignores .mdkb metadata from the seed commit. Test first asserts dirty refusal preserves HEAD/sentinel, then commits only its owned sentinel before clean integration. |
| E05-live-9 | All integration assertions passed; owned close refused live isolated test app. Historical counted PIDs were not captured, so no foreign-process or regression attribution is made for this trial. |
| E02-live-4 | Real wrong commit rejected and repair activated, but adaptive sandbox could not read inherited signing key. Disposable repository config now disables signing; real account Git configuration is unchanged. |
| E02-live-5 | Author claimed completed without file or commit. The harness's actual file oracle failed; see product observation below. |
| E02-live-6 | Actual wrong commit existed, but model returned failed instead of completed-attempt/unsatisfied-criterion, so graph stopped. Prompt now distinguishes attempt completion from criterion correctness. |
| E03-live-5 / E20-live-4 | Real question and independent reviewer reached setup. Command preflight returned its explicit retry response. Added at most five retries with fresh workflow sequence, unchanged question revision/subject, and the same stable command ID. |
| E03-live-6 | ego ACP session did not advertise requested effort configuration; launch failed closed before seed artifact. No model/effort policy weakening. |
| E20-live-5 | Real deadline revision 2 survived restart; harness selected superseded revision 1 by shared ID. Corrected oracle to highest question revision. No expiry claim for this failed trial. |
| E03-live-8 | Idle-answer continuation and idempotency passed; cleanup cancellation hit explicit preflight retry. Shared the same bounded preflight retry with cleanup, then reran E03-live-9. |

Cleanup retry handles only the precise command-preflight retry response; stale
question revisions, uncertain transport outcomes and close refusals are not
silently retried as successful actions. Same-command answer replay uses the exact
successful payload, including its original sequence.

## Product observation: completed claim without a new commit

E02-live-5's author used ego over ACP and submitted a typed completed report while
its actual branch remained at seed commit `b7f38e3` and result.txt did not exist.
The report itself said criterion 0 was unsatisfied. TUIC accepted the report,
marked the author attempt reported/completed and advanced to the published
harness submission Pause. The run snapshot retained accepted=false. The harness
failed on the missing real file before submitting review. No reviewer ran, no
story approval or integration was observed, and no TUIC artifact-refusal check
fired at report ingestion. This proves report acceptance, not integration of an
unchanged branch. The explicit unsatisfied criterion was retained in the report;
it must not be described as an automatic artifact verification.

E03-live-6's missing-effort session was **ego 0.4.3**, provider/model
**openai-codex/gpt-6-luna**, requested **medium**. Executable SHA-256 was
`2e4ac9b8da009d3fc9e6e776b068ece0c6641edb42ad055189884d6edf9d7b07`.
Later successful setup trials observed ego 0.4.3 with SHA-256
`cc308323ca603643756c392f6f363403665ecdde4ddf8932337467b3110936c2`.
These are separately recorded executable observations; version equality is not
used to transfer passing evidence between those bytes. Team reviewer was 2.1.316;
Python 3.9.6 and Git 2.54.0 (Apple Git-157). Exact versions/hashes remain per trial.

## Isolated MCP and process evidence

The reviewer uses a dedicated HOME/config under the evidence root, an explicit
strict MCP configuration, and this worktree's native tuic-bridge with TUIC_SOCKET
set to the exact agent-test-2707 Unix socket. There is no ambient/default daemon
fallback. `reviewer-tools-1793633481589520000.json` records an actual stdio
initialize/tools-list from the bound reviewer environment; workflow is present.
Actual subsequent typed reports prove the CLI reached that tool. Provider auth
values are process-only and absent from these manifests and reports.

Before close, the harness records PID/PPID/start/comm for test executables only.
E05-live-10 saw only its own PID 89325 and passed. E02-live-7 saw foreign PID 15551
(agent-test-2517, started 16:18:28 UTC) and own PID 21502. E03-live-9 saw the same
foreign PID 15551 and own PID 47813. The preserved close-test-app-inventory.json
files explain those later guard refusals with the fix present. No signal was sent
to the foreign instance. Historical E05-live-9 counted PIDs remain unknown.

Every isolated runner stopped its own daemon in finally, verified recorded owned
children absent, and retained stop receipts. A bridge gets a bounded ten-second
shutdown grace after the native daemon exits; surviving owned identities still
fail the run. Retained repositories, run-store records and a stale socket are
not running agents. No installed binary, daily daemon or real account config was
replaced. No provider-write, Fabbrica, E22 or automatic-closure rerun was added.

Full-story criteria 2–6 remain unfulfilled. The remaining work includes 2723,
2720, exclusive-instance cleanup evidence for affected variants, all unexecuted
preservation rows and the still-held provider/Fabbrica grants. See the original
and refreshed ledgers for earlier trials; this extension does not erase them.
