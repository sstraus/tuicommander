---
id: 328-f5a2
title: Integrate plugin dispatch into existing PTY pipeline
status: ready
priority: P1
created: "2026-02-21T09:53:53.388Z"
updated: "2026-02-21T10:02:49.068Z"
dependencies: ["320-e879"]
---

# Integrate plugin dispatch into existing PTY pipeline

## Problem Statement

The plugin system must be integrated INTO the existing PTY pipeline, not bolted on as a parallel path. handlePtyData() is the single entry point for raw output; pluginRegistry.processRawOutput() is called inside it before terminal.write(). Structured events from Rust also flow through pluginRegistry after the existing terminal-state handlers. No duplication of processing paths.

## Acceptance Criteria

- [ ] handlePtyData() calls pluginRegistry.processRawOutput(data, sessionId) BEFORE terminal.write() — integrated into the existing flow, not a parallel path
- [ ] pluginRegistry.processRawOutput() internally manages LineBuffer and stripAnsi, dispatches clean lines to registered OutputWatchers
- [ ] In the pty-parsed event handler, AFTER the existing switch/case for terminal state (progress, rate-limit, etc.), calls pluginRegistry.dispatchStructuredEvent(parsed, sessionId)
- [ ] plan-file case STAYS in the switch/case for now (removed in story 326 when plan plugin takes over)
- [ ] All existing terminal behavior unchanged: backpressure, activity flags, idle detection, tab naming
- [ ] All existing terminal tests still pass with no regressions
- [ ] No observable behavior change — no plugins registered yet

## Files

- src/components/Terminal/Terminal.tsx

## Related

- 317-af1e
- 320-e879

## Work Log

