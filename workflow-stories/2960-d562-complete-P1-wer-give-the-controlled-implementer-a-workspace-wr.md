---
id: 2960-d562
title: "WER: Give the controlled implementer a workspace write and stage tool"
status: complete
priority: P1
type: feature
created: "2026-11-12T04:36:49.944Z"
updated: "2026-11-12T05:06:09.096Z"
dependencies: []
plan: plans/orchestrator-as-workflow-implementation.md
plan_step: WF17
started_at: "2026-11-12T04:38:02.988Z"
completed_at: "2026-11-12T05:06:09.095Z"
---

# WER: Give the controlled implementer a workspace write and stage tool

## Problem Statement

Boss decision Q7=A (2026-11-12, via or-3 be2873a0). The converted story_delivery, resolve_plan and issue_delivery templates are refused because no installed operation lets an implementer change code. Mediated roles run with --no-tools/--no-hooks/--bare and may only call target-less installed operations as host tools (run/neutral_model_tools.rs). The role report requires a clean worktree (run/native_agent.rs clean_artifact). review and commit already exist as native operations over a staged index. Add a host-owned write/stage tool on the controlled agent path; review and commit stay in the workflow body, and verification is its own workflow step. The 006 safety rule stays: no coarse process, shell or broad filesystem privilege for the model; only exact host tools in the effect ledger. Unblocks 017 conversion of the three delivery templates and 022 AC5. Parent: 2530-79d5 (WF17).

## Acceptance Criteria

- [x] Before code, record the contract (effect, true state, outside world, blast radius) in the worklog.
- [x] native.workspace.stage@1 writes or deletes only listed workspace-relative files and stages them. It refuses absolute paths, '..', any .git component, a path through a symlink, oversized requests and a workspace that already has unstaged or untracked changes. The receipt holds base commit, staged tree and paths.
- [x] The stage operation is an installed target-less native operation, so an implementer agent.role can bind it as an exact model tool through the existing ModelToolPolicy; no shell, PTY or process privilege is added.
- [x] The native artifact rule accepts HEAD plus the exact index tree when the worktree equals the index and nothing is untracked; with a clean worktree it is unchanged. The role report records the staged tree, and review and commit consume it; verification stays a separate step on the committed artifact (the check runner requires a clean worktree). A later unstaged change makes the artifact stale.
- [x] RED/GREEN targeted tests catch a path escape, a symlink escape, staging foreign dirt, a stale staged tree reaching review or commit, and an implementer report that cannot carry staged work.
- [x] Docs and the regenerated native catalog fixtures describe the tool; a to-test item covers a live implementer run after an authorized restart.
- [x] REALITY: effect=an implementer agent.role stages listed workspace files through its exact host tool native.workspace.stage, and its report carries the staged tree for review and commit; state=the bound workspace's Git index/worktree plus the RunStore effect ledger (tool call = child effect holding native:workspace); assumptions=a local Git workspace, a fixture model process and no network; radius=check.rs staged_artifact, every require_artifact caller, the role report, native_stage.rs, the catalog and dispatch (contract recorded before code in the worklog)
## Work Log

### 2026-11-12T04:38:01.988Z - Contract before code. Effect: an implementer agent.role calls the host tool native.workspace.stage@1 {files:[{path,content|null}]} through its exact ModelToolPolicy. The tool writes or deletes only those workspace-relative files and stages them with git; the receipt is {base: HEAD, tree: git write-tree, paths}. The role report then records that staged tree (observed_artifact = HEAD + index tree). review.change {base, tree} reviews the staged tree, commit.create commits it after its exact approval, and verification.run is a separate workflow step. True state: the Git index and worktree of the bound workspace; the RunStore effect ledger owns each tool call as a child effect, holding the native:workspace resource. Outside world: none; nothing leaves the machine, and push/publication keep their own approvals. Safety (006 rule kept): no shell, PTY, process or broad filesystem privilege; only listed paths; no absolute path, '..', .git component or symlinked path; at most 64 files and 4 MiB per call; the workspace must have no unstaged or untracked change before the call, so foreign dirt is never staged or attributed. Not an approval-gated mutation: staging is local and reviewed before commit, and an approval per write would stop unattended runs (Boss Q7=A). A lost reply is not repeated (default native reconcile). Artifact rule change: require_artifact and the role report use staged_artifact = HEAD plus write-tree, with no unstaged or untracked change; this is identical to clean_artifact for a clean worktree. Blast radius: every require_artifact caller (verification, review with head, push, role source check) now accepts HEAD+index when the index is dirty but the worktree equals it; push/review-with-head still refuse because their tree differs from the staged tree. Owned paths: run/check.rs, run/native_operations.rs, run/native_agent.rs, run/native_stage.rs (new), native_catalog.rs, run/native_dispatch.rs, tests, catalog fixtures, docs. RED/GREEN: native_stage_tests (path/symlink escape, foreign dirt, limits, delete; tree equals write-tree), the staged implementer report plus a reviewer over the staged tree, a stale staged tree refused by review/commit after a later unstaged edit, and the tool reached as an implementer model tool child effect.

### 2026-11-12T05:00:39.054Z - Criterion 4 wording corrected by its author before checkoff. Verification does not consume the staged tree: execute_run_check still requires a clean worktree, which matches Q7 ('verification is its own workflow step'). RED: rb 20261112T044103Z-82969, test(native_stage_tests::), 0/3, with stage stubbed and staged_artifact reverted to the clean rule. GREEN: rb 20261112T044700Z-56631, 167/168 over the round-10 + 018 + stage filter. The 1 failure, operator_answer_fences_artifact_and_reconciles_original_resume, matched on the old 'clean worktree' diagnostic; the refusal itself held. The staged-artifact message now keeps that phrase. rb 20261112T045430Z-23813: stage 3/3, the operator test, and the catalog capture (46 -> 47, only native/workspace.stage@1 added; SHA256 caad231fcfcc14cf5239a4e1f03e35823ddae7bf64f0968c5fde402d230d9072). A stale staged tree is refused by review without head (native_review.rs:69-74, existing) and by commit (native_commit_refuses_index_changed_after_approval, existing); require_artifact staleness after an unstaged edit is covered by native_staged_artifact_is_current_until_an_unstaged_edit. Toolchain: unmeasured.

### 2026-11-12T05:05:47.011Z - Completed in 52c2cd7c66 on feat/workflow-engine-rework. Drift check without capture: rb 20261112T050121Z-92727, catalog fixture + native_stage_tests 4/4. Local vitest NeutralDesigner.native 4/4. Live implementer run after an authorized restart: to-test/workflows-stories-4.md.

