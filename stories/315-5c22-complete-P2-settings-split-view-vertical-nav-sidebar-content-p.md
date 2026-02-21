---
id: 315-5c22
title: "Settings Split View: vertical nav sidebar + content pane"
status: complete
priority: P2
created: "2026-02-21T09:17:54.231Z"
updated: "2026-02-21T09:25:29.983Z"
dependencies: []
---

# Settings Split View: vertical nav sidebar + content pane

## Problem Statement

SettingsPanel uses a horizontal tab bar that does not scale as categories grow. Switching to a VS Code-style split layout (nav on left, content on right) gives more room for settings content and makes navigation clearer.

## Acceptance Criteria

- [ ] SettingsShell replaced with two-column layout: fixed-width nav sidebar (settingsNavWidth from uiStore) + scrollable content pane
- [ ] Nav sidebar lists all settings sections vertically (General, Notifications, Prompt Library, etc.)
- [ ] Active nav item is visually highlighted
- [ ] Nav sidebar width is draggable and persisted via uiStore.setSettingsNavWidth
- [ ] Content pane renders the selected section component
- [ ] Existing settings section components (GeneralSettings, etc.) are reused unchanged
- [ ] Layout follows docs/frontend/STYLE_GUIDE.md (colors, spacing, typography)
- [ ] Screenshot taken to verify visual result

## Files

- src/components/Settings/SettingsShell.tsx
- src/components/Settings/SettingsPanel.tsx
- src/styles.css
- src/stores/ui.ts

## Work Log

### 2026-02-21T09:25:29.916Z - Implemented split view: vertical nav sidebar + content pane. SettingsShell redesigned, CSS updated, all 1633 tests green. Visual verified via screenshot.

