---
id: "049-5027"
title: "Lazygit per-repository configuration"
status: complete
priority: P3
created: 2026-02-04T17:06:31.283Z
updated: 2026-02-04T17:06:31.283Z
dependencies: []
---

# Lazygit per-repository configuration

## Problem Statement

Lazygit spawns with default config. Users may want custom lazygit configs per repo (themes, keybindings). Need to support repo-specific lazygit config files.

## Acceptance Criteria

- [ ] Check for .lazygit.yml in repo root before spawning
- [ ] Pass --use-config-file flag if config exists
- [ ] Allow global fallback to ~/.config/lazygit/config.yml
- [ ] Show config path in lazygit pane header
- [ ] Add settings option to specify custom config path

## Files

- src/App.tsx:217-229
- src/stores/settings.ts

## Work Log

