# TUICommander Architecture

## Overview

TUICommander is a Tauri application that provides a multi-terminal interface with git worktree support. It allows users to manage multiple coding sessions across different branches/worktrees of a repository.

## Tech Stack

| Layer | Technology |
|-------|------------|
| Frontend | SolidJS + TypeScript |
| Build | Vite |
| Backend | Tauri (Rust) |
| Terminal | xterm.js + WebGL renderer |
| State | Custom reactive stores |

## Directory Structure

```
src/
├── components/
│   ├── Sidebar/          # Repository tree, branch selection
│   ├── TabBar/           # Terminal tabs per branch
│   ├── Terminal/         # xterm.js wrapper
│   ├── HelpPanel/        # Help overlay
│   └── ...
├── stores/
│   ├── terminals.ts      # Terminal instances state
│   └── repositories.ts   # Repository/branch/worktree state
├── hooks/
│   ├── usePty.ts         # PTY session management
│   └── useRepository.ts  # Git operations
├── App.tsx               # Main orchestration
└── types/                # TypeScript definitions

src-tauri/
├── src/
│   ├── main.rs           # Tauri entry point
│   ├── pty.rs            # PTY commands
│   └── repo.rs           # Git worktree commands
└── Cargo.toml
```

## Core Data Models

### Terminal State (`src/stores/terminals.ts`)

```typescript
interface TerminalData {
  id: string;                    // e.g., "term-1"
  sessionId: string | null;      // PTY session ID from backend
  fontSize: number;              // Per-pane zoom level
  name: string;                  // Display name (e.g., "main 1")
  awaitingInput: AwaitingInputType;
}

interface TerminalsStoreState {
  terminals: Record<string, TerminalState>;
  activeId: string | null;
  counter: number;
}
```

### Repository State (`src/stores/repositories.ts`)

```typescript
interface BranchState {
  name: string;
  isMain: boolean;              // true for main/master/develop
  terminals: string[];          // Terminal IDs belonging to this branch
  additions: number;            // Git diff stats
  deletions: number;
}

interface RepositoryState {
  path: string;
  displayName: string;
  initials: string;
  expanded: boolean;
  branches: Record<string, BranchState>;
  activeBranch: string | null;
}

interface RepositoriesStoreState {
  repositories: Record<string, RepositoryState>;
  activeRepoPath: string | null;
}
```

## Key Flows

### Branch Selection Flow

```
User clicks branch in Sidebar
         ↓
handleBranchSelect(repoPath, branchName)
         ↓
1. repositoriesStore.setActive(repoPath)
2. repositoriesStore.setActiveBranch(repoPath, branchName)
3. Fetch diff stats via repo.getDiffStats()
         ↓
Check: Does branch have terminals?
         ↓
    YES → terminalsStore.setActive(branch.terminals[0])
    NO  → handleAddTerminalToBranch() → Create new terminal
```

### Tab Filtering (Current Implementation)

The TabBar filters terminals based on the active branch:

```typescript
// src/components/TabBar/TabBar.tsx
const activeTerminals = () => {
  const activeRepoPath = repositoriesStore.state.activeRepoPath;
  const repo = repositoriesStore.state.repositories[activeRepoPath];

  if (!repo || !repo.activeBranch) {
    return terminalsStore.getIds();  // Fallback: all terminals
  }

  // Return ONLY terminals for this branch
  return repo.branches[repo.activeBranch]?.terminals || [];
};
```

### Terminal Lifecycle

```
Create:
  terminalsStore.add() → Creates terminal record
  repositoriesStore.addTerminalToBranch() → Associates with branch
  Terminal component mounts → Creates xterm instance → Spawns PTY

Close:
  pty.close(sessionId) → Closes PTY in backend
  repositoriesStore.removeTerminalFromBranch() → Removes association
  terminalsStore.remove(id) → Removes terminal record
```

## State Hierarchy

```
┌─────────────────────────────────────────────────────────┐
│ repositoriesStore                                        │
│   activeRepoPath: "/path/to/repo"                       │
│   repositories: {                                        │
│     "/path/to/repo": {                                  │
│       activeBranch: "main"                              │
│       branches: {                                        │
│         "main": { terminals: ["term-1", "term-2"] }     │
│         "worktree-001": { terminals: ["term-3"] }       │
│       }                                                  │
│     }                                                    │
│   }                                                      │
└─────────────────────────────────────────────────────────┘
                          ↓ references
┌─────────────────────────────────────────────────────────┐
│ terminalsStore                                           │
│   activeId: "term-1"                                    │
│   terminals: {                                           │
│     "term-1": { name: "main 1", sessionId: "abc..." }   │
│     "term-2": { name: "main 2", sessionId: "def..." }   │
│     "term-3": { name: "worktree-001 1", sessionId: ... }│
│   }                                                      │
└─────────────────────────────────────────────────────────┘
```

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Cmd+T | New terminal tab |
| Cmd+W | Close current tab |
| Cmd+1-9 | Switch to tab N |
| Cmd+Plus | Zoom in (current terminal) |
| Cmd+Minus | Zoom out (current terminal) |
| Cmd+0 | Reset zoom |

## Known Issues & Planned Improvements

### Issue: Global Tab Visibility

**Current behavior**: When switching branches, the TabBar shows terminals from ALL branches, not just the selected one.

**Root cause**: The `activeTerminals()` function in TabBar.tsx may not be correctly filtering, or the filtering logic has a bug where terminals from other branches remain visible.

**Planned fix**: Implement proper tab scoping per worktree so each branch maintains its own isolated tab context.
