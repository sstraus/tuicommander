---
id: 009-76e5
title: GitHub integration
status: complete
priority: P2
created: "2026-02-04T10:50:24.115Z"
updated: "2026-02-04T11:15:41.136Z"
dependencies: []
---

# GitHub integration

## Problem Statement

Need PR status, CI checks display. Use gh CLI for querying. Show in sidebar or status bar.

## Acceptance Criteria

- [ ] Detect if repo has GitHub remote
- [ ] Show PR status for current branch (gh pr status)
- [ ] Show CI checks (gh run list)
- [ ] Refresh every 30s

## Files

- src-tauri/src/lib.rs

## Work Log

