---
id: 263-e70b
title: gh CLI not using resolve_cli - fails when launched from GUI
status: complete
priority: P1
created: "2026-02-20T07:38:38.166Z"
updated: "2026-02-20T22:48:48.951Z"
dependencies: []
---

# gh CLI not using resolve_cli - fails when launched from GUI

## Problem Statement

github.rs calls Command::new("gh") directly without resolve_cli(). When TUI Commander is launched from Finder/Spotlight (not a terminal), /opt/homebrew/bin is not on PATH so gh is not found. GitHub token resolution silently fails, breaking all GitHub API features (PR status, CI checks).

## Acceptance Criteria

- [ ] github.rs uses resolve_cli("gh") instead of bare Command::new("gh")
- [ ] GitHub token resolves correctly when app is launched from Finder
- [ ] PR status and CI check features work in release build

## Files

- src-tauri/src/github.rs

## Work Log

### 2026-02-20T22:48:43.545Z - Verified: already resolved in a previous session. Code reviewed and confirmed fix is in place.

