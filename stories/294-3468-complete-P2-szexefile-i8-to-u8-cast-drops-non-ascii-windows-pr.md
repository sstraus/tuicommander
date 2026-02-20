---
id: 294-3468
title: szExeFile i8 to u8 cast drops non-ASCII Windows process names
status: complete
priority: P2
created: "2026-02-20T13:57:16.842Z"
updated: "2026-02-20T14:13:54.443Z"
dependencies: []
---

# szExeFile i8 to u8 cast drops non-ASCII Windows process names

## Problem Statement

from_utf8 fails for non-ASCII returns None at pty.rs:597-602. Agent not detected.

## Acceptance Criteria

- [ ] Use String::from_utf8_lossy

## Files

- src-tauri/src/pty.rs

## Work Log

### 2026-02-20T14:13:54.372Z - Changed String::from_utf8 to from_utf8_lossy for non-ASCII process names

