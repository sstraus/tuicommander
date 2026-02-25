# TUICommander - Project Instructions

## Documentation Sync Matrix

Every code change that affects user-visible behavior, APIs, or configuration MUST update the corresponding documentation files. This matrix maps codebase areas to their docs.

### New Feature Checklist

- [ ] Feature works correctly
- [ ] Keyboard shortcut added (if applicable) — `keybindingDefaults.ts` + `actionRegistry.ts` ACTION_META
- [ ] `docs/FEATURES.md` updated with new feature entry
- [ ] `CHANGELOG.md` — entry in Unreleased section
- [ ] `SPEC.md` — feature status updated
- [ ] Domain-specific docs updated (see matrix below)
- [ ] Screenshot taken (if visual/CSS/layout change)

### Sync Matrix by Area

#### Plugin System
When modifying PluginHost API, capabilities, manifest schema, or Tauri commands used by plugins:

| File | What to update |
|------|----------------|
| `src/plugins/types.ts` | PluginHost interface, PluginCapability union, snapshot types |
| `src/plugins/pluginRegistry.ts` | Implementation in `buildHost()` |
| `src-tauri/src/plugins.rs` | `KNOWN_CAPABILITIES` list (new capabilities) |
| `src-tauri/src/lib.rs` | Register new Tauri commands in `invoke_handler` |
| `docs/plugins.md` | Plugin developer guide (API reference, capabilities table, examples) |
| `src-tauri/src/mcp_http/plugin_docs.rs` | AI-optimized plugin reference (`PLUGIN_DOCS` const) |
| `docs/api/tauri-commands.md` | Tauri commands reference table |
| `docs/api/http-api.md` | HTTP API reference (if new HTTP endpoints) |
| `docs/backend/mcp-http.md` | MCP/HTTP server docs (if new routes) |
| `docs/FEATURES.md` | Section 17.1 capabilities list |
| `docs/user-guide/plugins.md` | User installation/management guide |

#### Terminal & PTY
When modifying PTY behavior, output parsing, shell state, or terminal UI:

| File | What to update |
|------|----------------|
| `docs/backend/pty.md` | PTY session lifecycle, reader threads, output handling |
| `docs/backend/output-parser.md` | Rate limits, structured events, parsing rules |
| `docs/FEATURES.md` | Section 1 (Terminal Management) |
| `docs/user-guide/terminals.md` | User-facing terminal features |
| `docs/api/tauri-commands.md` | PTY commands (create_pty, write_pty, resize_pty, etc.) |

#### Keyboard Shortcuts & Actions
When adding or changing shortcuts:

| File | What to update |
|------|----------------|
| `src/keybindingDefaults.ts` | ACTION_NAMES + default key combo |
| `src/actions/actionRegistry.ts` | ACTION_META (label, category) — auto-populates Settings and Command Palette |
| `docs/FEATURES.md` | Section 15 (Keyboard Shortcut Reference) |
| `docs/user-guide/keyboard-shortcuts.md` | User-facing shortcut table |

#### Tauri Commands & IPC
When adding or changing Tauri commands:

| File | What to update |
|------|----------------|
| `src-tauri/src/lib.rs` | `invoke_handler!` macro registration |
| `docs/api/tauri-commands.md` | Command signature + description |
| `docs/api/http-api.md` | HTTP endpoint mapping (if browser/remote mode) |
| Domain backend doc | e.g. `docs/backend/pty.md`, `docs/backend/git.md` |

#### HTTP & MCP Server
When adding routes or changing server behavior:

| File | What to update |
|------|----------------|
| `docs/api/http-api.md` | REST endpoint reference |
| `docs/backend/mcp-http.md` | Server architecture, routing |
| `docs/user-guide/remote-access.md` | User setup guide |
| `src-tauri/src/mcp_http/plugin_docs.rs` | PLUGIN_DOCS (if plugin-facing) |

#### Git & Worktree Integration
When modifying git operations, worktree logic, or GitHub API:

| File | What to update |
|------|----------------|
| `docs/backend/git.md` | Git command lifecycle, diff parsing |
| `docs/backend/github.md` | PR fetching, CI checks, GraphQL |
| `docs/user-guide/worktrees.md` | Worktree workflow, configuration |
| `docs/user-guide/github-integration.md` | PR monitoring, CI rings |
| `docs/FEATURES.md` | Sections 7 (Git) and 8 (GitHub) |
| `docs/api/tauri-commands.md` | Git/worktree commands |

#### Settings & Configuration
When adding config fields or settings UI:

| File | What to update |
|------|----------------|
| `docs/backend/config.md` | Config files, schema, platform directories |
| `docs/user-guide/settings.md` | Settings tab breakdown |
| `docs/FEATURES.md` | Section 11 (Settings) |

#### Agent Detection
When adding agents or changing detection logic:

| File | What to update |
|------|----------------|
| `docs/user-guide/ai-agents.md` | Agent support, detection method |
| `docs/backend/output-parser.md` | Agent-specific parsing rules |
| `docs/FEATURES.md` | Section 6 (AI Agent Support) |
| `src-tauri/src/mcp_http/plugin_docs.rs` | agentTypes valid values in PLUGIN_DOCS |

#### UI Components & Panels
When adding or modifying panels, status bar, toolbar, sidebar:

| File | What to update |
|------|----------------|
| `docs/FEATURES.md` | Relevant section (2-5: Sidebar, Panels, Toolbar, Status Bar) |
| `docs/frontend/STYLE_GUIDE.md` | If changing visual patterns |
| `docs/frontend/components.md` | Component tree, panel descriptions |
| Domain user guide | e.g. `docs/user-guide/sidebar.md`, `docs/user-guide/file-browser.md` |

#### Deep Links
When adding or changing `tuic://` schemes:

| File | What to update |
|------|----------------|
| `docs/FEATURES.md` | Section 17.4 (Deep Links) |
| `docs/plugins.md` | If affecting plugin contentUri format |

### Documentation File Index

| Path | Purpose |
|------|---------|
| **Root** | |
| `SPEC.md` | Feature specification, architecture, version |
| `CHANGELOG.md` | Release history (Keep a Changelog format) |
| `AGENTS.md` | This file — project rules, sync matrix |
| `to-test.md` | Manual testing tracker |
| **docs/** | |
| `docs/FEATURES.md` | Canonical feature inventory (single source of truth) |
| `docs/plugins.md` | Plugin developer authoring guide |
| `docs/api/tauri-commands.md` | All Tauri IPC commands |
| `docs/api/http-api.md` | REST/HTTP endpoint reference |
| `docs/architecture/overview.md` | High-level architecture |
| `docs/architecture/data-flow.md` | IPC and data flow |
| `docs/architecture/state-management.md` | Store patterns |
| `docs/backend/pty.md` | PTY session lifecycle |
| `docs/backend/output-parser.md` | Output parsing and structured events |
| `docs/backend/git.md` | Git operations |
| `docs/backend/github.md` | GitHub API integration |
| `docs/backend/config.md` | Configuration file management |
| `docs/backend/mcp-http.md` | MCP/HTTP server |
| `docs/backend/dictation.md` | Whisper voice dictation |
| `docs/backend/error-classification.md` | Error types and backoff |
| `docs/frontend/STYLE_GUIDE.md` | Visual design rules |
| `docs/frontend/components.md` | Component tree reference |
| `docs/frontend/hooks.md` | Custom hooks |
| `docs/frontend/stores.md` | SolidJS stores |
| `docs/frontend/transport.md` | Tauri/HTTP dual-mode transport |
| `docs/user-guide/*.md` | User-facing guides (13 files) |
| **Code-embedded docs** | |
| `src-tauri/src/mcp_http/plugin_docs.rs` | AI-optimized plugin reference (`PLUGIN_DOCS` const) |
| `src/actions/actionRegistry.ts` | ACTION_META → auto-populates HelpPanel + Command Palette |
| `examples/plugins/` | Reference plugin implementations (6 examples) |

## Testing Tracker

**`to-test.md`** contains features awaiting manual testing when TUI is more usable.

When implementing minor features, add test items to `to-test.md` instead of testing immediately.

## Visual Style Guide

**All UI work MUST follow [`docs/frontend/STYLE_GUIDE.md`](docs/frontend/STYLE_GUIDE.md).** This defines the color palette, typography, spacing, component patterns, animations, and anti-patterns. Read it before any visual/CSS/layout change.

**Icons MUST be monochrome inline SVGs.** Never use emoji for UI icons — they render inconsistently across platforms and break the visual style. Use `<svg>` elements with `fill="currentColor"` so they inherit the text color.

## Visual Verification

**IMPORTANT:** After EVERY visual/CSS/layout change to the TUI, you MUST take a screenshot to verify the result. You cannot reliably judge rendering from code alone.

## Branch Management

**NEVER create branches autonomously.** Boss works with multiple windows simultaneously and autonomous branch creation causes conflicts. Only create/switch branches when explicitly asked.

## Cross-Platform

**This app targets macOS, Windows, and Linux.** Always design and implement features for all three platforms. Use Cmd/Ctrl abstractions, avoid platform-specific APIs without fallbacks, and use Tauri's cross-platform primitives (menus, dialogs, shortcuts, etc.).

### Release-build environment differences

**CRITICAL:** Code that works in `tauri dev` may fail in compiled release builds. Release builds launched from Finder/Explorer/desktop have a minimal environment — they do NOT inherit the user's shell PATH, environment variables, or locale settings. Every feature MUST be tested against these constraints:

- **Never assume CLI tools are on PATH.** Use `resolve_cli()` (in `agent.rs`) to probe well-known directories, or detect apps via `.app` bundles / registry entries instead of `which`/`where`.
- **Never assume environment variables exist.** `$EDITOR`, `$SHELL`, `$HOME` etc. may be absent or different. Always provide fallbacks.
- **Never assume window dimensions are valid.** Persisted window state can contain 0x0 or off-screen positions. Always validate and clamp to sane defaults.
- **Test features in release mode** (`cargo tauri build`) before considering them complete, not just `tauri dev`.

## Panel Refresh Pattern

**All panels that display repo-dependent data MUST subscribe to the `revision` signal from `repositoriesStore.getRevision(repoPath)` inside their `createEffect`.** The Rust `repo_watcher` monitors `.git/` for changes (index, refs, HEAD, merge state) and emits `"repo-changed"` events, which bump the revision counter via `repositoriesStore.bumpRevision()`. Do NOT implement per-panel file watchers or polling.

Example:
```typescript
createEffect(() => {
  const repoPath = props.repoPath;
  const _rev = repoPath ? repositoriesStore.getRevision(repoPath) : 0;
  // ... fetch logic re-runs when revision bumps
});
```

## Architecture Rule: Logic in Rust

**All business logic, data transformation, and parsing MUST be implemented in Rust (backend), NOT in the UI layer (TypeScript/SolidJS stores or components).** The frontend should only handle rendering and user interaction — never data reshaping or computation.

## Release & Tag Checklist

When Boss asks to tag a release:

1. **Update version** in `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`
2. **Update SPEC.md** header version and date
3. **Update CHANGELOG.md** — move Unreleased items under the new version heading
4. **Commit** with message `chore: bump version to vX.Y.Z`
5. **Tag** with `git tag vX.Y.Z`
6. **GitHub release** — create via `gh release create vX.Y.Z --generate-notes`
7. **Milestone** — close the matching milestone if one exists, create the next one

### GitHub Issue Management

- **Labels**: Use `type:`, `P0-P3:`, `area:`, `effort:` prefixes. Apply `needs triage` to new issues.
- **Milestones**: Assign issues to version milestones (v0.4.0, v1.0.0, etc.)
- **Issue templates**: Bug reports and feature requests use `.github/ISSUE_TEMPLATE/*.yml` forms
- **Token for project ops**: Use `GH_TOKEN=$GH_STRAUS gh ...` when commands need the `project` scope (the default `gh auth` token only has `repo` + `workflow`)

## Ideas Tracker

See CLAUDE.md for ideas folder rules (gitignored).
