---
id: 148-ff15
title: Frontend transport abstraction — Tauri IPC vs HTTP/WebSocket
status: complete
priority: P1
created: "2026-02-15T23:38:45.008Z"
updated: "2026-02-16T00:15:09.611Z"
dependencies: ["147-55c8"]
---

# Frontend transport abstraction — Tauri IPC vs HTTP/WebSocket

## Problem Statement

The frontend uses invoke() and listen() everywhere, which only work inside Tauri webview. Browser clients need fetch()/WebSocket equivalents. Need a transport layer that auto-detects the environment.

## Acceptance Criteria

- [ ] Create src/transport.ts with isTauri detection (__TAURI__ in window)
- [ ] Implement rpc() function: uses invoke() in Tauri, fetch() in browser
- [ ] Implement subscribe() function: uses listen() in Tauri, WebSocket in browser
- [ ] Refactor usePty.ts to use transport layer instead of direct invoke()
- [ ] Refactor Terminal.tsx to use transport layer instead of direct listen()
- [ ] Existing Tauri mode works identically after refactor (no regressions)
- [ ] Tests for transport layer in both modes

## Files

- src/transport.ts
- src/hooks/usePty.ts
- src/components/Terminal/Terminal.tsx

## Work Log

### 2026-02-16T00:15:09.544Z - Created transport.ts with isTauri detection, rpc() function (invoke/fetch), subscribePty() (listen/WebSocket), mapCommandToHttp. Refactored usePty.ts and Terminal.tsx to use transport layer. 16 new transport tests, all 991 tests pass.

