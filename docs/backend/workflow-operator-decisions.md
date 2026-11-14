# Retained operator decisions

`native.operator.wait` version 1 uses the existing owned effect question/answer
ledger. Its typed `request` carries `source_node`, `resume_to`, `artifact`
(`commit`, `tree`), nullable `source_generation`, and a distinct `decision.kind`:

- `resume` continues the exact pinned activity's `resumed` edge.
- `human_closure {story,revision}` applies native story `Approve` with an
  authenticated owner answer and the selected StoryBackend's transition rules.
- `release_consent {release,scope_digest,intent}` records consent in the existing
  release authorization journal for one exact publication intent. The digest
  covers the frozen release scope, rules, revision and release/next/boss_wait
  membership. A changed scope is refused in the authorization transaction.
- `portfolio_release {id,revision}` delegates to the existing portfolio release
  service, retaining its completed owner, integration, proof and connector checks.

An authenticated owner submits the shared neutral action `operator_answer` with
`run_id`, `command_id`, `expected_sequence`, exact effect `key`, question `revision`
and `answer: {request,accept,reason}`. The backend records actor, timestamp,
sequence, publication digest and complete subject atomically with the existing
answer decision. Caller JSON cannot supply that authority receipt. Ordinary
`answer {approved:true}` cannot satisfy an operator wait. A declined typed answer
selects the activity's fixed `declined` edge; neither outcome is a jump command.

The version 1 `native.operator.automatic_close` handler instead accepts
`decision: {kind:"automatic_closure",story,revision,proof:{base}}`. It has no human
question. The host resolves the latest actual native independent review and
verification effects in the same run for the exact current committed artifact.
The review must pass, every captured host check must have a stopped successful
receipt matching the host command, and native story revision/claim/self-review
rules still apply. Model labels, child completion, notifications and timeout do
not supply those receipts. A human closure and an automatic closure use distinct
handlers and cannot substitute for each other.

Waits retain the exact publication and effect history across store reopen.
Artifact and normalized source generation are checked again before consumption;
a draft edit cannot change the pinned edge. Pure resume/decline replies reconcile
from the original retained answer. A lost story/release write reply remains
uncertain until original service evidence is reconciled; it never launches a
fresh write. Cancellation and resource ownership remain the existing 005 ledger.

Release consent performs no push, tag, upload or API publication. Readiness and
the existing release gate remain independent, and later publication effects
still need their own exact approvals. Portfolio claims and frozen membership
stay behind the existing services; this binding adds no release scheduler.
Optional notifications can be composed with installed notification activities
and explicit destinations; delivery never answers the question. Connector
freshness, portfolio readiness and migration scenario evidence remain 017/018
obligations and block cutover where unverified.

The installed catalog and shared neutral action schema expose the same contract
to Designer, IPC, HTTP and MCP. No route-specific decision implementation exists.
These Rust changes require an authorized manual restart/rebuild to load; no live
application restart or publication is part of implementation verification.
