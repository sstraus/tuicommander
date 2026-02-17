---
id: 140-cddf
title: Extract SettingsShell component from duplicated panel structure
status: complete
priority: P1
created: "2026-02-15T22:32:27.443Z"
updated: "2026-02-15T22:48:52.020Z"
dependencies: ["139"]
---

# Extract SettingsShell component from duplicated panel structure

## Problem Statement

SettingsPanel and RepoSettingsPanel share identical shell structure (overlay → panel → header → tabs → content) but implement it independently. Extracting a shared shell enables the unification in the next story.

## Acceptance Criteria

- [ ] New SettingsShell.tsx created in src/components/SettingsPanel/
- [ ] SettingsShell accepts props: visible, onClose, title, subtitle?, tabs[], activeTab, onTabChange, footer?, children
- [ ] SettingsPanel.tsx refactored to use SettingsShell internally — no external API change
- [ ] RepoSettingsPanel.tsx refactored to use SettingsShell internally — no external API change
- [ ] All existing tests pass unchanged
- [ ] Barrel export updated in index.ts

## Files

- src/components/SettingsPanel/SettingsShell.tsx
- src/components/SettingsPanel/SettingsPanel.tsx
- src/components/RepoSettingsPanel/RepoSettingsPanel.tsx
- src/components/SettingsPanel/index.ts

## Related

- A

## Work Log

### 2026-02-15T22:48:51.954Z - Extracted SettingsShell component. Both panels refactored to use it. 13 new tests, all 965 tests pass.

