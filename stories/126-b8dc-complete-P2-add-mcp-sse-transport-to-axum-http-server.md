---
id: 126-b8dc
title: Add MCP SSE transport to axum HTTP server
status: complete
priority: P2
created: "2026-02-15T21:06:34.834Z"
updated: "2026-02-15T22:11:30.649Z"
dependencies: []
---

# Add MCP SSE transport to axum HTTP server

## Problem Statement

The MCP stdio bridge binary works but requires a separate process. Claude Code CLI supports SSE transport natively, so adding MCP SSE endpoints directly to the axum server eliminates the middleman for SSE-capable clients while keeping the stdio bridge for Codex and others.

## Acceptance Criteria

- [ ] GET /sse returns text/event-stream with endpoint event containing messages URL
- [ ] POST /messages accepts JSON-RPC requests and routes responses via SSE
- [ ] Implements initialize, tools/list, tools/call MCP methods
- [ ] special_key translation (enter, ctrl+c, tab, etc.)
- [ ] Claude Code can connect with url-based MCP config
- [ ] Existing REST API routes unchanged
- [ ] Tests for MCP protocol handling

## Files

- src-tauri/src/mcp_http.rs
- src-tauri/Cargo.toml

## Work Log

### 2026-02-15T22:11:30.377Z - Implemented MCP SSE transport (spec 2024-11-05) in mcp_http.rs: GET /sse returns text/event-stream with endpoint event + message stream; POST /messages?sessionId=xxx accepts JSON-RPC (initialize, tools/list, tools/call, notifications/initialized). All 20 tools mirror the stdio bridge. Added async-stream crate, mpsc channels per SSE session in AppState. 5 new tests (invalid session 404, valid session initialize, tools/list, tool definitions count, special key translation). All 75 Rust tests pass.

