---
id: 374-e52f
title: Fix plugin_exec.rs security - untrusted binary path via which
status: complete
priority: P1
created: "2026-02-25T07:45:22.405Z"
updated: "2026-02-25T08:27:38.949Z"
dependencies: []
---

# Fix plugin_exec.rs security - untrusted binary path via which

## Problem Statement

plugin_exec.rs uses which output to resolve binary path for exec:cli. A symlink to a malicious binary passes validation. Must validate against trusted directory allowlist instead.

## Acceptance Criteria

- [ ] Drop which lookup in resolve_binary, use hardcoded candidate paths only
- [ ] Symlink resolution: canonicalize() then re-validate canonical path against allowlist
- [ ] Trusted dirs: /usr/local/bin, ~/.cargo/bin, /opt/homebrew/bin etc
- [ ] Add unit tests: valid path, symlink attack, out-of-allowlist path
- [ ] make check passes

## Files

- src-tauri/src/plugin_exec.rs

## Work Log

### 2026-02-25T08:27:38.881Z - Removed which/where PATH lookup from resolve_binary(). Now uses only hardcoded candidate paths (trusted_dirs). Added canonicalize() + is_in_trusted_dir() to prevent symlink attacks. 11 tests passing.

