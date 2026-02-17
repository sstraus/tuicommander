---
id: "048-1491"
title: "Lazygit auto-detection and path resolution"
status: complete
priority: P2
created: 2026-02-04T17:06:31.282Z
updated: 2026-02-04T17:06:31.282Z
dependencies: []
---

# Lazygit auto-detection and path resolution

## Problem Statement

System does not detect if lazygit is installed. Spawning lazygit without checking binary existence causes cryptic errors. Need proper binary detection and user-friendly error messages.

## Acceptance Criteria

- [ ] Add detect_lazygit_binary Tauri command (similar to detect_agent_binary)
- [ ] Check lazygit availability on app startup
- [ ] Show warning in UI if lazygit not found with install instructions
- [ ] Cache detection result to avoid repeated checks
- [ ] Provide fallback to open GitOperationsPanel if lazygit unavailable

## Files

- src-tauri/src/lib.rs:893-936
- src/hooks/useLazygit.ts

## Work Log

