---
id: 272-af4c
title: "Hardcoded C:\Program Files drive letter in Windows paths"
status: complete
priority: P3
created: "2026-02-20T07:38:38.184Z"
updated: "2026-02-20T22:48:48.998Z"
dependencies: []
---

# Hardcoded C:\Program Files drive letter in Windows paths

## Problem Statement

agent.rs Windows candidate paths hardcode C:\Program Files. Correct approach is env::var("ProgramFiles") to handle non-standard installation drives.

## Acceptance Criteria

- [ ] Windows paths use env::var("ProgramFiles") and env::var("ProgramFiles(x86)") instead of hardcoded C:\
- [ ] Fallback to C:\Program Files if env var absent

## Files

- src-tauri/src/agent.rs

## Work Log

### 2026-02-20T22:48:44.086Z - Verified: already resolved in a previous session. Code reviewed and confirmed fix is in place.

