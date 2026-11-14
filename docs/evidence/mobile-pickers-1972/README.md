# Mobile picker navigation: 1972-128e

## Source

Captured on 2026-10-17 from real Claude Code 2.1.286 in a throwaway PTY of
this worktree's native macOS headless build, baseline `b09cd576f8`.
The backend used its own HOME under `~/Gits/.tmp/mp-home`, port 19871, and
no agent configuration writes. The mobile client used worktree Vite on 19872,
with an 820 by 1180 viewport. No production TUIC session was used.
Claude was asked to call AskUserQuestion with Amber, Blue, and Cyan options.
Hooks and upstream MCP servers were disabled for the throwaway Claude process.

## Captured evidence

- `session-state.json`: the actual `/sessions` response. Claude is awaiting input
  without a `choice_prompt`, which selects the atomic free-text reply path.
- `claude-question.tcap`: backend TUICCAP2 capture,
  SHA-256 `92edce555436bad67b85a643bc96bbd7c448396143fb381f5b0bf2087d562b92`.
  Its geometry header is zero: the first recorded HTTP input created the file
  before an output record supplied geometry. The real PTY and captured grid
  frame are 41 by 110; a faithful replay must supply that observed geometry.
- `question-before.grid`: unmodified first binary grid WebSocket frame,
  SHA-256 `3d10873bb8f4936253a0beca67e86acbcc0ef57a442a84dc172625e40d77a566`.
- `screen-before.json`: terminal text snapshot while Amber is highlighted.
- `keybar-writes-before.json`: intercepted browser fetch writes after Up and Down
  button handlers run with the pre-fix atomic guard: none.
- `keybar-writes-after.json`: the fixed Down handler sends exactly CSI B.
- `screen-after.json`: Claude highlights Blue after that Down key.
- `shell-history-up.json` and `shell-history-down.json`: a separate throwaway
  bash PTY recalls `printf SHELL_HISTORY_1972` with the mobile Up button and
  clears that recalled command with Down. The submitted command remains in
  scrollback; the current prompt becomes empty again.

The failing layer is the mobile composer request boundary: `atomicReply()`
returned before `write_pty`. It is not an application cursor mode mismatch.
The fix keeps navigation in the existing write path without claiming ownership
of a completion or history response in atomic-answer mode.

The macOS screen was locked during this check. Trusted CDP pointer actions did
not reliably dispatch clicks, so the before/after comparison also invoked the
real DOM buttons with `button.click()`. No synthetic picker or mocked backend
was used. Screenshot capture was unavailable in the locked browser.

## Regression

`SessionPickerKeys.test.tsx` consumes the captured session state and composes
the production session screen, key bar, and composer. Only transport, output
presentation, and voice availability are replaced. The pre-fix navigation
assertions fail because no write reaches the transport boundary. Draft retention,
Tab, and ended-session handling are checked alongside the two arrows. Existing
`CommandInput.history-regression.test.tsx` covers composer-owned shell history.
