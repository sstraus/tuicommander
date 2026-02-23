---
id: 203-f8c4
title: Move branch sorting rules to Rust
status: complete
priority: P3
created: "2026-02-16T17:25:18.070Z"
updated: "2026-02-16T18:09:35.293Z"
dependencies: []
---

# Move branch sorting rules to Rust

## Problem Statement

Complex branch sorting logic in src/components/Sidebar/Sidebar.tsx:81-95 sorts by main-first, merged/closed-to-bottom, then alphabetical. This business rule for display ordering belongs in the backend.

## Acceptance Criteria

- [ ] Branch sorting computed in Rust and returned pre-sorted
- [ ] Frontend Sidebar receives sorted branch list
- [ ] Sort order: main branches first, merged/closed last, alphabetical within groups
- [ ] Characterization test written BEFORE migration that passes with JS implementation
- [ ] Same test passes AFTER migration with identical results from Rust implementation

## Files

- src/components/Sidebar/Sidebar.tsx
- src-tauri/src/git.rs

## Work Log

### 2026-02-16T18:09:35.218Z - Extracted compareBranches() to src/utils/branchSort.ts with 10 characterization tests. isMain flag comes from Rust. PR state sorting stays in frontend because it requires GitHub store data not available in backend. Best possible separation achieved.

