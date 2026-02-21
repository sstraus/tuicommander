---
id: 334-8eb6
title: "PluginHost: expanded API surface (Tier 2-4)"
status: complete
priority: P1
created: "2026-02-21T14:51:35.070Z"
updated: "2026-02-21T15:38:55.168Z"
dependencies: []
---

# PluginHost: expanded API surface (Tier 2-4)

## Problem Statement

External plugins need read-only state access, controlled write actions, and scoped Tauri invoke

## Acceptance Criteria

- [ ] Tier 2: getActiveRepo, getRepos, getActiveTerminalSessionId, getPrNotifications, getSettings
- [ ] Tier 3 capability-gated: writePty, openMarkdownPanel, playNotificationSound
- [ ] Tier 4: scoped invoke with extensible whitelist
- [ ] PluginCapabilityError thrown for undeclared capabilities
- [ ] Whitelist is a constant easy to extend
- [ ] TDD with vitest

## Files

- src/plugins/types.ts
- src/plugins/pluginRegistry.ts
- src/__tests__/plugins/pluginRegistry.test.ts

## Work Log

### 2026-02-21T14:52:01.495Z - Added sandboxed per-plugin data dir (readPluginData/writePluginData/deletePluginData) as Tier 1 - always available, scoped to {app_data_dir}/plugins/{id}/data/. Boss flagged that blocking writes entirely would prevent cache/state plugins.

