---
id: 381-26f1
title: Document untracked features in FEATURES.md - tab pinning, branch sorting, kitty, PTY pause
status: complete
priority: P3
created: "2026-02-25T07:45:53.874Z"
updated: "2026-02-25T08:30:30.754Z"
dependencies: []
---

# Document untracked features in FEATURES.md - tab pinning, branch sorting, kitty, PTY pause

## Problem Statement

Session 0b1dd1e3 scan found features implemented but not in docs/FEATURES.md: tab pinning, branch sorting (main first), kitty keyboard protocol, PTY pause/resume, MCP registration with Claude CLI.

## Acceptance Criteria

- [ ] Tab pinning documented (pinnable diff/md/editor tabs)
- [ ] Branch sorting documented in Section 7 (main first, merged PRs last)
- [ ] Kitty keyboard protocol documented
- [ ] PTY pause/resume commands documented
- [ ] MCP registration with Claude CLI documented
- [ ] Docs only, no code changes, CHANGELOG.md entry

## Files

- docs/FEATURES.md
- CHANGELOG.md

## Work Log

### 2026-02-25T08:30:30.683Z - Documented: tab pinning (1.2), PTY pause/resume (1.1), Kitty keyboard protocol (new 1.10), branch sorting (2.3), MCP registration (14.6). Updated CHANGELOG with Documentation section.

