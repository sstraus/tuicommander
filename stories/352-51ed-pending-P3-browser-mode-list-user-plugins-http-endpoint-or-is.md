---
id: "352-51ed"
title: "Browser mode: list_user_plugins HTTP endpoint or isTauri guard"
status: pending
priority: P3
created: 2026-02-21T20:34:55.191Z
updated: 2026-02-21T20:34:55.191Z
dependencies: []
---

# Browser mode: list_user_plugins HTTP endpoint or isTauri guard

## Problem Statement

list_user_plugins is called in pluginLoader.ts with no HTTP mapping and no isTauri() guard. Throws an error in browser mode. Plugins cannot load in browser mode.

## Acceptance Criteria

- [ ] Either add GET /plugins route to Rust HTTP server and transport.ts mapping
- [ ] Or guard pluginLoader.ts with isTauri() and gracefully skip plugin loading in browser mode
- [ ] Browser mode does not crash on plugin loader initialization

## Files

- src/pluginLoader.ts
- src/transport.ts
- src-tauri/src/mcp_http/mod.rs

## Work Log

