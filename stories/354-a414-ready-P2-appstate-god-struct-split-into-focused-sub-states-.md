---
id: 354-a414
title: "AppState god struct: split into focused sub-states (sessions, pty, config, github)"
status: ready
priority: P2
created: "2026-02-22T16:16:43.682Z"
updated: "2026-02-23T07:49:38.237Z"
dependencies: []
---

# AppState god struct: split into focused sub-states (sessions, pty, config, github)

## Problem Statement

AppState in state.rs holds all application state in one struct (sessions, pty data, GitHub tokens, config, ring buffers, etc.). This makes it hard to reason about, causes excessive locking scope, and slows future refactoring.

## Acceptance Criteria

- [ ] Sessions and pty-related state extracted to a SessionStore sub-struct
- [ ] Config state isolated behind its own accessor to minimize lock scope
- [ ] GitHub-related state (tokens, rate limit) isolated in GitHubState sub-struct
- [ ] All existing Tauri commands and HTTP routes continue to work after refactor
- [ ] cargo check passes with no new warnings

## Files

- src-tauri/src/state.rs
- src-tauri/src/lib.rs
- src-tauri/src/mcp_http/mod.rs

## Work Log

