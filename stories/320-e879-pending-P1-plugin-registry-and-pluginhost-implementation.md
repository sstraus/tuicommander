---
id: 320-e879
title: Plugin registry and PluginHost implementation
status: pending
priority: P1
created: "2026-02-21T09:34:19.725Z"
updated: "2026-02-21T09:35:41.668Z"
dependencies: ["318-d0e3", "319-0595"]
---

# Plugin registry and PluginHost implementation

## Problem Statement

Need a central plugin registry that manages plugin lifecycle (Obsidian-style onload/onunload + Disposable), dispatches PTY output lines to registered OutputWatchers, routes structured Tauri events to handlers, and exposes the PluginHost API surface to plugins.

## Acceptance Criteria

- [ ] src/plugins/pluginRegistry.ts exports pluginRegistry singleton
- [ ] register(plugin): calls plugin.onload(host); unregister(id): calls plugin.onunload() and disposes all registrations
- [ ] PluginHost.registerSection delegates to activityStore
- [ ] PluginHost.registerOutputWatcher: adds watcher; dispatchLine(cleanLine, sessionId) calls all matching watchers synchronously
- [ ] PluginHost.registerStructuredEventHandler: dispatchStructuredEvent(type, payload, sessionId) routes to correct handlers
- [ ] PluginHost.registerMarkdownProvider delegates to markdownProviderRegistry
- [ ] PluginHost.addItem/removeItem/updateItem delegate to activityStore
- [ ] dispatchLine resets lastIndex on global regexes before each test
- [ ] dispatchLine catches and logs exceptions from watchers, never throws
- [ ] All tests pass

## Files

- src/plugins/pluginRegistry.ts
- src/__tests__/plugins/pluginRegistry.test.ts

## Related

- 318-d0e3
- 319-0595

## Work Log

