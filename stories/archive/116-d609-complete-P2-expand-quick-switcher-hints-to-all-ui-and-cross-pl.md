---
id: 116-d609
title: Expand quick switcher hints to all UI and cross-platform modifier keys
status: complete
priority: P2
created: "2026-02-15T14:11:26.290Z"
updated: "2026-02-15T17:59:40.539Z"
dependencies: []
---

# Expand quick switcher hints to all UI and cross-platform modifier keys

## Problem Statement

The quick switcher (Cmd+Ctrl hold) only shows shortcut hints on sidebar branch items. a competitor shows hints on ALL interactive elements: tabs (Cmd+1/2/3), New Tab (Cmd+T), IDE launcher (Cmd+O), Run (Cmd+R), sidebar branches (Cmd+Ctrl+1/2/3). Additionally, ALL shortcut hints in the app are hardcoded to the macOS Command symbol (U+2318). On Windows and Linux these should show Ctrl instead. The platform.ts module already provides detection but no component uses it for shortcut display.

## Acceptance Criteria

- [ ] PLATFORM MODIFIER: Create a shared utility (e.g. getModifierSymbol()) that returns the correct modifier symbol per platform. macOS: ⌘ (U+2318). Windows/Linux: Ctrl. Use platform.ts detectPlatform() which already exists. All hotkey-hint spans must use this utility instead of hardcoded ⌘
- [ ] QUICK SWITCHER SCOPE: When Cmd+Ctrl is held (quick switcher active), show shortcut overlays on ALL interactive elements, not just sidebar branches. Specifically: tab bar tabs (⌘1, ⌘2, ⌘3...), New Tab button (⌘T), IDE launcher button (⌘O), Run button (⌘R if story 115 is implemented), sidebar toggle (⌘[), Diff button (⌘D), MD button (⌘M), Help panel (⌘?)
- [ ] TAB SHORTCUTS: Show ⌘1 through ⌘9 on tab bar items when quick switcher is active. Wire the actual keyboard handler if not already done (Cmd+1 switches to tab 1, etc.)
- [ ] HOTKEY HINT VISIBILITY: Currently hotkey-hint spans are shown inline always (small text). During quick switcher mode, make them more prominent: larger font, higher contrast, maybe a badge/pill background so they stand out like in a competitor
- [ ] REPLACE ALL HARDCODED ⌘: Search and replace every instance of hardcoded U+2318 or ⌘ in .tsx components with the platform-aware utility. Files: Toolbar.tsx (⌘[), TabBar.tsx (⌘T), StatusBar.tsx (⌘M, ⌘D), IdeLauncher.tsx, HelpPanel.tsx if it shows shortcuts
- [ ] WINDOWS/LINUX KEYBOARD HANDLER: Verify App.tsx keyboard handler works on Windows/Linux. Currently uses isMeta = e.metaKey || e.ctrlKey which is correct. But quick switcher activation (line 715) checks e.metaKey && e.ctrlKey which on Windows means Ctrl+Win key - may need to use a different activation combo on Windows/Linux (e.g. Ctrl+Alt)
- [ ] CROSS-PLATFORM QUICK SWITCHER ACTIVATION: macOS: Cmd+Ctrl (existing). Windows/Linux: Ctrl+Alt (since Ctrl+Win is intercepted by OS). Detect platform and use appropriate key combo

## Files

- src/platform.ts
- src/App.tsx
- src/components/Toolbar/Toolbar.tsx
- src/components/TabBar/TabBar.tsx
- src/components/StatusBar/StatusBar.tsx
- src/components/IdeLauncher/IdeLauncher.tsx
- src/components/Sidebar/Sidebar.tsx
- src/components/HelpPanel/HelpPanel.tsx
- src/styles.css

## Work Log

### 2026-02-15T17:59:40.469Z - Added getModifierSymbol(), isQuickSwitcherActive(), isQuickSwitcherRelease() to platform.ts. Replaced all hardcoded ⌘ in 10 component files with platform-aware utility. Cross-platform quick switcher: Cmd+Ctrl (macOS), Ctrl+Alt (Win/Linux). Expanded quick switcher hints to tabs (number badges), toolbar (sidebar toggle), status bar (MD/Diff buttons). Added quick-switcher-active CSS class for prominent hint styling. 11 new platform tests, 903 total pass.

