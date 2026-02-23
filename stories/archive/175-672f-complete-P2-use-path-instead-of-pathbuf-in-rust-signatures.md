---
id: 175-672f
title: Use &Path instead of &PathBuf in Rust signatures
status: complete
priority: P2
created: "2026-02-16T07:12:19.791Z"
updated: "2026-02-16T07:34:24.595Z"
dependencies: []
---

# Use &Path instead of &PathBuf in Rust signatures

## Problem Statement

Non-idiomatic Rust: functions take &PathBuf instead of &Path throughout. PathBuf is to Path as String is to str.

## Acceptance Criteria

- [ ] Change function signatures to accept &Path
- [ ] Fix all call sites

## Files

- src-tauri/src/lib.rs

## Related

- RS-06

## Work Log

### 2026-02-16T07:34:24.521Z - Changed 2 occurrences: create_worktree_internal(worktrees_dir: &PathBuf) and walk_dir(dir: &PathBuf, base: &PathBuf) to use &Path. Added Path to import.

