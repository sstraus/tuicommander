---
id: 037-526a
title: Prompt library drawer with injectable commands
status: complete
priority: P1
created: "2026-02-04T12:06:48.199Z"
updated: "2026-02-04T12:21:44.473Z"
dependencies: ["033-9a09"]
---

# Prompt library drawer with injectable commands

## Problem Statement

Users have frequently-used prompts and commands they want to quickly inject into agent terminals. A centralized prompt library would allow quick injection of user-defined prompts into the active terminal without manual typing every time.

## Acceptance Criteria

- [ ] Drawer/palette UI accessible via keyboard shortcut (Cmd+K) and toolbar button
- [ ] Categorized prompt list: Custom Prompts, Recent, Favorites
- [ ] Each prompt shows: name, description, optional keyboard shortcut
- [ ] Search/filter functionality with fuzzy matching
- [ ] Click or Enter injects prompt text into active terminal
- [ ] Option to inject and execute immediately vs inject and wait for user edit
- [ ] Custom prompt editor: create, edit, delete personal prompts
- [ ] Prompt variables support (e.g. {filename}, {branch}) with fill-in dialog
- [ ] Sync prompt library to config file for persistence across sessions

## Files

- index.html
- src/main.ts
- src/styles.css
- src-tauri/src/lib.rs

## Work Log

