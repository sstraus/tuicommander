---
id: 390-4c4c
title: Replace auto-resume with banner prompt after agent session restore
status: complete
priority: P2
created: "2026-02-25T22:32:53.518Z"
updated: "2026-02-25T22:35:59.033Z"
dependencies: []
---

# Replace auto-resume with banner prompt after agent session restore

## Problem Statement

Implement: Replace auto-resume with banner prompt after agent session restore

## Acceptance Criteria

- [ ] Implement as described

## Work Log

### 2026-02-25T22:35:55.767Z - Removed auto-inject of resume command in Terminal.tsx (hasResumedAgent + pty.write on idle). Added clickable banner with dismiss button. CSS follows style guide (--bg-secondary, --fg-muted, --accent hover). Test added for pendingResumeCommand being set on agent terminal restore. Committed 4ce49a8.

