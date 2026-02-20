---
id: 286-228d
title: Dead stub handlers in Sidebar for group rename and color change
status: complete
priority: P2
created: "2026-02-20T13:57:16.825Z"
updated: "2026-02-20T14:09:59.638Z"
dependencies: []
---

# Dead stub handlers in Sidebar for group rename and color change

## Problem Statement

Context menu items Rename Group and Change Color are silently inert at Sidebar.tsx:690-696.

## Acceptance Criteria

- [ ] Wire to existing GroupsTab functionality or remove menu items

## Files

- src/components/Sidebar/Sidebar.tsx

## Work Log

### 2026-02-20T14:09:59.566Z - Wired group rename and color change to PromptDialog with repositoriesStore methods

