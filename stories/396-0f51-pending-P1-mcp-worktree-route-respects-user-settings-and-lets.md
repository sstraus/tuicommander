---
id: 396-0f51
title: MCP worktree route respects user settings and lets model choose only the name
status: pending
priority: P1
created: "2026-02-26T12:47:28.109Z"
updated: "2026-02-26T12:47:32.122Z"
dependencies: ["392-f764"]
---

# MCP worktree route respects user settings and lets model choose only the name

## Problem Statement

POST /worktrees MCP endpoint hits the same broken create_worktree_internal path with hardcoded worktrees_dir. Models creating worktrees via MCP bypass all user settings. Per user requirement: model should only determine the branch name; all other settings (storage strategy, naming conventions) must come from user config.

## Acceptance Criteria

- [ ] MCP POST /worktrees reads per-repo effective config (storage strategy, etc.) before creating
- [ ] Model provides only branch_name; path is computed from settings
- [ ] plugin_docs.rs documents that models must use MCP to create worktrees (not raw git) so settings are respected
- [ ] plugin_docs.rs documents that only the branch name should be model-chosen; naming/path conventions come from user config
- [ ] Depends on storage strategy story being fixed first

## Files

- src-tauri/src/mcp_http/worktree_routes.rs
- src-tauri/src/mcp_http/plugin_docs.rs

## Work Log

