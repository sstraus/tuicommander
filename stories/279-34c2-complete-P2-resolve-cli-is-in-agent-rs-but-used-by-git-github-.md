---
id: 279-34c2
title: resolve_cli is in agent.rs but used by git/github/worktree/lib
status: complete
priority: P2
created: "2026-02-20T13:57:16.821Z"
updated: "2026-02-20T14:16:28.531Z"
dependencies: []
---

# resolve_cli is in agent.rs but used by git/github/worktree/lib

## Problem Statement

Feature-envy: git.rs should not depend on the agent module. 25+ cross-module call sites at agent.rs:69.

## Acceptance Criteria

- [ ] resolve_cli extracted to cli.rs or platform.rs
- [ ] All call sites updated

## Files

- src-tauri/src/agent.rs

## Work Log

### 2026-02-20T14:16:28.461Z - Created cli.rs module, re-exported resolve_cli from agent.rs for backwards compat

