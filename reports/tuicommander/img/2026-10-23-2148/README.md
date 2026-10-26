# Mobile repository contrast evidence

## Change

The real `SessionCard` repository label now uses `--fg-primary`. The title retains its 14px semibold type; the repository stays 12px regular. Intent and time retain `--fg-muted`.

## Computed colors

Captured through Chrome DevTools on an isolated worktree Vite page at `http://127.0.0.1:19893/2148-card-check.html`. The page mounted the real component and stylesheet, used a synthetic display-only session, and mapped the bundled Commander appChrome palette to its CSS tokens. It did not contact a backend. See `before-colors.json` and `after-colors.json` for browser-computed values.

WCAG relative luminance uses sRGB linearization and `(Llighter + 0.05) / (Ldarker + 0.05)`.

| Commander label | Foreground | Card background | Ratio |
| --- | --- | --- | --- |
| Repository before | `#737373` | `#262626` | 3.1916:1 |
| Repository after | `#d4d4d4` | `#262626` | 10.2097:1 |
| Intent/time, unchanged | `#737373` | `#262626` | 3.1916:1 |

Additional source-token checks: Commander foreground on the pressed `#3a3a3a` surface is 7.6738:1. Paper foreground on its normal card is 11.3865:1. Mobile only offers Commander and Paper (`mobileTheme.ts`). The regression test covers both and the pre-theme-load defaults.

## Screenshot blocker

No screenshots were captured. Bounded headless Chrome attempts on versions 149.0.7827.54 and 149.0.7827.22 stalled at screenshot capture. Direct CDP could read the mounted card and computed styles, but `Page.captureScreenshot` also stalled. `ioreg -n Root -d1 -a` reported `CGSSessionScreenIsLocked = true`. The existing browser wrapper documents that a locked macOS screen stops screenshots and requestAnimationFrame-driven UI. The required before/after visual criterion remains open until the Mac is unlocked. No full-screen capture was attempted, and no daily TUICommander instance or config was accessed.
