---
id: 096-c95c
title: Widen sidebar to 300px and make it draggable
status: complete
priority: P2
created: "2026-02-15T10:17:08.070Z"
updated: "2026-02-15T11:07:14.928Z"
dependencies: []
---

# Widen sidebar to 300px and make it draggable

## Problem Statement

Sidebar is too narrow at 280px and has a fixed width. Users cannot resize it to their preference.

## Acceptance Criteria

- [ ] Increase --sidebar-width from 280px to 300px
- [ ] Add drag handle on sidebar right edge for horizontal resize
- [ ] Enforce min-width (~200px) and max-width (~500px) constraints
- [ ] Persist user-chosen width in localStorage
- [ ] Smooth drag UX with appropriate cursor changes

## Work Log

### 2026-02-15T11:07:14.860Z - Implemented draggable sidebar: increased to 300px default, drag handle with 200-500px range, localStorage persistence. 886 tests pass.

