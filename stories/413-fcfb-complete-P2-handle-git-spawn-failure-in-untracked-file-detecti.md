---
id: 413-fcfb
title: Handle git spawn failure in untracked file detection instead of silent fallthrough
status: complete
priority: P2
created: "2026-02-26T21:07:12.884Z"
updated: "2026-02-27T11:00:32.108Z"
dependencies: []
---

# Handle git spawn failure in untracked file detection instead of silent fallthrough

## Problem Statement

In `git.rs:460`, if `git ls-files --error-unmatch` fails to spawn (e.g. git not on PATH in release build), `unwrap_or(false)` silently assumes the file is tracked. The function falls through to normal `git diff` which also produces empty output — user sees no diff and no error.

## Acceptance Criteria

- [ ] Log the spawn failure via `eprintln!` or `appLogger` equivalent
- [ ] Maintain fallthrough behavior (don't hard-error) but ensure the failure is observable in logs
- [ ] Also check `--no-index` diff exit code: exit > 1 means git error, not "files differ"

## QA

None — defensive logging, no behavioral change

## Work Log

### 2026-02-27T11:00:31.973Z - Completed: Replaced unwrap_or(false) with match that logs eprintln on spawn failure. Added exit code > 1 check for --no-index diff to distinguish 'files differ' (1) from real errors (>1). Rust compiles clean, 717 tests pass.

