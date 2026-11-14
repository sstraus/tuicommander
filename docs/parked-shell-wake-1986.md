# Isolated parked-shell browser check — 1986-1236

Date: 2026-10-20. Initial source: `chore/live-1986`, runtime commit `3c2cdfd007` (empty branch-start commit on main `09b2ad81ff`). No production code changed.

**Current disposition:** the final native desktop run below proves raw-input wake after actual standby, with a **44 ms synthetic terminal keydown-to-canvas-echo** sample. Earlier observer blockers are retained as history and explicitly corrected below. The combined visual AC4 remains unchecked: this is not a physical keyboard/display measurement, an authenticated AI-agent exit, or Boss's phone check.

## Scope and isolation

The native macOS headless backend was built in this worktree through build-slot and mbx:

```sh
cd "<worktree>"
export PATH="<mbx-shim-directory>:$PATH"
build-slot.sh cargo build --manifest-path src-tauri/Cargo.toml --bin tuic-remote --no-default-features
```

The confirmed worktree build passed in 50.71 seconds with 614 cache hits. Its dependency receipt names this worktree. `mbx doctor` reported zero failures and warnings. An earlier background invocation resolved to another worktree; its binary was not used.

Backend port: 21986. Worktree Vite port: 21987, with `--strictPort`. The backend used `--instance live-1986 --bind 127.0.0.1 --no-agent-configs`, an isolated HOME, and temporary paths beneath `~/Gits/.tmp/results/live-1986-run/`. Its config directory was beneath that HOME, not Boss's application config directory. The temporary Vite proxy attached this instance's test token to API requests. The first page lacked API authentication and stalled at browser Basic Auth; the authenticated proxy and a fresh owned tab resolved that harness issue.

All browser interaction used the mandated `ab-stealth.sh` wrapper, with named session `live-1986-r2`. No desktop TUICommander instance was launched, restarted or captured. No installed binary was replaced. No request targeted port 9876.

## Real standby and wake observations

Two owned interactive `/bin/bash -l` PTYs ran a foreground `/bin/sh` child which printed `FOREGROUND_CHILD_EXITED` and exited. Setup used the production `sendCommand` helper. This is a real foreground-child exit, **not** a paid agent conversation or independently verified agent-exit transition.

| Lane | Session | Shell PID / PGID |
| --- | --- | --- |
| Desktop browser | `090acc15-84c3-47bf-8195-404eabbabea0` | 78330 |
| Mobile browser | `b47effe3-9e3d-4e74-96b3-308d29369310` | 78348 |

The isolated standby setting was one minute. The browser navigated to `about:blank` and both sessions were marked hidden. No signal was sent manually to manufacture standby.

- **Desktop cycle:** backend logs record both groups entering standby at 05:57:54 UTC. `park-confirmed.json`, sampled at 05:58:09 UTC, records both as `Ts+`. Opening the desktop page at 1400×900 produced backend `SIGCONT` logs at 05:58:11 UTC for the same groups. Raw browser terminal text plus trusted Enter executed `printf 'DESK_%s\n' OK1986`, yielding `DESK_OK1986` without a resend.
- **Separate mobile cycle:** the browser left the page again. `mobile-park-observation.json` records both groups as `Ts+`; the backend records standby at 06:02:54 UTC. Opening only `mobile.html` at 390×844 produced `SIGCONT` at 06:03:02 UTC. Opening the mobile shell and submitting `printf 'MOB2_%s\n' OK1986` through its composer yielded `MOB2_OK1986` without a resend. No desktop page was open during this wake cycle.

The first observer had a 125-second outer bound and ended before standby occurred. It is **not** a passing observation. A later direct OS sample and the backend logs establish the first cycle; the separate mobile observer completed successfully at 127.205 seconds. Watch-release and recent-observation grace can outlast the configured idle interval, so the one-minute setting is not an exact one-minute stop deadline.

## Input latency measurements

These are single local samples after attachment had resumed the shell, not benchmarks or wake-inclusive latency bounds.

| Surface | Start | End | Measured interval |
| --- | --- | --- | --- |
| Desktop browser, 1400×900 | Captured trusted raw Enter `keydown` | Browser fetch observes the new execution result `DESK2_OK1986` in backend terminal output | **21.3 ms** |
| Mobile browser, 390×844, separate standby cycle | Captured trusted Send click | MutationObserver observes `MOB2_OK1986` in the rendered terminal DOM | **370.2 ms** |

The desktop observer used `performance.now()` and an 8 ms retry interval. Its end includes HTTP observation overhead; it does not measure canvas paint. The mobile measurement includes command submission and terminal DOM rendering, but does not measure display presentation or a physical keyboard. The command source does not contain the contiguous result marker, so these markers prove shell execution rather than input echo alone.

Retained `desktop-enter-latency.json` and `mobile-second-latency.json` contain timestamps, trusted-event evidence and output. The earlier `desktop-latency.json` and `desktop-latency-key.json` are invalid timing attempts: the first did not observe text key events, and the second compared against an old baseline. Do not cite their apparent 7.7 ms value. An early desktop retry clicked before tab activation settled and typed into the other owned shell; its expired timing attempt is not evidence. The final desktop measurement selected and confirmed the correct terminal before submission.

## Evidence and acceptance disposition

Evidence root: `~/Gits/.tmp/results/live-1986-run/`.

- Build: `build-own-cwd.log`, its exit receipt, and the binary dependency receipt.
- Standby: `park-confirmed.json`, `mobile-park-observation.json`, `backend.log`.
- Input/output: `desktop-final-output.json`, `mobile-final-output.json`, `browser-transcript.txt`.
- Timing: `desktop-enter-latency.json`, `mobile-second-latency.json`.
- Screenshots: `desktop-after-input.png`, `desktop-final.png`, `mobile-second-after-input.png`.
- Cleanup: `cleanup.json`, `sessions-after-cleanup.json`, `process-cleanup.json`, `browser-cleanup.txt`, `cleanup-ps.txt`.

**AC4 remains unchecked.** This run proves isolated desktop-browser and emulated-mobile recovery after real standby. Both browser surfaces use HTTP, and attachment/visibility resumed the shell before input. It does not directly execute the changed native desktop `write_pty_parts_blocking` boundary, nor does it prove raw input itself woke an already parked shell. The existing real-PTY regression tests remain the evidence for that boundary.

Boss still needs the native desktop check after his chosen backend restart, including actual agent exit, and a check on his physical phone. The emulated mobile result is not silently substituted for either requirement. No story closure is justified by this run.

## Native agent-test attempt — desktop peer

A separate run on 2026-10-20 used runtime commit `4c4267606bfc608fa3faff8a7373ca31e283496c`, the authorized `make agent-test AGENT_TEST_INSTANCE=at-1986 AGENT_TEST_PORT=19877` target, and its worktree-local executable. The launcher built the embedded frontend and native binaries through build-slot and mbx. No installed binary or daily instance was changed. Evidence is under `~/Gits/.tmp/results/desk-1986-run/`.

The TCP listener requires authentication. The test used the launcher's own Unix HTTP socket without reading any token store. Debug evaluation ran inside the native `tauri://localhost` WebView, not a browser page. One owned interactive bash PTY (`34880664-14c4-48f8-9508-3f33a13e8352`, PID/PGID `23368`, parent agent-test PID `16463`) ran a foreground `/bin/sh` child to exit. This was not an authenticated AI agent exit.

To isolate raw-input wake from attachment wake, the harness marked this session desktop-hidden through native `set_session_visible(false)` while retaining the terminal input element. With the standby interval set to one minute, the production checker recorded SIGSTOP at **07:37:14.672787 UTC**. An OS sample at **07:37:15.145863 UTC** confirmed the exact shell group in `Ts+` state. The harness then dispatched one synthetic `Q` keydown to the terminal input at **07:37:15.346 UTC**. The native backend recorded SIGCONT for PGID `23368` at **07:37:15.349566 UTC**. The next OS sample found the shell runnable, and the output ring contained exactly one new `Q`. No visibility change, input resend or HTTP output read occurred between the stopped-state sample and this key event. The output read followed the observation window. This supports native raw-input recovery from actual standby; it is not a physical-keyboard or natural tab-focus scenario.

**No valid keyboard-to-rendered-echo latency was obtained.** The WebView reported `document.visibilityState === "hidden"` and no focus, and all terminal canvas rectangles had zero area. Its native window reported visible. The synthetic event had `isTrusted:false`. A runtime IPC tracing attempt could not replace the non-writable, non-configurable `__TAURI_INTERNALS__.invoke` property, so it provided no IPC timing. The five-second canvas observer recorded no paint. Blank canvas exports were discarded rather than retained as screenshots or visual evidence. The roughly four-millisecond key-dispatch-to-SIGCONT timestamp difference is not an echo latency and must not be used as one.

The attempt to show the owned test window through its WebView API failed with the exact permission error:

```text
window.show not allowed. Permissions associated with this command: core:window:allow-show
```

No alternative native input or window-control transport was used to bypass this refusal. A follow-up needs an approved way to present and focus only this test window, and trusted input confined to that window, before it can claim keyboard-to-visible-echo latency. AC4 stays unchecked, including its physical-phone portion. This run does not replace Boss's manual post-restart check.

Retained evidence: `agent-test.log`, `arm-result.json`, `park-observation.json`, `native-key-latency.json` (an incomplete timing attempt, not a latency result), `after-key-ps.txt`, `after-key-output.json`, `window-state.json`, and cleanup receipts. The owned PTY was deleted and the final session list was empty. `make agent-test-stop` requested shutdown through this instance's supervisor FIFO. No Vite server or browser session was started for this attempt.

## Visible-window follow-up after 2090

The desktop peer merged main at the coordinator's request and rebuilt runtime `5eba9d81bcd65c570117de5cb1a5413a80f94957`. This supersedes the earlier hidden-window prerequisite failure: the first follow-up observed `document.visibilityState === "visible"`, one terminal input, and nonzero canvas rectangles (302×320 and 302×280 CSS pixels) in the compact agent-test window. The observer did not request focus, send OS keystrokes, or inspect another window's pixels.

The requested latency measurement is still incomplete:

- `~/Gits/.tmp/results/desk-1986-visible-run/`: a native New Tab shell ran the foreground-child exit command and returned `CHILD_EXITED1986`. After native hidden visibility, a bounded 240-second OS observer did not see the owned shell enter `Ts+`. No parked-input latency sample was taken. This is a failed setup observation, not proof that standby or raw-input wake is defective.
- `~/Gits/.tmp/results/desk-1986-visible-r2/`: an HTTP-created owned shell was idle in backend logs, but evaluation of a simple WebView canvas-state query returned no `eval_js` result within the native harness's 30-second budget. The script stopped before parking or measuring input.
- `~/Gits/.tmp/results/desk-1986-visible-r3/`: a native New Tab shell again executed the foreground child to completion. The first terminal-store snapshot reported `shellState:null`; a later `return window.__TUIC__.terminals()` evaluation produced no result within 30 seconds. The observer stopped before parking or measuring input. This narrows the blocker to missing native evaluation completion during setup, not to a measured input-to-echo delay. It does not identify whether WebView execution or result delivery stalled.

Each attempt started `make agent-test` as an owned subprocess, polled its own socket for readiness, and stopped it in the observer's `finally` block. Each `cleanup.json` records successful `make agent-test-stop`, an empty final session list, and no remaining executable from this worktree. The supervisor's make exit 2 reflects the requested child SIGTERM (143), not a build failure. No Vite server, browser daemon, production session, installed binary or daily API was used. There is still no valid native keyboard-to-rendered-echo latency, and AC4 remains unchecked. The physical-phone check remains with Boss.

## Observer correction and integrated-shell follow-up

The coordinator's source audit explained two invalid prerequisites in the preceding attempts. `app_logger.rs` coalesces consecutive identical messages while retaining their log ID; the old `evaluate` helper waited only for a new ID. Those apparent timeouts do not establish a WebView stall. `shell_integration.rs` exports the integration path for interactive login bash but does not source it. The owned native bash therefore needed `source "$TUIC_SHELL_INTEGRATION"` before the child command to establish backend `idle` through OSC 133. The earlier `shellState:null` observation was not a valid standby prerequisite.

`scripts/agent-test-visible.py` now wraps each evaluation in a unique nonce and matches that nonce in the result log. It ignores unrelated results and reports a correlated JavaScript error directly. The native r4/r5 observers each verified two consecutive identical snapshots and an expected thrown error against the real WebView, without mocks. Both then sourced the integration script in the owned bash, observed backend `idle`, and reached production SIGSTOP without reading session output during the standby wait. Thus the earlier evaluation and idle-prerequisite blockers are resolved.

Evidence roots are `~/Gits/.tmp/results/desk-1986-visible-r4/` and `desk-1986-visible-r5/`. The r4 shell (PGID `15037`) stopped at 10:53:11.813269 UTC and resumed at 10:53:12.256102 UTC after one synthetic raw `Ω` key. The output ring contains that character. However, its document had become hidden, so the ten-second observer saw no canvas paint and produced no valid rendered-echo latency. Its initial frontmost application was `loginwindow`; this is not evidence from an unlocked visible surface.

In r5 the initial and pre-key frontmost application was Slack, the setup document was visible, and the shell (PID/PGID `61198`) reached OS-confirmed `Ts+`. Before any key was sent, the owned native window reported `isVisible() === false` and the document reported hidden. The observer correctly aborted rather than cite a hidden-window latency. The cause of that presentation loss is not established. A question and the exact evidence were sent to the coordinator; no denied show/focus API was bypassed.

Both observers deleted their owned session and ran `make agent-test-stop` in `finally`; both cleanup receipts record an empty session list and no remaining worktree executable. There is still no valid native keydown-to-painted-echo latency. AC4 and the physical-phone check remain open.

## Cleanup

Both owned PTYs were deleted; the final session list was empty. The owned browser tab and named daemon were closed; shared Chrome was left running. Backend PID 65820 and Vite PID 65821 received SIGTERM, and the final `ps` check found neither process nor either owned shell. The temporary Vite configuration was removed. No capture corpus, dependencies, production code or application test suites changed or ran.

## Retained r6 native evidence (consolidated 2026-10-24)

The receipts in `reports/tuicommander/1986-native-observer/r6/` record the final visible synthetic-input sample. Attempt 1 is explicitly excluded in `attempts.json`: PlatformSSO interference was swallowed by the earlier readiness retry. Attempt 2 is the retained sample. The observer now records frontmost observations and aborts on PlatformSSO or a different agent-test instance, including during readiness and standby observation.

`owned.json` identifies app PID 43600 and shell PID/PGID 44470. `native-key-latency.json` records a visible `tauri://localhost` document, an untrusted synthetic Omega keydown at performance time 90263, and canvas echo at 90307: **44 ms for this single synthetic sample**, not physical keyboard/display latency. `execution-output.json` retains `NATIVE_EXECUTED1986`. `cleanup.json` records no remaining sessions or worktree processes and a successful scoped stop request. The PNG is retained evidence, not a new visual inspection during consolidation.

The earlier statements that no native echo timing was available describe r4/r5 only and are superseded by this r6 receipt. No new desktop launch or timing measurement was made during consolidation. The combined manual-restart desktop/physical-phone acceptance criterion remains open.
