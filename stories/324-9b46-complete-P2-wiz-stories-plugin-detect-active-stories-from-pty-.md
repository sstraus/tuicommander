---
id: 324-9b46
title: "Wiz stories plugin: detect active stories from PTY output"
status: complete
priority: P2
created: "2026-02-21T09:34:57.240Z"
updated: "2026-02-21T10:38:54.498Z"
dependencies: ["320-e879"]
---

# Wiz stories plugin: detect active stories from PTY output

## Problem Statement

When wiz:work runs a story, TUICommander should detect it from PTY output and show the active story in the bell dropdown with a virtual markdown view. The plugin watches for story-related output patterns and generates formatted markdown from the story JSON.

## Acceptance Criteria

- [ ] src/plugins/wizStoriesPlugin.ts implements TuiPlugin interface
- [ ] onload registers section { id: stories, label: STORIES, priority: 20, canDismissAll: false }
- [ ] onload registers output watcher(s) with patterns validated against actual wiz:stories CLI output
- [ ] On pattern match: adds ActivityItem with title=story short title, subtitle=story id + status, icon=bolt SVG, contentUri=stories:detail?id=...
- [ ] MarkdownProvider for stories scheme: reads story file from disk, parses JSON, returns formatted markdown
- [ ] Multiple stories can be active simultaneously (each gets its own item)
- [ ] onunload disposes all registrations
- [ ] All stories plugin tests pass
- [ ] NOTE: exact regex patterns must be validated against real wiz:stories output during implementation

## Files

- src/plugins/wizStoriesPlugin.ts
- src/__tests__/plugins/wizStoriesPlugin.test.ts

## Related

- 320-e879

## Work Log

### 2026-02-21T10:32:14.059Z - Starting implementation

### 2026-02-21T10:38:50.691Z - Implemented wizStoriesPlugin: STATUS and WORKLOG patterns, MarkdownProvider via list_markdown_files+read_file, factory for injectable storiesDir. 22 tests green.

