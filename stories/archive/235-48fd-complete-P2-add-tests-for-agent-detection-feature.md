---
id: 235-48fd
title: Add tests for agent detection feature
status: complete
priority: P2
created: "2026-02-17T10:28:43.735Z"
updated: "2026-02-17T10:28:59.397Z"
dependencies: []
---

# Add tests for agent detection feature

## Problem Statement

New foreground agent detection feature needs test coverage across Rust and TypeScript.

## Acceptance Criteria

- [ ] 6 Rust tests for classify_agent (all 5 agents + unknown processes)
- [ ] 5 TypeScript tests for useAgentPolling (polling, no-active, no-session, null, errors)
- [ ] 3 tests for agentType in terminals store (init, update, clear)
- [ ] 1 transport mapping test for get_session_foreground_process

## Files

- src-tauri/src/pty.rs
- src/__tests__/hooks/useAgentPolling.test.ts
- src/__tests__/stores/terminals.test.ts
- src/__tests__/transport.test.ts

## Work Log

### 2026-02-17T10:28:59.328Z - Added 6 Rust classify_agent tests, 5 useAgentPolling tests, 3 terminals store agentType tests, 1 transport mapping test. All 271 Rust + 1465 TS tests pass.

