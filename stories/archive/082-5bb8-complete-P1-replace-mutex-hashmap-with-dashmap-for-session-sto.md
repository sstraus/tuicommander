---
id: 082-5bb8
title: Replace Mutex HashMap with DashMap for session storage
status: complete
priority: P1
created: "2026-02-08T10:18:04.003Z"
updated: "2026-02-08T10:52:08.477Z"
dependencies: []
---

# Replace Mutex HashMap with DashMap for session storage

## Problem Statement

parking_lot::Mutex<HashMap<String, PtySession>> locks the entire map for every session operation. With 50+ concurrent sessions doing writes, reads, resizes, and pauses, all operations serialize on a single lock. Research shows DashMap provides 60%+ better performance at 30+ concurrent accesses via per-bucket locking.

## Acceptance Criteria

- [ ] Add dashmap = 6 to Cargo.toml
- [ ] Replace Mutex<HashMap<String, PtySession>> with DashMap<String, PtySession>
- [ ] Update all session access patterns (insert, get, get_mut, remove) to use DashMap API
- [ ] Remove parking_lot::Mutex import if no longer used elsewhere
- [ ] Cargo check passes with no warnings
- [ ] Verify write_pty, resize_pty, pause_pty, resume_pty, close_pty all work with DashMap

## Files

- src-tauri/Cargo.toml
- src-tauri/src/lib.rs (AppState, all session access sites)

## Work Log

