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
| `docs/backend/mcp-http.md` | Server architecture, routing, lazy tool discovery (`collapse_tools` / meta-tools) |
| `docs/user-guide/remote-access.md` | User setup guide |
| `src-tauri/src/mcp_http/plugin_docs.rs` | PLUGIN_DOCS (if plugin-facing) |

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

### AI Chat
When modifying AI Chat panel, settings, context menu actions, or streaming backend:

| File | What to update |
|------|----------------|
| `src-tauri/src/ai_chat.rs` | Backend: config, streaming, context assembly, Ollama detection |
| `src/stores/aiChatStore.ts` | Frontend store: messages, streaming state, terminal attachment |
| `src/components/AIChatPanel/AIChatPanel.tsx` | Chat panel component |
| `src/components/AIChatPanel/contextMenuActions.ts` | Terminal context menu integration |
| `src/components/SettingsPanel/tabs/AiChatTab.tsx` | Settings panel section |
| `src/stores/ui.ts` | `aiChatPanelVisible` + `aiChatPanelWidth` signals |
| `src/keybindingDefaults.ts` | `toggle-ai-chat` hotkey |
| `docs/FEATURES.md` | AI Chat feature section |
| `docs/user-guide/ai-chat.md` | User-facing guide (create when needed) |

### AI Agent (ReAct loop, knowledge store, MCP terminal tools)
When modifying the AI agent loop engine, tool dispatch, session knowledge store,
OSC 133 outcome capture, or the `ai_terminal_*` MCP tools:

| File | What to update |
|------|----------------|
| `src-tauri/src/ai_agent/engine.rs` | ReAct loop, approval flow, ACTIVE_AGENTS registry, system prompt |
| `src-tauri/src/ai_agent/tools.rs` | Tool dispatch: 6 terminal + 6 filesystem tools |
| `src-tauri/src/ai_agent/safety.rs` | SafetyChecker: command safety + file-write sensitive path rules |
| `src-tauri/src/ai_agent/sandbox.rs` | FileSandbox: path jail for filesystem tools (canonicalize + starts_with) |
| `src-tauri/src/mcp_http/ai_terminal.rs` | MCP exposure of all 12 `ai_terminal_*` tools; write-tool confirmation |
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
| `docs/user-guide/*.md` | User-facing guides (13 files) |
| **Code-embedded docs** | |
| `src-tauri/src/mcp_http/plugin_docs.rs` | AI-optimized plugin reference (`PLUGIN_DOCS` const) |
| `src/actions/actionRegistry.ts` | ACTION_META → auto-populates HelpPanel + Command Palette |
| `examples/plugins/` | Reference plugin implementations (6 examples) |
