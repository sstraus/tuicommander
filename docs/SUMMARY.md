# Summary

[Introduction](./index.md)
[Feature Overview](./FEATURES.md)

---

# User Guide

- [Getting Started](./user-guide/getting-started.md)
- [TUICommander Modes](./user-guide/modes.md)
- [Troubleshooting](./user-guide/troubleshooting.md)

## Workspace

- [Terminals](./user-guide/terminals.md)
- [Sidebar](./user-guide/sidebar.md)
- [File Browser](./user-guide/file-browser.md)
- [Command Palette](./user-guide/command-palette.md)
- [Keyboard Shortcuts](./user-guide/keyboard-shortcuts.md)
- [Settings](./user-guide/settings.md)
- [Design Mode](./user-guide/design-mode.md)

## AI Agents

- [Agent Support](./user-guide/ai-agents.md)
- [Agent Teams](./user-guide/agent-teams.md)
- [Automations](./user-guide/automations.md)
- [Project Progress](./user-guide/project-progress.md)
- [Progress Reporting Evaluation](./evaluations/progress-reporting.md)
- [AI Chat](./user-guide/ai-chat.md)
- [Smart Prompts](./user-guide/smart-prompts.md)
- [Prompt Library](./user-guide/prompt-library.md)
- [Voice Dictation](./user-guide/dictation.md)

## Git & GitHub

- [Branch Management](./user-guide/branches.md)
- [Worktrees](./user-guide/worktrees.md)
- [GitHub Integration](./user-guide/github-integration.md)

## Extensibility

- [CLI Companion (tuic)](./user-guide/cli.md)
- [Plugins](./user-guide/plugins.md)
- [MCP Proxy Hub](./user-guide/mcp-proxy.md)
- [Remote Access](./user-guide/remote-access.md)
- [SSH Tunnels](./features/ssh-tunnels.md)

---

# Architecture

- [Overview](./architecture/overview.md)
- [Data Flow](./architecture/data-flow.md)
- [State Management](./architecture/state-management.md)
- [Terminal State Machine](./architecture/terminal-state-machine.md)

## Architecture Decisions

- [Atomic MCP Agent Submission](./decisions/2026-08-27-atomic-mcp-agent-submission.md)

## Agent Detection

- [Agent UI Analysis](./architecture/agent-ui-analysis.md)
- [Detection Matrix](./architecture/agents/detection-matrix.md)
- [Claude Code](./architecture/agents/claude-code.md)
- [Codex](./architecture/agents/codex.md)
- [Aider](./architecture/agents/aider.md)
- [Gemini CLI](./architecture/agents/gemini-cli.md)
- [OpenCode](./architecture/agents/opencode.md)
- [pi](./architecture/agents/pi.md)

---

# Backend

- [Configuration](./backend/config.md)
- [PTY Management](./backend/pty.md)
- [Output Parser](./backend/output-parser.md)
- [Error Classification](./backend/error-classification.md)
- [Git Operations](./backend/git.md)
- [GitHub Integration](./backend/github.md)
- [MCP & HTTP Server](./backend/mcp-http.md)
- [ACP Client for ego](./backend/acp.md)
- [MCP Proxy Hub](./backend/mcp-proxy.md)
- [Voice Dictation](./backend/dictation.md)
- [Alacritty Integration](./backend/alacritty-integration.md)
- [VT100-to-PWA Protocol](./backend/vt100-PWA.md)

---

# Frontend

- [Components](./frontend/components.md)
- [Stores](./frontend/stores.md)
- [Hooks](./frontend/hooks.md)
- [Utilities](./frontend/utilities.md)
- [Transport Layer](./frontend/transport.md)
- [Terminal Features](./frontend/terminal-features.md)
- [Custom Glyph Rendering](./frontend/custom-glyph-rendering.md)
- [Visual Style Guide](./frontend/STYLE_GUIDE.md)
- [Plugin Dashboard Style Guide](./plugins-style.md)

---

# API Reference

- [HTTP API](./api/http-api.md)
- [Tauri Commands](./api/tauri-commands.md)
- [TUIC SDK](./tuic-sdk.md)
- [Plugin Authoring](./plugins.md)

---

# Developer Guide

- [Development Setup](./guides/development-setup.md)
- [Continuous Integration](./guides/ci.md)
- [Performance Profiling](./guides/profiling.md)
- [Documentation Sync Matrix](./sync-matrix.md)
- [Release & Tag Checklist](./release-checklist.md)
- [Project History](./guides/project-history.md)

## Audits & Plans

- [CanvasTerminal Feature Audit](./frontend/canvas-terminal-audit.md)
- [Font Size Audit](./frontend/font-size-audit.md)
- [Keyboard Shortcuts Comparison](./frontend/KEYBOARD_SHORTCUTS_COMPARISON.md)
- [Solid Refactoring Plan](./frontend/solid-refactoring-plan.md)
