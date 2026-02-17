---
id: 061-7d57
title: Audit and fix focus handling after operations
status: complete
priority: P2
created: "2026-02-04T22:00:24.966Z"
updated: "2026-02-07T22:26:50.572Z"
dependencies: []
---

# Audit and fix focus handling after operations

## Problem Statement

Focus may be lost or incorrectly set after splitting panes, closing terminals, or switching tabs. This causes confusion and requires manual re-focusing.

## Acceptance Criteria

- [ ] After split: new terminal gets focus
- [ ] After close: remaining terminal gets focus
- [ ] After tab switch: switched-to terminal gets focus
- [ ] Focus is never lost to body or non-interactive elements
- [ ] Add regression tests for focus scenarios

## Files

- src/App.tsx
- src/stores/terminals.ts
- src/components/Terminal/Terminal.tsx

## Work Log

