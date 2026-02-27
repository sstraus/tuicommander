---
id: 409-c1bc
title: Stream untracked file line count instead of read_to_string
status: complete
priority: P2
created: "2026-02-26T21:07:12.572Z"
updated: "2026-02-27T11:20:28.074Z"
dependencies: []
---

# Stream untracked file line count instead of read_to_string

## Problem Statement

`get_changed_files` in `git.rs:431` uses `read_to_string` to load entire untracked files into memory just to count lines. A 50MB lock file or binary allocates the full content then discards it. Should use `BufReader::lines()` to stream line-by-line (8KB chunks).

## Acceptance Criteria

- [ ] Replace `read_to_string` with `BufReader::lines().count()` for untracked file line counting
- [ ] Binary files (invalid UTF-8) should still return 0 gracefully
- [ ] No full-file heap allocation for line counting

## QA

None — covered by existing get_changed_files tests

## Work Log

### 2026-02-27T11:20:27.944Z - Replaced read_to_string with BufReader::lines() for streaming line count. Binary files (invalid UTF-8) return 0 gracefully by stopping on first error. No heap allocation for file content.

