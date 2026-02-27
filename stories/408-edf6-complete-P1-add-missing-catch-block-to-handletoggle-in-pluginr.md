---
id: 408-edf6
title: Add missing catch block to handleToggle in PluginRow
status: complete
priority: P1
created: "2026-02-26T21:07:12.493Z"
updated: "2026-02-27T09:09:31.863Z"
dependencies: []
---

# Add missing catch block to handleToggle in PluginRow

## Problem Statement

`handleToggle` in `PluginsTab.tsx:68-76` has try/finally but no catch. If `pluginStore.setEnabled` throws, the error is silently swallowed. Compare with `handleUninstall` in the same file and the new `handleInstall` pattern which both catch + log + display errors.

## Acceptance Criteria

- [ ] Add catch block to `handleToggle` that logs via `appLogger.error` and displays inline error
- [ ] Follow the same `installError` signal pattern already used in `BrowseRow`

## Work Log

### 2026-02-27T09:09:31.785Z - Added catch block to handleToggle logging via appLogger.error, matching handleUninstall/handleInstall pattern.

