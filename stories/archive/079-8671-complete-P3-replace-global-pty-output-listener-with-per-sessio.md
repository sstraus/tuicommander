---
id: 079-8671
title: Replace global pty-output listener with per-session event channels
status: complete
priority: P3
created: "2026-02-08T09:44:21.530Z"
updated: "2026-02-08T09:53:45.979Z"
dependencies: []
---

# Replace global pty-output listener with per-session event channels

## Problem Statement

Every Terminal component subscribes to the global pty-output event and filters by session_id. With N terminals open, every PTY output event triggers N listener invocations, N-1 of which discard the data. This is O(N) per event. At 50+ concurrent sessions this becomes a measurable performance bottleneck under high-throughput output.

## Acceptance Criteria

- [ ] Backend emits pty-output-{session_id} instead of global pty-output
- [ ] Frontend subscribes to the specific session channel via listen(pty-output-{sessionId})
- [ ] Each terminal only receives its own output with no filtering needed
- [ ] Verify: with 10+ terminals open, high-throughput output in one should not affect others

## Files

- src-tauri/src/lib.rs (reader thread emit call)
- src/components/Terminal/Terminal.tsx (listen call in initSession)

## Related

- 066-3a86

## Work Log

