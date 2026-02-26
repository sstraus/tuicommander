---
id: "392-f764"
title: "Wire up worktree storage strategy setting (sibling __wt, appdir, inside-repo)"
status: pending
priority: P1
created: 2026-02-26T12:47:28.105Z
updated: 2026-02-26T12:47:28.105Z
dependencies: []
---

# Wire up worktree storage strategy setting (sibling __wt, appdir, inside-repo)

## Problem Statement

resolve_worktree_dir exists but is behind #[cfg(test)] and never called in production. lib.rs:512 hardcodes config_dir/worktrees regardless of the Storage Strategy setting. All 3 strategies (Sibling __wt, AppDir, InsideRepo) are dead.

## Acceptance Criteria

- [ ] Remove #[cfg(test)] from resolve_worktree_dir in worktree.rs:45
- [ ] create_worktree Tauri command reads per-repo effective storage strategy and calls resolve_worktree_dir
- [ ] create_pty_with_worktree also uses resolve_worktree_dir instead of state.worktrees_dir
- [ ] get_worktrees_dir becomes repo-aware so dialog preview shows correct path
- [ ] Sibling strategy creates reponame__wt/ next to the repo
- [ ] InsideRepo strategy creates .worktrees/ inside the repo
- [ ] AppDir strategy continues using config_dir/worktrees/{repo}

## Files

- src-tauri/src/worktree.rs
- src-tauri/src/lib.rs
- src-tauri/src/pty.rs
- src-tauri/src/config.rs

## Work Log

