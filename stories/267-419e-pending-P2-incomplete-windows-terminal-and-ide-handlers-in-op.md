---
id: "267-419e"
title: "Incomplete Windows terminal and IDE handlers in open_in_app"
status: pending
priority: P2
created: 2026-02-20T07:38:38.168Z
updated: 2026-02-20T07:38:38.168Z
dependencies: []
---

# Incomplete Windows terminal and IDE handlers in open_in_app

## Problem Statement

On Windows, open_in_app only handles terminal (cmd.exe) and finder (explorer). macOS has 13+ app-specific handlers. Windows users cannot open repos in Sourcetree, GitKraken, GitHub Desktop, Fork, or any non-CLI IDE. Windows terminal handler also launches cmd.exe specifically rather than Windows Terminal.

## Acceptance Criteria

- [ ] Windows handlers added for: sourcetree, github-desktop, gitkraken, fork, warp, ghostty, wezterm, alacritty
- [ ] Terminal handler prefers Windows Terminal (wt.exe) over cmd.exe
- [ ] All handlers tested on Windows

## Files

- src-tauri/src/agent.rs

## Work Log

