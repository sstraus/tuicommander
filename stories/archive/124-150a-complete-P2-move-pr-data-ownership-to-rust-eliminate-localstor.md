---
id: 124-150a
title: Move PR data ownership to Rust, eliminate localStorage PR state
status: complete
priority: P2
created: "2026-02-15T16:13:34.417Z"
updated: "2026-02-15T18:07:40.344Z"
dependencies: []
---

# Move PR data ownership to Rust, eliminate localStorage PR state

## Problem Statement

PR data is persisted in localStorage via the frontend repo store (setPrInfo), creating phantom PRs when terminal-parsed PR URLs are wrongly associated with the active sidebar repo instead of the actual repo. This violates the architecture rule that all business logic belongs in Rust. The GitHub store already polls validated PR data from Rust (get_repo_pr_statuses), making the frontend PR persistence redundant and conflicting. We need a single source of truth in Rust with proper validation and stale data scavenging.

## Acceptance Criteria

- [ ] Remove setPrInfo/clearPrInfo from repositories store - stop persisting PR data in localStorage
- [ ] GitHub store (polling get_repo_pr_statuses from Rust) becomes the single source of truth for all PR data
- [ ] Terminal-detected pr-url events are either dropped or forwarded to Rust for validation against repo remotes before acceptance
- [ ] Add Rust-side scavenger that cleans stale branch/worktree data on poll cycles
- [ ] Sidebar PrBadge reads from GitHub store instead of repositories store prNumber field
- [ ] No phantom PRs appear on repos without GitHub remotes

## Files

- src/stores/repositories.ts
- src/stores/github.ts
- src/components/Sidebar/Sidebar.tsx
- src/components/Terminal/Terminal.tsx
- src-tauri/src/lib.rs
- src-tauri/src/output_parser.rs

## Work Log

### 2026-02-15T18:07:40.274Z - Removed prNumber/prUrl from BranchState, removed setPrInfo/clearPrInfo from repositories store, dropped terminal pr-url detection in Terminal.tsx, removed StatusBar terminal PR fallback display, cleaned up 8 tests. GitHub store polling is now the single source of truth for PR data.

