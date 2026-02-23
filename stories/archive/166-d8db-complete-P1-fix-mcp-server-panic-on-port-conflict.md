---
id: 166-d8db
title: Fix MCP server panic on port conflict
status: complete
priority: P1
created: "2026-02-16T07:11:38.775Z"
updated: "2026-02-16T07:25:33.585Z"
dependencies: []
---

# Fix MCP server panic on port conflict

## Problem Statement

expect() on address binding panics the entire app if the port is already in use, making the app unusable.

## Acceptance Criteria

- [ ] Return error gracefully instead of panicking
- [ ] Log meaningful error message
- [ ] Allow user to configure different port or retry

## Files

- src-tauri/src/mcp_http.rs

## Related

- SF-02

## Work Log

### 2026-02-16T07:25:33.523Z - Replaced expect() with match block on local_addr(), returns gracefully instead of panicking

