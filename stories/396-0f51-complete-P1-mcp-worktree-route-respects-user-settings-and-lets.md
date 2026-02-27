---
id: 396-0f51
title: MCP worktree route respects user settings and lets model choose only the name
status: complete
priority: P1
created: "2026-02-26T12:47:28.109Z"
updated: "2026-02-26T22:27:54.181Z"
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

### 2026-02-26T22:25:28.867Z - AUTONOMOUS DECISION: Remove create_branch from MCP request body (always true — model creates new branches, not checks out existing ones). Add optional base_ref param (model can specify start point). Keep base_repo required (model must know which repo). Remove create_branch from CreateWorktreeRequest to prevent model from controlling this setting.

### 2026-02-26T22:26:22.914Z - AUTONOMOUS DECISION: Adding base_ref to MCP CreateWorktreeRequest to match plugin_docs.rs spec and Tauri command parity. Also fixing cache invalidation gap (Tauri path has it, MCP path doesn't). Also fixing transport.ts to forward baseRef to HTTP body.

### 2026-02-26T22:26:46.816Z - Completed: Removed create_branch from CreateWorktreeRequest (always true now). Added optional base_ref param. Added invalidate_repo_caches call after creation. Added Worktree Management section to plugin_docs.rs documenting model constraints. Build clean, 2119 tests pass.

### 2026-02-26T22:27:50.552Z - Completed: Most work was already done by story 392 (resolve_worktree_dir_for_repo, cache invalidation, base_ref in struct). Fixed transport.ts to send base_ref instead of stale create_branch to HTTP body. Verified plugin_docs.rs already has complete MCP worktree docs with model responsibility section. All 2119 TS + 701 Rust tests pass.

