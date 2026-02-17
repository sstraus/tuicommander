---
id: "087-5546"
title: "Add graceful PTY shutdown with SIGINT before kill"
status: pending
priority: P2
created: 2026-02-08T10:18:04.006Z
updated: 2026-02-08T10:18:04.006Z
dependencies: []
---

# Add graceful PTY shutdown with SIGINT before kill

## Problem Statement

close_pty removes the session and drops handles immediately. The child process is forcibly terminated without a chance to clean up (flush buffers, remove temp files, save state). This can corrupt in-progress git operations or leave stale lock files.

## Acceptance Criteria

- [ ] Send SIGINT (0x03 byte) to PTY writer before dropping session
- [ ] Wait 100ms grace period for process to handle signal
- [ ] If process still running after grace period, proceed with forced drop
- [ ] Timeout prevents hanging on unresponsive processes
- [ ] close_pty returns success even if SIGINT write fails

## Files

- src-tauri/src/lib.rs (close_pty function)

## Work Log

