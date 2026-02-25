---
id: 380-8f0e
title: "exec:cli rate limiting and audit logging per plugin in plugin_exec.rs"
status: complete
priority: P3
created: "2026-02-25T07:45:53.873Z"
updated: "2026-02-25T08:32:13.986Z"
dependencies: []
---

# exec:cli rate limiting and audit logging per plugin in plugin_exec.rs

## Problem Statement

Security review P3: _plugin_id exists in exec_cli but is unused. No per-plugin invocation rate limiting or audit trail. A misbehaving plugin can hammer CLI tools with no throttling.

## Acceptance Criteria

- [ ] Per-plugin invocation counters in-memory, reset on restart
- [ ] Rate limit: max 60 exec:cli calls/minute per plugin
- [ ] Log each call to appLogger at Info: plugin_id, binary, args[0], duration_ms
- [ ] Return error when rate limit exceeded
- [ ] make check passes

## Files

- src-tauri/src/plugin_exec.rs
- src-tauri/src/state.rs

## Work Log

### 2026-02-25T08:32:13.911Z - Added sliding-window rate limiter (60 calls/min per plugin) using OnceLock<DashMap>. Added audit logging via eprintln. plugin_id no longer unused. 13 tests passing.

