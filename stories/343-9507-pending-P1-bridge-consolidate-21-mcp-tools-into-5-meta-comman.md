---
id: "343-9507"
title: "Bridge: consolidate 21 MCP tools into 5 meta-commands"
status: pending
priority: P1
created: 2026-02-21T19:56:53.601Z
updated: 2026-02-21T19:56:53.601Z
dependencies: []
---

# Bridge: consolidate 21 MCP tools into 5 meta-commands

## Problem Statement

The bridge binary tui_mcp_bridge.rs defines 21 individual MCP tools that clutter the tool listing. Need to consolidate tool_definitions() into 5 meta-commands (session, git, agent, config, plugin_dev_guide) and rewrite handle_tool_call() with action-based dispatch. Update bridge tests (count 21→5, name assertions, add action routing error tests).

## Acceptance Criteria

- [ ] Replace 21-item tool_definitions() with 5 meta-command ToolDefinition entries
- [ ] Replace 21-arm handle_tool_call() match with 5 arms + inner action dispatch
- [ ] Error messages guide model: missing action, unknown action, missing required param
- [ ] All bridge tests pass (cargo test --bin tui-mcp-bridge)

## Files

- src-tauri/src/bin/tui_mcp_bridge.rs

## Work Log

