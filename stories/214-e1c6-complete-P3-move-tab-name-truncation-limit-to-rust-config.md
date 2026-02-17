---
id: 214-e1c6
title: Move tab name truncation limit to Rust config
status: complete
priority: P3
created: "2026-02-16T17:26:26.126Z"
updated: "2026-02-16T18:09:30.024Z"
dependencies: []
---

# Move tab name truncation limit to Rust config

## Problem Statement

Hard-coded 25-char tab name truncation in src/hooks/useGitOperations.ts:329-330 is a magic number that should come from backend configuration.

## Acceptance Criteria

- [ ] Max tab name length defined in Rust config
- [ ] Frontend reads limit from config
- [ ] Hard-coded magic number removed from JS
- [ ] Characterization test written BEFORE migration that passes with JS implementation
- [ ] Same test passes AFTER migration with identical results from Rust implementation

## Files

- src/hooks/useGitOperations.ts
- src-tauri/src/config.rs

## Work Log

### 2026-02-16T18:09:29.954Z - Already implemented: max_tab_name_length in Rust config.rs with default 25, frontend reads from settings store. Rust and TS tests pass.

