---
id: 362-da09
title: Branch rename on HEAD change + show all branches per repo
status: ready
priority: P2
created: "2026-02-23T18:56:02.638Z"
updated: "2026-02-23T18:56:05.239Z"
dependencies: []
---

# Branch rename on HEAD change + show all branches per repo

## Problem Statement

When a terminal does git checkout -b new-branch, the sidebar creates a duplicate entry instead of renaming the existing one. Also, users have no way to see all local branches in the sidebar — only worktrees and the active branch are shown.

## Acceptance Criteria

- [ ] head-changed event renames branch entry (not creates new) when old branch has worktreePath === null
- [ ] head-changed with worktree branch creates new entry (existing behavior preserved)
- [ ] Tests cover all head-changed handler cases
- [ ] showAllBranches: boolean flag on RepositoryState, default false
- [ ] toggleShowAllBranches(path) method on repositoriesStore
- [ ] Global default in settingsStore + AppConfig (show_all_branches, default false)
- [ ] addRepository initializes showAllBranches from global default
- [ ] refreshAllBranchStats fetches all local branches via list_local_branches when flag is on
- [ ] Cleanup removes non-worktree, non-active, terminal-less branches when flag is turned off
- [ ] Repo context menu (⋯) has Show All Branches / Show Active Only toggle
- [ ] Settings panel has global default checkbox
- [ ] make check passes, all tests green

## Files

- src/hooks/useAppInit.ts
- src/__tests__/hooks/useAppInit.test.ts
- src/stores/repositories.ts
- src/__tests__/stores/repositories.test.ts
- src/stores/settings.ts
- src/__tests__/stores/settings.test.ts
- src-tauri/src/config.rs
- src/hooks/useGitOperations.ts
- src/__tests__/hooks/useGitOperations.test.ts
- src/components/Sidebar/RepoSection.tsx
- src/components/SettingsPanel/SettingsPanel.tsx
- docs/FEATURES.md

## Related

- plans/branch-rename-and-show-all-branches.md

## Work Log

