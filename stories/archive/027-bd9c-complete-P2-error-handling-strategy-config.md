---
id: 027-bd9c
title: Error handling strategy config
status: pending
priority: P2
created: "2026-02-04T11:31:14.252Z"
updated: "2026-02-04T11:53:03.945Z"
dependencies: ["033-9a09"]
---

# Error handling strategy config

## Problem Statement

Agent errors need configurable handling: retry with backoff, skip to next task, or abort all. Currently no strategy.

## Acceptance Criteria

- [ ] Three strategies: retry, skip, abort
- [ ] Exponential backoff for retry (configurable base)
- [ ] Max retries limit
- [ ] Per-task or global strategy setting
- [ ] UI feedback on retry attempts

## Files

- src/error-handler.ts
- src/main.ts

## Work Log

