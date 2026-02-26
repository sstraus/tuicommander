---
id: "401-d636"
title: "Optimize GitHub PR polling: single batched GraphQL call for all repos"
status: pending
priority: P2
created: 2026-02-26T15:02:13.197Z
updated: 2026-02-26T15:02:13.197Z
dependencies: []
---

# Optimize GitHub PR polling: single batched GraphQL call for all repos

## Problem Statement

Currently we make 1 GraphQL call per repo per poll cycle (2N total calls with git rev-list). With 5+ repos this is wasteful. Also fetching states [OPEN, CLOSED, MERGED] wastes the 50-slot limit on stale PRs. Additionally, merged PRs that happen while app is closed are missed if we switch to OPEN-only.

## Acceptance Criteria

- [ ] Startup poll: single aliased GraphQL query with states [OPEN, MERGED] for all repos — detects offline transitions
- [ ] Subsequent polls: single aliased GraphQL query with states [OPEN] only — lightweight steady-state
- [ ] Reduce first param from 50 to 20 per repo (configurable)
- [ ] Fallback: if batched aliased query fails (partial auth, query too complex), automatically retry with per-repo individual calls (current behavior)
- [ ] Persist last-known PR states to disk before app close so startup can diff against them for transition detection
- [ ] Wire up check_details: either fold into batch query or lazy-load when PrDetailPopover opens (currently dead — always empty)
- [ ] DRY the duplicate merged-grace-period logic in RepoSection.tsx and StatusBar.tsx into shared utility
- [ ] Cache get_github_status (git rev-list ahead/behind) with short TTL instead of spawning subprocess every poll

## Files

- src-tauri/src/github.rs
- src/stores/github.ts
- src/components/Sidebar/RepoSection.tsx
- src/components/StatusBar/StatusBar.tsx
- src/components/PrDetailPopover/PrDetailPopover.tsx

## Work Log

