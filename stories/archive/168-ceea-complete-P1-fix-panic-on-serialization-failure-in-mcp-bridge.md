---
id: 168-ceea
title: Fix panic on serialization failure in MCP bridge
status: complete
priority: P1
created: "2026-02-16T07:11:38.776Z"
updated: "2026-02-16T07:25:33.711Z"
dependencies: []
---

# Fix panic on serialization failure in MCP bridge

## Problem Statement

expect() on JSON serialization panics the bridge process. Should return JSON-RPC error response instead.

## Acceptance Criteria

- [ ] Replace expect() with match, return error response on failure
- [ ] No panics in production code paths

## Files

- src-tauri/src/bin/tui_mcp_bridge.rs

## Related

- RS-02

## Work Log

### 2026-02-16T07:25:33.647Z - Replaced expect() with match on serde_json::to_string, sends minimal JSON-RPC error response on failure

