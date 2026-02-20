---
id: "262-2920"
title: "Foreground process detection is a stub on Windows"
status: pending
priority: P1
created: 2026-02-20T07:38:38.165Z
updated: 2026-02-20T07:38:38.165Z
dependencies: []
---

# Foreground process detection is a stub on Windows

## Problem Statement

process_name_from_pid returns None on Windows. This breaks agent detection (claude, gemini, aider indicators in terminal tabs), agent-specific UI, and any feature that depends on knowing the foreground process.

## Acceptance Criteria

- [ ] Implement process_name_from_pid for Windows using winapi or sysinfo crate
- [ ] Agent detection indicators work on Windows
- [ ] Feature parity with macOS/Linux implementation

## Files

- src-tauri/src/pty.rs

## Work Log

