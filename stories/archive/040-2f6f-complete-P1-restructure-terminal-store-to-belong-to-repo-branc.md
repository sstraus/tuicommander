---
id: 040-2f6f
title: Restructure terminal store to belong to repo/branch
status: complete
priority: P1
created: "2026-02-04T14:45:57.902Z"
updated: "2026-02-05T12:31:35.465Z"
dependencies: []
---

# Restructure terminal store to belong to repo/branch

## Problem Statement

Terminals are currently global but should belong to a specific repository and branch. When selecting a branch, only its terminals should be visible.

## Acceptance Criteria

- [ ] Create new data model: Repository -> Branch -> Terminal[]
- [ ] Update terminalsStore to support repo/branch hierarchy
- [ ] Migrate existing terminal creation to use new structure
- [ ] Active terminal selection respects branch context

## Files

- src/stores/terminals.ts
- src/stores/repositories.ts

## Work Log

