# TUICommander Specification

Development builds show a dismissible restart warning when Cargo dep-info inputs are newer than the backend executable, or that executable was rebuilt after application entry. Main and used bridge inputs are checked independently; unrelated frontend-only changes stay quiet. An edit followed by a revert still warns, as Cargo would rebuild. Missing build artifacts report identity unavailable. Release builds contain no warning module or debug identity (#2332-3eef).

The Plans column exposes Edit plan with Save/Cancel and conflict-safe draft retention. Native plan title/source corrections use atomic `update_plan` patches with a stored expected revision across MCP, HTTP, IPC, and CLI. Workflow planning and verification subjects include current plan metadata. Source changes wait until any running plan coordinator/planner finishes or is cancelled; title-only corrections stay available (#2170-d828).

Native story list service (#2174-6b3f): optional state/priority/story_type filters combine server-side. Optional limit 1–100 and a plan/filter-bound keyset cursor page id-ordered records with additive next_cursor; absent limit preserves full filtered reads. Dispatch retains the existing complete plan and priority/id order.

Agent desktop verification (#2086-269b): `make agent-test` uses a dedicated macOS debug binary, named config namespace and explicit loopback port. Global input, dictation, Telegram, updater, deep links, global MCP writes, global CLI installation/update/removal, shared agent hook changes and sleep inhibition are disabled. Dry-run and supervised instance-only stop are available; simultaneous desktop/TCC proof remains an operator validation step.

MCP capability cutover (#2068-63b9): persist `mcp_capabilities`, migrate saved tool denials once with visible unknown-name diagnostics, enforce parent/member restrictions on discovery and dispatch, and remove retired names without aliases. Clients must reconnect and review external allow rules.

Native MCP guidance (#2069-7347): session, agent, repo, ui, voice, telegram and automations expose compact agent-facing action contracts. Their action=help returns every displaced rule from the full native guide. Existing action validation, conditional availability, disabled-tool gates and Settings copy remain unchanged.
Design Mode optional capture (#1724-fe7e): the extension declares `<all_urls>` only in optional host permissions. **Let agents capture any site** requests that grant from a user click; after consent Chrome capture needs no further action gesture. Without consent, preserve `activeTab` capture and return a typed error naming the option. Capture consent does not grant agent eval or opt into remote error injection. Remote error reads require explicit host consent; the panel confirms actual eval grant origins instead of trusting request success alone.
UI Design Mode actions (#2067-e708): `ui design_start/stop/status/reload/navigate/screenshot/errors/eval` reuse the Design Mode owner, armed caller/repository checks and extension host grants. Saved `design_mode` denials apply to the grouped actions. The old tool is removed by the 2068 capability cutover; use `ui design_*`.


Conditional MCP tools (#2066-516c, conditional portion of #2067-e708): remote requires a live authenticated connected server; voice requires persisted dictation enabled and spoken replies enabled; Telegram requires enabled setup, a bot alias and its private token file. Shared predicates gate listing, search, schema lookup and calls. Owner changes invalidate the catalogue. Configuration does not replace conversation binding, Telegram registration or deployment consent. Design Mode uses grouped `ui design_*` actions after the catalogue cutover.
- [x] Mobile CLI answer bars occupy a separate gutter outside the text box, including column-zero ego rows, without adding Claude text indentation (#2110-734c).
- [x] Mobile CLI answers reuse the desktop answer planner and green gutter style instead of displaying protocol speech bubbles; prompts and fenced examples stay literal (#2070-773d).

- [x] Native MCP Settings rows use domain sections without changing tool names, saved disabled values or effective denials (#2063-baa9).

- [x] Mobile Markdown review sends persisted edits and tweak comments through the shared desktop prompt and backend idle queue, with repository agent selection and delivery feedback (#2042-a71b).

- [x] Durable independent peer-mail subscriptions: shared insertion journal, filtered bounded replay, explicit acknowledgement, retention-loss counters, HTTP SSE and ACP resource notifications (#2004-4760).

- [x] Generated launch playbook modules with global/per-agent switches, append-only user additions, explicit all off, legacy default-copy migration and machine-owned effective previews (#2000-b50f).

- [x] Mobile open Markdown tabs: one shared list in a top-bar dropdown at >=768 px, otherwise in the session menu; unchanged-cwd polls do not reset entries (#2024-9a86, #2025-82cb).

- [x] AI Chat smart tables (layout A): protected short columns, sticky first column, conditional scroll hints, exact-source Markdown toggle/copy and read-only document action (#1923-c6fb).

- [x] AI Chat Tidy layout: single header, conditional tab strip, labelled overflow actions, compact process rows, latest-answer suggestions and untimed history (#1958-b67e).

Plugin settings (Step 5, #1785-c7a1): loaded plugins register complete scoped schemas and typed private-file owners; patches preserve siblings, redact secrets and schedule reload. Core migrations cover build-cleaner, at-capacity-retry, cache-keepalive, claude-wakeup and the Telegram notifier example. Unload revokes catalogue entries and write authority. General settings MCP cutover remains separate.

Settings policy (Step 6, #1786-9cbd): the shared service/registry enforce host-issued origin context, write-only reads, and backend-built exposure/execution proposals. Only the existing human confirmation channel grants a connection/target/revision/content-bound approval, consumed once within 300 seconds. No lock spans the wait; failed, refused, disconnected or expired proposals write nothing. The explicit host scope covers remote services, trusted/custom executables, stdio execution selection and tunnel port/auto-connect; ordinary settings remain immediate. Transport publication remains Step 7.

Desktop close policy (B1, #1802-e6b8): Rust owns `app.close_behavior` (`ask` default, `background`, `quit`). Ask offers Remember; Background hides main only. Explicit Quit uses `confirm_before_quit` and closes live PTYs in Rust. Detached panels close independently. Tray and WebView unload belong to later stages.

Agent capture tooling: `scripts/agent_capture/run.py` records real installed CLIs through isolated headless sessions, promotes reviewed captures with ordered expected states, and integrates with the 1342 replay oracle. Missing or unauthenticated CLIs remain unverified.


Published story workflows can start in Plans and Stories through the owning daemon. Run history renders graph positions, decisions, evidence and paged event payloads, with explicit graph recovery and cancellation. IPC/HTTP and the generated `workflow action=run` MCP schema share the native run service. Plan start remains visibly unavailable pending plan dispatch. The grouped `workflow` MCP tool exposes typed story, run, proposal_create, report and launch actions through existing service owners. Proposal retries preserve coordinator ownership and proposal-key idempotency. Legacy workflow tool names are removed without aliases; scheduled automations stay separate. `tuic workflow` forwards the same envelope.

**Version:** 2.0.0-beta.3
**Last Updated:** 2026-09-16

## Shared form controls — Implemented (#1815-01c9)

Native Select, Input and Textarea wrappers and a keyboard-accessible segmented
view switch share theme-based form styling. All desktop and mobile native select fields
use the shared Select. Tasks, Settings, Automations, Git review and chat forms also
share their text controls. The JSX source guard rejects native selects outside ui
without migration exceptions. Desktop fields are 36px and mobile touch fields are 44px.

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

### Tasks roadmap component — Implemented (#1690-5d98)

`TasksRoadmap` renders provider list pages using explicit host start/target field IDs,
or the first two date descriptors in provider order when IDs are absent. Month/quarter
zoom and previous/next/Today navigate a local calendar. Date-only values preserve
their day; zoned timestamps use local dates, with inclusive target days and clipped bars.
One endpoint gives a milestone; unset, invalid or reversed dates remain in a side list.
Virtual rows and the list query controller retain pagination and stale-response protection.
Provider-declared date roles and Tasks shell mounting remain separate integration work.

### Tasks list component — Implemented (#1686-ce77)

`TasksList` is a host-rendered Solid component backed by the data-only work-item
provider port. It offers account-scoped sources, presets, descriptor filters,
250 ms search debounce, provider sorting, virtual rows and cursor pagination.
The query controller keeps at most 32 cached pages, ignores obsolete requests,
and labels retained rows stale after an error. Mounting the Tasks shell is a
separate integration step.

TUICommander is a multi-agent terminal orchestrator designed to manage supported AI coding agents, including Claude Code, Gemini CLI, OpenCode, and Codex, in parallel. It provides per-pane zoom, git worktree isolation, and GitHub integration.

## Goals

1. **Parallel Agent Orchestration** - Run 50+ coding agents simultaneously
2. **Git Worktree Isolation** - Each task gets its own isolated workspace
3. **Per-Pane Font Control** - Independent zoom levels for each terminal, shared by its CLI and Chat views
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

### Bounded input frame scheduling

Desktop and HTTP/PWA interactive input arm a 100ms priority window before the
PTY write. Only subsequent reader-applied output can wake the existing frame
ticker early. Priority uses a rolling budget of four bypass cuts per 100ms and
8ms coalescing from the previous ticker cut; repeated input does not reset the
budget. The normal animation/load floors, desktop receipt gate, DEC-2026
buffering, ordering and shared-damage repair remain on that same serializer.
This is a scheduling policy, not a trusted keyboard-to-presentation guarantee.

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
type AgentType = "claude" | "gemini" | "opencode" | "codex" | "amp" | "cursor" | "goose" | "grok" | "droid" | "pi" | "git" | "api";
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

Hold the primary modifier alone for 450 ms to show the first nine terminal-tab shortcut badges. Other keys, mouse buttons, wheel events, release, blur, and visibility changes cancel these hints. The Cmd+Ctrl quick switcher remains independent.
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
and does not change shell cwd or acquire cleanup ownership. Explicit declarations
own tab placement during OSC 7, reconnect and restore while the matching checkout
exists. Creation without declaration remains cwd-based. Removing
or recycling a worktree closes only terminals whose current cwd lies inside that
checkout. A stale sidebar membership never authorizes closing an unrelated tab.


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

Detached checkouts under OS/environment temporary roots, `~/Gits/.tmp` and
Unix `/tmp`, `/private/tmp`, `/var/folders`, `/private/var/folders` are excluded
from orphan scans and orphan removal. Registered and resolved paths are checked,
including missing checkouts. Detached sidebar inventory stays independent of
cleanup eligibility. Repository refresh triggers cleanup; there is no separate
orphan scan timer.

For eligible detached orphan worktrees, the Ask dialog automatically removes after a
configurable countdown only when every checkout is clean (including untracked
files) and its HEAD is reachable from a branch. Otherwise it lists the unsafe
reasons without a countdown. Keep and Escape cancel. An agent can answer the
open dialog through MCP; a remove answer and the final removal repeat the
backend safety check.

## Project Progress

Native plan and story records have a separate config-directory SQLite authority (`stories.sqlite3`). Progress remains a human-readable journal and does not determine story status. Manual story actions use the shared Rust service across IPC, HTTP, MCP, and CLI. The desktop and browser UI exposes plan and story lists, criteria, dependencies, and manual transitions. For manual plans, WontFix is terminal for plan aggregation but does not satisfy a dependency or promote a dependent; a nonempty plan with only Done/WontFix stories is Done, while an empty plan is Draft. Any trusted caller can remove a direct WontFix dependency only from a Backlog story with a current revision. In plans with no workflow run, a dependent becomes Ready when every remaining dependency is Done; workflow-owned plans additionally require current integration receipts. An explicitly Blocked story is never auto-unblocked. Rust derives direct and transitive abandoned dependency indicators and the WontFix count on `plan_view` reads; the UI renders them. Any trusted caller may approve after checking the acceptance criteria. Actor identity is tracking only and never restricts story or workflow actions. Desktop IPC and credential-authenticated HTTP record `human`, sessionless local HTTP records `local_api`, and managed transitions record the acting session. Local token exchange is accepted. The optional workflow engine described in `plans/native-story-workflows.md` remains in development; import/export is excluded.

Story details show the persisted origin, plan step, revision and claiming session. A live terminal name appears beside the stable session ID; closed or unavailable terminals retain the ID. Transition history shows committed commands, revisions and actors in backend order, with Refresh history and explicit loading, error and empty states. New story offers Native or Plan step origin; a plan-step story requires a step label.

Native stories have an optional persisted story type (feature, fix, refactor, chore, task). Old and omitted values remain untyped. Rust derives combined open/closed/type filters and per-status board membership without filtering dependency records or plan aggregation. Open excludes only Done and WontFix. The dialog retains filters and the List/Kanban switch for the application session, including detach and reattach through panel parameters and return actions. The core Solid board follows the separate Wiz plugin layout without importing its code; native cards open details without plugin drag/status mutation.


Workflow definitions have their own config-directory database (`workflows.sqlite3`): editable drafts and immutable published revisions. Graph validation gates publication; the seeded `Resolve plan` definition pins a published `Story delivery` revision. The existing `human` closure policy requires an explicit approval transition, which may come from any trusted caller; `automatic` closure cannot be published until a TUIC-owned evidence gate exists. Definitions pin bounded direct-executable check commands. TUIC runs independent checks outside the global receipt-service lock. Completion probes the assigned worktree and Git artifact before the receipt transaction, then revalidates persisted run sequence, story revisions and command identity inside it; unrelated run events do not discard a valid result, and retries keep their original request cursor. TUIC records durable commit/tree-bound check receipts, then verifies an operator-created merge on the canonical branch against the clean tree computed by Git from its exact parents and runs the checks again on its result before recording `StoryIntegrated`. Extra or omitted merge content is rejected. A conflicted merge requires explicit human review or a separately verified artifact; the current receipt path cannot certify it. A later canonical commit requires a TUIC-computed `CanonicalRecertified` event with successful checks at the clean new tip and ancestry of every integrated source. Durable run records and event projections exist in `workflow_runs.sqlite3`, with idempotent actions and recovery of uncertain effects after restart. Runtime reconciliation preserves live attempts and in-flight effects; restart-only recovery logs each failed run and continues recovering healthy runs. Explicit story dispatch enforces bounded concurrency and conservative file-scope exclusion, while a managed coordinator assigns registered isolated worktrees before spawn. Dependents and final run completion require a current integration receipt at each accepted story revision. Plans and Stories keeps a stable, viewport-bounded size while tabs and selections change; long content scrolls inside. On desktop, its header can detach it into a resizable window and bring it back through the shared panel controls. The Plans and Stories dialog reads a plan's run list and ordered event timeline. The owning daemon executes the published serial story and plan graphs; public graph start controls remain in development.

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

`update_story` takes `story_id`, `expected_revision`, and a camel-case `patch`. Omitted fields stay unchanged. The patch accepts `title`, `priority` (1–3), `fileScope`, `origin` (including a plan step), nullable `storyType` (`feature`, `fix`, `refactor`, `chore`, `task`), optional `problem` and `risk` (`low`, `medium`, `high`), ordered `criteria`, and the final `dependencies` set. Null clears `storyType`, `problem` or `risk`; old story records default both to absent. Unknown fields, invalid values, cross-plan dependencies, duplicates, cycles and stale revisions are rejected without partial story writes. Only unclaimed Backlog/Ready stories without active or paused workflow ownership can be amended. Unique byte-identical criteria retain their checks after reorder; changed, new or ambiguous duplicate criteria become unchecked. A priority-only patch preserves all checks. Dependency changes recompute readiness. Planning and execution fingerprints include the amended requirements/revision, so existing evidence cannot certify changed work. StoriesDialog amendments use a fixed revision snapshot and changed-field patch, with Save/Cancel and retained drafts on conflict. QA is not amendment-eligible. Criteria use separate ordered entries so multiline requirement text is preserved during edits.

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

The desktop and headless HTTP routers share GitHub poller controls and repository
status, PR and issue reads. Poller startup and event delivery do not require a
WebView; TCP authentication and the trusted local Unix transport are unchanged.
GitHub account/auth and mutation routes are not part of this shared surface.

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
- [x] Terminal HTTP(S) link menu starts Design Mode with the owning agent and exact URL; HTTP/IPC optional `url` and MCP `ui design_start/design_stop/design_status` share the lifecycle. Agents use URL-aware start as the default path for layout work.
- [x] Design Mode extension heartbeat: every 5 seconds, expiry after 60 seconds without a beat (600 seconds before the first beat), and MCP `ui action=design_reload` delivered on the next heartbeat.
- [x] Design Mode v2 pilot: dedicated browser profile with automatic extension loading on cold Edge starts (running profiles must close first; branded Chrome reports an unsupported-autoload error), extension sidebar, explicit picking, per-pick comments, launching-agent default, same-repository recipient selection, native messaging, idle-gated Send and live dev-server discovery; native Edge validation remains tracked in to-test.md
- [x] Split pane layout
- [x] Touch branch action trays: left swipe reveals existing actions; vertical scrolling and desktop hover controls remain unchanged
- [x] Multi-repository sidebar
- [x] Sidebar session context menu copies the PTY session ID, live child PID, and owning session-list alias through the shared clipboard path; unknown PID and absent alias entries are hidden.
- [x] Rich parent-agent links reveal and select live children in other repositories without moving sessions
- [x] Git diff panel
- [x] Interactive agent prompts UI
- [x] IDE launcher dropdown
- [x] GitHub integration
- [x] Sidebar PR badges retain `#number` while showing lifecycle, conflict, CI, and review state
- [x] Parallel agent orchestration
- [x] Optional subagent sounds and OS notifications through the existing silence control; preserve Activity, bell entries and urgent user cards
- [x] Orchestrated PTY task descriptions with prompt-derived fallback metadata
- [x] Agent task_get/task_cancel share the existing task service, spawning-owner checks and immutable outcomes; task cancellation leaves the PTY alive. The current task tool stays until catalogue cutover (#2064-6385).
- [x] Page retained MCP terminal output through text/byte offsets, with continuation instructions and eviction-gap reporting
- [x] One-call MCP managed-agent submission with bounded terminal-movement receipt
- [x] Expandable terminal Context bar for agent intent, orchestrator assignment, and last user prompt
- [x] Recover captured intent and substantial prompt through session snapshot catch-up and live state reconciliation
- [x] Font selection setting
- [x] Tab bar with keyboard navigation
- [x] Density modes for readability
- [x] Terminal Find routes to the visible CLI buffer or Chat transcript; Chat supports next/previous selected matches and switching views closes search and clears highlights (#1633-f2cc).
- [x] Answer surfaces use a green left rule without a background tint; Paper uses its darker success green for the rule; paragraph spacing and user-question styling stay unchanged (#1896-f355).
- [x] Terminal Chat groups answer blocks into one reading-width card per turn, copies the whole card, compresses terminal pastes and system notices, and keeps the collapsed tool row with Claude Bash descriptions. Turn separators are omitted until transcript timestamps reach the frontend (#1832-b19a).
- [x] Terminal Compose accepts ordered image drops and an image-only multi-file picker through the existing paste save path. Drop inserts at the pointer; unsupported images and failed saves show an error.
- [x] Terminal Chat uses focused, docked Compose input with existing send/queue semantics; CLI open/pin preferences and shared unsent drafts survive view switches. Active exported native questions and permission choices render above Compose and use the same PTY key semantics as mobile, without submitting drafts; unexported/free-form prompts retain CLI fallback.
- [x] Terminal selection copy unwraps soft-wrapped rows, removes coherent Claude visual gutters and composer margins, and preserves literal block characters, pasted prompt glyphs, and short typed line breaks
- [x] Status bar with branch and PR info
- [x] Rate limit detection
- [x] JSONL output parsing
- [x] Prompt library with variables
- [x] Keyboard redirect to terminal
- [x] Ideas panel (formerly Notes) with send-to-terminal and delete actions
- [x] Planner `add_idea`, `list_ideas`, and `update_idea` share the locked notes writer across IPC, HTTP, CLI and MCP. Source/project provenance persists; desktop/SSE notices reload panels and keyed UI deltas preserve concurrent agent additions.
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
- [x] Mobile Ideas uses shared browser dictation at the caret/selection, without saving an idea or sending to a session (#2022-1b80).
- [x] Mobile terminal prompt keyboard correction defaults on, with a device-local Settings toggle for autocorrect, spellcheck and sentence capitalization (#1878-3a3c).
- [x] Mobile CLI and Chat jump-to-bottom arrow floats over output at least 8px above the key bar, outside the composer, only while scrolled back and resumes output follow without terminal input (#1975-4a38).
- [x] Mobile terminal Ctrl menu (Ctrl+C, Ctrl+B, Ctrl+D, agent-aware Ctrl+Enter)
- [x] Mobile Send long-press Queue transfers the live draft to the shared server ready-window queue; taps still submit immediately.
- [x] Session uploads live under TUIC data, with launch grants and session-end cleanup; startup reclaims only provably dead owners, preserving concurrent instances; ego invocation-scoped binary-file read roots remain unsupported (#1833-aa60).
- [x] Shared mobile composer + menu: attach file, camera capture, supported queue/commands and Chat draft parking; browser mic inside the text box (#1880-d7ff).
- [x] Mobile Companion PWA (cached connection-test landing on unreachable navigation; visible API/stream outage state with retry and automatic recovery; Working/Idle/Awaiting header counts share row badge state and the Working hold across poll updates, include Sub-tasks as Working and exclude Finished rows; searchable sessions, live output, question reply including Codex interactive choices, activity feed)
- [x] Mobile blocked reports keep their Input state while current suggested actions replace generic Yes/No controls; native dialog choices retain priority (#2082-f782).
- [x] Automatic mobile resume evidence: bounded per-tab lifecycle/viewport/root/stream trail, acknowledged backend-log upload after an abnormal prior document; original iPhone grey-screen cause remains unproven (#1900-5ff6)
- [x] Mobile shell re-measures viewport height and top on pageshow, foreground visibility and document resume, using the existing coalesced animation-frame update; conditional stale-geometry regression does not prove the physical iPhone cause (#1900-5ff6)
  - [x] Compact bare attachment icon left of the session composer; smaller mic and Send sit inside its right border (#1719-ff5d)
  - [x] Files tree search, hidden-folder ordering, long-path preview, and full-height wrapped editing
  - [x] Repository-relative Markdown images through an authenticated, repository-confined image route
  - [x] Expandable Progress messages, local Activity time and minute durations, version display, and a server-persisted mobile light theme separate from the desktop theme
- [~] Managed-agent blocked questions alert the mobile PWA through encrypted Web Push when the desktop is away; a phone reply returns through atomic session submission with a receipt (real-phone verification pending)
- [~] Pending ego permissions and form requests alert a subscribed phone when the desktop is away, linking to the matching mobile Chat conversation with a 30-second limit per conversation (real-phone verification pending; ego card notices await a defined wire shape)
- [x] MCP Proxy Hub (aggregate upstream MCP servers via HTTP and stdio, tool namespace prefixing, circuit breaker, hot-reload, origin-bound OS keyring credentials enforced before outgoing requests, tool filtering, session-local Grok compatibility through lazy meta-tools)
- [x] Copy Path in Markdown panel
- [x] Claude Usage Dashboard (native SolidJS component with API polling, session analytics, usage timeline)
- [x] ConfirmDialog component (in-app dark-themed replacement for native OS dialogs)
- [x] Status bar unified agent badge with priority cascade (rate limit > usage API > PTY usage > name)
- [x] Movement-based PTY agent activity detection ("text above the input area moves = active") with explicit-hook precedence, prompt-based Ready screens, Codex presence-based Working policy, Grok activity/composer disambiguation, interrupt confirmation, and confirmed-idle safety gates
- [x] PR lifecycle filtering (CLOSED hidden, MERGED hidden after 5min user activity)
- [x] Notes/Ideas: mark as used, badge count in status bar
- [x] Notes/Ideas: image paste support (Ctrl+V), thumbnails, send absolute paths to terminal
- [x] Inter-Agent Messaging (`messaging` MCP tool: register, list_peers, send, inbox with channel push + polling fallback)
- [x] Opt-in interrupting mail (`agent send urgency=interrupt`): safe Esc, ten-second confirmed-idle wait, full input submission and explicit urgent fallback; interrupted tools may leave partial work.
- [x] Smart Prompts (29 built-in AI prompts with context variable resolution, shell/inject/headless-CLI execution, toolbar dropdown, SmartButtonStrip, Command Palette integration). The `api` execution mode runs one unattended ego turn over ACP (#787-ee50): a session with no MCP server, every question refused, the final text routed to the prompt's output target. Its old executor — a direct provider call from TUICommander — went with the embedded engine (#784-0aec) and did not come back
- [x] AI Chat panel (`Cmd+Alt+A`) — ego over ACP (#785-58ca), bound to a repository and ACP session rather than a terminal. TUICommander renders the journal, permissions, elicitation forms, plans and session controls while carrying no LLM client or provider API key of its own. The selected session is saved per root and restored after restart; the picker lists ego's durable sessions by title and activity time (#1071-46c9)
- [x] AI Chat conversation deletion — capability-gated row controls confirm the conversation name, call ACP session/delete, remove successful deletions from tabs and the list, and retain refused conversations with the agent error (#1917).

- [x] AI Chat repository context pin — freeze the per-prompt repository per conversation, with configured repository text color; draft parking stays independent (#1924-12be).
- [x] AI Chat image paste before connection — discover image capability on first paste, and share Finder/text clipboard selection with Ideas and Compose.

- [x] AI Chat image paste — supported images stage removable previews and become ACP image content blocks when the agent advertises image prompts
- [x] AI Chat session details — ACP title updates rename the header and picker; context-window use and reported cost appear in the footer; a session settings dialog labels every select option and the one-row control bar summarizes the model's short name and mode beside named icon actions
- [x] AI Chat transcript and tabs — selectable messages, with message Copy on hover or keyboard focus; sent prompts reconciled with ego's chunked echo; copyable code and tool output; trailing `suggest:` tokens rendered as reply buttons; terminal-shared web and file link handlers; parallel ACP sessions with independent drafts and transcripts; only running tool calls pulse
- [x] AI Chat control bar has no duplicate + action; New chat tab creates conversations (#1755-7709).
- [x] Shared terminal Chat and AI Chat new-text pill — detached content arrivals offer a return to the bottom; search alone does not trigger it; desktop/mobile provide a per-client opt-out (#1757-c6a9).
- [x] AI Chat transcript polish — collapsed tool rows show short names, message Copy keeps its own space, and streaming follows the bottom until the reader scrolls up
- [x] Desktop AI Chat composer actions reuse the terminal composer pin/play icons and button sizes; tooltips and accessible names distinguish Send, Queue and parked drafts.

- [x] AI Chat composer polish — text grows to a bounded height and pastes over 200 words stay compact until the full text is sent
- [x] AI Chat prompt parking — Ctrl+S or the composer control parks text and images per chat tab, swaps or restores them, and returns the parked draft after the next Send
- [~] AI Agent loop (ReAct) — shipped, then deleted in #784-0aec with no TUICommander-side successor. ego runs its own tool loop and reaches terminals from outside, through the `session` MCP tool family, exactly as Claude Code does
- [~] Session knowledge store — removed in #2115-1333 after its last reader was removed; no command-outcome recording or periodic flush remains. Existing history files stay untouched.
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
- [x] `Cmd+Alt+V` (`Ctrl+Alt+V` on Windows/Linux) toggles hands-free on the active terminal; user overrides win and disabled dictation is a silent no-op.
- [x] Custom start notice: `hands_free_start_notice` replaces the built-in `MODE_ENTRY_HINT` (empty = built-in, folded to one line); `get_hands_free_default_notice` serves the default for a reset control
- [x] Earcons: an 80 ms Web Audio blip on a delivered turn and a softer one on a gate drop, keyed on the monotonic `deliveredTurns`/`droppedTurns` status counters and played only by the audio owner
- [x] Speech is typed straight into the bound agent's composer through the framed injection write (`pty::write_voice_turn`), even while the agent is busy, and never enters the Compose queue. A confident question/permission dialog or a draft in the composer holds the turn in the hands-free mode, which retries it every tick; there is no other delivery path
- [x] Spoken replies via local Kokoro (`speech/`), driven over IPC, HTTP and MCP by the same functions; downloadable per-language voice bundles verified by hash
- [x] Persist Spoken replies in existing dictation settings, default on; mobile and Settings mute the server voice tool without disarming capture. Edge HTTP 401/403 suppresses retries for five minutes and sends one asynchronous outage notice to the bound conversation (#1659-f3cc)
- [x] The conversation holds one language end to end — Whisper's detection picks the voice and the model is asked to answer in it
- [x] Barge-in — WebRTC AEC3 (`echo.rs`) keeps our own reply out of the segmenter; the user talking stops playback and opens the next turn (measured: 200 ms stop latency — hush waits for `min_speech_ms` of speech so residual echo cannot stop a reply — 0 false triggers)
- [x] Download progress and utterance state are pushed on `/events` as well as to the desktop window, from one serialized payload per event; utterance transitions come from the render thread that performs them, through an observer port, so `finished` and `interrupted` reach a client that never polls
- [x] Mobile session headers distinguish the two-way voice call with a handset icon and call labels; composer dictation keeps its microphone (#1979-89e1).
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
- [x] Queued user commands answer an idle agent's progress-blocked wait in FIFO order; native dialogs, busy state, uncertain delivery and partial drafts still block delivery (#2177-053d).
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

The headless daemon provides native Telegram mail, correlated drafts, exact final replies, done/blocked notices and opaque callbacks. Any MCP-bound TUIC agent opts in with `telegram register`; one volatile registration replaces the previous agent with one mail notice. Registration ends on MCP session end, PTY close or agent exit while its shell remains. No configured agent UUID is required. Without explicit registration, allowed inbound reaches the latest-connected live AI Chat peer; connection order, not selected tab, resolves multiple live chats. Explicit registration always wins. Allowed inbound with neither kind of agent replies "No agent is listening. Ask an agent in TUICommander to register for Telegram." and drops; strangers stay silent. Only the cursor persists, so restart may lose unread mail and requires registration again. Any live MCP-bound agent may send without registration or changing inbound ownership. Non-owner sends have server-derived name prefixes on every chunk, owner-only buttons, and a 30-second per-peer interval. Telegram Replies select the unique live exact-name peer from replied-to text, otherwise the inbound owner; no message-ID cache is retained. Owner text remains byte-exact, including leading brackets. Single-destination sends and private token/allowlist authorization remain. See [Telegram channel](docs/design/telegram-channel.md). Live mint deployment remains pending.

Workflow safety: pinned checks own process trees and stop on timeout, run cancellation, and shutdown. Operator decisions and workflow policy mutations require host Human authority (desktop IPC or credential-authenticated HTTP). Workflow plan Done requires current integration evidence for every approved story; stale canonical refs reopen its derived state until recertification.

Graph runtime slice A adds version-2 serial activation/predecessor/decision history and executable start validation with pinned Pause targets and deterministic final checks. History is bounded to 4096 activations per execution. Fork/all-Join schema and execution are deferred to slice G. Pre-contract runs support inspect/cancel only. Autonomous scheduling remains disabled pending slice B and authorization story 956-9745.

Native stories expose `qa` for work that waits on manual checks. A criterion starts with `[MANUAL]`, or is marked through `set_criterion_manual {index,manual}`. Active work automatically enters QA when all unchecked criteria are manual; `submit_qa` explicitly requests that state. `reject_qa` returns QA to In progress; `approve` requires every criterion checked. QA keeps plans Active and dependencies blocked. All transitions retain expected-revision checks. See the [native stories guide](docs/user-guide/native-stories.md#manual-qa).

### Reversible native record archival

In Plans and Stories, **Archive plan** and **Archive story** move the selected record out of the default list. Use **Archived plans** or **Archived stories** to select archived records, then choose **Restore plan** or **Restore story**. The toggles are independent, so archived stories remain reachable inside active plans. Archived detail stays readable; restore before starting work. Backend refusal messages remain visible when a claim or active run owns the record.

All native story transports share `archive_story`, `unarchive_story`, `archive_plan`, and `unarchive_plan`. Story actions take `story_id`; plan actions take `plan_id`. Each requires `expected_revision` and returns the updated record with its incremented `revision` and `archived` flag. Stale revisions fail without a write. Archival preserves status, criteria, dependencies, and child archive flags; it never deletes records or cascades to children.

`list_plans` and `list_stories` default to `archived: false`. Set `archived: true` to select archived records. The story filter combines with the other filters and is bound into pagination cursors. Direct reads remain available. Default plan views and workflow dispatch omit archived stories; archived plans and stories cannot start manual or workflow work. Restore the parent plan before starting its stories.

Both archive and restore refuse claimed stories and plans that contain a claimed story. An active or paused plan-root workflow owns the plan's story set. A story-root workflow owns its target and execution stories, not unrelated siblings. Cleanup of an owned story or its parent plan is refused until the run ends. The workflow writer lock remains held through the archive transaction, so a concurrent run cannot start between the ownership check and the write.

`story_archive_history` (`story_id`) and `plan_archive_history` (`plan_id`) return `{type:"archive_history",value:[{revision,archived,actor}]}` in revision order. Events commit atomically with the record and retain caller provenance. Existing status transition history remains separate. Consecutive archive-only revisions preserve accepted-work integration receipts; a work change or a moved canonical Git ref still invalidates them. Example: `tuic story '{"action":"archive_story","story_id":"<id>","expected_revision":1}' --project /absolute/project`.


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

The shared backend now starts an owner-guarded Once runtime on desktop and
headless boot. It claims reservations before effects, rechecks definitions and
destinations, uses the configured workspace base/profile and literal prompt,
and saves precheck pass/refusal/manual-bypass/no-check outcomes and launch ids.
Cron dispatch stays disabled in phase 1. Public execution transports and
completion/maximum-duration handling remain pending; idle is not success.

### Automation HTTP and IPC parity

Saved run configurations use one case-insensitive named-profile lookup across
create, update, re-enable, list diagnostics and managed launch. Unresolved
profiles are rejected with valid-profile repair choices; persisted invalid
list rows expose `run_config_error` and no next occurrence. Failed agent-created
runs offer one creator inbox notice through a durable owner-channel claim,
including failures before session/task binding. Mail acceptance is not proof
of consumption.

Desktop IPC `automation_action` and desktop/headless HTTP `/automations/action`
share one Rust action API for list/get/create/update/delete, atomic pause/resume,
Run Now, paginated retained run history, previews/presets and `24h`/`7d` aggregates.
The Automations dialog uses the registered command mapping in browser/PWA mode.
Definitions and immutable run snapshots belong to the addressed instance. Run
Now requires its runtime owner with no remote fallback. Execution remains
Once-only in phase 1; previews and storage also support recurring cron.

Automation completion core (#1616-1882): task, known PTY exit and durable progress
provide result provenance. Idle is not success; lost completion is unknown.
Persisted deadlines include precheck and needs-you time, stop only owned sessions,
and preserve immutable terminal history with 256 KiB output snapshots. Transitions
use desktop and SSE dual-emission; boot interrupts open runs without retry.

CLI directory opens and MCP `repo action=add` share server-owned repository registration over local IPC. They persist and activate canonical Git or shell directories without an OS URL handler, preserve existing workspace and terminal state on repeated calls, and broadcast configuration changes. MCP `ui tab` rejects `tuic://open-repo` URLs and directs callers to `repo add`.

## Automation notices (#1620-82ae)

Live needs_you/failed transitions claim each delivery channel durably before effects. Existing native question/error preferences, Web Push opt-in and owning-agent Telegram registration apply. No historical or ambiguous send replay; delivery does not mutate execution. Bell/native macOS clicks resolve the canonical run, even after definition deletion. Bell delivery is an unacknowledged live event, not proof of client display.
The Automations editor exposes **Include precheck stdout in the agent prompt**,
off by default, using the same saved definition flag (#1624-c5f5).
Automation precheck context (#1631-45bf): definitions can opt in with
`precheck.inject_output: true` (default false). Proceeding checks prefix the
literal prompt with delimited, escaped untrusted stdout capped at 16 KiB;
truncation is marked and stderr is excluded. Empty output and manual bypasses
inject nothing. The run ledger records `precheck_context: {injected, bytes}` for
the prepared launch prompt, counting escaped stdout bytes before framing/marker.

## Automation Runs View (#1619-b84d)

The Runs tab displays paginated immutable history across definitions, including deleted definitions, and backend 24h/7d elapsed-window summaries. View run focuses only a live terminal owned by the addressed daemon. Absent, suspended or exited sessions display saved bounded output, truncation evidence and recorded workspace details without creating sessions or workspaces. The dialog captures the active repository's daemon owner when opened. Visual verification remains pending because headless Chrome screenshot attempts timed out.

## Core account registry status

A1 implements the shared account/site registry, generic token-only provider registration/verification, write-only account action transport and GitHub migration. The core retains GitHub provider/binding semantics. Account reads and OAuth replies contain no tokens. Settings catalogue aggregate registration, private native secret-entry UI and plugin manifest/authenticated HTTP wiring remain separate assigned integration steps.

Account token entry: `account_secret_form` / `POST /accounts/secret-form` launches
a metadata-only add/rotate request. Only the active private native window can
submit its nonce and token; Rust verifies and stores it through the account owner
service. Replies contain status only. Account tokens are excluded from general
secret-command values, HTTP form bootstrap, and mobile submission.

Closing an account form before Submit cancels without writing. An accepted
submission finishes verification and storage even if its window closes.

## Tasks contract foundation

The provider-neutral work-item DTOs live in `src-tauri/src/work_items`; TypeScript is generated by `scripts/generate-work-item-types.py`. Provider plugins supply data and actions; core owns rendering. A local synthetic provider supports the forthcoming Tasks views, without external API assumptions or transport registration.

## Tasks detail — Component implemented (#1687-27a3)

The generic Tasks detail uses the A4 provider contract for header, editable
fields, description, all comment sources and pages, comment creation and status
actions. Provider text uses sanitized Markdown without provider CSS. Rejected
actions keep authoritative state and comment drafts. Visible details refresh
every 60 seconds; item generations fence stale responses and disclosure state
survives refresh. Tasks shell mounting is a separate integration step.

Automations agent API parity (#1630-cbfa, #1621-5dce): MCP and CLI now call the
Step 8 API. The same action JSON controls definitions, enabled-only updates,
Once previews, run history, summaries and Run Now on the addressed backend.
MCP create requires host-bound creator provenance. CLI exposes these actions as
`tuic automations '<JSON>'` or stdin via `-`; results and validation text match.

GitHub line review backend supports paginated thread/comment reads, reply, resolve/unresolve and head-pinned draft submission on both transports. The PR Files tab (`PrDiffTab`) shows the threads on the existing diff, with replies, resolution and a head-pinned draft review.

GitHub PR action backend supports failed-job reruns, head-pinned auto-merge and queue entry, queue removal/state and active-rule-aware merge. Legacy review submission validates REQUEST_CHANGES bodies. Frontend action integration remains separate.

## GitHub Work adapter — integration pending (#1693-a816)

The data-only GitHub adapter uses an injected account-scoped fetch port and the A4 contract. REST fixtures cover search, issue/PR identity, details, comments and repository create metadata. Authenticated host fetch wiring, lifecycle, manifest and catalog registration are present. Duplicate-picker integration and B6/B7 binding remain pending. Projects v2 success-path evidence requires `read:project`; no successful Projects response has been recorded.

## Tasks create — Component implemented (#1688-518d)

The generic Tasks create form reads item types from the `itemType` enum field of
an untyped `createFields()`, then loads the schema of the chosen type. A type
change keeps only same-id, same-kind values and drops late schemas of an earlier
type. Create stays disabled until Title, Type and every required field hold a
valid value; it dispatches one `create` action and hands the returned item to the
host to open. A rejection keeps the draft. Tasks shell mounting is separate.


Automation worktree cleanup (#1623-8b2a): Automation runs retain automation/run IDs and the exact created workspace path and ID in their saved snapshot, including after definition deletion. Worktree cleanup is explicit and limited to final runs. The backend resolves the saved identity and rechecks shared Git integration/archive proof, dirt, submodule commits and live sessions; it never forces removal or closes sessions. Shared `automation_action` actions `worktree_preview` and `cleanup_worktree` accept a saved run `id` on IPC, HTTP, MCP and CLI.

### Guided event configuration (#2012-72ff)

Three built-in Smart Prompts guide issue-to-draft-PR, incoming PR review and authorized bounded-fix configuration. They open in the composer for review and ask for owner/profile/GitHub/skill prerequisites, confirmed policy limits and supported host capabilities. They do not add a GitHub event runtime or grant publication authority; unavailable intake/publication stops with a draft policy. The after-beta live pilot is not yet verified. See [Guided event configuration](docs/user-guide/event-driven-templates.md).

### Built-in automation templates

New automation drafts offer weekday repository audit, release readiness, daily
change review and hourly maintenance review templates. Enter the repository and
run config, then select **Apply template**. Rust supplies the literal prompt,
cadence, limits and zone (the backend host's local IANA zone unless selected).
Review and edit the draft, then select **Save**. Templates start paused; applying
one never persists a definition or starts a run. **Resume** is a separate action.
Templates use the existing checkout with no precheck and do not change agent
permissions. Recurring cadence previews retain the existing runtime limitations.


### Automatic agent toast cards (#1760-bab7)

MCP toast notices show a transient card only while their origin terminal tab is active. Settings → Notifications → **Show cards from the active terminal** defaults on and is saved per client; turning it off restores urgent-only cards. Urgent notices always show a card, and notices without an origin session remain bell-only unless urgent. The listener reads `terminalsStore.state.activeId` at arrival; `notificationsStore.state.autoToastCards` is stored under `tui-commander-auto-toast-cards` in localStorage. Bell deduplication, sound and terminal navigation are preserved.

## Finished worktree recycling

Use a durable per-checkout allocation marker. Creation reserves the checkout before
returning it. Explicit archive or safe removal marks it finished. Reuse requires fresh
clean and integration evidence and no live session. Rename the registered checkout and
repair Git and submodule paths; preserve ignored artifacts and publish lifecycle events.

## Beta update channel (#1813-4bdf)

The Beta channel checks a rolling signed manifest published only after the versioned beta release finishes. It also checks stable and selects the newer version, so beta users receive the final release. Stable users retain the stable endpoint. New beta installs default to Beta; explicit saved settings are preserved. Beta installers stay on the immutable versioned release and open in the browser; stable installs use the existing in-app updater.

### Repository settings owner coverage

Implemented internal `repo_defaults` and `repo_local` adapters (#1825-4566). Repository targets resolve through one owner per store, with existing fixed-target adapters unchanged. `.tuic.json` writes use the shared locked file owner; arbitrary unregistered targets are rejected. Transport cutover remains pending. `repositories_registration` is an accepted inventory gap until registration/group runtime identities and delta-save contracts have a typed owner schema.

GitHub settings owner adapter (1774-ce8f): internal typed read/perform actions use
the existing GitHub/account owners for authentication, credential handling and
removal effects. Bindings require a registered repository, matching remote host
and a current binding revision. Reads expose presence and runtime provenance.
The shared settings transport cutover and refresh publisher remain separate steps.

## Ego settings owner adapters (#1775-330c)

The internal `ego` settings store is bound to one configured profile (or the user
base). Managed keys use the trusted Ego CLI with the AI Chat workspace as cwd,
then read back stored values. Provider selection is the model slug prefix.
Credential reads expose stored presence only through `ego auth status`.
Managed keys are `model`, `roots` and `network`. Launch defaults stay in the
`app` store; executable writes await Step 6 confirmation. Existing ACP/PTY sessions
keep their launch state. Transport publication remains a later integration step.
Unsupported auth overrides and exec_paths stay in the owner-gap ledger. Multi-key
changes are refused before writing; cross-process conditional writes await an Ego
owner API. These are explicit limits rather than raw-file fallback paths.

Tasks field schemas declare sort, filter and group capabilities; display-only fields disable all three while roadmap dates remain available. Setup edits preserve account keys outside the form and clear blank optional form fields. Create-form filter choices require exact field IDs.

Tasks board paging (#1844-858d): lane headers show loaded counts, with More controls for the provider’s shared board cursor. Appended pages deduplicate cards by provider/account/item identity. Cards show assignees and labels; a Move button opens legal destination actions with keyboard navigation and rollback on failure. Refresh after a move merges page one into loaded cards; later pages remain cached until reload. Scrolling inside the move menu keeps it open.

## Tasks view memory

Tasks remembers the selected source per repository and List, Board or Roadmap plus list grouping, sort, preset, search and filters per repository/provider/account. The existing `uiStore` saves these preferences through `load_ui_prefs` and `save_ui_prefs` in `ui-prefs.json`. Results and page cursors remain transient. A rejected saved sort retries the last accepted order, then the provider default. Other query failures keep their error and Retry control. An unsaved scope keeps the normal defaults.

## Tasks MCP access (#1854-4960)

The MCP repo tool adds `work_item` and `work_item_act`. Rust resolves the caller's worktree link or an explicit reference, URL or account-scoped key, validates DTOs and gates comment/transition writes with `mcp_work_item_writes` (default false). A typed event/claim/reply bridge dispatches to existing frontend provider ports. Claims prevent duplicate actions across frontend clients. Detail and paged comments return as JSON inside the start-work untrusted-content block; forged tags are escaped. Account credentials remain in the existing Rust HTTP injection/redaction boundary. Raw provider errors do not reach MCP. Providers require a connected frontend; a bare headless daemon reports timeout. A timed-out write must be read back before retrying.

### Mobile desktop Markdown tabs (#1879-05ad)

Implemented: session overflow lists the desktop’s open file-based Markdown tabs
for the registered session repository, with titles and relative paths. Entries
open the existing mobile viewer from their original workspace. A temporary Rust
snapshot receives desktop metadata; the read-only HTTP list shares remote auth,
and SSE invalidation updates open menus without a page reload. Virtual/plugin
panels and document contents are excluded. The iPhone visual check remains manual.

### Background tray and menu-bar icon

Background tray (B3): implemented. Native tray/menu-bar status reads current backend SessionState on transitions, with Show and explicit Quit using the shared background lifecycle. Tray preference defaults to background_only; never prevents hiding. Missing native tray support minimizes instead. Native template appearance and desktop-host integration require verification after a rebuild; WebView unload and hibernation remain separate work.

Tasks list controls use provider-declared field sort, group and filter capabilities.
A rejected selection restores the last successful sort, group and filter controls, keeps attempted search text for Retry, and shows the provider reason.

Tasks small-list mode (#1851-7ade): when the provider reports at most 1,000 matches, Tasks collects the complete search/preset scope and uses the Rust projection for all-field sort, group and filter. A changed total, missing rows or a nonterminal cursor cannot establish completeness. Unknown totals and larger lists keep provider-supported server controls. No extra cache or persisted state is added.

GitHub Tasks boards (#1846-55d7): the optional provider `listBoards(signal)` port discovers the current user Projects v2 when Board opens. The existing picker selects a project; paging preserves project identities. Missing `read:project` stays visible instead of claiming there are no boards. Providers without discovery retain their configured boards. Discovery does not persist selections or write to GitHub.

Design Mode agent browser commands use bounded per-repository native-port requests
and UUID replies, with ten-second timeouts and Stop cancellation. The extension
owns reviewed-tab and host-permission checks. Eval needs explicit optional-origin
consent, including all-sites consent. The legacy heartbeat restart identity and
navigation sequences remain unchanged. Browser screenshot/client-refresh reality
verification is still required before story 1724-fe7e can close.

### Rust-owned suspended tabs

Tab suspension is owned by Rust and remains available with no UI attached. Durable config records keyed by `tuic_session` survive document recreation and process restart, retaining agent/profile, cwd/worktree, alias and pane placement. Rust verifies resume commands and opens the replacement PTY; the frontend renders records and attaches the returned session. Lifecycle events are available over IPC and SSE.

## Plugin settings pages

Loaded plugins with registered settings appear under Settings > Integrations. Search includes their catalogue field labels. Unloading removes the page and search entries. Reads, revision-checked edits, resets and secret set/unset status use the scoped Rust settings service; the UI owns no second configuration store.

### Tasks notifications

Settings → Notifications → Tasks has **OS notifications for Tasks**, off by default and independent of the existing PR notification preference. Failed checks on a linked GitHub PR use the existing PR notice once and name the linked Tasks item. Click the notice to open the PR.

Use **Follow task** in an item detail to follow comments locally; use **Unfollow task** to stop. Follow state and the policy are saved in the existing Tasks UI preferences. A first observation establishes a silent baseline. Existing visible detail refreshes (including its 60-second refresh) and provider-bridge detail reads compare authoritative comments; there is no extra background poller. Your own comment action advances the baseline silently. Native notices recheck the Tasks policy after permission is granted and retain the existing background-window policy; browser clients do not send native OS notices.

GitHub provides a full normal-comment count, so an increase is detected even beyond the first page. Jira and GitLab provide paged comment entries: newly observed comments notify when their creation time is newer than the prior baseline, or when a previously unseen comment ID shares its latest timestamp. Unloaded pages, unseen review sources and items that are not refreshed cannot produce a notice. Editing a comment or loading older history does not notify. No Linear provider is shipped, so it is not covered.


Shared wake (B6, #1807-4f1f): `session_wake::ensure_live` serializes durable resume with suspension. Tab open, submit and peer mail use this path; input waits in the existing ready-window queue. Failed creation retains the durable tab. IPC `resume_session` maps to `POST /sessions/wake`; MCP list/status expose retained sessions as `hibernated`.

## ego terminal Chat view

ego PTY terminals use the existing Chat adapter registry, bounded view log,
one-second ticker, IPC/HTTP snapshot and SSE wake. Exact binding comes from the
live foreground PID and ego's lease, with its process-owned `EGO_HOME`; there is
no newest-session fallback. Content is read only through documented local
transcript v1: `ego export SESSION --format transcript --after SEQ`.
The configured absolute ego executable must support this contract. Header version
and session identity are checked before events are accepted. Completed messages
render once (preview deltas are omitted); tools retain their start/result identity.
Goal continuations and compaction checkpoints are notices. Rebinding resets the
epoch, including compaction successors. Unbound or unreadable sessions explain why
Chat is unavailable. Desktop and mobile reuse `TerminalChatTranscript`; no ACP
connection or second event subscription is created. Compose, dictation targeting
and answer marker rendering remain on the existing terminal paths.

## Mobile terminal Chat view

The terminal stays mounted. Chat edit tool disclosures render bounded
old/new blocks, preserve tabs with four-space tab stops and scroll long lines
horizontally. Claude Edit projection retains diff content through tool completion;
failed calls remain labelled failed, not represented as applied edits.

Claude, Grok, Codex and ego PTY sessions offer CLI / Chat in the existing mobile header. The shared
`TerminalChatTranscript` owns rendering, visibility-paused keepalive and backend
unavailability. Mobile injects wakes from `useSessions`; it opens no additional
EventSource. Both hosts defer Claude binding to the backend. Mobile also defers Codex exact-hook and Grok registry binding to `chat_view_snapshot`; an unbound response returns to CLI with its reason. Desktop and mobile share the supported-adapter predicate. Mobile retains the
existing composer and mounted hidden output, returning CLI to the bottom.
Transcript follow pauses during touch and detaches on upward movement.

### Automatic idle-agent hibernation

The backend checks unattended top-level agent tabs every five seconds. `agents.hibernate_idle_minutes` defaults to 60 awake minutes; zero disables the sweep. `agents.hibernate_scope` defaults to `background_only`; `always` also permits foreground hibernation. Both machine-owned controls are in Settings → Agents (Expert Mode).

Hibernation requires an idle agent and a verified resume session in its saved profile. Plain shells and agents without a verifiable resume session stay live. Unread peer mail, pending commands or background work, awaiting input, attached readers, keep-open tabs and open automation runs prevent hibernation. Managed children retain their separate idle-close policy. Sleep gaps longer than 30 seconds do not advance the idle clock.

The normal Rust suspension path persists `reason: "idle_hibernate"` before ending the PTY. Opening the tab, submitting input or sending peer mail uses the shared wake path. Storage or resume-verification failures leave the live session intact.

GitHub Work B8: registered accounts bind core B6 review threads and B7 actions through account-scoped PluginHost methods. The Rust adapter enforces net:http, on-disk GitHub provider consent and matching account/PR identity, then uses existing core policy. Projects move/clear coverage uses recorded public reads and documented mutation contracts; live writes and desktop verification remain pending.

## Settings transport cutover (#1787-4778)

MCP settings, POST /settings/action and IPC settings_action share generated Rust requests and owner dispatch. Config has no MCP compatibility alias. The complete owner inventory permits transactions. Typed workflows cover GitHub, Ego provider auth, upstream credentials, connection credentials, Telegram and available voice assets. Owner choice refresh uses explicit target/store/paths; describe remains cache-only. Live frontend refresh remains separate dependent work.

## Settings choice discovery (#1908-c327)

Internal owner requests now support cached dynamic-choice describe and explicit refresh, scoped by target and revision. Notification output devices reuse the UI enumeration owner and preserve unavailable versus successful empty results. General settings transport publication remains separate.


Agent model settings support typed model edits without starting a CLI. Explicit
choice refresh probes only the addressed Claude run config with `--help`, using
its command and environment. Published aliases are suggestions, not account
entitlement or an exhaustive model list. Other CLIs and failed probes report
unavailable choices; typed edits remain supported. Describe never probes. Model
edits preserve sibling configs and the default selection and apply next session
(#1910-451e).
### Session attachment launch grants (#1833-aa60)

Gemini receives only the session upload directory through `--include-directories`.
A session-owned trusted-folders overlay preserves user trust decisions and adds
that directory, without changing global settings or bypassing workspace trust.
Ego PTY launches use `--read-only-dir` for the same canonical directory. Shell
shims apply the grants to typed launches; resumed ego sessions receive fresh
read roots because discovery does not persist ego argv. ACP uploads still require an existing profile read root.

Plugin HTTP cancellation: a host-local AbortSignal maps to a generated request UUID scoped to the plugin. Shared IPC/HTTP helpers cancel the full Rust request future, including headers and body. Jira URL resolver cancellation reaches all detail requests.

### Voice owner discovery (#1909-0d24)

Internal voice settings refresh resolves devices, model state, installed voices and language-filtered Edge voices through existing owners. Describe is cache-only; typed asset actions retain existing owner validation and runtime effects. Public settings transport cutover remains separate.

### Settings restyle foundation (1936-0caa)

The approved 2026-10-16 Settings restyle uses a stable 1136px maximum shell and shared presentation-only vocabulary across Application, Workspace, AI, Integrations, loaded plugin pages and repository pages. Existing lazy mounting, persisted navigation width, configuration sources and save boundaries remain unchanged. Search uses static and loaded-plugin catalogues without mounting all pages; expert results reveal the requested control without changing the persisted Expert preference. Section links wait for the selected page to mount and retain retired-tab mappings. Plugin loading/read failures stay inside the editor, and unloaded plugins cannot retain editors or search entries. S10 adds integrated boundary coverage and documentation; Commander/Paper visual comparison remains a separate capture gate.

### Terminal ego provider recovery

Terminal ego OSC 7770 recovery snapshots expose phase, attempt and next-attempt
time through the existing session lifecycle snapshot. The tab and status bar
show recovery and offer `/retry`; MCP session `force_retry` provides the same
single-command control. Entry/exit notices reach managed parents. Explicit clear,
busy/idle markers and shell OSC 133 A retract recovery. This is PTY-only and does
not attach ACP to a terminal process. Wire limits are in docs/backend/output-parser.md.

Telegram Settings reads a process-local in-memory ring of the last 20 events through the existing status endpoint. Time, direction, kind, safe outcome and a redacted preview of at most 60 Unicode characters are shown newest first. Pairing input and unauthorized text are never retained. The ring is not persisted and clears on process restart; it is not shared across owner/standby processes.

## Persistent stall evidence (1638-c94c)

The backend retains fixed-kind numeric slow-event metadata in a bounded journal independent of the log ring. Frontend slow structural batches include body/flush time, topology counts and response size while perfDebug is enabled. Backend heartbeat-gap records include receipt and observation timestamps. The count-based fixture builder emits synthetic isolated inputs; it does not satisfy the original captured-workload RED/GREEN criteria. See `docs/backend/stall-journal.md` for retention, privacy, timing semantics and replay limits.

### Telegram request streaming (1959-162a)

Telegram replies now use TUIC-driven receipt (👀), working (👨‍💻), waiting (🤔), answered (👍) and failed (👎) reactions. An ephemeral native draft shows Italian status lines or unapproved agent previews; it refreshes while work continues and the final reply replaces it. Acknowledgement of work requires inbox Read and a new recipient turn. PTY intent/activity and AI Chat assistant chunks share request correlation; finish remains authoritative after draft retirement. Telegram-mail-woken AI Chat turns automatically send their settled assistant text if finish has not answered. Queue and no-reply notices occur at ten minutes and sixty seconds after Read respectively. Restart keeps no request history. Phone rendering requires a live check after the backend restart.

### Mobile session Chat presentation (#1974-e6ae)

The session detail CLI/Chat projection uses the same TerminalChatTranscript and semantic Chat tokens as desktop terminal Chat. Shared font sizes cover prompt, reply, tool and hint text. The mobile switch keeps 44px targets and the existing CommandInput and keybar; no new terminal row is added. AI Chat is outside this change.

### Mobile Telegram setup removal (#1976-711e)

Mobile Settings does not mount or offer Telegram setup. Desktop Settings retains the shared Telegram configuration controls and existing backend transports.

### Settings MCP live refresh contract

The shared settings facade publishes `{section, generation, persisted, applied}` after durable writes, including vault-only and partial-persist outcomes. The monotonic counter is process-local, is never persisted, and does not replace owner CAS revisions or hash credentials. Desktop and SSE carry the same value-free payload. Frontend read/apply coordination preserves dirty intent and acknowledged baselines, suppresses obsolete writes and stale reads, scopes remote events by connection, and refetches active owners after reconnect.

## Settings UI catalogue contract (#1789-796d)

Controls carry Rust catalogue IDs, including raw inputs, dynamic sound rows, composite record editors, platform and expert branches. Shared selects derive static options from descriptors. The typed All settings inspector covers writable scalar/string-list descriptors, with concrete stable-key paths and explicit machine/instance/repository/profile targets. It uses shared settings_action reads and revision-checked edits; it never edits arbitrary JSON. Existing owner workflows retain credentials and structured records. Per-ID exclusion ledgers require review; CircleCI/corrections catalogue ownership and transient PR-filter persistence are tracked by 1991-bd88, 1992-d4fa and 1993-e7af.

Frontend plugin shortcut IDs publish runtime metadata through the shared action endpoint. Registration/unload/reconnect update the current set without durable writes. Plugin-action choice discovery remains unavailable until publication. Source parity and renderer tests are authored; batch/runtime acceptance is recorded separately from implementation.

## Settings startup contract status (#1790-1011)

Published workflows and recovery are documented in `docs/backend/settings-startup.md`. Typed account enumeration and configured Ego interactive login start/status are published. The login gives the person a tracked PTY; closed is not authentication success, and credential readback comes from the configured owner. Step 10 remains incomplete until schema-only startup and refusal/expiry/conflict/apply-failure integration are verified; native inventory and compile checks alone are insufficient.

### CircleCI Settings owner (#1991-bd88)

Settings discovery includes the write-only machine setting `circleci.token`.
Typed `set_token` and `clear_token` actions share the existing token owner across
MCP, IPC and HTTP. Readback reports stored presence only (`set`, `unset`,
`unavailable`), never a secret. Git & GitHub keeps its effective-source status;
removing a stored token leaves environment and CircleCI CLI fallbacks unchanged.

### Automation lean creation (#2001-5c1f)

Bound agents can create with name, prompt, in_secs and an explicit run_config.
The shared API resolves elapsed delay using the host clock and uses only
caller-specific repository defaults. Actual saved profile identity is not retained
in current caller metadata, so missing run_config fails instead of substituting
an agent-type default. Generated IDs hash caller identity and the original request;
identical retries atomically reuse the original occurrence. A separate identical
reminder needs a new explicit id. Schedule choices are exclusive. Create returns the saved definition and next_run_ms; once_local and
once_at preserve both the selected zone's local display and the exact UTC instant.
Unbound HTTP/IPC callers still supply explicit definition fields.

## Durable managed launch (2006-f5a1)

Managed workers can launch through one keyed transaction across MCP, HTTP and IPC. The journal reserves the child identity before workspace or PTY side effects, retains key tombstones, and refuses conflicting reuse. A retry returns the same identity; ambiguous interrupted creation blocks without a replacement or fallback checkout. Existing/new workspaces, a stable name, a registered parent and explicit keep-open policy are supported. Receipts separate prompt transport delivery from model acceptance, require native Ready-screen evidence for ready, and expose warm-artifact state independently. Model/account/brief policy stays outside TUIC.

Native automation and workflow run bindings also use this transaction through `managed_launch`: automation run IDs and workflow attempt IDs provide stable keys, with an in-process run-owner scope when no peer parent exists. No synthetic peer is registered. Run-owned children use the same pinned-name and explicit keep-open path as MCP children; the existing run controller still owns cancellation and completion. The unkeyed public `spawn` compatibility action continues to use the same low-level PTY spawn implementation.
## Frame workload evidence (#1638-c94c)

Implemented: opt-in, perfDebug-gated global bounded frame bytes and paint snapshots, rotated instance-local captures, manual debug dump and production grid-paint replay timing. The synthetic smoke fixture checks harness wiring only. Original captured-stall heartbeat/input/backend RED and same-workload GREEN remain open. See docs/backend/stall-journal.md for bounds and privacy.

Dictation corrections use the machine-owned `corrections` Settings collection,
keyed by heard text. The existing TextCorrector map remains the stored format;
Settings and legacy transports share locked atomic persistence, runtime adoption
and value-free refresh hints. Non-dictation builds report unavailable descriptors
(#1992-d4fa).

MCP lazy discovery returns an optional complete top_schema, bounded by schema_byte_budget (default 16384, maximum 65536 UTF-8 JSON bytes), and catalogue_generation. Clients invalidate admitted schemas on tools/list_changed or changed generation (#2013-73f9).

## Bounded MCP runtime outcomes (#2010-30a7)

HTTP and ACP tools/call share a body-free 1024-record instance-local audit ring;
`GET /mcp/audit` projects the newest 100 rows. Explicit call/parent UUIDs, recognized
operations, target digests and UTF-8 text-byte costs support runtime correlation.
Application error, transport observation and effect certainty stay separate;
reply construction is not delivery proof and cancellation does not imply rollback.
Real ego/Codex correlation and captured lost-reply traces remain unverified.

## Managed MCP stateless identity (#2008-5317)

Modern tools/call uses an internal routing context for an exact live PTY/ACP peer
asserted by the host header, without adding legacy session state to the wire.
Unknown peers remain unbound. Missing-context diagnostics prohibit unchanged
retries and require host reconnect or explicit external initialize/register.
Consumer retry enforcement remains a separate live ego verification requirement.

## Mobile device-local font sizes

Implemented (#2023-54c4): mobile Settings has separate Terminal and Chat controls with smaller/larger buttons and current values, without explanatory text. Each preference has its own localStorage key and 10–24 px range with 1 px steps. The existing defaults stay 14 px for CLI output and Chat body text. Chat changes apply only within Session Chat and AI Chat transcripts. Composers, UI chrome, desktop fonts and other devices are unchanged. CLI rows rewrap locally without resizing the shared PTY and keep the visible row when scrolled back.

Mobile offers the **Chat** tab only when the host has a configured ego executable, using the same availability predicate as desktop. Host settings changes update the tab without reloading. Removing ego configuration returns an open Chat view to **Sessions**; unavailable saved Chat links also stay on **Sessions**.

### Abandoned shells

Desktop sidebar session rows and mobile session cards mark a parentless plain shell as **Abandoned** after one idle hour and show its idle age. The five-second backend sweep requires a fresh OS process inventory: the PTY child must be an interactive shell with no child processes. An unknown inventory, script, foreground command, background child, detached TUIC job, managed identity, live child session, pending input or keep-open hold prevents closure. Unsupported or incomplete OS process observations fail closed.

Settings → Agents → Idle agent hibernation (Expert Mode) includes **Close abandoned shells after (hours)** (`agents.abandoned_shell_hours`). The machine-owned value defaults to **0 (off)**; whole hours from 1 to 65535 enable closure. Idle age is measured from the most recent input, output or semantic activity, including elapsed time while the machine sleeps. Closure rechecks the process inventory and activity at the destructive boundary, and uses the normal session close/tombstone path. The sweep can lag the threshold by one five-second tick. A remote daemon applies its own setting, not the desktop host's value.

GitHub Settings preserves per-alias API access errors by owner/account. Only explicit root NOT_FOUND responses receive the 24-hour exclusion; other denials retry on the normal poll and clear on success.

### Detached checkout inventory

Structure and summary snapshots include a separate detached-checkout vector (path, HEAD SHA, short SHA, directory name, detached state). The sidebar renders it without branch selection or actions and retains a header count when folded. Existing branch-workspace IDs and rich idle-session folding stay unchanged. Discovery excludes the configured TUIC archive pool, not every directory named `__archived`.

### Session-owned mobile usage (#2150-4e0c)

`get_session_usage(session_id)` / `GET /sessions/{id}/usage` returns display-ready
Rust lines with agent, verified model when available, scope, source, availability,
optional observation time and a stale flag. The overflow fetches on open and drops
responses after its session changes or it closes. Unknown values are not zero.
Claude uses profile-bound cache or its primary non-generating OAuth usage GET.
Codex quota uses the live process home/profile and App Server rate-limits read;
unbound configuration overrides are unavailable. ego uses exact PID/store/session
binding and its v3 report, not workspace totals or native status text. The model
is the latest admitted model, while session totals can span multiple models.
Remote PTYs route to their owner or return unavailable with reason `remote session`.
ACP usage is unchanged.

Worktree discovery preserves existing branch workspace IDs. Forced duplicates use a deterministic primary checkout and read-only diagnostics for additional paths. Missing registrations are non-selectable with repair guidance, never auto-pruned by discovery. Cached discovery errors remain errors. Bisect and relative-gitdir rebase retain the branch recorded by Git.

Unavailable detached registrations are excluded from automatic orphan cleanup in both Ask and On modes, including the Ask countdown. They remain repair diagnostics until restored or explicitly removed. Safe-only removal rechecks availability, so a directory moved after assessment cannot be pruned automatically. Explicit user-confirmed removal remains available.

Native story notes: the shared project-scoped story service appends immutable commentary with backend actor provenance, Unix-millisecond time and the current subject revision. Notes do not advance story revisions. Optional workflow evidence stores validated `{runId, commandId}` references to persisted receipts in the same project and plan, not copied payloads. Story detail lists notes with author/time/revision provenance and receipt IDs, provides an append-only add field, and has an explicit Refresh notes control for external additions. Drafts and pending replies stay scoped to the selected story.
