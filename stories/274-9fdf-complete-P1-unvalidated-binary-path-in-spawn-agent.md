---
id: 274-9fdf
title: Unvalidated binary_path in spawn_agent
status: complete
priority: P1
created: "2026-02-20T13:56:35.898Z"
updated: "2026-02-20T14:00:21.187Z"
dependencies: []
---

# Unvalidated binary_path in spawn_agent

## Problem Statement

Caller-supplied binary_path passed directly to CommandBuilder::new() without path validation in agent_routes.rs:66-68 and agent.rs:442-451.

## Acceptance Criteria

- [ ] binary_path validated as absolute path that exists
- [ ] Invalid paths return appropriate error

## Files

- src-tauri/src/mcp_http/agent_routes.rs
- src-tauri/src/agent.rs

## Work Log

### 2026-02-20T14:00:21.111Z - Added binary_path validation (must be absolute, must exist) in both agent_routes.rs and agent.rs

