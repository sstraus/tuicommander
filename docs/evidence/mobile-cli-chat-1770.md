# Mobile Claude CLI/Chat audit — 2026-10-17

Story: #1770-3e28 (raised to P1 in the main story store).

## Existing implementation

The requested feature is already on main. Do not add a second switch or transcript renderer.

- `src/mobile/screens/SessionDetailScreen.tsx:269` mounts the CLI/Chat switch in the session header.
- `src/mobile/components/useSessionChatView.tsx:22` enables it for an existing Claude session. The backend owns transcript availability; a frontend agent-session ID is not required.
- `src/mobile/components/useSessionChatView.tsx:57` mounts the shared `TerminalChatTranscript` with the mobile host hint and existing wake subscription.
- `src/mobile/useSessions.ts:195` handles `chat-view-changed` in the existing session event subscription.
- `src/mobile/screens/SessionDetailScreen.tsx:488` keeps terminal output mounted and hides it while Chat is active.
- `src/mobile/mobile.css:1` and `src/global.css:1` import the same Chat presentation tokens. This is the landed #1974-e6ae change (merge `546ee325f9`).

This lane synchronizes the existing implementation from main. It changes no application behavior or test assertion.

## Validation and limits

- Instruction-link validation passed: two instruction pairs.
- Changed-file formatting check against `main...HEAD` passed with no files listed.
- No RED or GREEN tests ran in this audit. No production defect or new feature implementation required a test. The batch integrator owns GREEN validation under the managed build policy.
- Attempted the existing rendering-only fixture at `/src/__tests__/browser/sessionChatAppearance.html?mobile`, dark and light, with a 390×844 headless Chrome window and an 8-second virtual-time budget. Both attempts timed out at the 18-second outer bound. No screenshot was produced. Logs: `~/Gits/.tmp/tuic-1770-visual/`.
- The fixture uses synthetic transcript entries. It does not prove live-agent transport behavior or phone performance.
- Process-group cleanup returned `PermissionError: [Errno 1] Operation not permitted`. A subsequent read-only process listing found no matching Vite or fixture Chrome processes. No alternate termination mechanism was used.
- Story-close preflight refused the unmeasured phone first-open/resume criterion and historical test-change metadata. The new 390×844 visual criterion also remains unchecked. Do not close the story based on this audit.

## Remaining evidence

Capture and inspect the mobile session Chat at 390×844. Measure first open and background/resume with 2,000 entries on a real phone, including device, browser, network and elapsed times. Prior synthetic Chromium timings are not phone measurements.


## Fresh browser evidence — 2026-10-18

The night-mobile audit captured the existing presentation through the mandated
`ab-stealth.sh` wrapper, in its own named tab against this worktree's Vite on
`127.0.0.1:15970`. The viewport was 390 × 844. No desktop instance or production
configuration was changed.

- [Mobile, Commander](mobile-cli-chat-1770/mobile-dark.png)
- [Mobile, light](mobile-cli-chat-1770/mobile-light.png)
- [Desktop presentation at phone width](mobile-cli-chat-1770/desktop-narrow-dark.png)
- [Browser command transcript](mobile-cli-chat-1770/browser-transcript.txt)

The production appearance fixture mounts the shared Transcript, real CLI/Chat
switch and real composer. DOM evidence confirms the header, both mode buttons,
user prompt, folded tool activity, answer, quote, list and code block were
present. Both measured presentations had viewport width 390 and document scroll
width 390: no document-level horizontal overflow. These are synthetic appearance
entries, not live-agent transport evidence or a phone benchmark. The screenshot
files were captured and their PNG dimensions verified; this agent's available
file reader returns raw image bytes, so pixel-level visual inspection still
needs an image-capable reviewer before the visual criterion is checked.

The first navigation timed out while Vite scanned dependencies. The bounded
retry limited dependency scanning to this fixture and waited for its header;
it exited successfully and captured all three images. The named tab, its
agent-browser daemon and the owned Vite process were stopped. Detailed failed
and successful harness logs remain under `~/Gits/.tmp/results/nw-mobile-visual*`.

The 2,000-entry first-open/resume phone measurement remains open. No application
code or tests changed, and no extra application suite was run for this audit.


## Code Copy correction — 2026-10-19

Coordinator pixel inspection of the first captures found the code Copy control
clipped by a one-line answer block. Story #2045-848c records the browser RED,
CSS cause and correction. Use the [corrected captures](chat-code-copy-2045/README.md)
for the final layout review; the earlier images above retain the defect evidence.

## Current source and capture audit — 2026-10-19

The feature remains implemented at audit base `fe226587cf`. No new application
change or regression test is needed. Current source locations supersede the older
line numbers above:

- `useSessionChatView.tsx:24` allows an existing Claude session without a frontend
  discovered session ID. Later work also enables ego; this audit does not change it.
- `useSessionChatView.tsx:60-70` mounts `TerminalChatTranscript` and injects the wake.
- `SessionDetailScreen.tsx:333` mounts the switch; lines 539-541 mount Chat and
  keep OutputView mounted with `visible={!chat.active()}`.
- `useSessions.ts:195-198` dispatches Chat wakes from the existing SSE subscription.
- `TerminalChatTranscript.tsx:26-53` gates refreshes while hidden and releases its
  watcher on cleanup. `Transcript.tsx:565-568` catches up on touch release only
  while following; lines 739-740 use that handler for touch end and cancellation.

A fresh capture-only run of `scripts/check-chat-code-copy-layout.py` through the
build slot exited 0. It used one named stealth-browser session against this
worktree's Vite, captured five 390 × 844 presentations, then closed its tab,
browser daemon and Vite process. Evidence is outside the repository at
`/Users/stefano.straus/Gits/.tmp/results/1770-3e28/current/`: mobile dark, mobile
light, desktop narrow dark, mobile long code and desktop long code PNGs,
`measurements.json`, `transcript.txt` and `vite.log`.

Every measured document width is 390 px. Short code blocks have equal client
and scroll heights of 54 px; long blocks have equal heights of 134 px. The Copy
button bottom is 342.7890625 px; first code lines begin at 348.2890625 px on mobile
and 347.2890625 px on desktop. These are Chromium DOM measurements, not pixel
inspection, physical-phone performance or live-session transport evidence.

No RED/GREEN, Vitest, TypeScript or Rust run was made: only documentation changed,
and existing implementation tests were inspected rather than rerun. Historical
test-change explanations were added to the main story store after checking the
original `6dae12fbe7` diff. The real-phone 2,000-entry timing criterion remains
open; an image-capable reviewer must inspect the new captures before checking
the visual criterion.

## Standalone iOS simulator measurement — 2026-10-23

A new owned iPhone 13 simulator ran the full standalone PWA against the isolated
`agent-test-1770` instance with a synthetic 2,000-entry Claude transcript. Five
samples per transition recorded first-open p50/p95 13,351/14,257 ms, retained
foreground resume 57/59 ms, and CLI-to-Chat remount 13,514/21,848 ms. These are
post-layout animation-frame opportunities, not compositor first-paint or
physical-phone timings. The [report, raw events, summary and app-only captures](mobile-cli-chat-1770-simulator/README.md)
record the exact clock, fixture, setup failures and limits. The owned PTY,
proxy, application instance and simulator were cleaned up. The main story
worklog has the simulator results; the phone criterion remains unchecked.
