---
id: 162-757c
title: Fix command injection on Windows terminal path
status: complete
priority: P1
created: "2026-02-16T07:11:38.771Z"
updated: "2026-02-16T07:19:00.508Z"
dependencies: []
---

# Fix command injection on Windows terminal path

## Problem Statement

Path is concatenated into shell command string on Windows. Attacker-controlled path with shell metacharacters executes arbitrary code.

## Acceptance Criteria

- [ ] Split command arguments instead of string concatenation
- [ ] Test with paths containing shell metacharacters

## Files

- src-tauri/src/lib.rs

## Related

- SEC-02

## Work Log

### 2026-02-16T07:19:00.443Z - Fixed: replaced format!() string concatenation in Windows cmd /c with separate args to prevent shell metacharacter injection

