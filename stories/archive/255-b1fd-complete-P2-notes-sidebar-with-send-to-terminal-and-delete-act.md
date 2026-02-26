---
id: 255-b1fd
title: Notes sidebar with send-to-terminal and delete actions
status: complete
priority: P2
created: "2026-02-18T18:32:10.069Z"
updated: "2026-02-18T22:14:16.227Z"
dependencies: []
---

# Notes sidebar with send-to-terminal and delete actions

## Problem Statement

While the system is working autonomously, the user needs a quick way to jot down ideas, prompts, and follow-up tasks without leaving the app. Currently there is no in-app notepad, forcing context switching to external tools.

## Acceptance Criteria

- [ ] A new button labeled Notes (or a suitable icon) is added in the StatusBar right controls section, next to the existing MD toggle button
- [ ] Clicking the Notes button (or pressing a keyboard shortcut) toggles a Notes sidebar panel that slides in from the right, following the same visual pattern as the MarkdownPanel and DiffPanel
- [ ] The Notes panel displays a scrollable list of notes; each note shows: the note text, the creation date/time, a Delete button, and a Send to Terminal button
- [ ] The Delete button removes the note from the list immediately (with no confirmation dialog, consistent with the app style)
- [ ] The Send to Terminal button writes the note text to the active terminal tab (using terminalsStore.getActive()?.ref?.write()), allowing the text to be used as a prompt or command
- [ ] A text input area is pinned to the bottom of the Notes panel; pressing Enter (or a Submit button) appends a new note to the top of the list with the current timestamp
- [ ] Notes are persisted across app restarts using Tauri store (tauri-plugin-store or localStorage as fallback), so notes survive session reloads
- [ ] The panel, buttons, typography, and spacing follow the existing CSS design system (CSS variables --bg-secondary, --fg-primary, --accent, --border, toggle-btn class, etc.)
- [ ] The keyboard shortcut to toggle the Notes panel is documented in HelpPanel
- [ ] SPEC.md feature status is updated to reflect the new Notes panel

## Files

- src/components/StatusBar/StatusBar.tsx
- src/components/NotesPanel/NotesPanel.tsx
- src/stores/notes.ts
- src/hooks/useKeyboardShortcuts.ts
- src/App.tsx
- src/styles.css
- src/components/HelpPanel/HelpPanel.tsx
- SPEC.md

## Work Log

### 2026-02-18T22:14:16.138Z - Already implemented: NotesPanel component, notes store, StatusBar button, keyboard shortcut (Cmd+N), send-to-terminal and delete actions

