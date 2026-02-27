---
id: 405-ad47
title: Evolve notification system using debounced busy signal
status: complete
priority: P2
created: "2026-02-26T20:50:26.146Z"
updated: "2026-02-27T11:19:02.174Z"
dependencies: []
---

# Evolve notification system using debounced busy signal

## Problem Statement

The notification system has several weaknesses rooted in the raw 500ms shellState timer: (1) Sound notifications only play for NON-active terminals — if you are watching a terminal and it hits an error or completes, no sound. (2) Completion sound only fires on session EXIT, not when a long-running command finishes (busy→idle transition). (3) Sleep prevention in App.tsx uses raw shellState which flickers every 500ms gap, causing rapid block_sleep/unblock_sleep calls. (4) The 2-second debounced busy pattern was implemented in Sidebar RepoSection but is local — other consumers (TabBar, App.tsx sleep, notifications) cannot reuse it. (5) There is no notification for "task done" when a terminal was busy for >N seconds and becomes idle (common use case: build finished, tests done).

## Acceptance Criteria

- [ ] Centralize the debounced busy signal into terminalsStore (e.g. terminalsStore.isBusy(id) with 2s hold) so all consumers share one source of truth instead of each component reimplementing it
- [ ] Refactor Sidebar RepoSection.tsx to use the centralized store signal instead of local createEffect+setTimeout
- [ ] Refactor App.tsx sleep prevention (line 302-318) to use centralized debounced busy instead of raw shellState — prevents rapid block_sleep/unblock_sleep toggles
- [ ] Add completion notification: when a terminal transitions from busy→idle after being busy for >=5 seconds, play the completion sound and set activity flag — even for the active terminal (user is watching but may have looked away)
- [ ] Refactor TabBar to use centralized busy signal for consistent tab styling
- [ ] Update the notification sound logic to also fire for active terminals when awaitingInput is set (question/error) — the user may be looking at a different part of the screen

## Files

- src/stores/terminals.ts
- src/components/Sidebar/RepoSection.tsx
- src/components/TabBar/TabBar.tsx
- src/App.tsx
- src/components/Terminal/Terminal.tsx
- src/notifications.ts

## Related

- 403

## QA

None — covered by tests

## Work Log

### 2026-02-27T11:19:02.026Z - Centralized debounced busy signal in terminalsStore with reactive SolidJS state (debouncedBusy record). Added isBusy(id), isAnyBusy(), getBusyDuration(id), onBusyToIdle(). Replaced local 2s debounce in RepoSection. Refactored App.tsx sleep prevention to use isAnyBusy(). Added shellBusy CSS class to TabBar. Added completion notification (>=5s busy→idle) that fires even for active terminal. Removed background-only guard from question and API-error notification sounds. 16 new unit tests.

