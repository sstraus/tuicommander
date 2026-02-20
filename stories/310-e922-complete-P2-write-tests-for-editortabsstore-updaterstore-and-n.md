---
id: 310-e922
title: Write tests for editorTabsStore, updaterStore, and notesStore
status: complete
priority: P2
created: "2026-02-20T19:25:07.618Z"
updated: "2026-02-20T20:08:06.737Z"
dependencies: []
---

# Write tests for editorTabsStore, updaterStore, and notesStore

## Problem Statement

editorTabsStore (tab lifecycle with deduplication), updaterStore (10s timeout, friendly errors, download/install state machine), and notesStore (CRUD with empty-string guard and persistence) all have no test files.

## Acceptance Criteria

- [ ] Verify before: confirm no test files exist for these three stores
- [ ] Create editorTabs.test.ts: add() deduplication, remove() with fallback active, clearForRepo(), getActive()
- [ ] Create updater.test.ts: checking state, available/not-available, 10s timeout, friendly error mapping (404 and fetch errors), downloadAndInstall progress, dismiss()
- [ ] Create notes.test.ts: addNote() trims, guards empty string, prepends; removeNote() by id; hydrate() from backend
- [ ] Run tests and confirm all pass

## Files

- src/stores/editorTabs.ts
- src/stores/updater.ts
- src/stores/notes.ts
- src/__tests__/stores/editorTabs.test.ts
- src/__tests__/stores/updater.test.ts
- src/__tests__/stores/notes.test.ts

## Work Log

### 2026-02-20T20:08:06.663Z - Created editorTabs.test.ts (20 tests: add deduplication, remove fallback, clearForRepo, getActive, setDirty), updater.test.ts (12 tests: checking state, available/not, timeout, friendly errors, progress, dismiss), notes.test.ts (14 tests: addNote trim/empty guard/prepend/persist, removeNote, hydrate, count). All 1672 tests pass.

