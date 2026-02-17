---
id: "058-b258"
title: "Add terminal progress bar support (OSC 9;4)"
status: complete
priority: P2
created: 2026-02-04T22:00:24.965Z
updated: 2026-02-04T22:00:24.965Z
dependencies: []
---

# Add terminal progress bar support (OSC 9;4)

## Problem Statement

Long-running terminal operations (npm install, builds, etc.) do not show progress. The OSC 9;4 escape sequence is a standard for terminal progress indicators but we do not support it.

## Acceptance Criteria

- [ ] Install @xterm/addon-progress package
- [ ] Load progress addon in Terminal component
- [ ] Display progress indicator in tab title or status bar
- [ ] Clear progress when operation completes

## Files

- package.json
- src/components/Terminal/Terminal.tsx
- src/components/TabBar/TabBar.tsx

## Work Log

