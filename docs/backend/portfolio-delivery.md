# Opt-in portfolio delivery (WF10)

The daemon owns durable intake and plan claims in the workflow run journal. Story data stays behind the `StoryBackend` port (`stories::story_backend()`); portfolio code does not query the native story tables. A future repository story connector can replace the native adapter without changing the intake journal.

Use the existing project-scoped workflow run action over IPC, HTTP or MCP:

```json
{"action":"portfolio","request":{"action":"configure","enabled":true}}
```

Responses are `{"type":"portfolio","value":...}`. Nested request fields use camelCase; action, source, decision and inventory enum values use snake_case. Intake is disabled by default. Disabling prevents new intake and delivery recovery; existing requests, dispositions and claims remain visible.

Optional `unownedAlarmMs` (1 ms to 7 days) sets the release-owner alarm. Each intake accepted while it is set schedules one durable timer in the same transaction. When the timer fires and the request is still `unowned`, the daemon appends one `owner_alarm` wake for that request. A request that has a recorded decision (claim, link, hold, park or dismiss) raises no alarm. The wake and the fired timer receipt commit together, so a restart neither loses nor repeats the alarm. Omitting the field keeps the current alarm, so a plain enable toggle does not turn it off. An explicit `null` removes the policy for later intake; timers already scheduled remain. The reply reports the alarm that is now in effect. A timer row that cannot be read is logged and retained; it does not stop other timers or claimed-delivery recovery.

## Intake and responsibility

`intake {source,sourceKey,summary,detail}` accepts `manual`, `event`, `timer`, `mail`, `result` or `decision`. The caller supplies the stable identity of the requirement/event, not a fresh identity for every poll. Repeated label observations therefore reuse the same key and cannot assign more work. There is no webhook receiver or automatic mailbox import in this feature.

The unique key is `(project,source,sourceKey)`. Equal redelivery returns the same current record, even when intake was subsequently disabled. Reusing a key with different original summary/detail is refused. Summary is limited to 512 bytes, detail to 32,000 and source key to 512. Original payloads remain available through `get {id}` after linkage or dismissal.

`decide {id,expectedRevision,actor,reason,decision}` requires the current item revision. Decisions are:

- `{"type":"deliver","planId":"...","definitionId":"...","definitionRevision":1,"limits":{...}}`: retain a plan claim and pin a published plan graph and its existing Story Dispatch child definition.
- `{"type":"link","runId":"...","storyId":"..."}`: explicitly link a retained request to existing project-owned work; `storyId` is optional.
- `{"type":"hold","bossOnly":true}`: record a reserved Boss decision. Ordinary questions are not automatically Boss-only.
- `{"type":"park"}` or `{"type":"dismiss"}`: explicitly park or dismiss responsibility.

An operator or an authorized model can make the classification decision. This does not introduce another scheduler or confer integration/publication authority. Existing Agent/Call prompt mechanisms may obtain model advice; portfolio does not launch a mandatory coordinator.

## Delivery and recovery

A delivery definition must be an executable plan publication with exactly one Story Dispatch and no Coordinator Agent node. WF04 typed `SubmitStoryProposals` supplies each Create Stories activation. Story-level Create Stories remains forbidden. The journal does not invent proposals when no input exists.

A committed `claimed` item is the durable delivery outbox. The owning daemon reconciles it at boot and every five seconds, using the existing payload-bound graph-root start key `portfolio:<item-id>`. `deliver {id}` explicitly reconciles the same intent through the owning daemon. The root identity, immutable definition and limits must match the claim. A crash after root creation but before the delivery receipt returns the same root.

One claim owns one plan. An existing unresolved workflow or manual story owner prevents a new claim. Graph-root starts, legacy plan starts, manual story starts and Story Dispatch share the portfolio ownership fence. Cancellation retains a portfolio claim because it is not proof of integration or stopped/unattributed work. The owning root can still dispatch its children through the existing admission service.

Story Dispatch remains the only fan-out: current dependency integration receipts, file-scope conflicts, cross-run reservations and pinned capacity remain authoritative. `maxParallelStories:1` preserves serial admission. Existing legacy batch workflows are not replaced or reinterpreted by this opt-in service; their complete-artifact critic policy remains their responsibility during migration. Portfolio adds no utilization-based scheduling policy.

Serial story admission and the retained model-driven batch brief are separate contracts. The latter assigns ordered logical stories to one managed implementer and one critic over its complete artifact. Declare its claims, reports, branches and release/publication membership through migration inventory. There is no native multi-story batch ownership engine in WF10; the orchestrator model retains that existing batch policy during migration.

Model decisions use the existing typed services. A passing implementer must check the satisfied criteria and submit the native story for review before submitting its completed attempt report. Reports are advisory and do not perform those story transitions. An approving reviewer report against an in-progress story therefore reaches an uncertain Judge and retains responsibility. Integration remains an explicit `integrate` run action with the current sequence and exact base/source commits; WF08 proof collection remains independently owned. No Integrate or ProofWait node kind is introduced. Automatic-closure publication is still refused by the existing definition service.

`release {id,expectedRevision,actor,reason}` requires a completed owning run, a clean canonical artifact, current integration receipts for included stories, no outstanding WF08 proof debt and current empty snapshots from every declared migration connector. Unknown, expired or nonempty connector inventory retains the claim. This conservative release contract does not infer ownership from branch names, idle workers or poll results. It performs no reaping, branch deletion or publication.

WF08's independently started cohort service owns proof execution. Portfolio recovery does not run proof commands or replace the cohort waiter/owner rules. A completed child or a Done story cannot bypass proof/integration admission.

## Bounded inventory and replay

`view {afterId?,limit}` keyset-pages 1–100 requests. Summary records omit original detail; at most the page's linked runs are hydrated. Each run reports at most 20 ready candidates, holds and unresolved questions. Read the existing run/question APIs for full evidence. Boss-only waits come from explicit portfolio decisions, while native questions remain decision waits.

The view includes request state totals; linked native run pins, admission and question summaries; unresolved report counts; WF08 debt/owner/reason summaries (at most 20 cohorts); declared connector counts; and canonical Git observations. Release membership is explicitly supplied by connector `release_member` rows and never inferred from the next dispatch wave. `publication_debt` remains a distinct connector kind.

Canonical Git inventory uses only local `git diff --quiet --no-ext-diff` (working tree and index) and `git rev-parse --verify --quiet refs/stash`, each with a three-second deadline. Failures are unknown. The tracked-diff observation explicitly leaves untracked inventory unknown; it is not a clean-worktree certificate. Native integration/release uses the full clean-artifact verifier instead.

`inventory {connector,expectedRevision,observation}` declares a migration connector snapshot. An observation is `{"state":"unknown","reason":"..."}` or `{"state":"known","observedMs":...,"expiresMs":...,"rows":[...]}`. There are at most 16 declared connectors and 100 rows per snapshot. Each row contains `kind`, `identity` and `summary` (512 bytes each for the strings). Kinds are `claim`, `branch`, `report`, `dirty_trunk`, `stash`, `publication_debt`, `release_member`. A stale known snapshot projects as unknown. Summary views return counts; `inventory_get {connector}` returns the bounded detail. An undeclared connector makes no claim to inventory completeness.

`wakes {afterCursor,limit}` returns 1–100 ordered durable hints with `nextCursor` and `hasMore`. Reads never acknowledge or dispose responsibility. Save the cursor, reread current items by ID and resume after restart. `hasMore` reflects the current journal, so it can change when new events arrive even when the replayed prefix is identical.

`schedule {key,dueMs,summary,detail}` records an opt-in one-shot timer using WF19's durable timer validation. Stable keys bind payload and due time. Timer intake commits before the fired receipt, making either crash boundary idempotent. Disabled projects retain timers for later recovery; overdue timers retain the request rather than silently losing it.

## Evidence limits

Native service tests cover intake deduplication, retained claims across cancellation/reopen, delivery recovery, typed coordinator-free input, dependency receipt admission, parallel work with a branch-local question, cursor replay, HTTP parity and conservative connector release. They use real stores/Git and explicit protocol reports; they do not claim live model-process or seven-gate execution evidence. The exact composite A fork/repair/integration/proof chain under B still needs its separate scenario evidence before WF10 is complete.

The isolated `agent-test-2517` runtime replay on the implementation commit additionally exercised genuine ACP implementers and an independent critic without a Coordinator node. It retained the exact unanswered question, wake-event prefix, undisposed result and plan claim across a controlled host restart. Exact disjoint file scopes admitted a sibling while the question waited; the critic rejected an intentional value-41 candidate, the old fork generation became superseded, and a new implementer committed and checked value 42. The initial fixture omitted native submit-for-review transitions, so an advisory approval correctly reached an uncertain Judge instead of accepting the story. These observations do not establish integration, proof convergence or complete-batch evidence. A trailing-slash scope such as `one/` was conservatively unknown; declare exact file paths for this fixture.

ACP implementers use the same `story` MCP tool with their host-issued connection origin to check criteria and submit native review. The live peer root scopes the request; the peer remains a managed actor and gains no human approval authority. Manual PTY claims remain separate from workflow-owned ACP attempts.

## Release scope and publication authority (WF15)

`{"action":"release_scope","request":{...}}` on the portfolio run action pins one release and decides publication. It is evidence-driven: the daemon never pushes, tags or uploads; `gate` returns an intent for an adapter.

- `pin {release,version,rules,members}` freezes a scope (`release`, `next`, `boss_wait` buckets). `rules` holds the repository version files, plugins-first order, history rule (`linear` or `public_squash`), required CI jobs and assets. An empty required field is reported as `policy_missing`, never read as "nothing required".
- `admit {release,storyId}` puts new work in `next`; the release set and revision do not change. Only `amend {expectedRevision,actor,reason}` moves a story, and it bumps the revision.
- `count {release}` counts that release alone (no cross-release total) and, at the first zero of a scope revision, writes one `release_zero` wake (cursor `wakes`, `itemId` is the release). Unreadable stories count as open.
- `readiness {release,evidence}` needs zero release work, no proof debt (every integration receipt of a release story has a converged or filed proof; unknown is blocked, and a done release story with no integration receipt is `proof_unknown`; WontFix needs no proof), repository version files, plugins-first, private-history and, for the exact candidate commit, the latest attempt of every required CI job plus uploaded assets. Missing jobs or assets are `waiting` until `waitDeadlineMs`, then `*_timed_out`. CI of another commit never counts.
- `authorize {release,intent,actor,reason}` records consent for one exact `{kind,target,commit}`; the first consent stands.
- `gate {release,intent,evidence}` clears only with that exact consent, an intent commit equal to the evidence commit, a clean denied-material scan (missing denylist refuses), push refs that are exactly the intent's own ref (`tag`: `refs/tags/<target>`; `branch`: `refs/heads/<target>`; `package`/`release`: none or `refs/tags/<target>`; otherwise `refs_outside_intent`), and no owed frontend batch. The one exception is a tag-only push of a validated tag (readiness ready, push refs exactly that tag). `branch` intents skip release readiness; `tag`, `package` and `release` require it. Green CI is readiness, not consent.

Boss reads release questions in AI Chat. When a request runs through the daemon service (IPC, HTTP or MCP), two replies also send peer mail to the latest-connected live AI Chat whose workspace resolves to the release project: a `count` that raises the zero notice sends `release_ready` (release, revision, remaining work, countdown and proof state `current`, `debt` or `unknown`), and a `gate` refused only for `no_authorization` sends `release_consent` with the exact intent and a ready `authorize` request. AI Chat may submit that `authorize` request through MCP `workflow action=run` in its own workspace; actor `Boss` and his answer as the reason become the recorded consent. The reply carries `aiChat {delivered,peer?,reason?}`; with no live chat for the project it reports `no_live_ai_chat`, and the once-only zero notice is not raised again.

Limits: CI, assets, versions, ancestry, push refs, frontend batch state and the denied scan are supplied by the caller; this feature does not read GitHub or run hooks. Real publication needs a separately authorized rehearsal or release. Zero notices fire when `count` is evaluated, not from a timer.
