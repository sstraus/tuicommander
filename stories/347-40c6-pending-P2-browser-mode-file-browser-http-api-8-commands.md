---
id: "347-40c6"
title: "Browser mode: file browser HTTP API (8 commands)"
status: pending
priority: P2
created: 2026-02-21T20:34:55.184Z
updated: 2026-02-21T20:34:55.184Z
dependencies: []
---

# Browser mode: file browser HTTP API (8 commands)

## Problem Statement

File browser commands (list_directory, fs_read_file, write_file, create_directory, delete_path, rename_path, copy_path, add_to_gitignore) have no HTTP mappings in transport.ts and no isTauri() guards. They throw unhandled errors in browser mode, making the file browser panel completely broken when accessing via HTTP.

## Acceptance Criteria

- [ ] Add Rust HTTP routes in mcp_http/ for all 8 file browser commands
- [ ] Add transport.ts mappings for all 8 commands
- [ ] Guard useFileBrowser.ts calls with isTauri() or show appropriate error in browser mode
- [ ] File browser panel renders without crashing in browser mode

## Files

- src/hooks/useFileBrowser.ts
- src/transport.ts
- src-tauri/src/mcp_http/mod.rs

## Work Log

