---
id: 194-f5e3
title: Cross-platform shell detection and spawn
status: complete
priority: P1
created: "2026-02-16T13:20:02.387Z"
updated: "2026-02-16T13:37:42.160Z"
dependencies: []
---

# Cross-platform shell detection and spawn

## Problem Statement

Shell spawning hardcodes /bin/zsh fallback and -l (login shell) flag in 4 locations. Windows has no /bin/zsh and PowerShell does not accept -l. Terminals will not spawn on Windows at all.

## Acceptance Criteria

- [ ] Replace /bin/zsh fallback with platform-aware default: powershell.exe on Windows, /bin/bash on Linux/macOS
- [ ] Skip -l flag on Windows (use -NoExit or no flag for PowerShell/cmd)
- [ ] Fix all 4 locations: pty.rs:155, pty.rs:271, mcp_http.rs:249, mcp_http.rs:833
- [ ] Extract shared shell_default() helper to avoid duplication
- [ ] Existing shell override from settings still takes precedence

## Files

- src-tauri/src/pty.rs
- src-tauri/src/mcp_http.rs

## Work Log

### 2026-02-16T13:37:42.092Z - Extracted shared helpers default_shell(), build_shell_command(), resolve_shell() in pty.rs. Replaced 4 hardcoded /bin/zsh + -l flag with platform-aware logic.

