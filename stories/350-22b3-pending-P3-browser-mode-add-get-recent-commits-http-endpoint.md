---
id: "350-22b3"
title: "Browser mode: add get_recent_commits HTTP endpoint"
status: pending
priority: P3
created: 2026-02-21T20:34:55.188Z
updated: 2026-02-21T20:34:55.188Z
dependencies: []
---

# Browser mode: add get_recent_commits HTTP endpoint

## Problem Statement

get_recent_commits is called in useRepository.ts with try/catch that returns []. No HTTP mapping exists in transport.ts. Commit history panel shows empty in browser mode. The silent fallback hides the issue.

## Acceptance Criteria

- [ ] Add GET /repo/recent-commits?path=&branch= route to Rust HTTP server
- [ ] Add transport.ts mapping for get_recent_commits
- [ ] Commit history loads correctly in browser mode

## Files

- src/hooks/useRepository.ts
- src/transport.ts
- src-tauri/src/mcp_http/mod.rs

## Work Log

