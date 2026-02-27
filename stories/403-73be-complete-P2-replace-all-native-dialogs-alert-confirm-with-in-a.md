---
id: 403-73be
title: Replace all native dialogs (alert/confirm) with in-app UI
status: complete
priority: P2
created: "2026-02-26T16:21:16.216Z"
updated: "2026-02-27T06:52:21.134Z"
dependencies: []
---

# Replace all native dialogs (alert/confirm) with in-app UI

## Problem Statement

PluginsTab.tsx still uses confirm() for uninstall (line 80), PromptDrawer.tsx uses confirm() for delete (line 188) and alert() for validation (line 380). These pop native macOS dialogs that break the app UX. The alert() calls for install errors in PluginsTab were already fixed (replaced with inline error signals), but confirm() calls remain because they require a blocking boolean return which needs an async in-app confirmation component.

## Acceptance Criteria

- [ ] Create a reusable async confirmation dialog component (e.g. ConfirmDialog)
- [ ] Replace confirm() in PluginsTab.tsx:80 (uninstall) with in-app dialog
- [ ] Replace confirm() in PromptDrawer.tsx:188 (delete prompt) with in-app dialog
- [ ] Replace alert() in PromptDrawer.tsx:380 (validation) with inline error
- [ ] Grep for any remaining alert/confirm/prompt calls in src/ and eliminate them

## Files

- src/components/SettingsPanel/tabs/PluginsTab.tsx
- src/components/PromptDrawer/PromptDrawer.tsx

## Work Log

### 2026-02-27T06:52:21.068Z - Replaced all native alert/confirm/prompt calls: confirm in PluginsTab via useConfirmDialog, confirm+alert in PromptDrawer with inline confirm dialog and error signal, confirm+alert in deep-link-handler via callbacks, window.prompt in useGitOperations browser mode via promptRepoPath callback. Added tests for browser mode handleAddRepo.

