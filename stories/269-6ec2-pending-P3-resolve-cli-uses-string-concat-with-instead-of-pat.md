---
id: "269-6ec2"
title: "resolve_cli uses string concat with / instead of Path::join"
status: pending
priority: P3
created: 2026-02-20T07:38:38.175Z
updated: 2026-02-20T07:38:38.175Z
dependencies: []
---

# resolve_cli uses string concat with / instead of Path::join

## Problem Statement

agent.rs resolve_cli builds candidate paths with format!("{dir}/{name}") using a literal / separator instead of std::path::Path::join. Works on Windows since Windows accepts / but is unconventional.

## Acceptance Criteria

- [ ] Path construction uses PathBuf::join or Path::new(dir).join(name)
- [ ] No behaviour change on any platform

## Files

- src-tauri/src/agent.rs

## Work Log

