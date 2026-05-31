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
| `docs/frontend/canvas-terminal-audit.md` | CanvasTerminal feature completeness audit |
| `docs/FEATURES.md` | Section 1 (Terminal Management) |
| `docs/user-guide/terminals.md` | User-facing terminal features |
| `docs/api/tauri-commands.md` | PTY commands (create_pty, write_pty, resize_pty, etc.) |
| `docs/backend/alacritty-integration.md` | Alacritty patch inventory, upstream API usage, update procedure |

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

#### Tauri events emitted by backend
When adding a new `app.emit(event_name, payload)` call, document it here and listen in `useAppInit.ts`:

| Event | Payload | Emitted from | Frontend listener |
|-------|---------|-------------|-------------------|
| `session-standby` | `{ session_id: string, standby: bool }` | `pty.rs emit_standby_event()` | `useAppInit.ts` → `terminalsStore.update(termId, { standby })` |
| `worktree-created` | `{ repo_path: string, branch: string, worktree_path: string }` | `mcp_transport.rs`, `session.rs`, `worktree_routes.rs` | TBD — frontend switch prompt |

### HTTP & MCP Server
When adding routes or changing server behavior:

| File | What to update |
|------|----------------|
| `docs/api/http-api.md` | REST endpoint reference |
| `docs/backend/mcp-http.md` | Server architecture, routing, lazy tool discovery (`collapse_tools` / meta-tools) |
| `docs/user-guide/remote-access.md` | User setup guide |
| `src-tauri/src/mcp_http/plugin_docs.rs` | PLUGIN_DOCS (if plugin-facing) |

### Diagnostics
When modifying `cpu_watchdog.rs` or the `/diagnostics` HTTP endpoint:

| File | What to update |
|------|----------------|
| `src-tauri/src/cpu_watchdog.rs` | Watchdog logic, thresholds, snapshot fields |
| `src-tauri/src/mcp_http/log_routes.rs` | `/diagnostics` GET/POST handlers |
| `AGENTS.md` | Diagnostics section (usage, known failure patterns) |

### MCP Tool Surface (native tools, upstream proxy, meta-tools)
When changing the tool list, tool handlers, `disabled_native_tools`, upstream allow/deny filters, or the Speakeasy meta-tools:

| File | What to update |
|------|----------------|
| `src-tauri/src/mcp_http/mcp_transport.rs` | Tool definitions, `merged_tool_definitions`, `searchable_tool_definitions`, meta-tool handlers (`search_tools`, `get_tool_schema`, `call_tool`), `build_mcp_instructions` |
| `src-tauri/src/mcp_proxy/registry.rs` | `aggregated_tools`, `proxy_tool_call` (filter is enforced on BOTH — discovery no longer gates dispatch under `collapse_tools`) |
| `src-tauri/src/tool_search.rs` | BM25 `ToolSearchIndex` backing `search_tools` / `get_tool_schema` |
| `docs/backend/mcp-http.md` | Lazy Tool Discovery section, meta-tool table, filter-enforcement note |
| `docs/backend/config.md` | `collapse_tools` field in `AppConfig` table |
| `docs/user-guide/settings.md` | Services Tab — "Collapse tools" checkbox description |

#### Session tool actions added (swarm Layer 3–4)
- `session action=status` — returns `{shell_state, idle_since_ms, busy_duration_ms, exit_code, agent_type}`. Useful for polling agent progress without streaming output.
- `session action=list` response now includes `shell_state` per entry.

#### Agent tool actions added (swarm inbox)
- `agent action=inbox` response now includes `missed_count` — number of messages evicted from the FIFO inbox since last read. Non-zero means the orchestrator missed messages and should increase polling frequency.

### Provider Registry
When modifying provider types, slot names, credential storage, or the ProvidersTab UI:

| File | What to update |
|------|----------------|
| `src-tauri/src/provider_registry.rs` | `ProviderType`, `SlotName`, `ProviderRegistry` structs + Tauri commands |
| `src-tauri/src/credentials.rs` | `Credential::Provider` variant for per-provider key storage |
| `src/stores/providerRegistry.ts` | Frontend store: hydrate, save, slot resolution, CRUD |
| `src/components/SettingsPanel/tabs/ProvidersTab.tsx` | Settings UI: provider cards, model CRUD, slot assignments |
| `src/hooks/useSmartPrompts.ts` | `resolveSlot("headless")` check for headless execution |
| `docs/backend/config.md` | `providers.json` schema documentation |

### AI Prompts
When modifying customizable AI service prompts (diff triage, future services):

| File | What to update |
|------|----------------|
| `src-tauri/src/config.rs` | `AiPromptsConfig` struct, load/save commands |
| `src-tauri/src/diff_triage.rs` | `build_chat_request` system_prompt param, `default_system_prompt()` |
| `src/stores/aiPrompts.ts` | Frontend store: hydrate, save, `DEFAULT_DIFF_TRIAGE_PROMPT` const |
| `src/components/SettingsPanel/tabs/AiPromptsTab.tsx` | Settings UI: textarea per service, reset button |
| `src-tauri/src/mcp_http/mcp_transport.rs` | MCP config tool: `list_ai_prompts`, `load_ai_prompt`, `save_ai_prompt` actions |
| `docs/backend/config.md` | `ai-prompts.json` schema documentation |

### AI Chat
When modifying AI Chat panel, settings, context menu actions, or streaming backend:

| File | What to update |
|------|----------------|
| `src-tauri/src/ai_chat.rs` | Backend: config, streaming, context assembly, Ollama detection |
| `src-tauri/src/ai_chat_registry.rs` | Chat Registry: cross-window state sync, Channel fan-out, subscribe/unsubscribe |
| `src/stores/aiChatStore.ts` | Frontend store: messages, streaming state, registry subscription (sessionId passed per-call, derived from focused terminal) |
| `src/components/AIChatPanel/AIChatPanel.tsx` | Chat panel component + detach button + registry lifecycle |
| `src/components/AIChatPanel/contextMenuActions.ts` | Terminal context menu integration |
| `src/components/PanelOrchestrator.tsx` | Switches between AIChatPanel and DetachedPlaceholder |
| `src/components/DetachedPlaceholder.tsx` | Placeholder shown in main window when panel is detached |
| `src/components/SettingsPanel/tabs/AiChatTab.tsx` | Settings panel section |
| `src/stores/ui.ts` | `aiChatPanelVisible` + `aiChatPanelWidth` + `detachedPanels` map |
| `src/panelRouter.tsx` | Panel adapter registry + routing for detached panel windows |
| `src/utils/panelSync.ts` | PanelSyncProvider + PanelSyncReceiver for main↔detached communication |
| `src/hooks/initPanelWindow.ts` | Bootstrap for detached panel windows (theme, font, settings) |
| `src/keybindingDefaults.ts` | `toggle-ai-chat` + `detach-activity-dashboard` hotkeys |
| `docs/FEATURES.md` | AI Chat feature section |
| `docs/user-guide/ai-chat.md` | User-facing AI Chat guide |
| `docs/api/tauri-commands.md` | Chat Registry + `open_panel_window` / `close_panel_window` / `focus_main_window` commands |

### AI Agent (ReAct loop, knowledge store, MCP terminal tools)
When modifying the AI agent loop engine, tool dispatch, session knowledge store,
OSC 133 outcome capture, or the `ai_terminal_*` MCP tools:

| File | What to update |
|------|----------------|
| `src-tauri/src/ai_agent/engine.rs` | ReAct loop, approval flow, ACTIVE_AGENTS registry, system prompt |
| `src-tauri/src/ai_agent/tools.rs` | Tool dispatch: 19 tools (terminal, filesystem, drive_agent, search, list_sessions) |
| `src-tauri/src/ai_agent/safety.rs` | SafetyChecker: command safety + file-write sensitive path rules |
| `src-tauri/src/ai_agent/sandbox.rs` | FileSandbox: path jail for filesystem tools (canonicalize + starts_with) |
| `src-tauri/src/mcp_http/ai_terminal.rs` | MCP exposure of all 13 `ai_terminal_*` tools; write-tool confirmation |
| `src-tauri/src/ai_agent/knowledge.rs` | CommandOutcome, SessionKnowledge, OSC 133 scanner, persist/load/spawn_persist_task |
| `src-tauri/src/ai_agent/context.rs` | Session-knowledge injection into agent system prompt |
| `src-tauri/src/ai_agent/tui_detect.rs` | TerminalMode heuristics (Shell vs FullscreenTui) |
| `src-tauri/src/ai_agent/commands.rs` | Tauri commands: start/cancel/pause/resume/status/approve/get_session_knowledge |
| `src-tauri/src/pty.rs` | ChunkProcessor.record_osc133_outcomes + Inferred fallback in silence timer |
| `src-tauri/src/state.rs` | session_knowledge DashMap, knowledge_dirty set, has_osc133_integration, record_outcome helper |
| `src-tauri/src/lib.rs` | Register new commands in `invoke_handler`; spawn_persist_task at boot |
| `src-tauri/src/mcp_http/mcp_transport.rs` | `ai_terminal_*` MCP tool defs + dispatch |
| `src/stores/aiAgentStore.ts` | Frontend agent state (running/paused), tool-call log, approvals |
| `src/components/AIChatPanel/AIChatPanel.tsx` | Agent banner, approval card, tool-call cards |
| `src/components/AIChatPanel/SessionKnowledgeBar.tsx` | Collapsible footer summarising the session's knowledge store |
| `docs/api/tauri-commands.md` | `start_agent_loop`, `cancel_agent_loop`, `pause_agent_loop`, `resume_agent_loop`, `agent_loop_status`, `approve_agent_action`, `get_session_knowledge` |
| `docs/backend/mcp-http.md` | `ai_terminal_*` MCP tools table |
| `docs/FEATURES.md` | AI Agent section (Level 2/3 of the AI-assisted terminal roadmap) |
| `ideas/ai-assisted-terminal.md` | Status updates as capability levels ship |

### Terminal Watcher (event-driven autonomous actions)
When modifying the watcher engine, trigger evaluation, or watcher UI:

| File | What to update |
|------|----------------|
| `src-tauri/src/ai_agent/watcher.rs` | WatcherRule model, WatcherEngine event loop, trigger evaluation, burst guard, fire_rule |
| `src-tauri/src/ai_agent/commands.rs` | Tauri commands: watcher_create, watcher_list, watcher_delete, watcher_toggle, watcher_attach, watcher_detach, watcher_update |
| `src-tauri/src/state.rs` | `watcher_engine` OnceLock in AppState, `session_visibility` DashMap |
| `src-tauri/src/lib.rs` | Command registration + WatcherEngine spawn |
| `src/components/WatcherManager/WatcherManager.tsx` | Template CRUD, attach/detach, edit form (toolbar popover) |
| `src/components/WatcherManager/WatcherManager.module.css` | Popover styles |
| `docs/backend/ai-watchers.md` | Architecture doc: data model, trigger paths, safety guards |
| Config: `ai-watchers.json` | Persisted watcher rules (app config dir) |

### Remote Daemon (`tuic-remote`)
When modifying the remote daemon binary, `run_headless`, or standalone server behavior:

| File | What to update |
|------|----------------|
| `src-tauri/src/bin/tuic_remote.rs` | Binary entry point |
| `src-tauri/src/lib.rs` | `run_headless()` function |
| `docs/user-guide/remote-access.md` | `tuic-remote (Beta)` section |
| `docs/FEATURES.md` | Section 22 (Remote Daemon) |
| `.github/workflows/release.yml` | Release artifact build job |

### SSH Tunnel Management
When modifying tunnel profiles, supervisor, audit logging, backoff, or tunnel UI:

| File | What to update |
|------|----------------|
| `src-tauri/src/tunnels/profile.rs` | TunnelProfile, ForwardSpec, ProfileOptions structs |
| `src-tauri/src/tunnels/command.rs` | SSH command-line argument building |
| `src-tauri/src/tunnels/classifier.rs` | ExitReason enum and stderr classification |
| `src-tauri/src/tunnels/agent.rs` | SSH agent socket discovery |
| `src-tauri/src/tunnels/port.rs` | Local port availability check |
| `src-tauri/src/tunnels/backoff.rs` | BackoffCalculator (delays, jitter, max retries) |
| `src-tauri/src/tunnels/audit.rs` | AuditLog SQLite schema, insert/query/rotate |
| `src-tauri/src/tunnels/supervisor.rs` | TunnelSupervisor lifecycle and reconnect loop |
| `src-tauri/src/tunnels/storage.rs` | ProfileStore: TOML load/save (global + per-repo) |
| `src-tauri/src/tunnels/manager.rs` | TunnelManager: orchestrates supervisors |
| `src-tauri/src/tunnels/commands.rs` | Tauri commands for tunnel CRUD and control |
| `src/stores/tunnels.ts` | Frontend tunnel state (profiles, statuses) |
| `src/stores/tunnelPanel.ts` | Tunnel panel UI state |
| `src/components/TunnelsPanel/TunnelsPanel.tsx` | Tunnel list with start/stop controls |
| `src/components/TunnelsPanel/TunnelEditorModal.tsx` | Profile create/edit form |
| `src/components/TunnelsPanel/TunnelStatusBadge.tsx` | Color-coded status indicator |
| `docs/features/ssh-tunnels.md` | Feature architecture doc |
| `docs/FEATURES.md` | Section 23 (SSH Tunnel Manager) |
| `docs/user-guide/remote-access.md` | SSH Tunnel Management section |

### Remote Connection Manager
When modifying remote connection config, storage, or transport routing:

| File | What to update |
|------|----------------|
| `src-tauri/src/remote_connection.rs` | RemoteConnection, RemoteTransport, RemoteConnectionStore |
| `src/stores/remoteConnections.ts` | Frontend remote connections store |
| `src/utils/remoteEventBridge.ts` | SSE event bridge for remote daemons |
| `src/utils/transport.ts` | connectionId-based routing in COMMAND_TABLE |
| `src/utils/canvasTerminalTransport.ts` | baseUrl support for remote WebSocket |
| `docs/FEATURES.md` | Section 24 (Remote Connection Manager) |
| `docs/user-guide/remote-access.md` | Remote Connection Manager section |

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

### TUIC SDK & iframe Integration
When modifying the TUIC SDK, iframe postMessage protocol, path resolution, or tab injection:

| File | What to update |
|------|----------------|
| `src/components/PluginPanel/tuicSdk.ts` | Inline SDK script for plugin iframes |
| `src/components/PluginPanel/resolveTuicPath.ts` | Path resolution (relative/absolute, traversal guard) |
| `src/components/PluginPanel/PluginPanel.tsx` | Host-side message handlers, SDK injection |
| `docs/tuic-sdk.md` | SDK reference — API methods, path resolution, testing |
| `docs/examples/sdk-test.html` | Interactive test page (update when adding SDK methods) |
| `docs/plugins.md` | Plugin developer guide (if plugin-facing API changes) |

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
| `CONTRIBUTING.md` | Contributor guide (test requirements, PR quality gates) |
| `to-test.md` | Manual testing tracker |
| **docs/** | |
| `docs/FEATURES.md` | Canonical feature inventory (single source of truth) |
| `docs/plugins.md` | Plugin developer authoring guide |
| `docs/tuic-sdk.md` | TUIC SDK reference (inline + URL tab postMessage protocol) |
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
| `docs/backend/mcp-http.md` | MCP/HTTP server, lazy tool discovery, meta-tools |
| `docs/backend/dictation.md` | Whisper voice dictation |
| `docs/backend/error-classification.md` | Error types and backoff |
| `docs/frontend/STYLE_GUIDE.md` | Visual design rules |
| `docs/frontend/components.md` | Component tree reference |
| `docs/frontend/hooks.md` | Custom hooks |
| `docs/frontend/stores.md` | SolidJS stores |
| `docs/frontend/transport.md` | Tauri/HTTP dual-mode transport |
| `docs/frontend/utilities.md` | Utility function reference |
| `docs/features/ssh-tunnels.md` | SSH tunnel architecture and module map |
| `docs/user-guide/*.md` | User-facing guides (14 files) |
| **Code-embedded docs** | |
| `src-tauri/src/mcp_http/plugin_docs.rs` | AI-optimized plugin reference (`PLUGIN_DOCS` const) |
| `src/actions/actionRegistry.ts` | ACTION_META → auto-populates HelpPanel + Command Palette |
| `examples/plugins/` | Reference plugin implementations (6 examples) |
