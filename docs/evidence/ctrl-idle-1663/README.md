# Mobile toolbar enabled styling (1663-8c02)

Headless Chrome for Testing 149 on macOS, 2026-10-10. Captures render the full
worktree mobile frontend through Vite, using an owned mock session and mock
HTTP responses. No real sessions, credentials or terminal writes are involved.

The phone layout is 390×844 with visualViewport.height reduced to 500; the
tablet layout is 1024×1366 with visualViewport.height reduced to 966. The real
MobileApp viewport listener sets --app-height accordingly. This simulates
keyboard-open geometry; it does not verify a native iOS keyboard or real touch.
The blank area below the app is the simulated keyboard's reserved space.

Each device has before/after screenshots with Ctrl idle and its menu open.
Before uses the main branch TerminalKeybar.module.css, mapped to the current
CSS module names with the worktree module stylesheet disabled. After restores
the worktree module stylesheet. The component and remaining styles are identical.

Inspected all eight PNGs: the toolbar stays above the reserved keyboard space,
Ctrl/Tab/Esc match at idle, and the expanded Ctrl has a clear accent border and
text. The menu fits above the toolbar on both layouts. Computed values are in
visual-measurements.json:

- Before: Ctrl, Tab and Esc all use rgb(160,160,160), including expanded Ctrl.
- After idle: all three use rgb(204,204,204), border rgb(62,62,66), opacity 1.
- After expanded: Ctrl uses rgb(89,168,221) text/border and rgb(55,55,61) background.

The original report's Ctrl-only dim styling was not reproduced: all three keys
already shared one enabled class. The fix brightens that shared class and gives
the existing aria-expanded state a distinct appearance.
