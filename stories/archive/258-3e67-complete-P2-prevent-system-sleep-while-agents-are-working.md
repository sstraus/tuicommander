---
id: 258-3e67
title: Prevent system sleep while agents are working
status: complete
priority: P2
created: "2026-02-19T06:36:06.003Z"
updated: "2026-02-19T06:49:33.266Z"
dependencies: []
---

# Prevent system sleep while agents are working

## Problem Statement

When an AI agent runs for a long time (e.g. Claude Code working autonomously), the system may go to sleep/standby, interrupting the work. Users need an option to prevent this, similar to caffeinate on macOS.

## Acceptance Criteria

- [ ] Add prevent_sleep_when_busy: bool field to AppConfig (default false)
- [ ] Add keepawake crate to Cargo.toml with platform-specific deps
- [ ] Create src-tauri/src/sleep_prevention.rs with block_sleep/unblock_sleep Tauri commands
- [ ] Hold keepawake::KeepAwake in AppState (Mutex<Option<KeepAwake>>)
- [ ] Register block_sleep and unblock_sleep commands in invoke_handler
- [ ] Add preventSleepWhenBusy setting to frontend settings store with persist logic
- [ ] Add toggle in GeneralTab settings UI
- [ ] Frontend monitors shellState across all terminals: if any is busy and setting enabled, call block_sleep; when all idle, call unblock_sleep
- [ ] Add test for config round-trip with new field

## Work Log

### 2026-02-19T06:49:29.772Z - Implemented prevent-sleep-when-busy feature: added keepawake 0.6 crate, sleep_prevention.rs module with block/unblock Tauri commands, AppConfig field, frontend settings store + GeneralTab toggle, reactive shellState monitoring in App.tsx. All Rust tests pass (22/22), TypeScript type check clean.

