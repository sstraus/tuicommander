---
id: "265-efe5"
title: "isValidPath does not block Windows shell metacharacters"
status: pending
priority: P2
created: 2026-02-20T07:38:38.167Z
updated: 2026-02-20T07:38:38.167Z
dependencies: []
---

# isValidPath does not block Windows shell metacharacters

## Problem Statement

shell.ts isValidPath does not reject Windows shell metacharacters like % (cmd.exe variable expansion) or ^ (cmd.exe escape char). Secondary to the shell syntax issue but needs addressing for correct Windows input validation.

## Acceptance Criteria

- [ ] isValidPath rejects Windows shell metacharacters when on Windows
- [ ] Existing Unix path validation behaviour unchanged on macOS/Linux

## Files

- src/utils/shell.ts

## Work Log

