---
id: "033c"
title: "SolidJS Tauri integration hooks"
status: pending
priority: P1
created: 2026-02-04T13:00:00.000Z
updated: 2026-02-04T13:00:00.000Z
dependencies: ["033a", "033b"]
blocks: ["033d", "033e", "033f"]
---

# SolidJS Tauri integration hooks

## Problem Statement

Current code has raw invoke() calls scattered throughout. Need reusable hooks that integrate Tauri commands with Solid's reactivity.

## Tauri Commands to Wrap

```typescript
// PTY Management
can_spawn_session() → boolean
create_pty(config) → sessionId
write_pty(sessionId, data) → void
resize_pty(sessionId, rows, cols) → void
close_pty(sessionId, cleanupWorktree) → void

// Repository
get_repo_info(path) → RepoInfo
get_git_diff(path) → string
get_github_status(path) → GitHubStatus

// System
get_orchestrator_stats() → Stats
open_in_app(path, app) → void
detect_agent_binary(binary) → Detection
```

## Acceptance Criteria

- [ ] Create src/hooks/usePty.ts - PTY session management
- [ ] Create src/hooks/useRepository.ts - repo info and git operations
- [ ] Create src/hooks/useGitHub.ts - GitHub status with polling
- [ ] Create src/hooks/useOrchestrator.ts - session stats
- [ ] Create src/hooks/useAgentDetection.ts - agent binary detection
- [ ] All hooks use createResource() for async data
- [ ] Hooks integrate with stores for state updates
- [ ] Error handling with try/catch and user feedback

## Technical Notes

```typescript
// Example hook pattern
export function usePty() {
  const createSession = async (config: PtyConfig) => {
    const sessionId = await invoke<string>("create_pty", { config });
    return sessionId;
  };

  return { createSession, writeToSession, resizeSession, closeSession };
}
```

## Files

- src/hooks/usePty.ts
- src/hooks/useRepository.ts
- src/hooks/useGitHub.ts
- src/hooks/useOrchestrator.ts
- src/hooks/useAgentDetection.ts
- src/hooks/index.ts (re-exports)
