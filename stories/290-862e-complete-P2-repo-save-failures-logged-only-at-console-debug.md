---
id: 290-862e
title: Repo save failures logged only at console.debug
status: complete
priority: P2
created: "2026-02-20T13:57:16.828Z"
updated: "2026-02-20T14:13:14.074Z"
dependencies: []
---

# Repo save failures logged only at console.debug

## Problem Statement

Every group mutation saves via fire-and-forget with console.debug catch at repositories.ts:96. Invisible data loss.

## Acceptance Criteria

- [ ] Escalate to console.error

## Files

- src/stores/repositories.ts

## Work Log

### 2026-02-20T14:13:13.991Z - Escalated save error from console.debug to console.error

