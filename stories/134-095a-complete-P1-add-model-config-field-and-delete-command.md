---
id: 134-095a
title: Add model config field and delete command
status: complete
priority: P1
created: "2026-02-15T22:01:30.927Z"
updated: "2026-02-15T22:28:24.016Z"
dependencies: ["133"]
---

# Add model config field and delete command

## Problem Statement

Commands.rs hardcodes LargeV3Turbo in get_dictation_status() and start_dictation(). Need to add model field to DictationConfig, read it from config instead of hardcoding, add delete_whisper_model command, and update get_model_info() to list 4 models.

## Acceptance Criteria

- [ ] DictationConfig has model: String field (default: large-v3-turbo)
- [ ] get_dictation_status() reads model from config
- [ ] start_dictation() reads model from config, reloads transcriber if model changed
- [ ] get_model_info() lists all 4 variants
- [ ] delete_whisper_model(model_name) command added, unloads transcriber if active
- [ ] Command registered in Tauri builder (main.rs or lib.rs)

## Files

- src-tauri/src/dictation/commands.rs
- src-tauri/src/main.rs

## Related

- 001

## Work Log

### 2026-02-15T22:28:23.942Z - Added model field to DictationConfig with serde default. Added configured_model() helper. get_dictation_status reads from config. start_dictation reloads transcriber on model change via active_model tracking. Added delete_whisper_model command with transcriber unload. Registered in lib.rs. 84 Rust tests pass.

