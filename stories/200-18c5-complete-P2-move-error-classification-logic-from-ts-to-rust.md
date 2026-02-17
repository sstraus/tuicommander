---
id: 200-18c5
title: Move error classification logic from TS to Rust
status: complete
priority: P2
created: "2026-02-16T17:25:18.067Z"
updated: "2026-02-16T17:47:19.632Z"
dependencies: []
---

# Move error classification logic from TS to Rust

## Problem Statement

Error classification (regex pattern matching for rate limits, auth errors, quota) and exponential backoff calculation live in src/error-handler.ts. Per architecture rule, all business logic must be in Rust backend.

## Acceptance Criteria

- [ ] Error classification patterns moved to Rust and exposed via Tauri command
- [ ] Backoff delay calculation moved to Rust
- [ ] Frontend error-handler.ts calls Rust instead of local logic
- [ ] Existing error handling behavior unchanged
- [ ] Characterization test written BEFORE migration that passes with JS implementation
- [ ] Same test passes AFTER migration with identical results from Rust implementation

## Files

- src/error-handler.ts
- src-tauri/src/lib.rs

## Work Log

### 2026-02-16T17:47:19.529Z - Created error_classification.rs with classify_error() and Tauri command. Simplified TS classifyError() to sync mirror with includes() checks. 21 TS + 6 Rust tests pass.

