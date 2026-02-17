---
id: 006-d2f2
title: Interactive agent prompts UI
status: complete
priority: P2
created: "2026-02-04T10:50:24.106Z"
updated: "2026-02-04T11:12:02.992Z"
dependencies: []
---

# Interactive agent prompts UI

## Problem Statement

When agent asks questions (AskUserQuestion), need interactive UI instead of raw terminal. Show numbered options with keyboard selection.

## Acceptance Criteria

- [ ] Detect question pattern in PTY output
- [ ] Render as interactive UI overlay
- [ ] Keyboard navigation (1-9, arrows, Enter)
- [ ] Send selection back to PTY

## Files

- src/main.ts

## Work Log

