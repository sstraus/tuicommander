---
id: 208-b5c3
title: Move PR review decision mapping to Rust
status: complete
priority: P3
created: "2026-02-16T17:26:26.123Z"
updated: "2026-02-16T17:47:19.787Z"
dependencies: []
---

# Move PR review decision mapping to Rust

## Problem Statement

PR review decision state mapping (APPROVED->Approved, CHANGES_REQUESTED->Changes requested, etc.) in src/components/PrDetailPopover/PrDetailPopover.tsx:61-74 is GitHub domain knowledge that belongs in the backend.

## Acceptance Criteria

- [ ] Review decision classification done in Rust
- [ ] Backend returns display-ready labels for review states
- [ ] Frontend uses backend-provided review labels
- [ ] Characterization test written BEFORE migration that passes with JS implementation
- [ ] Same test passes AFTER migration with identical results from Rust implementation

## Files

- src/components/PrDetailPopover/PrDetailPopover.tsx
- src-tauri/src/github.rs

## Work Log

### 2026-02-16T17:47:19.719Z - Already completed by story 204 - classify_review_state() exists in github.rs, frontend reads pre-computed review_state_label. 17 TS + 26 Rust tests pass.

