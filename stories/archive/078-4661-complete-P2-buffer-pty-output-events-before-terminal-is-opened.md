---
id: 078-4661
title: Buffer PTY output events before terminal is opened
status: complete
priority: P2
created: "2026-02-08T09:44:21.530Z"
updated: "2026-02-08T09:52:08.276Z"
dependencies: []
---

# Buffer PTY output events before terminal is opened

## Problem Statement

The pty-output event listener is set up inside initSession(), which only runs when the terminal becomes active. However the PTY backend starts emitting events immediately after create_pty returns. If a terminal is created and the user switches away before initSession fires, all PTY output including the shell prompt is lost. When switching back the terminal appears blank until new output arrives.

## Acceptance Criteria

- [ ] Register the pty-output event listener at terminal creation time not at activation time
- [ ] Buffer incoming events in an array when terminal has not been opened yet
- [ ] Replay buffered events into xterm when terminal.open() is called during activation
- [ ] Cap buffer size to prevent unbounded memory growth (keep last 100KB of output)
- [ ] Clear buffer after replay
- [ ] Verify: create new terminal, immediately switch away, wait 5s, switch back, initial prompt visible

## Files

- src/components/Terminal/Terminal.tsx (initSession, openTerminal, pty-output listener)

## Related

- 066-3a86

## Work Log

