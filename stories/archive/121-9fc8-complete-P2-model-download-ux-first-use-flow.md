---
id: 121-9fc8
title: Model download UX + first-use flow
status: pending
priority: P2
created: "2026-02-15T14:42:08.919Z"
updated: "2026-02-15T14:42:14.618Z"
dependencies: ["120-4767"]
---

# Model download UX + first-use flow

## Problem Statement

Users need a smooth first-use experience: modal prompting model download (~1.6GB large-v3-turbo), progress indicator, error/retry handling, and model management in Settings.

## Acceptance Criteria

- [ ] First-use modal when dictation triggered without model
- [ ] Download progress bar in Settings > Dictation
- [ ] Cancel/retry download support
- [ ] Model status indicator: not_downloaded, downloading, ready, error
- [ ] Delete model option in Settings

## Files

- src/components/SettingsPanel/DictationSettings.tsx
- src/stores/dictation.ts

## Related

- plans/voice-dictation.md

## Work Log

