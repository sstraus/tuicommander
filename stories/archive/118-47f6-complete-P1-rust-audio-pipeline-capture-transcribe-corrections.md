---
id: 118-47f6
title: "Rust audio pipeline: capture + transcribe + corrections"
status: in_progress
priority: P1
created: "2026-02-15T14:42:08.918Z"
updated: "2026-02-15T14:55:00.704Z"
dependencies: ["117-1cde"]
---

# Rust audio pipeline: capture + transcribe + corrections

## Problem Statement

Need Rust backend modules for microphone audio capture (cpal), Whisper inference (whisper-rs), text correction dictionary, and model download management. All business logic must be in Rust per architecture rules.

## Acceptance Criteria

- [ ] dictation/audio.rs: cpal capture at 16kHz mono f32
- [ ] dictation/transcribe.rs: Whisper inference in spawn_blocking
- [ ] dictation/corrections.rs: dictionary-based text replacement from JSON
- [ ] dictation/model.rs: model download from HuggingFace with progress
- [ ] dictation/commands.rs: Tauri commands registered in lib.rs
- [ ] Unit tests for corrections module
- [ ] Integration test: capture audio + transcribe + correct

## Files

- src-tauri/src/dictation/mod.rs
- src-tauri/src/dictation/audio.rs
- src-tauri/src/dictation/transcribe.rs
- src-tauri/src/dictation/corrections.rs
- src-tauri/src/dictation/model.rs
- src-tauri/src/dictation/commands.rs
- src-tauri/Cargo.toml
- src-tauri/src/lib.rs

## Related

- plans/voice-dictation.md

## Work Log

