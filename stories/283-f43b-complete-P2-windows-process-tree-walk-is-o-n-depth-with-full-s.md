---
id: 283-f43b
title: Windows process tree walk is O(n*depth) with full system snapshot
status: complete
priority: P2
created: "2026-02-20T13:57:16.824Z"
updated: "2026-02-20T14:13:54.288Z"
dependencies: []
---

# Windows process tree walk is O(n*depth) with full system snapshot

## Problem Statement

CreateToolhelp32Snapshot of all processes then linear scan per depth level at pty.rs:624-668. Runs every 3s per terminal.

## Acceptance Criteria

- [ ] Build HashMap parent->children once
- [ ] Walk in O(depth) not O(n*depth)

## Files

- src-tauri/src/pty.rs

## Work Log

### 2026-02-20T14:13:54.215Z - Built HashMap parent->children once, walk in O(depth) via slice pattern match

