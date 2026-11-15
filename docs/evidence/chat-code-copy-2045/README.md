# Chat code Copy layout — #2045-848c

The existing production session Chat appearance fixture renders the real shared
Transcript and ContentRenderer at 390 × 844. Transcript answer-region tagging
includes fenced `pre` blocks. Its later spacing reset set their padding to zero,
overriding the normal code inset. An absolutely positioned Copy button then
extended below a one-line block and caused vertical overflow.

The fix excludes `pre` from that spacing reset. It restores existing code padding,
not a fixed height, a new overflow clamp or another rendering path.

## Recorded geometry

- Before, all three short-block presentations: block height 18.84375 px;
  button height 17.5 px starting 4 px down; button extends 2.65625 px below the
  block. `scrollHeight=22`, `clientHeight=19`.
- After: block height 34.84375 px, with the complete button inside it;
  `scrollHeight=35`, `clientHeight=35`.
- Long-code boundary: block expands to 129.0625 px;
  `scrollHeight=129`, `clientHeight=129`.
- Document width remains 390 px in every capture.

[Before measurements](before.json) · [Corrected measurements](measurements.json)

## Corrected captures

- [Mobile dark](mobile-dark.png)
- [Mobile light](mobile-light.png)
- [Desktop narrow dark](desktop-narrow-dark.png)
- [Mobile long-code boundary](mobile-long-code.png)

The pre-fix browser regression `short_code_copy_fits` failed on
`cae10e767fc4676ff278e721bf4e1301d3f1817e` for all three short-block presentations.
It asserts observable button containment, label containment and no vertical
code-block overflow. It does not inspect CSS source or mock browser geometry.

Run the focused browser check through the build slot:

```sh
python3 scripts/check-chat-code-copy-layout.py --evidence .tmp/code-copy-check
```

The evidence directory must not exist. The script uses a unique named stealth
browser tab, an isolated worktree Vite on port 15970, and cleans up its own
processes. `--capture-only` collects images and geometry without running the test
assertions. The post-fix operation used that mode as requested for screenshots;
the coordinator owns the final GREEN run and pixel review. No physical-phone,
WebKit, clipboard transport or performance claim follows from this fixture.


## Wrapped-line correction

The padding correction above fixed containment but still allowed the absolute
button to cover a long first line. Coordinator pixel review found that overlap.
The extended regression failed on `2a881325d716e8425ef87f07a8a3ce7ef86d7ead`:
`mobile-long-code: Copy covers code text`. It measures every rendered code line
with a DOM Range, not the bounding box of the whole code element.

Copy now occupies its own normal-flow row above the code. The browser lays out
the actual button height; no guessed top band, width gutter or hover-only reveal
is needed. This leaves the code's full available width and makes the control
visible on touch. The existing code inset remains intact.

Use these final captures, not the intermediate images above:

- [Mobile dark](flow/mobile-dark.png)
- [Mobile light](flow/mobile-light.png)
- [Desktop narrow dark](flow/desktop-narrow-dark.png)
- [Mobile long code](flow/mobile-long-code.png)
- [Desktop long code](flow/desktop-long-code.png)
- [Final line/control geometry](flow/measurements.json)
- [Overlapping pre-fix geometry](flow/before-overlap.json)

The button bottom is 342.7890625 px. The first rendered code line starts at
348.2890625 px on mobile and 347.2890625 px on desktop. No code line intersects
the control. Short code blocks have equal client/scroll heights of 54 px; long
mobile and desktop blocks have equal heights of 134 px. Document width stays
390 px in all five captures. This is native Chromium geometry, not WebKit proof.
The final operation was a capture-only run; the coordinator owns GREEN assertions
and pixel inspection under the managed build policy.
