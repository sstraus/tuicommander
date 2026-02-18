---
id: "250-2d2e"
title: "Show loading indicator when Whisper model loads on first use"
status: pending
priority: P2
created: 2026-02-18T16:42:55.039Z
updated: 2026-02-18T16:42:55.039Z
dependencies: []
---

# Show loading indicator when Whisper model loads on first use

## Problem Statement

When the user clicks the microphone for the first time in a session, the UI freezes while the Whisper model is loaded into memory. There is no feedback, making it look like the app has crashed.

## Acceptance Criteria

- [ ] A loading message or spinner is shown while the model loads on first use
- [ ] The UI remains responsive (loading happens asynchronously or loading state is shown before blocking call)
- [ ] The loading indicator disappears once the model is ready and recording starts
- [ ] Subsequent uses in the same session do not show the loading indicator (model stays loaded)

## Files

- src-tauri/src/dictation/commands.rs
- src-tauri/src/dictation/mod.rs
- src/stores/dictation.ts
- src/hooks/useDictation.ts

## Work Log

