---
id: 406-32c7
title: Add path traversal guard to get_file_diff for untracked files
status: complete
priority: P1
created: "2026-02-26T21:07:12.333Z"
updated: "2026-02-27T09:08:04.467Z"
dependencies: []
---

# Add path traversal guard to get_file_diff for untracked files

## Problem Statement

`get_file_diff` in `git.rs:454` joins the user-supplied `file` param onto `repo_path` without canonicalization, then passes it to `git diff --no-index`. A caller can supply `file = "../../etc/passwd"` to read arbitrary files. The existing `read_file_impl` in `lib.rs:415-421` already has the correct `canonicalize + starts_with` guard.

## Acceptance Criteria

- [ ] Add canonicalize + `starts_with` guard before `full_path` is used in `--no-index` diff
- [ ] Return error "Access denied: file is outside repository" on traversal attempt
- [ ] Test: path traversal attempt returns error, not file content
- [ ] Mirror the pattern from `read_file_impl`

## Work Log

### 2026-02-27T09:08:04.392Z - Added canonicalize+starts_with path traversal guard to get_file_diff. Mirrors read_file_impl pattern. Test added.

