---
id: 090-f90e
title: Batch PR status backend command
status: complete
priority: P2
created: "2026-02-08T17:14:12.268Z"
updated: "2026-02-09T21:25:46.947Z"
dependencies: []
---

# Batch PR status backend command

## Problem Statement

Current GitHub integration polls PR and CI status per-branch using separate gh CLI calls (gh pr view + gh run list). This does not scale to monitoring all worktrees. We need a single batch command that returns PR status, CI check rollup, and diff stats for all open PRs in a repo with one gh pr list call.

## Acceptance Criteria

- [ ] New Tauri command get_repo_pr_statuses(path) returns Vec<BranchPrStatus>
- [ ] Uses single gh pr list --state open --json number,title,state,url,headRefName,additions,deletions,statusCheckRollup --limit 50
- [ ] Parses statusCheckRollup into structured counts: {passed, failed, pending, total}
- [ ] Each result includes: branch name, PR metadata, diff stats (additions/deletions), CI check summary
- [ ] Returns empty vec when gh CLI not available or repo has no GitHub remote
- [ ] Unit tests for JSON parsing logic with realistic gh output samples

## Files

- src-tauri/src/lib.rs
- src/types/index.ts

## Related

- 060-9065
- 062-1530

## Work Log

