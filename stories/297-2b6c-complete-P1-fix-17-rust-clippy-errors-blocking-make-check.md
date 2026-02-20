---
id: 297-2b6c
title: Fix 17 Rust clippy errors blocking make check
status: complete
priority: P1
created: "2026-02-20T19:23:06.080Z"
updated: "2026-02-20T19:34:50.492Z"
dependencies: []
---

# Fix 17 Rust clippy errors blocking make check

## Problem Statement

Rust clippy fails with 17 errors: collapsible-if patterns in fs.rs, git.rs, github.rs, pty.rs; let-and-return in fs.rs; redundant closure and &PathBuf->Path in github.rs; needless borrow in mcp_transport.rs. Blocks CI and make check.

## Acceptance Criteria

- [ ] All 17 clippy errors resolved
- [ ] make check passes with tsc ✓ and clippy ✓
- [ ] No functional behavior changed
- [ ] Verify before: run make check and confirm 17 errors
- [ ] Verify after: run make check and confirm 0 Rust errors

## Files

- src-tauri/src/fs.rs
- src-tauri/src/git.rs
- src-tauri/src/github.rs
- src-tauri/src/pty.rs
- src-tauri/src/mcp_http/mcp_transport.rs

## Work Log

### 2026-02-20T19:34:50.413Z - Fixed all 17 clippy errors: collapsible-if in fs.rs/git.rs/github.rs/pty.rs, let-and-return in fs.rs, redundant closure and &PathBuf->Path in github.rs, needless borrow in mcp_transport.rs. All 374 Rust tests pass, clippy clean.

