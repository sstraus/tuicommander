---
id: 376-b8a7
title: sessionPromptPlugin - add unit tests (currently zero coverage)
status: complete
priority: P2
created: "2026-02-25T07:45:22.407Z"
updated: "2026-02-25T08:27:47.714Z"
dependencies: []
---

# sessionPromptPlugin - add unit tests (currently zero coverage)

## Problem Statement

sessionPromptPlugin.ts has zero tests. Logic reconstructs user-typed input lines from keystroke PTY data via input_line_buffer.rs - needs test coverage.

## Acceptance Criteria

- [ ] Tests for line reconstruction from keystrokes: backspace, enter, escape
- [ ] Tests for registerOutputWatcher integration (mock host)
- [ ] Tests for registerSection rendering
- [ ] make check passes with no failures

## Files

- src/__tests__/plugins/sessionPromptPlugin.test.ts
- src/plugins/sessionPromptPlugin.ts

## Work Log

### 2026-02-25T08:27:47.645Z - Tests already implemented by prior session: 32 tests in sessionPromptPlugin.test.ts covering lifecycle, events, content, markdown provider, payload validation. All passing.

