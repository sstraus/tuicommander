---
id: 345-d064
title: Update SSE test suite for meta-command consolidation
status: pending
priority: P1
created: "2026-02-21T19:56:53.603Z"
updated: "2026-02-21T19:57:01.295Z"
dependencies: ["344"]
---

# Update SSE test suite for meta-command consolidation

## Problem Statement

~30 tests in mcp_http/mod.rs reference old tool names (list_sessions, send_input, etc.) and hardcode count=21. All must be updated to use new meta-command names with action parameter.

## Acceptance Criteria

- [ ] 3 count assertions updated 21→5
- [ ] All call_mcp_tool invocations use new meta-command + action format
- [ ] Error string assertions use .contains() with key substring
- [ ] New tests for action routing errors (missing action, unknown action, missing param with guidance)
- [ ] cargo test --lib passes all ~394 tests

## Files

- src-tauri/src/mcp_http/mod.rs

## Work Log

