---
id: 386-d8fa
title: Replace console.* with appLogger in stores (wave 2 of 4)
status: complete
priority: P2
created: "2026-02-25T17:53:56.172Z"
updated: "2026-02-25T19:53:21.798Z"
dependencies: ["384-75e3"]
---

# Replace console.* with appLogger in stores (wave 2 of 4)

## Problem Statement

Implement: Replace console.* with appLogger in stores (wave 2 of 4)

## Acceptance Criteria

- [ ] Implement as described

## Work Log

### 2026-02-25T17:54:22.223Z - Files: settings.ts (20), dictation.ts (11), agentConfigs.ts (5), repoSettings.ts (3), repositories.ts (3), terminals.ts (3), notifications.ts (2), notes.ts (2), repoDefaults.ts (2), activityStore.ts (2), updater.ts (2), promptLibrary.ts (2), ui.ts (2), github.ts (1), keybindings.ts (1). Source mapping: github->github, dictation->dictation, settings/config->config, others->store. Depends on 384-75e3.

### 2026-02-25T19:53:21.865Z - Replaced ~58 console.* calls across 15 stores. Commit e133fd6.

