---
id: "059-cdf2"
title: "Add PTY spawn timeout and retry logic"
status: complete
priority: P2
created: 2026-02-04T22:00:24.965Z
updated: 2026-02-04T22:00:24.965Z
dependencies: []
---

# Add PTY spawn timeout and retry logic

## Problem Statement

If shell spawn hangs during terminal startup, there is no timeout or recovery. The user is stuck with a non-responsive terminal.

## Acceptance Criteria

- [ ] Add timeout to spawn_command (e.g. 10 seconds)
- [ ] Implement retry logic on spawn failure (up to 3 attempts)
- [ ] Show error message to user if spawn fails after retries
- [ ] Log spawn failures for debugging

## Files

- src-tauri/src/lib.rs

## Work Log

