---
id: 229-06ce
title: Detect foreground agent in PTY via process group leader
status: complete
priority: P1
created: "2026-02-17T10:28:43.727Z"
updated: "2026-02-17T10:28:57.719Z"
dependencies: []
---

# Detect foreground agent in PTY via process group leader

## Problem Statement

Rate limit notifications, agent fallback, and status bar agent display are broken because agentType is never passed to Terminal component and there is no mechanism to detect which agent is running in a PTY session.

## Acceptance Criteria

- [ ] Rust command get_session_foreground_process uses process_group_leader() and ps to identify foreground process
- [ ] classify_agent maps process names to known agent types (claude, gemini, opencode, aider, codex)
- [ ] Returns None for non-agent processes (bash, zsh, node, etc.)
- [ ] Platform-aware: works on macOS/Linux, returns None on Windows

## Files

- src-tauri/src/pty.rs
- src-tauri/src/lib.rs

## Work Log

### 2026-02-17T10:28:57.610Z - Implemented get_session_foreground_process Tauri command using portable-pty process_group_leader() + ps lookup. classify_agent maps 5 agent binaries. Platform-aware (Unix/Windows). 6 Rust unit tests pass.

