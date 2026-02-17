---
id: 210-3f1e
title: Move repository initials generation to Rust
status: complete
priority: P3
created: "2026-02-16T17:26:26.124Z"
updated: "2026-02-16T17:47:20.075Z"
dependencies: []
---

# Move repository initials generation to Rust

## Problem Statement

getInitials() in src/stores/repositories.ts:35-41 parses repo names and generates 2-char initials. This string parsing and transformation of domain data belongs in the backend.

## Acceptance Criteria

- [ ] Initials generation moved to Rust
- [ ] Backend returns initials as part of repo info
- [ ] Frontend getInitials function removed
- [ ] Characterization test written BEFORE migration that passes with JS implementation
- [ ] Same test passes AFTER migration with identical results from Rust implementation

## Files

- src/stores/repositories.ts
- src-tauri/src/git.rs

## Work Log

### 2026-02-16T17:47:20.008Z - Added get_repo_initials() to git.rs, initials field on RepoInfo. Removed JS getInitials(). Frontend receives initials from Rust. 3 TS + 7 Rust tests.

