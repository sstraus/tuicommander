---
id: 071-cc1f
title: Implement PR detection from terminal output
status: complete
priority: P2
created: "2026-02-05T11:41:56.548Z"
updated: "2026-02-05T12:28:19.455Z"
dependencies: []
---

# Implement PR detection from terminal output

## Problem Statement

Users cannot see which branches have associated Pull Requests without leaving TUICommander.


a competitor detects PRs by **monitoring terminal PTY output** for PR URLs - not by calling gh CLI separately.

### Detection Method
Parse PTY output with regex for PR URLs:
- GitHub: github.com/.*/pull/(\d+)
- GitLab: gitlab.com/.*/merge_requests/(\d+)
- Triggered by: gh pr view, gh pr create, git push output, etc.

### Display Locations

**1. Sidebar BranchItem** - Compact green badge:
```
★ ACME-00005/read... [#76] +×
```

**2. StatusBar** - Textual with icon:
```
[...info...] | 🔀 PR #73 | [...]
```
Clickable: opens PR URL in browser

## Acceptance Criteria

- [ ] Monitor PTY output for GitHub PR URLs (github.com/.*/pull/N)
- [ ] Monitor PTY output for GitLab MR URLs (gitlab.com/.*/merge_requests/N)
- [ ] Extract PR/MR number and full URL from matched output
- [ ] Store PR info per branch in repository store
- [ ] Display green badge #XX in Sidebar BranchItem
- [ ] Display 🔀 PR #XX in StatusBar
- [ ] Make StatusBar PR clickable (opens URL in browser)
- [ ] Persist PR detection across terminal sessions

## Files

- src/components/Terminal/Terminal.tsx
- src/components/Sidebar/Sidebar.tsx
- src/components/StatusBar/StatusBar.tsx
- src/stores/repositories.ts

## Work Log

