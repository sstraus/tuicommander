---
id: 001-7fa6
title: Git worktree management per task
status: complete
priority: P1
created: "2026-02-04T10:50:24.102Z"
updated: "2026-02-04T10:59:39.606Z"
dependencies: []
---

# Git worktree management per task

## Problem Statement

Each agent task needs isolated git worktree to avoid conflicts. Currently all terminals share same working directory. Need automatic worktree creation/cleanup when spawning agent tasks.

## Acceptance Criteria

- [ ] Create worktree for new task in ../worktrees/{task-name}
- [ ] Track worktree-to-terminal mapping
- [ ] Cleanup worktree on task completion
- [ ] Handle worktree conflicts gracefully

## Files

- src-tauri/src/lib.rs
- src/main.ts

## Work Log

