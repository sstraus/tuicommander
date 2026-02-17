---
id: 135-dd7f
title: Extend dictation store with model management
status: complete
priority: P1
created: "2026-02-15T22:01:30.928Z"
updated: "2026-02-15T22:38:52.614Z"
dependencies: ["134"]
---

# Extend dictation store with model management

## Problem Statement

Store only tracks single hardcoded model. Need to add selectedModel, models array, ModelInfo interface, and actions for refreshModels/setModel/deleteModel.

## Acceptance Criteria

- [ ] DictationConfig interface has model: string
- [ ] DictationStoreState has selectedModel: string, models: ModelInfo[]
- [ ] ModelInfo interface matches Rust struct (name, display_name, size_hint_mb, downloaded, actual_size_mb)
- [ ] refreshModels() action calls get_model_info
- [ ] setModel(name) action saves to config
- [ ] deleteModel(name) action calls delete_whisper_model then refreshes
- [ ] downloadModel() accepts model name parameter
- [ ] refreshConfig() loads model field from config

## Files

- src/stores/dictation.ts

## Related

- 002

## Work Log

### 2026-02-15T22:37:42.698Z - BUDGET STOP: quota >=100%, session stopped at limit. Story left in_progress.

### 2026-02-15T22:38:05.878Z - BUDGET STOP: quota >=100%, session stopped at limit. Story left in_progress.

