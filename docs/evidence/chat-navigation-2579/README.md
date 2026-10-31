# Session Chat navigation — 2579-6271

## Cause and history

The live-origin toast path already calls `navigateToTerminal`. The closed-origin
and repo-only fallback only changed `repositoriesStore.activeRepoPath`, leaving
`terminalsStore.activeId` unchanged. `TerminalArea` selects visible terminals by
that id, and each real `Terminal` owns its transcript and Compose send callback.
Thus the paths changed but the previous conversation and input destination did not.
An existing ToastContainer test explicitly expected that unsafe old terminal id.

This fallback never worked safely with Chat. The repo-only fallback was added in
`fc43451e772c565c82ef50860624bd8890d584ae` (2026-09-22). The progress-origin fix
`ecebd39bf234ebd896255871803f025a87e8b4e1` (2026-09-26) fixed live origins but
retained it. Chat's introduction, `2e2e5f8e92` (2026-10-07), already contained
that same fallback and active-id-based TerminalArea rendering. Compose later
made the wrong input destination visible; it did not introduce the stale selection.
These observations come from `git log -S` and `git show` of those revisions.
No original Boss-session capture was available; this proves the closed-origin and
repo-only failure, not that his original source terminal was necessarily closed.

## Fix

Keep `navigateToTerminal` as the single terminal-selection route for live origins
and surviving destination tabs. Prefer the remembered live, non-detached tab in
the destination's active workspace, then its first live tab. If none exists,
leave global workspace mode, preserve the source split layout, clear the old
layout and pane activation, and show the destination's empty terminal area.
No Chat listener or second session-binding state was added. Selection scans only
the destination workspace's terminal ids (the app supports up to 50 live PTYs).

## Real-component evidence

`src/__tests__/components/ToastChatNavigation.test.tsx` mounts the real
ToastContainer, TerminalArea, Terminal, TerminalChatView, transcript renderer and
CodeMirror ComposePanel. Only backend/PTY ports, grid rendering and the unrelated
file-drop hook are substituted. Tests put text into the real CodeMirror editor
and click its real Send button, then assert the session id at the PTY send port.
This is component evidence, not a live desktop or backend-transcript replay.

Cases: live origin shows and sends to B; missing origin selects B rather than A;
empty destination clears A and its split while preserving the source layout;
native terminal notification shows and sends to B; repo-only notice prefers the
remembered live destination tab. Existing tests retain removed-repo, toast-dismiss,
worktree-origin and action-order coverage.

Pre-fix revision: `565290ae19e36647592c8876e220b695095de5f2`.
Initial targeted RED: 1 passed (live origin), 2 failed (closed origin kept A,
empty destination kept A). Log: `~/Gits/.tmp/tuic-2579-red.log`.
The initial harness also exposed native-listener setup noise; the final suite
uses explicit backend adapters and verifies no unexpected error logs or timer leaks.

## Sibling navigation audit

- Sidebar branch/workspace row: RepoSection's `onSelect` calls `onBranchSelect`;
  App passes `gitOps.handleBranchSelect` (App.tsx:791). The branch coordinator
  selects a valid destination terminal (createBranchSelectionCoordinator.ts:347-357),
  restores saved terminals, creates a new terminal, or clears activeId and layout
  for an empty workspace (:419-422). Sidebar click semantics are unchanged.
- Native terminal notification: nativeNotificationNavigation.ts calls
  `navigateToTerminal`; the real-component test verifies display and Send.
  Native progress notifications open Progress, not another repo/session.
- MCP toast and spawned-agent Open terminal actions: useAppInit.ts:672 and :793
  call the same `navigateToTerminal`; closed bindings are not selected.
- MCP file Open: useAppInit.ts:890-900 changes repo only for focused opens,
  then activates the file tab through mdTabs/editorTabs. tabManager.ts:275-277
  deactivates the terminal pane through exclusive activation; background opens
  leave both repo and terminal unchanged. Chat is hidden, not retargeted.

No CSS/layout rules changed. No screenshot or live desktop check is claimed;
this story uses the real-component alternative from its acceptance criteria.
Independent critic and batch integration remain coordinator-owned.
