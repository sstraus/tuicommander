---
id: 207-7498
title: Move exponential backoff calculation to Rust
status: complete
priority: P2
created: "2026-02-16T17:26:26.121Z"
updated: "2026-02-16T18:09:32.669Z"
dependencies: []
---

# Move exponential backoff calculation to Rust

## Problem Statement

Exponential backoff delay calculation with jitter lives in src/error-handler.ts:65-74. This retry algorithm is business logic that should be centralized in the Rust backend.

## Acceptance Criteria

- [ ] Backoff calculation moved to Rust
- [ ] Frontend calls Rust for retry delay
- [ ] Jitter and clamping behavior preserved
- [ ] Characterization test written BEFORE migration that passes with JS implementation
- [ ] Same test passes AFTER migration with identical results from Rust implementation

## Files

- src/error-handler.ts
- src-tauri/src/lib.rs

## Work Log

### 2026-02-16T18:09:32.602Z - Already implemented by story 200 agent: calculate_backoff_delay() in error_classification.rs with Tauri command. JS keeps sync mirror for synchronous ErrorHandler.handle() calls. 7 Rust tests + 24 TS tests pass.

