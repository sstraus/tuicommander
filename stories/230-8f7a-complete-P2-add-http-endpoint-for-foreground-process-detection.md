---
id: 230-8f7a
title: Add HTTP endpoint for foreground process detection
status: complete
priority: P2
created: "2026-02-17T10:28:43.730Z"
updated: "2026-02-17T10:28:58.192Z"
dependencies: []
---

# Add HTTP endpoint for foreground process detection

## Problem Statement

Browser mode clients need HTTP access to foreground process detection for the transport abstraction layer.

## Acceptance Criteria

- [ ] GET /sessions/{id}/foreground endpoint returns {agent: name} or {agent: null}
- [ ] Route registered in mcp_http/mod.rs
- [ ] Transport mapping added in transport.ts with transform

## Files

- src-tauri/src/mcp_http/session.rs
- src-tauri/src/mcp_http/mod.rs
- src/transport.ts

## Work Log

### 2026-02-17T10:28:58.084Z - Added GET /sessions/{id}/foreground HTTP endpoint in session.rs, route in mod.rs, transport mapping with transform in transport.ts.

