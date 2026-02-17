---
id: "047-67a9"
title: "Dedicated lazygit split pane"
status: complete
priority: P2
created: 2026-02-04T17:06:31.280Z
updated: 2026-02-04T17:06:31.280Z
dependencies: []
---

# Dedicated lazygit split pane

## Problem Statement

Current lazygit implementation (Cmd+G) spawns lazygit in the active terminal, taking it over completely. Users cannot work in terminal while using lazygit. Need dedicated pane that opens alongside current work.

## Acceptance Criteria

- [ ] Cmd+Shift+L opens lazygit in right-side split pane (50% width)
- [ ] Lazygit pane shows repo path in header
- [ ] Pane is resizable via drag handle
- [ ] Closing lazygit (q) automatically closes/hides the pane
- [ ] If no repo selected in active branch, shows error message
- [ ] Pane persists across terminal tab switches

## Files

- src/App.tsx:217-229
- src/components/Terminal/Terminal.tsx
- src/styles.css

## Work Log

