---
id: 142-92ff
title: Rust config infrastructure — generic helpers + new config structs
status: complete
priority: P1
created: "2026-02-15T22:32:27.444Z"
updated: "2026-02-15T23:00:17.702Z"
dependencies: []
---

# Rust config infrastructure — generic helpers + new config structs

## Problem Statement

Only AppConfig and DictationConfig are persisted in Rust. 8 frontend stores still use localStorage. Need generic config helpers and new Rust structs to serve as backend for all settings.

## Acceptance Criteria

- [ ] New src-tauri/src/config.rs with generic load_json_config<T> and save_json_config<T> using ~/.tui-commander/{filename}
- [ ] Existing load_config_internal/save_config_internal refactored to use generic helpers
- [ ] Dictation config load/save refactored to use generic helpers
- [ ] AppConfig expanded with #[serde(default)] fields: ide (String), default_font_size (u16)
- [ ] New structs: AgentConfig (agent-config.json), NotificationConfig (notifications.json), UIPrefsConfig (ui-prefs.json), RepoSettingsMap (repo-settings.json), PromptLibraryConfig (prompt-library.json)
- [ ] All new load_*/save_* Tauri commands registered in invoke_handler
- [ ] Existing config.json files still load without errors (backward compat via serde default)
- [ ] cargo test passes with serde round-trip tests for each config type

## Files

- src-tauri/src/config.rs
- src-tauri/src/lib.rs
- src-tauri/src/dictation/commands.rs

## Work Log

### 2026-02-15T23:00:17.633Z - Created config.rs with generic load/save helpers. Refactored AppConfig and dictation config. Added 6 new config structs with Tauri commands. All 89 Rust tests + 963 TS tests pass.

