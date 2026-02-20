---
id: "264-7c77"
title: "git CLI not using resolve_cli - may fail when launched from GUI"
status: pending
priority: P1
created: 2026-02-20T07:38:38.167Z
updated: 2026-02-20T07:38:38.167Z
dependencies: []
---

# git CLI not using resolve_cli - may fail when launched from GUI

## Problem Statement

git.rs, worktree.rs, and lib.rs call Command::new("git") at ~15 call sites without resolve_cli(). On macOS with Homebrew git, /usr/bin/git is a shim and /opt/homebrew/bin/git is preferred. When launched from Finder without full PATH, git operations can silently fail.

## Acceptance Criteria

- [ ] All Command::new("git") calls use resolve_cli("git")
- [ ] Git operations work correctly in release build launched from Finder
- [ ] Worktree operations work correctly in release build

## Files

- src-tauri/src/git.rs
- src-tauri/src/worktree.rs
- src-tauri/src/lib.rs

## Work Log

