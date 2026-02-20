---
id: "260-de57"
title: "escapeShellArg uses Unix-only single-quote escaping"
status: pending
priority: P1
created: 2026-02-20T07:38:38.164Z
updated: 2026-02-20T07:38:38.164Z
dependencies: []
---

# escapeShellArg uses Unix-only single-quote escaping

## Problem Statement

shell.ts escapeShellArg wraps args in single quotes using POSIX escaping. PowerShell uses double quotes with backtick escaping; cmd.exe uses double quotes with caret escaping. Every call site that writes to the PTY on Windows produces malformed or broken commands.

## Acceptance Criteria

- [ ] escapeShellArg detects target shell and applies correct quoting
- [ ] Or: replace PTY string-building with structured command arrays that avoid shell escaping entirely
- [ ] All existing call sites produce correct output on Windows

## Files

- src/utils/shell.ts

## Work Log

