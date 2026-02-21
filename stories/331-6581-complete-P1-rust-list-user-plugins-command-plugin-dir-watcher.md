---
id: 331-6581
title: "Rust: list_user_plugins command + plugin dir watcher"
status: complete
priority: P1
created: "2026-02-21T14:51:35.067Z"
updated: "2026-02-21T15:31:13.859Z"
dependencies: []
---

# Rust: list_user_plugins command + plugin dir watcher

## Problem Statement

Frontend needs to discover installed plugins and get notified when plugins change for hot reload

## Acceptance Criteria

- [ ] Tauri command scans {app_data_dir}/plugins/ for manifest.json files
- [ ] Validates required fields: id, name, version, minAppVersion, main
- [ ] Returns Vec<PluginManifest>
- [ ] Invalid manifests logged and skipped
- [ ] File watcher emits plugin-changed events
- [ ] Unit tests

## Files

- src-tauri/src/lib.rs
- src-tauri/Cargo.toml

## Work Log

### 2026-02-21T15:31:13.783Z - All criteria already implemented in plugins.rs during story 330 (combined implementation). list_user_plugins, validate_manifest, start_plugin_watcher, sandboxed data storage all tested and passing.

