---
id: 419-e5dd
title: Add per-repo auto-fetch interval setting for main branch
status: complete
priority: P2
created: "2026-02-27T08:25:45.959Z"
updated: "2026-02-27T11:54:22.190Z"
dependencies: []
---

# Add per-repo auto-fetch interval setting for main branch

## Problem Statement

There is no way to configure automatic git fetch on the base branch at a regular interval. Users working on long-lived feature branches may miss upstream changes on main until they manually fetch. A per-repo setting (autoFetchIntervalMinutes) would keep the base branch up to date in the background, so branch stats (ahead/behind) stay fresh and merge conflicts are detected earlier.

## Acceptance Criteria

- [ ] Add autoFetchInterval field to per-repo settings (0 = disabled, default)
- [ ] Settings UI: numeric input or dropdown in repo settings
- [ ] Background timer runs git fetch origin <baseBranch> at configured interval
- [ ] Timer resets on manual fetch or repo switch
- [ ] Fetch errors logged to appLogger, not shown as blocking dialogs
- [ ] Setting persisted per-repo via existing config system
- [ ] Timer cleaned up on repo removal or app exit

## QA

None — covered by tests

## Work Log

### 2026-02-27T11:54:17.535Z - Completed: Added autoFetchIntervalMinutes to Rust RepoSettingsEntry + RepoDefaultsConfig, TypeScript RepoSettings + EffectiveRepoSettings + RepoDefaults stores, OVERRIDABLE_NULL_DEFAULTS. Created useAutoFetch hook with master-tick pattern (1min tick, checks interval per-repo). Settings UI dropdown in RepoWorktreeTab (Disabled/5/15/30/60 min). Wired into initApp lifecycle. 9 TDD tests. Updated config.md, FEATURES.md, CHANGELOG.md. All 2199 tests pass.

