---
id: 211-9981
title: Move settings default comparison to Rust
status: complete
priority: P3
created: "2026-02-16T17:26:26.124Z"
updated: "2026-02-16T17:39:31.657Z"
dependencies: []
---

# Move settings default comparison to Rust

## Problem Statement

hasCustomSettings() in src/stores/repoSettings.ts:122-133 compares repo settings against defaults to determine if customized. The definition of defaults and what counts as custom should be centralized in the backend.

## Acceptance Criteria

- [ ] Backend exposes has_custom_settings check
- [ ] Default values defined once in Rust config
- [ ] Frontend calls backend instead of local comparison
- [ ] Characterization test written BEFORE migration that passes with JS implementation
- [ ] Same test passes AFTER migration with identical results from Rust implementation

## Files

- src/stores/repoSettings.ts
- src-tauri/src/config.rs

## Work Log

### 2026-02-16T17:39:31.589Z - Added has_custom_settings() to RepoSettingsEntry in Rust. Fixed Default impl for correct base_branch. check_has_custom_settings Tauri command. Frontend hasCustomSettings now async via invoke. 8 TS + 7 Rust tests pass.

