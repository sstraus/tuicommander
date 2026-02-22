---
id: 344-3daf
title: "SSE transport: consolidate 21 MCP tools into 5 meta-commands"
status: pending
priority: P1
created: "2026-02-21T19:56:53.603Z"
updated: "2026-02-21T19:57:01.217Z"
dependencies: ["343"]
---

# SSE transport: consolidate 21 MCP tools into 5 meta-commands

## Problem Statement

The SSE transport mcp_transport.rs defines identical 21 tools. Need to consolidate mcp_tool_definitions() into 5 meta-commands and rewrite handle_mcp_tool_call() with action-based dispatch calling Rust functions directly. Keep spawn_agent SSE stub with improved error.

## Acceptance Criteria

- [ ] Replace 21-item mcp_tool_definitions() with 5 identical schemas to bridge
- [ ] Replace 21-arm handle_mcp_tool_call() with 5 arms + inner action dispatch
- [ ] spawn_agent SSE stub preserved with updated error referencing new tool/action names
- [ ] cargo clippy clean

## Files

- src-tauri/src/mcp_http/mcp_transport.rs

## Work Log

