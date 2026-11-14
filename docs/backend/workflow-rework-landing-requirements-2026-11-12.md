# Workflow rework: parity evidence and landing requirements

Date: 2026-11-12. Status: author implementation and focused evidence recorded;
independent review and closure proof remain open. This is not a landing or
live-cutover approval.

## Scope and retained inventory

The task covers stories008,017–024,2961 and2962, the two peer branches, duplicate
020 reconciliation, native plan/issue composition, and landing requirements.
The detailed inventory remains in
[the recovery report](workflow-rework-recovery-2026-11-12.md).

- Retain `feat/wer-018-parity`; do not merge it wholesale. Required evidence and
  source changes were already replayed, including `12782b94f0` and `76d27fc328`.
  Earlier exact comparisons and conflict analysis are recorded in recovery.
- Retain `feat/2530-switch-review`; it does not supply actual switch/parity
  evidence. Its retirement work belongs to separately approved019.
- Duplicate pending020 was removed in `ea39f0964d`. The in-progress020 and its
  worklogs remain.
-2961 screenshot obligations and019 live trigger transfer stay separate from
  the native2962 composition. No gate in this report waives either.

## Implementation pins and review state

| Pin | Scope |
|---|---|
| `66ad1b0c1eb378f42db95767b67deefc3bb119fd` | Previous incomplete composition checkpoint. |
| `b3486182c60e30c9869a801e0a7bcdd33607669d` | Approved depth8 and bounded read-only role snapshots. |
| `a085f08be8` | Cherry-pick of the independent binary-summary regression. |
| `280184b9b6fcb420ae7002d406d0f540152471a7` | Parent-requested context corrections and actual host-derived review subject/gates. |
| `91c15ce35157b31e915f84a934f3ac22c901a30d` | Source-bound issue composition and controlled full plan/issue execution evidence. |

Critic round1 requested changes on `b3486182c6`. The parent directed removal of
duplicate `verify()` calls and their verify-only assertions, while retaining
request shape/caps and source-read restrictions. Binary summaries now remain
visible. Oversized diffs carry bounded stats with an explicit truncation marker;
only coordinator reads require plan document bytes. The parent accepted the
non-UTF-8 refusal. Requested TEST-CHANGE criteria explain the depth and converter
expectation changes.

Review ranges sent to the parent:

- Context round2: `b3486182c6..280184b9b`.
- Issue/composed execution: `280184b9b..91c15ce35`.

The independent critic accepted the original context corrections and classified
all author tests useful. It requested two deletion-only follow-ups: use the
settled graph receipt without re-running composition at status time, and remove
the duplicate preparation table in favor of transactional backend plan-once
identity plus the effect ledger. Those deletion corrections passed focused validation in run22;
a final accepted verdict remains required. Story2962 keeps an unchecked REVIEW criterion and remains
`in_progress`; it is not closed. The author-side `story-close.sh --check`
refused criterion11 (unchecked REVIEW), as expected; no proof or status was
fabricated. Log: `story-close-final-author-check.log`.

## Behavior and parity boundary

The configured plan graph creates ordered coordinator proposals, admits bounded
sequential batches, delivers pinned story children, reviews the combined artifact
and re-enters only the armed FIX owner. Generic subflow depth8 is accepted and9
refused; edge32, cycle and activation bounds remain unchanged.

The configured issue graph retains accepted source material, reserves one native
plan/proposal, creates its story, runs the pinned story child and records a
read-only source-bound graph receipt. Only completed children reach artifact pin,
push, draft PR and separate MarkReady approval. A blocked child or refused/stopped-
uncertain publication permits only its exact status attempt. Original unknown
results and resource claims remain retained. A writer that may still run cannot
advance to status. Issue problem material preserves the full source URL within
its20,000-character cap, including long multibyte bodies.

The composed harness drives actual converted publications through the graph
kernel, preparation, approval answers, dispatch and observations. Git, native
story creation/lifecycle, cohort, batch/FIX, check and correction state are real.
Coordinator/reviewer replies cross the real ACP fixture process. The implementer
uses a test-owned model binding and real nested staging. Independent-review
replies and provider publication observations are controlled external ports.
This is controlled execution evidence, not live-model or GitHub acceptance.
The separate public-actor case proves that an empty coordinator list reaches the
operator without creating stories or claiming completed delivery.

## Validation evidence

Logs are retained under `~/Gits/.tmp/wer-owner/`. All listed Rust runs use the
orchestrator remote build-slot wrapper and `scripts/with-test-tmp.sh`.

| Log / rb run | Result | Scope |
|---|---|---|
| `composed-execution-18.log` / `20261112T214201Z-90082` |15/16 passed; wrapper100 | Final context5; exact issue conversion/catalog, MarkReady provenance, preparation/receipt/status authority, and both composed issue ready/decline cases passed. Plan progressed through its final reviewer at118.86s before the120s outer cutoff. |
| `plan-execution-19.log` / `20261112T215442Z-95613` |2/2 passed; wrapper0 | Full composed plan delivery/FIX127.441s; public actor/ACP empty-proposal/operator2.396s. Only the exact plan test's outer hang guard changed to360s;60s role budgets, workflow limits and assertions did not change. |
| `issue-material-20.log` / `20261112T220034Z-25531` |5/5 passed in2.619s; wrapper0 | Short/long ASCII/multibyte issue material through actual normalized admission/preparation, persistent once-only plan/story identity, native source refusal and lost-write behavior. |
| `story-create-consumers-21.log` / `20261112T220526Z-56301` |4/4 passed in0.506s; wrapper0 | Existing native story-create approval/ownership, reservation/cap/unknown, cross-project, stale-source and revoked-actor consumers. |
| `critic-deletions-22.log` / `20261112T223148Z-13934` |5/5 passed in3.263s; wrapper0 | Repeated exact preparation returns the same plan; original observation/uncertainty semantics and source-bound graph/status authority remain covered after deleting duplicate proof/state. |
| `designer-catalog54-14.log` |4/4 passed in1.77s | Focused Designer test against the shared54-descriptor native catalog capture. |

Run20's intended `native_story_create_tests::` term matched no registered path;
no coverage was inferred. Run21 selected the actual test-name substrings instead
of repeating its five green cases. All rb trees above reported clean. One
existing `admit_gitlab_event` dead-code warning remains. Run18 explicitly enabled
diagnostic traces; final run19 did not emit that diagnostic stderr.

Commands and filters:

```sh
# Prefix Rust commands with the authorized remote wrapper:
# <orchestrator>/tools/build/build-slot.sh --remote scripts/with-test-tmp.sh
cargo nextest run --manifest-path src-tauri/Cargo.toml --lib \
  -E 'test(composed_plan_execution) | test(converted_plan_public_actor)'
cargo nextest run --manifest-path src-tauri/Cargo.toml --lib \
  -E 'test(native_issue_prepare_) | test(native_issue_tests::) | test(issue_plan_once_) | test(issue_story_once_)'
cargo nextest run --manifest-path src-tauri/Cargo.toml --lib \
  -E 'test(native_story_create_)'
# Run18 additionally covered the issue publication/graph/provenance/conversion,
# native role context, catalog and composed issue filters retained in its log.
```

The changed-Rust/fixture formatter found one module-order issue in `run/mod.rs`;
it was corrected and the isolated module check produced no diff. Other selected
files passed. `git diff --cached --check` and commit hooks passed. Instruction
links passed with two canonical pairs. The fixture-scope script timed out before
reporting; inspection showed it runs several Cargo suites, so it is not counted
as a static or dynamic pass. Run18 proved fixture setup activates for the new ACP
consumers. Full fixture-scope validation belongs to the parent batch.

No full-suite, native desktop, live-provider, mutation or coverage/CRAP success is
claimed here. The parent owns aggregate validation and high-risk after-the-fact
checks. No desktop process, daily-app restart or external publication was started.

## Pinned comparison against main

Read-only `git merge-tree --write-tree HEAD main` at implementation
`91c15ce35157b31e915f84a934f3ac22c901a30d` versus main
`095e9c755ad809ce54e96b8715967ab2b6cdcc18` returned1. Main had162 unique commits;
this branch222. Evidence: `issue-final-main-merge-tree.txt` in the same directory.

Conflicts:

- `SPEC.md`
- `docs/api/http-api.md`
- `docs/api/tauri-commands.md`
- `docs/backend/command-threading.md`
- `plugins` submodule; the compared commits were not present for automatic merge

Git also automatically merged application and native-story storage files. That
is not proof of semantic compatibility. Test the proposed integrated snapshot,
including storage/protocol consumers, after an authorized conflict resolution.
No merge or rebase was performed. The pre-existing dirty `plugins` pointer is
untouched and was excluded from every implementation commit.

## Requirements before landing

1. Obtain accepted independent review/TDD proof for both pinned ranges and fix
   actionable findings. Do not close2962 merely because author tests pass.
2. Run `story-close.sh --check` against this worktree with accepted proof. A
   refusal is an open requirement, not a bypass opportunity. Actual closure is
   after an authorized landing; this task does not authorize one.
3. Resolve documentation conflicts semantically. The owner chooses the intended
   submodule pin; never stage the unrelated dirty pointer incidentally.
4. Refresh merge-tree on the final reviewed branch SHA and then-current main;
   repeat integration validation if source/configuration or the proposed merged
   tree changes. Do not equate an automatic merge with compatible behavior.
5. Run the required parent-owned integration batch, fixture-scope checks and
   high-risk mutation/coverage/CRAP follow-up. Reuse unaffected focused evidence;
   do not repeat native and rb aggregates for the same snapshot.
6. Keep2961 screenshots and019 live cutover/ownership transfer separate. This
   branch does not enable triggers, retire legacy owners, or supply that approval.
7. Load Rust changes only through a manual rebuild/restart when the owner is ready
   to lose the current session. Rust does not hot-reload. No restart occurred.

### Parent decisions (2026-11-13)

- Requirement 1: the parent accepted the review. Critic round 1 requested only
  OE2a and OE3. Commit `4b3daab197` fixed them and `e6990a9895` applied N4.
  These changes only delete code or move a comment, so no second critic round
  is required. The story REVIEW criterion is checked.
- Requirement 5: no mutation, coverage or CRAP run. The coordinator rule allows
  no added run without a named bug, and the critic named no remaining bug. The
  one aggregate nextest run on the merged tree stays required.
- Requirement 3: the `plugins` submodule keeps the pin of main. The dirty
  pointer in this worktree is unrelated and is not staged.

Risk is medium/high: shared effect disposition, persistent proposal identity,
source generation, migrations and model-context boundaries are changed. The
remaining review and integrated-tree evidence are material, not administrative.

## Independent review follow-up risks

The critic accepted larger receipt simplification, the existing crash-time
unstopped-uncertainty behavior, status wording and fixture setup breadth without
requesting changes. These are not newly verified live-provider guarantees.
It also identified decisive code evidence of a pre-existing live-cutover hazard:
the run's accepted label changes the issue generation after the next poll, so
later source-bound operations can refuse as superseded. This was not reproduced
live or fixed here.019 must resolve or explicitly disposition that requirement
before live transfer; controlled no-poll composition tests do not cover it.
