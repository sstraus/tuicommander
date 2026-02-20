---
id: 266-45cb
title: No Windows-specific IDE detection (registry/install paths)
status: complete
priority: P2
created: "2026-02-20T07:38:38.168Z"
updated: "2026-02-20T22:48:48.968Z"
dependencies: []
---

# No Windows-specific IDE detection (registry/install paths)

## Problem Statement

detect_installed_ides has no #[cfg(windows)] block. On Windows, apps like Sourcetree, GitKraken, GitHub Desktop, Fork install .exe files not on PATH. They are undetectable without checking LOCALAPPDATA, ProgramFiles, or registry entries.

## Acceptance Criteria

- [ ] detect_installed_ides has a #[cfg(windows)] block
- [ ] Checks LOCALAPPDATA\Programs, ProgramFiles, scoop, winget install locations
- [ ] Sourcetree, GitKraken, GitHub Desktop, Fork detected on Windows when installed

## Files

- src-tauri/src/agent.rs

## Work Log

### 2026-02-20T22:48:43.736Z - Verified: already resolved in a previous session. Code reviewed and confirmed fix is in place.

