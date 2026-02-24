---
id: 232-4db0
title: Create useAgentPolling hook for active terminal agent detection
status: complete
priority: P1
created: "2026-02-17T10:28:43.732Z"
updated: "2026-02-17T10:28:58.749Z"
dependencies: []
---

# Create useAgentPolling hook for active terminal agent detection

## Problem Statement

No mechanism exists to periodically check what agent is running in the active terminal and update the store.

## Acceptance Criteria

- [ ] Polls active terminal foreground process every 3 seconds via RPC
- [ ] Updates agentType in terminals store when value changes
- [ ] Only polls when there is an active terminal with a session
- [ ] Handles errors gracefully (session may close during poll)

## Files

- src/hooks/useAgentPolling.ts
- src/App.tsx

## Work Log

### 2026-02-17T10:28:58.632Z - Created useAgentPolling hook: polls every 3s via invoke, updates store, only active terminal with session. Wired in App.tsx.

