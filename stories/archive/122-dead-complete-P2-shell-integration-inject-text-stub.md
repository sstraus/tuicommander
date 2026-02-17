---
id: 122-dead
title: Shell integration inject_text stub
status: pending
priority: P2
created: "2026-02-15T14:42:08.924Z"
updated: "2026-02-15T14:42:14.684Z"
dependencies: ["118-47f6"]
---

# Shell integration inject_text stub

## Problem Statement

Prepare the inject_text Tauri command for future shell integration. Command should be callable internally but not wired to any external trigger mechanism.

## Acceptance Criteria

- [ ] inject_text Tauri command registered and callable
- [ ] Applies text corrections before injection
- [ ] Writes to active terminal via write_pty
- [ ] API documented in code comments for future external trigger

## Files

- src-tauri/src/dictation/commands.rs
- src-tauri/src/lib.rs

## Related

- plans/voice-dictation.md

## Work Log

