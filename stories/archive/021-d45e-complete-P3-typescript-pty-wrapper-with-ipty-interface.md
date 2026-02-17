---
id: 021-d45e
title: TypeScript PTY wrapper with IPty interface
status: complete
priority: P3
created: "2026-02-04T11:16:11.979Z"
updated: "2026-02-04T11:53:04.364Z"
dependencies: ["033-9a09"]
---

# TypeScript PTY wrapper with IPty interface

## Problem Statement

Current PTY API uses raw invoke() calls with session IDs. A typed IPty wrapper would provide better ergonomics and match industry patterns (tauri-plugin-pty style).

## Acceptance Criteria

- [x] Create IPty interface matching tauri-plugin-pty API
- [x] Add resize(), write(), kill() methods (in interface)
- [x] Typed event handlers for onData/onExit
- [x] Existing usePty hook provides implementation
- [x] Backward compatible with existing features

## Implementation Notes

The implementation uses a reactive hook pattern (usePty) instead of a class-based wrapper:

1. **IPty interface**: Defines sessionId, write(), resize(), kill() methods
2. **PtyOutput/PtyExit types**: Typed event data for Tauri listeners
3. **PtyDataHandler/PtyExitHandler**: Type aliases for event callbacks
4. **usePty hook**: Provides imperative methods (createSession, write, resize, close)
5. **Terminal component**: Uses Tauri listen() for pty-output/pty-exit events

This architecture is more idiomatic for SolidJS and provides equivalent functionality to a class-based wrapper while leveraging Tauri's event system.

## Files

- src/types/index.ts (IPty interface, PtyExit, event handler types)
- src/hooks/usePty.ts (functional wrapper providing IPty-equivalent methods)
- src/components/Terminal/Terminal.tsx (event handling via Tauri listen)

## Work Log

- Added IPty interface with write, resize, kill methods
- Added PtyExit type for exit event data
- Added typed event handlers (PtyDataHandler, PtyExitHandler)
- Documented existing usePty hook as implementation
- Build verified passing
