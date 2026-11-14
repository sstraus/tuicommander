# Refreshed workflow evidence — 2707-619d

This supplements [the original ledger](workflow-e2e-runs.md). It does not replace
any of the 147 approved preservation rows or claim complete family coverage.

## Pinned application and execution

Coordinator-authorized candidate: `ee8908602ea1d4fb6c19c204217495965aa5edfe`.
Candidate tree: `9d3259df88700f61460e8d45a3107af1373bcbd9`.
Native binary SHA-256:
`e1336a56e1e95c98e58e6c11c91f0331f505a70f03481efec8ddf510d917c89e`.
The worktree merged this candidate at `07733351b5a2cdab1cbf11df8d38783076691866`;
subsequent edits affect only the harness and documentation. The native build used
make agent-test and build-slot/mbx, never an installed or daily application.
`candidate-refresh.log` and `candidate-refresh-health.json` retain build evidence.

All paths below are relative to the retained local evidence root
`/Users/stefano.straus/Gits/.tmp/workflow-e2e-2707`.
Each attempt has its own result.json, recorded HTTP operations, provenance.json,
immutable harness file hashes, exact command and observed tool versions. The
isolated runner adds a daemon stop receipt beside each attempt directory.

| Attempt | Result | Scope actually observed |
|---|---|---|
| E22-live-2 | FAIL, blocked by 2720-89e2 | Real daemon restart, same run identity, idempotency and no fabricated acceptance assertions passed. Owned close refused because the historical ACP connection was absent after restart. No overall PASS. |
| E24-live-7 | PASS for implemented subcase; family INCOMPLETE | Manual QA paused with no worker or worktree; separate real sibling committed; explicit operator criterion check and approval reached done; owned close and daemon cleanup passed. |
| E02-live-2 | FAIL, setup/model interpretation | Author treated scoped MCP capabilities as denying filesystem tools. Prompt now explicitly authorizes assigned-worktree writes and commits. No repair assertion passed. |
| E02-live-3 | FAIL, reviewer setup | Real author committed; reviewer stopped at external instruction import approval. No independent review or repair loop asserted. |
| E05-live-2 | FAIL, reviewer setup | Reviewer stopped at external instruction import approval. No integration assertion ran. |
| E05-live-3 | FAIL, reviewer setup | Canonical repository trust/import approval succeeded in isolated configuration; reviewer stopped at bypass warning. No integration assertion ran. |
| E05-live-4 | FAIL, isolated MCP setup | Reviewer started, read committed bytes and verified commit/tree/digest, but could not discover the workflow MCP tool. Its chat approval is not a typed report. Run paused at 600 seconds; no integration assertion ran. |
| E20-live-2 | FAIL, reviewer setup | External instruction import approval blocked the independent reviewer. No timer was armed. |
| E20-live-3 | FAIL, reviewer setup | Real typed question existed, but the independent reviewer did not report. No deadline was armed and no expiry/restart claim is made. |

E01, E03 and E07 were exercised on the earlier candidate only; their results
must not be transferred to this candidate. E07 proved its implemented Call pin
and request-key idempotency assertions before the earlier owned-close failure.
E01/E02/E05 automatic closure variants remain blocked by intentional guard
2716-944f; only separately authorized human-closure variants were attempted.
E22's close failure is tracked separately from the earlier 2709 close defect.

## Reviewer setup and remaining work

The reviewer used the separately authorized Team subscription and base model
with medium effort. Its HOME and config were isolated under the evidence root.
Trust covered both assigned cwd and canonical Git common repository identity.
The installed CLI's user setting `skipDangerousModePermissionPrompt` removed its
bypass onboarding warning only in that isolated home. No Boss account settings
were edited. Authentication values were passed in process environment, never
printed, saved in manifests or copied into this report.

Moving HOME also removed ambient MCP discovery. E05-live-4/http-0272.json records
three unsuccessful tool searches and a proposed report in chat. Fix the isolated
MCP registration against this test instance before another reviewed workflow run;
do not point the reviewer at the daily daemon or count chat as a report. This is
a setup finding, not demonstrated workflow-engine failure. The next run must also
supply the actual typed report schema to the reviewer, which guessed field names.

E05-live-4/owned-close.json proves the actual reviewer process absent. Its adjacent
daemon-stops.json records no surviving owned processes and stopExit 0. Earlier
retained workspaces/run-store records remain evidence; they are not running agents.

The 90-minute extension ends at epoch milliseconds 1793633730087. That chunk stopped after E05-live-4. The coordinator subsequently granted a
[120-minute extension](workflow-e2e-extension.md), whose later evidence supersedes
those setup limitations only for the explicitly rerun variants. Provider-write scenarios still require the
scoped disposable-repository grant. Fabbrica still requires the non-production
App installation and worker subscription adapter. Successful rb CA/enrollment
preparation remains setup evidence only. No release or capability promotion is
justified, and story criteria 2–6 remain unchecked.
