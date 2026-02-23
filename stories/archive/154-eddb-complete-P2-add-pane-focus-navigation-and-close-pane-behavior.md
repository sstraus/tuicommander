---
id: 154-eddb
title: Add pane focus navigation and close-pane behavior
status: complete
priority: P2
created: "2026-02-15T23:39:35.058Z"
updated: "2026-02-16T00:29:49.575Z"
dependencies: ["152-b7a2"]
---

# Add pane focus navigation and close-pane behavior

## Problem Statement

Users need Alt+Arrow to toggle focus between split panes, and Cmd+W should close the active pane (collapsing the split) rather than the whole tab. If only one pane remains, Cmd+W closes the tab as before.

## Acceptance Criteria

- [ ] Alt+Left/Right toggles activePaneIndex in vertical splits
- [ ] Alt+Up/Down toggles activePaneIndex in horizontal splits
- [ ] Only the focused pane receives keyboard input
- [ ] Cmd+W in a split: kills active pane PTY, collapses to single pane
- [ ] Cmd+W in a single pane: closes tab (existing behavior preserved)
- [ ] Update HelpPanel with new keyboard shortcuts

## Files

- src/App.tsx
- src/components/HelpPanel/HelpPanel.tsx

## Work Log

### 2026-02-16T00:29:49.508Z - Added Alt+Arrow pane navigation (left/right for vertical, up/down for horizontal). Updated Cmd+W to close active pane and collapse split before falling back to tab close. Added Split Panes section to HelpPanel with all new shortcuts.

