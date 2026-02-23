---
id: 149-1bf2
title: Remote access settings UI in Services tab
status: complete
priority: P2
created: "2026-02-15T23:38:45.009Z"
updated: "2026-02-16T00:21:12.565Z"
dependencies: ["148-ff15"]
---

# Remote access settings UI in Services tab

## Problem Statement

Users need to configure remote access (enable/disable, port, username, password) from the Settings panel. The Services tab already has MCP toggle — add remote access section below it.

## Acceptance Criteria

- [ ] Add Remote Access section to ServicesTab with enable toggle, port input, username/password fields
- [ ] Show connection URL when enabled (http://{lan-ip}:{port})
- [ ] Password field shows dots, with reveal toggle
- [ ] Save triggers config update and server restart if needed
- [ ] Show server status (running/stopped) like MCP status
- [ ] Warn that remote access exposes terminals to the network

## Files

- src/components/SettingsPanel/tabs/ServicesTab.tsx
- src/styles.css

## Work Log

### 2026-02-16T00:21:12.502Z - Added remote access settings UI to ServicesTab: enable toggle with network warning, port/username/password fields, show/hide password, connection URL display, save button with bcrypt hashing. Added hash_password Tauri command and HTTP endpoint. Added transport mapping. Added CSS styles for new classes.

