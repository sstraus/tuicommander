---
id: 378-4835
title: mdkb Dashboard - external installable plugin for mdkb status and config panel
status: complete
priority: P2
created: "2026-02-25T07:45:42.398Z"
updated: "2026-02-25T08:28:30.342Z"
dependencies: []
---

# mdkb Dashboard - external installable plugin for mdkb status and config panel

## Problem Statement

Session 95ef6175 researched this. Architecture: fs:read for config.toml, net:http for mdkb HTTP server (MCP-over-HTTP) for status/stats/memory. Targets active repo path, shows index stats, memory list, config, Edit Config button.

## Acceptance Criteria

- [ ] External plugin in tuicommander-plugins submodule (not native built-in)
- [ ] manifest.json: capabilities fs:read, net:http, ui:panel, ui:ticker
- [ ] Panel: index health, memory list (searchable), config.toml read-only, Edit Config button opens file in TUICommander editor
- [ ] Status bar ticker: compact stats (N docs / N memories)
- [ ] Targets active repo .mdkb/ path, handles server offline gracefully
- [ ] Added to plugins/registry.json, make check passes

## Files

- plugins/mdkb-dashboard/manifest.json
- plugins/mdkb-dashboard/main.js
- plugins/registry.json

## Work Log

### 2026-02-25T08:28:30.275Z - Plugin already implemented by prior session (548 lines). Committed to submodule with manifest, pushed to origin.

