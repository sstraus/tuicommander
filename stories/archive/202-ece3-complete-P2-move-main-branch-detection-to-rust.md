---
id: 202-ece3
title: Move main branch detection to Rust
status: complete
priority: P2
created: "2026-02-16T17:25:18.069Z"
updated: "2026-02-16T17:36:40.262Z"
dependencies: []
---

# Move main branch detection to Rust

## Problem Statement

isMainBranch() in src/stores/repositories.ts:43-47 defines which branches are main/master/develop. This domain definition risks diverging from any backend branch logic.

## Acceptance Criteria

- [ ] Main branch detection defined once in Rust
- [ ] Frontend queries backend for is-main-branch or receives it as a field
- [ ] Duplicate JS isMainBranch function removed
- [ ] Characterization test written BEFORE migration that passes with JS implementation
- [ ] Same test passes AFTER migration with identical results from Rust implementation

## Files

- src/stores/repositories.ts
- src-tauri/src/git.rs

## Work Log

### 2026-02-16T17:36:40.198Z - Added is_main_branch() to Rust git.rs, check_is_main_branch Tauri command, is_main field in get_git_branches response. Exported JS isMainBranch for testing. 8 TS tests + 3 Rust tests pass.

