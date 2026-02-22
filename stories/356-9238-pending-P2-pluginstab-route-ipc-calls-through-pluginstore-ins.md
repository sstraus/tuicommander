---
id: "356-9238"
title: "PluginsTab: route IPC calls through pluginStore instead of calling Tauri directly"
status: pending
priority: P2
created: 2026-02-22T16:16:43.684Z
updated: 2026-02-22T16:16:43.684Z
dependencies: []
---

# PluginsTab: route IPC calls through pluginStore instead of calling Tauri directly

## Problem Statement

PluginsTab.tsx calls Tauri commands (enable_plugin, disable_plugin, etc.) directly via invoke() instead of going through pluginStore. This bypasses the store reactive layer and means plugin state can go out of sync with the UI.

## Acceptance Criteria

- [ ] PluginsTab calls pluginStore methods only, not invoke() directly
- [ ] Plugin enable/disable actions trigger reactive UI updates via store
- [ ] All existing plugin UI tests pass

## Files

- src/components/SettingsPanel/tabs/PluginsTab.tsx
- src/stores/pluginStore.ts

## Work Log

