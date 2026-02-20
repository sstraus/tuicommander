---
id: 268-3d50
title: MCP bridge output uses LF only - may break Windows MCP clients
status: complete
priority: P2
created: "2026-02-20T07:38:38.169Z"
updated: "2026-02-20T12:14:34.361Z"
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

### 2026-02-20T12:14:34.291Z - Investigated: LF-only output is correct for JSON-RPC 2.0 over stdio. MCP clients (Claude Code, Cursor) expect LF on all platforms. Added doc comment to send_response() explaining this decision.

