---
id: 080-bdcb
title: Add UTF-8 boundary handling in PTY reader threads
status: complete
priority: P1
created: "2026-02-08T10:18:04.000Z"
updated: "2026-02-08T10:45:59.485Z"
dependencies: []
---

# Add UTF-8 boundary handling in PTY reader threads

## Problem Statement

Reader threads use String::from_utf8_lossy() on 4KB buffer chunks. Multi-byte characters (emoji, CJK, accented Latin) that straddle a buffer boundary are corrupted with U+FFFD replacement chars. With 50+ agent sessions outputting structured data (JSONL), this silently corrupts output and can break downstream parsing.

## Acceptance Criteria

- [ ] Create Utf8ReadBuffer struct that carries incomplete bytes across reads
- [ ] Use Utf8ReadBuffer in all 3 reader thread spawn sites (create_pty, create_pty_with_worktree, spawn_agent_session)
- [ ] Multi-byte characters spanning buffer boundaries render correctly
- [ ] No replacement characters in output for valid UTF-8 input
- [ ] Incomplete UTF-8 at EOF is dropped gracefully without panic

## Files

- src-tauri/src/lib.rs (reader threads at lines 317, 417, 1327)

## Work Log

