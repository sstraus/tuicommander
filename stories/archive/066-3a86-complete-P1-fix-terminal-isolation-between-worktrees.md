---
id: 066-3a86
title: Fix terminal isolation between worktrees
status: complete
priority: P1
created: "2026-02-04T23:34:12.820Z"
updated: "2026-02-07T22:25:08.083Z"
dependencies: []
---

# Fix terminal isolation between worktrees

## Problem Statement

Terminals are shared globally across all worktrees instead of being isolated per branch. When switching between worktrees (e.g., worktree-001, worktree-002, worktree-003), all tabs show the same terminal content. The terminalIds() fallback shows all terminals when a branch has no valid terminals, and terminals are stored globally in terminalsStore while branch.terminals only stores IDs that may become stale.

## Acceptance Criteria

- [ ] Each worktree/branch must have completely isolated terminal sessions
- [ ] Switching between worktrees should show only terminals belonging to that specific branch
- [ ] Terminal content should be preserved when switching between branches and returning
- [ ] Remove fallback in terminalIds() that shows all terminals when branch has none
- [ ] Ensure terminal IDs in branch.terminals always reference valid terminals in terminalsStore

## Files

- src/App.tsx:584-608
- src/App.tsx:267-298
- src/stores/terminals.ts
- src/stores/repositories.ts:186-191
- src/components/TabBar/TabBar.tsx:13-30

## Work Log


### 2026-02-05 - E2E Testing Complete

**Test Results**: Documented in `e2e-test-results.md`

**Critical Bugs Found**:

1. **Terminal content not persisted** (P0)
   - Symptom: Switching between worktrees causes terminal scrollback to disappear
   - Evidence: Executed `ls` in worktree-001, switched to worktree-002, returned to worktree-001 - terminal completely black
   - PTY sessions remain active (3/50) but display is lost

2. **Wrong working directory** (P0)  
   - Symptom: Terminals not starting in worktree directory
   - Evidence: `pwd` shows `/Users/stefano.straus` instead of worktree path
   - Impact: All commands execute in wrong context

3. **Tab naming inconsistency** (P2)
   - Symptom: Terminal tabs show non-sequential numbers per worktree
   - Evidence: worktree-001 shows tabs 1, 3, 4 instead of 1, 2, 3

4. **Fallback logic mixing terminals** (P1)
   - Location: `src/App.tsx:584-608` terminalIds()
   - Shows ALL terminals when branch has none

**Root Cause Analysis**:
- PTY session creation likely missing correct `cwd` parameter
- Terminal component not preserving xterm buffer when switching worktrees
- Check `src/components/Terminal/Terminal.tsx:136-143` for session creation

**Updated Acceptance Criteria**:
- [x] Terminals must start in correct worktree working directory
- [x] Terminal scrollback buffer must persist when switching worktrees  
- [ ] Each worktree must have isolated terminal sessions (architecture issue)
- [ ] Remove terminalIds() fallback showing all terminals
- [ ] Fix tab numbering to show 1, 2, 3... per worktree

