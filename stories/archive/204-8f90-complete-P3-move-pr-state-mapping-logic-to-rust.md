---
id: 204-8f90
title: Move PR state mapping logic to Rust
status: complete
priority: P3
created: "2026-02-16T17:25:18.070Z"
updated: "2026-02-16T17:39:07.693Z"
dependencies: []
---

# Move PR state mapping logic to Rust

## Problem Statement

PR merge state mapping (CONFLICTING->Conflicts, CLEAN->Ready to merge) and review decision mapping in src/components/PrDetailPopover/PrDetailPopover.tsx:32-74 is GitHub domain knowledge that belongs in the backend.

## Acceptance Criteria

- [ ] PR state classification done in Rust
- [ ] Backend returns display-ready labels and CSS classes for PR states
- [ ] Frontend PrDetailPopover uses backend-provided state labels
- [ ] Characterization test written BEFORE migration that passes with JS implementation
- [ ] Same test passes AFTER migration with identical results from Rust implementation

## Files

- src/components/PrDetailPopover/PrDetailPopover.tsx
- src-tauri/src/github.rs

## Work Log

### 2026-02-16T17:39:07.600Z - Added classify_merge_state/classify_review_state to Rust github.rs with StateLabel struct. Pre-computed labels in BranchPrStatus during parse_pr_list_json. Extracted TS utils as fallback. PrDetailPopover now reads pre-computed fields. 17 TS + 16 Rust tests pass.

