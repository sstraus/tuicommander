---
id: 329-3718
title: "Repo settings: global defaults with per-repo overrides"
status: complete
priority: P2
created: "2026-02-21T10:03:28.852Z"
updated: "2026-02-21T10:21:15.504Z"
dependencies: []
---

# Repo settings: global defaults with per-repo overrides

## Problem Statement

Users must configure each repository individually with no way to set global defaults. With many repos, this is tedious and error-prone. Need global defaults for worktree/script settings that individual repos can override.

## Acceptance Criteria

- [ ] Add a global repo defaults store (repoDefaultsStore) with fields: baseBranch, copyIgnoredFiles, copyUntrackedFiles, setupScript, runScript
- [ ] Add a Repository Defaults section in the General settings tab (or new nav item) to configure global defaults
- [ ] Per-repo settings fields show the global default as placeholder text when not overridden (empty = inherit)
- [ ] repoSettingsStore.getEffective(path) merges global defaults with per-repo overrides — all consumers use this instead of raw store values
- [ ] Reset to Defaults in repo settings resets only that repo overrides back to empty (inheriting global again)
- [ ] All new code covered by tests

## Files

- src/stores/repoSettings.ts
- src/stores/repoDefaults.ts
- src/components/SettingsPanel/tabs/GeneralTab.tsx
- src/components/SettingsPanel/tabs/RepoWorktreeTab.tsx
- src/components/SettingsPanel/tabs/RepoScriptsTab.tsx

## Work Log

### 2026-02-21T10:21:15.431Z - Implemented repoDefaultsStore (Rust+TS), nullable RepoSettings overrides, getEffective() merge, Repository Defaults in GeneralTab, inherit UI in RepoWorktreeTab/RepoScriptsTab. 1770 TS tests + 369 Rust tests passing.

