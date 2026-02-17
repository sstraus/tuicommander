---
id: "052-9fb7"
title: "Enhanced GitOperationsPanel with live branch list"
status: complete
priority: P2
created: 2026-02-04T17:06:31.284Z
updated: 2026-02-04T17:06:31.284Z
dependencies: []
---

# Enhanced GitOperationsPanel with live branch list

## Problem Statement

GitOperationsPanel hardcodes branch list (main, develop, feature/current). Users cannot see actual branches from their repo. Need dynamic branch fetching via Tauri command.

## Acceptance Criteria

- [ ] Add get_git_branches Tauri command that runs git branch -a
- [ ] Parse output to extract local and remote branches
- [ ] Populate branch dropdown with real data
- [ ] Show current branch highlighted with checkmark
- [ ] Add refresh button to reload branch list
- [ ] Handle repos with no branches gracefully

## Files

- src-tauri/src/lib.rs
- src/components/GitOperationsPanel/GitOperationsPanel.tsx:132-147
- src/hooks/useRepository.ts

## Work Log

