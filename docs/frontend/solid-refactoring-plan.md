# Solid Frontend Refactoring Plan

**Status:** Work units 1–7 implemented and validated  
**Baseline commit:** `65c653c8`  
**Worktree:** `refactor/solid-architecture`

## Purpose

This document maps the current SolidJS frontend and defines an incremental refactoring program. The work is intended to improve maintainability, test isolation, load performance, and runtime diagnosability without changing product behavior or replacing SolidJS.

The refactoring is not a line-count reduction exercise. A smaller file is useful only when it establishes a real ownership boundary, removes a dependency, isolates a lifecycle, or enables deferred loading.

## Goals

- Make `App.tsx` a composition root instead of the owner of unrelated application lifecycles.
- Establish feature boundaries that can be changed and tested independently.
- Keep expensive, optional UI code out of the initial desktop and mobile load paths.
- Preserve the current direct canvas terminal hot path and its performance invariants.
- Reduce runtime dependency cycles and implicit store-to-store coupling.
- Keep each change behavior-preserving, independently testable, and independently revertible.

## Non-goals

- Replacing SolidJS, Tauri, or the canvas terminal.
- Redesigning the UI or changing user-visible behavior.
- Moving backend business logic into the frontend.
- Rewriting all stores or introducing a new state-management framework.
- Creating generic abstractions before a current feature needs them.
- Combining feature work with structural refactoring.

## Measured Baseline

Measurements were taken from the baseline commit after a clean `pnpm install --frozen-lockfile` and `pnpm build`.

### Source inventory

| Category | Files | Lines |
| --- | ---: | ---: |
| Production TypeScript | 270 | 42,848 |
| Production TSX | 165 | 52,705 |
| Production CSS | 117 | 23,447 |
| Test and test-support sources | 285 | 61,948 |
| Total frontend | 837 | 180,948 |

The production frontend contains 393 `createSignal`, 142 `createMemo`, 190 `createEffect`, 51 `onMount`, and 144 `onCleanup` call sites. These totals are not defects by themselves; they identify the amount of lifecycle behavior that must remain observable while modules are moved.

### Build and test baseline

- `pnpm build`: pass, 13.17 seconds in the measured run.
- `pnpm test --run`: 281 test files and 4,651 tests passed.
- The first test run exposed an uninitialized `plugins` submodule. The referenced commit was no longer fetchable from the remote, so the exact commit was transferred from the primary local checkout before rerunning the suite.

### Initial load baseline

| Entry | Initial JS/CSS payload | Gzip payload |
| --- | ---: | ---: |
| Desktop `index.html` | 4,589,184 bytes | 1,419,904 bytes |
| Mobile `mobile.html` | 1,819,210 bytes | 610,692 bytes |

The desktop preload graph currently includes:

- main application: 399,406 bytes gzip;
- CodeMirror: 543,046 bytes gzip;
- diff viewer: 308,732 bytes gzip;
- Markdown parser/sanitizer: 42,452 bytes gzip;
- shared application and transport chunks.

CodeMirror and the diff viewer are emitted as separate chunks, but they are static dependencies of the desktop entry and are therefore still preloaded at startup. The mobile entry also preloads the CodeMirror chunk despite having no source-level path to the editor. The multi-entry chunk/preload arrangement must be corrected rather than merely adding more `manualChunks` entries.

The build also reports ineffective dynamic imports for `openUrl.ts`, `dragDrop.ts`, and `useFileDrop.ts` because the same modules remain statically imported elsewhere.

## Current Architecture Map

```text
index.tsx
  -> App.tsx
       -> application bootstrap and hydration
       -> native-window and browser-mode branching
       -> global event listeners and detached-window synchronization
       -> terminal lifecycle and completion notifications
       -> repository, branch, worktree, and Git operations
       -> action registry, keyboard shortcuts, and native menu dispatch
       -> dialogs, overlays, panels, and application layout
       -> plugin initialization and context-action registration
       -> TerminalArea
            -> terminal canvas
            -> editor, diff, Markdown, HTML, and plugin tabs

mobile/index.tsx
  -> MobileApp
       -> HTTP/WebSocket transport
       -> mobile session, activity, command, and settings views

invoke.ts
  -> native Tauri invoke/listen facade
  -> transport.ts for browser and remote operation

transport.ts
  -> IPC-to-HTTP command mapping
  -> HTTP request execution
  -> PTY WebSocket subscriptions
  -> application SSE subscriptions
```

The desktop and mobile entries correctly share stores and transport contracts, but optional desktop views currently leak into the startup graph.

## Coupling Hotspots

The fan-in and fan-out values below count internal TypeScript/TSX module dependencies. Type-only imports are excluded from the runtime-cycle list.

| Module | Lines | Fan-out | Notable responsibilities |
| --- | ---: | ---: | --- |
| `src/App.tsx` | 3,053 | 132 | bootstrap, events, actions, notifications, panels, dialogs, layout |
| `src/components/Terminal/CanvasTerminal.tsx` | 3,288 | 24 | frame lifecycle, canvas paint, input, selection, links, search, scrolling, DOM lifecycle |
| `src/transport.ts` | 2,374 | — | command map, HTTP, WebSocket, SSE; imported by 53 modules |
| `src/hooks/useGitOperations.ts` | 2,257 | 24 | repository refresh, branch switching, worktrees, merge cleanup, terminal reassignment |
| `src/components/SettingsPanel/tabs/ServicesTab.tsx` | 2,239 | — | local MCP, upstream MCP, bridges, Tailscale, remote machines |
| `src/components/TabBar/TabBar.tsx` | 1,607 | 27 | four tab types, two ordering modes, menus, drag/drop, scrolling, rename |
| `src/components/FileBrowserPanel/FileBrowserPanel.tsx` | 1,519 | 27 | tree state, file operations, pointer/native drag, menus, keyboard navigation |

`App.tsx` is the dominant coupling hotspot: its fan-out of 132 is almost five times the next-highest module. Moving its body into one large `useAppController` would preserve that coupling and is explicitly rejected.

High fan-in modules are legitimate shared boundaries but require stable contracts: `appLogger.ts` (142 importers), `invoke.ts` (108), `repositories.ts` (71), `terminals.ts` (54), and `transport.ts` (53).

## Runtime Dependency Cycles

The production runtime import graph contains three strongly connected components:

1. Transport/store cycle: `tunnels -> perfTrace -> repositories -> remoteConnections -> transport -> appLogger -> invoke`.
2. Sidebar component cycle: `PrSection -> GitHubPanel -> RemoteOnlyPrPopover -> RepoSection`.
3. Appearance component cycle: `AppearanceTab -> ColorSwatchPicker`.

The larger 12-module plugin cycle seen in the unfiltered graph is composed of type-level relationships and is not a runtime cycle. It should not be treated as equivalent to the three cycles above.

Runtime cycles are not automatically bugs, but they make initialization order implicit and make lazy loading less predictable. They should be removed through dependency direction, not barrel-file reshuffling.

## Hotspot Responsibilities and Safe Seams

### `App.tsx`

Existing cohesive seams are visible in the code and can be extracted without inventing new behavior:

- detached-panel registration and event routing;
- application bootstrap, update checks, and deep-link initialization;
- tab activation synchronization across terminal, Markdown, diff, and editor stores;
- completion notification and idle-triggered triage lifecycle;
- plugin context-action registration;
- native window, file-open, and menu event bridges;
- command/action construction;
- dialog and overlay rendering.

Each seam should expose a narrow function or component contract. Store reads should remain inside the owner when possible instead of being forwarded through a large dependency object.

### `CanvasTerminal.tsx`

The terminal already has useful lower-level modules for transport, frame decoding, glyph caching, grid rendering, input encoding, touch input, and timing. The remaining component still owns these distinct lifecycles:

- frame subscription, reconciliation, and resize;
- base/overlay/cursor rendering coordination;
- smooth scrolling, scrollback cache, overscan, and scrollbar interaction;
- selection, copy, and drag auto-scroll;
- link detection, asynchronous path verification, and link menus;
- keyboard, IME, paste, mouse protocol, and touch handling;
- search state and imperative public API;
- resource cleanup.

The hot path must not be converted into reactive store state. Extraction should use explicit controller state and injected narrow ports so frame decoding and paint scheduling remain imperative.

### `useGitOperations.ts`

This hook is already an extraction from `App.tsx`, but its public facade now covers several domains:

- repository refresh and stale-result suppression;
- branch selection and serialized switching;
- worktree creation, recovery, setup, and removal;
- PR autofix, conflict assistance, and merge cleanup;
- terminal creation and worktree reassignment;
- CWD-based terminal ownership tracking.

Its generation counters, FIFO branch-selection queue, creation grace period, and removal deduplication are correctness mechanisms. They must remain owned by the relevant coordinator and receive characterization tests before being moved.

### `ServicesTab.tsx`

The file already contains three UI domains with natural boundaries: local MCP/bridge configuration, upstream MCP servers, and remote machines. These can become sibling components sharing existing setting fields and transport types. This is a low-risk structural extraction.

### `TabBar.tsx`

The grouped and free-order render branches duplicate terminal, diff, Markdown/plugin, and editor tab rendering. Extracting typed tab view components will remove real behavioral duplication while leaving ordering and drag/drop coordination in `TabBar`.

## Target Dependency Direction

```text
entrypoints
  -> application composition
       -> feature controllers
            -> stores and transport ports
       -> feature views
            -> shared UI primitives

backend transport ports
  -> invoke/http/ws/sse adapters

pure models and helpers
  -> no stores, DOM, transport, or UI imports
```

Rules for new boundaries:

1. Components render and bind interaction; coordinators own lifecycle; pure helpers transform values.
2. A feature may depend on shared infrastructure, but shared infrastructure must not import the feature.
3. Transport adapters must not depend on UI stores. Connection lookup and logging should enter through narrow ports.
4. Avoid new barrel imports across feature boundaries when they obscure the concrete dependency.
5. Keep browser/Tauri parity at the existing `invoke` and `transport` boundaries.
6. Do not introduce a new global event bus or state framework.

## Implementation Sequence

Every work unit below must be independently green and revertible. No unit combines a behavior change with structural movement.

### Work unit 1: Correct deferred-loading boundaries

This is the first implementation because it has the clearest measurable runtime benefit and does not require altering application behavior.

- Lazy-load editor and diff views at the tab-content routing boundary.
- Ensure Markdown/Mermaid dependencies load only for content that needs them.
- Remove static imports that make existing dynamic imports ineffective.
- Correct the multi-entry preload graph so the mobile entry does not preload CodeMirror.
- Record desktop and mobile initial preload bytes in an automated build report.

Acceptance criteria:

- `index.html` does not preload CodeMirror or the diff viewer before either feature is opened.
- `mobile.html` does not preload CodeMirror, diff, or desktop-only view code.
- Opening an existing persisted editor or diff tab still works after startup.
- Build and full frontend tests remain green.

Measured result:

| Entry | Baseline gzip | Work unit 1 gzip | Reduction |
| --- | ---: | ---: | ---: |
| Desktop `index.html` | 1,419,904 bytes | 418,607 bytes | 70.5% |
| Mobile `mobile.html` | 610,692 bytes | 67,417 bytes | 89.0% |

The build now fails if an optional editor, diff, Markdown/Mermaid, or compose
asset returns to either initial entry graph. It also enforces 500 KiB and 100
KiB gzip budgets for the desktop and mobile entrypoints respectively, and prints
a `WARNING` (without failing) once an entry reaches 98% of its budget. The
reporting and checks live in `scripts/report-frontend-bundles.mjs`.

Validation performed for work unit 1:

- `pnpm build`: passed, including entry-graph and gzip-budget checks.
- `pnpm vitest --run --maxWorkers=4`: 281 files and 4,651 tests passed.
- The unconstrained Vitest run exhausted the local fork pool; its single timeout
  passed in isolation, and the complete concurrency-limited rerun passed.
- Focused AI chat and terminal tests passed after isolating the lazy Markdown
  renderer in the AI chat unit test.
- The full run also reports three pre-existing asynchronous promise leaks from
  `pluginLoader.test.ts`; they do not fail the suite and are outside this work
  unit.
- `make check`: attempted. TypeScript passed, then the command stopped on three
  pre-existing Biome formatting errors in `useAppInit.ts` and its tests; none of
  those files is changed by this work unit.
- Browser-mode verification is pending because the required
  `brainstorming/x-xcan/ab-stealth.sh` wrapper is absent from both local
  checkouts and the in-app browser runtime failed to initialize. No CSS or
  layout files changed in this work unit. This checkpoint was superseded by the
  final browser validation recorded below.

### Work unit 2: Establish App lifecycle boundaries

Add characterization tests before moving each lifecycle. Extract in this order:

1. tab activation synchronization;
2. completion notifications and idle-triggered triage;
3. detached-panel and native event bridges;
4. plugin context-action registration;
5. bootstrap/update/deep-link lifecycle;
6. dialog/overlay rendering;
7. action and native-menu construction.

`App.tsx` remains responsible for composing the hooks, passing feature callbacks, and laying out the application. The work unit is complete when it no longer implements feature lifecycles directly and its imports describe application-level modules rather than every leaf store.

Progress on 2026-07-21:

- Extracted tab and active-terminal synchronization, completion notifications,
  idle triage, detached-panel routing, file-open and reattach bridges, plugin
  context actions, application bootstrap, native menu dispatch, terminal
  context menus, dialog integrations, automation bridges, appearance and
  system lifecycles, plugin runtime ownership, quick-switcher visibility,
  shortcut registration, dictation hotkeys, shell-exit handling, and
  application shortcut actions into focused hooks.
- `ApplicationOverlays` now accepts eight domain contracts instead of a
  43-property relocation boundary. Git dialogs, confirmation/folder-drop
  dialogs, and post-merge cleanup own focused render groups; none receives an
  entire hook return object. `App.tsx` retains composition and layout ownership,
  and the existing Settings/Help lazy boundaries remain unchanged.
- Added 67 focused characterization tests across 14 files. They cover lifecycle
  registration and cleanup, activation ordering, timer cancellation, native
  event routing, action construction, dialog state, plugin registration, and
  overlay behavior.
- `App.tsx` decreased from the 3,053-line baseline to 1,078 lines and contains
  no direct `createEffect`, `onMount`, `onCleanup`, or native `listen` calls.
- `pnpm vitest --run --maxWorkers=4`: 295 files and 4,718 tests passed without
  leaked-resource reports.
- `pnpm build`: passed in 26.96 seconds, including entry-graph and gzip-budget
  checks. The resulting initial payload is 419,480 desktop gzip bytes and
  67,427 mobile gzip bytes.
- `make check`: TypeScript passed. Biome then stopped on the same three
  pre-existing formatting errors in `useAppInit.ts`, `useAppInit.test.ts`, and
  `tweakComments.test.ts`; targeted checks for the new work pass.
- Browser-mode verification remains pending because the required stealth
  wrapper is absent from both checkouts and the in-app browser runtime could
  not initialize. No CSS or layout behavior changed in this work unit. This
  checkpoint was superseded by the final browser validation recorded below.

### Work unit 3: Split settings service domains

- Extract local MCP and bridge settings.
- Extract upstream MCP server management.
- Extract remote machine management.
- Keep persistence and authorization behavior unchanged.
- Lazy-load service subpanels only if measurement shows a meaningful benefit; file splitting alone is not sufficient justification.

Result on 2026-07-21:

- `ServicesTab` is now a composition boundary for three sibling domains:
  `LocalServicesPanel`, `UpstreamMcpPanel`, and `RemoteMachinesPanel`.
- Upstream configuration, OAuth helpers, persistence, and status polling moved
  together. The existing immediate refresh and three-second cadence are
  preserved, and cleanup ownership is covered by a focused lifecycle test.
- Remote-machine forms, transport presentation, CRUD, and connection controls
  moved together with their existing `remoteConnectionsStore` contract.
- Existing helper imports remain compatible through re-exports from
  `ServicesTab`; no caller or persisted configuration shape changed.
- Three focused files pass 21 tests. The concurrency-limited full suite passes
  297 files and 4,725 tests.
- `pnpm build` passes with 419,482 desktop gzip bytes and 67,427 mobile gzip
  bytes. `make check` reaches the same three pre-existing Biome failures and no
  changed file fails a targeted check.
- The split is intentionally eager: measurement showed no startup benefit that
  would justify lazy-loading settings subpanels.

### Work unit 4: Unify tab rendering

- Introduce typed view components for terminal, diff, Markdown/plugin, and editor tabs.
- Reuse the same view components in grouped and free-order layouts.
- Keep ordering, overflow, drag/drop, and context-menu coordination in `TabBar` until their contracts are explicit.
- Add parity tests that run the same tab behaviors in both ordering modes.

Result on 2026-07-21:

- Added typed `TerminalTabView`, `DiffTabView`, `MarkdownTabView`, and
  `EditorTabView` components. Grouped and free/terminals-first modes now render
  the same components instead of maintaining duplicated JSX branches.
- Ordering, overflow, drag/drop, context menus, and visibility filtering remain
  in `TabBar`; presentation-specific differences such as pinned icons and
  global-workspace metadata are explicit props.
- Added a parity matrix that renders and selects every tab kind in both
  `grouped-by-type` and `free` modes. The complete `TabBar` suite passes 47
  tests.
- `TabBar.tsx` decreased from 1,607 to 964 lines; the shared typed views occupy
  437 lines, for a net removal of 206 lines of duplicated rendering logic.
- The full suite passes 297 files and 4,727 tests. `pnpm build` passes with
  419,554 desktop gzip bytes and 67,427 mobile gzip bytes.

### Work unit 5: Split Git operation coordinators

Preserve the existing public facade initially so callers do not change at the same time as internals.

- Extract repository refresh with generation and deduplication state.
- Extract branch switching with its FIFO serialization queue.
- Extract worktree creation/removal and recovery state.
- Extract merge/autofix/conflict workflows.
- Extract CWD-based terminal reassignment.
- Replace the oversized facade only after consumers and tests show stable smaller contracts.

Result on 2026-07-21:

- `useGitOperations` remains the caller-compatible composition facade, while
  focused coordinators now own repository refresh, branch selection,
  terminal/worktree reassignment, worktree creation, worktree removal, and
  merge/autofix/conflict workflows under `src/hooks/git/`.
- Correctness state moved with its behavior: refresh generations and request
  deduplication, the FIFO branch-selection queue, creation grace tracking,
  removal locking, OSC 7 reassignment debouncing, and workflow recovery state
  each have a single lifecycle owner.
- Agent command seeding is isolated in a pure helper while retaining the
  compatibility exports from `useGitOperations`.
- `useGitOperations.ts` decreased from the 2,257-line baseline to 775 lines.
  The complete existing hook suite passes all 155 tests, including stale
  refresh suppression, serialized switching, worktree recovery/removal, CWD
  reassignment, and merge/conflict workflows.
- Focused TypeScript and Biome validation passes for the facade and every new
  coordinator. Full-suite and production-build results are recorded below
  after the work-unit integration run.

### Work unit 6: Decompose the canvas terminal

This is last because it is both performance-sensitive and behavior-dense.

- Extract selection and search state behind an imperative controller.
- Extract link discovery and verification behind a cancellable controller.
- Extract smooth-scroll/cache/scrollbar state without adding reactive dependencies.
- Extract keyboard/IME/mouse binding and cleanup.
- Keep frame decode, scheduling, and paint coordination together until profiling demonstrates a safe seam.
- Keep the public `CanvasTerminalRef` contract stable during decomposition.

Result on 2026-07-21:

- Added imperative controllers for selection/search state, cancellable link
  verification and caches, smooth-scroll position/cache/handoff state, and DOM
  input-listener ownership. None introduces Solid signals or store updates on
  the frame path.
- Selection extraction covers forward/reverse multi-row text, offscreen range
  detection, cached-copy reset, and search navigation. Link checks now use an
  explicit generation token and dispose queued verification on unmount.
- Scroll state owns the fractional position, pending absolute offset, backend
  settle handoff, gesture distance, styled-row cache, and requested chunks.
  Frame decode, row reconciliation, scheduling, and canvas paint remain together
  in `CanvasTerminal`.
- Keyboard, IME, paste, mouse, wheel, and scrollbar listeners now share one
  idempotent binding lifecycle. The pre-subscription cleanup guarantee remains
  intact if unmount occurs while transport subscription is pending.
- `CanvasTerminal.tsx` decreased from 3,288 to 3,194 lines. Four new controller
  suites add 12 focused tests; the complete terminal-focused run passes 15 files
  and 173 tests. TypeScript and targeted Biome checks pass.

Each extraction requires focused unit tests plus terminal-specific regression tests. Visual changes are not expected; if canvas output changes, it requires explicit visual verification rather than relying on HTTP inspection.

### Work unit 7: Remove runtime dependency cycles

The production runtime graph is enforced by `pnpm architecture:cycles`, which
is part of the standard `make check` path. `pnpm architecture:cycles:test`
exercises the checker itself against temporary acyclic and cyclic graphs.

Cycle removal can be interleaved only where a preceding work unit creates the required seam:

- inject connection lookup and logging into transport adapters to break the transport/store cycle;
- move shared sidebar models/actions below the four sidebar views;
- make `ColorSwatchPicker` receive values and callbacks rather than importing its settings owner.

Do not create adapter modules whose only purpose is to hide a cycle while preserving both directions.

Result on 2026-07-21:

- `transport.ts` no longer imports application stores. Logging and remote-base-
  URL lookup enter through `transportRuntime` ports configured by `appLogger`
  and `remoteConnectionsStore`; transport keeps safe no-op/unavailable defaults
  during module initialization.
- Shared PR merge eligibility and `PrStateBadge` now sit below sidebar views.
  `GitHubPanel`, `PrSection`, `RemoteOnlyPrPopover`, and `RepoSection` no longer
  import one another in a cycle; compatibility exports remain on `RepoSection`.
- `ColorSwatchPicker` receives its preset list as a prop. Preset data lives in a
  shared leaf module instead of importing the owning `AppearanceTab`.
- Added `pnpm architecture:cycles`, which parses production TypeScript with the
  compiler API, excludes type-only edges, and fails on strongly connected
  runtime components. The resulting graph contains 475 production files and
  zero runtime cycles.
- Focused transport/store validation passes 4 files and 163 tests. Focused
  sidebar/appearance validation passes 4 files and 136 tests. TypeScript and
  targeted Biome checks pass.

## Validation Contract

Each work unit must include:

1. Relevant characterization tests written before extraction.
2. Focused Vitest execution while iterating.
3. `pnpm test --run` before handoff.
4. `pnpm build` and comparison of the generated preload graph.
5. `make check` before integration.
6. Browser-mode verification against the worktree test instance for affected interactive UI.
7. A screenshot after any visual, CSS, or layout change.
8. `perfDebug` comparison for any claim about responsiveness or frame behavior.

Runtime performance claims require measurements. File size or line count alone is not evidence of a faster UI.

### Final validation on 2026-07-24

- The complete Vitest suite passes 302 files and 4,762 tests without leaked
  timers. Test teardown now cancels terminal cooldown/question timers and the
  activity persistence debounce owned by isolated store modules.
- The production build passes. The desktop initial payload is 1,518,882 bytes
  raw and 433,460 bytes gzip; the mobile initial payload is 202,544 bytes raw
  and 63,836 bytes gzip. All optional-asset budgets pass.
- `pnpm architecture:cycles` analyzes 475 production runtime files and reports
  zero cycles.
- `cargo nextest run --no-fail-fast` passes all 3,927 Rust tests, with 10 tests
  skipped. The tunnel supervisor tests now wait for terminal state transitions
  instead of sampling them at fixed timing boundaries under parallel load.
- Repository-wide `make check` passes, including TypeScript, Biome, Rust tests,
  and dependency audits. Formatting drift exposed by the final dependency
  upgrade was normalized during integration.
- No production Rust behavior, CSS, or layout files changed. Rust edits are
  limited to compile-time lint cleanup and test timing robustness. Visual
  output was not changed intentionally.
  Browser-mode verification through the mandatory stealth wrapper covered the
  main terminal layout and the Appearance and Services settings views;
  screenshots showed no clipping, overlap, or missing controls.

## Commit and Rollback Strategy

- One cohesive extraction or loading-boundary change per commit.
- Preserve public contracts until the implementation behind them is stable.
- Do not rename and behavior-change the same code in one commit.
- Keep characterization tests in the same commit as the seam they protect or in the immediately preceding commit.
- If a work unit cannot remain independently green, its boundary is too broad and must be divided.

## Completion Criteria

The program is complete when:

- optional editor/diff/Markdown code is absent from unrelated startup paths;
- `App.tsx` is a composition root with no feature lifecycle implementation;
- grouped and free-order tabs share render components;
- Git refresh, switching, worktree, and terminal-reassignment coordinators have isolated tests;
- canvas terminal controllers have explicit lifecycle ownership and preserve measured frame behavior;
- the three current runtime dependency cycles are gone;
- all existing behavior remains covered by a green full suite and the required browser/visual checks.
