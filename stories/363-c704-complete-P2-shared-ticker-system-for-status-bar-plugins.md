---
id: 363-c704
title: Shared ticker system for status bar plugins
status: complete
priority: P2
created: "2026-02-24T17:44:07.393Z"
updated: "2026-02-24T18:04:22.054Z"
dependencies: []
---

# Shared ticker system for status bar plugins

## Problem Statement

The status bar ticker slot is a single area currently owned by one plugin at a time. As more plugins register tickers (usage stats, CI status, custom alerts), they compete for the same space with no coordination — later plugins silently overwrite earlier ones. Users cannot tell which plugin owns the current message, or that other messages exist.

## Acceptance Criteria

- [ ] Plugin API exposes setTicker({ id, text, label, priority, ttl?, color? }) and clearTicker(id)
- [ ] Multiple plugins can register tickers simultaneously; only one is shown at a time
- [ ] Auto-rotation cycles through normal-priority tickers every 5s
- [ ] Urgent-priority tickers pause rotation and pin themselves until cleared
- [ ] Low-priority tickers appear only in the popover, not in rotation
- [ ] Status bar shows source label prefix (e.g. Usage · ) before the message
- [ ] A counter badge (1/3 ▸) indicates total active tickers and current index
- [ ] Clicking the ticker area cycles to the next ticker manually and pauses auto-rotation for 10s
- [ ] Right-clicking or holding opens a popover listing all active tickers with source, text and priority
- [ ] Counter badge and rotation are hidden when only one ticker is active
- [ ] Tickers with ttl expire automatically after the given duration
- [ ] Existing Claude Usage plugin migrated to new API without behavior change

## Files

- src/stores/tickerStore.ts
- src/components/StatusBar/StatusBar.tsx
- src/components/StatusBar/TickerArea.tsx
- src/plugins/types.ts
- src/plugins/pluginRegistry.ts
- docs/plugins.md

## Work Log

### 2026-02-24T18:03:47.014Z - Implemented: store extension (label, getRotationState, advanceManually, priority tiers), TickerArea component (counter badge, click-to-cycle, right-click popover), StatusBar integration, setTicker/clearTicker plugin API, Claude Usage migration, documentation updates. All checks pass.

