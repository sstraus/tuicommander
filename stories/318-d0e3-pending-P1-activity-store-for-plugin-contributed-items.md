---
id: 318-d0e3
title: Activity store for plugin-contributed items
status: pending
priority: P1
created: "2026-02-21T09:33:07.572Z"
updated: "2026-02-21T09:35:41.439Z"
dependencies: ["317-af1e"]
---

# Activity store for plugin-contributed items

## Problem Statement

Plugins need a reactive SolidJS store to register sections and add/remove/dismiss activity items. This store is the data backbone for the bell dropdown plugin sections, separate from prNotificationsStore which remains unchanged.

## Acceptance Criteria

- [ ] src/stores/activityStore.ts exports activityStore singleton
- [ ] registerSection(section): adds section, returns Disposable that removes it on dispose()
- [ ] addItem(item): adds item with createdAt timestamp; removeItem(id) and updateItem(id, updates) work correctly
- [ ] getActive(): returns all non-dismissed items across all sections
- [ ] getForSection(sectionId): returns non-dismissed items for a specific section
- [ ] getLastItem(): returns the most recently created non-dismissed item across all sections
- [ ] dismissItem(id) and dismissSection(sectionId) mark items as dismissed
- [ ] clearAll() resets all items and sections (for testing)
- [ ] All tests pass

## Files

- src/stores/activityStore.ts
- src/__tests__/stores/activityStore.test.ts

## Work Log

