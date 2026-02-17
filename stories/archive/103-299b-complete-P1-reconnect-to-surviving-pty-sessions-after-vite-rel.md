---
id: 103-299b
title: Reconnect to surviving PTY sessions after Vite reload
status: complete
priority: P1
created: "2026-02-15T11:16:50.550Z"
updated: "2026-02-15T17:12:08.743Z"
dependencies: []
---

# Reconnect to surviving PTY sessions after Vite reload

## Problem Statement

When Vite HMR reloads the frontend, terminalsStore is re-initialized empty (in-memory only, no persistence). All terminal-to-PTY session mappings are lost. The reconnection code in Terminal.tsx works correctly but never triggers because sessionId is always null after reload. Old PTY sessions become orphans in the Rust backend, leaking processes and file descriptors until the Tauri process exits.

## Acceptance Criteria

- [ ] Add Rust command list_active_sessions that returns all live PTY session IDs with metadata (cwd, creation time, worktree path if any)
- [ ] On frontend startup, query list_active_sessions from Rust backend to discover surviving PTY sessions
- [ ] Match surviving sessions to terminal tabs by cwd/worktree path and re-adopt them (set sessionId in terminalsStore, attach event listeners)
- [ ] For sessions that cannot be matched to any tab, either create a new tab for them or gracefully close them (prefer creating tabs to avoid killing user work)
- [ ] Verify reconnected terminals receive new output and scrollback is functional (scrollback before reload is acceptably lost)
- [ ] Add cleanup: if Rust reports sessions that the frontend did not re-adopt, close them to prevent orphan accumulation
- [ ] Test scenario: start 3 terminals, trigger Vite reload, verify all 3 reconnect without creating new shells

## Files

- src-tauri/src/lib.rs
- src/stores/terminals.ts
- src/components/Terminal/Terminal.tsx
- src/App.tsx

## Work Log

### 2026-02-15T17:12:08.670Z - Added cwd field to PtySession, list_active_sessions Rust command, frontend reconnection logic in App.tsx startup, tests pass (876/876)

