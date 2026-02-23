---
id: 348-08ad
title: "Browser mode: notes load/save HTTP API"
status: ready
priority: P2
created: "2026-02-21T20:34:55.185Z"
updated: "2026-02-23T07:52:04.346Z"
dependencies: []
---

# Browser mode: notes load/save HTTP API

## Problem Statement

load_notes and save_notes have no HTTP mappings. load_notes is called at app hydration in notes.ts — failure silently results in no notes loaded. save_notes fails silently too. Notes data is inaccessible in browser mode.

## Acceptance Criteria

- [ ] Add GET /config/notes and PUT /config/notes routes to Rust HTTP server
- [ ] Add transport.ts mappings for load_notes and save_notes
- [ ] Notes load correctly at hydration in browser mode
- [ ] Notes save correctly in browser mode

## Files

- src/stores/notes.ts
- src/transport.ts
- src-tauri/src/mcp_http/mod.rs

## Work Log

### 2026-02-23T07:52:04.269Z - Triaged: implement now

