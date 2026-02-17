---
id: 010-a0ff
title: Agent stats display
status: complete
priority: P3
created: "2026-02-04T10:50:24.117Z"
updated: "2026-02-04T11:32:58.049Z"
dependencies: []
---

# Agent stats display

## Problem Statement

Show token usage, timing, tool calls after agent completes. Format: Done (3 tool uses · 48.5k tokens · 28s).

## Acceptance Criteria

- [ ] Parse stats from agent output
- [ ] Display in status bar or toast
- [ ] Cumulative stats per session
- [ ] Export stats to JSON

## Files

- src/main.ts

## Work Log

