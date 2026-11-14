# Worktree orphan cleanup and archive retention audit (story 2867-4336)

Audit of existing behaviour, 2026-11-07, against main `212ee260e4`. Line numbers
refer to this branch. Read-only: no worktree was removed.

## 1. Inventory

Source: `git worktree list --porcelain`, the lock file under the common git dir,
`git status --porcelain --untracked-files=all` per checkout, and `lsof -d cwd` for
shells. TUIC's HTTP routes need a session token and were not called; live agent
sessions are therefore from `lsof`, not from the session registry.

| Class | Count | Owner / proof |
|---|---|---|
| Linked, branch checked out | 6 | Branch name plus allocation marker; shells run in `fix-2904-progress-track` and `test-2707-e2e-candidate` |
| Archived (`__archived/`) | 1 | `feat-2539-chat-quote-reactions`; `dirty=1`, so retention keeps it |
| Detached | 0 | — |
| Missing directory / prunable | 0 | — |
| Locked | 0 | — |
| Unregistered directory with shells | 0 now | `lsof` showed shell cwds in `feat-tuic-w2`, absent from `git worktree list`; the directory no longer exists on a re-check (a sibling session's checkout was removed under live shells). Unknown ownership: cleanup never acts without a registration |
| Temporary (test) | 0 registered today | `tuic_core::temp_paths::recognized_temp_roots` (OS temp, `/tmp`, `/var/folders`, `~/Gits/.tmp`) |

Name or age is never proof: `detect_orphan_worktrees_blocking`
(`tuic-git/src/worktree.rs`) decides by detached HEAD, location and branch
tips, not by name or mtime. A detached HEAD that still carries a non-default local
branch is not an orphan (`non_default_local_branch_at_head`).

## 2. Existing cleanup, retention, retry and restart behaviour

### Orphan cleanup (`orphanCleanup` = off / ask / on)

1. Scan: `assess_orphan_worktrees` (`:16555`) lists detached checkouts outside temp
   roots whose directory exists. Missing directories are excluded (repair
   diagnostic, never swept).
2. Safe verdict `orphan_cleanup_safety` (`:16603`): clean status incl. untracked
   and submodules, no skip-worktree / assume-unchanged, HEAD not the tip of a
   non-default local branch, HEAD reachable from a branch or remote ref.
3. App layer adds sessions: `orphan_cleanup_safety_with_sessions`
   (`src-tauri/src/worktree.rs:1748`) rejects live and suspended tabs
   (`live_sessions_in` `:228`; unreadable suspended storage vetoes) and workflow
   run workspaces (`guard_run_workspace_base`).
4. Removal point `orphan_removal_guard` (`:1770`) then
   `remove_orphan_worktree_internal` (`tuic-git:2769`): refuses temp paths
   (`refuse_temporary_orphan` `:16501`), then
   `remove_worktree_internal_with_lock` (`:3014`) re-checks main worktree, presence
   vs. confirmation, missing registration, unregistered directory without warm
   token, in-progress Git operation, detached HEAD without a durable ref, dirty
   files, submodule state, fingerprint, and a lock (single `--force` never
   overrides a lock). Every check runs again at removal, not only at assessment.
5. Frontend (`createRepositoryRefreshCoordinator.ts:651-808`): safe orphans go
   without asking; unsafe ones go to the countdown dialog (ask) or only those with
   live sessions (on).

**Retry and restart.** There is no retry loop: each refresh tick reassesses from
scratch, so the retry bound is one attempt per tick and the operation is
idempotent (a removed checkout is simply absent next tick). Nothing about a pending
removal is persisted. After restart the scan runs again and an unsafe orphan is
asked about again; a safe one is removed again. This is safe: the verdict is
recomputed from disk, not remembered.

**Keep.** Orphan Keep is session-scoped memory (`keptOrphans`, :655), bound to
`dirty_fingerprint`; it holds until the checkout changes, and is lost on restart
(the user is asked again; no data is touched without an answer). The backend
mirrors a Keep so other clients close their dialog (`clear_orphan_cleanup_internal`
`:1937`). Covered by `orphanKeepFingerprintCritic.test.ts` and
`keeping_twice_stays_kept` etc. (`worktree.rs:2144`).

### Archive retention (`archiveRetentionDays`, 0 = off)

`sweep_archived_worktrees` (`tuic-git:5439`), run per refresh tick
(`createRepositoryRefreshCoordinator.ts:495`) through `sweep_archive_retention_impl`
(`src-tauri/src/worktree.rs:640`; the threshold comes from config, never the
caller). A candidate needs: registered, directly under `__archived/`, a recorded
`tuic-archive.json` at the canonical recorded path, allocation marker `finished`,
age past the threshold, no lock, no live session, no index flags, clean status
including ignored files, HEAD equal to the recorded tip, recovery ref still at the
tip. Removal is non-force, so Git re-checks dirt, locks and submodules at the
removal point. Retained archives return a reason (`ArchiveRetentionReport.kept`),
shown on the status line.

**Retry and restart.** The archive record and marker live in the git admin dir, so
they survive restart; ageing continues from `archived_at`. Each tick recomputes;
a refused removal is reported and tried again next tick (idempotent, no counter).
A legacy archive without a record is kept forever by design.

**Recovery refs.** `preserve_archive_tip` (`:3856`) writes `refs/archive/<branch>`
(or the flat fallback); retention never deletes refs or branches. Submodule refs
are preserved before removal (`preserve_submodule_refs`); a missing checkout's
modules before its registration is dropped (`preserve_missing_worktree_modules`).
Archive-time dirty content is not snapshotted (DEFERRED 2026-10-29 in the sweep);
the archive keeps the whole directory and the sweep keeps any dirty archive, so
dirty content is retained, not copied.

**Keep and retention.** Keep (orphan dialog, and the PR-notification Keep of story
2895 in `keptCheckouts.ts`) governs whether TUIC acts on a checkout. A kept
checkout is never archived, so it is never a retention candidate; retention has no
Keep of its own and needs none.

### Temporary test worktrees

Never orphan candidates (`temporary_detached_worktrees_are_never_orphan_candidates`) and never removable by cleanup, automatic or
confirmed (`src-tauri/src/worktree.rs:3628`). They stay registered until the
creator (a test teardown) removes them: TUIC cannot prove who owns a temp
checkout, and age or name is not proof. Missing temp checkouts keep their
registration for the same reason. Specified follow-up, not built: creators remove
their own worktree; a generic prune of temp registrations needs an ownership
record that does not exist today.

## 3. No-data-loss invariant

Before no-force removal of any orphan or archive, the removal point revalidates:
exact checkout (registered path, canonical path equals record), tip and base
(HEAD vs recorded tip; detached HEAD has a durable ref), dirty / untracked /
submodule state and fingerprint, index-flagged hidden edits, sessions (live and
suspended), locks, and an in-progress Git operation. Unknown ownership retains:
no registration without a warm token, no readable archive record, unreadable
suspended storage, or missing marker all refuse.

### Gap found and fixed

Orphan assessment and removal used plain `git status`, which skips edits hidden by
`skip-worktree` / `assume-unchanged`. Such an orphan was judged safe and removed
with `git worktree remove --force`; only the archive sweep checked the index.
`reject_index_flagged_files` (`tuic-git:5414`) is now shared by the sweep,
`orphan_cleanup_safety` and the no-force branch of
`remove_worktree_internal_with_lock`.
Test: `orphan_with_edits_hidden_by_index_flags_is_unsafe_and_never_removed`
(catches: hidden local configuration deleted with a clean-looking orphan). It
failed before the fix (`safe: true`).

### Open observations, not changed

- Ignored files (`.env`, build output) are not checked by orphan removal; the
  archive sweep keeps them. Blocking on them would stop nearly every orphan from
  being swept. Needs a Boss decision; not built.
- Orphan Keep is not persisted across restart (safe, re-asks).
- Merging remains manual; nothing here merges.

## 4. Test map

| Behaviour | Test |
|---|---|
| Every kind of unsaved work unsafe | `orphan_assessment_is_unsafe_for_every_kind_of_unsaved_work` |
| Hidden index-flag edits (new) | `orphan_with_edits_hidden_by_index_flags_is_unsafe_and_never_removed` |
| Detached HEAD without durable ref / in-progress op | `orphan_removal_refuses_a_detached_commit_without_a_durable_ref`, `orphan_removal_refuses_an_in_progress_operation` |
| Branch restored after discovery | `automatic_orphan_removal_rechecks_a_branch_restored_after_discovery` |
| Temporary worktree lifecycle | `temporary_detached_worktrees_are_never_orphan_candidates`, `temporary_orphan_removal_refuses_automatic_and_confirmed_requests` |
| Missing metadata | `orphan_whose_directory_is_gone_requires_explicit_removal_to_drop_entry`, `orphan_whose_directory_is_gone_stays_out_of_fingerprint_cleanup` |
| Retention age, ownership, availability, dirty, index flags | `archive_retention_honors_age_and_keeps_unrecoverable_dirty_files`, `archive_retention_requires_recorded_ownership_and_availability`, `archive_retention_keeps_local_edits_hidden_by_skip_worktree` |
| Keep | `orphanKeepFingerprintCritic.test.ts`, `keeping_twice_stays_kept` |
