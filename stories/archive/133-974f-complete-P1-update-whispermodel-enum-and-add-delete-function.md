---
id: 133-974f
title: Update WhisperModel enum and add delete function
status: complete
priority: P1
created: "2026-02-15T22:01:30.925Z"
updated: "2026-02-15T22:26:32.536Z"
dependencies: []
---

# Update WhisperModel enum and add delete function

## Problem Statement

Model.rs hardcodes Base and LargeV3Turbo. Need to replace Base with Small/SmallEn/LargeV2 to match screenshot models, and add delete_model() function to remove downloaded files.

## Acceptance Criteria

- [ ] WhisperModel enum has Small, SmallEn, LargeV2, LargeV3Turbo (Base removed)
- [ ] All match arms updated: filename(), download_url(), display_name(), size_hint_mb(), from_name(), name()
- [ ] delete_model(model) function deletes the .bin file from ~/.tui-commander/models/
- [ ] Correct GGML filenames: ggml-small.bin, ggml-small.en.bin, ggml-large-v2.bin, ggml-large-v3-turbo.bin
- [ ] Correct size hints: Small=488MB, SmallEn=488MB, LargeV2=3090MB, LargeV3Turbo=1620MB

## Files

- src-tauri/src/dictation/model.rs

## Work Log

### 2026-02-15T22:26:32.470Z - Replaced Base with Small/SmallEn/LargeV2 in WhisperModel enum. Added ALL constant array. Updated filenames, download_urls, display_names, size_hints, from_name, name. Added delete_model(). Updated get_model_info() to use WhisperModel::ALL. 9 model tests + 84 total Rust tests pass.

