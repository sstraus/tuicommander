---
id: 280-b501
title: window.prompt used in Sidebar despite PromptDialog existing
status: complete
priority: P2
created: "2026-02-20T13:57:16.822Z"
updated: "2026-02-20T14:09:59.311Z"
dependencies: []
---

# window.prompt used in Sidebar despite PromptDialog existing

## Problem Statement

window.prompt at Sidebar.tsx:144 does not work in Tauri webview. PromptDialog was created specifically to replace it.

## Acceptance Criteria

- [ ] window.prompt replaced with PromptDialog component

## Files

- src/components/Sidebar/Sidebar.tsx

## Work Log

### 2026-02-20T14:09:59.236Z - Already resolved - window.prompt was already replaced with PromptDialog

