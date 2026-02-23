---
id: 206-5452
title: Move orphan terminal detection to Rust
status: complete
priority: P3
created: "2026-02-16T17:25:18.070Z"
updated: "2026-02-16T17:47:19.928Z"
dependencies: []
---

# Move orphan terminal detection to Rust

## Problem Statement

Orphan terminal detection (nested loop across repos/branches) in src/hooks/useGitOperations.ts:226-239 and valid terminal filtering at line 111-113 are cross-store state consistency checks that belong in the backend.

## Acceptance Criteria

- [ ] Backend provides orphan terminal detection via Tauri command
- [ ] Frontend calls backend instead of doing cross-store traversal
- [ ] Terminal cleanup logic still works correctly
- [ ] Characterization test written BEFORE migration that passes with JS implementation
- [ ] Same test passes AFTER migration with identical results from Rust implementation

## Files

- src/hooks/useGitOperations.ts
- src-tauri/src/lib.rs

## Work Log

### 2026-02-16T17:47:19.860Z - Extracted findOrphanTerminals() to src/utils/terminalOrphans.ts. Pure function, no store deps. 6 new tests. Stays in frontend (terminal state is frontend-only).

