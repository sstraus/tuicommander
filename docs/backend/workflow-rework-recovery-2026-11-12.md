# Workflow rework recovery inventory

Recovery baseline: `feat/workflow-engine-rework`, `31d5556a8e`, 2026-11-12.
The branch has 214 commits not on local main (`59b0900fee`). The pre-existing
`plugins` submodule pointer change is not part of this work.

This is an evidence inventory, not acceptance or a cutover authorization.
Unchecked criteria remain open until their actual obligations are verified.

| Story | Implemented and retained evidence | Missing acceptance / next action |
| --- | --- | --- |
| 008-22c4 | Native development operations and public owner binding; HTTPS publication fix `a3b6dcd047`. Worklog records 21/21 public-binding cases and 2/2 HTTPS rewrite cases on rb. | AC1 complete development path, including exact reviewed artifact and approved publication, needs acceptance across the composed consumer. Offline URL tests are not live GitHub publication. |
| 017-70f7 | Mapping audit, preserved source identity, inactive conversion of triage, PR review and story delivery. `131ce142f7`, `aec19f60b7`, `8871c6cae9`; 2962 adds constituent subflows. | Complete resolve_plan and issue_delivery conversion; validate pinned transitive closure and no hidden scheduler for these consumers. Preservation rows and cutover blockers remain explicit. |
| 018-bb68 | Composed parity tests `8c0a3fcca5`; A/B/D and connector evidence recorded. Additional work is isolated on feat/wer-018-parity. | C real delivery/review/correction and integrated A-E acceptance remain open. Coordinate the parity peer before integration. |
| 019-683f | Dependency/cutover planning only. | All cutover criteria remain open: snapshot, explicit boundary approval, drain/recovery, trigger ownership transfer, history, rollback and removal. No live cutover is authorized by implementation ownership. |
| 020-6471 | Scoped role/profile/report adapter `58bee4cb8b`, evidence `d199ed2f69`, catalog fixture `814a294112`. Worklog records 27 passing native-host cases. | Verify role/report source-bound composition and pinning through converted consumers, not only adapter tests. Live provider evidence is not claimed. Remove the stale duplicate pending file; its content is retained in the in_progress file with added evidence. |
| 021-6e1b | Owned capped idempotent creation `ec2ecadc8d`, catalog `577b3172fc`; four rb cases include zero cap and lost-response reconciliation. | Catalog/shared admission acceptance and original coordinator review binding in the complete parent graph. 2962 create-body already fixes cap/review constants. |
| 022-597e | Child/cohort adapter `58bee4cb8b`; batch barrier, landing, pin, verdict and scoped re-entry integrated. `67ac6f9aa8`, `8871c6cae9`, `c69d032895`, `7d8b21f8df`, `8c49e6af0c`, `31d5556a8e`. Recorded rb boundary cases prove slot ordering, receipts and exact artifact/report. | Parent round graph must compose children, landing and reviewer verdict with bounded FIX re-entry. Reconcile complete child/source criteria against that graph, including crash recovery and unchanged publication pins. |
| 023-026f | Issue effects `a51075d2c6`; mediated production service integration `9c53c63e4a` / `7859780edb`. Worklog records 69/69 integration cases and an authorized sandbox comment/label receipt plus cleanup. | Current production handoff/observe/cancel explicitly refuses under H2: no composed story-create/child path. This is a real remaining capability gap, not a reason to tick all criteria. |
| 024-c9ee | Exact review events/publication intents `498c74280c`; retained source-generation gate. Worklog records nine rb Git/SQLite and provider-wire cases. | Complete template/public source-start composition; actual live review/readiness acceptance remains unproved in this story. Separate approval is already covered. |
| 2961-1d60 | Plans/stories and workflow spaces implemented; criteria 2-5 checked in retained story. | Criterion 6 requires screenshots of both spaces and targeted navigation evidence. The late contract is explicitly rejected, not silently marked satisfied. |
| 2962-554e | G1/G2, story child, sequential member, coordinator create body and scoped Reopen are on baseline HEAD. Latest retained rb run: 24/24. | Native pending batch selection; batch round, exact critic and bounded loop; coordinator mapping and final resolve_plan/issue_delivery conversions. |

## Branch dispositions

- **feat/wer-018-parity: no merge needed after source reconciliation.** The
  resumed owner confirmed HEAD `4c590f4b64` and retained evidence. Its scheduled
  admission and mid-run pin regression are already replayed as `12782b94f0` and
  `76d27fc328`. Direct comparison found byte-identical scheduled-trigger,
  approval, parity, live fixture and HTTP regression sources. Its story differs
  only by stale dependency filenames. A whole-branch merge predicts thirteen
  conflicts in newer role/provenance implementation and documentation. Preserve
  this branch, reuse compatible evidence, and do not merge obsolete ancestry.
- **feat/2530-switch-review: do not merge.** `647e378ebf` is an empty start marker;
  `05d6eb0e5f` adds only a future-dated legacy publisher retirement comment. It
  neither switches the publisher nor proves parity, and retirement belongs to
  the explicitly approved 019 boundary. Retain the branch for its owner; do not
  delete it. No implementation is discarded from the rework branch.

## Validation policy

Reuse the cited worklog results where source hashes remain compatible. No test
has been rerun for this inventory. Targeted validation of new compositions must
cover their actual request types, publication admission and changed consumers.
A final landing report must inspect conflicts against then-current main without
switching this checkout or merging into main. Batch gates and any authorized
cutover remain the parent's responsibility.

## Landing simulation

A read-only `git merge-tree --write-tree HEAD main` at local main `59b0900fee`
reported content conflicts in the HTTP API, Tauri command and command-threading
documents, plus the `plugins` submodule. Evidence is retained under
`~/Gits/.tmp/wer-owner/main-merge-tree.txt`. This is not a main merge or a
final pinned landing check; re-evaluate against the then-current main. The
pre-existing dirty submodule pointer remains untouched.


## Recovery outcome before owner decisions

- Native pending selection, immutable receipt replay/current child revisions and
  the loop-control verdict field are implemented. rb task
  `20261112T173304Z-50657` passed all five native batch cases in 2.621s after a
  2m30s compile. The earlier dynamic cohort/current-revision case passed in task
  `20261112T172034Z-9963` and its source is unchanged.
- The shared 50-descriptor installed-host capture adds pending selection. The
  local Designer consumer passed four tests in 1.82s (pnpm 12.15.0, Vitest 5.0.0).
- The proposed round/loop/coordinator/resolve_plan composition remains incomplete:
  the existing depth-four publication bound refuses its reusable child chain.
  Fresh/migrated seed recognition was corrected from decisive store code; the
  complete plan conversion has not been rerun green. No limit was raised without
  approval. Role contents also remain identifier-only, not real execution proof.
- Issue delivery/publication-tail conversion remains refused. 2961 screenshots
  remain absent after the bounded headless attempt. 019 remains entirely open.
- Formatting passed; instruction links passed. Story closure correctly refused
  the unchecked 2962 REALITY criterion. These results are not a landing approval.
- Two parent decisions are outstanding: generic bounded role data context with
  actual source/artifact content, and depth-eight admission versus flattening the
  new graph. Until resolved, do not claim 017/018/2962 parity or close the stories.

Logs and content hashes are retained under `~/Gits/.tmp/wer-owner/`. The rb
compiler identifies Rust 1.99.1; nextest version was not measured. Native macOS
execution was limited to the rendering-only screenshot attempt; Rust behavioral
evidence is Linux rb, not native desktop behavior.


## Approved continuation: depth and real role material

This section supersedes the earlier depth/content blocked disposition. The parent
approved generic depth eight and bounded read-only role snapshots. No graph was
flattened; expanded-edge cap 32, cycle detection and activation bounds are unchanged.

- rb `20261112T184118Z-10554` passed depth-eight save/read and depth-nine refusal
  with the existing edge/cycle cases. That run had nine passes and two failures:
  stale catalog capture and an old activity-list expectation, both diagnosed.
- rb `20261112T190308Z-37899` passed all five targeted cases in 1.976s after a
  3m04s compile; peak RSS 12981576 KiB. The corrected story-delivery conversion
  test now validates the resolve-plan draft and all eight published content pins.
- `native.agent.context.read` derives actual plan/story/member snapshots and an
  exact artifact diff from the selected backend, original batch pin and Git tree.
  A coordinator also receives the exact Markdown bytes at `Plan.source`, not its
  filename. Missing/oversized documents refuse. Part, total and member bounds are
  262144 bytes, 786432 bytes and 128; declared model limits also apply.
- Role admission proves byte identity with the original same-run settled read,
  current artifact and backend revisions. Plan membership changes also refuse,
  because story creation does not increment the plan revision. The real-backend
  tests cover literal request contents, forged data, foreign runs/projects, stale
  artifacts, missing batch pins, document byte preservation and size refusal.
  Context grants no tools or write rights; independent review remains tool-less.
- The refreshed shared capture has 51 descriptors; the Designer consumer passed
  four tests in 3.35s. These are direct-service and Linux-rb adapter/publication
  results, not complete plan execution or live provider acceptance.
- Issue conversion remains refused pending the parent shared-contract decision.
  The audit additionally proves that plan creation, problem text, issue-bound
  MarkReady and failure/uncertainty status routing must be preserved. No approved
  existing-graph shortcut was found, and no pending design was implemented.
- 2962 REALITY, issue parity, 2961 screenshots and separately unapproved 019 remain
  open. No push, landing, desktop launch or daily-app restart occurred.

## Inactive issue graph continuation

The issue converter now recognizes the exact seeded intake and its exact
referenced issue-story publication. It composes native preparation, exact
problem-preserving story creation, created-cohort observation, the pinned story
child and a read-only graph receipt. The receipt proves normal child return by
journal replay. It never dispatches a child or trusts caller completion JSON.
The publication tail is push, draft PR and native mark-ready approval. A stopped
refusal/uncertainty or blocked child permits only its exact source-bound status;
unknown original effects and claims are retained.

Focused rb evidence:

- `20261112T204853Z-82903`: three of four passed. The MarkReady provenance case,
  graph receipt case and catalog capture passed. The converter retained the old
  unreachable accepted terminal; that terminal and its outcome were removed.
- `20261112T205924Z-6774`: four of four passed in 3.206 seconds after 2m27s compile.
  This covers the actual NativeHost graph-dispatch route and blocked-child status
  gate, corrected conversion, normal 54-descriptor catalog equality, and existing
  same-owner typed subflow replay. Peak RSS was 12811728 KiB; remote tree was clean.
  The wrapper returned 75 because the parent-requested context commit and capture
  README updates changed the local tree during execution. Production and test
  sources did not change after sync. This is frozen-snapshot evidence, not a
  wrapper-green final-tree claim.

The context slice is isolated in `b3486182c60e30c9869a801e0a7bcdd33607669d` for
parent-owned independent review. Issue source/store/effect changes and composed
execution parity are separate. Successful complete plan/issue execution is not
established by the focused cases above. Live019 remains excluded.

## Refreshed landing comparison (context commit only)

Read-only `git merge-tree --write-tree HEAD main` compared context commit
`b3486182c60e30c9869a801e0a7bcdd33607669d` with main
`96dc0cea28b019bfdcbba58bd321329122bcbd2a`. It returned 1. Main has 149 unique
commits and this branch 219 at that comparison. The conflict set remains
`SPEC.md`, `docs/api/http-api.md`, `docs/api/tauri-commands.md`,
`docs/backend/command-threading.md` and the `plugins` submodule. The submodule
comparison reports missing commits, so its intended pin requires owner review.
The unrelated dirty worktree pointer is not part of the context commit.

This comparison excludes uncommitted issue work. Repeat on the final reviewed
branch SHA and then-current main before landing. Resolve the documentation
contracts semantically, choose the submodule pin explicitly, and validate the
merged tree with parent-owned repository gates. No merge, rebase, push, trigger
transfer or landing was performed.

## Controlled composition evidence after review r1

The immutable context/depth commit is `b3486182c6`. Parent-requested critic
regression `ceccbe57cd` was cherry-picked as `a085f08be8`; context follow-up
`280184b9b` removes duplicate snapshot verification, preserves binary summaries,
uses explicitly truncated bounded diff stats for oversized diffs, reads plan
Markdown only for coordinators, and exposes the host-derived story review
subject/gates. The parent owns critic round2 on that exact range.

The inactive issue graph now composes accepted triage, owned plan preparation,
problem-preserving story creation, created cohort, pinned story child, read-only
source-bound graph receipt, artifact pin, push, draft PR and MarkReady approval.
Only a completed child reaches publication. Blocked child or refused/stopped-
uncertain publication can attempt only its exact status; the original uncertain
result and claims remain retained. A potentially running writer cannot advance.
Long title/body material is truncated within the20,000-character problem cap
while reserving the complete original issue URL. External backends without the
once-only plan/problem capabilities refuse, not fall back to native storage.

Author-focused results (logs under `~/Gits/.tmp/wer-owner/`):

| Run | Result | Scope and limits |
|---|---|---|
|18, rb20261112T214201Z-90082 |15/16 passed; wrapper100 | Final context5, issue conversion/catalog/provenance/preparation/receipt/status authority and both composed issue ready/decline cases passed. Plan reached the final reviewer at118.86s but hit the120s outer harness cutoff. |
|19, rb20261112T215442Z-95613 |2/2 passed; wrapper0 | The same plan assertions pass in127.441s after only the outer guard increased to360s; role and workflow limits unchanged. Actual public actor/ACP empty-proposal/operator path passed in2.396s. |
|20, rb20261112T220034Z-25531 |5/5 passed; wrapper0 | Short/long ASCII/multibyte issue material through real admission/preparation, once-only plan/story persistence, source refusal and lost-write behavior. |
|21, rb20261112T220526Z-56301 |4/4 passed; wrapper0 | Existing native story-create consumers: ownership/approval, reservation/cap/unknown, foreign project, stale source/revoked actor. Corrected a nonmatching run20 module filter; no coverage was inferred from its absence. |

These are actual converted graph/kernel/native-backend executions with controlled
external ports, not live model/provider acceptance. Coordinator/reviewer replies
cross the real ACP fixture process. Implementer execution uses a test-owned model
binding and real nested staging. Provider observations and native review replies
are controlled. The plan case creates/delivers a story, reviews the combined
artifact, arms FIX, reopens/re-delivers only that owner, and completes the real
ledger. The issue cases deliver actual story/file bytes before ready or decline.
The separate public-actor case proves an empty proposal list cannot claim success.

Nextest fixture setup now includes the new ACP consumers, and run18 verified
that prerequisite activation. The fixture-scope script itself timed out before
reporting; inspection showed it launches multiple Cargo suites, so its full
check stays with the parent batch rather than being counted as a static pass.
Independent review, review proof, story-close --check and the final pinned
landing comparison remain required. No live019 binding, push, landing, desktop
launch, unrelated plugin-pin change or daily-app restart was performed.
