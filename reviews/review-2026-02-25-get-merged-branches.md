# Code Review: get_merged_branches (uncommitted changes on main)
**Date:** 2026-02-25
**Reviewers:** Multi-Agent (security, performance, architecture, simplicity, silent-failure, test-quality, rust, typescript)
**Target:** Uncommitted changes in `src-tauri/src/git.rs`, `src-tauri/src/lib.rs`, `src/stores/repositories.ts`
**Confidence Threshold:** 70

## Summary
- **P1 Critical Issues:** 2
- **P2 Important Issues:** 6
- **P3 Nice-to-Have:** 5
- **Filtered Out (below threshold):** 0

---

## P1 - Critical (Block Merge)

- [ ] **#1 [TEST]** Test assertion is semantically wrong — will fail on any non-main branch (Confidence: 97)
  - Location: `src-tauri/src/git.rs:1001`
  - Issue: The test asserts `merged.contains(current_branch)`, but the implementation runs `git branch --merged <main_branch>`, not `--merged HEAD`. On any unmerged feature branch, the current branch is NOT in `--merged main`, so this assertion fails. The test will break in CI on every feature branch.
  - Fix: Assert that the main branch itself is in the merged list (it always is), or guard the assertion with `if is_main_branch(branch)`
  - Agent: test-quality-reviewer

- [ ] **#2 [ARCH]** Main-branch candidate list diverges from the canonical `is_main_branch()` (Confidence: 97)
  - Location: `src-tauri/src/git.rs:501` vs `src-tauri/src/git.rs:467`
  - Issue: `get_merged_branches_impl` probes `["main", "master", "develop"]` but the authoritative `is_main_branch()` also recognizes `"development"` and `"dev"`. A repo using `dev` as its default branch will be correctly identified as main elsewhere but silently fail merged-branch detection. Three divergent copies of this list now exist (two in Rust, one in TS).
  - Fix: Use the same candidate set as `is_main_branch()`, or better yet, extract the candidate list into a shared const
  - Agent: architecture-reviewer

---

## P2 - Important (Fix Before/After Merge)

- [ ] **#3 [PERF]** No TTL cache — `get_merged_branches` re-runs subprocesses on every repo-changed event (Confidence: 92)
  - Location: `src-tauri/src/git.rs:527`
  - Issue: `get_repo_info` uses `AppState::get_cached` with a 5s TTL. `get_merged_branches` has no cache despite spawning up to 4 subprocesses. On a 10-repo workspace, a single `git commit` triggers 40+ processes within 500ms.
  - Fix: Add a `merged_branches_cache: DashMap<String, (Vec<String>, Instant)>` to `AppState` and wrap with the existing TTL pattern
  - Agent: performance-reviewer

- [ ] **#4 [SILENT]** `.ok()?` collapses spawn errors and missing-branch into the same `None`, then fallback to "main" masks both (Confidence: 92)
  - Location: `src-tauri/src/git.rs:506-508`
  - Issue: If `git` binary isn't found, `.ok()?` silently swallows it, falls back to "main", and the subsequent `git branch --merged main` fails with a misleading error. For repos with non-standard main branches (e.g. `trunk`), the fallback also produces a confusing downstream error.
  - Fix: Return `Err(...)` early on spawn failure; use `.ok_or_else(...)? ` instead of `.unwrap_or_else(...)` when no candidate branch is found
  - Agent: silent-failure-hunter

- [ ] **#5 [TYPE]** Missing hydration migration for `isMerged` — persisted branches have `undefined` despite required `boolean` type (Confidence: 92)
  - Location: `src/stores/repositories.ts:182-191`
  - Issue: Every other boolean field added to `BranchState` after initial release has a migration guard in `hydrate()`. `isMerged` skips this, so persisted branches have `isMerged: undefined` at runtime until `refreshAllBranchStats` fires. The TS type says `boolean` but the runtime value is `undefined`.
  - Fix: Add `if (branch.isMerged === undefined) { branch.isMerged = false; }` in the hydration loop
  - Agent: typescript-reviewer

- [ ] **#6 [TEST]** Test passes vacuously on detached HEAD (Confidence: 92)
  - Location: `src-tauri/src/git.rs:1000`
  - Issue: When HEAD is detached (common in CI), `read_branch_from_head` returns `None`, the `if let` guard is skipped, and the test makes zero assertions. It only verifies the function doesn't panic.
  - Fix: Add a fallback assertion for detached HEAD (e.g., `assert!(!merged.is_empty())`)
  - Agent: test-quality-reviewer

- [ ] **#7 [TEST]** No negative test cases — non-git directory and invalid path (Confidence: 90)
  - Location: `src-tauri/src/git.rs` (missing tests)
  - Issue: The function's error paths (non-existent path, non-git directory) have zero test coverage.
  - Fix: Add `get_merged_branches_rejects_nonexistent_path` and `get_merged_branches_rejects_non_git_directory` tests
  - Agent: test-quality-reviewer

- [ ] **#8 [PERF]** Up to 3 sequential subprocess spawns for main branch detection — replaceable with file I/O (Confidence: 85)
  - Location: `src-tauri/src/git.rs:501-508`
  - Issue: The file already has `resolve_git_dir()` and `read_branch_from_head()` helpers that do zero-subprocess I/O. The main branch probe could check `refs/heads/<name>` existence via `Path::exists()` instead of spawning `git rev-parse`.
  - Fix: Replace subprocess probing with `resolve_git_dir(repo_path)?.join("refs/heads").join(name).exists()`
  - Agent: performance-reviewer, simplicity-reviewer

---

## P3 - Nice-to-Have

- [ ] **#9 [RUST]** Doc comment says "Returns a set" but returns `Vec` (Confidence: 98)
  - Location: `src-tauri/src/git.rs:498`
  - Fix: Change to "Returns branch names whose tips are reachable..."
  - Agent: rust-reviewer

- [ ] **#10 [RUST]** Unnecessary `String` allocations for branch candidates (Confidence: 90)
  - Location: `src-tauri/src/git.rs:507-508`
  - Issue: `.to_string()` allocates heap Strings; `&'static str` suffices since the value is only passed to `.args()`
  - Fix: Use `Some(*name)` / `.unwrap_or("main")` with `&str` instead of `String`
  - Agent: rust-reviewer

- [ ] **#11 [ARCH]** Parameter named `repo_path` breaks the `path` convention used by all other git commands (Confidence: 80)
  - Location: `src-tauri/src/git.rs:527`
  - Fix: Rename to `path: String` for IPC consistency
  - Agent: architecture-reviewer

- [ ] **#12 [RUST]** Test anti-pattern: `assert!(is_ok())` then `unwrap()` — use `expect()` (Confidence: 85)
  - Location: `src-tauri/src/git.rs:996-997`
  - Fix: `let merged = result.expect("should succeed on real repo");`
  - Agent: rust-reviewer

- [ ] **#13 [SILENT]** Frontend catch block swallows errors as empty array — caller can't distinguish failure from "nothing merged" (Confidence: 82)
  - Location: `src/hooks/useRepository.ts:168-174`
  - Fix: Return `null` on error to distinguish from empty success
  - Agent: silent-failure-hunter

---

## Cross-Cutting Analysis

### Root Causes Identified

| Root Cause | Findings Affected | Suggested Fix |
|------------|-------------------|---------------|
| Main branch candidate list not centralized | #2, #4 | Extract `MAIN_BRANCH_CANDIDATES` const, reuse in `is_main_branch()` and `get_merged_branches_impl()` |
| No caching pattern applied to new command | #3 | Add `merged_branches_cache` to `AppState` with same TTL as `repo_info_cache` |
| Branch detection uses subprocesses instead of file I/O | #4, #8 | Use `resolve_git_dir` + `Path::exists()` instead of `git rev-parse` |
| Test logic mismatch with implementation semantics | #1, #6, #7 | Rewrite test with correct assertion and add negative cases |

### Single-Fix Opportunities

1. **Centralize main branch candidates** — Fixes #2 and #4 (~5 lines: extract const, update both callsites)
2. **Replace subprocess probing with file I/O** — Fixes #4 and #8 (~10 lines: use `resolve_git_dir` + `exists()`)
3. **Rewrite test** — Fixes #1, #6, #7 (~25 lines: correct assertion, detached HEAD handling, negative cases)

### Context Files (Read Before Fixing)

| File | Reason | Referenced By |
|------|--------|---------------|
| `src-tauri/src/git.rs:466-472` | `is_main_branch()` — the canonical main branch list | architecture, simplicity |
| `src-tauri/src/git.rs:17-53` | `resolve_git_dir` + `read_branch_from_head` — zero-subprocess alternatives | performance, simplicity, rust |
| `src-tauri/src/state.rs` | TTL cache infrastructure (`AppState::get_cached`, `GIT_CACHE_TTL`) | performance |
| `src/hooks/useGitOperations.ts:61-73` | Call site — fans out across all repos on every repo-changed event | performance |
| `src/hooks/useRepository.ts:167-174` | Frontend wrapper with silent error swallowing | silent-failure |

---

## Security Note

The security reviewer flagged that `repo_path` is not validated via `validate_path_string()`. However, this is a **pre-existing systemic pattern** — no Tauri git commands validate paths (they rely on the frontend only sending paths from the user's own repo list). The new command is consistent with the existing codebase. This was noted but filtered as a pre-existing concern, not a regression introduced by this diff.

---

## Simplicity Note

The simplicity reviewer confirmed that:
- The `_impl` / Tauri wrapper split is **consistent** with `get_repo_info_impl`, `rename_branch_impl`, etc. — not unnecessary indirection
- The `isMerged` field is **fully wired** end-to-end (`useRepository.ts` → `useGitOperations.ts` → `RepoSection.tsx`) — not dead code
- The separate command (vs. integrating into `get_git_branches`) is a **reasonable separation of concerns** since merged-status computation is heavier than branch listing
