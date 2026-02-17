---
id: 125-4ce1
title: Tab context menu with iTerm2-style operations
status: complete
priority: P2
created: "2026-02-15T18:21:24.443Z"
updated: "2026-02-15T22:02:42.063Z"
dependencies: []
---

# Tab context menu with iTerm2-style operations

## Problem Statement

TUI Commander has no right-click context menu on tabs. Users can only close via the X button or Cmd+W, and rename via double-click (not discoverable). iTerm2 and other terminal apps offer Close Others, Close to Right, and Rename via right-click — standard tab management UX that users expect.

## Acceptance Criteria

- [ ] Right-click on terminal tab shows context menu with: Close Tab, Close Other Tabs, Close Tabs to the Right, separator, Rename Tab
- [ ] Close Other Tabs closes all sibling tabs in current branch except the right-clicked one, with confirmation if active sessions exist
- [ ] Close Tabs to the Right closes all tabs after the right-clicked one, disabled when tab is last
- [ ] Rename Tab enters inline rename mode (reuses existing double-click behavior)
- [ ] Diff and markdown tabs also get context menu scoped to their tab type (Close, Close Others, Close to Right)
- [ ] Reuse existing ContextMenu component and createContextMenu hook

## Files

- src/components/TabBar/TabBar.tsx
- src/components/ContextMenu/ContextMenu.tsx
- src/App.tsx

## Work Log

### 2026-02-15T18:25:37.987Z - DESIGN DECISION: Web-rendered context menu (not Tauri native). Existing ContextMenu component (src/components/ContextMenu/ContextMenu.tsx) already handles viewport-aware positioning, Escape/click-outside to close, shortcut hints, disabled states, separators. Tauri native menu API targets app menus/system tray, not arbitrary right-click on DOM elements. Staying web-rendered keeps consistency with existing terminal-pane context menu.

### 2026-02-15T18:25:41.971Z - IMPL STEP 1 - TabBar props: Extend TabBarProps (TabBar.tsx:8-14) with onCloseOthers: (id: string) => void and onCloseToRight: (id: string) => void.

### 2026-02-15T18:25:46.405Z - IMPL STEP 2 - Context menu state in TabBar: Import createContextMenu and ContextMenu from existing component. Add signals: tabContextMenu = createContextMenu(), contextTabId signal to track which tab was right-clicked (may differ from active tab), contextTabType signal to track tab type (terminal/diff/md).

### 2026-02-15T18:25:50.579Z - IMPL STEP 3 - Wire onContextMenu on terminal tab div (TabBar.tsx:118): e.preventDefault(), e.stopPropagation() to prevent bubble to terminal-panes context menu (App.tsx:1233), setContextTabId(id), setContextTabType('terminal'), tabContextMenu.open(e).

### 2026-02-15T18:25:55.558Z - IMPL STEP 4 - Terminal tab context menu items: Close Tab (shortcut hint Cmd+W), Close Other Tabs (disabled if only 1 tab), Close Tabs to the Right (disabled if last or only tab), separator, Rename Tab (calls setEditingId to reuse double-click inline rename). Items derived from contextTabId() + activeTerminals() array position.

### 2026-02-15T18:25:59.710Z - IMPL STEP 5 - Render ContextMenu in TabBar: Add <ContextMenu items={getTabContextMenuItems()} x={tabContextMenu.position().x} y={tabContextMenu.position().y} visible={tabContextMenu.visible()} onClose={tabContextMenu.close} /> before closing </div> of TabBar return.

### 2026-02-15T18:26:05.822Z - IMPL STEP 6 - Bulk close handlers in App.tsx (add after closeTerminal at line 347): closeOtherTerminals(keepId) — filters terminalIds() to exclude keepId, counts active sessions, shows single confirmation via dialogs.confirm() if activeCount > 0 (message: 'Close N tabs? M running processes will be terminated'), then loops closeTerminal(id, true) for each. Ensures keepId stays active via handleTerminalSelect(keepId). closeTerminalsToRight(afterId) — same pattern but slices ids.slice(afterIndex + 1).

### 2026-02-15T18:26:09.508Z - IMPL STEP 7 - Wire new props at App.tsx:1217: Add onCloseOthers={closeOtherTerminals} and onCloseToRight={closeTerminalsToRight} to TabBar component.

### 2026-02-15T18:26:15.634Z - IMPL STEP 8 - Diff/markdown tab context menus: Add onContextMenu to diff tab div (TabBar.tsx:194) and md tab div (TabBar.tsx:229). Simpler menu — no Rename (these are file-based tabs). Close calls diffTabsStore.remove()/mdTabsStore.remove(). Close Others/Close to Right operate on respective store getIds() array. Use contextTabType signal to dispatch correct menu builder.

### 2026-02-15T18:26:19.434Z - IMPL STEP 9 - Event propagation: Tab onContextMenu must e.stopPropagation() to prevent bubble to #terminal-panes div (App.tsx:1233) which has its own Copy/Paste/Clear/Lazygit context menu.

### 2026-02-15T18:26:23.720Z - IMPL STEP 10 - Tests: Add tests in src/__tests__/components/TabBar.test.tsx: (1) right-click on terminal tab triggers context menu, (2) Close Others callback receives correct tab ID, (3) Close to Right callback receives correct tab ID, (4) menu items disabled when single tab or last tab, (5) Rename enters edit mode.

### 2026-02-15T18:26:28.055Z - OUT OF SCOPE: Tab color (no theming system), Move to new window (single-window app), Pin tab (no pinning concept), Tauri native menu.

### 2026-02-15T22:02:41.887Z - Implemented tab context menu: right-click on any tab (terminal, diff, markdown) shows Close Tab, Close Other Tabs, Close Tabs to Right. Terminal tabs also get Rename Tab and Cmd+W hint. Added closeOtherTabs and closeTabsToRight handlers in App.tsx. Reused existing ContextMenu component and createContextMenu hook. All 916 tests pass.

