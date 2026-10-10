# Claude mobile dot pulse, story 1654-a3d5

Captured on 2026-10-10 from Claude Code 2.1.286 in an owned TUIC PTY,
160 columns by 40 rows. The session was closed after capture. `data` contains
session output `format=raw`; `toolLine` is its exact text snapshot tool header.
The OFF frame replaces column one's U+23FA with U+0020; the existing mobile
normalizer adds VS15 to the visible dot. Green is RGB (78, 186, 101).

Raw byte count: 53021. SHA256: `7e3871a6969b381a377d2015c699743e6c42a51174516580b9c51448291120f8`.
The separate terminal-grid fixture and replay remain passing: the producer and
native grid both give dot/space one cell. This defect belongs to mobile DOM layout.

Before the fix, OutputView counted ON's leading dot as zero indentation and
OFF's two leading spaces as 2ch. CSS therefore changed continuation position and
available wrap width every pulse. Font fallback also gave the text-form dot a
slightly different advance from a space. A one-cell inline slot for either frame,
plus the same hanging indent, keeps the body stationary. The slot resets inherited
text-indent so the dot stays visible.

## Browser regression

Serve from this checkout:

```sh
pnpm exec vite --config src/mobile/__tests__/browser/dotPulse.vite.config.ts
```

Open `http://127.0.0.1:14352/src/mobile/__tests__/browser/dotPulse.html` using
the mandated browser wrapper. Run `agent-browser eval --stdin` with
`src/mobile/__tests__/browser/dotPulseLayout.js`. This renders the real OutputView
with the recorded header through a fixture-only PTY subscription seam. The OFF
state applies the captured one-space replacement, rather than inventing producer
output. Shorter scenarios are excerpts of that same captured header.

The regression compares every non-space body character's DOM Range x/y,
block height, and the following block's y across grey ON, blank OFF and green.
RED on b99b409c7 failed body geometry in all four excerpts at 390×844.
GREEN passes unwrapped, two-row, three-row and full four-row excerpts there;
at 1024×1366 it passes one/two-row excerpts (the recorded header fits two rows).
Exact reports are in the adjacent evidence JSON. The following-block assertion
was added after RED; the original body-coordinate assertion supplies RED evidence.

Both runs used Chromium with the mac-safari profile/device emulation, and
navigator.webdriver was undefined. Safari UA strings do not make this WebKit:
maxTouchPoints was zero. Actual iPhone/iPad Safari PWA font and touch checks are
tracked in to-test.md; browser evidence does not claim them.
