---
id: 312-8719
title: "TSX code quality: enum for dropdowns, dead buttons, IIFE removal, legacy shim"
status: complete
priority: P3
created: "2026-02-20T19:25:49.685Z"
updated: "2026-02-20T20:25:01.067Z"
dependencies: []
---

# TSX code quality: enum for dropdowns, dead buttons, IIFE removal, legacy shim

## Problem Statement

Three mutually-exclusive dropdown booleans in uiStore (ideDropdownVisible, fontDropdownVisible, agentDropdownVisible) should be one enum. IIFE in TabBar.tsx render section should be extracted to component scope. Two dead sidebar footer buttons (Notifications, Tasks) have no onClick. SettingsTab type alias is a within-repo legacy shim. globalErrorHandler export is unused.

## Acceptance Criteria

- [ ] Verify before: confirm three separate boolean dropdown signals with mutual-close logic in uiStore
- [ ] Replace three dropdown booleans in uiStore with activeDropdown: 'ide' | 'font' | 'agent' | null; update all toggle methods and consumers
- [ ] Extract IIFE variables (layout, isUnifiedMode) in TabBar.tsx to component scope; remove IIFE wrapper
- [ ] Remove two dead sidebar footer buttons (Notifications, Tasks) from Sidebar.tsx until features are implemented
- [ ] Remove SettingsTab legacy type alias from SettingsPanel.tsx; update App.tsx to use the real tab key type
- [ ] Run make check and all tests pass

## Files

- src/stores/ui.ts
- src/components/TabBar/TabBar.tsx
- src/components/Sidebar/Sidebar.tsx
- src/components/SettingsPanel/SettingsPanel.tsx
- src/App.tsx

## Work Log

### 2026-02-20T20:25:00.993Z - Replaced 3 dropdown booleans with activeDropdown enum, extracted layout/isUnifiedMode from IIFE in TabBar, removed dead Notifications/Tasks sidebar buttons, removed SettingsTab legacy type alias

