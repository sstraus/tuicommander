---
id: 196-6889
title: Fix agent binary detection for Windows
status: complete
priority: P1
created: "2026-02-16T13:20:02.390Z"
updated: "2026-02-16T13:48:44.096Z"
dependencies: []
---

# Fix agent binary detection for Windows

## Problem Statement

detect_agent_binary() uses $HOME env var (missing on Windows), Unix-only candidate paths, and calls which directly instead of the existing cross-platform has_cli() helper.

## Acceptance Criteria

- [ ] Use dirs::home_dir() instead of std::env::var HOME
- [ ] Add Windows candidate paths (AppData, Program Files, scoop, chocolatey)
- [ ] Use has_cli() or where on Windows instead of which
- [ ] Keep Unix candidate paths for macOS/Linux

## Files

- src-tauri/src/agent.rs

## Work Log

### 2026-02-16T13:48:44.024Z - Changed std::env::var(HOME) to dirs::home_dir(), added Windows candidate paths with .exe, platform-aware which/where lookup, handle where multi-line output

