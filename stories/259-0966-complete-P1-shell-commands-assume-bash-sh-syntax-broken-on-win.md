---
id: 259-0966
title: Shell commands assume bash/sh syntax - broken on Windows
status: complete
priority: P1
created: "2026-02-20T07:38:38.163Z"
updated: "2026-02-20T22:48:48.824Z"
dependencies: []
---

# Shell commands assume bash/sh syntax - broken on Windows

## Problem Statement

useAppLazygit.ts and GitOperationsPanel.tsx write bash-syntax commands to the PTY (test -f, &&, if [ ], $(...), single-quote escaping). On Windows the PTY runs PowerShell or cmd.exe which does not understand this syntax. Every git operation and lazygit launch fails on Windows.

## Acceptance Criteria

- [ ] Detect shell type at runtime (bash/zsh vs PowerShell/cmd)
- [ ] Build shell commands using platform-appropriate syntax
- [ ] lazygit launch works on Windows
- [ ] Git panel operations (pull, push, etc.) work on Windows

## Files

- src/hooks/useAppLazygit.ts
- src/components/GitOperationsPanel/GitOperationsPanel.tsx
- src/utils/shell.ts

## Work Log

### 2026-02-20T22:48:43.264Z - Verified: already resolved in a previous session. Code reviewed and confirmed fix is in place.

