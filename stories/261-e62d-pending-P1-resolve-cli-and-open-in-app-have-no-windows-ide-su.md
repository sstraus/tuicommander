---
id: "261-e62d"
title: "resolve_cli and open_in_app have no Windows IDE support"
status: pending
priority: P1
created: 2026-02-20T07:38:38.165Z
updated: 2026-02-20T07:38:38.165Z
dependencies: []
---

# resolve_cli and open_in_app have no Windows IDE support

## Problem Statement

extra_bin_dirs() returns empty Vec on Windows so resolve_cli always falls back to bare binary name. open_in_app has no #[cfg(windows)] handlers for IDEs beyond terminal/finder. Editors like VS Code, Cursor, GitKraken etc. are unlaunchable on Windows unless they happen to be on PATH.

## Acceptance Criteria

- [ ] extra_bin_dirs() includes Windows-specific paths (LOCALAPPDATA, ProgramFiles, scoop, winget)
- [ ] open_in_app has Windows handlers for all supported IDEs
- [ ] IDE launch works on Windows when app is opened from desktop shortcut

## Files

- src-tauri/src/agent.rs

## Work Log

