---
id: "069-1ed6"
title: "Consolidate status bar into single inline flow"
status: complete
priority: P2
created: 2026-02-05T09:39:39.438Z
updated: 2026-02-05T09:39:39.438Z
dependencies: []
---

# Consolidate status bar into single inline flow

## Problem Statement

TUI Commander status bar is split into three sections (left/middle/right) with visual separators, making it feel fragmented. a competitor presents all status information in a single natural inline flow, providing comprehensive context without visual breaks.

## Acceptance Criteria

- [ ] Merge left/middle/right sections into single inline display
- [ ] Remove visual section separators
- [ ] Maintain all existing information (zoom, sessions, git status)
- [ ] Add more contextual information inline (model info, token usage if applicable)
- [ ] Improve information density while maintaining readability
- [ ] Keep MD and Diff toggle buttons accessible

## Files

- src/components/StatusBar/StatusBar.tsx
- src/styles.css

## Work Log

