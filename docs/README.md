# TUICommander Documentation

- **[Complete Feature Reference](FEATURES.md)** — Canonical inventory of every feature, shortcut, and UI element

## User Guide

- [Getting Started](user-guide/getting-started.md) - First launch, workflow overview
- [Keyboard Shortcuts](user-guide/keyboard-shortcuts.md) - All shortcuts (terminal, tabs, panels, git)
- [Terminal Features](user-guide/terminals.md) - Tabs, splits, zoom, copy/paste, AI agents
- [File Browser & Code Editor](user-guide/file-browser.md) - Browse files, edit code, git status
- [Settings](user-guide/settings.md) - Groups, diff panel, repository config
- [Git Worktrees](user-guide/worktrees.md) - Worktree workflow, configuration, storage
- [GitHub Integration](user-guide/github-integration.md) - PR monitoring, CI rings, merge state
- [Voice Dictation](user-guide/dictation.md) - Push-to-talk setup, models, corrections
- [Prompt Library](user-guide/prompt-library.md) - Template management, variables

## Architecture

- [Overview](architecture/overview.md) - Tech stack, directory structure, hexagonal architecture
- [Data Flow](architecture/data-flow.md) - IPC communication, state management, event lifecycle
- [State Management](architecture/state-management.md) - Frontend stores, backend AppState, persistence

## Backend (Rust)

- [PTY Management](backend/pty.md) - Session lifecycle, reader threads, output processing
- [Git Operations](backend/git.md) - Repository info, diff, branches, worktrees
- [GitHub Integration](backend/github.md) - PR status, CI checks, GraphQL batching
- [Configuration](backend/config.md) - Config files, platform directories, migration
- [Dictation](backend/dictation.md) - Whisper transcription, audio capture, push-to-talk
- [MCP & HTTP Server](backend/mcp-http.md) - REST API, WebSocket streaming, Streamable HTTP transport
- [Error Classification](backend/error-classification.md) - Error types, backoff calculation
- [Output Parser](backend/output-parser.md) - Rate limits, PR URLs, progress detection

## Frontend (TypeScript/SolidJS)

- [Stores Reference](frontend/stores.md) - All reactive stores and their APIs
- [Hooks Reference](frontend/hooks.md) - All hooks and their return types
- [Components](frontend/components.md) - Component tree, panels, dialogs
- [Utilities](frontend/utilities.md) - Pure functions, helpers, type definitions
- [Transport Layer](frontend/transport.md) - Tauri IPC vs HTTP, dual-mode abstraction

## API Reference

- [Tauri Commands](api/tauri-commands.md) - All 73+ Tauri commands by module
- [HTTP API](api/http-api.md) - REST endpoints for MCP/remote access

## Guides

- [Project History](guides/project-history.md) - Timeline, milestones, contributor analysis
- [Development Setup](guides/development-setup.md) - Build, test, run instructions

## Existing Documents

These documents predate the structured docs and contain valuable context:

| Document | Description |
|----------|-------------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Original architecture deep-dive with data models |
| [UI_CHANGES.md](UI_CHANGES.md) | Removed/relocated UI elements |
| [proposals/split-panes.md](proposals/split-panes.md) | Split pane design proposal |
