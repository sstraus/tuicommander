# Ideas Index

Feature concepts under evaluation for TUICommander. Each idea lives in its own file for easy reading and updating.

**Status legend:**
- `concept` — Just an idea, needs discussion
- `validated` — We agree it's worth doing, needs design
- `designed` — Approach decided, ready to implement
- `partially implemented` — Some pieces exist, more work needed
- `done` — Completed, kept for reference
- `rejected` — Evaluated and discarded (kept for memory)
- `moved` — Promoted to SPEC.md / shipped

---

## Active Ideas

| Status | Idea | File |
|--------|------|------|
| `concept` | Native Stories (API-first work items, native Kanban, plans, GitHub origin, parallel lanes, CLI/MCP parity, tuic-remote) | [native-stories.md](native-stories.md) |
| `concept` | Work-item providers (GitHub, Jira, GitLab) as plugins: Account Manager, plugin Settings tabs, work-item surface, host APIs (full analysis) | [orca-jira-github-plugins.md](orca-jira-github-plugins.md) |
| `moved` | Automations: cron-scheduled agent runs with precheck, grace window, run history (Orca as reference) | [automations-scheduler.md](automations-scheduler.md) → [plan](../plans/automations-scheduler.md) |
| `moved` | Remote Repo Browser (pick a repo on a `tuic-remote` machine instead of typing its path) | [remote-repo-browser.md](remote-repo-browser.md) |
| `concept` | Kokoro Neural TTS as a Downloadable Addon (parked behind the `tuic-voice` plugin) | [kokoro-tts-addon.md](kokoro-tts-addon.md) |
| `moved` | Build Artifacts Cleaner (scan/clean target, node_modules, .venv; threshold alerts) | [build-cleaner-plugin.md](build-cleaner-plugin.md) |
| `concept` | VM / Computer Use (Linux desktop container an agent drives via Cua MCP, noVNC viewer; parked, from the OpenMausBot study) | [vm-computer-use.md](vm-computer-use.md) |
| `concept` | MCP Channels (push peer mail + permission relay into agent sessions) | [mcp-channels.md](mcp-channels.md) |
| `concept` | MCP 2026-07-28 Modern Era (server/discover, stateless identity, tasks/* front door) | [mcp-2026-07-28-modern-era.md](mcp-2026-07-28-modern-era.md) |
| `concept` | SSH Native Connections (config-aware, managed, remote integration) | [ssh-native-connections.md](ssh-native-connections.md) |
| `moved` | AI-Assisted Terminal (chat + agent loop + knowledge base, Levels 1-3 shipped) | [ai-assisted-terminal.md](ai-assisted-terminal.md) |
| `moved` | AI Chat Widget (subsumed by ai-assisted-terminal Level 1) | [ai-chat-widget.md](ai-chat-widget.md) |
| `concept` | Multi-Project Progress View (project selector + git correlation from entry timestamps) | [multi-project-progress.md](multi-project-progress.md) |
| `concept` | Rich File Preview (PDF, CSV, images, video inline) | [rich-file-preview.md](rich-file-preview.md) |
| `concept` | Named Workspaces (user-managed, persistent layouts) | [named-workspaces.md](named-workspaces.md) |
| `concept` | Tab Backgrounds and Advanced Theming | [tab-backgrounds-theming.md](tab-backgrounds-theming.md) |
| `concept` | Command Blocks (per-command isolation) | [command-blocks.md](command-blocks.md) |
| `concept` | Secret Store UI (expose keyring to users) | [secret-store-ui.md](secret-store-ui.md) |
| `concept` | Per-Terminal Profiles (connection theming) | [per-terminal-profiles.md](per-terminal-profiles.md) |
| `validated` | WSL Support (Windows Subsystem for Linux) | [wsl-support.md](wsl-support.md) |
| `concept` | AI CLI Pipe (terminal output to AI) | [ai-cli-pipe.md](ai-cli-pipe.md) |
| `concept` | GUI Control from Terminal (wsh-style CLI) | [gui-control-from-terminal.md](gui-control-from-terminal.md) |
| `concept` | Cross-Host File Operations (depends on SSH) | [cross-host-file-ops.md](cross-host-file-ops.md) |
| `moved` | Detachable Panels: Generic Multi-Window Panel System | [detachable-panels.md](detachable-panels.md) |
| `concept` | Panel Groups — Tabbed Panel Windows (VSCode-style) | [panel-groups-tabbed.md](panel-groups-tabbed.md) |
| `concept` | Panel Wave Blocks — Tiling Block Layout (Wave-style) | [panel-wave-blocks.md](panel-wave-blocks.md) |
| `concept` | Multi-Monitor: Dependent Secondary Window | [multi-monitor-window.md](multi-monitor-window.md) |
| `partially implemented` | Cross-Repo Knowledge Base (mdkb) | [cross-repo-semantic-search.md](cross-repo-semantic-search.md) |
| `concept` | GitHub-Style Code Navigation (mdkb subprocess) | [code-navigation-mdkb.md](code-navigation-mdkb.md) |
| `concept` | Move Business Logic to Rust | [business-logic-to-rust.md](business-logic-to-rust.md) |
| `concept` | HTTP Server Audit (compression + auth) | [http-server-audit.md](http-server-audit.md) |
| `validated` | Smart Prompts — AI Automation Layer | [smart-prompts.md](smart-prompts.md) |
| `concept` | Codebase Decomposition (god components + struct) | [codebase-decomposition.md](codebase-decomposition.md) |
| `concept` | Kitty Keyboard Protocol Full (flags 2-16) | [kitty-keyboard-full.md](kitty-keyboard-full.md) |
| `designed` | Agent Swarm Flow (orchestration protocol) | [agent-swarm-flow.md](agent-swarm-flow.md) |
| `moved` | ACP — Agent Client Protocol Integration (client shipped; ego is the chat engine) | [acp-integration.md](acp-integration.md) |
| `concept` | MCP Channel Support for Swarm (push replaces polling) | [mcp-channel-swarm.md](mcp-channel-swarm.md) |
| `concept` | Terminal Virtualization (dispose inactive xterm) | [terminal-virtualization.md](terminal-virtualization.md) |
| `concept` | Diff Scroll View: Include Untracked Files | [diff-scroll-untracked.md](diff-scroll-untracked.md) |
| `concept` | Capacitor Native App (push notifications) | [capacitor-native-app.md](capacitor-native-app.md) |
| `concept` | P2P WebRTC with Relay Fallback | [p2p-webrtc-relay.md](p2p-webrtc-relay.md) |
| `concept` | Terminal UX Standards (missing behaviors) | [terminal-ux-standards.md](terminal-ux-standards.md) |
| `concept` | Disk-Backed Terminal Log + Global History Search | [disk-backed-terminal-log.md](disk-backed-terminal-log.md) |
| `concept` | SQLite App Logs (persistent, queryable log storage) | [sqlite-app-logs.md](sqlite-app-logs.md) |
| `concept` | Agent Sandbox (per-tab PTY policy engine) | [agent-sandbox.md](agent-sandbox.md) |
| `concept` | PR Dashboard Panel | [pr-dashboard-panel.md](pr-dashboard-panel.md) |
| `concept` | Per-Repo MCP Server Definitions (.tuic.json) | [mcp-per-repo-definitions.md](mcp-per-repo-definitions.md) |
| `concept` | Dynamic MCP Server Injection (headersHelper + .mcp.json) | [dynamic-mcp-injection.md](dynamic-mcp-injection.md) |
| `designed` | One-Click Task → Worktree → Agent | [one-click-task-worktree.md](one-click-task-worktree.md) |
| `concept` | CC Native Integration (channels, task bridge, swarm) | [cc-native-integration.md](cc-native-integration.md) |
| `concept` | Remote-Controlled Tab Indicator (live WS-based) | [remote-controlled-tab-indicator.md](remote-controlled-tab-indicator.md) |
| `concept` | Cross-Repo Split Panes (multi-repo side by side) | [cross-repo-split-panes.md](cross-repo-split-panes.md) |
| `concept` | MCP Proxy Elicitation (structured user input mid-call) | [mcp-proxy-elicitation.md](mcp-proxy-elicitation.md) |
| `concept` | Inline Issue Comments in GitHub Panel (Phase 2) | [github-issue-inline-comments.md](github-issue-inline-comments.md) |
| `concept` | Alt-Screen Scrollback on Windows (Codex wheel-scroll bug) | [altbuf-scrollback.md](altbuf-scrollback.md) |
| `concept` | Inline Diff Review Tab (structured diff in dedicated tab) | [inline-diff-review.md](inline-diff-review.md) |
| `concept` | Cross-Session Conversation Search (full-text across sessions) | [cross-session-search.md](cross-session-search.md) |
| `moved` | Claude Code Worktree Compatibility | [claude-code-worktree-compat.md](claude-code-worktree-compat.md) |
| `moved` | Swarm Mode Performance Issues (profiled) | [swarm-perf-issues.md](swarm-perf-issues.md) |
| `concept` | wgpu Native Terminal Renderer | [wgpu-terminal-renderer.md](wgpu-terminal-renderer.md) |
| `concept` | Terminal Soft-Wrap on Panel Resize (alacritty fork) | [terminal-soft-wrap-resize.md](terminal-soft-wrap-resize.md) |
| `concept` | WebGL Terminal Renderer (in-WebView GPU acceleration) | [webgl-terminal-renderer.md](webgl-terminal-renderer.md) |

## Shipped / Done

| Status | Idea | File |
|--------|------|------|
| `moved` | Remote Settings Context (context-aware settings panel) | [remote-settings-context.md](remote-settings-context.md) |
| `moved` | Remote Daemon (tuicommander-remote + SSH tunnels + connection manager) | [remote-daemon.md](remote-daemon.md) |
| `moved` | CI Heal Auto-Loop | [ci-heal-auto-loop.md](ci-heal-auto-loop.md) |
| `moved` | Pre-Archive Script (lifecycle hook) | [pre-archive-script.md](pre-archive-script.md) |
| `moved` | Repo-Local Config (.tuic.json) | [repo-local-config.md](repo-local-config.md) |
| `moved` | PR Review from Popover | [pr-review-from-popover.md](pr-review-from-popover.md) |
| `moved` | Agent Teams Integration (it2 Shim) | [agent-teams-tmux-shim.md](agent-teams-tmux-shim.md) |
| `moved` | MCP Proxy Hub (aggregate upstream servers) | [mcp-proxy-hub.md](mcp-proxy-hub.md) |
| `moved` | Command Palette | [command-palette.md](command-palette.md) |
| `moved` | Activity Dashboard | [activity-dashboard.md](activity-dashboard.md) |
| `moved` | Live Dictation | [live-dictation.md](live-dictation.md) |
| `moved` | Remote Access Mode | [remote-access-mode.md](remote-access-mode.md) |
| `moved` | Clickable File Paths | [clickable-file-paths.md](clickable-file-paths.md) |
| `moved` | Project File Browser | [project-file-browser.md](project-file-browser.md) |
| `moved` | Integrated Code Editor (CodeMirror 6) | [codemirror-editor.md](codemirror-editor.md) |
| `moved` | Plugin Filesystem API | [plugin-filesystem-api.md](plugin-filesystem-api.md) |
| `moved` | Cost Tracking (Claude Usage Dashboard) | [cost-tracking-plugin.md](cost-tracking-plugin.md) |
| `moved` | Drag-and-Drop Repo Reordering | [drag-drop-repo-reorder.md](drag-drop-repo-reorder.md) |
| `moved` | Move Terminal to Worktree | [move-terminal-to-worktree.md](move-terminal-to-worktree.md) |
| `moved` | Quick Branch Switcher | [quick-branch-switcher.md](quick-branch-switcher.md) |
| `done` | TERM_PROGRAM=kitty workaround | [term-program-kitty-workaround.md](term-program-kitty-workaround.md) |
| `done` | PR Merge Readiness Status | [pr-merge-readiness.md](pr-merge-readiness.md) |
| `done` | Periodic Worktree Status Refresh | [worktree-status-refresh.md](worktree-status-refresh.md) |
| `done` | UX Study: Compare with VS Code | [ux-study-vs-code.md](ux-study-vs-code.md) |
| `done` | Terminal Status Dot Redesign | [terminal-status-dot-redesign.md](terminal-status-dot-redesign.md) |

## Rejected

| Status | Idea | File |
|--------|------|------|
| `rejected` | Multi-Instance Support | [multi-instance-support.md](multi-instance-support.md) |
| `rejected` | Context Capture | [context-capture.md](context-capture.md) |
| `rejected` | Structured Agent Output Protocol | [structured-agent-output.md](structured-agent-output.md) |
| `rejected` | Replace xterm.js with ghostty-web | [ghostty-web-terminal.md](ghostty-web-terminal.md) |
| `rejected` | Notification System (Popover + Toasts) | [notification-system.md](notification-system.md) |
| `rejected` | Unified Notification Service | [unified-notification-service.md](unified-notification-service.md) |
| `rejected` | Analytics (editor settings done) | [analytics-editor-settings.md](analytics-editor-settings.md) |
| `rejected` | Session-Aware Resume Phase 2 | [session-aware-resume-phase2.md](session-aware-resume-phase2.md) |
| `rejected` | Archived Worktrees View | [archived-worktrees-view.md](archived-worktrees-view.md) |
| `rejected` | Multi-Select Sidebar Actions | [multi-select-sidebar.md](multi-select-sidebar.md) |
