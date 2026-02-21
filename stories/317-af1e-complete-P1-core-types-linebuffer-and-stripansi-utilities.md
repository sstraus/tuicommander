---
id: 317-af1e
title: Core types, LineBuffer and stripAnsi utilities
status: complete
priority: P1
created: "2026-02-21T09:30:48.737Z"
updated: "2026-02-21T10:06:03.484Z"
dependencies: []
---

# Core types, LineBuffer and stripAnsi utilities

## Problem Statement

The plugin system needs shared TypeScript interfaces (TuiPlugin, PluginHost, ActivityItem, etc.) and two utilities: LineBuffer to reassemble PTY chunks into complete lines, and stripAnsi to remove ANSI escape codes before regex matching.

## Acceptance Criteria

- [ ] src/plugins/types.ts defines all shared interfaces: ActivityItem, ActivitySection, TuiPlugin, PluginHost, OutputWatcher, MarkdownProvider, Disposable
- [ ] src/utils/lineBuffer.ts exports LineBuffer class: push(chunk) returns complete lines, retains partial trailing line
- [ ] src/utils/stripAnsi.ts exports stripAnsi function: removes SGR, OSC, cursor movement sequences
- [ ] Tests pass for chunk splitting, partial lines, empty input, multi-byte chars
- [ ] Tests pass for ANSI stripping of all code types including nested sequences

## Files

- src/plugins/types.ts
- src/utils/lineBuffer.ts
- src/utils/stripAnsi.ts
- src/__tests__/utils/lineBuffer.test.ts
- src/__tests__/utils/stripAnsi.test.ts

## Work Log

### 2026-02-21T10:06:03.412Z - Implemented LineBuffer (push returns complete lines, retains partial tail), stripAnsi (ECMA-48 CSI+OSC, fixed param-byte range \x30-\x3F), and types.ts (all plugin interfaces). 31 new tests, 1671 total passing.

