---
id: 399-e363
title: Implement PR merge strategy setting (Merge/Squash/Rebase) via GitHub API
status: complete
priority: P3
created: "2026-02-26T12:47:28.110Z"
updated: "2026-02-27T07:29:04.103Z"
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

### 2026-02-27T07:23:56.857Z - AUTONOMOUS DECISION: Architecture — add new Rust fn merge_pr_github(state, repo_path, pr_number, merge_method) in github.rs that calls REST PUT /repos/{owner}/{repo}/pulls/{pull_number}/merge; wrap as Tauri command. Modify handleMergeAndArchive in TypeScript: if githubStore.getPrStatus returns a PR and strategy \!= 'merge' (local), call new mergePrViaGithub then finalize worktree; otherwise fall through to existing mergeAndArchiveWorktree (local git). Strategy 'merge' still does local git to avoid GitHub-required PR reviews blocking simple local merges. After GitHub merge, use finalizeMergedWorktree for the worktree action. Reason: cleanest separation — Rust github.rs handles API call, worktree.rs handles filesystem cleanup, TypeScript orchestrates the decision.

### 2026-02-27T07:29:04.032Z - Completed: merge_pr_github_impl in github.rs calls GitHub REST PUT /repos/{owner}/{repo}/pulls/{pr_number}/merge; Tauri command merge_pr_via_github + HTTP POST /repo/merge-pr; transport.ts mapping; mergePrViaGithub in useRepository.ts; handleMergeAndArchive checks githubStore.getPrStatus first, uses GitHub API with prMergeStrategy (merge/squash/rebase), falls back to local git if GitHub fails or no PR. 4 tests covering: GitHub path with squash, fallback on API error, ask-mode pending ctx, no-PR local git path.

