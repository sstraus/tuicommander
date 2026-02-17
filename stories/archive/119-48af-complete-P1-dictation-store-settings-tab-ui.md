---
id: 119-48af
title: Dictation store + Settings tab UI
status: pending
priority: P1
created: "2026-02-15T14:42:08.918Z"
updated: "2026-02-15T14:42:14.481Z"
dependencies: ["118-47f6"]
---

# Dictation store + Settings tab UI

## Problem Statement

Frontend needs reactive state management for dictation feature and a new Settings > Dictation tab with model status, hotkey config, language selector, mic selector, and replacement map editor.

## Acceptance Criteria

- [ ] dictation.ts store with state: enabled, recording, processing, modelStatus, hotkey, language, corrections
- [ ] DictationSettings.tsx component with all setting controls
- [ ] SettingsPanel.tsx updated with Dictation tab
- [ ] Replacement map table editor with Add/Delete/Import/Export
- [ ] Hotkey recorder input
- [ ] Settings persist to localStorage

## Files

- src/stores/dictation.ts
- src/components/SettingsPanel/DictationSettings.tsx
- src/components/SettingsPanel/SettingsPanel.tsx

## Related

- plans/voice-dictation.md

## Work Log

