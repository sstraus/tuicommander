---
id: 271-8216
title: WinGet glob pattern in detect_agent_binary never matches
status: complete
priority: P3
created: "2026-02-20T07:38:38.183Z"
updated: "2026-02-20T22:48:48.994Z"
dependencies: []
---

# WinGet glob pattern in detect_agent_binary never matches

## Problem Statement

agent.rs detect_agent_binary Windows candidates include a path with ** glob (AppData\Local\Microsoft\WinGet\Packages\**\binary.exe). Path::exists() does literal matching - the ** is not expanded so this candidate never matches.

## Acceptance Criteria

- [ ] Replace glob pattern with actual directory walk of WinGet Packages dir
- [ ] Or use the glob crate to expand the pattern
- [ ] Agent binary detection works for WinGet-installed tools on Windows

## Files

- src-tauri/src/agent.rs

## Work Log

### 2026-02-20T22:48:44.019Z - Verified: already resolved in a previous session. Code reviewed and confirmed fix is in place.

