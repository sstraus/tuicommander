# Stores Reference

All stores use SolidJS `createStore` for reactive state. Each store exposes a `state` getter and action methods.

## terminalsStore

**File:** `src/stores/terminals.ts`

Manages terminal instances, active tab selection, split pane layout, and closed tab history.

### State Shape

| Field | Type | Description |
|-------|------|-------------|
| `terminals` | `Record<string, TerminalData>` | All terminals by ID |
| `activeId` | `string \| null` | Currently active terminal |
| `layout` | `TabLayout` | Split pane layout state |

### Key Types

```typescript
interface TerminalData {
  id: string;
  sessionId: string | null;
  name: string;
  repoPath: string;
  branchName: string;
  fontSize: number;
  awaitingInput: AwaitingInputType; // "question" | "error" | "confirmation" | null
  shellState: ShellState;           // "busy" | "idle" | null
  hasActivity: boolean;
}

interface TabLayout {
  direction: SplitDirection;  // "none" | "vertical" | "horizontal"
  panes: string[];            // Terminal IDs (max 2)
  ratio: number;              // Split ratio (0.2-0.8)
  activePaneIndex: number;    // 0 or 1
}
```

### Actions

| Method | Description |
|--------|-------------|
| `add(data)` | Add a terminal |
| `remove(id)` | Remove a terminal |
| `setActive(id)` | Set active terminal (clears activity flag) |
| `update(id, data)` | Partial update terminal data |
| `setSessionId(id, sessionId)` | Update session ID |
| `setFontSize(id, fontSize)` | Update font size |
| `setAwaitingInput(id, type)` | Set awaiting input indicator |
| `clearAwaitingInput(id)` | Clear awaiting input |
| `splitPane(direction)` | Split into two panes |
| `closeSplitPane(index)` | Collapse back to single pane |
| `setSplitRatio(ratio)` | Adjust split ratio |
| `setActivePaneIndex(index)` | Switch active pane |

### Queries

| Method | Description |
|--------|-------------|
| `get(id)` | Get terminal by ID |
| `getActive()` | Get active terminal |
| `getIds()` | Get all terminal IDs |
| `getCount()` | Get terminal count |
| `hasAwaitingInput()` | Any terminal awaiting input? |
| `getAwaitingInputIds()` | Get IDs of terminals awaiting input |

---

## repositoriesStore

**File:** `src/stores/repositories.ts`

Manages saved repositories, branches, terminal associations, and PR status cache.

### State Shape

| Field | Type | Description |
|-------|------|-------------|
| `repos` | `Record<string, RepositoryState>` | Repositories by path |
| `activePath` | `string \| null` | Active repository path |

### Key Types

```typescript
interface RepositoryState {
  path: string;
  name: string;
  initials: string;
  expanded: boolean;      // Show branch list
  collapsed: boolean;     // Icon-only mode
  activeBranch: string | null;
  branches: Record<string, BranchState>;
}

interface BranchState {
  name: string;
  terminals: string[];     // Terminal IDs
  worktreePath: string | null;
  isMain: boolean;
  additions: number;
  deletions: number;
  runCommand: string | null;
}
```

### Actions

| Method | Description |
|--------|-------------|
| `hydrate()` | Load from Rust backend |
| `add(repo)` | Add repository |
| `remove(path)` | Remove repository |
| `setActive(path)` | Set active repository |
| `toggleExpanded(path)` | Toggle branch list visibility |
| `toggleCollapsed(path)` | Toggle icon-only mode |
| `setBranch(repoPath, branchName, data)` | Add/update branch |
| `setActiveBranch(repoPath, branchName)` | Set active branch |
| `addTerminalToBranch(repoPath, branchName, terminalId)` | Link terminal |
| `removeTerminalFromBranch(repoPath, branchName, terminalId)` | Unlink terminal |
| `setRunCommand(repoPath, branchName, command)` | Save run command |
| `updateBranchStats(repoPath, branchName, additions, deletions)` | Update diff stats |
| `removeBranch(repoPath, branchName)` | Remove branch |
| `renameBranch(repoPath, oldName, newName)` | Rename branch |
| `reorderTerminals(repoPath, branchName, fromIndex, toIndex)` | Reorder tabs |

### Queries

| Method | Description |
|--------|-------------|
| `get(path)` | Get repository by path |
| `getActive()` | Get active repository |
| `getPaths()` | Get all repository paths |
| `getActiveTerminals()` | Get terminal IDs for active branch |
| `isEmpty()` | Check if no repositories |

---

## settingsStore

**File:** `src/stores/settings.ts`

Application settings: font, shell, IDE, theme, confirmations.

### State Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `ide` | `IdeType` | `"cursor"` | IDE for "Open in..." |
| `font` | `FontType` | `"JetBrains Mono"` | Terminal font |
| `agent` | `string` | `"claude"` | Primary agent |
| `defaultFontSize` | `number` | `12` | Default font size |
| `shell` | `string` | `""` | Shell override |
| `theme` | `string` | `"dark"` | Terminal theme |
| `confirmBeforeQuit` | `boolean` | `true` | Quit confirmation |
| `confirmBeforeClosingTab` | `boolean` | `true` | Tab close confirmation |
| `maxTabNameLength` | `number` | `20` | Max tab name length |

### Constants

- `IDE_NAMES` — Display names for IDEs
- `IDE_ICONS` — Emoji icons
- `IDE_ICON_PATHS` — SVG icon paths
- `IDE_CATEGORIES` — IDE grouping (editors, terminals, git, utilities)
- `FONT_FAMILIES` — CSS font-family strings

---

## githubStore

**File:** `src/stores/github.ts`

GitHub PR and CI data with background polling.

### Actions

| Method | Description |
|--------|-------------|
| `updateRepoData(repoPath, prStatuses)` | Update PR data for all branches (detects state transitions for notifications) |
| `startPolling()` | Start background polling (30s base, 2m when hidden, 5m backoff on rate limit) |
| `stopPolling()` | Stop polling |
| `pollRepo(path)` | Immediately poll a single repo (debounced 2s to coalesce rapid git events) |
| `setRemoteStatus(repoPath, remote)` | Set remote tracking status directly (used by simulator) |

### Queries

| Method | Description |
|--------|-------------|
| `getCheckSummary(repoPath, branch)` | Get CI check summary |
| `getPrStatus(repoPath, branch)` | Get PR status |
| `getCheckDetails(repoPath, branch)` | Get CI check details |
| `getBranchPrData(repoPath, branch)` | Get full BranchPrStatus |
| `getRemoteStatus(repoPath)` | Get remote tracking status (ahead/behind) |

---

## promptLibraryStore

**File:** `src/stores/promptLibrary.ts`

Prompt template management with variable substitution.

### State Fields

| Field | Type | Description |
|-------|------|-------------|
| `prompts` | `SavedPrompt[]` | All prompts |
| `drawerOpen` | `boolean` | Drawer visibility |
| `searchQuery` | `string` | Search filter |
| `selectedCategory` | `PromptCategory` | Category filter |
| `recentIds` | `string[]` | Recently used prompt IDs |

### Actions

| Method | Description |
|--------|-------------|
| `hydrate()` | Load from Rust |
| `openDrawer()` / `closeDrawer()` / `toggleDrawer()` | Drawer visibility |
| `createPrompt(data)` | Create new prompt |
| `updatePrompt(id, data)` | Update prompt |
| `deletePrompt(id)` | Delete prompt |
| `toggleFavorite(id)` | Toggle pinned status |
| `markAsUsed(id)` | Add to recent list |
| `processContent(prompt, variables)` | Substitute variables (via Rust) |
| `extractVariables(content)` | Parse `{{variable}}` placeholders (via Rust) |

---

## statusBarTicker

**File:** `src/stores/statusBarTicker.ts`

Rotating message ticker for the status bar. Plugins and native features post messages; the highest-priority message is displayed, with rotation among equal-priority messages.

### TickerMessage Type

```typescript
interface TickerMessage {
  id: string;           // Unique message ID (scoped to plugin)
  pluginId: string;     // Plugin that posted the message
  text: string;         // Display text (~40 chars max)
  icon?: string;        // Optional inline SVG icon
  priority: number;     // Higher = more visible. >=80 gets warning styling
  ttlMs: number;        // Time-to-live in ms (0 = persistent until removed)
  createdAt: number;    // Timestamp when added
  onClick?: () => void; // Optional click handler
}
```

### Actions

| Method | Description |
|--------|-------------|
| `addMessage(msg)` | Add or replace a message (by id + pluginId). Resets TTL on replace. |
| `removeMessage(id, pluginId)` | Remove a specific message |
| `removeAllForPlugin(pluginId)` | Remove all messages from a plugin |
| `clear()` | Clear all messages and stop timers |

### Queries

| Method | Description |
|--------|-------------|
| `getCurrentMessage()` | Get the highest-priority non-expired message (rotates among equal-priority) |
| `getAll()` | Get all active (non-expired) messages |

### Internals

- **Rotation:** Messages at the same priority level rotate every 5 seconds.
- **Scavenging:** Expired messages (past TTL) are cleaned up every 1 second.
- **StatusBar integration:** The `claude-usage` ticker message (pluginId `"claude-usage"`) is absorbed into the agent badge when the active terminal runs Claude, and suppressed from the separate ticker area.

---

## notesStore

**File:** `src/stores/notes.ts`

Persistent notes/ideas with per-repo tagging and usage tracking.

### Note Type

```typescript
interface Note {
  id: string;
  text: string;
  createdAt: number;
  repoPath: string | null;
  repoDisplayName: string | null;
  usedAt: number | null;      // Timestamp when sent to terminal
}
```

### Actions

| Method | Description |
|--------|-------------|
| `hydrate()` | Load notes from Rust backend |
| `addNote(text, repoPath?, repoDisplayName?)` | Add a new note, optionally tagged with a repo |
| `removeNote(id)` | Remove a note by ID |
| `reassignNote(id, repoPath, repoDisplayName)` | Reassign a note to a different project |
| `markUsed(id)` | Mark a note as used (sets `usedAt` timestamp) |

### Queries

| Method | Description |
|--------|-------------|
| `getFilteredNotes(activeRepo)` | Get notes for repo (global + repo-specific). `null` = all notes. |
| `filteredCount(activeRepo)` | Count of notes visible for the given repo filter |
| `count()` | Total note count |

---

## Other Stores

### repoSettingsStore (`repoSettings.ts`)
Per-repository settings (base branch, scripts, worktree options).

### uiStore (`ui.ts`)
Panel visibility (sidebar, diff, markdown, notes, file browser), sidebar width, dropdown state, loading state.

### notificationsStore (`notifications.ts`)
Notification sound preferences and playback.

### dictationStore (`dictation.ts`)
Whisper dictation config, model management, recording state.

### errorHandlingStore (`errorHandling.ts`)
Error retry configuration and active retry tracking.

### rateLimitStore (`ratelimit.ts`)
Active rate limit tracking per session.

### tasksStore (`tasks.ts`)
Agent task queue management.

### promptStore (`prompt.ts`)
Active prompt overlay state and agent stats buffer.

### diffTabsStore (`diffTabs.ts`) / mdTabsStore (`mdTabs.ts`)
Open diff and markdown tab management (identical API patterns).

### updaterStore (`updater.ts`)
App update check, download, and install. Supports stable (Tauri built-in), beta, and nightly channels.

### keybindingsStore (`keybindings.ts`)
Rebindable keyboard shortcuts (persisted, auto-populated from action registry).

### commandPaletteStore (`commandPalette.ts`)
Command palette visibility and search state.

### activityDashboardStore (`activityDashboard.ts`)
Activity center (bell dropdown) visibility.

### prNotificationsStore (`prNotifications.ts`)
PR state transition notifications (merged, closed, blocked, CI failed, etc.).

### userActivityStore (`userActivity.ts`)
Tracks last user activity timestamp. Used for merged PR grace period calculations.

### editorTabsStore (`editorTabs.ts`)
Open code editor tabs (CodeEditorTab).
