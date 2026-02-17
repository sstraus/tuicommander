# Hooks Reference

Hooks contain business logic and side effects, bridging stores and Tauri commands.

## useAppInit

**File:** `src/hooks/useAppInit.ts`

Initializes the application on startup.

```typescript
export function initApp(deps: AppInitDeps): Promise<void>
```

**What it does:**
1. Hydrates all stores from Rust backend (settings, repos, UI, prompts, etc.)
2. Detects installed binaries (Claude, Aider, lazygit)
3. Applies platform CSS class (`platform-darwin`, `platform-win32`, `platform-linux`)
4. Sets up close handler (quit confirmation dialog)
5. Starts GitHub polling
6. Loads custom fonts from settings
7. Refreshes dictation config

---

## usePty

**File:** `src/hooks/usePty.ts`

Low-level PTY session management. Wraps Tauri PTY commands.

### Return API

| Method | Description |
|--------|-------------|
| `canSpawn()` | Check if under session limit (50) |
| `createSession(config)` | Create PTY session, returns session ID |
| `createSessionWithWorktree(ptyConfig, wtConfig)` | Create worktree + PTY |
| `write(sessionId, data)` | Write to PTY |
| `resize(sessionId, rows, cols)` | Resize PTY |
| `pause(sessionId)` | Pause reader thread |
| `resume(sessionId)` | Resume reader thread |
| `close(sessionId, cleanupWorktree)` | Close PTY session |
| `getStats()` | Get orchestrator stats |
| `getMetrics()` | Get session metrics |
| `listWorktrees()` | List managed worktrees |
| `getWorktreesDir()` | Get worktrees directory |
| `listActiveSessions()` | List all active sessions |

---

## useGitOperations

**File:** `src/hooks/useGitOperations.ts`

High-level git workflows: branch switching, worktree creation, repo management.

### Dependencies

```typescript
interface GitOperationsDeps {
  createTerminal: (repoPath, branch, opts?) => Promise<void>;
  closeTerminal: (id) => void;
  // ... other callbacks from App.tsx
}
```

### Return API

| Method | Description |
|--------|-------------|
| `handleBranchSelect(repoPath, branch)` | Switch to branch (creates worktree if needed) |
| `handleAddTerminalToBranch(repoPath, branch)` | Add terminal to existing branch |
| `handleRemoveRepo(repoPath)` | Remove repository from sidebar |
| `handleRemoveBranch(repoPath, branch)` | Remove worktree and branch |
| `handleRenameBranch(oldName, newName)` | Rename git branch |
| `handleAddRepo()` | Open folder dialog, add repository |
| `handleAddWorktree(repoPath)` | Create worktree with generated name |
| `handleNewTab()` | Create new tab for active branch |
| `handleRunCommand(forceDialog, openDialog)` | Execute or configure run command |
| `handleRepoSettings(repoPath, openPanel)` | Open repo-specific settings |
| `refreshAllBranchStats()` | Refresh diff stats for all branches |
| `activeWorktreePath()` | Get active worktree path |
| `activeRunCommand()` | Get active run command |

### Signals

| Signal | Type | Description |
|--------|------|-------------|
| `currentRepoPath()` | `string \| null` | Active repository path |
| `currentBranch()` | `string \| null` | Active branch name |
| `repoStatus()` | `string` | Repository git status |
| `branchToRename()` | `{repoPath, branchName} \| null` | Branch rename state |

---

## useTerminalLifecycle

**File:** `src/hooks/useTerminalLifecycle.ts`

Terminal tab management: create, close, zoom, copy/paste, reopen.

### Return API

| Method | Description |
|--------|-------------|
| `createNewTerminal()` | Create terminal for active branch |
| `closeTerminal(id, skipConfirm?)` | Close terminal (with confirmation) |
| `closeOtherTabs(keepId)` | Close all except one |
| `closeTabsToRight(afterId)` | Close tabs after given ID |
| `reopenClosedTab()` | Reopen last closed tab |
| `navigateTab(direction)` | Switch to prev/next tab |
| `clearTerminal()` | Clear active terminal |
| `copyFromTerminal()` | Copy selection from terminal |
| `pasteToTerminal()` | Paste to active terminal |
| `zoomIn()` / `zoomOut()` / `zoomReset()` | Font size controls |
| `activeFontSize()` | Get active terminal's font size |
| `handleTerminalFocus(id)` | Handle terminal focus event |
| `handleTerminalSelect(id)` | Handle tab click |
| `terminalIds()` | Memo: terminal IDs for active branch |

---

## useKeyboardShortcuts

**File:** `src/hooks/useKeyboardShortcuts.ts`

Registers global keyboard event listener with platform-aware modifiers.

```typescript
interface ShortcutHandlers {
  newTab: () => void;
  closeTab: () => void;
  toggleSidebar: () => void;
  // ... 30+ handlers
}
```

Returns cleanup function to remove listener on unmount.

---

## useGitHub

**File:** `src/hooks/useGitHub.ts`

GitHub status for a single repository.

### Return API

| Signal/Method | Description |
|---------------|-------------|
| `status()` | Reactive GitHub status |
| `loading()` | Loading state |
| `error()` | Error message |
| `refresh()` | Force refresh |
| `startPolling()` / `stopPolling()` | Polling control |

---

## useRepository

**File:** `src/hooks/useRepository.ts`

Git repository operations (lower level than useGitOperations).

### Return API

| Method | Description |
|--------|-------------|
| `getInfo(path)` | Get RepoInfo (name, branch, status) |
| `getDiff(path)` | Get full git diff |
| `getDiffStats(path)` | Get additions/deletions counts |
| `getChangedFiles(path)` | List changed files with stats |
| `getFileDiff(path, file)` | Get single file diff |
| `openInApp(path, app)` | Open in IDE |
| `renameBranch(repoPath, old, new)` | Rename branch |
| `createWorktree(base, branch)` | Create worktree |
| `removeWorktree(repo, branch)` | Remove worktree |
| `getWorktreePaths(repo)` | Get worktree paths |
| `listMarkdownFiles(path)` | List .md files |
| `readFile(path, file)` | Read file contents |
| `generateWorktreeName(existing)` | Generate unique worktree name |

---

## useQuickSwitcher

**File:** `src/hooks/useQuickSwitcher.ts`

Held-key branch quick-switcher (Cmd+Ctrl on macOS, Ctrl+Alt on Win/Linux).

```typescript
switchToBranchByIndex(index: number): void
```

---

## useSplitPanes

**File:** `src/hooks/useSplitPanes.ts`

Split terminal pane management.

```typescript
handleSplit(direction: "vertical" | "horizontal"): void
```

---

## useConfirmDialog

**File:** `src/hooks/useConfirmDialog.ts`

Confirmation and info dialogs using Tauri dialog plugin.

| Method | Description |
|--------|-------------|
| `confirm(options)` | Show Yes/No confirmation |
| `info(title, message)` | Show info dialog |
| `error(title, message)` | Show error dialog |
| `confirmRemoveWorktree(branch)` | Confirm worktree removal |
| `confirmCloseTerminal(name)` | Confirm terminal close |
| `confirmRemoveRepo(name)` | Confirm repo removal |

---

## useDictation

**File:** `src/hooks/useDictation.ts`

Push-to-talk dictation integration.

| Method | Description |
|--------|-------------|
| `handleDictationStart()` | Start recording |
| `handleDictationStop()` | Stop and transcribe, inject text |

---

## useAgentDetection

**File:** `src/hooks/useAgentDetection.ts`

Detect installed AI agents and IDEs.

| Method | Description |
|--------|-------------|
| `detectAll()` | Detect all known agents |
| `detectAgent(type, binary)` | Detect specific agent |
| `isAvailable(type)` | Check if agent is available |
| `getAvailable()` | Get all available agents |
| `getDetection(type)` | Get detection result (path, version) |

---

## useAppLazygit

**File:** `src/hooks/useAppLazygit.ts`

Lazygit integration (inline pane or floating window).

| Method | Description |
|--------|-------------|
| `spawnLazygit()` | Spawn lazygit inline |
| `openLazygitPane()` | Open lazygit pane |
| `closeLazygitPane()` | Close lazygit pane |
| `buildLazygitCmd(repoPath)` | Build lazygit command for repo |

---

## useKeyboardRedirect

**File:** `src/hooks/useKeyboardRedirect.ts`

Redirects keyboard events from sidebar to active terminal (bypasses focus trap). Setup-only hook, no return value.
