---
id: "389-9b85"
title: "Reusable KeyComboCapture component for shortcut input fields"
status: pending
priority: P2
created: 2026-02-25T22:18:00.727Z
updated: 2026-02-25T22:18:00.727Z
dependencies: []
---

# Reusable KeyComboCapture component for shortcut input fields

## Problem Statement

Implement: Reusable KeyComboCapture component for shortcut input fields

## Acceptance Criteria

- [ ] Implement as described

## Work Log

### 2026-02-25T22:19:05.993Z - Problem: Two places need key combo capture — DictationSettings has inline handleHotkeyCapture (DictationSettings.tsx:109-135) with button/input toggle + modifier normalization. PromptDrawer (PromptDrawer.tsx:428-436) is a plain text input with no capture logic. Solution: Extract reusable <KeyComboCapture> component. Props: value, onChange, placeholder, exclude (action names for self-collision skip). Features: (1) capture combos on keydown, ignore bare modifiers, (2) normalize to Tauri format Cmd/Ctrl/Alt/Shift+key, (3) collision detection via keybindingsStore.getActionForCombo + dictation hotkey check, (4) button display -> click -> capture mode -> blur/Escape cancels, (5) signal capturingShortcut so global shortcuts suspended. Consumers: DictationSettings (replace inline code), PromptDrawer (replace text input), KeyboardShortcutsTab (future). Files: new src/components/shared/KeyComboCapture.tsx + .module.css + test, refactor DictationSettings.tsx and PromptDrawer.tsx.

