---
id: 410-89f9
title: Pass untracked flag to get_file_diff to eliminate redundant git subprocess
status: complete
priority: P2
created: "2026-02-26T21:07:12.646Z"
updated: "2026-02-27T11:25:50.826Z"
dependencies: []
---

# Pass untracked flag to get_file_diff to eliminate redundant git subprocess

## Problem Statement

`get_file_diff` in `git.rs:455` spawns `git ls-files --error-unmatch` on every call to detect if the file is untracked. The frontend already knows the status (`"?"`) from `get_changed_files`. Passing this flag eliminates the redundant subprocess.

## Acceptance Criteria

- [ ] Add `untracked: Option<bool>` parameter to `get_file_diff` Tauri command
- [ ] Frontend passes `file.status === "?"` when calling `get_file_diff`
- [ ] When `untracked == Some(true)`, skip the `ls-files` probe and go directly to `--no-index` diff
- [ ] Update HTTP transport and MCP routes if applicable

## QA

None — covered by existing tests

## Work Log

### 2026-02-27T11:25:50.684Z - Added untracked: Option<bool> param to Rust get_file_diff. Updated HTTP route (FileQuery), transport.ts, useRepository hook, DiffTabData, DiffTab component, TerminalArea, and DiffPanel to pass untracked=true for ? status files. Eliminates redundant git ls-files subprocess.

