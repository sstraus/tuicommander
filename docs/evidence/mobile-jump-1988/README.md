# Mobile jump-button layout, 1988-0233

Captured on 2026-10-17 with local headless Chrome for Testing 149.0.7827.54.
The Vite fixture mounts the real TerminalKeybar and CommandInput components,
with an inert session identity and no backend. This proves component geometry,
not a real phone, a PTY interaction, or backend scroll behavior.

`360.png`, `390.png`, and `844.png` show the fixed layout at 360x800,
390x844 and 844x390. The existing horizontal key bar is scrolled to its right
edge in the images so all arrow keys are visible. Measurements are taken
before that horizontal scroll. The jump control is outside the key bar in
both states. At 360px, its rectangle is x314-358/y754-798; Send is
x276-312/y758-794, so both centers are y776. The key bar ends at y747.
The other widths have the same separate rows and aligned control centers.

RED command on pre-fix commit `90c84d370`:

```sh
node scripts/check-mobile-jump-layout.mjs
```

Run through the machine build gate, it failed with
`360: jump overlaps a key-bar button`. The old jump rectangle was
x320-360/y703-747, intersecting the left-arrow rectangle
x324.28-368.28/y703-747.

After the fix, the script was run with `--capture-only` to take the required
screenshots and record rectangles. The GREEN assertion run and the updated
MobileJumpBottom Vitest suite belong to batch integration. The script closes
its own browser and Vite server in both success and failure paths.
