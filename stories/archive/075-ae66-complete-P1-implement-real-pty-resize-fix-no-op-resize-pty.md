---
id: 075-ae66
title: Implement real PTY resize (fix no-op resize_pty)
status: complete
priority: P1
created: "2026-02-08T09:44:21.526Z"
updated: "2026-02-08T09:50:06.531Z"
dependencies: []
---

# Implement real PTY resize (fix no-op resize_pty)

## Problem Statement

resize_pty in lib.rs is a complete no-op — it discards session_id, rows, cols and returns Ok(()). When xterm.js calls fit() after a window resize or tab switch, the new dimensions never reach the PTY backend. The PTY continues operating at its original creation dimensions while xterm.js renders at the new size. This causes ANSI cursor positioning to land at wrong columns, producing overlapping text, mangled status bars, and garbled rendering in any program that uses cursor positioning (Claude Code status line, vim, htop, lazygit). This is the root cause of the terminal scramble visible in screenshots.

## Acceptance Criteria

- [ ] Store MasterPty handle in PtySession struct alongside writer and child
- [ ] Implement resize_pty to call master.resize() with the new PtySize
- [ ] Validate rows > 0 and cols > 0 before calling resize
- [ ] Add error handling for failed resize operations
- [ ] Verify with manual test: run htop or vim, resize window, confirm layout stays correct
- [ ] Verify Claude Code status line renders correctly after window resize

## Files

- src-tauri/src/lib.rs (PtySession struct, create_pty, resize_pty)

## Related

- 066-3a86

## Work Log

