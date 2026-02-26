---
id: 385-00c2
title: Replace console.* with appLogger in Terminal.tsx and core hooks (wave 1 of 4)
status: complete
priority: P2
created: "2026-02-25T17:53:56.110Z"
updated: "2026-02-25T19:47:12.807Z"
dependencies: ["384-75e3"]
---

# Replace console.* with appLogger in Terminal.tsx and core hooks (wave 1 of 4)

## Problem Statement

Implement: Replace console.* with appLogger in Terminal.tsx and core hooks (wave 1 of 4)

## Acceptance Criteria

- [ ] Implement as described

## Work Log

### 2026-02-25T17:54:22.152Z - Files: Terminal.tsx (19 calls - ParsedEvent debug logs, api-error, session lifecycle), useAppInit.ts (18 calls - watchers, hydration, PTY reconnect), useGitOperations.ts (1), useRepository.ts (8), usePty.ts (1), useDictation.ts (2), useTerminalLifecycle.ts (3), useAgentDetection.ts (1), useAgentPolling.ts (1). Rules: error->error, warn->warn, log/info->info if user-relevant else debug, debug->debug. Exclude dev/simulator.ts and appLogger.ts itself. Depends on 384-75e3.

### 2026-02-25T19:47:12.872Z - Replaced 51 console.* calls across 9 files (Terminal.tsx, useAppInit.ts, useRepository.ts, etc). Commit 68402d4.

