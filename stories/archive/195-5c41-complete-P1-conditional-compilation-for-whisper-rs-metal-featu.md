---
id: 195-5c41
title: Conditional compilation for whisper-rs Metal feature
status: complete
priority: P1
created: "2026-02-16T13:20:02.390Z"
updated: "2026-02-16T13:38:23.668Z"
dependencies: []
---

# Conditional compilation for whisper-rs Metal feature

## Problem Statement

Cargo.toml unconditionally enables whisper-rs metal feature which is Apple-only GPU API. Project will not compile on Windows or Linux.

## Acceptance Criteria

- [ ] Use target-specific dependencies: metal feature only on macOS
- [ ] Use no GPU acceleration (or cuda) on Windows/Linux as default
- [ ] Verify cargo check passes with cfg checks

## Files

- src-tauri/Cargo.toml

## Work Log

### 2026-02-16T13:38:23.594Z - Split whisper-rs dep: base in [dependencies], metal feature in target macOS-only section

