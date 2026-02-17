---
id: 076-147e
title: Add flow control with watermarks for PTY output
status: complete
priority: P1
created: "2026-02-08T09:44:21.529Z"
updated: "2026-02-08T09:56:50.473Z"
dependencies: []
---

# Add flow control with watermarks for PTY output

## Problem Statement

The PTY reader thread in Rust does a tight loop (read 4096 bytes then emit pty-output event) with no pause mechanism. The frontend calls terminal.write() with no backpressure. When a command produces massive output (find /, cat large-file, cargo build verbose), the Tauri IPC channel floods with events, xterm internal write buffer grows unbounded, causing UI freezes, memory pressure, and unresponsive input.

## Acceptance Criteria

- [ ] Track pending write bytes on the frontend using xterm.js write() Promise callbacks
- [ ] Define HIGH_WATERMARK (e.g. 1MB) and LOW_WATERMARK (e.g. 256KB) thresholds
- [ ] When pending bytes exceed HIGH_WATERMARK, signal backend to pause reading via Tauri command
- [ ] When pending bytes drop below LOW_WATERMARK, signal backend to resume reading
- [ ] Backend reader thread must respect pause/resume signals via condvar or channel
- [ ] Verify with manual test: cat a large file over 10MB, confirm UI remains responsive during output

## Files

- src-tauri/src/lib.rs (reader thread in create_pty)
- src/components/Terminal/Terminal.tsx (pty-output handler)

## Related

- 066-3a86

## Work Log

