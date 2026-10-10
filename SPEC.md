# TUICommander Specification

Agent capture tooling: `scripts/agent_capture/run.py` records real installed CLIs through isolated headless sessions, promotes reviewed captures with ordered expected states, and integrates with the 1342 replay oracle. Missing or unauthenticated CLIs remain unverified.


Published story workflows can start in Plans and Stories through the owning daemon. Run history renders graph positions, decisions, evidence and paged event payloads, with explicit graph recovery and cancellation. IPC/HTTP and the generated `workflow_run` MCP schema share the native run service. Plan start remains visibly unavailable pending plan dispatch.

**Version:** 1.8.0
**Last Updated:** 2026-09-16

## ego Perimeter Settings — Implemented (#1401-ab1c)

Settings > AI Chat edits user/profile `roots` and `network` through ego CLI,
with IPC/HTTP parity. Roots add permission reach beside the configured AI Chat
workspace. The effective preview comes from `ego config ls --effective --json`
in that workspace/profile. An explicit empty roots declaration stays empty.
Capability evidence is never inferred from the OS or selected backend; the
current ego `not_checked` value is shown as **Enforcement not checked** (approved
by the coordinator on 2026-10-06). The ego297 measured-array contract is supported: all selected exec guarantees
produce an OS badge; partial/empty arrays produce Prompt only with explanation.
Unknown evidence remains unverified, and off+online never passes vacuously. Existing ACP sessions retain their admitted state.

## Overview

TUICommander is a multi-agent terminal orchestrator designed to manage supported AI coding agents, including Claude Code, Gemini CLI, OpenCode, Aider, and Codex, in parallel. It provides per-pane zoom, git worktree isolation, and GitHub integration.

## Goals

1. **Parallel Agent Orchestration** - Run 50+ coding agents simultaneously
2. **Git Worktree Isolation** - Each task gets its own isolated workspace
3. **Per-Pane Font Control** - Independent zoom levels for each terminal
4. **Rate Limit Resilience** - Detection and countdown display when agents hit rate limits
5. **Productivity Features** - Prompt library, keyboard shortcuts, IDE integration

## Architecture

### Technology Stack

| Layer | Technology | Purpose |
|-------|------------|---------|
| Frontend | SolidJS | Reactive UI with fine-grained reactivity |
| Terminal | alacritty_terminal | Native VT engine with canvas rendering |
| Backend | Rust + Tauri | Native PTY management, file system access |
| Build | Vite | Fast HMR development, optimized production builds with automatic code splitting and deferred loading |

### Terminal character integrity

The Alacritty cell is authoritative: its base scalar and retained zero-width
scalars must survive text extraction, grid transport, row caches, rendering,
selection and copy. Search uses the exact stored codepoint sequence, without
canonical normalization. Grid coordinates stay cell-based; string offsets
exposed to JavaScript use UTF-16. An accent arriving in a later PTY chunk must
damage its base cell. Intentional terminal erasure/overwrite, history eviction,
and the existing nine-zero-width-scalars-per-cell bound remain unchanged.
The sparse wire extension is specified in
[Binary Frame Format](docs/frontend/canvas-terminal-audit.md#binary-frame-format).

### Terminal selection during output

Selection endpoints identify retained text rows, including while new output
arrives and the history buffer rotates. A stationary history viewport must not
move its highlight onto different text. Evicted rows must not alias newly
retained rows. Selection overlays and text painting must use the same accepted
viewport; partial frames from a different viewport cannot be merged into its
row cache. These guarantees apply both during a drag and after mouse release.
Intentional ANSI edits, history eviction and resize/reflow remain distinct from
renderer data loss.

### Backend Execution Model

All Tauri commands that perform I/O (git subprocesses, network, bcrypt) are `async` and run inside `tokio::task::spawn_blocking` to avoid blocking Tokio worker threads. Git data is cached with a 60s TTL, invalidated immediately by `repo_watcher` on file system changes. PTY output is serialized once and reused for both Tauri IPC and event bus broadcast. Frontend coalesces paint triggers via `requestAnimationFrame` (~60 repaints/sec) to reduce canvas render passes during burst output.

### Why SolidJS?

- Fine-grained reactivity without virtual DOM
- Direct DOM manipulation for terminal performance
- Compile-time optimizations
- Smaller bundle size than React/Vue
- Familiar JSX syntax

### Frontend Refactoring Workstream

The SolidJS frontend is being refactored incrementally to improve module ownership,
test isolation, and deferred loading without changing the framework or product
behavior. The measured architecture map, dependency constraints, sequencing, and
validation contract are maintained in
[`docs/frontend/solid-refactoring-plan.md`](docs/frontend/solid-refactoring-plan.md).

The work preserves browser/Tauri transport parity and keeps terminal frame and
paint scheduling imperative. Structural changes must remain independently tested
and revertible; line-count reduction alone is not a goal.

### Component Architecture

```
App
├── Sidebar
│   ├── Repository List
│   └── Terminal List
├── TabBar
│   └── Terminal Tabs
├── Terminal Container
│   ├── Terminal (CanvasTerminal)
│   ├── MarkdownPanel
│   └── IdeasPanel
├── GitPanel (side panel)
├── StatusBar
├── PromptOverlay
└── PromptDrawer
```

## State Management

### Stores (SolidJS createStore)

#### terminalsStore
Manages terminal instances and their state.

```typescript
interface TerminalState {
  terminals: Record<string, TerminalData>;
  activeId: string | null;
  nextId: number;
}

interface TerminalData {
  id: string;
  sessionId: string | null;
  fontSize: number;
  name: string;
  awaitingInput: AwaitingInputType;
}

type AwaitingInputType = "question" | "error" | null;
```

### Authoritative agent wait state

Agent lifecycle state is owned by one per-session backend state machine. Every
input turn has a monotonically increasing `turn_epoch`; asynchronous parser and
timer events are applied only to the epoch that produced them. A wait carries
its source and confidence, and every SET has an explicit CLEAR path (submitted
input, choice resolution/disappearance, protocol busy/idle, interruption, or
PTY exit). Terminal scrollback is evidence only for the current chat turn: a
question retained above a later response or completion must never re-arm
`awaiting_input`.

Desktop IPC, HTTP/PWA, WebSocket, MCP, and orchestrator injection use the same
post-input bookkeeping. Frontends render the backend snapshot; parsed terminal
events may trigger one-shot effects but are not a second state authority.
PTY lifecycle mutations reach that snapshot through a lossless ordered lane;
the broadcast event bus is reserved for reconnectable live consumers and cannot
be the sole carrier of sticky SET/CLEAR state. Before any lifecycle evidence,
the shell state is absent and a detected agent remains `starting`.

MCP managed-agent commands use one `session action=submit` request. It claims a
confirmed-idle empty composer, never queues, serializes the complete raw-mode
payload through Enter, advances this same input FSM and epoch, and waits
internally for bounded child terminal movement. The receipt distinguishes
complete, not-started, and uncertain writes; only a provably not-started write
is retry-safe. Terminal movement is acknowledgement evidence, not semantic
application acceptance. `session action=input` remains raw and write-only.

#### repositoriesStore
Manages the list of git repositories.

```typescript
interface Repository {
  path: string;
  displayName: string;
}
```

#### settingsStore
User preferences with localStorage persistence.

```typescript
interface SettingsState {
  ide: IdeType;
  fontFamily: FontType;
  defaultFontSize: number;
}
```

#### promptLibraryStore
Saved prompts with variable substitution.

```typescript
interface SavedPrompt {
  id: string;
  name: string;
  content: string;
  description?: string;
  shortcut?: string;
  category: PromptCategory;
  isFavorite: boolean;
  variables?: PromptVariable[];
  lastUsed?: number;
  useCount: number;
}

interface PromptVariable {
  name: string;
  description?: string;
  defaultValue?: string;
}
```

#### rateLimitStore
Tracks rate limit status per session.

```typescript
interface RateLimitInfo {
  sessionId: string;
  agentType: AgentType;
  detectedAt: number;
  retryAfterMs: number | null;
}
```

## Hooks

Key hooks in `src/hooks/`:

- **usePty** — PTY session lifecycle (spawn, write, resize, close, subscribe to data/exit events)
- **useRepository** — Git operations (getInfo, getDiff, getDiffStats, openInApp with line/col, renameBranch)
- **useGitHub** — Reactive wrapper over `githubStore`; returns `{ status, loading, error, refresh, startPolling, stopPolling }`
- **useKeyboardRedirect** — Redirects keyboard input from non-terminal areas to active terminal
- **useFileDrop** — External file drag & drop handling

See `src/hooks/` for full signatures — the above is a representative summary.

## Agent Types

```typescript
type AgentType = "claude" | "gemini" | "opencode" | "aider" | "codex" | "amp" | "cursor" | "goose" | "grok" | "droid" | "pi" | "git" | "api";
```

Full agent configuration (binary, resume command, session discovery, detection patterns) lives in `src/agents.ts`.

### PTY versus ACP routing

Two transports carry an assistant, and a session belongs to exactly one of them.
There is no hybrid route and no fallback between them.

- **PTY.** Every member of `AgentType` above, and a separately launched terminal
  `ego` CLI when its PTY integration is enabled. TUICommander allocates a terminal,
  runs the CLI executable, and infers state by parsing the rendered rows into
  `ParsedEvent`. Session state is recovered from the agent's own session files
  on disk (see AGENTS.md, "Agent Session Management").
- **ACP.** AI Chat's `ego`, through the Agent Client Protocol v1 client in
  `src-tauri/src/acp/`. TUICommander launches `ego acp -C <root>` directly,
  adding `--profile <name>` only when a user selected an ego profile, and
  owns its stdio JSON-RPC connection. A workspace's `.tuic.json` `ego_profile`
  selects a session profile bounded by the explicit machine profile via ACP
  `_meta.ego.ceilingProfile`; ego owns the clamp and TUIC displays its warnings. No terminal is allocated, no shell is
  invoked, and no output is scraped. The host issues a durable `TUIC_SESSION`
  peer UUID for the repository conversation, persists it beside the selected
  conversation binding, and passes it to ego and its MCP bridge. Mail and child
  parentage use that peer identity without a PTY wake.

Standalone `ego` outside TUIC is a third face: it has neither a TUIC peer
identity nor a TUIC transport. A terminal `ego` and an ACP-hosted `ego` are
separate processes and sessions; neither falls back to the other's transport.

The ACP-hosted `ego` is deliberately **not** an `AgentType`. `AgentType` describes a CLI
executable, its launch arguments and its parser behaviour; it carries no
negotiated protocol version, connection lifetime, capability snapshot, reverse
request, or durable ACP session ID. Adding `ego` to it would make the ACP
process look like a terminal and would recreate exactly the hybrid this rule
forbids. ACP data is already structured and must never pass through the terminal
parser or be projected into `ParsedEvent`.

When the ACP connection fails, it settles as a failure. It does not degrade to a
PTY session, to reading ego's files, or to terminal input. There is no runtime
feature flag selecting between the two paths.

**This section is the contract.** It is final rather than exploratory, and it is
self-contained on purpose: an earlier version delegated its authority to a plan
file that a plan sweep later replaced with a status stub, which left the rule
looking unsubstantiated. Nothing outside this section defines the routing.

An early proof of concept did propose a PTY fallback behind a feature gate. Both
were rejected, and the rule above states the rejection directly. Do not restore
them from an archived plan or an idea file.

## Rate Limit Detection

Provider-specific patterns detect rate limits in terminal output:

### Provider usage capabilities

Account usage is capability-based, not assumed to be uniform across agents:

- **Claude:** account limits plus local transcript analytics.
- **Codex:** account limits and token history come through the documented Codex
  App Server JSON-RPC API. TUICommander does not read Codex OAuth credentials or
  call private ChatGPT backend routes.
- **Grok:** account billing comes through a short-lived `grok agent stdio` ACP
  connection and its `_x.ai/billing` extension. This telemetry-only connection
  does not change Grok's PTY session routing and is not a PTY/ACP fallback.
- **Gemini:** no stable machine-readable account quota interface is available.
  TUICommander keeps terminal rate-limit detection and does not fabricate an
  account dashboard from session-local `/stats` output.

An unavailable capability stays unavailable. Authentication, protocol, network,
and schema failures must never be converted into zero usage or full headroom.

### Claude Code
- `rate limit`
- `API rate limit`
- `overloaded`
- `try again later`
- Retry-after extraction from error messages

### Gemini CLI
- `429`
- `quota exceeded`
- `rate limit exceeded`
- `resource exhausted`

### Generic Patterns
- `too many requests`
- `rate limited`
- `slow down`
- `retry after`

## Output Parser

JSONL event parser for structured agent output:

```typescript
type OutputEventType = "result" | "assistant" | "error" | "tool" | "system" | "unknown";

interface OutputEvent {
  type: OutputEventType;
  content: string;
  timestamp: number;
  raw?: unknown;
}
```

Features:
- Streaming parser with line buffering
- 100KB buffer limit to prevent memory bloat
- Handles partial lines across chunks

## Keyboard Shortcuts

### Global
| Shortcut | Action |
|----------|--------|
| Cmd+T | New terminal |
| Cmd+W | Close terminal |
| Cmd+K | Open prompt library |
| Cmd+Shift+D | Toggle Git Panel |
| Cmd+G | Git Panel — Branches tab |
| Cmd+M | Toggle markdown panel |
| Cmd+Alt+N | Toggle Ideas panel |
| Cmd+O | Open file… |
| Cmd+N | New file… |
| Cmd+1-9 | Switch to tab N |
| Cmd++/- | Zoom in/out in the active terminal, Markdown tab or code editor |
| Cmd+0 | Reset zoom in the active terminal, Markdown tab or code editor |
| Cmd+F | Find in terminal |
| Cmd+E | Toggle file browser |
| Cmd+[ | Toggle sidebar |
| Cmd+? | Toggle help panel |
| Cmd+Shift+[ | Previous tab |
| Cmd+Shift+] | Next tab |
| Cmd+Shift+T | Reopen closed tab |
| Cmd+P | Command palette |
| Cmd+Shift+A | Activity dashboard |
| Cmd+, | Settings |

### Prompt Library
| Shortcut | Action |
|----------|--------|
| ↑/↓ | Navigate prompts |
| Enter | Insert prompt |
| Ctrl+N | New prompt |
| Ctrl+E | Edit selected |
| Ctrl+F | Toggle favorite |
| Esc | Close drawer |

## Workspaces: linked worktrees with warm artifacts

MCP callers can declare an existing linked worktree through
`session action=declare_worktree worktree_path=/absolute/path`. Ownership comes
from the immutable PTY launch directory and registered Git worktrees. The
caller-only association persists under stable `TUIC_SESSION`, survives restart
and does not change shell cwd or acquire cleanup ownership. MCP creation
without a session spawn also places only the same-repository creator.


Every workspace created by TUICommander is a linked Git worktree. Its refs and
objects are shared with the parent repository, so commits are visible from the
parent immediately. Git permits a branch to be checked out in only one
worktree; creation refuses a branch that already has a checkout.

`workspace_id` is explicit on every API and currently equals the checked-out
branch. Callers must use the returned id rather than derive it from display
data. Removal, lifecycle inspection, merge, and finalization all address the
same worktree by that id.

Creation starts from a clean checkout. Parent tracked or untracked changes are
not carried into the workspace. Reintroducing tracked changes would be a linked
worktree operation: pipe `git diff HEAD` in the parent into `git apply` in the
new worktree.

After `git worktree add`, TUICommander warms Git-ignored directories such as
`node_modules`, `target`, and `.venv` with copy-on-write filesystem copies.
Only selected ignored build directories are copied; tracked files remain Git's responsibility.
The capability probe and copy primitive share the same clonefile/reflink flags;
unsupported filesystems produce one cold-worktree warning rather than one per
directory. Warming is best-effort and never invalidates an otherwise complete
worktree. After cloning, owner write permission is restored on the copies in
the new worktree, without following symlinks or changing the source checkout.

Lifecycle state is one backend verdict keyed by workspace id: working-tree
dirtiness, whether `HEAD` is merged into the default branch, and removal safety.
A missing registered checkout has no dirty fingerprint; its preflight identifies
the missing directory and requires explicit force confirmation. Cleanup preserves
its submodule refs before pruning and checks separately before overriding a lock.
Merged GitHub PR state can also prove a squash-merged branch safe when its local
tip is contained in the PR head; ancestry in the checked-out integration branch
is sufficient even when the remote default branch has not advanced. The MCP
`repo branch_delete` action applies the same proof to a local branch with no
worktree, and refuses current, default, checked-out, or unmerged branches.
Any inspection failure is `Unknown` and cannot authorize removal. Destructive
UI obtains a fresh verdict, and deletion repeats the safety checks so a stale
confirmation cannot authorize changed state.

For detached orphan worktrees, the Ask dialog automatically removes after a
configurable countdown only when every checkout is clean (including untracked
files) and its HEAD is reachable from a branch. Otherwise it lists the unsafe
reasons without a countdown. Keep and Escape cancel. An agent can answer the
open dialog through MCP; a remove answer and the final removal repeat the
backend safety check.

## Project Progress

Native plan and story records have a separate config-directory SQLite authority (`stories.sqlite3`). Progress remains a human-readable journal and does not determine story status. Manual story actions use the shared Rust service across IPC, HTTP, MCP, and CLI. The desktop and browser UI exposes plan and story lists, criteria, dependencies, and manual transitions. For manual plans, WontFix is terminal for plan aggregation but does not satisfy a dependency or promote a dependent; a nonempty plan with only Done/WontFix stories is Done, while an empty plan is Draft. Any trusted caller can remove a direct WontFix dependency only from a Backlog story with a current revision. In plans with no workflow run, a dependent becomes Ready when every remaining dependency is Done; workflow-owned plans additionally require current integration receipts. An explicitly Blocked story is never auto-unblocked. Rust derives direct and transitive abandoned dependency indicators and the WontFix count on `plan_view` reads; the UI renders them. Any trusted caller may approve after checking the acceptance criteria. Actor identity is tracking only and never restricts story or workflow actions. Desktop IPC and credential-authenticated HTTP record `human`, sessionless local HTTP records `local_api`, and managed transitions record the acting session. Local token exchange is accepted. The optional workflow engine described in `plans/native-story-workflows.md` remains in development; import/export is excluded.

Workflow definitions have their own config-directory database (`workflows.sqlite3`): editable drafts and immutable published revisions. Graph validation gates publication; the seeded `Resolve plan` definition pins a published `Story delivery` revision. The existing `human` closure policy requires an explicit approval transition, which may come from any trusted caller; `automatic` closure cannot be published until a TUIC-owned evidence gate exists. Definitions pin bounded direct-executable check commands. TUIC runs independent checks outside the global receipt-service lock. Completion probes the assigned worktree and Git artifact before the receipt transaction, then revalidates persisted run sequence, story revisions and command identity inside it; unrelated run events do not discard a valid result, and retries keep their original request cursor. TUIC records durable commit/tree-bound check receipts, then verifies an operator-created merge on the canonical branch against the clean tree computed by Git from its exact parents and runs the checks again on its result before recording `StoryIntegrated`. Extra or omitted merge content is rejected. A conflicted merge requires explicit human review or a separately verified artifact; the current receipt path cannot certify it. A later canonical commit requires a TUIC-computed `CanonicalRecertified` event with successful checks at the clean new tip and ancestry of every integrated source. Durable run records and event projections exist in `workflow_runs.sqlite3`, with idempotent actions and recovery of uncertain effects after restart. Runtime reconciliation preserves live attempts and in-flight effects; restart-only recovery logs each failed run and continues recovering healthy runs. Explicit story dispatch enforces bounded concurrency and conservative file-scope exclusion, while a managed coordinator assigns registered isolated worktrees before spawn. Dependents and final run completion require a current integration receipt at each accepted story revision. The Plans and Stories dialog reads a plan's run list and ordered event timeline. The owning daemon executes the published serial story and plan graphs; public graph start controls remain in development.

Progress is one append-only journal per project, read from a dialog. The dialog
opens on the active PTY and can switch to another PTY or the repository aggregate.
On mobile, where there is no desktop active repository, the journal supplies a
newest-first project list; selecting one loads that project's entries.
The toolbar bell entry stays visible with zero unread updates; the command palette
and `Cmd/Ctrl+Shift+P` open the same dialog.
It answers "what happened while I was not watching?" and nothing else.

Five entry kinds. Agents report `done` and `blocked` through the compact MCP
`progress` tool, whose `initialize` obligation is imperative rather than
descriptive — the shipped descriptive version produced zero entries across 39
repositories. TUICommander itself writes `intent` from the agent's `intent:`
marker; that trigger fires on every task, so it is the reliability floor under an
obligation the agent read hours earlier. An agent cannot report an `intent`: the
kind exists because it is observed rather than claimed. For the same reason
TUICommander writes `delegated` at `agent action=spawn` and `message` at an
explicit `agent action=send` between terminals, attributed to the sender and
naming the target terminal; their text is redacted and capped at 500
characters, and that cap is the whole record. An agent cannot report either.

The journal is append-only. There is no pause, clear, correction, revision,
deduplication or Markdown export — an entry is written once and either kept or
deleted. Agents keep exactly one read action on `repo`, `progress_list`;
everything else a reader might want costs instruction budget in every
`initialize` and belongs to the reader instead.

Storage is one SQLite database in the configuration directory with the project
and a nullable PTY ID as columns. Existing entries remain unattributed; direct
local reports with no terminal binding do too. Nothing is written inside a repository, so Progress cannot produce a
repository change event, a Git-exclude entry or an indexing pass. Managed
workspaces resolve to their parent project, so a worktree and its repository
share one history. A directory belonging to no registered project is not
recorded against the focused repository; it is not recorded at all.

Collection is gated by `progress_tracking`: a global setting ANDed with an
optional per-agent override. Global off also removes the tool from every agent's
tool list.

The UI is a dialog, not a panel: one newest-first list for the active PTY, with a
selector for other PTYs and the repository aggregate, a
per-scope last-visit divider frozen while that view is open, blocked entries in red,
`intent` entries muted, one blocked-only filter and per-entry deletion. No pages,
no per-repository fan-out.

Boss decided on 2026-09-23 to evolve Progress into a structured delegation
view. A **List | Flow** toggle draws the same journal as a sequence diagram: one
column per terminal and per Claude subagent of an open terminal, order rather
than time downward, delegations and messages as arrows, a child's
`done`/`blocked` as its return arrow to the parent, and intents as notes. The
backend builds the whole sequence (`progress_flow`); a subagent's full prompt or
report is fetched on demand (`progress_flow_detail`), redacted. Hand-offs are
journaled rather than read from `session_parent` or the agent inbox, because
both are in memory and lost when the child closes or the app restarts. The toolbar bell carries one aggregate entry
that opens it.

The implementation contract is maintained in `plans/project-progress.md`. The
reporting-quality evaluation of the short default against the optional prompt is
recorded in [Progress reporting evaluation](docs/evaluations/progress-reporting.md).

Periodic inference, generated summaries, issue-tracker and
remote-synchronization integrations, scheduled or manual Markdown export, and
Markdown import stay outside this version by decision, not by omission. The
delegation structure the Flow view draws is observed from spawns and sends; it
is not inferred, and nothing groups entries into workstreams.

## Native plans and stories

The story service stores plan and story records outside the repository. It discovers
existing Markdown plan documents from the project's top-level `plans/` and
`.claude/plans/` directories on each `list_plan_sources` call. The Plans and Stories
dialog uses this action through the same story service as MCP, HTTP, IPC, and CLI.
Selecting a document calls `add_plan_source`, which derives the title from
front matter or the first Markdown heading and reuses an existing record for
the same source. Nested archives are not offered. A path or link can still be
added through the secondary dialog control.

## Settings navigation

The Settings panel groups its global pages by task. Group labels are static,
noncollapsible rows; every page stays one click away and search stays global.

| Group | Pages |
|---|---|
| Application | General, Appearance, Notifications |
| Workspace | Terminal, Keyboard Shortcuts, Git & GitHub |
| AI | Agents, AI Chat, Voice, Smart Prompts |
| Integrations | MCP, Remote Access, Remote Machines, Plugins |
| Repositories | One direct entry per configured repository |

- `MCP` owns HTTP/MCP server status, bridge configuration, native tool
  controls, and upstream servers. `Remote Access` owns enablement,
  authentication, network settings, Tailscale, the QR/connect URL, and the
  relay. `Remote Machines` owns connections to other TUIC hosts. These three
  pages replace the former "Services & MCP" tab.
- `General` also holds the TUIC CLI, Code Intelligence (MDKB), the ego
  executable, the default IDE and custom launchers. These are general
  configuration, so they have no separate page. The ego executable is shown
  also while Experimental Features is off.
- `AI Chat` holds ego's providers and default model. It
  replaces the former "AI Providers" tab and is hidden while Experimental
  Features is off. `Voice` is the former "Dictation" page.
- The reorganization moves controls only. Persisted config keys and Tauri/HTTP
  contracts are unchanged.

**Expert mode.** An expert setting is a stable setting whose default is correct
for almost everyone. In basic mode it is hidden while its value equals the
config default; it is shown in expert mode, when modified, when a search result
revealed it during the current Settings open, and whenever the default is
unknown. Hiding a user's override is the one failure this rule must never
produce. The switch persists as the UI pref `settings_expert_mode`; the
defaults come from the read-only `get_config_defaults` command. Expert mode is
not Experimental Features: that flag gates unstable *features*, expert mode
gates the *visibility of stable settings*. Which controls are expert is Boss's
classification of 2026-09-24 (the Step 6 table of
`plans/settings-navigation-reorganization.md`, listed per page in
`docs/user-guide/settings.md` → Expert Mode). A page is never all expert: in
basic mode it would render an empty page.

## Persistence

Repository state is persisted by the Rust backend in `repositories.json`, in
the single platform config directory shared by debug and release builds,
written through the locked/atomic `ConfigFile` path.

At bootstrap, `tuic-remote` may select one immutable named application instance
with `--instance <id>`. Omitting it preserves the existing platform config path,
keyring tuple (`tuicommander`/`vault`), and migrations. A named ID is a lowercase
ASCII DNS label of 1–63 characters with alphanumeric ends and internal hyphens;
`default` is reserved. Its files live below
`<platform-app-config>/instances/<id>/` and its vault uses
`tuicommander-instance-<id>`/`vault`. Named state never falls back to or migrates
default/legacy state. Selection occurs before `--set-password` or persistence,
and an invalid ID or unavailable named release vault terminates before network
bind. Runtime switching is unsupported.

Production black-box consumers pin and verify the digest of the exact
`tuic-remote` artifact they launch. The daemon also reports its running build's
version, target triple and SHA-256 in `/health`; the desktop uses that identity
to detect an outdated remote and to verify an update after restart.

Ordinary `config.json` and `mcp-upstreams.json` mutations use delta-under-lock
semantics: after taking the cross-process file lock, the backend reloads the
latest document and applies only the caller's changed fields or server-ID
operations. Independent saves from concurrent debug and release instances do
not overwrite one another's unrelated changes.

Some frontend-only stores persist to localStorage:

| Key | Store | Content |
|-----|-------|---------|
| `tui-commander-settings` | settingsStore | IDE, font, preferences |
| `tui-commander-prompt-library` | promptLibraryStore | Saved prompts |

## Feature Status

### Completed (P1)
- [x] ACP client for ego — backend complete (connections, sessions, turns, the
      agent's questions back, ego's pause/resume/compact), reachable identically
      from Tauri IPC and HTTP; no frontend surface yet
- [x] Multi-agent support through the canonical `AgentType` registry
- [x] Git worktree management per task
- [x] Linked workspaces with best-effort copy-on-write warming of ignored directories
- [x] Agent spawning integration
- [x] SolidJS migration

### Completed (P2)
- [x] Design Mode opens a dedicated Chrome window per repository and pre-fills selected element context into the bound agent terminal without submitting the draft; local per-repository URL, status events, HTTP parity and bounded/redacted payloads
- [x] Split pane layout
- [x] Touch branch action trays: left swipe reveals existing actions; vertical scrolling and desktop hover controls remain unchanged
- [x] Multi-repository sidebar
- [x] Rich parent-agent links reveal and select live children in other repositories without moving sessions
- [x] Git diff panel
- [x] Interactive agent prompts UI
- [x] IDE launcher dropdown
- [x] GitHub integration
- [x] Sidebar PR badges retain `#number` while showing lifecycle, conflict, CI, and review state
- [x] Parallel agent orchestration
- [x] Orchestrated PTY task descriptions with prompt-derived fallback metadata
- [x] Page retained MCP terminal output through text/byte offsets, with continuation instructions and eviction-gap reporting
- [x] One-call MCP managed-agent submission with bounded terminal-movement receipt
- [x] Expandable terminal Context bar for agent intent, orchestrator assignment, and last user prompt
- [x] Recover captured intent and substantial prompt through session snapshot catch-up and live state reconciliation
- [x] Font selection setting
- [x] Tab bar with keyboard navigation
- [x] Density modes for readability
- [x] Terminal Find routes to the visible CLI buffer or Chat transcript; Chat supports next/previous selected matches and switching views closes search and clears highlights (#1633-f2cc).
- [x] Terminal Chat uses focused, docked Compose input with existing send/queue semantics; CLI open/pin preferences and shared unsent drafts survive view switches. Permission prompts remain in CLI.
- [x] Terminal selection copy unwraps soft-wrapped rows, removes coherent Claude visual gutters and composer margins, and preserves literal block characters, pasted prompt glyphs, and short typed line breaks
- [x] Status bar with branch and PR info
- [x] Rate limit detection
- [x] JSONL output parsing
- [x] Prompt library with variables
- [x] Keyboard redirect to terminal
- [x] Ideas panel (formerly Notes) with send-to-terminal and delete actions
- [x] Terminal session persistence across app restarts
- [x] GitHub GraphQL API (replaces gh CLI for PR/CI data)
- [x] Multi-account GitHub: multiple github.com logins + GitHub Enterprise Server (PAT), per-repo account bindings with ambiguity chooser, isolated per-account polling/rate-limits/circuit-breaker (see FEATURES.md 8.14)
- [x] Auto-update via tauri-plugin-updater with progress badge
- [x] Prevent system sleep while agents are working (keepawake)
- [x] Usage limit detection for Claude Code (weekly/session) with status bar badge
- [x] Repository groups with accordion UI (named, colored, collapsible, drag-and-drop)
- [x] HEAD file watcher for branch change detection
- [x] Git status via .git file reads (no subprocess)
- [x] Lazy terminal restore (sessions materialize on branch click, not app startup)
- [x] Windows compatibility (shell escaping, process detection, resolve_cli, IDE detection)
- [x] Repo watcher for automatic panel refresh on `.git/` changes
- [x] Git Panel (4 tabs: Changes with History/Blame sub-panels, Log with canvas commit graph, Stashes, Branches) — replaces Git Operations Panel and DiffPanel
- [x] Shared branch integration proofs: ancestry, patch equivalence, no-op merges, corroborated squash messages, and archived content heuristics; MCP list/single-branch queries and safe UI deletion (#1295-a2ce)
- [x] Branch Panel (4th tab in Git Panel): checkout, create, delete, rename, merge, rebase, push, pull, fetch, prefix folding, inline search, context menu, stale/merged indicators. `Cmd+G` opens directly on Branches tab
- [x] Context menu submenus and "New Group..." via PromptDialog
- [x] Remote OS file drops copy through the existing authenticated daemon connection; bounded streaming, repository-confined staging, atomic publication, recursion confirmation and conflict skipping
- [x] File Browser panel (`Cmd+E`) with content search (`Cmd+Shift+F`, case/regex/whole-word, streaming results)
- [x] CodeMirror code editor
- [x] Modified click on editor links and paths opens the matching browser or TUICommander view
- [x] Editor line wrapping toggle with separate saved defaults for text and code
- [x] Find in terminal (`Cmd+F`)
- [x] Configurable keybindings system
- [x] Command palette (`Cmd+P`)
- [x] Activity dashboard (`Cmd+Shift+A`)
- [x] Park repos feature
- [x] Plugin system (see FEATURES.md section 17), with Plan Tracker and Stories Ticker shipped as one-time-seeded external packages rather than compiled built-ins; binary reads support bounded per-call budgets and panel messages can transfer buffer ownership
- [x] Self-contained SQLite Viewer plugin under `plugins/`: sql.js/WebAssembly browsing, native filtering/pagination, indexes, visual plans, CSV copy, and explicit atomic inline-edit saves, with the engine and database scoped to the viewer iframe lifecycle
- [x] Connected daemon MCP toasts reach the desktop Messages bell with host attribution, original level/sound and safe remote-terminal navigation; no disconnected replay
- [x] Remote access / HTTP server
- [x] SSH-managed remote daemon deployment, idle lifetime, pairing-token vaulting, and systemd/launchd installation
- [x] Mobile terminal Ctrl menu (Ctrl+C, Ctrl+B, Ctrl+D, agent-aware Ctrl+Enter)
- [x] Mobile Companion PWA (searchable sessions, live output, question reply including Codex interactive choices, activity feed)
  - [x] Files tree search, hidden-folder ordering, long-path preview, and full-height wrapped editing
  - [x] Repository-relative Markdown images through an authenticated, repository-confined image route
  - [x] Expandable Progress messages, local Activity time and minute durations, version display, and a server-persisted mobile light theme separate from the desktop theme
- [~] Managed-agent blocked questions alert the mobile PWA through encrypted Web Push when the desktop is away; a phone reply returns through atomic session submission with a receipt (real-phone verification pending)
- [~] Pending ego permissions and form requests alert a subscribed phone when the desktop is away, linking to the matching mobile Chat conversation with a 30-second limit per conversation (real-phone verification pending; ego card notices await a defined wire shape)
- [x] MCP Proxy Hub (aggregate upstream MCP servers via HTTP and stdio, tool namespace prefixing, circuit breaker, hot-reload, OS keyring credentials, tool filtering, session-local Grok compatibility through lazy meta-tools)
- [x] Copy Path in Markdown panel
- [x] Claude Usage Dashboard (native SolidJS component with API polling, session analytics, usage timeline)
- [x] ConfirmDialog component (in-app dark-themed replacement for native OS dialogs)
- [x] Status bar unified agent badge with priority cascade (rate limit > usage API > PTY usage > name)
- [x] Movement-based PTY agent activity detection ("text above the input area moves = active") with explicit-hook precedence, prompt-based Ready screens, Codex presence-based Working policy, Grok activity/composer disambiguation, interrupt confirmation, and confirmed-idle safety gates
- [x] PR lifecycle filtering (CLOSED hidden, MERGED hidden after 5min user activity)
- [x] Notes/Ideas: mark as used, badge count in status bar
- [x] Notes/Ideas: image paste support (Ctrl+V), thumbnails, send absolute paths to terminal
- [x] Inter-Agent Messaging (`messaging` MCP tool: register, list_peers, send, inbox with channel push + polling fallback)
- [x] Smart Prompts (29 built-in AI prompts with context variable resolution, shell/inject/headless-CLI execution, toolbar dropdown, SmartButtonStrip, Command Palette integration). The `api` execution mode runs one unattended ego turn over ACP (#787-ee50): a session with no MCP server, every question refused, the final text routed to the prompt's output target. Its old executor — a direct provider call from TUICommander — went with the embedded engine (#784-0aec) and did not come back
- [x] AI Chat panel (`Cmd+Alt+A`) — ego over ACP (#785-58ca), bound to a repository and ACP session rather than a terminal. TUICommander renders the journal, permissions, elicitation forms, plans and session controls while carrying no LLM client or provider API key of its own. The selected session is saved per root and restored after restart; the picker lists ego's durable sessions by title and activity time (#1071-46c9)
- [x] AI Chat image paste before connection — discover image capability on first paste, and share Finder/text clipboard selection with Ideas and Compose.

- [x] AI Chat image paste — supported images stage removable previews and become ACP image content blocks when the agent advertises image prompts
- [x] AI Chat session details — ACP title updates rename the header and picker; context-window use and reported cost appear in the footer; a session settings dialog labels every select option and the one-row control bar summarizes the model's short name and mode beside named icon actions
- [x] AI Chat transcript and tabs — selectable messages, with message Copy on hover or keyboard focus; sent prompts reconciled with ego's chunked echo; copyable code and tool output; trailing `suggest:` tokens rendered as reply buttons; terminal-shared web and file link handlers; parallel ACP sessions with independent drafts and transcripts; only running tool calls pulse
- [x] AI Chat transcript polish — collapsed tool rows show short names, message Copy keeps its own space, and streaming follows the bottom until the reader scrolls up
- [x] Desktop AI Chat composer actions reuse the terminal composer pin/play icons and button sizes; tooltips and accessible names distinguish Send, Queue and parked drafts.

- [x] AI Chat composer polish — text grows to a bounded height and pastes over 200 words stay compact until the full text is sent
- [x] AI Chat prompt parking — Ctrl+S or the composer control parks text and images per chat tab, swaps or restores them, and returns the parked draft after the next Send
- [~] AI Agent loop (ReAct) — shipped, then deleted in #784-0aec with no TUICommander-side successor. ego runs its own tool loop and reaches terminals from outside, through the `session` MCP tool family, exactly as Claude Code does
- [x] Session knowledge store — per-session command outcomes, error→fix pairs, CWD history, TUI apps seen; fed by OSC 133 with silence-timer fallback; persisted with 2s debounce
- [x] TUI app detection — alternate-screen tracking classifies terminal as Shell or FullscreenTui with app hint (vim/htop/lazygit/…)
- [~] Private secret forms (#1435-6e1d) — in-memory zeroizing store, separate field-schema form, exact argv consent, pipe output masking, HTTP nonce entry and inspection gating; security review pending. Headless initiation is not part of this slice. Browser entry uses the existing app origin/auth/transport; origin isolation, enforced TLS and in-flight inspection withholding are accepted omissions.
- [~] `ai_terminal_*` MCP tools — shipped, then deleted in #f6ed. Two overlapping tool families cost tokens on every turn and made the model guess; external clients now drive terminals through the `session` tool. Secret redaction moved to `session action=output`; the mandatory native confirmation was dropped on purpose (a remote client cannot answer it — `ui action=confirm` can)
- [x] ChoicePrompt parser variant — numbered confirmation menu detection with destructive-label flagging, PWA overlay, `sendPtyKey()` helper
- [~] Claude AskUserQuestion on mobile — captured Ink dialog supplies all choices and arrow-then-Enter selection; real-phone verification after backend restart pending
- [x] MCP OAuth 2.1 — RFC 9728 + RFC 8414 PKCE flow for upstream MCP servers, `tuic://oauth-callback` deep link, shared `TokenManager` with thundering-herd-safe refresh
- [x] GitHub Ops dashboard — live review findings, proposals, auto-fix sessions, conflict assists, and CI/merge readiness. Review, changelog, and improvement scans run as unattended ego turns (#795-320b)

### Completed (Voice Dictation)
- [x] Local Whisper inference via whisper-rs (Metal GPU acceleration)
- [x] Audio capture (cpal, 16kHz mono resampling)
- [x] Text correction map (longest-match-first dictionary)
- [x] Model download from HuggingFace (large-v3-turbo)
- [x] Push-to-talk mic button in StatusBar (blue pulsing animation)
- [x] Configurable push-to-talk hotkey (keydown/keyup)
- [x] Transcribed text injection into active terminal via PTY
- [x] Settings > Voice page, formerly the Dictation tab (model, hotkey, language, corrections)
- [x] Shell integration inject_text stub (prepared for external triggers)
- [x] Streaming transcription with adaptive sliding windows (1.5s→3s)
- [x] Streaming preserves speech before trailing silence; shared RMS and speech-confidence gates reject no-speech audio
- [x] Floating toast for partial transcription results
- [x] Prompt token carry-forward across windows
- [x] Hands-free conversation — one bound terminal, continuous VAD segmentation, optional activation phrase with a timed window, cancellable hold-back before sending. Disarms itself on target closure, owner disconnect or capture failure
- [x] Custom start notice: `hands_free_start_notice` replaces the built-in `MODE_ENTRY_HINT` (empty = built-in, folded to one line); `get_hands_free_default_notice` serves the default for a reset control
- [x] Earcons: an 80 ms Web Audio blip on a delivered turn and a softer one on a gate drop, keyed on the monotonic `deliveredTurns`/`droppedTurns` status counters and played only by the audio owner
- [x] Speech is typed straight into the bound agent's composer through the framed injection write (`pty::write_voice_turn`), even while the agent is busy, and never enters the Compose queue. A confident question/permission dialog or a draft in the composer holds the turn in the hands-free mode, which retries it every tick; there is no other delivery path
- [x] Spoken replies via local Kokoro (`speech/`), driven over IPC, HTTP and MCP by the same functions; downloadable per-language voice bundles verified by hash
- [x] Persist Spoken replies in existing dictation settings, default on; mobile and Settings mute the server voice tool without disarming capture. Edge HTTP 401/403 suppresses retries for five minutes and sends one asynchronous outage notice to the bound conversation (#1659-f3cc)
- [x] The conversation holds one language end to end — Whisper's detection picks the voice and the model is asked to answer in it
- [x] Barge-in — WebRTC AEC3 (`echo.rs`) keeps our own reply out of the segmenter; the user talking stops playback and opens the next turn (measured: 200 ms stop latency — hush waits for `min_speech_ms` of speech so residual echo cannot stop a reply — 0 false triggers)
- [x] Download progress and utterance state are pushed on `/events` as well as to the desktop window, from one serialized payload per event; utterance transitions come from the render thread that performs them, through an observer port, so `finished` and `interrupted` reach a client that never polls
- [x] A browser tab is its own microphone and speaker over one WebSocket (`dictation/browser.rs`), so a remote client holds a whole conversation on its own hardware. Neither owner falls back to the other's devices, a client that vanishes disarms on the same path as a dead local device, and the frontend half stays transport only — segmentation, the activation phrase and the hold-back all remain in Rust

### Completed (P2)
- [x] Agent toast repository action — a toast from a different registered repository offers a keyboard-reachable action that selects its origin and focuses the still-live originating session; the action is absent for the active repository and for removed repositories (#835-314c)
- [x] Alternate-screen scrollback — isolated bounded history for fullscreen apps, primary-only durable logs, and atomic renderer-generation transitions
- [x] Task completion detection
- [x] Audio notification when agent awaits input
- [x] Intent tab titles override spawn labels while explicit user renames remain protected across reconnects
- [x] Remote completion muting survives reconnects and deduplicates idle/exit signals per busy cycle
- [x] IDE launcher with app icons

### Pending (P2)
- [ ] Error handling strategy config

### Agent Configuration (Done)
- [x] Settings > Agents tab with per-agent run configurations and an explicit, removable Codex bypass argument with warning icon
- [x] MCP spawn accepts caller environment overrides and an overrideable run-config model while preserving legacy model arguments
- [x] MCP bridge install/remove for every MCP-capable agent in the canonical registry
- [x] Terminal context menu > Agents submenu with run configs
- [x] Agent binary detection and version display
- [x] Per-agent native scrollback preference applies to supported CLI launches and commands typed in TUIC shells
- [x] Managed Claude and direct Codex spawns accept new workspace trust by default without editing the agents' saved trust files; per-agent opt-out leaves their normal question in place
- [x] ego terminal permission mode and sandbox choices persist in agents.json and share one Rust translator for terminal launches and MCP spawns
- [x] agents.json persistence for run configurations

### Completed (P3)
- [x] Markdown rendering (MarkdownPanel, not inline terminal), with in-app local links and guarded WebView navigation
- [x] Task queue UI
- [x] Advanced keyboard shortcuts

### Pending (P3)
- [ ] Agent stats display
- [ ] Config file support
- [ ] TypeScript PTY wrapper

## Future Considerations

### WebSocket Backend
For web deployment without Tauri:
- Go backend with PTY multiplexing
- WebSocket protocol for PTY I/O
- Session management

## References

- [SolidJS Documentation](https://www.solidjs.com/docs/latest)
- [alacritty_terminal crate](https://crates.io/crates/alacritty_terminal)
- [Tauri Documentation](https://tauri.app/v1/guides/)

### Configured remote MCP ownership

Session list/output/submit and agent list_peers/send cover configured remote daemons.
Connection-qualified addresses disambiguate hosts. An authenticated desktop-initiated
duplex mail link forms a star topology; remote replies and remote-to-remote delivery
use the desktop hub. Daemon-local inbox/wake semantics are authoritative; spawn is
excluded from this protocol and local mail survives hub loss.

## Telegram channel implementation status (#1438-79b4)

The headless daemon provides native Telegram mail, correlated drafts, exact final replies, done/blocked notices and opaque callbacks. Any MCP-bound TUIC agent opts in with `telegram register`; one volatile registration replaces the previous agent with one mail notice. Registration ends on MCP session end, PTY close or agent exit while its shell remains. No configured agent UUID is required. Allowed inbound without an agent replies "Nessun agent registrato" and drops; strangers stay silent. Only the cursor persists, so restart may lose unread mail and requires registration again. Single-destination sends and private token/allowlist authorization remain. See [Telegram channel](docs/design/telegram-channel.md). Live mint deployment remains pending.

Workflow safety: pinned checks own process trees and stop on timeout, run cancellation, and shutdown. Operator decisions and workflow policy mutations require host Human authority (desktop IPC or credential-authenticated HTTP). Workflow plan Done requires current integration evidence for every approved story; stale canonical refs reopen its derived state until recertification.

Graph runtime slice A adds version-2 serial activation/predecessor/decision history and executable start validation with pinned Pause targets and deterministic final checks. History is bounded to 4096 activations per execution. Fork/all-Join schema and execution are deferred to slice G. Pre-contract runs support inspect/cancel only. Autonomous scheduling remains disabled pending slice B and authorization story 956-9745.

Workflow graph slice B: AppState owns the serial daemon executor and OS database owner lock on desktop and headless boot. Initial graph position is atomic and start-key idempotent; per-run mailboxes and durable deadlines drive controls without a frontend. Graph roots reserve native stories, and manual claim/start shares the run writer lock. No Agent effects, delivery policy or new UI are enabled; unsupported graphs cannot start as executable workflows.

Workflow graph slice C: the owning daemon executes pinned serial Agent/Judge/Loop/Pause/Notify/Join nodes through the existing RunStore ledger and managed launch fences. Named sol/sonnet profiles are required. New unsupported Gate and plan node publications are refused visibly. Manual pauses suspend active duration. Slice F supplies public story graph starts and history controls; D/E add independent approval and plan dispatch; graph completion alone never closes a native story.


### Workflow story policy (slice D)

The daemon runs pinned deterministic checks before accepting an independent reviewer approval. Native approval history preserves the reviewer actor and revision; the run retains the reviewed artifact and check receipts. A workflow implementer cannot approve its story after exit or claim release. Integration remains an explicit operator action.


### Workflow plan dispatch (slice E)

The owning daemon executes pinned plan coordinator and Create Stories visits through the existing proposal effects. Disjoint story children share a bounded project wave; overlapping or unknown scopes wait. A dependency starts only after its accepted revision has a current explicit Git integration receipt. Done alone and WontFix never release it. Approved children wait for an operator merge. The final plan Judge runs the pinned canonical checks and binds verification to the current plan fingerprint. Failed checks route through a bounded replan with a fresh coordinator attempt. Public start/history controls use the same graph entry point; parallel graph branches follow in slice G.

## Managed launch instruction inspection

Implemented: terminal context-menu inspector backed by prospectively captured live-session receipts. It displays final managed briefs, explicit system instruction arguments/file snapshots and MCP initialize responses with source and original UTF-8 byte size. Existing workflow preview/hash receipts are unchanged. Autonomous agent file reads and historical launches are explicitly unobservable. Redacted receipt text is limited to 64 KiB/16 sections, with a 32 KiB section cap; receipts are not restored after backend restart.

### Run incidents

Plans and Stories run history contains a read-only incident projection owned by Rust. Workflow reports, interrupted attempts, bound-session exit/state and pending prompt-delivery failure markers, and bound task records supply evidence. No elapsed-time stuck inference, new persistence or automatic recovery is introduced. Suggestions require operator action. Only agents explicitly bound to the selected run participate; unavailable causes are disclosed. IPC and HTTP use the existing workflow run action contract.

### Conversation-specific AI Chat launch (implemented)

Custom conversations snapshot executable/profile/workspace overrides in `ai_chat_launches`, keyed by ego session id, and retain a host-issued peer identity. Each has its own ACP connection; default chats continue on the shared default connection. The header shows saved launch values. `acp_chat_open` and `POST /acp/chat/open` create or reopen the same conversation. MCP inbox reads emit content-free INFO audit events with protocol caller, bound peer/owner and returned message ids.

### Automation Once schedule foundation

The Rust core supports one local date-time in a stored IANA timezone, alongside
cron. Creation rejects elapsed instants and spring gaps; folds use the earlier
instant. Once has one scheduled occurrence, a completed state after consumption,
and a retained definition. The scheduler uses durable occurrence reservation to
prevent restart/catch-up duplicates. Public API and UI integration follow the
[Automations plan](plans/automations-scheduler.md).
## Automations Dialog — Frontend verified, API integration pending (#1618-685b)

The machine-local command-palette dialog edits cron or Once definitions and
displays backend zone-aware previews and recent run evidence. Pause and Resume
mutate only enabled by id, preserving concurrent agent edits and unsaved drafts.
Once keeps `once_local` as a wall time; the backend owns timezone resolution and
completed-occurrence evidence. One injectable adapter isolates Step 8 envelopes.
Targeted frontend tests verify this boundary; real scheduler integration remains
a separate requirement before the feature is available.
