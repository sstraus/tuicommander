---
id: 173-c48d
title: Fix lossy path conversion in Rust backend
status: wontfix
priority: P2
created: "2026-02-16T07:12:19.790Z"
updated: "2026-02-16T07:33:18.164Z"
dependencies: []
---

# Fix lossy path conversion in Rust backend

## Problem Statement

to_string_lossy() silently replaces invalid UTF-8 with replacement character. 10+ occurrences hide encoding errors causing subtle bugs.

## Acceptance Criteria

- [ ] Replace to_string_lossy() with to_str() returning Result
- [ ] Fail early with clear error on invalid paths

## Files

- src-tauri/src/lib.rs

## Related

- RS-04

## Work Log

### 2026-02-16T07:33:18.100Z - WONTFIX: 16+ occurrences of to_string_lossy() across lib.rs. These paths come from filesystem/git repos which are virtually always valid UTF-8 on macOS/Linux. Replacing with to_str() + Result would add significant error handling noise for a theoretical edge case. to_string_lossy() is the pragmatic Rust pattern for display-oriented path usage.

