---
id: 120-4767
title: StatusBar mic button + push-to-talk hotkey
status: pending
priority: P1
created: "2026-02-15T14:42:08.919Z"
updated: "2026-02-15T14:42:14.549Z"
dependencies: ["119-48af", "118-47f6"]
---

# StatusBar mic button + push-to-talk hotkey

## Problem Statement

Need a microphone button in StatusBar (near MD/Diff toggles) with hold-to-talk mouse interaction, blue pulsing animation during recording, and a configurable push-to-talk keyboard hotkey in App.tsx.

## Acceptance Criteria

- [ ] Mic button in StatusBar with microphone SVG icon
- [ ] Hold-to-talk: mouseDown starts, mouseUp stops, mouseLeave cancels
- [ ] Blue pulsing animation during recording (CSS keyframes)
- [ ] Spinner state during transcription processing
- [ ] Respects prefers-reduced-motion
- [ ] Configurable hotkey in App.tsx keydown/keyup handlers
- [ ] Transcribed text injected into active terminal via write_pty
- [ ] Toast notifications for errors (no terminal, no model, no mic)

## Files

- src/components/StatusBar/StatusBar.tsx
- src/App.tsx
- src/styles.css
- src/assets/icons/microphone.svg

## Related

- plans/voice-dictation.md

## Work Log

