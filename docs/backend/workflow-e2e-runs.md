# Workflow e2e execution ledger — 2707-619d

Candidate application commit: `c7230e29a371ff10b311ec50f555bd72dfed9b8b`.
The application source must match this pin; harness changes have their own hashes.
No full-family, preservation-row or release PASS is currently claimed.
The [refreshed candidate ledger](workflow-e2e-reruns.md) records later runs and supersedes this candidate pin only for those attempts.
The [main-pin ledger](workflow-e2e-main-pin.md) records the 2026-11-03 runs on main `05632abeb5`.

## Authorized chunk

Boss set a three-hour cap on 2026-11-02. Chunk 1 is E01, E02, E03, E05,
E07, E20, E22, E24 plus E13, E14, E28 and E32. Runs are serial. The private
sandbox is `sstraus/tuic-e2e-sandbox`; no other provider repository is authorized.
Provider writes await its scoped token. Fabbrica awaits its non-production
App installation and worker model-credential path. Real isolated C1 preparation succeeded on rb; no worker, model or GitHub operation was started.

Subsequent workers use `openai-codex/gpt-6-luna` via ego and a separate
Team-account reviewer using a base model. The earlier E24 and E07 attempts
precede that change. Ego sandbox/mode variants are recorded explicitly; a
failed variant cannot be counted as passing a different variant.

## Current observations

Evidence root: `/Users/stefano.straus/Gits/.tmp/workflow-e2e-2707`.
These local files are retained trial evidence; they are not published artifacts.

| Attempt | Outcome | Proven scope / limitation |
|---|---|---|
| preflight-1 | PASS | Original isolated socket/binary provenance only; no workflow coverage |
| E24-setup-1 / E24-setup-2 | Harness setup failures | Corrected story envelope and separate-plan sibling setup |
| E24-live-1 | Harness setup failure | Undefined ego profile; corrected isolated profile |
| E24-live-2 | Incomplete | Real typed needs_input: workspace confinement denied xcrun cache/Git access; not model failure |
| E24-live-3 | Harness oracle failure | Real ego artifact and completed graph; waiting for certified run completion was wrong for this subcase |
| E24-live-4 | FAIL | Manual QA had zero agents/worktrees and approved to done; separate real sibling committed. Owned close refused because host test app appears in global process inventory (fix story 2709) |
| E03-live-1 | Harness setup failure | Publication correctly refused implementer agents inside a read-only parallel region; changed to seeded independent read-only reviewers |
| E07-live-1 | Harness setup failure | Static Call reachability was missing; corrected with typed conditional route |
| E07-live-2 | Harness setup failure | Legacy manifest lacked revised secondary routing; refused before scenario execution |
| E07-live-3 | RUNNING | Cheap subscription model, adaptive/yolo, exact reused binary |

The E24 cleanup failure expected `state=closed` with observed process absence;
actual `owned-close.json` contains `state=refused`, reason `live isolated test
app prevents owned close`, and no stop receipt. It did **not** expect manual QA
to spawn an agent. The coordinator owns the production fix; the harness refuses
PASS when cleanup remains unproven. Later isolated runners stop their own test
daemon in `finally` and retain its exit result.

## Non-production Fabbrica preparation

`fabbrica-preparation-2.log` records successful real C1 preparation on rb through
build-slot (handle `20261102T113241Z-93191`). State is retained at
`/home/rb/Gits/.tmp/workflow-e2e-2707-fabbrica-1793619176`: fresh private test CA,
loopback server certificate, actual panel registration state, consumed one-use
enrollment and the node's atomic identity. Panel and node processes were stopped
and their exit receipts retained. Private keys and token values are excluded
from reports. This is setup evidence only; E28/E32 remain blocked on the sandbox
App installation and the worker subscription-token adapter (ego 818-e820).
The first preparation attempt stopped before remote execution because the
Mac PATH selected openrsync; the retry selected installed rsync 3.5.1.

## Running the harness

Use a linked worktree and a named `make agent-test` instance. Obtain the native
launch manifest from its dry run, then add the pinned `candidate`, absolute
`ego` and `egoRuntime` executables, `egoModel`, `secondary` executable,
`secondaryAgent`, `secondaryProfile`, `secondaryModel`, `secondaryArgs`,
`secondaryEnv`, and an `experiment` object naming sandbox/mode/account policy.
Set `budgetExpiresMs` to the approved absolute deadline; an absent or exhausted budget refuses execution. Do not put credentials in this JSON. The runtime may use the already configured
provider's supported login store; do not inspect or copy that store.

```
scripts/workflow-e2e.sh --candidate <full-commit> --manifest <manifest.json> \
  --evidence <fresh-directory-under-Gits> --case E24 --preflight-only
scripts/workflow-e2e.sh --candidate <full-commit> --manifest <manifest.json> \
  --evidence <another-fresh-directory> --case E24
scripts/with-test-tmp.sh python3 -B scripts/workflow-e2e/preflight_tests.py
```

Preflight rejects a mismatched candidate, daily namespace, mismatched socket,
binary outside this worktree's instance, changed application source, reused or
out-of-Gits evidence directory, and a running binary/hash mismatch. Publication
and start-readiness validate the actual declared graph and exact agent routes.
Missing providers/permissions remain failed or incomplete live cases.

The current selectors implement bounded **subcases**. E01 currently exercises
one independently reviewed story, not the full multi-story dependent portfolio;
E02 exercises a repair, without the complete stale/parallel-check matrix;
E03 uses two read-only review branches after a real seed author; E05 covers a
real merge and identical-request retry, without its restart/moved-base matrix;
E07 covers a pinned child update and root retry; E20 covers a real question
deadline, without its restart/service/retry matrix; E22 covers client reconnect
and duplicate start, without process-crash boundaries; E24 covers native QA and
independent delivery, without all dependency/rejection/restart variants.
Their result records explicitly retain `familyStatus=INCOMPLETE`.

Installed TUIC source also establishes two specific missing adapters: plan
runtime `runtime/plan.rs` explicitly waits for operator integration after a
story is approved, so an unattended E01 delivery cannot currently complete;
`NodeKind` has no published Timer node and public commands reject internal
Timer primitives, so E20's generic Hold:Timer row cannot be replaced by a
question-deadline success. These remain required, independently visible gaps.

## Preservation rows

Each approved row remains separately tracked below. A successful subcase does
not silently promote a broad family or its additional preservation oracles.

| Case | Status | Evidence / missing proof |
|---|---|---|
| E16/A01 | NOT_RUN | No complete compatible row evidence |
| E16/A02 | NOT_RUN | No complete compatible row evidence |
| E04/A03 | NOT_RUN | No complete compatible row evidence |
| E16/A04 | NOT_RUN | No complete compatible row evidence |
| E16/A05 | NOT_RUN | No complete compatible row evidence |
| E16/A06 | NOT_RUN | No complete compatible row evidence |
| E16/A07 | NOT_RUN | No complete compatible row evidence |
| E16/A08 | NOT_RUN | No complete compatible row evidence |
| E09/A09 | NOT_RUN | No complete compatible row evidence |
| E16/A10 | NOT_RUN | No complete compatible row evidence |
| E02/A11 | NOT_RUN | No complete compatible row evidence |
| E09/A12 | NOT_RUN | No complete compatible row evidence |
| E08/E01 | NOT_RUN | No complete compatible row evidence |
| E08/E02 | NOT_RUN | No complete compatible row evidence |
| E08/E03 | NOT_RUN | No complete compatible row evidence |
| E08/E04 | NOT_RUN | No complete compatible row evidence |
| E08/E05 | NOT_RUN | No complete compatible row evidence |
| E08/E06 | NOT_RUN | No complete compatible row evidence |
| E16/E07 | NOT_RUN | No complete compatible row evidence |
| E08/E08 | NOT_RUN | No complete compatible row evidence |
| E08/E09 | NOT_RUN | No complete compatible row evidence |
| E16/E10 | NOT_RUN | No complete compatible row evidence |
| E04/E11 | NOT_RUN | No complete compatible row evidence |
| E06/E12 | NOT_RUN | No complete compatible row evidence |
| E16/E13 | NOT_RUN | No complete compatible row evidence |
| E05/E14 | NOT_RUN | No complete compatible row evidence |
| E08/E15 | NOT_RUN | No complete compatible row evidence |
| E16/S01 | NOT_RUN | No complete compatible row evidence |
| E18/S02 | NOT_RUN | No complete compatible row evidence |
| E08/S03 | NOT_RUN | No complete compatible row evidence |
| E23/S04 | NOT_RUN | No complete compatible row evidence |
| E06/S05 | NOT_RUN | No complete compatible row evidence |
| E08/S06 | NOT_RUN | No complete compatible row evidence |
| E10/S07 | NOT_RUN | No complete compatible row evidence |
| E16/Q01 | NOT_RUN | No complete compatible row evidence |
| E23/Q02 | NOT_RUN | No complete compatible row evidence |
| E22/Q03 | NOT_RUN | No complete compatible row evidence |
| E04/Q04 | NOT_RUN | No complete compatible row evidence |
| E23/Q05 | NOT_RUN | No complete compatible row evidence |
| E06/Q06 | NOT_RUN | No complete compatible row evidence |
| E06/Q07 | NOT_RUN | No complete compatible row evidence |
| E06/Q08 | NOT_RUN | No complete compatible row evidence |
| E04/Q09 | NOT_RUN | No complete compatible row evidence |
| E06/Q10 | NOT_RUN | No complete compatible row evidence |
| E06/Q11 | NOT_RUN | No complete compatible row evidence |
| E06/Q12 | NOT_RUN | No complete compatible row evidence |
| E05/L01 | NOT_RUN | No complete compatible row evidence |
| E04/L02 | NOT_RUN | No complete compatible row evidence |
| E06/L03 | NOT_RUN | No complete compatible row evidence |
| E05/L04 | NOT_RUN | No complete compatible row evidence |
| E16/L05 | NOT_RUN | No complete compatible row evidence |
| E05/L06 | NOT_RUN | No complete compatible row evidence |
| E08/L07 | NOT_RUN | No complete compatible row evidence |
| E05/L08 | NOT_RUN | No complete compatible row evidence |
| E05/L09 | NOT_RUN | No complete compatible row evidence |
| E05/L10 | NOT_RUN | No complete compatible row evidence |
| E16/L11 | NOT_RUN | No complete compatible row evidence |
| E10/L12 | NOT_RUN | No complete compatible row evidence |
| E15/L13 | NOT_RUN | No complete compatible row evidence |
| E15/L14 | NOT_RUN | No complete compatible row evidence |
| E04/B01 | NOT_RUN | No complete compatible row evidence |
| E04/B02 | NOT_RUN | No complete compatible row evidence |
| E04/B03 | NOT_RUN | No complete compatible row evidence |
| E04/B04 | NOT_RUN | No complete compatible row evidence |
| E04/B05 | NOT_RUN | No complete compatible row evidence |
| E16/B06 | NOT_RUN | No complete compatible row evidence |
| E04/B07 | NOT_RUN | No complete compatible row evidence |
| E04/R01 | NOT_RUN | No complete compatible row evidence |
| E04/R02 | NOT_RUN | No complete compatible row evidence |
| E04/R03 | NOT_RUN | No complete compatible row evidence |
| E04/R04 | NOT_RUN | No complete compatible row evidence |
| E04/R05 | NOT_RUN | No complete compatible row evidence |
| E16/R06 | NOT_RUN | No complete compatible row evidence |
| E16/R07 | NOT_RUN | No complete compatible row evidence |
| E16/R08 | NOT_RUN | No complete compatible row evidence |
| E03/C01 | NOT_RUN | No complete compatible row evidence |
| E09/C02 | NOT_RUN | No complete compatible row evidence |
| E15/C03 | NOT_RUN | No complete compatible row evidence |
| E15/C04 | NOT_RUN | No complete compatible row evidence |
| E18/C05 | NOT_RUN | No complete compatible row evidence |
| E18/C06 | NOT_RUN | No complete compatible row evidence |
| E09/C07 | NOT_RUN | No complete compatible row evidence |
| E09/C08 | NOT_RUN | No complete compatible row evidence |
| E23/C09 | NOT_RUN | No complete compatible row evidence |
| E09/H01 | NOT_RUN | No complete compatible row evidence |
| E17/H02 | NOT_RUN | No complete compatible row evidence |
| E17/H03 | NOT_RUN | No complete compatible row evidence |
| E17/H04 | NOT_RUN | No complete compatible row evidence |
| E16/H05 | NOT_RUN | No complete compatible row evidence |
| E17/H06 | NOT_RUN | No complete compatible row evidence |
| E16/H07 | NOT_RUN | No complete compatible row evidence |
| E17/H08 | NOT_RUN | No complete compatible row evidence |
| E16/H09 | NOT_RUN | No complete compatible row evidence |
| E16/M01 | NOT_RUN | No complete compatible row evidence |
| E16/M02 | NOT_RUN | No complete compatible row evidence |
| E16/M03 | NOT_RUN | No complete compatible row evidence |
| E22/M04 | NOT_RUN | No complete compatible row evidence |
| E16/M05 | NOT_RUN | No complete compatible row evidence |
| E08/G01 | NOT_RUN | No complete compatible row evidence |
| E16/G02 | NOT_RUN | No complete compatible row evidence |
| E03/G03 | NOT_RUN | No complete compatible row evidence |
| E04/G04 | NOT_RUN | No complete compatible row evidence |
| E23/G05 | NOT_RUN | No complete compatible row evidence |
| E06/G06 | NOT_RUN | No complete compatible row evidence |
| E15/G07 | NOT_RUN | No complete compatible row evidence |
| E22/F01 | NOT_RUN | No complete compatible row evidence |
| E02/F02 | NOT_RUN | No complete compatible row evidence |
| E16/F03 | NOT_RUN | No complete compatible row evidence |
| E04/F04 | NOT_RUN | No complete compatible row evidence |
| E03/F05 | NOT_RUN | No complete compatible row evidence |
| E23/F06 | NOT_RUN | No complete compatible row evidence |
| E04/F07 | NOT_RUN | No complete compatible row evidence |
| E23/F08 | NOT_RUN | No complete compatible row evidence |
| E02/F09 | NOT_RUN | No complete compatible row evidence |
| E04/F10 | NOT_RUN | No complete compatible row evidence |
| E16/F11 | NOT_RUN | No complete compatible row evidence |
| E16/F12 | NOT_RUN | No complete compatible row evidence |
| E16/F13 | NOT_RUN | No complete compatible row evidence |
| E02/F14 | NOT_RUN | No complete compatible row evidence |
| E16/F15 | NOT_RUN | No complete compatible row evidence |
| E16/F16 | NOT_RUN | No complete compatible row evidence |
| E16/F17 | NOT_RUN | No complete compatible row evidence |
| E16/F18 | NOT_RUN | No complete compatible row evidence |
| E16/F19 | NOT_RUN | No complete compatible row evidence |
| E16/F20 | NOT_RUN | No complete compatible row evidence |
| E04/F21 | NOT_RUN | No complete compatible row evidence |
| E04/F22 | NOT_RUN | No complete compatible row evidence |
| E16/F23 | NOT_RUN | No complete compatible row evidence |
| E23/F24 | NOT_RUN | No complete compatible row evidence |
| E22/F25 | NOT_RUN | No complete compatible row evidence |
| E22/F26 | NOT_RUN | No complete compatible row evidence |
| E02/F27 | NOT_RUN | No complete compatible row evidence |
| E02/F28 | NOT_RUN | No complete compatible row evidence |
| E07/F29 | NOT_RUN | No complete compatible row evidence |
| E23/F30 | NOT_RUN | No complete compatible row evidence |
| E09/F31 | NOT_RUN | No complete compatible row evidence |
| E03/F32 | NOT_RUN | No complete compatible row evidence |
| E03/F33 | NOT_RUN | No complete compatible row evidence |
| E03/F34 | NOT_RUN | No complete compatible row evidence |
| E20/F35 | BLOCKED | No published Timer node; question deadlines cover a different bounded subcase |
| E22/F36 | NOT_RUN | No complete compatible row evidence |
| E03/F37 | NOT_RUN | No complete compatible row evidence |
| E22/F38 | NOT_RUN | No complete compatible row evidence |
| E09/F39 | NOT_RUN | No complete compatible row evidence |
| E09/F40 | NOT_RUN | No complete compatible row evidence |
| E09/F41 | NOT_RUN | No complete compatible row evidence |
| E04/F42 | NOT_RUN | No complete compatible row evidence |

E07-live-3 stopped at reused-binary admission (stale SHA); no scenario ran. E07-live-4 verified reused SHA from E07-live-2 health, then stopped at harness configuration setup because an unset agent entry was assumed present. Corrected sparse settings initialization; neither attempt is product failure evidence.

E07-live-5: policy admission correctly refused missing explicit PTY effort. Added supported `--effort medium` to the isolated saved launch envelope; no agent or scenario ran.

E07-live-6: real ACP launch paused at initialize after 60s (adaptive/yolo); no agent binding, author file or Call proof. Failure reported; root cause remains unknown. Daemon descendant absence proved in daemon-stops receipt. Corrected pause oracle to require the intended Hold activation.

E24-live-5: off/yolo sibling ACP initialization also timed out60s. Manual-QA zero-spawn boundary held; unrelated sibling assertion failed before author binding. Same symptom as E07; no confinement root cause established.

E07-live-7 repeats startup timeout with recorded ego0.4.3 SHAa620bb2f. Bare/wrapper initialize replay decisively narrowed to external harness argv: global flags before `acp` entered ordinary chat. TUIC launch_spec uses ACP first (src-tauri/src/acp/mod.rs:195). Coordinator independently reproduced and filed the ego CLI defect. Wrapper corrected to supported ACP argv plus EGO_MODEL; mode override omitted because ACP rejects --mode. These runs do not establish a TUIC startup defect.

E07-live-8: corrected ACP initializes73ms, real luna author committed pin.txt and submitted typed completed. Harness receipt predicate on ordinary agent had uncertain evidence and bypassed Hold; both Call and root completed, so pin-update assertion could not run. Corrected setup to a pinned boolean input predicate, and fail fast on unexpected graph completion. No product failure claimed.

E07-live-9: scenario assertions completed (real author commit, updated child publication, old transitive pin retained, same root idempotency key no duplicate). See pin-proof.json. Overall FAIL solely at owned-close refusal, same2709; daemon stop proves recorded descendants absent. This is a subcase result, not whole-family or147-row coverage. Following cases request default mode; ACP CLI cannot apply yolo directly.

E01-live-1: automatic closure publication HTTP500, explicitly unsupported by store.rs:448 until its evidence gate exists. No agents launched. E02/E05 reviewed-delivery setup shares this missing precondition; these families retain BLOCKED setup coverage, no automatic-to-human substitution. Reported to coordinator.

E03-live-2: publication guard rejected an author role accidentally retained inside Fork. Fixed harness roles: seed author before Fork; both branches read-only reviewers. No production defect claim.

E22-live-1: restart recovery and duplicate-start subcase assertions passed; interrupted binding retained without invented acceptance or integration. Both named-daemon stops proved descendant absence. OverallFAIL only known2709 close refusal. See reconnect-observation.json and daemon-stops receipt.

E03-live-4 real typed question existed; Team sibling had no report. E01-live-2 terminal evidence narrowed shared Team startup to initial theme selection, not workflow failure. Harness selects default theme in its own bound PTY and records all output/writes; no authentication automation. Human closure assertions remain separate from2716 automatic block.

## Final bounded local chunk (2026-11-02)

| Family / latest evidence directory | Actual observation | Remaining assertion |
|---|---|---|
| E01 / E01-live-8 | Real author commit/report; Team reviewer awaits workspace trust | Independent review/closure not proved; automatic closure BLOCKED2716-944f |
| E02 / E02-live-1 | Real agents launched; Team workspace trust prevents report | Repair not executed; automatic closure BLOCKED2716-944f |
| E03 / E03-live-4 | Real seed commit and typed question; Team sibling has no report | Answer/replay and sibling independence not proved |
| E05 / E05-live-1 | Real author; Team workspace trust prevents review | Merge/receipt not executed; automatic closure BLOCKED2716-944f |
| E07 / E07-live-9 | Actual author commit, immutable child pin, repeated root key proved | Overall FAIL only2709 owned-close; other variants not run |
| E20 / E20-live-1 | Real agents launched; Team workspace trust prevents setup | Deadline not armed; restart/expiry not executed |
| E22 / E22-live-1 | Actual daemon restart, same run/key, no fabricated acceptance | Overall FAIL only2709 owned-close; other boundaries not run |
| E24 / E24-live-6 | QA zero-spawn/worktree, actual sibling commit, explicit approval proved | Overall FAIL only2709 owned-close; other variants not run |

No preservation row is promoted. E13/E14 and Fabbrica E28/E32 await scoped setup/credentials; C1 enrollment preparation is not worker assignment coverage. No confirmation to replace the candidate was received. All named daemon stop receipts record no surviving owned descendants. Harness guards: preflight3/3, reused-build4/4, existing agent-test9/9. Native live evidence is separate from rb C1 preparation. External runtime wrappers/manifests are operator setup; their hashes and safe launch envelopes accompany evidence, with no portable provider credentials.
