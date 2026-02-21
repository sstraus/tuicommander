---
id: 322-f60f
title: PTY output interception for plugin dispatch
status: pending
priority: P1
created: "2026-02-21T09:34:39.625Z"
updated: "2026-02-21T09:35:41.821Z"
dependencies: ["320-e879"]
---

# PTY output interception for plugin dispatch

## Problem Statement

Plugins need to receive both raw PTY text lines and structured Tauri parsed events. Terminal.tsx must intercept PTY output before writing to xterm, buffer into complete lines, strip ANSI codes, and dispatch to pluginRegistry. It must also route pty-parsed events to pluginRegistry.dispatchStructuredEvent.

## Acceptance Criteria

- [ ] In the pty-output event handler in Terminal.tsx, raw data is fed through a LineBuffer instance
- [ ] Complete lines are stripped of ANSI codes and dispatched to pluginRegistry.dispatchLine(cleanLine, sessionId) before terminal.write(rawData)
- [ ] In the pty-parsed event handler, every parsed event is also dispatched via pluginRegistry.dispatchStructuredEvent(parsed.type, parsed, sessionId)
- [ ] The direct uiStore.setPlanFilePath call in the plan-file case is kept in place (plan plugin not wired yet — that happens in story 322)
- [ ] All existing terminal tests still pass with no regressions
- [ ] No observable behavior change — no plugins are registered yet

## Files

- src/components/Terminal/Terminal.tsx

## Related

- 317-af1e
- 320-e879

## Work Log

