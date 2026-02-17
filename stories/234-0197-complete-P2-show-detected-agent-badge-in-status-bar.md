---
id: 234-0197
title: Show detected agent badge in status bar
status: complete
priority: P2
created: "2026-02-17T10:28:43.734Z"
updated: "2026-02-17T10:28:59.246Z"
dependencies: []
---

# Show detected agent badge in status bar

## Problem Statement

Users cannot see which AI agent is running in the active terminal from the status bar.

## Acceptance Criteria

- [ ] Agent badge appears in status bar left section when active terminal has agentType
- [ ] Uses AGENT_DISPLAY for icon and color from agents.ts
- [ ] CSS class status-agent-badge with appropriate styling

## Files

- src/components/StatusBar/StatusBar.tsx
- src/styles.css

## Work Log

### 2026-02-17T10:28:59.168Z - Added agent badge in StatusBar left section using AGENT_DISPLAY icon/color. CSS class status-agent-badge styled.

