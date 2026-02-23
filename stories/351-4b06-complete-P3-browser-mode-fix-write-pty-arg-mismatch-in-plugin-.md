---
id: 351-4b06
title: "Browser mode: fix write_pty arg mismatch in plugin registry"
status: complete
priority: P3
created: "2026-02-21T20:34:55.189Z"
updated: "2026-02-23T08:02:00.295Z"
dependencies: []
---

# Browser mode: fix write_pty arg mismatch in plugin registry

## Problem Statement

pluginRegistry.ts calls invoke("write_pty", { id, data }) but the transport.ts mapping and Rust handler expect { sessionId, data }. Plugin-initiated terminal writes fail silently in browser mode due to arg name mismatch.

## Acceptance Criteria

- [ ] Update pluginRegistry.ts to pass sessionId instead of id when calling write_pty
- [ ] Or update transport.ts mapping to accept both id and sessionId
- [ ] Plugin terminal writes work correctly in browser mode

## Files

- src/pluginRegistry.ts
- src/transport.ts

## Work Log

### 2026-02-23T07:52:04.705Z - Triaged: implement now

### 2026-02-23T08:02:00.368Z - Fixed write_pty sessionId vs id arg mismatch

