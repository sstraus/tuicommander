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

## Critic follow-up

The regression now chooses each excerpt by its actual DOM row count, then
asserts that count for every pulse frame. At 390×844 the measured scenarios are
1/2/3/4 rows with heights 22.3984375/44.796875/67.1953125/89.59375 px.
At 1024×1366 the recorded source reaches two rows, so three rows are explicitly
reported as skipped rather than mislabelled. No synthetic longer producer text
was added.

Every continuation's first body character must align with the first character
of Bash, within 0.02 CSS px for layout rounding. Phone GREEN measures Bash at
x=28.84375 and continuations at x=28.8515625 (difference 0.0078125 px).
The enhanced test also fails against the real pre-fix b99b409c7 component loaded
through a Vite transform, with current CSS (the new slot class is unused by the
pre-fix component). Visible-dot continuations were at x=12 while Bash was at
x=29.2421875. A fixture-only forced 3ch hanging indent fails absolute alignment
while remaining stable across all pulses, proving that equality alone cannot
satisfy the assertion. These reports are in `criticFollowup` in the evidence JSON.

### CI coverage limitation

This browser geometry script is an opt-in local proof, **not a normal CI test**.
`.github/workflows/ci.yml:120` invokes Vitest. `vitest.config.ts:38` includes only
`*.test/spec.ts(x)`, and line 49 uses happy-dom, which cannot measure browser
layout. `package.json` has no Playwright, Puppeteer or Vitest browser provider.
The existing `scripts/chat-view-follow-proof.mjs` browser harness is reached from
an explicitly ignored opt-in Rust test (`src-tauri/src/chat_view/follow_http_fixture.rs:10`)
and requires the local browser wrapper. It is not a portable CI browser path.
No new browser framework or CI dependency was introduced. Native grid replay and
the existing targeted mobile unit/component tests remain normal-suite coverage;
they do not establish real browser geometry. Actual Safari PWA checks also remain
separate in to-test.md.
