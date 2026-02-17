# Split Panes — Terminal Layout Proposal

**Status:** Designed
**Date:** 2026-02-16

## Problem

Currently each tab shows a single terminal. Users who need multiple terminals visible simultaneously must use separate tabs and can't see them side-by-side.

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Branch model | Branch-bound | Split layout belongs to the branch, switches with it |
| Nesting depth | Single level only | Max 2 panes per tab. Covers 90% of use cases, vastly simpler |
| New pane content | New terminal, same cwd | Matches iTerm2 behavior |
| Lazygit | Stays special | Keeps its own float/dock mechanism, independent of splits |
| Close behavior | Expand remaining | Closing one pane collapses the split, survivor goes full |
| Diff/Markdown panels | Outside split system | Stay as flex siblings of the terminal area |

## Data Model

No binary tree needed — single-level splits are a flat structure:

```typescript
interface TabLayout {
  direction: "none" | "vertical" | "horizontal";
  panes: [string] | [string, string]; // terminal IDs
  ratio: number; // 0.0–1.0, first pane gets this fraction (default 0.5)
  activePaneIndex: 0 | 1;
}
```

- `direction: "none"` = single terminal (current behavior)
- `direction: "vertical"` = two panes side by side
- `direction: "horizontal"` = two panes stacked

## Rendering

No recursive component. Just a flex container:

```
direction: "none"
  #terminal-panes
    [Terminal A fills 100%]

direction: "vertical"
  #terminal-panes (flex-direction: row)
    [Terminal A] [resize-handle] [Terminal B]

direction: "horizontal"
  #terminal-panes (flex-direction: column)
    [Terminal A]
    [resize-handle]
    [Terminal B]
```

CSS requirements:
- `min-width: 0` and `min-height: 0` on pane containers
- Resize handle: 8px transparent hit area, 1px visible border line
- Active pane indicator: subtle accent-colored border on focused pane

## User Interactions

### Keyboard Shortcuts

| Action | macOS | Notes |
|--------|-------|-------|
| Split vertical | `Cmd+\` | New terminal to the right |
| Split horizontal | `Cmd+Opt+\` | New terminal below |
| Navigate panes | `Alt+Arrow` | Toggle focus between panes |
| Close pane | `Cmd+W` | If split: collapse. If single: close tab |
| Maximize/restore | `Cmd+Shift+Enter` | Temporarily zoom active pane to full |

### Split Actions

1. **Split** (`Cmd+\`): If tab is single pane, create split. If already split, do nothing (single level limit).
2. **Navigate** (`Alt+Left/Right` or `Alt+Up/Down`): Toggle `activePaneIndex` between 0 and 1.
3. **Resize**: Drag handle changes `ratio`. Constrain to 0.2–0.8 to prevent unusably small panes.
4. **Close pane** (`Cmd+W`): Kill PTY, remove pane, collapse to single. If already single, close tab as today.
5. **Maximize** (`Cmd+Shift+Enter`): Temporarily hide the other pane (CSS only), restore on repeat.

### Focus Tracking

`activePaneIndex` determines which pane receives keyboard input. Visual indicator: accent-colored left/top border on the active pane.

## Branch Integration

Each branch's state gains a `layout: TabLayout` (or the layout lives alongside the branch's terminal list). When switching branches:
- The entire layout switches (same as current terminal list switching)
- Split state, ratio, and active pane are preserved per branch

## Performance Notes

- Max 2 xterm.js instances rendering simultaneously (acceptable)
- Each visible pane has its own PTY reader thread (already supported)
- Resize handle drag triggers `fit()` on both panes — debounce to avoid jank

## Implementation Scope

### Files affected

| File | Change |
|------|--------|
| `src/stores/terminals.ts` | Add `TabLayout` type, layout state per branch |
| `src/App.tsx` | Render 1 or 2 panes based on layout, wire split/navigate/close |
| `src/styles.css` | Split container, resize handle, active pane indicator |
| `src/components/Terminal/Terminal.tsx` | Support being visible without global `activeId` |
| `src/components/TabBar/TabBar.tsx` | Optional: show split indicator on tab |

### Migration path

1. Add `TabLayout` to state, default all existing tabs to `direction: "none"`
2. Render layout — single pane works exactly as before (zero visual change)
3. Wire `Cmd+\` to create vertical split (spawns new terminal, same cwd)
4. Add resize handle between panes
5. Wire `Alt+Arrow` for focus navigation
6. Wire `Cmd+W` close-pane with collapse logic
7. Optional: `Cmd+Shift+Enter` maximize/restore
