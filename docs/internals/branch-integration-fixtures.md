# Branch integration fixture provenance

Story: 1295-a2ce. Source: read-only branch audit of 55 branches on 2026-09-30
(`branch-audit-1790753745.md`). Tests rebuild reduced histories with real Git
in the test scratch root; they do not depend on the audited branches remaining
available. These are reduced reproductions of change shapes, not exports of
whole repositories.

| Rule | Audit case and independent evidence | Fixture |
| --- | --- | --- |
| Ancestry | wiz `POC-00168/activity-evaluation` tip is an ancestor of merged PR #189's head; ordinary ancestry is already covered by lifecycle and merged-list fixtures | Existing ancestry and checked-out integration tests |
| Patch equivalence | wiz `POC-00171/git-discard-guard-env-expand`, merged PR #195, has no unique `git cherry` patches | Existing rebase and single-patch squash fixtures |
| No-op merge | tuic revised worktree twins contain the same changes in a different history; the reduced fixture reconstructs the unchanged final merge tree, rather than assuming that every current audited ref merges cleanly | `noop_merge_branch_is_integrated_1295` |
| Squash message | wiz PR #189 squash `4a190ec8` contains `* subject` bullets, including `fix(hud): add Opus 4.7 to pricing table (#343-d76a)` | Multi-commit squash and actual-bullet-format fixtures |
| Context-free twins | tuic `backup/worktree-v4-466eb71a`: `c6585c43e` and `11c68cb52` have main twins with zero extra changed lines | `audited_same_subject_twin_survives_later_refactoring_1295`; uses the real `c6585c43e` subject and two added source lines |
| Content superset | Audit checks added lines against main HEAD when patch IDs differ; partial unique WIP must stay protected | Content, unique-line, binary, duplicate-line, deletion and path fixtures |

The audit's `twin 0` result motivates exact equality of changed lines, paths
and modes, ignoring context and hunk coordinates. The main-HEAD heuristic
requires **100%** of added lines, with multiplicity, in the same regular
files. The audit's 97% row also had a merged PR proving the landed head; it
is not evidence for a general 97% threshold. Even 100% content overlap is
not a semantic proof, so either content heuristic requires a recovery ref
at the exact branch tip before safe deletion.

The no-op and squash fixtures prove that Git's virtual merge leaves the
target tree unchanged; text in a commit message alone never suffices.
Conflicts and unsupported virtual merges do not produce a no-op proof.
Merge commits are excluded from patch/twin-only proofs because those omit
resolution changes. Arbitrary reimplementations, unrelated histories and
partial overlap remain uncertain unless a stronger existing proof applies.
The existing archived-tip deletion rule remains available for deliberate
retirement without claiming semantic integration.

Attacks cover forged or incomplete squash subjects, unique additions,
`++` content mistaken for diff headers, repeated lines, binary content,
missing final newline, deleted paths, literal bracketed filenames, stale
archives and archive hooks removing the recovery ref. Existing tests cover
current/default/worktree protections, stale upstreams, dirty files, PR API
errors, and branch-tip comparison at deletion.
