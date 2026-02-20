---
id: "268-3d50"
title: "MCP bridge output uses LF only - may break Windows MCP clients"
status: pending
priority: P2
created: 2026-02-20T07:38:38.169Z
updated: 2026-02-20T07:38:38.169Z
dependencies: []
---

# MCP bridge output uses LF only - may break Windows MCP clients

## Problem Statement

tui_mcp_bridge.rs writes JSON-RPC responses with LF line endings only. Some Windows MCP clients expect CRLF. While Rust lines() correctly reads both, outgoing writes use LF only.

## Acceptance Criteria

- [ ] MCP bridge writes CRLF on Windows or verifies target clients accept LF
- [ ] No MCP communication regressions on macOS/Linux

## Files

- src-tauri/src/bin/tui_mcp_bridge.rs

## Work Log

