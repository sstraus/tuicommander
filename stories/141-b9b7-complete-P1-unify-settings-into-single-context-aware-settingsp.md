---
id: 141-b9b7
title: Unify settings into single context-aware SettingsPanel
status: complete
priority: P1
created: "2026-02-15T22:32:27.443Z"
updated: "2026-02-15T22:55:57.860Z"
dependencies: ["140"]
---

# Unify settings into single context-aware SettingsPanel

## Problem Statement

Two separate settings panels exist but RepoSettingsPanel is never mounted (dead TODO in App.tsx:737). Need one panel that shows repo tabs when opened from repo menu, global tabs when opened from gear icon.

## Acceptance Criteria

- [ ] SettingsPanel accepts context prop: { kind: global } | { kind: repo, repoPath, displayName }
- [ ] When kind=repo: repo tabs first (General, Worktree, Scripts) + separator + global tabs
- [ ] When kind=global: only global tabs shown
- [ ] Tab content components extracted to individual files under src/components/SettingsPanel/tabs/
- [ ] App.tsx handleRepoSettings sets context to repo with correct repoPath and displayName
- [ ] App.tsx gear icon sets context to global
- [ ] src/components/RepoSettingsPanel/ directory deleted
- [ ] RepoSettingsPanel test assertions merged into SettingsPanel tests
- [ ] All tests pass

## Files

- src/components/SettingsPanel/SettingsPanel.tsx
- src/App.tsx
- src/components/RepoSettingsPanel/

## Related

- B

## Work Log

### 2026-02-15T22:55:57.800Z - Extracted 8 tab components to tabs/. Rewrote SettingsPanel with SettingsContext prop. Deleted RepoSettingsPanel. Updated App.tsx with settingsContext signal. All 963 tests pass.

