---
id: 357-4512
title: Move isMainBranch() determination to Rust backend
status: complete
priority: P2
created: "2026-02-22T16:16:43.685Z"
updated: "2026-02-23T08:08:20.177Z"
dependencies: []
---

# Move isMainBranch() determination to Rust backend

## Problem Statement

isMainBranch() in TypeScript duplicates the branch-classification logic that already exists in the Rust backend. Having two implementations risks diverging. Branch analysis belongs in Rust with actual git data.

## Acceptance Criteria

- [ ] Rust exposes an is_main_branch Tauri command or includes isMain in existing branch data responses
- [ ] TypeScript isMainBranch() utility removed
- [ ] All call sites updated to use the backend field
- [ ] Tests updated accordingly

## Files

- src/utils/branchUtils.ts
- src-tauri/src/git.rs
- src-tauri/src/lib.rs

## Work Log

### 2026-02-23T07:52:05.196Z - Triaged: implement now

### 2026-02-23T08:08:20.252Z - Made isMainBranch internal, tests use setBranch behavior, Rust is_main already in branch data

