---
id: 332-7538
title: "Frontend: pluginLoader.ts with hot reload"
status: complete
priority: P1
created: "2026-02-21T14:51:35.068Z"
updated: "2026-02-21T15:43:17.899Z"
dependencies: ["330-e0f8", "331-6581"]
---

# Frontend: pluginLoader.ts with hot reload

## Problem Statement

Need discovery, validation, loading, and hot reload for user plugins from plugin:// protocol

## Acceptance Criteria

- [ ] loadUserPlugins() calls invoke list_user_plugins then import(plugin://id/main.js)
- [ ] validateManifest checks required fields and minAppVersion
- [ ] validateModule checks default export shape
- [ ] Bad plugins skipped with console.error
- [ ] Hot reload: listen for plugin-changed, unregister, re-import with cache-bust, register
- [ ] Vitest tests for all validation paths

## Files

- src/plugins/pluginLoader.ts
- src/__tests__/plugins/pluginLoader.test.ts

## Work Log

### 2026-02-21T15:43:17.594Z - Implemented pluginLoader.ts with validateManifest, validateModule, loadUserPlugins, hot reload via plugin-changed events. Added __APP_VERSION__ Vite define. 18 tests passing.

