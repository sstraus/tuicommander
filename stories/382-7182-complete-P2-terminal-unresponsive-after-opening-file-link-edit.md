---
id: 382-7182
title: Terminal unresponsive after opening file link, editor tab close removes terminal tab
status: complete
priority: P2
created: "2026-02-25T08:52:31.883Z"
updated: "2026-02-25T17:48:47.380Z"
dependencies: []
---

# Terminal unresponsive after opening file link, editor tab close removes terminal tab

## Problem Statement

When a file path link is clicked in terminal output, an editor tab opens and the terminal becomes unresponsive (input blocked). When the editor tab is closed, the terminal tab visually disappears from the tab bar. The PTY is still alive but unreachable via UI until the user manually clicks the terminal tab (if still accessible).

Root cause: handleOpenFilePath (App.tsx:510) calls handleTerminalSelect(editorTabId) which sets terminalsStore.setActive(null) — hiding the terminal pane via CSS and blurring xterm. When the editor tab is closed, closeTerminal (useTerminalLifecycle.ts:79-82) removes the editor tab and returns early WITHOUT restoring terminalsStore.activeId. Since activeId was set to null when the editor tab opened, it stays null.

## Acceptance Criteria

- [ ] Clicking a file link in terminal output opens the editor tab without making the terminal unresponsive
- [ ] Closing the editor tab (or any non-terminal tab: diff-, md-, edit-) restores focus to the most recently active terminal
- [ ] The terminal tab does not disappear from the tab bar after closing an editor tab
- [ ] PTY session remains accessible throughout the file-open/close cycle

## Files

- src/App.tsx
- src/hooks/useTerminalLifecycle.ts
- src/components/TerminalArea.tsx
- src/styles.css

## Work Log

### 2026-02-25T17:48:43.892Z - Already fixed: selectAfterNonTerminalClose restores terminal focus when closing editor/diff/md tabs. Test at useTerminalLifecycle.test.ts line 304-320 confirms the fix.

