---
id: 337-65c1
title: Integration test and make check
status: complete
priority: P1
created: "2026-02-21T14:51:35.071Z"
updated: "2026-02-21T15:46:48.062Z"
dependencies: ["333-ad87", "334-8eb6"]
---

# Integration test and make check

## Problem Statement

Need to verify the complete plugin system works end-to-end

## Acceptance Criteria

- [ ] hello-world sample loads and works
- [ ] Syntax error plugin is skipped gracefully
- [ ] onload-throw plugin is skipped gracefully
- [ ] Capability denial works (writePty without pty:write)
- [ ] Hot reload works (modify file, verify re-registration)
- [ ] make check green (tsc + clippy + tests)

## Work Log

### 2026-02-21T15:46:47.935Z - make check passes: tsc clean, clippy clean (fixed collapsible_if), 394 Rust tests, 1846 TS tests. npm audit failure is pre-existing (minimatch/purgecss).

