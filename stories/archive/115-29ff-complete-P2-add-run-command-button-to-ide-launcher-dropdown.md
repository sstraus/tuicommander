---
id: 115-29ff
title: Add Run command button to IDE launcher dropdown
status: complete
priority: P2
created: "2026-02-15T14:07:52.667Z"
updated: "2026-02-15T17:49:35.285Z"
dependencies: []
---

# Add Run command button to IDE launcher dropdown

## Problem Statement

There is no way to save and quickly re-run a development command (like 'npm run dev', 'cargo watch', 'make dev') per worktree. Users have to manually type the command each time. a competitor has a dedicated Run button that opens a dialog to enter a command, saves it to repo settings, and runs it in a terminal with one click.

## Acceptance Criteria

- [ ] UI: Add a 'Run' entry at the bottom of the IDE launcher dropdown menu (after Copy Path), with a play icon and Cmd+R shortcut hint. If no command is saved yet, show 'Run...' to indicate it will prompt. If a command is saved, show 'Run: <command>' truncated
- [ ] DIALOG: Clicking Run when no command is saved opens a modal dialog titled 'Run' with description 'Enter a command to run in this worktree. It will be saved to repository settings.' Contains a textarea pre-filled with the saved command (if any), Cancel button, and 'Save and Run' primary button
- [ ] PERSISTENCE: Save the run command per-repo per-branch in the repositories store (add runCommand field to BranchState). Persist to localStorage with the rest of the repo state. Each worktree/branch can have its own run command
- [ ] EXECUTION: 'Save and Run' saves the command to the store and opens a new terminal tab in the worktree's cwd, executing the saved command. The terminal tab name should be the command (truncated)
- [ ] RE-RUN: If a command is already saved, clicking Run in the dropdown immediately executes it (no dialog). To edit the saved command, add a small edit icon or hold Shift+click to reopen the dialog
- [ ] KEYBOARD: Cmd+R shortcut triggers Run from anywhere (add to App.tsx keyboard handler). If no command saved, opens dialog. If command saved, runs immediately
- [ ] DIALOG STYLING: Match the a competitor dialog aesthetic - dark background, monospace font for the textarea, rounded corners, blur backdrop, Cancel and Save and Run buttons

## Files

- src/components/IdeLauncher/IdeLauncher.tsx
- src/stores/repositories.ts
- src/App.tsx
- src/styles.css

## Work Log

### 2026-02-15T17:49:35.223Z - Added runCommand to BranchState, created RunCommandDialog component, added Run button to IdeLauncher dropdown with play icon and ⌘R hint, implemented Cmd+R/Cmd+Shift+R shortcuts, executeRunCommand creates terminal tab with command, updated HelpPanel. 8 new tests, 892 total pass.

