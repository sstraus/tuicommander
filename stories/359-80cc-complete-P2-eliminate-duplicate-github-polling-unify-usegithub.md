---
id: 359-80cc
title: "Eliminate duplicate GitHub polling: unify useGitHub hook and githubStore"
status: complete
priority: P2
created: "2026-02-22T16:16:43.686Z"
updated: "2026-02-23T08:17:43.758Z"
dependencies: []
---

# Eliminate duplicate GitHub polling: unify useGitHub hook and githubStore

## Problem Statement

Both useGitHub hook and githubStore independently poll the GitHub API on a timer. This doubles API calls, wastes rate limit quota, and can cause inconsistent data between components using different sources.

## Acceptance Criteria

- [ ] Single polling source for GitHub data (githubStore owns the polling)
- [ ] useGitHub hook reads from githubStore reactively, does not start its own timers
- [ ] GitHub API call count halved in tests or manual verification
- [ ] No duplicate network requests visible in browser devtools

## Files

- src/hooks/useGitHub.ts
- src/stores/githubStore.ts

## Work Log

### 2026-02-23T07:52:05.481Z - Triaged: implement now

### 2026-02-23T08:17:43.844Z - Unified GitHub polling: useGitHub is now a reactive wrapper around githubStore. Remote tracking data fetched alongside PR statuses in pollAll/pollRepo.

