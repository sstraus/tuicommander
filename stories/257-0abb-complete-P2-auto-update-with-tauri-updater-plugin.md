---
id: 257-0abb
title: Auto-update with Tauri updater plugin
status: complete
priority: P2
created: "2026-02-18T22:16:38.245Z"
updated: "2026-02-19T11:51:26.756Z"
dependencies: []
---

# Auto-update with Tauri updater plugin

## Problem Statement

When a new version of TUICommander is released, users have no way to know about it or update without manually downloading. The app needs automatic update checking and installation using tauri-plugin-updater.

## Acceptance Criteria

- [ ] App checks for updates on startup (and optionally on a configurable interval)
- [ ] When an update is available, a non-intrusive notification or badge is shown to the user
- [ ] User can trigger the update download and install from within the app
- [ ] Update progress is shown during download
- [ ] App restarts after update is applied
- [ ] Settings option to enable/disable auto-update checking
- [ ] Works on macOS, Windows, and Linux

## Files

- src-tauri/Cargo.toml
- src-tauri/tauri.conf.json
- src-tauri/src/lib.rs
- src/stores/updater.ts
- src/components/StatusBar/StatusBar.tsx
- src/components/SettingsPanel/GeneralSettings.tsx

## Work Log

### 2026-02-19T11:51:26.691Z - Implemented auto-update: tauri-plugin-updater integrated, updater store with check/download/install/relaunch, 10s timeout on check, progress tracking, StatusBar badge, Settings toggle, signing keypair generated and pubkey configured in tauri.conf.json

