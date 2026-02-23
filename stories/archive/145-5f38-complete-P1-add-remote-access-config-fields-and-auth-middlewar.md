---
id: 145-5f38
title: Add remote access config fields and auth middleware
status: complete
priority: P1
created: "2026-02-15T23:38:45.004Z"
updated: "2026-02-15T23:59:24.246Z"
dependencies: []
---

# Add remote access config fields and auth middleware

## Problem Statement

The axum HTTP server has no authentication and binds only to 127.0.0.1. Remote browser access needs: config fields (enabled, port, username, password_hash), Basic Auth middleware, and conditional bind to 0.0.0.0 when remote access is enabled.

## Acceptance Criteria

- [ ] Add remote_access_enabled, remote_access_port, remote_access_username, remote_access_password_hash to AppConfig in config.rs
- [ ] Add axum Basic Auth middleware layer that validates credentials against config
- [ ] When remote_access_enabled=true, bind to 0.0.0.0:{port} instead of 127.0.0.1:0
- [ ] Password stored as bcrypt hash, never plaintext
- [ ] Auth NOT required for localhost connections (preserve MCP bridge behavior)
- [ ] Tests for auth middleware (valid creds, invalid creds, localhost bypass)

## Files

- src-tauri/src/config.rs
- src-tauri/src/mcp_http.rs
- src-tauri/Cargo.toml

## Work Log

### 2026-02-15T23:59:24.181Z - Added 4 config fields (remote_access_enabled, port, username, password_hash), bcrypt/base64 deps, Basic Auth middleware with localhost bypass, conditional 0.0.0.0 bind, 8 auth unit tests, config round-trip tests. All 97 tests pass.

