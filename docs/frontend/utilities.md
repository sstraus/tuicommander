# Utilities

Pure functions and helpers in `src/utils/`.

## Branch Sorting (`branchSort.ts`)

```typescript
compareBranches(a: SortableBranch, b: SortableBranch, aPr?: BranchPrState, bPr?: BranchPrState): number
```

Sort priority:
1. Active branch first
2. Main branches (main, master, develop, trunk)
3. Branches with open PRs (alphabetical)
4. Feature branches without PRs (alphabetical)
5. Merged/closed PR branches (last, alphabetical)

**Note:** This is a frontend wrapper around Rust's `sort_branches()`. The sorting rules are implemented in Rust; this utility provides the TypeScript interface.

## CI Ring Segments (`ciRingSegments.ts`)

```typescript
computeCiRingSegments(
  failed: number,
  pending: number,
  passed: number,
  circumference: number,
  colors: { passed: string, failed: string, pending: string }
): CiRingSegment[]
```

Calculates SVG arc segments for the circular CI status indicator. Returns array of segments with `offset`, `length`, and `color` for each status category.

## PR State Mapping (`prStateMapping.ts`)

```typescript
classifyMergeState(mergeable?: string, mergeStateStatus?: string): StateLabel | null
classifyReviewState(reviewDecision?: string): StateLabel | null
```

Maps GitHub merge state and review decision to display labels with CSS classes. Frontend mirror of Rust's `classify_merge_state()` / `classify_review_state()`.

## Terminal Utilities

### terminalFilter.ts

```typescript
filterValidTerminals(branchTerminals: string[], existingTerminalIds: string[]): string[]
```

Filters branch's terminal list to only include IDs that exist in the terminals store. Handles cleanup of stale references.

### terminalOrphans.ts

```typescript
findOrphanTerminals(terminalIds: string[], branchTerminalMap: Record<string, string[]>): string[]
```

Finds terminals that exist in the store but aren't associated with any branch. Used for cleanup.

## Shell Utilities (`shell.ts`)

| Function | Description |
|----------|-------------|
| `escapeShellArg(arg)` | Escape string for safe shell argument |
| `isValidBranchName(name)` | Validate git branch name format |
| `isValidPath(path)` | Validate file system path |

## Hotkey Utilities (`hotkey.ts`)

| Function | Description |
|----------|-------------|
| `hotkeyToTauriShortcut(hotkey)` | Convert display format to Tauri format |
| `tauriShortcutToHotkey(shortcut)` | Convert Tauri format to display format |

## Time Utilities (`time.ts`)

```typescript
relativeTime(isoString: string): string
```

Formats ISO timestamp as relative time (e.g., "5 minutes ago", "2 hours ago", "yesterday").

## Main Index (`utils/index.ts`)

General utilities including:
- Platform detection helpers
- Path manipulation
- Theme conversion utilities
- Hotkey conversion helpers (display ↔ Tauri format)

## Type Definitions (`types/index.ts`)

All shared TypeScript types are centralized in a single file. Key type groups:

### Terminal Types
`TerminalPane`, `TerminalRef`, `PtyOutput`, `PtyExit`, `PtyConfig`, `IPty`, `SessionState`, `SavedTerminal`

### Repository Types
`RepoInfo`, `Repository`

### GitHub Types
`GitHubStatus`, `PrStatus`, `CiStatus`, `CheckSummary`, `CheckDetail`, `BranchPrStatus`, `PrLabel`

### PR State Types
`MergeableState`, `MergeStateStatus`, `ReviewDecision`

### UI Types
`SplitNode`, `SplitDirection`, `AgentStats`, `DetectedPrompt`, `OrchestratorStats`

### Callback Types
`PtyDataHandler`, `PtyExitHandler`
