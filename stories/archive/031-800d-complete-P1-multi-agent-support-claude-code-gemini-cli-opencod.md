---
id: "031-800d"
title: "Multi-agent support: Claude Code, Gemini CLI, OpenCode, Aider"
status: complete
priority: P1
created: 2026-02-04T11:42:00.237Z
updated: 2026-02-04T12:50:00.000Z
dependencies: []
---

# Multi-agent support: Claude Code, Gemini CLI, OpenCode, Aider

## Problem Statement

Currently only supports Claude Code. Need to support multiple CLI agents that users may prefer or switch between.

## Acceptance Criteria

- [x] Detect installed agents: claude, gemini, opencode, aider, codex
- [x] Agent selector in settings and per-terminal
- [x] Agent-specific spawn commands and arguments
- [x] Parse output format differences between agents
- [x] Store agent preference in config
- [x] Show active agent in status bar

## Files

- src/agents.ts (NEW)
- src-tauri/src/lib.rs
- src/main.ts
- src/styles.css
- index.html

## Work Log

### 2026-02-04

- Created `src/agents.ts` with:
  - AgentType enum for all supported agents
  - AgentConfig interface with spawn args, output format, and detection patterns
  - AGENTS record with full configuration for claude, gemini, opencode, aider, codex
  - AGENT_DISPLAY for UI icons and colors
  - AgentManager class with detection, caching, rate limit tracking, and fallback logic

- Updated `src-tauri/src/lib.rs`:
  - Added `detect_agent_binary` command for generic binary detection
  - Added `get_binary_version` helper
  - Extended `AgentConfig` to support custom binary paths and args
  - Updated `spawn_agent` to support multiple agent types

- Updated frontend:
  - Added agent selector dropdown in status bar
  - Added CSS styles for agent selector UI
  - Integrated AgentManager for detection and selection
  - Agent preference persisted to localStorage
