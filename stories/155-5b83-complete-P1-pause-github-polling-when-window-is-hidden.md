---
id: 155-5b83
title: Pause GitHub polling when window is hidden
status: complete
priority: P1
created: "2026-02-16T07:04:39.834Z"
updated: "2026-02-16T07:31:16.893Z"
dependencies: []
---

# Pause GitHub polling when window is hidden

## Problem Statement

githubStore polls every 30s even when the app is backgrounded or minimized, wasting battery, CPU, and network. Should stop polling on visibilitychange hidden and resume on visible.

## Acceptance Criteria

- [ ] Add document visibilitychange listener in github.ts to stop/resume polling
- [ ] Skip polling when repositoriesStore has zero repos
- [ ] Verify polling resumes immediately on window focus with a fresh poll
- [ ] Tests for visibility-based polling pause/resume

## Files

- src/stores/github.ts

## Work Log

### 2026-02-16T07:31:16.822Z - Added visibilitychange listener to pause polling when hidden, resume with immediate poll when visible. Skip polling when no repos. Tests added for all scenarios.

