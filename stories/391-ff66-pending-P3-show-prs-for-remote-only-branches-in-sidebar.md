---
id: "391-ff66"
title: "Show PRs for remote-only branches in sidebar"
status: pending
priority: P3
created: 2026-02-26T08:19:59.442Z
updated: 2026-02-26T08:19:59.442Z
dependencies: []
---

# Show PRs for remote-only branches in sidebar

## Problem Statement

TUICommander only shows PR indicators for local branches. If a PR headRefName has no matching local branch, the PR is fetched from GitHub but never displayed. RepoSection.tsx:408-414 iterates only local branches; github.rs fetches all open PRs but result is keyed by headRefName.

## Acceptance Criteria

- [ ] Investigate options: remote-only PR section, remote tracking branch matching, or repo-level PR badge
- [ ] Decide on approach and implement

## Work Log

