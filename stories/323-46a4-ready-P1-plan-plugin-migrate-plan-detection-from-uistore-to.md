---
id: 323-46a4
title: "Plan plugin: migrate plan detection from uiStore to plugin system"
status: ready
priority: P1
created: "2026-02-21T09:34:49.510Z"
updated: "2026-02-21T10:02:49.051Z"
dependencies: ["320-e879"]
---

# Plan plugin: migrate plan detection from uiStore to plugin system

## Problem Statement

Currently plan file detection sets uiStore.planFilePath directly from Terminal.tsx. This must become a plugin that listens for structured plan-file events, adds an ActivityItem to activityStore, and provides virtual markdown content for the plan file.

## Acceptance Criteria

- [ ] src/plugins/planPlugin.ts implements TuiPlugin interface
- [ ] onload registers section { id: plan, label: PLAN, priority: 10, canDismissAll: false }
- [ ] onload registers structured event handler for type plan-file
- [ ] On plan-file event: adds ActivityItem with title=plan display name, subtitle=path, icon=document SVG, contentUri=plan:file?path=..., dismissible=true
- [ ] Plan-file event with same path deduplicates (updates existing item instead of adding duplicate)
- [ ] MarkdownProvider for plan scheme: reads file via invoke(read_file, { path }) and returns content
- [ ] onunload disposes all registrations via Disposable.dispose()
- [ ] All plan plugin tests pass

## Files

- src/plugins/planPlugin.ts
- src/__tests__/plugins/planPlugin.test.ts

## Related

- 320-e879

## Work Log

