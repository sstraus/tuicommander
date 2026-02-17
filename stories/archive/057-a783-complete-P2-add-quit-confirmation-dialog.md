---
id: "057-a783"
title: "Add quit confirmation dialog"
status: complete
priority: P2
created: 2026-02-04T22:00:24.964Z
updated: 2026-02-04T22:00:24.964Z
dependencies: []
---

# Add quit confirmation dialog

## Problem Statement

Users can accidentally close the app and lose their terminal sessions. There is no confirmation dialog before quitting when terminals are active.

## Acceptance Criteria

- [ ] Intercept Tauri close-requested event
- [ ] Show confirmation dialog if any terminals have active sessions
- [ ] Allow bypass with Cmd+Shift+Q or similar
- [ ] Save session state before closing if user confirms

## Files

- src/App.tsx
- src-tauri/src/lib.rs

## Work Log

