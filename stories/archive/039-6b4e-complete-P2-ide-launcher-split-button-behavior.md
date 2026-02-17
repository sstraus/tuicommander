---
id: 039-6b4e
title: IDE launcher split button behavior
status: complete
priority: P2
created: "2026-02-04T14:39:23.146Z"
updated: "2026-02-07T22:26:50.458Z"
dependencies: ["033-9a09"]
---

# IDE launcher split button behavior

## Problem Statement

Currently clicking anywhere on the IDE button opens the dropdown menu. Users expect that clicking directly on the editor name opens the editor immediately, while clicking on the dropdown arrow should open the menu for selecting a different editor.

## Acceptance Criteria

- [ ] Split the button into two clickable zones: label area and dropdown arrow
- [ ] Click on editor name/icon area opens the selected editor directly
- [ ] Click on dropdown arrow (▾) opens the selection menu
- [ ] Visual feedback: hover states for both zones separately
- [ ] Keyboard support: Enter opens editor, arrow down opens menu

## Files

- index.html
- src/main.ts
- src/styles.css

## Work Log

