---
id: 056-cee9
title: Fix xterm.js macOS Option key handling
status: complete
priority: P1
created: "2026-02-04T22:00:24.962Z"
updated: "2026-02-05T12:31:35.599Z"
dependencies: []
---

# Fix xterm.js macOS Option key handling

## Problem Statement

The Option/Alt key on macOS does not work correctly in the terminal because xterm.js is missing the macOptionIsMeta configuration. This breaks keyboard shortcuts and special character input that rely on the Option modifier.

## Acceptance Criteria

- [ ] Add macOptionIsMeta: true to Terminal xterm.js config
- [ ] Test Option+key combinations work in terminal
- [ ] Test on macOS that special characters (like @, #, etc via Option) work

## Files

- src/components/Terminal/Terminal.tsx

## Work Log

