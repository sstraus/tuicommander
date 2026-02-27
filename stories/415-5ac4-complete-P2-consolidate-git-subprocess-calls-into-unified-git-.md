---
id: 415-5ac4
title: Consolidate git subprocess calls into unified git_cli module
status: complete
priority: P2
created: "2026-02-26T21:53:20.025Z"
updated: "2026-02-27T14:26:35.316Z"
dependencies: []
---

# Consolidate git subprocess calls into unified git_cli module

## Problem Statement

All git operations in src-tauri/src use Command::new(resolve_cli("git")) scattered across multiple files (git.rs, worktree.rs, agent.rs, etc.) with no consistent error handling, PATH resolution, or output parsing strategy. This makes the codebase brittle and hard to maintain.

## Acceptance Criteria

- [ ] Create src-tauri/src/git_cli.rs module with a single run_git() helper that wraps Command::new(resolve_cli("git")), captures stdout/stderr, and returns a typed Result<String, GitError>
- [ ] All existing Command::new(resolve_cli("git")) calls in git.rs, worktree.rs, agent.rs, and any other Rust files are migrated to use run_git()
- [ ] GitError type covers: spawn failure, non-zero exit (with stderr), utf8 decode error
- [ ] No behavior changes — all existing Tauri commands return identical results
- [ ] All existing Rust tests pass; add unit tests for run_git() error paths

## Files

- src-tauri/src/git_cli.rs
- src-tauri/src/git.rs
- src-tauri/src/worktree.rs
- src-tauri/src/agent.rs
- src-tauri/src/lib.rs

## QA

None — covered by tests

## Work Log

### 2026-02-27T11:29:02.690Z - Deferred: 34 callsites across 3 files with varied error handling patterns. Needs careful dedicated session, not batch mode.

### 2026-02-27T14:26:30.030Z - Completed: Created git_cli.rs module with GitCmd builder (git_cmd, run, run_silent, run_raw). Migrated all 55+ callsites across git.rs (14), worktree.rs (34), github.rs (1), fs.rs (7), worktree_routes.rs (1). All 746 tests pass, 0 warnings. Net -430 lines.

