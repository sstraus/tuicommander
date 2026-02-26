---
id: "399-e363"
title: "Implement PR merge strategy setting (Merge/Squash/Rebase) via GitHub API"
status: pending
priority: P3
created: 2026-02-26T12:47:28.110Z
updated: 2026-02-26T12:47:28.110Z
dependencies: []
---

# Implement PR merge strategy setting (Merge/Squash/Rebase) via GitHub API

## Problem Statement

pr_merge_strategy is stored but merge_and_archive_worktree does local git merge only. No GitHub API merge call exists. Squash/Rebase are never used.

## Acceptance Criteria

- [ ] When merging via the sidebar button, use GitHub API to merge the PR with the configured strategy
- [ ] Supports Merge, Squash, Rebase strategies
- [ ] Falls back to local git merge if no PR is associated with the branch
- [ ] Strategy is read from effective per-repo config

## Files

- src-tauri/src/worktree.rs
- src-tauri/src/github.rs

## Work Log

