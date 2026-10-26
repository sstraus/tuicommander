# Stores Reference

Slice F exposes `start_graph {target:{type:story|plan,id},expected_revision?,definition_id,definition_revision,request_id,limits?}` through the existing owning-daemon service. Story starts require the current native revision and pin the selected publication; the request ID is bound to its payload. Omitted limits use Rust defaults. Plan dispatch remains unavailable in this build and its start control says so. The `workflow action=run` MCP entry uses an inline schema generated from `RunAction` and the public `RunCommand` variants; it returns the same scoped snapshots and cursor-ordered events as IPC/HTTP. Run history shows pinned activations, decisions/evidence, repair counters, pause targets and complete event payloads across pages. Graph recovery uses `resume_graph {execution_id,activation_id,resolution}` after answering pending input; pause and cancel use the existing sequence-fenced commands. Legacy runs offer inspection and cancellation in the UI. No new persistence or client scheduler is added.

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
  nameIsCustom: boolean;            // When true, OSC/status-line title changes are ignored
  fontSize: number;
  cwd: string | null;               // Current working directory (from OSC 7)
  repoPath: string | null;          // Owning repo — the record. null = parked guess (see terminalOwnership)
  awaitingInput: AwaitingInputType; // "question" | "error" | null
  awaitingInputConfident: boolean;  // High-confidence detection — don't clear on idle→busy
  shellState: ShellState;           // "busy" | "idle" | null
  activity: boolean;
  unseen: boolean;                  // Terminal completed work while user wasn't viewing it
  progress: number | null;          // OSC 9;4 progress (0-100), null when inactive
  agentType: AgentType | null;      // Detected foreground agent process (e.g. "claude")
  pendingResumeCommand: string | null; // Set at restore time, consumed on first shell idle
  pendingInitCommand: string | null;   // Setup/run script to auto-execute on first shell idle
  usageLimit: { percentage: number; limitType: string } | null;
  lastDataAt: number | null;        // Timestamp of last PTY output
  lastActivityAt: number | null;    // Backend timestamp of last semantic session activity
  lastPrompt: string | null;        // Last relevant user prompt (>= 10 words), set by Rust
  agentIntent: string | null;       // LLM-declared intent via intent: token
  currentTask: string | null;       // Current agent task from status-line parsing
  activeSubTasks: number;           // Count of running sub-agents from ›› status line
  isRemote: boolean;                // Created via HTTP/MCP (not locally by the UI)
  exactAgentSessionId?: string | null; // Live, validated Codex hook binding; never persisted
  agentSessionId: string | null;    // Agent session ID for session-specific resume
  tuicSession: string | null;       // Stable tab UUID — injected as TUIC_SESSION env var
  suggestedActions: string[] | null; // Follow-up suggestions from suggest: token
  suggestDismissed: boolean;        // true after user dismissed — prevents re-show
}

interface TabLayout {
  direction: SplitDirection;  // "none" | "vertical" | "horizontal"
  panes: string[];            // Terminal IDs (up to MAX_SPLIT_PANES = 6)
  ratios: number[];           // N fractions summing to 1.0 (length === panes.length)
  activePaneIndex: number;    // 0..N-1
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
  displayName: string;
  initials: string;
  isGitRepo?: boolean;    // false for plain directories
  expanded: boolean;      // Show branch list
  collapsed: boolean;     // Icon-only mode
  parked: boolean;        // Hidden from sidebar (recallable via popover)
  workspaces: Record<WorkspaceId, WorkspaceState>;
  activeBranch: string | null;     // holds a WorkspaceId; renamed in a later pass
}

/**
 * Opaque. Existing records were migrated with `workspaceId = branchName`, so the
 * value looks parseable, but callers still read `WorkspaceState.branchName`
 * instead of taking display data out of the key.
 */
type WorkspaceId = string;

// Load-time repair (`workspaceIdentity.ts` `repairIdentity`) TYPE-CHECKS the
// path and name fields rather than only replacing null/undefined. The document
// is untrusted input, and the realistic corruption is a shape skew, not a
// hostile write: `get_worktree_paths` became `{id: {branch, path}}` in one
// commit, and a WebView still holding the pre-change module wrote the whole
// record into `worktreePath`. That value round-trips through every later load
// and reaches `joinPath`, which calls `.replace` on it and takes the app down
// with an error naming neither the field nor the repo; a non-string
// `branchName` does the same via `compareBranches` -> `localeCompare` inside
// the sidebar's sort memo. A corrupt path degrades to `null` — "no separate
// checkout", which every reader already handles — and the next refresh writes
// the real value back. Salvaging `.path` out of the object is deliberately not
// done: it encodes one historical shape and would hide the next one.

interface WorkspaceState {
  workspaceId: WorkspaceId;        // equal to the key that holds this record
  branchName: string;              // display data, not a lookup key
  kind: "main" | "worktree";
  parentRepoPath: string | null;
  isMain: boolean;
  isShell?: boolean;               // true for non-git directory shell entries
  worktreePath: string | null;
  terminals: string[];             // Terminal IDs
  hadTerminals: boolean;           // Suppresses auto-spawn after close-all
  lastActiveTerminal: string | null;
  additions: number;
  deletions: number;
  isMerged: boolean;               // Fully merged into main branch
  lifecycleStatus?: {
    dirty: boolean | null;
    commitStatus: "unmerged" | "merged" | "unknown";
    removalSafety: "safe" | "requires_force" | "unknown";
    error?: string;
  };                               // derived, workspace-id keyed, never deletion authority
  lastCommitTs: number | null;     // Unix timestamp of last commit
  runCommand?: string;
  savedTerminals?: SavedTerminal[];
  ciAutoHeal?: { enabled: boolean; attempts: number; lastRunId?: number; healing?: boolean };
  layout?: TabLayout;              // Split layout persisted per-branch
}
```

`lifecycleStatus` is populated by progressive refresh from the backend's
`workspace_statuses` map. It is preserved as live derived state when another
window writes the repository document and excluded from intent comparisons.
Removal never trusts the cached value: it requests a fresh backend preflight.

### Actions

| Method | Description |
|--------|-------------|
| `hydrate()` | Load from Rust backend |
| `add(repo)` | Add repository |
| `remove(path)` | Remove repository |
| `setActive(path)` | Set active repository |
| `toggleExpanded(path)` | Toggle branch list visibility |
| `toggleCollapsed(path)` | Toggle icon-only mode |
Every method below that takes a `workspaceId` takes the **map key**, never a branch
to look up. The two are the same string for everything a linked worktree ever
created — the identity migration minted `workspaceId = branchName` so nothing
persisted moved — which is exactly why the parameter is named for the key: a caller
holding a branch off git output and a caller holding an id off a workspace record
are indistinguishable at the call site otherwise.

| Method | Description |
|--------|-------------|
| `setWorkspace(repoPath, workspaceId, data)` | Add/update one workspace. `data.branchName` defaults to the id |
| `setActiveWorkspace(repoPath, workspaceId)` | Set the active workspace. **Rejects an id that names no row** and keeps the previous pointer — a dangling one fails later and elsewhere: the sidebar renders no tab row for it, and the next terminal added to that workspace spawns in `$HOME`. `migrateActiveWorkspaceId` drops the same dangling id at load time; this is the live half of that rule |
| `addTerminalToWorkspace(repoPath, workspaceId, terminalId)` | Link terminal |
| `removeTerminalFromWorkspace(repoPath, workspaceId, terminalId)` | Unlink terminal |
| `setRunCommand(repoPath, workspaceId, command)` | Save run command |
| `updateWorkspaceStats(repoPath, workspaceId, additions, deletions)` | Update diff stats |
| `removeWorkspace(repoPath, workspaceId)` | Remove one workspace |
| `renameBranch(repoPath, oldName, newName)` | A branch was renamed — moves the record AND its `workspaceId`, since a branch-derived id is the key |
| `mergeWorkspaceState(repoPath, sourceId, targetId)` | Move terminals/saved tabs between workspaces |
| `reorderTerminals(repoPath, workspaceId, fromIndex, toIndex)` | Reorder tabs |

### Queries

| Method | Description |
|--------|-------------|
| `get(path)` | Get repository by path |
| `getWorkspace(repoPath, workspaceId)` | One workspace record — the only supported way to reach one |
| `branchNameFor(repoPath, workspaceId)` | The branch that workspace has checked out. `id → branch` is a lookup; `branch → id` is a guess, so there is no inverse |
| `findOwnerForTerminal(termId)` | `{ repoPath, workspaceId }` for a terminal |
| `getActive()` | Get active repository |
| `getPaths()` | Get all repository paths |
| `getActiveTerminals()` | Get terminal IDs for the active workspace |
| `isEmpty()` | Check if no repositories |

`resolveRepoOwner(path)` (same module) answers `{ repoPath, workspaceId | null }`:
a path resolves to a workspace **id** because two workspaces may share a branch but
cannot share a directory. `null` means the match was at the repo root, whose
checkout changes under the user's feet — resolve it late through
`activeWorkspaceId`, or via `placementWorkspaceFor(owner)`.

---

## settingsStore

**File:** `src/stores/settings.ts`

Application settings: font, shell, IDE, theme, confirmations.

Each hydrated window tracks its own normalized settings baseline. Debounced saves
apply only preferences changed since that window's last successful save to a fresh
config load, then submit the existing `base`/`config` pair through `updateAppConfig`.
Unchanged preferences stay as loaded, including fields another settings window
edited. Failed saves retain pending edits; edits made during a save remain pending
for the next save. Hydration itself does not persist normalized defaults.

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

## ideasStore

**File:** `src/stores/ideas.ts`

Persistent ideas with per-repo tagging and usage tracking.

The store, the panel and this type say "idea". The backend commands
(`load_notes` / `save_notes` / `save_note_image` / `delete_note_assets`), the
`notes.json` payload field and the `note-` id prefix still say "note": they are
on-disk and on-wire identifiers, renamed by no migration. The translation
happens only at the IO boundary in `hydrate()` and `saveIdeas()`.

### Idea Type

```typescript
interface Idea {
  id: string;
  text: string;
  createdAt: number;
  repoPath: string | null;
  repoDisplayName: string | null;
  usedAt: number | null;      // Timestamp when sent to or queued for a terminal
  images: string[];
}
```

### Actions

| Method | Description |
|--------|-------------|
| `hydrate()` | Load ideas from Rust backend |
| `addIdea(text, repoPath?, repoDisplayName?, images?, ideaId?)` | Add a new idea, optionally tagged with a repo |
| `removeIdea(id)` | Remove an idea by ID |
| `updateIdea(id, text, images)` | Update an idea in-place |
| `reassignIdea(id, repoPath, repoDisplayName)` | Reassign an idea to a different project |
| `markUsed(id)` | Mark an idea as used (sets `usedAt` timestamp) |
| `clearCompleted()` | Remove every idea that has been used |

### Queries

| Method | Description |
|--------|-------------|
| `getFilteredIdeas(activeRepo)` | Get ideas for repo (global + repo-specific). `null` = all ideas. |
| `filteredCount(activeRepo)` | Count of ideas visible for the given repo filter |
| `pendingCount(activeRepo)` | Count of ideas not yet used |
| `count()` | Total idea count |

---

## Other Stores

Config-backed stores retain the snapshot returned by their load command and
send `{ base, config }` on save (`paneLayoutStore` uses `layout`, and
`activityStore` uses `items` for the edited value). Their shared
`configDeltaWriter` orders overlapping saves within one WebView and advances
the base after each successful write. The backend merges only changed keys
against the latest locked file; arrays such as keybindings and notes replace as
a unit. A failed load disables saving rather than treating defaults as the
user's prior document.

### repoSettingsStore (`repoSettings.ts`)
Per-repository settings (base branch, scripts, worktree options).

### uiStore (`ui.ts`)
Panel visibility (sidebar, diff, markdown, notes, file browser), sidebar width, dropdown state, loading state.
Editor wrap defaults for text and code live here too; they use localStorage, like recent command-palette actions, so toggling them needs no backend restart.

### notificationsStore (`notifications.ts`)
Notification sound preferences and playback. Remote orchestration muting uses
the terminal's backend-preserved `isRemote` origin; completion lifecycle code
sets a per-busy-cycle latch before playback so idle and exit cannot both chime.
The playback manager applies one 500 ms gate across every sound type, because
all types share the same audio output and separate per-type gates allowed tones
from a notification burst to overlap.
ACP interaction notifications are keyed by connection and request ID, so a
re-render cannot send a duplicate. Settlement closes the matching notification.

`acpTranscript` also projects ACP session titles and usage by session ID. The
AI Chat header, conversation picker and usage footer read that projection.
Provider-retry patches retain the provider text and an arrival-based presentation
deadline. `createRetryCountdown` owns one timer for the selected conversation;
recovery, final failure, settlement or panel disposal stops it. Missing or invalid
delays keep the provider text without starting a timer.
`aiChatTabs` is the single per-root store for open chat session IDs and the
selected tab. It mirrors its state to localStorage so a detached WebView can
restore the same tabs; `useAcpChat` replays their ACP histories after connecting.
The existing `ai_chat_sessions` app config value remains the last selected
conversation for older documents and clients.
`acpStore` replaces only the matching attachment's complete option set on
`config_option_update`, including an empty set. It applies the published options
directly, without a snapshot request that could arrive after a confirmed settings
change. Explicit requests still refresh the authoritative backend snapshot.
`acpStore` takes `queuedPrompts` from the connection snapshot and replaces it
on each `promptQueueChanged` event, so desktop and browser views share the
same FIFO. `acpTranscript` adds a user message on `promptSent`, when the host
has sent it to ego; a queued prompt cancelled before dispatch never enters
the transcript.
`turnFailed` carries an ACP error diagnostic into the transcript and restores
the attachment state. A completed turn with no agent message receives an
explicit no-reply entry.

### dictationStore (`dictation.ts`)
Whisper dictation config, model management, recording state — plus the speech
assets, the hands-free conversation and the spoken-reply status the Dictation
settings panel renders.
The backend crate split leaves its IPC and HTTP response shapes unchanged.
`startRecording(source)` sends the same origin over IPC or HTTP. A native Fn
release stops capture before the frontend's `stopRecording()` awaits the final
transcription. DOM hotkeys release on blur if key-up was lost. Native macOS Fn
keeps recording across focus loss while physically held; Rust detects its
release outside the main window and sends the same `fn-key-up` event.
`stopRecording()` stores the response's `skip_reason` as `lastSkipReason`, and
`useDictation` shows that reason in the status. A final transcription gate's
specific reason reaches both; an empty successful pass shows `no speech detected`.

handsFreeListening reads and saves hands_free_listening the same way (`setHandsFreeListening`); `armHandsFree` passes it to `connectBrowserVoice` so a replies-only browser tab opens no microphone. spokenReplies reads and saves hands_free_spoken_replies through the existing
load/base/delta configuration path. Browser clients refresh configuration when
voice controls load; mobile Settings and conversation controls use the same
store action. Rust owns enforcement and cool-down decisions.

- `speechAssets` / `speechDownloads` — the installable languages and ONNX
  runtime, and a percent per asset **keyed by asset id**, because the runtime
  library and a language are separate downloads a user can start together.
  `downloadSpeechAsset` clears its key with `setState("speechDownloads", id,
  undefined)`: a store update at a path *merges*, so returning a smaller object
  leaves the key exactly where it was.
- `speechVoices` — the voices of one language as `get_speech_voices` returns
  them (`{ id, source }`, `source` `"default"`, `"downloaded"` or `"user"`).
  `refreshSpeechVoices(language)` reloads it; the voice picker offers only these
  ids, because Rust refuses a catalogue voice that is not downloaded.
- `importSpeechVoice(language, file)` sends the file as base64 (`dataBase64`)
  under its file name without the extension, and `deleteSpeechVoice(language,
  name)` removes a user voice. Both re-read the catalogue. The import returns the
  reason Rust refused the file, or null, so the panel can show it.
- `previewSpeechVoice(language, voice)` calls `preview_speech_voice` with a short
  sample. It does not save the voice. It returns the refusal (for example, a
  hands-free reply is being spoken), or null.
- `speechVolumeDb` / `speechLevelling` — the loudness settings, read by
  `refreshConfig` (defaults `DEFAULT_SPEECH_VOLUME_DB` -18 and
  `DEFAULT_SPEECH_LEVELLING` 0.67 until then). `setSpeechVolumeDb` and
  `setSpeechLevelling` each save one field.
- `handsFree` / `speech` — polled status, never pushed. `refreshHandsFree` is
  deliberately **one** command, because the dictation hotkey asks on every press.
  Starting arm or disarm invalidates older status reads, so their late responses
  cannot overwrite the new conversation state or close its browser audio.
  `handsFree.pendingText` is the unsent hands-free turn; push-to-talk's
  `partialText` is a separate recording. With an activation phrase,
  `handsFree.holdBackMs` is at least 5000 even if the saved setting is shorter.
- `turnEarcon(previous, next, owner)` — `applyHandsFree` plays an earcon
  (`utils/earcon.ts`, Web Audio, 80 ms) when `deliveredTurns` or `droppedTurns`
  moved since the last stored status. Only the client that owns the audio plays
  it, and the first status a client reads is only a baseline. `armHandsFree`
  primes the audio context inside the user's gesture, and reads
  `hands_free_earcons` into `handsFreeEarcons` at each arm, including after another client saves. `setHandsFreeEarcons(value)` saves it; off, nothing plays.
- `armHandsFree(sessionId)` sends `DESKTOP_AUDIO_OWNER` on the desktop and
  `browserAudioOwner` — a per-tab random id — in a browser. In browser mode it
  opens the audio socket (`utils/browserVoice.ts`) **before** arming, because
  Rust refuses an owner with no connected client rather than falling back to the
  server's microphone. A refused arm closes that socket again in the `catch`, so
  a failure never leaves the device light on; `disarmHandsFree` closes it in a
  `finally`. The refusal is stored in `handsFreeError` rather than thrown.
  Successful browser arming also owns the page's Media Session through
  `browserVoiceMediaSession`: title **TUICommander hands-free**, Play/Pause use
  `resumeSpeech`/`pauseSpeech` (capture remains active), and Stop uses
  `disarmHandsFree` (capture and replies end). Speech responses and pushed reply
  state keep `playbackState` current; listening is active, held replies are
  paused. Disarm, including a polled backend disarm, clears metadata, handlers
  and playback state and closes browser audio. Browsers without the API still
  support hands-free; unsupported actions are logged and omitted.
- `handsFreeStartNotice` — the saved start notice, `""` meaning the built-in
  text. `setHandsFreeStartNotice(value)` saves it trimmed,
  `resetHandsFreeStartNotice()` saves `""`, and
  `getDefaultHandsFreeStartNotice()` asks Rust for the built-in text
  (`get_hands_free_default_notice`) — the frontend keeps no copy of it. Its save
  falls back to the stored value, like the other one-panel fields.
- `saveConfig` abandons the save when `get_dictation_config` cannot be read or
  answers something that is not a config. It sends the loaded document as
  `base` and overrides only the requested fields in `config`, preserving
  one-panel settings that other surfaces do not model.

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

MCP native Markdown file tabs survive document reloads through a per-window `sessionStorage` snapshot. `initApp` restores their MCP identity, file target, repository/branch scope and pin state after terminal adoption. The store then saves relevant changes synchronously, including closes, pinning and selection, so native recovery does not depend on `beforeunload`. The unload handler also saves a final snapshot. Both automatic and explicit saves wait until initial restoration finishes, preserving the unread snapshot if another reload interrupts startup. Restoration consumes the prior snapshot only once per document and still runs if restoring the active repository branch fails. The selected document is selected again; background tabs remain inactive. A new MCP event received during startup takes precedence over the snapshot, including one that moves the identity to an editor tab. Snapshots are consumed before parsing so corrupt JSON is not replayed on the next initialization. Restoration indexes existing Markdown/editor identities once, accepts the first valid entry per identity, and appends the new tabs and their display order in one batch; duplicate-heavy snapshots do not rescan the growing tab collection. User-opened files and generated or iframe panels are not included. This is reload recovery, not backend persistence across app restarts.

`mdTabsStore.openUiTab` treats URL and HTML content as alternatives. Updating an existing URL tab with HTML clears its URL; updating an HTML tab with a URL clears its HTML. Visibility is decided by the tab renderers, which unload hidden plugin and URL iframes.

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

### worktreeManagerStore (`worktreeManager.ts`)
Worktree Manager overlay state and selection.

**State Shape:**

| Field | Type | Description |
|-------|------|-------------|
| `isOpen` | `boolean` | Overlay visibility |
| `selectedIds` | `Set<string>` | Multi-select worktree IDs |
| `repoFilter` | `string \| null` | Filter by repo path |
| `textFilter` | `string` | Free-text search filter |

**Actions:** `open()`, `close()` (resets all state), `toggle()`, `toggleSelect(id)`, `selectAll(ids)`, `clearSelection()`, `setRepoFilter(path)`, `setTextFilter(text)`.

### agentConfigsStore (`agentConfigs.ts`)
Per-agent configuration (spawn args, environment overrides).

### editorTabsStore (`editorTabs.ts`)
Open code editor tabs (CodeEditorTab).

### activityStore (`activityStore.ts`)
Session activity history and timeline data.

### branchSwitcher (`branchSwitcher.ts`)
Branch switch state and loading indicators.

### terminalOwnership (`terminalOwnership.ts`)
Which repo owns a terminal. `TerminalData.repoPath` is the record; the branch
`terminals[]` arrays are a display index derived from it, so a wrong placement is
repairable instead of permanent. `null` means no registered repo claimed the cwd —
the tab is parked in whatever repo was active so it stays visible, and the null
marks the placement as a guess.

| Function | Use |
|---|---|
| `reconcileTerminalOwnership(terminalId?)` | Ask "who owns this?" again. For answers that genuinely changed: repos loaded, one added or removed, a worktree appeared, a branch renamed. Omit the id to sweep every terminal. |
| `reclaimParkedTerminal(terminalId)` | The only thing an OSC 7 cwd change may trigger. No-op unless `repoPath === null`. |

**A `cd` does not re-home an owned tab.** The tab belongs to the repo it was
opened in; the directory the shell sits in does not revoke that. Calling the full
reconcile from the cwd handler moved tabs out from under the user, because three
states answer "where am I" and only one moved: `activeRepoPath` stayed put, so the
sidebar and the tab bar (which filters on it) kept showing the old repo while the
tab left the strip — and `TerminalArea` renders on `terminalsStore.activeId` alone,
so the pane went on drawing a terminal belonging to a repo nobody had selected.
Agents `cd` across repos constantly, which is why it read as the app switching repo
on its own. Only a parked tab is settled by a `cd`, because for it the question was
still open.

**Parking says which repo is missing.** An MCP-spawned agent inherits its parent's
cwd, so sessions land in worktrees of repos the user never registered; the tab was
then filed under whichever repo had focus, and the only trace was a warning naming
the cwd. `unregisteredRepoRootFor(cwd)` (`utils/repoOwnership.ts`) turns that cwd
into the directory to register — `…/gate-os__wt/poc-0001` → `…/gate-os` via the
`__wt` convention, otherwise the path itself — and `assignSessionToRepoBranch`
puts it in the warning and in one deduped toast. It is a guess for the user to act
on, never a placement: `resolveRepoOwnerIn` remains the single answer to "who owns
this tab", and registering the repo lets `reconcileTerminalOwnership` move the tab
home by itself. Auto-registering instead was rejected — `addRepository` calls
`setActive()`, which would yank the user's focused repo from a background event,
the exact failure the paragraph above describes.

### contextMenuActionsStore (`contextMenuActionsStore.ts`)
Dynamic context menu action registration.

### progressStore (`progress.ts`)

Transport-neutral presentation state for Progress. It selects the active PTY
when available and retains a repository aggregate view. It holds the selected
scope's entry list and its divider timestamp frozen while that scope is shown.
Switching scopes loads the new mark; closing records every visited scope.

It queries on open, for the one project the dialog shows — not once per
registered repository, which is what made the old panel fire a request per repo
and answer with a red block for each one that no longer existed. Boot reads
nothing at all. Each read requests at most 100 entries. `nextCursor` is an
exclusive entry ID returned by the backend, and `loadOlder` reads one page on
demand with the current filters. Refreshes replace the list with the newest
page. A request epoch discards stale reads after a refresh or selection change;
the loading state prevents concurrent older-page appends.

The unread count is the number of `progress-recorded` pushes that arrived while
the dialog was closed, reset on open. It is deliberately not a query: the
persistent record of where the reader stopped is the divider.

`view` selects List or Flow. The Flow view's data lives in `state.flows`,
per project, and is read with `progress_flow` for the selected scope whenever
the Flow view is showing and something changes (open, PTY switch, live entry).
The list is read in both views because it carries the divider and deletion.
`fetchFlowDetail` fetches one subagent arrow's full text on demand.

A Progress toast opts out of the generic MESSAGES mirror because the bell has its
own aggregate row, and it is silent — a blocked entry is not automatically a
demand for attention. Its source PTY id lets the repo action activate the
reporting terminal and its workspace; if that terminal has closed, the action
opens the repository and logs why it could not focus the terminal. `intent`,
`delegated` and `message` entries toast nothing: they are what an agent set out
to do or said to another agent, not a result. Failures stay on the affected
project as one line instead of being rendered as an empty feed.

### errorLog (`errorLog.ts`)
Error ring buffer and error panel state.

### pluginStore (`pluginStore.ts`)
Loaded plugin instances and lifecycle state.

### registryStore (`registryStore.ts`)
Remote plugin registry cache and install state.

### repoDefaults (`repoDefaults.ts`)
Default settings applied to newly added repositories.

### tabManager (`tabManager.ts`)
Tab ordering, branch-key mapping, and tab persistence logic.

**One ordering implementation.** `reorderIds` (splice source before/after target) and
`orderedThenRemainder` (known order first, never-dragged ids appended) are the only
ordering code in the app. Both display orders call them: each store's own `_order`
via `reorderByIds` / `getVisibleIds`, and `tabOrderingStore` — the single cross-kind
list that spans terminals, diffs, markdown and editor tabs, which the tab bar reads in
the `terminals-first` and `free` ordering modes. A per-store `_order` cannot express
that list, because a drag between two kinds moves an id across store boundaries.

`tabOrderingStore` is only correct while the stores keep it populated: `_addTab`,
`_addTabBackground`, `remove`, `clearAll` and `_clearWhere` mirror into it, and
`terminals.ts` (outside the factory) does the same from its `add` and `remove`. Skip
that wiring and the list stays empty, `reorder` finds neither id and returns, and every
drag across tab kinds silently does nothing. Pinned by
`src/__tests__/stores/tabOrderWiring.test.ts`.

Terminal order itself is NOT here: it lives in `repositoriesStore`, per repo and per
branch, and is persisted. `tabOrderingStore` holds terminal ids so a cross-kind drag
can place them, but the branch list stays the source of truth for terminals alone.

**Exclusive pane activation.** TerminalArea renders terminals, diffs, markdown and
editors as four independent `For` lists, each marking its pane `active` from its
OWN store's `activeId` — so "only one pane shows" is a cross-store invariant.
`createTabManager` registers a deactivator per store (`registerPaneDeactivator`);
`terminals.ts` registers its own since it doesn't use the factory. Activating a
tab — `setActive(id)` with a non-null id, or `_addTab` — calls
`activatePaneExclusively(storeName)`, which clears every other store's `activeId`.
`setActive(null)` and the `_addTabBackground` variants are local: a background
open must not yank the user out of the pane they're in.

Call sites therefore must NOT hand-roll `setActive(null)` on the other stores.
This replaced the `useTabActivationSync` hook, which enforced the same rule from
deferred `on(activeId)` effects: an effect keyed on a *change* cannot enforce an
invariant that has to hold on every activation *request*, so re-activating an
already-active tab (Edit on a file whose editor tab was already the active one)
wrote the same value, fired nothing, and left the other pane rendered underneath.
Pinned by `src/__tests__/stores/paneExclusivity.test.ts`.

### appLogger (`appLogger.ts`)
Centralized logging — replaces direct `console.*` calls. Writes all levels and their data to the local ring buffer and surfaces them in ErrorLogPanel. Info, warn, and error messages also reach the browser console without data objects. Debug messages reach the console only while `window.__TUIC__.setPerfDebug(true)` is active; they also omit data objects. Info, warn, and error entries continue to reach the Rust log ring and `/logs`, including serialized data, for diagnostics such as `tuic-health.sh`. The only mirrored debug records are source `dictation` messages starting with `Browser dictation lifecycle:`: at most six content-free records per take, in the diagnostic audience. Other debug entries remain local.

### debugRegistry (`debugRegistry.ts`)
Dynamic snapshot registry for MCP `invoke_js` introspection. Stores self-register a snapshot function at init time, exposed on `window.__TUIC__` as `stores()` (list names) and `store(name)` (get snapshot).

**Registered stores:** github, globalWorkspace, keybindings, notes, paneLayout, repositories, settings, tasks, ui.

**Adding a new store** — append 2 lines at the end of the store file:
```ts
import { registerDebugSnapshot } from "./debugRegistry";
registerDebugSnapshot("storeName", () => ({ /* fields to expose */ }));
```
Each store decides what to expose — no need to modify `debugGlobals.ts`.

### remoteAcp (`remoteAcp.ts`)

Keeps pending ACP permissions and elicitations scoped to their owning daemon.
Only interaction, ready, and settled notices advance snapshot revisions; unrelated
card notices leave in-flight permission refreshes valid. Settlement and disconnect
invalidate obsolete responses before removing their pending requests.

### automations

`src/stores/automations.ts` exposes `automationsUi` for dialog visibility and
`createAutomationsStore(adapter)` for dialog-scoped state. It keeps failed-write
drafts, ignores obsolete previews and history replies, and reports Run now
receipts without claiming that a skipped run started. Pause and Resume send only the definition id through atomic backend actions;
they preserve both newer agent edits and an unsaved local prompt. Once stores
a local wall-clock string and uses backend `preview_definition` for UTC
occurrences and completion evidence. The backend owns validation, cron,
capacity, execution and durable history; the adapter owns transport.
Notice navigation reloads current definitions before exposing an editor; historical
run snapshots are never saved as replacement definitions. Deleted definitions
leave saved run details visible without an editable draft.

Automation all-run history uses 20-row pages plus one backend lookahead row. `loadRuns` reads both backend summaries with the page and rejects stale replies. Definition deletion does not remove the all-history view. The transport adapter accepts an explicit remote connection so global history and summaries do not fall back to the local ledger.


### Automatic agent toast cards (#1760-bab7)

MCP toast notices show a transient card only while their origin terminal tab is active. Settings → Notifications → **Show cards from the active terminal** defaults on and is saved per client; turning it off restores urgent-only cards. Urgent notices always show a card, and notices without an origin session remain bell-only unless urgent. The listener reads `terminalsStore.state.activeId` at arrival; `notificationsStore.state.autoToastCards` is stored under `tui-commander-auto-toast-cards` in localStorage. Bell deduplication, sound and terminal navigation are preserved.
### Transcript view preference

`transcriptPreferences` stores the shared terminal Chat and AI Chat new-text indicator toggle in client localStorage (`tui-commander-new-text-indicator`, default on). Desktop Appearance → Chat and mobile Settings edit the same client preference. It does not write backend configuration. The Transcript compares entry identities and text lengths to detect arrivals while detached, clears unread state when content is replaced, and follows in-place streamed text while attached.

Background entry uses `repositoriesStore.snapshotTerminals()` followed by
`flushSave()` to await both the in-flight save and the newest queued snapshot.
Only then does the main WebView acknowledge `snapshot-now`. A persistence error
is logged and leaves the acknowledgement pending for the backend timeout.

Tasks notice preferences use `uiStore.tasksViewPreferences`: a default-off policy and per-provider/account/item follow state with comment baselines. Existing detail refreshes send snapshots to Rust through `compare_work_item_comments`; native delivery reuses the PR notice service. No additional store or provider poller is created.

The settings Voice picker combines `speechVoices` (installed) with the language-scoped voice entries in `speechAssets` (downloads); a configured missing catalogue voice remains visible.

`TerminalData.providerRecovery` is an optional transient native-ego recovery
snapshot. `useAgentPolling.applySessionState` applies it from both lifecycle
catch-up and push snapshots; omission clears it. It never changes shell busy
semantics, awaiting state, or turn completion.

### Settings reload coordination

`services/settingsRefresh.ts` owns one disposable settings-event subscription per window. Readers register a section and optional remote connection, plus separate read and non-saving apply functions. It coalesces same-turn hints, deduplicates each owner/section generation, invalidates older in-flight reads and refetches on SSE resync. Replacement reads wait for the invalidated operation to settle, crossing the IPC read-deduplication boundary. A remote-machine hint only reaches readers registered for that connection.

App settings, notifications, preferences, repository defaults and overrides, registration metadata, agent configurations, prompts, pane layout, tunnels, connection metadata and upstream lists adopt reads without bootstrap migration. Mounted owner pages also refresh GitHub, ego, Telegram and plugin settings. Dirty form fields remain local; same-field conflicts produce a content-free notice. `createConfigDeltaWriter.adopt` advances the backend baseline. The existing save tail rebases queued edits onto that baseline: unrelated local changes still persist, while conflicting remote leaves are not overwritten automatically. Component-local launch-instruction drafts also survive incoming saved values until blur or explicit reset.

Pane layout reads apply the Rust owner's defaults for omitted `root`, `groups` and `activeGroupId` before baseline adoption and disk restoration. Direct `restore` calls use the same defaults. Restore batches tree, groups, focus and revision publication so reactive consumers see a complete layout, while reconciliation keeps unchanged group and tab identities.

Snapshot adoption merges object leaves. Arrays are atomic owner values: concurrent changes to the same list retain the local list and report a conflict instead of guessing an index-based merge. A remote disconnect cancels in-flight adoption; reconnect clears that owner's generation cursor and fetches a fresh snapshot. Successful daemon mirror stream subscriptions and owner-scoped lag notifications also trigger this resync, even when the health status stayed connected.

## Startup workflow state versus Settings state

[Settings startup workflows](../backend/settings-startup.md) separates device-login state, human confirmation, owner CAS revisions and process-local invalidation generations. A terminal command exiting is not authentication success. `services/egoLogin.ts` remains the UI login entry point. Schema-only consumers use the typed Ego owner `begin_login`/`login_status` workflow; status reads the configured credential owner after termination and provider refresh determines the displayed model state. The typed inspector blocks another write after partial/unknown persistence until authoritative readback, and retains a dirty draft's original CAS revision across incoming refreshes.

The dictation store subscribes separately to `voice` configuration and
`corrections` dictionary generations. Correction refresh calls
`get_correction_map` without saving; the Voice page's new-entry text remains
component-local and is not overwritten. Both subscriptions are disposed together.

Terminal ownership and creation provenance are independent: `TerminalData.isRemote` reflects a configured remote `connection_id`; `isExternallyCreated` preserves the backend `is_remote` HTTP/MCP creation flag for worker-tab cleanup, decoration and completion policy. Local `session-created` events set only creation provenance; startup reconciliation derives ownership from the connection, not the launch transport. Both live creation and reconnect retain the backend-issued `tuic_session` as `tuicSession`. Caller-bound worktree declarations use this stable identity to place a newly spawned tab without changing its actual cwd.

Native story controls retain the QA-only visibility filter through `storiesUi`, alongside All, Open and Closed. The backend derives filter membership and Kanban columns.

### Ideas synchronization

`ideasStore.hydrate()` subscribes once to `ideas-changed` on desktop or SSE, with reconnect resynchronization. Reads wait for queued local persistence. A read raced by a newer mutation or read is discarded and retried when needed. Hydration never saves. The backend merges `save_notes` baselines by stable idea id and changed field, so an agent addition absent from a panel snapshot is not a deletion.
