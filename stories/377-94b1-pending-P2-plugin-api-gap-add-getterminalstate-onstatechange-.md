---
id: "377-94b1"
title: "Plugin API gap - add getTerminalState, onStateChange and git:read capability"
status: pending
priority: P2
created: 2026-02-25T07:45:42.397Z
updated: 2026-02-25T07:45:42.397Z
dependencies: []
---

# Plugin API gap - add getTerminalState, onStateChange and git:read capability

## Problem Statement

Research (session 95ef6175) identified: getTerminalState() + onStateChange() would unlock 5 high-value plugin ideas (Agent Session Monitor etc). git:read capability would unlock 3 more (Branch Diff Viewer, Commit Helper, PR Lint).

## Acceptance Criteria

- [ ] PluginHost Tier 2: add getTerminalState() returning { sessionId, repoPath, agentActive, agentType, pid }
- [ ] PluginHost Tier 2: add onStateChange(callback) firing on agent start/stop and branch change
- [ ] Add git:read capability allowing read-only git commands (get_branches, get_recent_commits, get_git_diff)
- [ ] Update KNOWN_CAPABILITIES in src-tauri/src/plugins.rs
- [ ] Update PluginHost interface in src/plugins/types.ts and pluginRegistry.ts
- [ ] Update docs/plugins.md, plugin_docs.rs PLUGIN_DOCS, make check passes

## Files

- src/plugins/types.ts
- src/plugins/pluginRegistry.ts
- src-tauri/src/plugins.rs
- docs/plugins.md
- src-tauri/src/mcp_http/plugin_docs.rs

## Work Log

