---
id: 030-70a3
title: Centralized settings panel
status: pending
priority: P2
created: "2026-02-04T11:38:35.296Z"
updated: "2026-02-04T11:53:04.099Z"
dependencies: ["012-28b3", "033-9a09"]
---

# Centralized settings panel

## Problem Statement

Configuration is scattered across the UI. Need a unified settings panel (Cmd+,) to manage all preferences in one place.

## Acceptance Criteria

- [ ] Settings panel opens with Cmd+, shortcut
- [ ] Modal or sidebar panel with tabbed sections
- [ ] Sections: General, Appearance, Terminal, Agents, Notifications, Keyboard Shortcuts
- [ ] General: default shell, working directory, language
- [ ] Appearance: theme, font family, font size, density mode
- [ ] Terminal: cursor style, cursor blink, scrollback lines, bell
- [ ] Agents: default agent, fallback chain, error strategy, max retries
- [ ] Notifications: sounds on/off, volume, visual indicators
- [ ] Keyboard shortcuts: view and customize bindings
- [ ] Save to ~/.tui-commander/config.toml
- [ ] Apply changes immediately (live preview for appearance)
- [ ] Reset to defaults button per section

## Files

- src/settings.ts
- src/main.ts
- src/styles.css
- index.html

## Work Log

