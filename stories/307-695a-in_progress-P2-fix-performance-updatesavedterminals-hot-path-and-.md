---
id: 307-695a
title: "Fix performance: updateSavedTerminals hot path and ServicesTab interval placement"
status: in_progress
priority: P2
created: "2026-02-20T19:25:07.616Z"
updated: "2026-02-20T20:00:14.562Z"
dependencies: []
---

# Fix performance: updateSavedTerminals hot path and ServicesTab interval placement

## Problem Statement

createEffect in App.tsx:219 triggers O(repos x branches x terminals) updateSavedTerminals scan + debounced IPC save on every terminal mutation (fires dozens of times/sec during I/O). savedTerminals only consumed at startup so continuous updates have no UX benefit. ServicesTab.tsx:63 calls setInterval at component body instead of onMount.

## Acceptance Criteria

- [ ] Verify before: confirm createEffect with terminalsStore.getIds() at App.tsx:219 and bare setInterval at ServicesTab.tsx:63
- [ ] Remove the createEffect from App.tsx:219; confirm snapshotTerminals() at quit handles persistence
- [ ] Move setInterval and initial calls into onMount in ServicesTab.tsx
- [ ] Run make check and all tests pass
- [ ] Verify terminal persistence still works on app restart

## Files

- src/App.tsx
- src/components/SettingsPanel/tabs/ServicesTab.tsx
- src/stores/repositories.ts

## Work Log

