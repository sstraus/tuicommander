---
id: 002-2414
title: Agent spawning integration
status: complete
priority: P1
created: "2026-02-04T10:50:24.104Z"
updated: "2026-02-04T11:01:46.655Z"
dependencies: []
---

# Agent spawning integration

## Problem Statement

Need to spawn Claude Code agents in terminal panes. Currently just runs shell. Must detect claude binary, pass proper flags, capture structured output.

## Acceptance Criteria

- [ ] Detect claude binary location (which claude)
- [ ] Spawn with proper flags for non-interactive mode
- [ ] Parse agent output for metadata (tokens, timing)
- [ ] Handle agent exit gracefully

## Files

- src-tauri/src/lib.rs:36-121
- src/main.ts:43-159

## Work Log

