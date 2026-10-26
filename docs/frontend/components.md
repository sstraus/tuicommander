# Components

The selected plan has an **Edit plan** control in the Plans column. It edits title and document/link with Save and Cancel. Save sends only changed fields using the revision captured when editing started. A conflict keeps the draft and requires Cancel and reopening Edit plan to load the latest version; an active-coordinator refusal keeps the source draft and displays the backend remedy. Successful saves refresh the plan list.

Repository Settings uses scoped cards and responsive labelled rows. The existing repository update callbacks and export command remain authoritative. Reset is presented with its repository scope and consequences rather than as an unscoped footer action.

Agents Settings uses page and section primitives while its machine context still owns agent configuration writes. Per-agent lists and nested run/environment editors use scoped responsive styles; drafts, validation, and save handlers are unchanged.

Workspace Settings pages use `SettingPageHeader`, `SectionCard`, and opt-in row fields. `WorkspaceTabs.module.css` scopes shortcut action-first layout without changing capture handlers or global Settings styles.

Application Settings tabs opt into `SettingFields` row presentation. The default stacked layout remains available to unchanged consumers. Row controls use `SettingRow` for label and hint association; slider input and commit events remain separate.


`ui/Select`, `ui/Input` and `ui/Textarea` forward native form attributes and events.
`ui/SegmentedControl` provides a keyboard-accessible single-choice view switch.
`ui/Button` provides primary, secondary, ghost and danger actions in default or compact size.
`ui/Switch`, `ui/Slider` and `ui/ColorInput` forward native checkbox, range and colour input attributes/events. Switch and ColorInput wrap decorative visuals but forward refs, IDs and labels to the native input; ColorInput also accepts `selected` and decorative children.
All shared form controls use `ui/FormControls.module.css`; screen CSS owns layout only. Settings field wrappers retain labels, hints and owner callbacks.
Compose the shared `toolbar` density on header, tab, filter and toolbar rows: 22px height and 6px horizontal padding. Form density stays 36px with 8px padding. Nested configuration menus compose `form` to reset inherited toolbar tokens. Both densities retain 44px touch targets. CSS module compositions can export multiple class names; DOM selectors must join them as a compound selector, not a descendant selector.
Native list-row/card buttons use the explicit `data-control-layout` marker described in the style guide.
The native-select guard in `src/__tests__/nativeSelect.test.ts` rejects new raw
selects outside `ui/` and requires the explicit pending-migration list to shrink.

`Tasks/detail/TasksDetail` is the core work-item detail surface. Its props are
`provider`, `reference`, optional `visible`, `onClose` and `onItemChange`.
It renders schema-driven fields, sanitized Markdown, all declared comment
variants, cursor pagination, a comment composer and provider transition actions.
The shell must pass visibility to pause the default 60-second refresh timer.
Read-only enum fields resolve provider option IDs to their schema labels, falling back to the stored value when an option is unknown. Editing still sends the option ID.

`useTasksDetail` fences responses by item generation and serializes mutations.
Committed mutations remain accepted if reloading opened comment pages fails;
the view reports the history error and Retry reloads those pages.
`TaskMarkdown` preserves reader disclosure state across provider refreshes.
Shell mounting and provider registration are owned by the Tasks integration.

`Tasks/create/TasksCreate` is the core work-item create form. Its props are
`provider`, `onCreated(detail)` and optional `onCancel`. It renders the item-type
picker and per-type fields with `SettingField` and `SettingMultiSelect`.
`useTasksCreate` fences schema responses by generation and submits once;
`createDraft` converts control values to tagged field values.
`TaskCreateUser` renders static choices and optional provider user searches, with
request cancellation and stale-response fencing. Free labels keep raw text until
collection; required-field messages sit beside their inputs.

Slice F exposes `start_graph {target:{type:story|plan,id},expected_revision?,definition_id,definition_revision,request_id,limits?}` through the existing owning-daemon service. Story starts require the current native revision and pin the selected publication; the request ID is bound to its payload. Omitted limits use Rust defaults. Plan dispatch remains unavailable in this build and its start control says so. The `workflow action=run` MCP entry uses an inline schema generated from `RunAction` and the public `RunCommand` variants; it returns the same scoped snapshots and cursor-ordered events as IPC/HTTP. Run history shows pinned activations, decisions/evidence, repair counters, pause targets and complete event payloads across pages. Graph recovery uses `resume_graph {execution_id,activation_id,resolution}` after answering pending input; pause and cancel use the existing sequence-fenced commands. Legacy runs offer inspection and cancellation in the UI. No new persistence or client scheduler is added.

All components are SolidJS functional components in `src/components/`.

`Tasks/roadmap/TasksRoadmap` consumes a data-only `WorkItemProvider`, optional host
start/target date field IDs and an item-opening callback. It reuses `createTasksQuery`
and the list's virtual rows for undated items; `RoadmapTimeline` virtualizes dated rows.
Month/quarter controls navigate local calendar geometry from `calendar.ts`. The default
field mapping uses the first two date descriptors in provider order. Missing, invalid
or reversed dates stay in the side list. Shell mounting is a separate integration step.

`MobileViewBanner` offers browser clients a link to `/mobile`. On tablets it remains visible, even if previously dismissed on a phone, so users who chose desktop in mobile Settings can return. Switching clears the device-local desktop preference; native Tauri never renders the banner.

## Component Tree

```
App.tsx (central orchestrator)
├── Toolbar/                  # Window drag region, repo/branch display
├── Sidebar/                  # Repository tree with branches
│   ├── GroupSection          # Collapsible repo group with color + drag-reorder
│   ├── RepoSection           # Single repo entry with branches
│   ├── ParkedReposPopover    # Popover to recall parked (hidden) repos
│   ├── CiRing               # CI status ring per branch
│   ├── StatusBadge           # Git status badge (clean/dirty/conflict)
│   └── PrDetailPopover/      # PR details popup (CI, reviews, labels)
├── main
│   ├── TabBar/               # Terminal tabs with drag-to-reorder
│   ├── Terminal/             # Native terminal renderer (never unmounted)
│   ├── TerminalArea/         # Terminal + split pane layout (up to 6 panes)
│   ├── SuggestOverlay/       # suggest: follow-up action chips
│   ├── GitPanel/             # Git panel (6 tabs)
│   │   ├── ChangesTab        # Staged/unstaged file list with stage/unstage/discard
│   │   ├── LogTab            # Commit log with expandable diffs
│   │   ├── StashesTab        # Stash list with apply/pop/drop/show
│   │   ├── BranchesTab       # Branch CRUD, prefix folding, search, checkout
│   │   ├── BlameTab          # Line-by-line git blame viewer
│   │   ├── HistoryTab        # Per-file commit history
│   │   ├── CommitGraph       # Visual commit graph with lane assignments
│   │   └── SyncRow           # Push/pull/fetch action bar
│   ├── DiffTab/              # Individual file diff tab (with Cmd+F search)
│   │   └── BranchDiffScrollView  # All-files scroll view (scroll mode)
│   ├── PrDiffTab/            # PR diff viewer tab
│   ├── CodeEditorPanel/      # CodeMirror 6 code editor tab
│   ├── MarkdownPanel/        # Markdown file browser
│   │   └── ContentRenderer  # Markdown to HTML (DOMPurify), interactive checkboxes, tweak highlights
│   ├── HtmlPreviewTab/       # Multi-format preview tab (HTML, PDF, images, video, audio, text)
│   ├── MarkdownTab/          # Markdown tab and detached document window (checkboxes, tweak comments, search)
│   ├── IdeasPanel/           # Ideas panel with edit, send, delete
│   ├── FileBrowserPanel/     # File tree browser with content search
│   │   └── TreeNode          # Recursive tree node (lazy-loaded)
│   ├── PluginPanel/          # Plugin HTML panel (sandboxed iframe)
│   ├── ClaudeUsageDashboard/ # Claude API usage dashboard (SolidJS)
│   ├── CodexUsageDashboard/  # Codex App Server usage dashboard
│   ├── GrokUsageDashboard/   # Grok ACP billing dashboard
│   ├── ErrorLogPanel/        # Application error log viewer
│   └── StatusBar/            # Status messages, agent badge, toggles
│       └── ZoomIndicator     # Font size display
├── TabBar/                   # Ordering, overflow, drag/drop, and menus
│   └── TabViews              # Shared terminal, diff, Markdown, and editor tab views
├── SettingsPanel/            # Settings overlay; nav grouped by task (GLOBAL_TAB_GROUPS)
│   ├── ExpertSetting         # ExpertSetting / ExpertSection / ExpertModeSwitch
│   ├── DictationSettings     # Voice page
│   ├── tabs/GeneralTab       # Language, context bar, confirmations, power management, updates, experimental, TUIC CLI, Code Intelligence, ego executable, IDE, custom launchers
│   ├── tabs/AppearanceTab    # Tabs, repository groups, layout, UI legend
│   ├── tabs/NotificationsTab # Sound and notification prefs
│   ├── tabs/TerminalTab      # Theme, shell, font, cursor, clipboard, blocks
│   ├── tabs/KeyboardShortcutsTab # Rebindable keyboard shortcuts, global hotkey
│   ├── tabs/GitHubTab        # Git & GitHub: OAuth login, PR/issue display, repo + worktree defaults
│   ├── tabs/AgentsTab        # Agent detection, run configs, Claude Usage toggle
│   ├── tabs/AiChatTab        # default model, providers
│   ├── tabs/SmartPromptsTab  # Smart Prompts library
│   ├── tabs/RemoteMachinesTab # Remote Machines page (wraps services/RemoteMachinesPanel)
│   ├── tabs/PluginsTab       # Plugin management, logs
│   ├── tabs/services/        # LocalMcpPanel + UpstreamMcpPanel (MCP page), RemoteAccessPanel, RemoteMachinesPanel
│   ├── tabs/RepoScriptsTab   # Per-repo scripts
│   └── tabs/RepoWorktreeTab  # Per-repo worktree options
├── HelpPanel/                # Keyboard shortcuts documentation
├── TaskQueuePanel/           # Agent task queue
├── PromptOverlay/            # Agent prompt interception
├── PromptDrawer/             # Prompt library management
├── CommandPalette/           # Cmd+P / browser-toolbar palette with transport-safe actions
├── ActivityDashboard/        # Compact inline or detached terminal activity list
├── BranchSwitcher/           # Quick branch switcher (held-key overlay)
├── BranchPopover/            # Branch selection popover
├── TipOfTheDay/              # Startup tip notification
├── DictationToast/           # Dictation recording/transcribing indicator, hands-free meter, reply controls and the spoken-replies toggle
├── ConfirmDialog/            # Reusable in-app confirmation dialog
├── RenameBranchDialog/       # Branch rename dialog
├── CreateWorktreeDialog/     # Worktree creation dialog
├── PostMergeCleanupDialog/   # Post-merge cleanup (switch base, pull, delete)
├── PromptDialog/             # Text input prompt dialog
├── RunCommandDialog/         # Configure terminal commands
├── WorktreeManager/          # Overlay panel for worktree management
├── MergePostActionDialog/    # Dialog for post-merge actions (keep/delete branch)
├── ContextMenu/              # Shared right-click menu (all panels). `separator:true` on an item = trailing divider AFTER it; empty-label item = standalone divider; a trailing separator on the LAST item is suppressed
└── IdeLauncher/              # Open repository in IDE
```

## Application Controllers

Application lifecycles live in focused hooks under `src/hooks/`; `App.tsx`
composes them and owns the top-level layout. Git operations retain the
`useGitOperations` facade for callers, with stateful domains implemented under
`src/hooks/git/`:

- repository refresh and stale-result suppression;
- serialized branch selection;
- terminal/worktree ownership and OSC 7 reassignment;
- worktree creation, setup, recovery, and removal;
- merge, autofix, and conflict-assistance workflows.

Each coordinator owns its timers, queues, generations, or locks. These are
behavioral boundaries rather than generic service wrappers.

`PanelOrchestrator` loads the AI Chat panel after its first inline opening and
keeps it mounted when hidden. The detached AI Chat adapter loads the same panel
when its window opens. Neither path loads its markdown renderer before the
desktop terminal view.
The detached Markdown document adapter likewise loads `MarkdownTab` only when
its window renders, so its renderer stays outside the initial desktop graph.
`SessionControls` lists ego's durable `session/list` results by `updatedAt` and
loads a picked session through ACP. `useAcpChat` restores the saved root-to-session
binding from app config after a fresh document opens.
The control bar summarizes the current model and mode, with pending values while
the selected attachment has not published options. Its session settings
button opens a shared-style modal with one labeled select and description per
ACP select config option. Choices and displayed values come from the selected
session's latest attachment snapshot. Flat choices use ACP `value` fields;
grouped choices use `group` and nested `options`. Rejected changes show an error
in the modal.
The panel draws open chat tabs from `aiChatTabs`; each tab selects one ACP
session on the shared default connection or its saved custom-launch connection.
Selecting a restored tab connects and attaches or replays it without a prompt;
the panel shows connecting status, or the reason and Retry after a refusal.
Concurrent selections share a pending default connection and only the latest
selection changes focus. A detached window reads the saved tab list.
`aiChatDraft` keeps unsent text and
staged images per tab while the document is mounted.
ACP session title updates rename the panel header and picker entry. The usage
footer shows context-window occupancy and the reported cumulative cost.
Untitled sessions use their first prompt or latest activity time in tabs and the picker,
with the session ID in the tooltip. Small single-choice elicitation
forms use direct answer buttons and Cancel. Prompt failures and empty completed
turns appear in the transcript.
`Transcript` displays one provider-retry status line. `useAcpChat` supplies its
live countdown, clamped at “retrying now”, through a panel-owned timer. User-message
Copy buttons use a single unwrapped line, including in the full app shell.

`Transcript` interprets complete ACP answer text as it streams: it hides the
connection acknowledgement, shows a declared intent as status, and offers
bracketed `suggest:` items as prompt buttons. Markdown examples stay literal.
Messages, tool output and code blocks have copy actions. Chat preserves fenced-code padding inside highlighted answers and places Copy in a normal-flow row above the code, so the control neither clips nor covers wrapped text. The transcript has
text selection and find/select-all/clear shortcuts. Find selects phrases across
inline Markdown within each block, skips collapsed disclosure bodies, and closes
on Escape from the input or navigation buttons. Local file paths use the
terminal's backend path resolver and file opener. Web links use the shared
external URL opener. A detached AI Chat window sends resolved file links to
the main window's same file opener, where the editor and viewer tabs live.
Tool-call dots pulse only while a call is pending or in progress. The
transcript projection settles unfinished calls when their turn ends, so a
missing final tool update cannot leave an old dot animating.
The status-bar AI Chat toggle uses the shared `CountBadge` to show pending ACP
questions while the panel is hidden.
The composer stages pasted images in its shared draft and sends ACP image
blocks through `acpClient.prompt`. It checks `promptImage` and the shared 10 MiB cap
before reading clipboard bytes; each preview can be removed before sending.
The mobile HTTP prompt route allows base64 expansion of that cap; images refused
by the draft show their size and the limit in the composer.
During a turn it offers **Queue** beside **Stop**, lists the host-owned queued
prompts, and can remove any queued ID. The list follows ACP snapshots and
events, so another window or a phone sees the same order and cancellations.

## Mobile Screens (`src/mobile/`)

`useSessionChatView` offers the existing header CLI / Chat switch for Claude, Grok, Codex and ego through the same supported-adapter predicate as desktop. `chat_view_snapshot` owns transcript binding; a refusal returns to CLI with its reason. The shared transcript does not replace the mounted terminal, composer, keyboard bar or native question controls.

`SessionDetailScreen` replaces its state with each complete WebSocket or polled
session snapshot, except polls whose `last_activity_ms` is older than the applied
state. Equal timestamps remain replaceable; this field is not a protocol revision.
Omitted optional fields mean cleared state, not missing deltas.
A fresh poll can therefore retire a stale socket's agent identity and question,
return Chat to CLI, and route the composer to the remaining shell after agent exit.

Mobile connection presentation follows `useSessions`, not the independent version poll. Recovery increments a root revision consumed by Files, Chat settings, Activity, Settings and Progress discovery. Failed Files reads retry without replacing unsaved edits. During an outage, tab views stay mounted but hidden so contradictory empty/setup errors do not replace the shared connection banner. Mobile Chat hydrates settings quietly and does not infer missing ego configuration from an unsuccessful read.

`OpenMarkdownMenu` adds a scrollable Markdown section to session overflow.
It reads repository-scoped desktop file metadata and refetches on tab changes,
repository changes and SSE resynchronization. Selecting an entry keeps the
session mounted and opens `FilesScreen` with the entry’s original checkout root.

`OutputView` keeps a 500-line live tail while following. While the reader is
scrolled up or touching the output, it retains the loaded rows instead of
evicting the content being read. Older-history pages extend that reading window.
The next log update after following resumes restores the 500-line tail.
The backend preserves the full Claude `/resume` modal in both the initial HTTP
screen and log-WebSocket frames, including session rows and no-match results;
the mobile view does not reconstruct the picker from transcript text.

Terminal ego selectors open the existing `ChoicePromptOverlay` when a new native question arrives or a waiting session is opened. The header question action can close and reopen it; equivalent polling snapshots and highlight changes preserve a manual close. Full titles and option labels, including descriptions, wrap within its scrollable overlay. A `selection_required` composer refusal reopens the current question without sending the refused draft. Free-text cards still open explicitly; the terminal keeps its full height. The composer uses the backend navigation selection mode and its existing in-flight guard to submit only once.

The session composer has a 44 px paperclip in its existing row. It uploads a
selected file and stages `@path` without Enter; the upload chip floats above
the row. Mobile AI Chat uses the shared composer to stage images as ACP image
blocks and other files as ACP resource links with a readable path in text.
The mobile PWA opens on Sessions. Its bottom tabs are Sessions, Chat (only with ego configured),
Files, Progress, and Activity; Settings opens from the app-bar overflow.
The session detail screen loads after a session is selected (or opened from a
deep link), keeping its terminal and attachment controls out of the Sessions
entry graph.

`SessionsScreen` shares one debounced status accessor per session between its card badge
and the Working, Idle and Awaiting header counts. Accessors are keyed by session id
so the Working hold survives replacement snapshots. Sub-tasks counts as Working.
Finished and other badge states
are excluded from these totals; exited sessions leave the live snapshot. The counts
cover the full snapshot even while search is open. Working and Idle are read-only.
`SessionsScreen` filters its existing session snapshot locally while search is open;
the backend list and card order remain unchanged. Closing search clears the query.
Waiting sessions sort above idle agents and shells without changing order within each
group. The new-session sheet chooses an agent and repository, then opens the new
session after `spawn_agent` succeeds. The question counter opens the first waiting
session from the AWAITING tile, without a duplicate app-bar count; each banner shows its session name and repository.
Each session card uses separate buttons to open or kill the session. Both have
accessible names, and the kill button has a 44 px hit area. The list reserves
space below its last card so the floating new-session button cannot cover it.

`SessionDetailScreen` uses one 56 px header row with the agent logo and state dot,
display name, repository/branch and elapsed state, tasks count, and overflow.
The name, tasks, and session-scoped Progress open temporary sheets over the
terminal; Commands, Files, output search, Ideas, usage, copy ID, and terminate
remain reachable in overflow. No fixed intent/task rows reduce terminal space.

`MobileApp` measures `visualViewport.height` and `offsetTop` into the fixed shell's
CSS variables. Resize, viewport scroll, `pageshow`, foreground `visibilitychange`
and document `resume` share one animation-frame measurement. Lifecycle listeners
are removed on unmount. This prevents stale geometry after a restore without a
viewport event; it does not establish the cause of the reported iPhone grey screen.

`MobileApp` keeps `SessionDetailScreen` mounted while its overflow opens the shared
`FilesScreen` at the session's worktree or containing registered repository.
The session's output stream and command draft stay alive while Files is shown;
the regular Files bottom tab still starts at the repository picker.
Repository paths preserve their trailing segment and reveal the full path on
long press. The tree sorts hidden folders after normal entries and uses the
backend's recursive `search_files` route for nested matches. File actions share
the title row; the editor fills the remaining height and wraps source lines.
Markdown references in output use the desktop path resolver and open the Files
viewer only within a registered repository. External paths show a toast with the
resolved path. HTTP(S) output links open outside the PWA.
Markdown images in the Files viewer resolve relative to their source file and
load through the authenticated `/fs/markdown-image` route.

Desktop Markdown list comment actions use a shared gutter outside the outermost list, independent of item depth. Paragraph and heading actions retain their block-relative position.

`FilesScreen` receives the existing mobile session list and the originating session ID from `MobileApp`. Its Markdown review controls prefer the document opener from published desktop metadata, resolve the stable TUIC identity to a live repository agent, and preserve an explicit user choice. Without an opener, the originating mobile session remains the default; an unavailable opener falls back to the first eligible repository agent. The controls call `enqueue_agent_command` over HTTP. Desktop and mobile use `buildMarkdownReviewPrompt` for the same path, checkbox, and tweak-comment contract. Successful sends clear only the acknowledged review state; failed sends remain retryable.

The embedded Progress list clamps long messages to four lines until More is
pressed. Activity presents local 24-hour times, minute durations, and singular
block counts. Mobile Settings shows app/server versions and a persistent Dark
or Light choice using the desktop theme tokens. Its server-side `mobile_theme`
preference is independent of the desktop terminal theme.

The detail keybar and composer each keep their previous total height while their
controls use 44 px touch targets. The keybar slash button inserts `/` at the
composer selection through the same input path as typing. It preserves the surrounding draft, syncs the same
PTY delta, and shows the agent parser menu with navigation. Picking a command
updates the input; closing the menu clears the input through the same sync path.
A vanished session keeps its output visible but disables both input controls.

The mobile composer owns its draft. PTY prompt snapshots do not create a draft
or extend it on their own: a hands-free paste can be visible before Enter, and
a queued agent message can look like prompt input. A Tab request from the
composer or keybar accepts one strict extension of a nonempty local draft.
Explicit keybar Up/Down accepts one history replacement after its PTY write is
acknowledged, including an empty line, and updates the edit delta baseline.
When a managed agent awaits an answer without parsed choices, keybar Tab and
Up/Down still go to the PTY so an open picker can navigate. These keys do not
import history into the local answer draft.
Typing, sending, Escape, or a failed write cancels the request. A history response
arriving before the HTTP acknowledgement is dropped, causing a missed recall.
Because snapshots have no causal request ID, an unrelated delayed snapshot after
the acknowledgement can still be mistaken for history.

## Core Components

### PluginPanel (`PluginPanel/`)

URL tabs and inline plugin panels mount their sandboxed iframe only while their tab is visible. Hiding a tab, switching repositories or pane tabs, or covering split panes with an orphan tab removes the iframe from the DOM. Showing it again loads the page anew, so iframe scroll, focus and JavaScript state do not persist. This prevents hidden page timers from blocking terminal input on the shared WebContent main thread. Native `tuic://edit` and `tuic://open` tabs use their own components and are unaffected.

### Terminal (`Terminal/`)

Native terminal renderer with full PTY integration.

**Responsibilities:**
- Creates and manages CanvasTerminal instance backed by `alacritty_terminal`
- Renders grid frames to HTML canvas for GPU-accelerated display
- Subscribes to PTY output events
- Handles terminal resize (with debouncing)
- Applies font, theme, and zoom settings
- Link detection for clickable URLs
- Selection management for copy operations
- Terminal search retains match highlights across unchanged or colour-only grid frames; changed text invalidates affected rows before the backend refresh completes.

`CanvasTerminal` keeps frame decode, reconciliation, scheduling, and paint in
one imperative hot path. Sibling controllers own selection/search state, link
verification cancellation and caches, fractional scroll/cache handoff, and DOM
input-listener cleanup. These controllers do not use reactive state.

**Key behavior:** Terminals are **never unmounted** — they stay in the DOM when switching tabs. Only visibility is toggled. This preserves terminal state (scroll position, content, active processes).

### Sidebar (`Sidebar/`)

Session rows use the shared ContextMenu and writeClipboard utility to copy their PTY session ID, child PID, or alias. PID and alias come from the owning row of an on-demand `list_active_sessions` snapshot, shared with `GET /sessions`; they are checked again before copying. Unknown PID and absent alias entries are hidden, including exited or suspended sessions. Both desktop and mobile copy confirmation uses the toast store.

Repository tree with branch management. Touch branch rows reveal their existing
actions in a full-height tray after a horizontal left swipe. A Sidebar-owned
row identity permits one open tray across repositories; vertical scrolling and
mouse interactions keep their existing behavior.

Branch rows keep the status icon separate from the always-visible agents disclosure button. Clicking status selects the workspace; the chevron toggles its agents.


**Features:**
- Expandable/collapsible repository entries
- Icon-only collapsed mode
- Branch list with active branch highlight
- CI ring indicator per branch (from githubStore)
- PR status badge
- Nested sub-agent rows use a muted parent robot tag, while the actionable GitHub badge keeps the accent color
- Compact diff stats (additions/deletions) with exact tooltip counts
- Workspace lifecycle badge from the backend (`Dirty`, `Merged`, or `Unknown`), plus a compact unmerged-commits mark,
  keyed by workspace id; `Dirty` and `Unknown` expose their meaning and removal
  consequence in a WebView-compatible hover/focus tooltip. Branch-name tooltips include each
  diagnostic once and omit removal warnings for main checkouts
- Context menu (right-click) for repo/branch operations
- Resizable width via drag handle (200-500px)
- Keyboard redirect to active terminal

Shared PR presentation (`PrStateBadge`) and merge eligibility are leaf modules
below the sidebar views. `RepoSection`, `GitHubPanel`, `PrSection`, and
`RemoteOnlyPrPopover` do not import back through one another.

### TabBar (`TabBar/`)

Terminal tab management.

**Features:**
- Tabs filtered to active branch only
- Drag-to-reorder tabs
- Tab rename (double-click)
- Close button per tab
- Activity indicator (dot) for background terminals
- Awaiting input indicator (question/error icons)
- Context menu: Close, Close Others, Close to Right
- Context menu, debug builds only (`isPerfDebug()`): **Capture Session** — arms the raw PTY
  capture tap on that session so a state-detection bug can be recorded as it happens. See
  `docs/frontend/utilities.md` → `ptyCapture.ts`.

### PromptDrawer (`PromptDrawer/`)

Creates, edits, and executes custom and built-in Smart Prompts. A normal click or
keyboard Enter follows the prompt's `autoExecute` value. Double-click and
**Insert & Run** explicitly submit once; **Insert** explicitly keeps the resolved
text editable. The drawer delays a single pointer click until the double-click
window closes so one gesture cannot trigger both delivery paths.

### SettingsPanel (`SettingsPanel/`)

Settings overlay. The nav groups the global pages by task
(`GLOBAL_TAB_GROUPS` in `SettingsPanel.tsx`); each group renders as a static
label row above its pages, and the configured repositories follow under
**Repositories**.

**Pages:**
- **Application**
  - **General** (`GeneralTab`) — Language, agent context bar, confirmations, power management, updates, Experimental Features, TUIC CLI, Code Intelligence, ego executable (always shown), default IDE, custom launchers
  - **Appearance** (`AppearanceTab`) — Tabs, repository groups, layout reset, UI legend
  - **Notifications** (`NotificationsTab`) — Sound and notification preferences
- **Workspace**
  - **Terminal** (`TerminalTab`) — Theme, shell, font, font size and weight, cursor style, clipboard, command blocks, scrollback reflow
  - **Keyboard Shortcuts** (`KeyboardShortcutsTab`) — Rebindable shortcuts (auto-populated from `actionRegistry.ts`) and the global hotkey. The Help panel reuses the same editor
  - **Git & GitHub** (`GitHubTab`) — GitHub OAuth login (Device Flow), token management, diagnostics, PR and issue display, repository and worktree defaults, additional accounts, repository bindings
- **AI**
  - **Agents** (`AgentsTab`) — Agent detection, run configurations, Claude Usage toggle
  - **AI Chat** (`AiChatTab`) — default model, provider list. Hidden while `isAiChatEnabled()` is false. The ego executable is on General
  - **Voice** (`DictationSettings`) — see below
  - **Smart Prompts** (`SmartPromptsTab`)
- **Integrations**
  - **MCP** (`LocalMcpPanel` + `UpstreamMcpPanel`) — HTTP API server status, TUIC tools, upstream MCP servers
  - **Remote Access** (`RemoteAccessPanel`) — Remote access, Tailscale HTTPS, QR/connect URL, cloud relay; local IP discovery failures stay inline with retry while the remaining controls stay available
  - **Remote Machines** (`RemoteMachinesTab`) — `tuic-remote` connections
  - **Plugins** (`PluginsTab`) — Plugin management, enable/disable, log viewer
- **Repositories**
  - **Repo Scripts** — Setup and run scripts, plus the optional per-repository Dev Server URL for Design Mode
  - **Repo Worktree** — Base branch, copy ignored/untracked files

**Expert mode** (`ExpertSetting.tsx`, `stores/settingsExpert.ts`). The header
carries `ExpertModeSwitch`, which flips the persisted UI pref
`settings_expert_mode`. `<ExpertSetting configKey value>` hides its children in
basic mode while `value` equals the default at `configKey`, as returned by
`get_config_defaults`. It shows them in expert mode, when the value is
modified, when a search result revealed that `configKey` during the current
Settings open, or while the defaults are unknown. All of this is one rule,
`settingsExpertStore.isVisible`.

The rule is sticky for an edited control: a native `input` or `change` event
from any child of an `ExpertSetting` calls `settingsExpertStore.pin(configKey)`,
and a pinned control stays shown for the current open, so setting it back to
the default does not make it disappear under the cursor.
`settingsExpertStore.open()`, which `SettingsPanel` calls each time Settings
opens, clears the pins and the search reveals. Pinning happens on the edit, not
when `isVisible` sees a non-default value: a tab's pre-load placeholder (a value
that differs from the default until its config loads) is not an edit and must
still hide. The listeners sit in the capture phase on the resolved top-level
child elements — no wrapper element, which would break the `.group + .group`
and `:last-child` rules — so the pin lands before the control's own handler
changes the value. A control that changes its value without `input`/`change`
(a button) does not pin.

`<ExpertSection>` hides a section, heading included, when all of its
`ExpertSetting`s are hidden. No page uses it today, because every section keeps
at least one basic control.

The search index marks labels inside an `ExpertSetting` with `expert: true` and
the `configKey`. `SettingsSearch` shows an **Expert** badge on such a result
(`data-expert-badge`). Selecting it calls `settingsExpertStore.reveal(configKey)`
before `SettingsPanel` scrolls to the label, because a hidden control has
nothing to scroll to.

One index entry carries one `configKey`, so one label must not cover two expert
controls. On **Git & GitHub**, **Copy ignored files** and **Copy untracked files**
are therefore two `SettingToggle` rows, each in its own `ExpertSetting`, and not
one group under a shared label.

#### DictationSettings (`SettingsPanel/DictationSettings.tsx`)

Hands-free listening is a standard toggle beside it (also in mobile Settings); off arms a replies-only conversation. The `DictationToast` carries a speaker button (`aria-pressed`) that flips the same spoken-replies setting while recording or while a conversation is armed. Spoken replies is a standard toggle, also available in the mobile conversation
header and mobile Settings. It stays visible in browser mode and saves the
backend device preference; it does not stop microphone dictation.

The **Voice** page (nav key `dictation`). One `<h3>` per section. Speech-to-text and
text-to-speech are separate sections, and each keeps its own advanced controls
at its bottom — there is deliberately no shared "Advanced" section:
The Rust dictation crate split does not change these controls or their transport calls.

1. **Dictation** — enable, hotkey, long-press threshold, auto-send
2. **Speech recognition** (`SpeechRecognition`) — input device (desktop only),
   Whisper model, language, then voice tuning: the test recording and both
   speech gates
3. **Auto-Corrections**
4. **Hands-free conversation** (`HandsFreeControls`)
5. **Spoken replies** (`SpeechSetup`)

`SpeechRecognition`, `HandsFreeControls` and `SpeechSetup` each open with their
own heading: the settings search index reads source order, so a sub-component
without one would file its labels under the wrong section.

The two voice sections:

- **Spoken replies** (`SpeechSetup`) — the runtime and language downloads from
  `get_speech_assets` (voices excluded), each with Download / Repair / Cancel /
  delete. Then, while the language ships a voice:
  - a voice `<select>` with the ids from `get_speech_voices` (only voices that
    can speak now), and a **Listen** button that previews the selected voice
    through `previewSpeechVoice` and shows a refusal inline;
  - `VoiceLibrary`, the **Voices** list of the language, in three groups:
    **Installed** (downloaded catalogue voices), **Downloadable** (the others,
    with Download and progress, collapsed by default in a `<details>` whose
    `<summary>` shows the count; both reuse `SpeechAssetRow`) and **Yours** (the
    user's voice files, each with a delete button, and **Add voice file…**, a
    hidden `.safetensors` file input; a refused file shows its reason inline);
  - two `SettingSlider`s: **Voice volume** (-30 to -12 dB) and **Levelling**
    (Off to Strong, stored as 0–1). Each shows the value while it is dragged and
    saves only on release (`onCommit`).
  It offers **no language control**: the spoken language is the Whisper
  language, and a second control would be a second source that disagrees with it
  (see `docs/backend/dictation.md` → "The language of the conversation"). A
  voice row is **Active** when it is the selected voice; only a language row is
  active for the language.
- **Hands-free conversation** (`HandsFreeControls`) — a terminal picker, Start /
  Stop (accent Start, distinct Stop), a coloured Running/Stopped indicator, the polled phase, the activation phrase, the hold-back slider, the
  earcons toggle, the "notify model" toggle and, while that is on, the start
  notice textarea. Its placeholder is `getDefaultHandsFreeStartNotice()` — Rust
  owns the built-in text, the frontend keeps no copy. The slider edits the
  saved delay; the polled status gives the effective delay, which is at least
  five seconds when an activation phrase is configured.

**Nothing here arms on mount.** Opening this panel must never open the
microphone, and neither must starting the app: `onMount` only starts a 500 ms
status poll, cleared in `onCleanup`. Push-to-talk (the hotkey) and continuous
mode (Start) are rendered as the two distinct mechanisms they are — there is no
"mode" config field behind them.

Since 832-e730 the tab **renders in browser mode** and `HandsFreeControls` still
has no browser branch — this time because none is needed. `armHandsFree` opens
the tab's own audio socket and passes its own owner id, so the same control runs
a conversation on either transport. Two groups stay behind `isTauri()`:

- the **global hotkey** and its long-press slider, which a browser cannot
  register; and
- the **microphone device** list, which enumerates the devices of the machine
  running TUICommander. A browser user picking from that list would be choosing
  hardware in another building — the browser's own device picker is the
  platform's, not ours.

`SpeechSetup` and `HandsFreeControls` are defined at the **bottom** of the file,
after `VoiceTuning`, because `extractSettings` builds the settings search index
from source order — their entries in `settingsSearchIndex.ts` follow in the same
order.

### PrDetailPopover (`PrDetailPopover/`)

Rich PR detail popup shown on hover/click in sidebar.

**Displays:**
- PR title, number, author
- State (open, merged, closed, draft)
- Merge readiness (ready, conflicts, behind, blocked)
- Review decision (approved, changes requested, review required)
- CI check summary (passed/failed/pending ring)
- Individual CI check details
- Labels with computed colors
- Line change counts (+additions/-deletions)
- Timestamps (created, updated)

### StatusBar (`StatusBar/`)

Status messages, agent badge, CWD display, ticker, and panel toggles.

**Layout (left to right):**
1. **ZoomIndicator** — font size display
2. **Status info** — notification text with pendulum ticker for overflow
3. **CWD** — current working directory (click to copy, shortened with `~/`)
4. **Agent badge** — unified agent + usage display (see below)
5. **Ticker** — rotating plugin messages (hidden when absorbed by agent badge)
6. **GitHub badges** — PR badge + CI badge with popover (center area)
7. **Toggle buttons** — Notes (with badge count), File Browser, Markdown, Diff, Dictation mic

**Agent Badge — display priority:**

The agent badge appears when the active terminal has a recognized agent type. It shows a single integrated element with the agent icon and the most relevant info, following this priority cascade:

| Priority | Condition | Display | Example |
|----------|-----------|---------|---------|
| 1 (highest) | PTY rate limit detected | Icon + warning + countdown | `⚠ 3m 20s` |
| 2 | Usage API available (Claude only) | Icon + usage percentages | `5h: 6% · 7d: 69%` |
| 3 | PTY usage limit parsed | Icon + percentage + limit type | `82% daily` |
| 4 (lowest) | No usage data | Icon + agent name | `claude` |

**Data sources:**
- **Rate limit (priority 1):** Detected by Rust output parser via regex on PTY output (e.g. "429", "rate limit", "too many requests"). Stored in `rateLimitStore`. Applies to all agents.
- **Usage API (priority 2):** Polled every 5 min from Claude's API by `claudeUsage.ts`. Posted to `statusBarTicker` with pluginId `"claude-usage"`. Claude Code only. When active, the separate ticker message is suppressed to avoid duplication.
- **PTY usage limit (priority 3):** Parsed from terminal output by the output parser (e.g. Claude's `[C1 S30 K26]` status line). Stored on the terminal entry as `usageLimit`. Applies to all agents that emit usage info.
- **Agent name (priority 4):** Fallback — just shows the agent type name.

**Usage blink threshold:** API ticker priorities and PTY percentages apply the critical animation only from 96% usage. Usage from 70% to below 96% stays warning-coloured. Dashboard critical colours remain at 90%.

**Ticker integration:** When the active agent is `claude` and the Claude Usage ticker is active, the ticker message is absorbed into the agent badge (priority 2) and hidden from the separate ticker area. Other ticker messages (from plugins, etc.) display normally.

**Pendulum ticker:** When the status info text overflows its container, a CSS pendulum animation scrolls the text back and forth at ~50px/s. Clicking the text dismisses the notification until the message changes.

**Ideas badge:** The Ideas toggle button shows a count badge (accent-colored) with the number of ideas visible for the current repo filter. Uses `ideasStore.filteredCount()`.

**PR lifecycle in StatusBar:** CLOSED PRs are never shown. MERGED PRs are shown with a 5-minute activity-based grace period (accumulated user activity tracked by `userActivityStore`). OPEN PRs are shown as-is.

### IdeasPanel (`IdeasPanel/`)

Ideas panel with per-repo filtering and terminal integration.

**Features:**
- Add, edit, delete notes
- Send note text to active terminal (marks note as "used")
- Notes filtered by active repo (global notes always visible)
- Reassign notes to different projects via dropdown
- Count badge in panel header and in the StatusBar toggle button
- Used notes shown with a checkmark and dimmed styling

### StoriesDialog (`StoriesDialog/`)

Archive and restore controls send the selected record revision to the shared native service. Independent Archived plans and Archived stories toggles select archived records; Rust projects list ids and Kanban columns. Archived details stay readable, but work actions and new-story creation under archived plans are unavailable until restoration. Ownership refusals use the existing error banner.

QA stories have a separate status label, QA-only filter and Kanban column. Criteria remain editable in QA. QA passed sends approval; QA failed returns development work to In progress. These controls use the shared revision-checked story service.

Story details show the persisted origin, plan step, revision and claiming session. A live terminal name appears beside the stable session ID; closed or unavailable terminals retain the ID. Transition history shows committed commands, revisions and actors in backend order, with Refresh history and explicit loading, error and empty states. New story offers Native or Plan step origin; a plan-step story requires a step label.

The project-scoped plan and story dialog creates manual plans and stories, presents criteria and dependency status, and sends revision-checked actions through the shared IPC/HTTP transport. It uses wire status types generated from Rust with `python3 scripts/generate-story-types.py` (`--check` detects drift). The selected plan state stays visible beside cancellation counts while a story is selected. It renders the Rust `plan_view` projection, including transitive abandoned dependency labels and the WontFix count or all-cancelled label. Only a direct WontFix dependency on a Backlog story offers the human Remove action; the backend decides whether the dependent becomes Ready. Visible labels use `t()` and English catalog entries, the controls use style-guide tokens and button variants, and the close button takes focus when the dialog opens.

The Plans and Stories toolbar button (`StoriesButton.tsx`) widens into a pill while the project has an active plan: active plans, stories in progress and a done/total bar, with the breakdown in its tooltip and `aria-label`. Counts come from the Rust `summary` story action. Nothing signals story changes, so it re-reads on repo change, window focus or visibility, dialog close, and every 60 s while the window is visible; a failed call leaves the plain icon.

The dialog probes `story_capabilities` before loading. A missing capability receives the restart message; action failures retain their own error details.

The project-scoped Plans and Stories dialog supports manual plan/story actions
and a separate Run history view. Run history reads persisted run snapshots and
ordered events through `workflow_run_action`; `workflowRunSignals` is a wake hint
that causes a cursor-based read, so missed or duplicate notifications cannot
replace the backend's durable sequence. The view pages events and stays usable
without starting a workflow.
The review view offers Approve in both the desktop app and browser. The backend
rejects approval from the managed session that claimed the story.

### ProgressDialog (`ProgressDialog/`)

The whole Progress UI: one newest-first list for the active PTY by default,
with a selector for other PTYs and the repository aggregate. It has a divider
marking where the last visit ended, and a blocked-only
checkbox. Blocked entries are red, host-written `intent` entries are muted, and
each row can be deleted. The list opens with at most 100 entries; **Load older**
appends another cursor page. The control is disabled while a read is pending.
The header uses its project selector as the project label when available.
Without a selector, including on desktop, it shows a project subtitle instead.
There is no export — Progress is a thing you glance at, so it is a dialog and not a
panel that competes with the terminal for width.
The toolbar bell always includes Terminal Progress, regardless of unread count.
The command palette and `Cmd/Ctrl+Shift+P` open the same dialog.
On macOS, clicking a native `done` or `blocked` notification opens this dialog
for the notification's project and source terminal, even if another project is active.

Opening asks the shared journal once, for the selected PTY or project. The old panel
fanned out across every registered repository and answered with one red
unavailable block per repository that no longer existed; the dialog shows one
project and one failure line.

Each PTY and the aggregate has a separate divider, frozen while that view is
open. Switching views loads the new scope's mark; closing records every visited
scope without moving a line under the reader's cursor.

A **List | Flow** toggle switches to `ProgressFlow.tsx`, a sequence diagram of
the same journal: one column per participant (terminal or Claude subagent), one
row per event, arrows for delegations, returns and messages, and intents as
muted notes. Every row is the same CSS grid, so lifelines, headers and arrow
ends line up without measurement. Journal text expands in place; a subagent
arrow fetches its full text through `progressStore.fetchFlowDetail`. The
component only renders what `progress_flow` returns.

`embedded` drops the overlay and the floating box so the mobile PWA's Progress
tab can host the same component full-bleed; a whole bottom tab is already the
modal surface a dialog would create. On phone widths, its header wraps the
title, project and terminal selectors, view toggle, and blocked filter so each
control remains visible without horizontal scrolling.

### ConfirmDialog (`ConfirmDialog/`)

Reusable in-app confirmation dialog that replaces native Tauri `ask()` dialogs (which render as light-mode macOS system sheets). Uses shared `dialog.module.css` for consistent dark-theme styling.

**Props:** `visible`, `title`, `message`, `confirmLabel`, `cancelLabel`, `kind` (warning/info/error), `onClose`, `onConfirm`.

**Keyboard:** Enter confirms, Escape cancels.

### ClaudeUsageDashboard (`ClaudeUsageDashboard/`)

Native SolidJS component (not a plugin) showing Claude API usage data. Displayed as a tab in the markdown/editor area. Features rate bucket gauges, per-model token breakdown, daily usage chart, and project stats. Clicking the Claude Usage ticker opens it for that terminal session's credential profile. The rate-limit API follows `CLAUDE_CONFIG_DIR`; transcript statistics still use the default Claude projects directory.

### CodexUsageDashboard (`CodexUsageDashboard/`)

Native dashboard for the official Codex App Server account snapshot. Shows rate
windows, reset times, daily token buckets, and the lifetime metrics that the
documented surface actually supplies. Unsupported legacy metrics are omitted.

### GrokUsageDashboard (`GrokUsageDashboard/`)

Native dashboard for Grok Build's `_x.ai/billing` ACP extension. Shows the
current billing-period percentage, subscription tier, period end, on-demand
used/cap amounts, and prepaid balance. It shares the usage-dashboard visual
system and never turns an absent provider value into zero.

## UI Primitives (`components/ui/`)

| Component | Description |
|-----------|-------------|
| `AgentIcon` | Agent type icon with consistent sizing and coloring |
| `CiRing` | SVG circular CI status indicator with proportional segments |
| `DiffViewer` | Syntax-highlighted unified diff renderer |
| `Dropdown` | Reusable dropdown select component |
| `ContentRenderer` | Safe markdown-to-HTML rendering with DOMPurify sanitization (including raw form and image-map removal), with escaped preformatted text on parser failure, interactive checkboxes, tweak highlights, and click interception for every rendered link; Mermaid 12 diagrams retain the dark theme and strict security setting while using upstream layout and appearance defaults; `MarkdownTab` sends local href resolution to Rust |
| `PanelResizeHandle` | Draggable resize handle for panel boundaries |
| `PromptOption` | Agent prompt multiple-choice option |
| `StatusBadge` | Git status badges (clean/dirty/conflict) |
| `ZoomIndicator` | Terminal font size indicator |

## Shared Components (`components/shared/`)

| Component | Description |
|-----------|-------------|
| `ColorPickerDialog` | Color selection dialog (used by repo groups) |
| `ColorSwatchPicker` | Preset color swatch grid |
| `KeyComboCapture` | Keyboard shortcut capture input (for keybinding editor) |
| `SearchBar` | Reusable search bar with regex/case-sensitive toggles |

## Panel Toggle States

| Panel | Toggle Shortcut | Store |
|-------|-----------------|-------|
| Sidebar | `Cmd+B` | `uiStore.toggleSidebar()` |
| Git Panel | `Cmd+Shift+D` | `uiStore.toggleGitPanel()` |
| Markdown Panel | `Cmd+Shift+M` | `uiStore.toggleMarkdownPanel()` |
| Ideas Panel | `Cmd+Alt+N` | `uiStore.toggleIdeasPanel()` |
| File Browser | `Cmd+E` | `uiStore.toggleFileBrowserPanel()` |
| Settings | `Cmd+,` | Local state in App.tsx |
| Help | `Cmd+?` | Local state in App.tsx |
| Prompt Library | `Cmd+Shift+K` | `promptLibraryStore.toggleDrawer()` |
| Task Queue | — | Local state in App.tsx |
| Command Palette | `Cmd+P`; browser toolbar button | `commandPaletteStore.toggle()`; browser mode filters to explicitly supported web/HTTP actions |
| Activity Dashboard | — | `activityDashboardStore.toggle()` |
| Project Progress | Command palette / bell | `progressStore.toggle()` |
| Worktree Manager | `Cmd+Shift+W` | `worktreeManagerStore.toggle()` |

Dashboard row navigation uses the shared `navigateToTerminal` path. It opens the
manual Global Workspace for parked sessions before activating their content,
leaves that workspace for a repository session that is not a member, and focuses
its existing pane group when present. Undocked terminals use the full-page overlay
without changing the split layout being left. Existing tab/session bindings are
preserved. Notification and detached-dashboard navigation use the same path.

Activity Dashboard uses a 500px inline overlay and the same compact row layout
in its detached window. On startup and reopening, saved detached geometry cannot
make the Activity window larger than its 550×650 default; smaller saved sizes
remain in effect. `SubAgentIcon` supplies the shared 11px robot marker
for both the dashboard and sidebar; only its tooltip/accessible name contains
the parent name. The session list supplies each parent's live `tuic_session`
when its PTY ID differs from that identity.

## Private secret form

`SecretForm` is a standalone entry (`secret-form.html`, `src/secretForm.tsx`),
mounted in a separate backend-created WebView or a capability-gated browser.
It imports no App stores, debug globals or logging handlers. Sensitive input
values stay in input elements until submit, then are cleared and unmounted
before IPC/HTTP entry. Only requested fields render; approval instead shows
exact argv/names/directory and optional template consent.

`SettingsPanel/tabs/TelegramTab` renders the shared Telegram setup controls in desktop Settings and, when opened, in mobile `SettingsScreen`. All validation, pairing, private-file writes and polling ownership remain in Rust. The registered agent is read-only; agents opt in through MCP rather than a destination selector.

Pending agent commands in `Terminal` drain once when both the command and an idle shell session are available. Parsed events, shell-state synchronization and WebSocket snapshots feed this same readiness check, including late launch preparation.

Closed refusal turns show one plain-text card with the agent’s existing ACP refusal text. A refusal without text shows a generic refusal message.

Browser HTML and Markdown previews load local images only from open repositories and their linked worktrees, through the authenticated repository image route. Images outside those roots show “Image unavailable: outside open repositories.” Desktop asset loading is unchanged.

`StoriesDialog` run history embeds `RunIncidents`. It renders backend-owned cause and recovery suggestion text with run/attempt/story/session/task identities. It refreshes on run cursor changes and explicit operator refresh, ignores stale responses after selection changes, and discloses unavailable evidence. It does not execute recovery.

Rich `RepoSection` metadata shows PR and worktree count chips only for positive counts, with reactive store updates.

In rich mode, sessions idle for more than two hours fold into an expandable count. Backend activity timestamps determine age; busy sessions, awaiting input, unread output, selected rows and parents of visible children stay visible. Expanding preserves row order. Compact mode does not fold idle sessions.

Rich working agent rows reserve two clamped intent lines. Idle, awaiting-input and error rows stay on one line with status dots; their full intent, task or prompt remains in the tooltip. Compact rows keep their one-line layout.

Rich navigation keeps compact typography and padding. Branch identity and facts share one line, with names truncated and full PR/lifecycle facts in tooltips. Repository metadata disappears when PR and worktree counts are both zero. Header controls use compact sizing to protect repo names. Returned subagents fold into an expandable count independently of running work.

In rich mode, ungrouped repositories with open terminals appear first, followed by an Idle section. Manual order is preserved inside each section. Idle headers start on one line; disclosure reveals their branches without changing activity or saved order. Headers count agents in the backend working state, independently of open-terminal membership.

Rich active/idle sections also apply independently inside each existing repository group. Group membership and group ordering remain intact; active-only filtering hides idle-only groups and empty Idle headings.

Rich merged linked worktrees with no sessions and verified lifecycle facts use a one-line summary: Merged, N uncommitted, Clean up. Live, unmerged, or unknown worktrees retain full rows. Clean up enters the existing lifecycle preflight and confirmation workflow; cancellation preserves the worktree.

Rich parent terminal rows show N agents in each other repository containing live children. Clicking reveals the child group, repository and workspace, then selects a child through existing terminal navigation. Session placement and same-branch nesting stay intact; closed children disappear from summaries.

### Browser repository picker

`ApplicationOverlays` uses `RemoteRepoPicker` for browser **Add Repository** as well as connected daemon folders. An omitted `connectionId` browses the serving TUIC with the existing `get_home_directory` and `list_directory` RPCs; an explicit ID browses that daemon. Selection and cancellation resolve the existing path prompt. Native desktop selection still uses the OS dialog.

### AutomationsDialog

`src/components/AutomationsDialog/` provides a machine-local scheduled-run dialog.
`AutomationsHost` loads it on demand from the command palette. The searchable
list shows the stored zone, next occurrence and last status, including overlap
skips. The editor supports literal prompts, run config, workspace, precheck,
limits, backend cadence presets and backend schedule previews. Once uses
`once_local` plus the stored zone, without browser timezone conversion. Pause, Run now,
save, confirmed deletion and recent history share one adapter. Deletion retains
saved history. Escape uses the modal stack; Tab stays in the dialog.

The Step 8 API is a separate integration dependency. `transportAdapter.ts` is the
only provisional command-envelope boundary; tests inject `AutomationAdapter`.
No schedules or launch decisions are calculated in TypeScript.

`RunsView` in the Automations dialog renders all-history pages and backend 24h/7d summaries. `RunOutput` renders saved stdout/stderr and immutable workspace details. View run only focuses a live terminal with matching daemon ownership; unavailable sessions never launch a replacement.

### Reusable setting field controls

`SettingsPanel/SettingFields.tsx` exports `SettingAccount`, `SettingList`,
`SettingMultiSelect`, `SettingStatus` and `SettingButton`. The implementations in
`SettingsPanel/fields/` share the existing label and hint layout. Editable rows use
stable IDs; multiselect uses checkboxes; status is read-only. The account picker
loads the core account list and delegates Add to a token-free native-flow callback.
These controlled components do not persist settings or define catalogue fields.
The dev-only `fields/preview.html` mounts the actual controls for visual checks.

The Precheck section includes a default-off stdout context checkbox. It edits
`precheck.inject_output` through the existing store and shared save API, without
changing prompt text. Disabling and re-enabling the precheck resets the flag.

The mobile session header uses a handset icon and Start/Stop voice call labels for the two-way voice control. The phone More menu uses the same Start/Stop voice call labels. The composer dictation button keeps its microphone icon.

### Mobile composer browser dictation

Restored dictated drafts stay in the composer without writing to the terminal on mount. Send or the first edit reconciles the saved text with the current PTY input line through the existing delta path. Later edits use the local baseline so delayed terminal echoes do not replay text.

`CommandInput` mounts `useBrowserDictation`, a feature-detected browser interaction hook. Its mic is directly before Send in the existing row; interim hypotheses are replaced and final speech stays in the current draft without focus or automatic Enter. `SessionDetailScreen` excludes server hands-free while browser recognition is pending or active, and `useMobileVoice.recording` blocks browser starts during server arming/recording. Manual typing or keybar text insertion cancels recognition and preserves the edited draft. The mic stays claimed until the browser emits `end`, including after an error; queued results from that recording are ignored. Tap the mic to resume after `end`. Browser errors request cancellation through the same shutdown path as an edit or send. Browser failures are logged as well as toasted; the existing “Dictation failed” label stays beside the mic in the keyboard-visible composer until the next take starts. The hook also stops on send, session end, or unmount. No setting or backend route is added. Bounded lifecycle diagnostics use source `dictation`, level `debug`: start attempt, first result, first error with code, abort request, end and ownership release. Each record includes the captured session ID, a random take ID and a per-take sequence number, not transcript text. The sequence preserves event order even if uploads arrive out of order. Unbound AI Chat drafts use a null session ID. At most six records are emitted per take; they reach the owner through the existing log route.

`IdeasOverlay` reuses the same hook and `MobileDictationButton`. `SessionDetailScreen` passes its existing voice-call and terminal-dictation recording signals to block the Ideas hook and disable its mic while either owns the microphone. It supplies an optional `getSelection` accessor, so each take captures its insertion range and replaces only that range as hypotheses change. Empty hypotheses preserve the original selection. Typing and explicit idea submission cancel recognition before changing the draft. Speech updates only the unsaved idea input, never the terminal or idea store. The hook keeps tail insertion for consumers that omit `getSelection`.

## Tasks board

`Tasks/board/TasksBoard.tsx` renders the provider/core board projection. Each lane
has a virtualized card list. Pointer capture supports drag and drop; the native
move menu provides keyboard and touch access. Moves dispatch the provider's per-card
action, show pending feedback, and restore the origin on rejection. Cancellation
and scope changes clean up drag state. Consumers refresh the projection after a
successful move by merging page one into the loaded cards. Other loaded pages are
retained until reload. The move menu stays open when its own list scrolls; scrolling
outside it dismisses it. The view does not infer workflow transitions.

Terminal marker layout uses `viewportProjection.ts`: suggest anchors and hard-wrapped tails consume no display row; answer marker cells and their trailing space consume no display columns. Intent stays visible. Grid painting, cursor/IME, selection, search and link hit testing use the same reversible projection, including cache-driven smooth scrolling; PTY cells and absolute scrollback/copy coordinates remain unchanged.

Submitting a new prompt clears old choices and reopens follow-up delivery. A `user-input` event can arrive after `shell-state: busy`, so it must not dismiss the choices for the new turn.
Tasks `useStartWork` obtains the brief from `build_start_work_prompt` before the
provider action, with a pending state covering that read. The Rust builder reads
`.tuic-start-work.md` in the repository root; absent files use the legacy brief.
Read failures use the existing error/Retry state. Host delivery retains the
untrusted-content wrapper and 12,000-character cap.

### Terminal Compose attachments

`ComposePanel` uses `useComposeImages` for paste, file drops and its image-only multi-file input. All three reuse `saveImageFile` in `pastedImage.ts`. Drop/picker insertion ranges track document edits during saving. Native file drops route to a registered mounted editor before the existing global terminal/tab dispatcher; closing or unmounting unregisters that target. Rejected files and save failures appear in an accessible alert.

The desktop macOS General page includes **Hide Dock icon in background** under
Confirmations. It uses the shared setting toggle and persists
`background.hide_dock_icon`; browser settings omit this native control.

The mobile AI Chat composer uses the same device-local keyboard correction preference as the terminal composer; toggling it updates autocorrect, spellcheck and sentence capitalization on the open textarea. Desktop Chat retains its existing input attributes.

Answer highlights in the terminal grid, AnswersPanel, AI Chat markdown and terminal Chat cards use a green left rule without an answer background layer. Paragraph spacing and each content block's own background stay unchanged. User-question styling is separate. Terminal Chat draws its rule on the blocks selected by `answerBlockRanges` and `answerExtent`, never on the final-message wrapper; desktop and mobile use the same selection.

The shared `--answer-rule` token preserves the dark-theme rule and uses `--success` in Paper to keep the rule visible on the light page. Terminal Chat cards already use `--success`.

Answer overlay caching includes cell height and projected row positions as well as marker content, so font loading and theme remeasurement cannot retain old rule geometry.

Tasks list controls use provider-declared field sort, group and filter capabilities.
A rejected selection restores the last successful query and shows the provider reason.

Tasks small-list mode (#1851-7ade): when the provider reports at most 1,000 matches, Tasks collects the complete search/preset scope and uses the Rust projection for all-field sort, group and filter. A changed total, missing rows or a nonterminal cursor cannot establish completeness. Unknown totals and larger lists keep provider-supported server controls. No extra cache or persisted state is added.

GitHub Tasks boards (#1846-55d7): the optional provider `listBoards(signal)` port discovers the current user Projects v2 when Board opens. The existing picker selects a project; paging preserves project identities. Missing `read:project` stays visible instead of claiming there are no boards. Providers without discovery retain their configured boards. Discovery does not persist selections or write to GitHub.

TasksDetail provides Follow task / Unfollow task in its toolbar. Settings → Notifications has a Tasks section with an independent default-off OS notice preference.

FileBrowserPanel subscribes to successful folder-drop completion and routes it through its existing mutation refresh. The subscription is scoped to the filesystem root and owning connection, and is removed when the panel unmounts.

### Settings presentation vocabulary

`SettingsPrimitives.tsx` provides SettingPageHeader, SectionCard, SettingRow, StatusSummary, ListRow, EmptyState and DangerZone. These components render caller-owned content only. SettingRow reuses SettingField label and descriptive IDs, including field errors; controls remain the existing UI Input/Select/Textarea components. SectionCard and DangerZone titles are understood by the static Settings search extractor and render h3 anchors. Existing field consumers retain their stacked layout unless they opt into row presentation.

Settings integration keeps `SettingsPanel` as the page-selection owner. Search catalogue reads do not mount page consumers; plugin `describe` supplies navigation and search while scoped `get` starts only when the editor mounts. Search reveals expert keys before the next-frame scroll without persisting Expert Mode. Section links preserve retired-tab mappings, ignore missing anchors and cancel scheduled work on close. `pluginSettingsTab.test.tsx`, `settingsPanelSearch.test.tsx`, `settingsPanelExpert.test.tsx` and `DeepLinkSection.test.tsx` cover these shared boundaries, including local loading/errors and removal during pending plugin reads. No global save transaction or status inference belongs in the shell.

The Pocket Voice picker includes installed and downloadable catalogue voices for the active language. A missing configured voice stays selected with a Not Downloaded label and an adjacent Download row.

Terminal ego provider recovery: the tab replaces its busy animation with a
recovering label. StatusBar mounts ProviderRecoveryStatus only while a complete
backend recovery snapshot exists; its one-second display timer stops on unmount.
The component displays phase, attempt, next-time and a zero-clamped countdown,
with a single-flight Force retry button using the shared backend retry transaction.
A non-empty composer refuses the retry; the status bar displays the error.

Voice correction controls carry `corrections.replacements.{from}.from` and
`corrections.replacements.{from}.to` catalogue identities. Import, add and remove
retain the existing TextCorrector workflow; export does not change settings.

Desktop `TerminalChatView` renders active native questions from the existing session lifecycle store below the shared transcript. `ChoicePromptOverlay` has an embedded layout; desktop and mobile share `sendNativeChoice` for agent-specific PTY keys. Compose drafts and ACP interaction state are not involved.

Mobile offers the **Chat** tab only when the host has a configured ego executable, using the same availability predicate as desktop. Host settings changes update the tab without reloading. Removing ego configuration returns an open Chat view to **Sessions**; unavailable saved Chat links also stay on **Sessions**.

Abandoned-shell markers in desktop sidebar session rows and mobile SessionCard render the same backend `state.abandoned_since_ms` timestamp. A missing field retracts the marker; the frontend does not infer abandonment from silence.

### Context menu viewport bounds

Context menus use their unscaled layout size to stay within an 8px viewport margin.
Oversized menus scroll; their minimum width also shrinks in narrow windows.
Shell workspace rows do not offer branch workflow actions or Git smart prompts.

Shell-folder rows offer the same available-agent menu as Git workspace rows.
Launching an agent uses the folder as its working directory, without creating a
branch or worktree. The Add Terminal button also offers agents on a long press.

Detached worktree inventory is rendered by `RepoSection` as informational rows, outside the selectable branch-workspace list. The header retains a detached count while folded; no branch or terminal actions are attached to detached rows.

FileBrowserPanel and TreeNode render the backend link target and target kind with a link icon instead of a file-size badge. Delete refreshes the listing after success and shows a persistent error notification after failure. Navigation stays unchanged. The code editor shows the backend target-specific refusal when an outside-repository link is opened.

### Mobile SessionUsageBlock

`SessionUsageBlock` is mounted inside the session overflow. It loads
`get_session_usage` on mount/session change, invalidates pending responses on
cleanup, and renders backend-owned lines without usage computation. It has
explicit loading, unavailable, unknown-model and stale presentation.

Sidebar worktree diagnostics render recorded paths and repair guidance without buttons, tab focus or branch-selection handlers. The repository header retains a diagnostic count while folded. These transient inventory rows are separate from branch workspaces and are not saved in repository configuration.

### StoryNotes (native story detail)

`StoryNotes` renders the selected story's append-only notes and workflow receipt references. `StoriesDialog` mounts it with a project/story key, so changing selection cannot carry a draft or a late append response into another story. It calls the shared story service for `list_notes` and `add_note`; the backend owns validation, author/time/revision provenance, receipt ownership and append order. A failed append keeps the exact draft. A confirmed append clears it and reloads persisted notes. **Refresh notes** loads additions from MCP or another window; there is no polling or alternate state store.

Stories dialog actions use the shared Button geometry. Plan, story and run list rows and Kanban cards retain native layout buttons with explicit row/card markers.

`ui/ColorSwatch` is the shared native button for immediate preset/clear color selection.
It keeps selection in the owner through `selected`/`aria-pressed`, defaults to
`type="button"`, forwards native refs/events/disabled, and uses a 36px circular
target with a 44px touch minimum. Set `swatchColor` for a preset; omit it for
the dashed clear target. Give each target an accessible name. Feature CSS may
arrange the wrapping picker but must not resize its swatches.

Settings shell and shared Settings actions use `ui/Button`; native navigation
and search-result rows retain their layout marker and click semantics. Settings
search lays out the icon, shared Input and clear Button as siblings. Select
arrow clearance belongs to FormControls, not to Settings selectors.
`SettingPageHeader`, `SectionCard`, `StatusSummary`, `ListRow`, `EmptyState`
and `DangerZone` action slots arrange their children only. Pass shared UI
controls into these slots; the containers do not set descendant button sizes.
`SettingButton`, account add/retry actions and list add/remove actions also
use `ui/Button`; field CSS owns their width and row layout only.

`Button geometry="stretch"` fills a flex/grid cross-axis container with square
edges and at least 36px desktop or 44px touch targets. Use it for edge actions
such as swipe actions and tab scroll arrows; the container owns placement.

`ButtonGroup` joins direct `Button` children with connected edges and one
divider. Each action retains its native events, disabled state and size. Label
the group with `aria-label` or `aria-labelledby`. Keep dropdown content outside
the direct action children; the group does not clip focus rings or popovers.

`Textarea autoGrow` is a compact content-sized editor: it starts at the shared
36px desktop or 44px touch target and grows to three control heights before
scrolling. It recalculates after native input, controlled value updates and
width changes. Remove caller height caps and auto-resize handlers when adopting
it; the shared CSS is the single owner of the bounds. Default Textarea keeps
its existing three-control-height minimum and manual vertical resize.

Shared fields style native `aria-invalid="true"` with the error border token
without replacing the focus outline. `Button variant="removalToggle"` supports
installed/removal toggles via native `aria-pressed`; see the Style Guide for
state and hover semantics. Consumers retain validation messages and callbacks.

Agent settings use shared Button, Switch and Textarea controls, including launch
module and launch-addition editors. Validation fields expose native invalid and
error descriptions; the installed MCP action uses the shared removal toggle.
Agent feature CSS retains only layout, width and text presentation.

GitHub settings actions use shared Button variants for secondary, primary and
danger actions. Feature CSS owns only action-row layout, not button geometry.

Shared Button supports opt-in `shape="icon"`, with required `aria-label` and
optional `circular` presentation. Its native square target stays 36/44px even
with compact size. Circular presentation does not remove the corner hit region.

Plugin settings actions use shared Button variants and a circular icon-only
documentation target. Log visibility uses native aria-pressed; install suggestions
use the same shared Button as their work-item setup consumer. Feature CSS owns
action layout only.

Repository settings use shared Button for team export and shared Switch for
worktree consolidation. The switch retains its native checkbox, accessible row
label, and repository-owned update callback; feature CSS only aligns the control.

Smart Prompt editor variable triggers and reset actions use shared Button.
Variable choices remain native list-row buttons with name and description;
`data-control-layout="row"` makes their intentional list geometry explicit.

Telegram settings inherit target sizing and touch typography from shared Button,
Input, and Switch controls. Feature styles only format the chat-list heading.

Keyboard Shortcuts uses shared Button for the built-in Rebind actions and the
global-hotkey Clear action. Workspace styles retain binding-column layout only.
