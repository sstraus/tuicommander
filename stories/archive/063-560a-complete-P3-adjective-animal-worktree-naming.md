---
id: "063-560a"
title: "Adjective-animal worktree naming"
status: complete
priority: P3
created: 2026-02-04T22:00:24.968Z
updated: 2026-02-04T22:00:24.968Z
dependencies: []
---

# Adjective-animal worktree naming

## Problem Statement

Worktree names are currently derived from task/branch names which can be boring or hard to remember. Fun adjective-animal names like proud-koala-313 are more memorable.

## Acceptance Criteria

- [ ] Add adjective and animal word lists in Rust backend
- [ ] Generate random name format: adjective-animal-NNN
- [ ] Ensure uniqueness by checking existing worktrees
- [ ] Allow user to override with custom name

## Files

- src-tauri/src/lib.rs

## Work Log

