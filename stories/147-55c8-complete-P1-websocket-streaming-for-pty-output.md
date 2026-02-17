---
id: 147-55c8
title: WebSocket streaming for PTY output
status: complete
priority: P1
created: "2026-02-15T23:38:45.007Z"
updated: "2026-02-16T00:10:06.483Z"
dependencies: ["145-5f38"]
---

# WebSocket streaming for PTY output

## Problem Statement

Browser clients cannot use Tauri listen() for real-time PTY data. The existing polling endpoint (GET /sessions/{id}/output) is too laggy for interactive terminals. Need WebSocket streaming.

## Acceptance Criteria

- [ ] Add GET /sessions/{id}/stream route with WebSocket upgrade
- [ ] Broadcast PTY output to connected WebSocket clients in spawn_reader_thread
- [ ] Add ws_clients DashMap to AppState for per-session WebSocket senders
- [ ] WebSocket also accepts write messages (bidirectional: read PTY output, send PTY input)
- [ ] Clean up WebSocket channels on disconnect or session close
- [ ] Test WebSocket connection lifecycle

## Files

- src-tauri/src/mcp_http.rs
- src-tauri/src/lib.rs

## Work Log

### 2026-02-16T00:10:06.418Z - Added WebSocket streaming route /sessions/{id}/stream, bidirectional PTY I/O, ws_clients DashMap in AppState, broadcast from both reader threads, cleanup on disconnect/close, initial ring buffer catch-up, 3 new tests. All 106 Rust tests pass.

