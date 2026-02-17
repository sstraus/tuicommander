---
id: 114-62df
title: Cross-platform IDE launcher with dynamic detection
status: complete
priority: P2
created: "2026-02-15T14:06:11.507Z"
updated: "2026-02-15T17:39:39.572Z"
dependencies: []
---

# Cross-platform IDE launcher with dynamic detection

## Problem Statement

The IDE launcher is macOS-only: uses 'which' (not on Windows), 'open -a' (macOS only), hardcoded /Applications/ paths, and assumes Terminal/Finder always exist. Xcode and Sourcetree are detected but their launch handlers are missing (return Unknown app). The dropdown should show ONLY actually installed tools, and work on macOS, Linux, and Windows. Inspired by competitors' launcher which supports 21 targets across 4 categories.

## Acceptance Criteria

- [ ] ARCHITECTURE: Refactor detect_installed_ides and open_in_app in Rust to be cross-platform. Use cfg!(target_os) for platform-specific detection and launch logic. On Windows use 'where' instead of 'which'. On Windows launch via direct .exe paths. On Linux use 'xdg-open' for file manager and detect terminals via 'which'
- [ ] DETECTION STRATEGY per platform - macOS: CLI tools via 'which' (code, cursor, zed, windsurf, smerge, kitty), .app bundles via /Applications/ path check (Xcode, Sourcetree, Ghostty, WezTerm, Alacritty, Warp, GitHub Desktop, Fork, GitKraken, SmartGit, GitUp). Linux: all via 'which'. Windows: 'where' + common install paths (AppData/Local/Programs/)
- [ ] LAUNCH COMMANDS - macOS: CLI tools spawn directly (code, cursor, zed, kitty, smerge <path>), .app bundles use 'open -a AppName <path>'. Linux: CLI tools spawn directly, xdg-open for file manager. Windows: CLI tools spawn directly, 'explorer' for file manager
- [ ] SYSTEM UTILITIES per platform - macOS: Terminal + Finder always available. Linux: detect installed terminal (gnome-terminal, konsole, xterm) + file manager (nautilus, dolphin, thunar, or fallback xdg-open). Windows: cmd/PowerShell + Explorer always available
- [ ] FULL TARGET LIST: Editors: VS Code, VS Code Insiders, Cursor, Zed, Windsurf, Xcode (macOS only). Terminals: Ghostty, WezTerm, Alacritty, Kitty, Warp (macOS only), system terminal. Git Clients: Sourcetree, GitHub Desktop, Fork, GitKraken, SmartGit, Sublime Merge, GitUp (macOS only). System: platform file manager,  env var
- [ ] FRONTEND: Only render targets returned by detect_installed_ides. Zero hardcoded always-visible items. Group by category: Editors, Terminals, Git Clients, System. Show  entry only when env var is set
- [ ] FIX EXISTING BUGS: Add missing launch handlers for xcode (open -a Xcode) and sourcetree (open -a Sourcetree) that currently return Unknown app error
- [ ]  SUPPORT: In Rust, check env::var EDITOR. If set, include 'editor' in detected list with the binary name. Launch by spawning  <path>

## Files

- src-tauri/src/lib.rs
- src/stores/settings.ts
- src/components/IdeLauncher/IdeLauncher.tsx
- src/assets/icons/

## Work Log

### 2026-02-15T17:39:39.511Z - Refactored IDE launcher with cross-platform support. Added 11 new apps (Windsurf, Ghostty, WezTerm, Alacritty, Kitty, Warp, GitHub Desktop, Fork, GitKraken, Sublime Merge, $EDITOR). Fixed xcode/sourcetree bugs. Platform-specific detection and launch (macOS/Linux/Windows). Refactored dropdown to data-driven categories.

