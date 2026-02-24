---
id: 239-368b
title: Add split_tab_mode setting
status: complete
priority: P3
created: "2026-02-17T10:28:47.144Z"
updated: "2026-02-17T10:37:03.866Z"
dependencies: []
---

# Add split_tab_mode setting

## Problem Statement

No setting to control whether splits create separate tabs or share one tab

## Acceptance Criteria

- [ ] Rust config has split_tab_mode field with default separate
- [ ] Settings store has splitTabMode with hydrate and setter
- [ ] GeneralTab has dropdown for Split Tab Mode
- [ ] Unified mode hides second pane tab and shows combined name
- [ ] Setting persists across app restarts

## Files

- src-tauri/src/config.rs
- src/stores/settings.ts
- src/components/SettingsPanel/tabs/GeneralTab.tsx
- src/components/TabBar/TabBar.tsx
- src/hooks/useSplitPanes.ts

## Work Log

### 2026-02-17T10:37:00.796Z - Added split_tab_mode to Rust config, settings store, GeneralTab UI. Unified mode hides second pane tab, shows combined name, close closes both.

