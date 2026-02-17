---
id: 032-b898
title: Redirect keyboard input from sidebar to active terminal
status: complete
priority: P2
created: "2026-02-04T11:42:00.239Z"
updated: "2026-02-04T12:18:42.115Z"
dependencies: ["033-9a09"]
---

# Redirect keyboard input from sidebar to active terminal

## Problem Statement

When focus is on sidebar and user starts typing, nothing happens. Text input should redirect to the visible/active terminal pane.

## Acceptance Criteria

- [ ] Detect text input when sidebar has focus
- [ ] Redirect keystrokes to active terminal
- [ ] Exclude navigation keys (arrows, tab, enter for sidebar actions)
- [ ] Visual feedback that input is going to terminal
- [ ] Optional: auto-focus terminal on first keystroke

## Files

- src/main.ts

## Work Log

