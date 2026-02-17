---
id: 177-733b
title: "Consolidate config system: generic loader and hydration utility"
status: wontfix
priority: P2
created: "2026-02-16T07:12:19.792Z"
updated: "2026-02-16T08:55:00.127Z"
dependencies: []
---

# Consolidate config system: generic loader and hydration utility

## Problem Statement

7 nearly identical config load/save command pairs in Rust. 8 stores duplicate identical hydration pattern with localStorage migration.

## Acceptance Criteria

- [ ] Single generic config loader parameterized by type and filename in Rust
- [ ] Extract shared hydrateStore<T>() utility for frontend stores
- [ ] Remove duplicated code

## Files

- src-tauri/src/config.rs
- src/stores/settings.ts
- src/stores/repositories.ts

## Related

- SIMP-01
- SIMP-02

## Work Log

### 2026-02-16T08:55:00.057Z - YAGNI: each store's config is slightly different, duplication is ~20 lines per store. Generic loader would be over-abstraction. Skip unless config bugs keep appearing.

