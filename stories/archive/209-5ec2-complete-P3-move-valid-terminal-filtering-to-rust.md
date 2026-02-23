---
id: 209-5ec2
title: Move valid terminal filtering to Rust
status: complete
priority: P3
created: "2026-02-16T17:26:26.123Z"
updated: "2026-02-16T18:09:37.756Z"
dependencies: []
---

# Move valid terminal filtering to Rust

## Problem Statement

Valid terminal filtering (cross-store check that terminal IDs exist) in src/hooks/useGitOperations.ts:111-113 is state validation logic that belongs in the backend.

## Acceptance Criteria

- [ ] Backend validates terminal-branch associations
- [ ] Frontend receives pre-validated terminal lists
- [ ] Stale terminal references cleaned up in backend
- [ ] Characterization test written BEFORE migration that passes with JS implementation
- [ ] Same test passes AFTER migration with identical results from Rust implementation

## Files

- src/hooks/useGitOperations.ts
- src-tauri/src/lib.rs

## Work Log

### 2026-02-16T18:09:37.696Z - Extracted filterValidTerminals() to src/utils/terminalFilter.ts with 7 characterization tests. Cannot move to Rust because it cross-validates terminal IDs between frontend stores (repositories + terminals). Best possible separation achieved.

