---
id: 316-3cfb
title: "Settings: repos as direct nav items with deep-link open"
status: complete
priority: P2
created: "2026-02-21T09:17:54.233Z"
updated: "2026-02-21T09:40:45.649Z"
dependencies: ["315"]
---

# Settings: repos as direct nav items with deep-link open

## Problem Statement

Currently opening repo settings requires two dialogs (gear → settings → repo tab → repo modal). The split-view settings sidebar should list each repo as a direct nav item (indented or prefixed with a symbol), eliminating the double-dialog pattern.

## Acceptance Criteria

- [ ] Each configured repo appears as a nav item under a Repositories section in the settings sidebar
- [ ] Repo nav items are visually distinct from top-level sections (indented or prefixed with a symbol)
- [ ] Clicking a repo nav item shows that repo settings in the content pane
- [ ] Gear icon in toolbar opens Settings panel on the General section
- [ ] 3-dots repo context menu opens Settings panel directly on that repo nav item
- [ ] No separate repo settings modal is needed; the split view handles it
- [ ] Screenshot taken to verify visual result

## Files

- src/components/Settings/SettingsShell.tsx
- src/components/Settings/SettingsPanel.tsx
- src/components/Toolbar/Toolbar.tsx
- src/components/Sidebar/Sidebar.tsx

## Work Log

### 2026-02-21T09:40:45.580Z - Implemented unified settings panel: repos appear as nav items below global sections (General/Groups/Notifications/Dictation/Services), separated by a REPOSITORIES label. Deep-link open via context kind=repo pre-selects the repo nav item. Reset to Defaults footer button only shows for repo nav. displayName removed from SettingsContext. All 1640 tests green, visual verification confirmed.

