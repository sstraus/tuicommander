---
id: 251-d2c2
title: Repo display name change in settings has no effect and does not persist
status: complete
priority: P1
created: "2026-02-18T16:43:43.964Z"
updated: "2026-02-18T16:45:06.088Z"
dependencies: []
---

# Repo display name change in settings has no effect and does not persist

## Problem Statement

In repo settings, changing the Display Name field does not update the sidebar immediately and the value is not saved — on reload the old name is shown again.

## Acceptance Criteria

- [ ] Changing the display name in repo settings updates the sidebar repo name immediately
- [ ] The new display name persists across app restarts
- [ ] The settings panel shows the current display name when reopened

## Files

- src/components/SettingsPanel/tabs/RepoWorktreeTab.tsx
- src/stores/repoSettings.ts
- src/stores/repositories.ts

## Work Log

### 2026-02-18T16:45:06.010Z - Root cause: displayName lives in both repositoriesStore (repositories.json, used by sidebar) and repoSettingsStore (repo-settings.json). updateRepoSetting only updated repoSettingsStore, never syncing back to repositoriesStore. Fix: added setDisplayName() to repositoriesStore and call it from SettingsPanel when displayName key is updated.

