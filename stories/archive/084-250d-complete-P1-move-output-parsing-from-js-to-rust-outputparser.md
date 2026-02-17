---
id: "084-250d"
title: "Move output parsing from JS to Rust (OutputParser)"
status: pending
priority: P1
created: 2026-02-08T10:18:04.004Z
updated: 2026-02-08T10:18:04.004Z
dependencies: []
---

# Move output parsing from JS to Rust (OutputParser)

## Problem Statement

Every PTY output chunk runs through 45+ regex patterns in JavaScript for rate-limit detection, plus ANSI stripping and status line parsing, plus PR URL detection, plus OSC 9;4 parsing. This is high-cost synchronous work on the UI thread. Moving all parsing to the Rust reader thread eliminates JS regex overhead and reduces GC pressure.

## Acceptance Criteria

- [ ] Create src-tauri/src/output_parser.rs with OutputParser struct
- [ ] Port all 45+ rate-limit regex patterns from rate-limit.ts to Rust
- [ ] Port status line detection from status-parser.ts to Rust
- [ ] Port PR/MR URL detection from pr-detector.ts to Rust
- [ ] Port OSC 9;4 progress parsing from Terminal.tsx to Rust
- [ ] Emit structured events: pty-progress, pty-status, pty-ratelimit, pty-pr per session
- [ ] Frontend listens to typed events instead of running regex
- [ ] Remove rate-limit.ts, status-parser.ts, pr-detector.ts after migration
- [ ] Add Rust unit tests for all parsing patterns

## Files

- src-tauri/src/output_parser.rs (new)
- src-tauri/src/lib.rs (reader threads)
- src/components/Terminal/Terminal.tsx
- src/rate-limit.ts (remove)
- src/status-parser.ts (remove)
- src/pr-detector.ts (remove)

## Work Log

