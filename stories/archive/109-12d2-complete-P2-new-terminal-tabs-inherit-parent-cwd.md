---
id: 109-12d2
title: New terminal tabs inherit parent cwd
status: complete
priority: P2
created: "2026-02-15T13:52:19.403Z"
updated: "2026-02-15T17:29:17.267Z"
dependencies: []
---

# New terminal tabs inherit parent cwd

## Problem Statement

When opening a new terminal tab, it always starts in the default directory. a competitor makes new tabs inherit the working directory of the currently active terminal. This saves users from repeatedly cd-ing into project directories.

## Acceptance Criteria

- [ ] New terminal tab spawns shell in the cwd of the currently active terminal
- [ ] If no active terminal, fall back to default directory
- [ ] Rust backend reads cwd from active PTY process before spawning new one
- [ ] Works across all platforms (macOS proc_pidinfo, Linux /proc/pid/cwd)

## Files

- src-tauri/src/lib.rs
- src/stores/terminals.ts
- src/components/TabBar/TabBar.tsx

## Work Log

### 2026-02-15T17:29:17.205Z - createNewTerminal now inherits cwd from active terminal. Frontend-only change - Terminal.tsx already passes cwd to Rust create_pty.

