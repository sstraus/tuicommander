---
id: 256-c848
title: Fix download progress bar layout in Dictation Settings
status: complete
priority: P2
created: "2026-02-18T21:52:08.895Z"
updated: "2026-02-18T21:52:42.087Z"
dependencies: []
---

# Fix download progress bar layout in Dictation Settings

## Problem Statement

When downloading a Whisper model, the progress bar is squished into a tiny corner. The Not downloaded badge is always visible alongside the progress, and the progress bar container has no width constraint inside the flex row, causing it to collapse.

## Acceptance Criteria

- [ ] Progress bar is full-width and readable during download
- [ ] Not downloaded badge is hidden while downloading (replaced by the progress bar)
- [ ] Progress bar has a minimum width so it never collapses
- [ ] Layout matches the existing design system

## Files

- src/components/SettingsPanel/DictationSettings.tsx
- src/styles.css

## Work Log

### 2026-02-18T21:52:42.008Z - Hide badge during download, give progress container min-width: 100px

