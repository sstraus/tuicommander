---
id: 036-968a
title: Per-repository settings panel with worktree configuration
status: pending
priority: P2
created: "2026-02-04T12:05:24.997Z"
updated: "2026-02-04T12:05:31.018Z"
dependencies: ["033-9a09"]
---

# Per-repository settings panel with worktree configuration

## Problem Statement

Repositories need per-repo configuration for worktree behavior, setup scripts, and run scripts. Currently there is no way to customize how worktrees are created or to define automation scripts per repository.

## Acceptance Criteria

- [ ] Context menu on repo in sidebar with Repo Settings and Remove Repository options
- [ ] Settings modal/panel with left sidebar navigation (General, Notifications, Worktree, Updates, GitHub)
- [ ] Display repo name and full path at top of settings
- [ ] Branch new workspaces from: dropdown to select base branch (Automatic/origin/main, or specific branch)
- [ ] Worktree section with toggles: Copy ignored files, Copy untracked files
- [ ] Setup Script: textarea for script executed once after worktree creation
- [ ] Run Script: textarea for script launchable on-demand from toolbar
- [ ] Per-repo list in settings sidebar to switch between configured repos
- [ ] Persist settings to config file per repository
- [ ] Backend commands to execute setup/run scripts in repo context

## Files

- index.html
- src/main.ts
- src/styles.css
- src-tauri/src/lib.rs

## Work Log

