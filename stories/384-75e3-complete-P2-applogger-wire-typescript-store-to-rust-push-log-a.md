---
id: 384-75e3
title: "appLogger: wire TypeScript store to Rust push_log and get_logs commands"
status: complete
priority: P2
created: "2026-02-25T17:53:56.048Z"
updated: "2026-02-25T19:39:59.273Z"
dependencies: ["383-de49"]
---

# appLogger: wire TypeScript store to Rust push_log and get_logs commands

## Problem Statement

Implement: appLogger: wire TypeScript store to Rust push_log and get_logs commands

## Acceptance Criteria

- [ ] Implement as described

## Work Log

### 2026-02-25T17:54:22.089Z - Scope: appLogger.push() fire-and-forget to invoke(push_log). getEntries() fetches from invoke(get_logs) async, called only when panel opens. clear() calls invoke(clear_logs). Remove JS ring buffer array - Rust is source of truth. Keep unseenErrorCount and revision signals. Queue entries locally before Tauri ready, drain on first success. Depends on 383-de49. Files: src/stores/appLogger.ts, tests.

### 2026-02-25T19:39:59.340Z - Wired appLogger to Rust backend: fire-and-forget push_log, hydration from get_logs with dedup, clear_logs on clear(). Pre-Tauri queue drains on first success. 18 vitest tests. Commit 0ee4542.

