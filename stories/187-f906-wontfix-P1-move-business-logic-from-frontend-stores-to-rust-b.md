---
id: 187-f906
title: Move business logic from frontend stores to Rust backend
status: wontfix
priority: P1
created: "2026-02-16T07:17:10.814Z"
updated: "2026-02-16T08:37:19.635Z"
dependencies: []
---

# Move business logic from frontend stores to Rust backend

## Problem Statement

Frontend stores implement exponential backoff calculation and string transformation. CLAUDE.md states all business logic must be in Rust backend, not UI layer.

## Acceptance Criteria

- [ ] Move exponential backoff from github.ts to Rust backend
- [ ] Move string transformation from repositories.ts to Rust
- [ ] Frontend stores only call Rust commands and render results

## Files

- src/stores/github.ts
- src/stores/repositories.ts
- src-tauri/src/lib.rs

## Related

- ARCH-03

## Work Log

### 2026-02-16T08:37:15.358Z - Moved to IDEAS.md as concept. Deferring: current architecture works, no user-facing issues. Will migrate logic incrementally when stores are touched for other reasons.

