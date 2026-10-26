# iOS standalone viewport capture for story 2116-0213

## Source

Recorded on 2026-10-23 from the iPhone 17 Pro simulator running iOS 26.4.
The native window is 402 × 874 points (1206 × 2622 screenshot pixels).
Safari installed the local mobile page to the Home Screen. The TUIC icon opened
it in `com.apple.webapp`, with both standalone flags true.

The page used the isolated `agent-test-2116` backend on port 19881. A local relay
on port 19882 forwarded HTTP and WebSocket traffic to that instance's Unix
socket. Read-only injected JavaScript collected browser geometry, computed safe
insets, a hidden `100dvh` probe, and the actual shell rectangle. It did not change
viewport metadata, shell styles, or application state.

The JSON contains selected actual reports from the pre-fix frontend at commit
`67dd907c24f033024052030463cfc48de2d34593`. It includes Safari, standalone with
the keyboard closed, and standalone with the keyboard open.

## Observations and limits

- Safari: visual viewport 714 points; safe insets zero.
- Standalone closed: visual viewport and `100dvh` 812 points; safe top 62 and
  bottom 34 points. The shell ends at 812.
- Standalone keyboard: visual viewport 471 points, offset zero, scale one;
  `innerHeight` remains 812.
- The native WebView frame is 874 points, but this does **not** prove its full
  height is drawable or interactive. An experimental shell expansion to 874
  moved the textarea to y831..869, where XCUITest could not tap it. Pixel analysis
  of both screenshots still found the background boundary at y812. That
  experiment refutes using `screen.height` alone as the shell height contract.

## Controlled status-bar probe

A fresh Home Screen install named `2116Default` used the same original embedded
frontend. The relay changed only the HTML status-bar meta content from
`black-translucent` to `default`, plus the same read-only telemetry injection.
It did not serve the rejected shell-expansion build. This is a controlled HTML
probe equivalent to the final one-line production change, not a rebuilt native
backend. The earlier Vite build only supported the rejected experiment.

`ios-standalone-default-2116.json` contains actual reports from that install:

| State | DOM viewport / shell | Safe top | Native textarea |
|---|---|---|---|
| Closed | 812pt | 0pt | y831..869 |
| Keyboard | 409pt | 0pt | y428..466 |
| Reclosed | 812pt | 0pt | y831..869 |

The non-overlay status bar changes the native content origin: native textarea
y is DOM textarea y plus 62pt. Thus the 812pt viewport now ends at screen y874,
not y812. No screen-height correction or keyboard heuristic is needed.
XCUITest confirmed the composer is hittable, opened the keyboard, typed a draft,
returned to Sessions to dismiss it, and reopened the session with a hittable
composer. It sent no terminal command. The final probe passed in 11.874 seconds.

Pixel analysis at screenshot x20:

- Original: composer RGB38 ends at row2436 (812pt), then RGB30 through row2621.
- Default: composer RGB38 continues through row2621; the 186-device-pixel band
  is absent. The screenshot is 1206 × 2622 pixels in both cases.

Evidence paths relative to `~/Gits/.tmp/results/2116-simulator`:

- Before: `loaded-before-attachments/9719CBBC-01B7-4A22-B83F-8230709A68E7.png`.
- After closed: `final-attachments/A4029936-8F8C-42CE-A9E7-55E229ACFB48.png`.
- After keyboard: `final-attachments/CAAF3303-F5E2-47AB-B8C1-F7841DBD1CBE.png`.
- After reclosed: `final-attachments/AE3D98E0-F2B6-4B28-8B4C-A63C6D8300DE.png`.
- Native results: `default.xcresult`, `final.xcresult`, `xcui-final.log`.
- Pixel measurements: `pixels.log`, `default-pixels.log`.
- Final HTML regression RED: `metadata-red.log` on the original commit:
  expected `default`, received `black-translucent`.

The shipping regression checks the HTML consumed by iOS at install time, not a
mock WebKit renderer. Existing keyboard and composer-padding tests remain
unchanged. No GREEN Vitest suite ran inside this story; integration owns that
run. CLI bottom padding remains 4px. Physical Home Indicator ergonomics remain
unverified and must not be inferred from the simulator tap result.

Raw reports, XCUITest logs/results, exported screenshots, and the collector are
retained under `~/Gits/.tmp/results/2116-simulator` and
`~/Gits/.tmp/2116-xcui`. The JSON records browser input, not a complete native
rendering fixture. Physical iPhone confirmation remains necessary.
