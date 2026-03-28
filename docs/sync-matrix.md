# Documentation Sync Matrix

Every code change that affects user-visible behavior, APIs, or configuration MUST update the corresponding documentation files. This matrix maps codebase areas to their docs.

## New Feature Checklist

- [ ] Feature works correctly
- [ ] Keyboard shortcut added (if applicable) — `keybindingDefaults.ts` + `actionRegistry.ts` ACTION_META
- [ ] `docs/FEATURES.md` updated with new feature entry
- [ ] `CHANGELOG.md` — entry in Unreleased section
- [ ] `SPEC.md` — feature status updated
- [ ] Domain-specific docs updated (see matrix below)
- [ ] Screenshot taken (if visual/CSS/layout change)
- [ ] `src/data/tips.ts` — add a Tip of the Day entry for discoverable features

## Sync Matrix by Area

### Plugin System
When modifying PluginHost API, capabilities, manifest schema, Tauri commands used by plugins, plugin panel rendering (base CSS, theme injection, iframe behavior), or plugin infrastructure (loader, registry, discovery):

| File | What to update |
|------|----------------|
| `src/plugins/types.ts` | PluginHost interface, PluginCapability union, snapshot types |
| `src/plugins/pluginRegistry.ts` | Implementation in `buildHost()` |
| `src/components/PluginPanel/pluginBaseStyles.ts` | Base CSS classes available to all plugin panels |
| `src-tauri/src/plugins.rs` | `KNOWN_CAPABILITIES` list (new capabilities) |
| `src-tauri/src/lib.rs` | Register new Tauri commands in `invoke_handler` |
| `docs/plugins.md` | Plugin developer guide (API reference, capabilities table, **Panel CSS Design Strategy** section, examples) |
| `src-tauri/src/mcp_http/plugin_docs.rs` | AI-optimized plugin reference (`PLUGIN_DOCS` const — **must stay in sync with `docs/plugins.md`**) |
| `docs/api/tauri-commands.md` | Tauri commands reference table |
| `docs/api/http-api.md` | HTTP API reference (if new HTTP endpoints) |
| `docs/backend/mcp-http.md` | MCP/HTTP server docs (if new routes) |
| `docs/FEATURES.md` | Section 17.1 capabilities list |
| `docs/user-guide/plugins.md` | User installation/management guide |

### Terminal & PTY
When modifying PTY behavior, output parsing, shell state, or terminal UI:

| File | What to update |
|------|----------------|
| `docs/backend/pty.md` | PTY session lifecycle, reader threads, output handling |
| `docs/backend/output-parser.md` | Rate limits, structured events, parsing rules |
| `docs/FEATURES.md` | Section 1 (Terminal Management) |
| `docs/user-guide/terminals.md` | User-facing terminal features |
| `docs/api/tauri-commands.md` | PTY commands (create_pty, write_pty, resize_pty, etc.) |

### Keyboard Shortcuts & Actions
When adding or changing shortcuts:

| File | What to update |
|------|----------------|
| `src/keybindingDefaults.ts` | ACTION_NAMES + default key combo |
| `src/actions/actionRegistry.ts` | ACTION_META (label, category) — auto-populates Settings and Command Palette |
| `docs/FEATURES.md` | Section 15 (Keyboard Shortcut Reference) |
| `docs/user-guide/keyboard-shortcuts.md` | User-facing shortcut table |

### Tauri Commands & IPC
When adding or changing Tauri commands:

| File | What to update |
|------|----------------|
| `src-tauri/src/lib.rs` | `invoke_handler!` macro registration |
| `docs/api/tauri-commands.md` | Command signature + description |
| `docs/api/http-api.md` | HTTP endpoint mapping (if browser/remote mode) |
| Domain backend doc | e.g. `docs/backend/pty.md`, `docs/backend/git.md` |

### HTTP & MCP Server
When adding routes or changing server behavior:

| File | What to update |
|------|----------------|
| `docs/api/http-api.md` | REST endpoint reference |
| `docs/backend/mcp-http.md` | Server architecture, routing |
| `docs/user-guide/remote-access.md` | User setup guide |
| `src-tauri/src/mcp_http/plugin_docs.rs` | PLUGIN_DOCS (if plugin-facing) |

### Git & Worktree Integration
When modifying git operations, worktree logic, or GitHub API:

| File | What to update |
|------|----------------|
| `docs/backend/git.md` | Git command lifecycle, diff parsing |
| `docs/backend/github.md` | PR fetching, CI checks, GraphQL |
| `docs/user-guide/worktrees.md` | Worktree workflow, configuration |
| `docs/user-guide/github-integration.md` | PR monitoring, CI rings |
| `docs/FEATURES.md` | Sections 7 (Git) and 8 (GitHub) |
| `docs/api/tauri-commands.md` | Git/worktree commands |

### Settings & Configuration
When adding config fields or settings UI:

| File | What to update |
|------|----------------|
| `docs/backend/config.md` | Config files, schema, platform directories |
| `docs/user-guide/settings.md` | Settings tab breakdown |
| `docs/FEATURES.md` | Section 11 (Settings) |

### Agent Detection
When adding agents or changing detection logic:

| File | What to update |
|------|----------------|
| `docs/user-guide/ai-agents.md` | Agent support, detection method |
| `docs/backend/output-parser.md` | Agent-specific parsing rules |
| `docs/FEATURES.md` | Section 6 (AI Agent Support) |
| `src-tauri/src/mcp_http/plugin_docs.rs` | agentTypes valid values in PLUGIN_DOCS |

### UI Components & Panels
When adding or modifying panels, status bar, toolbar, sidebar:

| File | What to update |
|------|----------------|
| `docs/FEATURES.md` | Relevant section (2-5: Sidebar, Panels, Toolbar, Status Bar) |
| `docs/frontend/STYLE_GUIDE.md` | If changing visual patterns |
| `docs/frontend/components.md` | Component tree, panel descriptions |
| Domain user guide | e.g. `docs/user-guide/sidebar.md`, `docs/user-guide/file-browser.md` |

### Deep Links
When adding or changing `tuic://` schemes:

| File | What to update |
|------|----------------|
| `docs/FEATURES.md` | Section 17.4 (Deep Links) |
| `docs/plugins.md` | If affecting plugin contentUri format |

## Documentation File Index

| Path | Purpose |
|------|---------|
| **Root** | |
| `SPEC.md` | Feature specification, architecture, version |
| `CHANGELOG.md` | Release history (Keep a Changelog format) |
| `AGENTS.md` | Project rules, compact reference |
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
