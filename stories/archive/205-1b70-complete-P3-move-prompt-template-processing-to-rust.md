---
id: 205-1b70
title: Move prompt template processing to Rust
status: complete
priority: P3
created: "2026-02-16T17:25:18.070Z"
updated: "2026-02-16T17:39:18.248Z"
dependencies: []
---

# Move prompt template processing to Rust

## Problem Statement

Prompt template variable extraction and substitution logic in src/stores/promptLibrary.ts:251-269 is domain logic. Template syntax and processing rules should be defined in the backend.

## Acceptance Criteria

- [ ] Variable extraction (regex for {name} patterns) moved to Rust
- [ ] Template substitution moved to Rust
- [ ] Frontend calls Rust for template processing
- [ ] Characterization test written BEFORE migration that passes with JS implementation
- [ ] Same test passes AFTER migration with identical results from Rust implementation

## Files

- src/stores/promptLibrary.ts
- src-tauri/src/lib.rs

## Work Log

### 2026-02-16T17:39:18.163Z - Created src-tauri/src/prompt.rs with extract_variables/process_content using simple string parsing (no regex crate). Made TS store methods async calling Rust via invoke. Updated PromptDrawer for async calls. 14 Rust + all 1415 TS tests pass.

