---
id: 350-22b3
title: "Browser mode: add get_recent_commits HTTP endpoint"
status: complete
priority: P3
created: "2026-02-21T20:34:55.188Z"
updated: "2026-02-23T08:02:00.149Z"
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

### 2026-02-23T07:52:04.571Z - Triaged: implement now

### 2026-02-23T08:02:00.229Z - get_recent_commits HTTP endpoint + transport mapping

