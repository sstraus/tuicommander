---
id: 139-3788
title: Deduplicate settings CSS — remove .repo-settings-* classes
status: complete
priority: P1
created: "2026-02-15T22:32:27.441Z"
updated: "2026-02-15T22:45:52.875Z"
dependencies: []
---

# Deduplicate settings CSS — remove .repo-settings-* classes

## Problem Statement

RepoSettingsPanel duplicates ~240 lines of CSS from SettingsPanel with a repo- prefix. Both panels use identical form elements (group, toggle, hint, slider, actions) but with different class names, making maintenance painful.

## Acceptance Criteria

- [ ] All .repo-settings-* CSS classes removed from styles.css (~lines 3069-3309)
- [ ] RepoSettingsPanel.tsx updated to use .settings-* classes instead
- [ ] Modifier classes added for repo-specific differences: .settings-header--repo (icon + subtitle path), .settings-footer (reset + done buttons)
- [ ] RepoSettingsPanel.test.tsx selectors updated to match new class names
- [ ] Visual parity maintained — no visual regressions
- [ ] All tests pass (npx vitest run)

## Files

- src/styles.css
- src/components/RepoSettingsPanel/RepoSettingsPanel.tsx
- src/__tests__/components/RepoSettingsPanel.test.tsx

## Work Log

### 2026-02-15T22:45:52.821Z - Removed ~240 lines of duplicated .repo-settings-* CSS. RepoSettingsPanel now uses shared .settings-* base with modifier classes for repo-specific elements.

