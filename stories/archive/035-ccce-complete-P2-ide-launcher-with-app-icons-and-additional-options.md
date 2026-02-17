---
id: 035-ccce
title: IDE launcher with app icons and additional options
status: pending
priority: P2
created: "2026-02-04T11:54:54.151Z"
updated: "2026-02-04T11:55:00.027Z"
dependencies: ["033-9a09"]
---

# IDE launcher with app icons and additional options

## Problem Statement

Current IDE launcher dropdown has text-only items and limited options. Need native app icons for visual recognition and additional options like Xcode, Sourcetree, and Copy Path action.

## Acceptance Criteria

- [ ] Add native app icons to dropdown items (VS Code blue logo, Xcode hammer, Finder icon, Terminal icon, Sourcetree icon)
- [ ] Add Xcode as IDE option with open command
- [ ] Add Sourcetree as git GUI option
- [ ] Add Copy Path action that copies repo path to clipboard
- [ ] Show selected IDE icon in the button itself
- [ ] Icons should be SVG or high-res PNG for retina displays
- [ ] Divider between IDE apps and utility actions (Copy Path)

## Files

- index.html
- src/main.ts
- src/styles.css
- src-tauri/src/lib.rs

## Work Log

