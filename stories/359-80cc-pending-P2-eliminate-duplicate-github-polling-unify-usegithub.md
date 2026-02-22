---
id: "359-80cc"
title: "Eliminate duplicate GitHub polling: unify useGitHub hook and githubStore"
status: pending
priority: P2
created: 2026-02-22T16:16:43.686Z
updated: 2026-02-22T16:16:43.686Z
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

