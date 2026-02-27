---
id: 388-3801
title: Replace console.* with appLogger in plugins and utils; migrate Rust eprintln\\! (wave 4 of 4)
status: complete
priority: P3
created: "2026-02-25T17:53:56.298Z"
updated: "2026-02-26T22:24:27.767Z"
dependencies: ["383-de49"]
---

# Replace console.* with appLogger in plugins and utils; migrate Rust eprintln\! (wave 4 of 4)

## Problem Statement

Implement: Replace console.* with appLogger in plugins and utils; migrate Rust eprintln\! (wave 4 of 4)

## Acceptance Criteria

- [ ] Implement as described

## Work Log

### 2026-02-25T17:54:22.360Z - Files TS: pluginLoader.ts (11, source=plugin), planPlugin.ts (1, plugin), deep-link-handler.ts (7, app), themes.ts (1, app), notifications.ts (1, app), utils/openUrl.ts (1, app). Files Rust: add app_log\!(level,source,message) macro to state.rs writing to AppState.log_buffer non-blocking, replace eprintln\! in pty.rs, github.rs, git.rs, claude_usage.rs, worktree.rs, lib.rs. Depends on 383-de49 and 384-75e3.

### 2026-02-26T22:24:27.688Z - Completed: Added log_via_handle() and log_via_state() to app_logger.rs. Migrated eprintln\! in plugins.rs (install/uninstall/watcher), repo_watcher.rs, plugin_fs.rs, lib.rs (HeadWatcher/RepoWatcher). TypeScript side was already clean. Kept eprintln\! where AppState unavailable (config.rs pre-init, WindowGuard, NavigationGuard, tui_mcp_bridge binary, simulator.ts dev tool).

