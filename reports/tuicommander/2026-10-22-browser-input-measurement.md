# Browser terminal input measurement — 2026-10-22

Story: `2040-16b2`. Measured source: `ec5327735b5994c02b1dc84c1913ffbf6df9c5ff`.
This is partial evidence, not story completion. No production code was changed.

## Scope and method

Boss authorized browser-mode evidence because the desktop WebView and browser
use the same terminal frontend: "The native UI is the same as the web browser,
it is Tauri." This run does not claim transport or engine equivalence.

The isolated `make agent-test AGENT_TEST_INSTANCE=agent-test-2040
AGENT_TEST_PORT=19884` served the embedded frontend. Backend setup and inspection
used only its instance Unix socket. Browser authentication used the supported
`/api/auth/session-token` endpoint on that socket; no credential store was read.
The worktree native build went through build-slot and the mbx cargo shim.

The named stealth-wrapper tab used Chromium with reported user agent
`Chrome/149.0.0.0`, a 1400×900 CSS viewport, DPR 1, and a visible document.
Actual engine build metadata was not independently captured. CDP
`Input.dispatchKeyEvent` drove the wrapper-owned tab, without JS event dispatch.
All measured keydown events reported `isTrusted=true` on the real terminal input.

A real Python PTY printed 12 source-like rows per 100 ms. Its raw-input reader
wrote each circled key in a persistent first-row echo, outside the scrolling
region. Each key also selected a distinct gray background for that echo. This
controlled workload prevents a key from scrolling away before observation; it is
not an ego transcript or an ego composer.

The observer recorded trusted event timestamps, matching production canvas
`fillText`, rAF gaps, a 100 ms event-loop sentinel, and CDP compositor PNG frames.
Offline analysis finds the first PNG with at least 20 pixels of the key's unique
gray echo background in its observed canvas cell crop. Neutral gray was used
because the earlier colored smoke exposed compositor color conversion. The
retained matching PNGs show the echoed glyph and its background.

The compositor number is event timestamp to the first matching screencast frame's
CDP timestamp, aligned through `performance.timeOrigin`. It is sampled browser
compositor observation, not physical display scanout or a native WebKit claim.
Capture overhead and frame sampling are included. A missed bound in this harness
does not, by itself, identify the production layer responsible for the delay.

## Results

Percentiles use nearest rank. The main run increased the input sample count after
the five-key smoke validated trusted input, the persistent echo and PNG matching.

| Observation | Smoke | Main |
| --- | ---: | ---: |
| Trusted inputs / matching fills / compositor matches | 5 / 5 / 5 | 20 / 20 / 20 |
| Event → matching fill p50 / p95 / max | 26.900 / 33.000 / 33.000 ms | 25.000 / 33.800 / 36.300 ms |
| Event → matching compositor PNG p50 / p95 / max | 40.186 / 62.131 / 62.131 ms | 29.557 / 42.789 / 44.755 ms |
| rAF gap p95 / max | 16.800 / 25.000 ms | 16.800 / 16.800 ms |
| 100 ms sentinel gap p95 / max | 101.000 / 101.000 ms | 101.300 / 101.400 ms |
| Compositor PNGs retained | 99 | 141 |

Main output grew from 5,696 to 72,738 bytes over 3,583.305 ms:
**18,709.541 bytes/s**. This is a current workload observation, not an A/B
throughput non-regression result. The sentinel is a browser event-loop heartbeat,
not the desktop backend's frontend-heartbeat receipt.

The run does not pass the original under-one-frame requirement or the later
below-33-ms target: main fill p95 is 33.8 ms and sampled compositor p95 is
42.789 ms. No speculative scheduling or rendering change was made from these
aggregate timings. There is no same-workload pre-fix A/B in this assignment.

## Incomplete criteria and prerequisites

- **Help versus plain PTY:** unavailable. The installed executable reported
  `ego 0.1.2`. The isolated plain nested-PTY probe printed its startup banner,
  but produced no prompt or `/help` response in the bounded observation. A
  second attempt submitted `/help` after the banner and still obtained no help
  bytes. This does not prove an ego defect; the probe or startup state needs
  diagnosis. No browser/plain help ratio or one-block-help claim is made.
- **WebKit:** `safaridriver` was installed and its status endpoint was ready.
  Session creation returned: `Could not create a session: You must enable
  'Allow remote automation' in the Developer section of Safari Settings to
  control Safari via WebDriver.` No setting was changed. An earlier driver
  attempt hit a port collision; the retained decisive retry used an ephemeral
  available port and confirmed driver readiness before requesting a session.
- **Initial Chromium attempt:** the console was locked and a compositor screenshot
  timed out despite `document.visibilityState=visible`. That attempt supplied no
  timing evidence. The later unlocked runs above supplied actual PNG frames.
- **Story mutation:** the main-store story was set `in_progress` through Wiz at
  assignment start. Later direct rewording was denied: `edit may not write ...:
  it is inside a read-only root`. No alternate write route was used after that
  denial. The coordinator must apply any browser-scope wording and worklog
  update in the main store. No criterion was dropped, rejected or checked here.

## Retained evidence and cleanup

Evidence root: `~/Gits/.tmp/2040-web/`.

- `main/{probe,frames,summary,throughput,output,setup}.json` and `match-*.png`:
  main raw timings, actual compositor frames and per-key matches.
- `smoke/`: successful five-key neutral-gray smoke.
- `smoke-initial/`, `smoke-color-profile/`, `diagnose.json`: rejected observer
  calibration attempts; not mixed into the main distributions.
- `browser.mjs`, `measure.mjs`, `rows.py`: retained acquisition harness.
- `safari-preflight.json`, `safaridriver.log`: actual WebKit prerequisite refusal.
- `plain-help-*.json`, `plain-help-live.bin`, `help_probe.py`: incomplete help
  probe; no successful help timing.
- `build.log`: native exploratory build; mbx doctor reported zero failures.
- `cleanup.json`: zero sessions before shutdown, successful owned tab and named
  daemon closure, and exact instance stop exit 0.

All measurement sessions were deleted. The instance supervisor exited after its
requested stop (the make wrapper reports the expected child termination). Shared
Chrome and the daily TUIC instance were not stopped. No test suite, rb test,
mutation run, production restart or installed-binary replacement was performed.


## Follow-up: layered Chrome trace

The coordinator narrowed the target to Chrome browser mode, authorized the same
controlled workload, and replaced the ego-help comparison with a plain-shell
`seq` comparison. Safari automation is not a prerequisite for this follow-up.
The production source was unchanged from the source identified above.

### Observation contract and trace boundaries

The consumer observes the glyph produced in response to a trusted terminal key,
not an input acknowledgement. Real state comes from the browser event, HTTP PTY
writer, real PTY reader/writer, terminal grid, WebSocket arrival, canvas operation,
and compositor PNG. The controlled Python echo is the external fixture. It does
not represent an ego composer. The radius of the observation is the isolated
`agent-test-2040` instance on port 19884 and its owned browser tab.

Temporary, environment-gated Rust observers recorded epoch microseconds at HTTP
write begin/end, PTY read, VT application, frame serialization and frame submit.
The Python fixture recorded epoch nanoseconds at input read and echo write.
Browser observers recorded fetch, binary grid-message arrival and the existing
fill/PNG boundaries. Browser clocks were aligned through `performance.timeOrigin`.
The native observers were removed byte-for-byte after acquisition; their patch is
retained as evidence, not shipped code. Logging and PNG capture can perturb the
instrumented run.

All 20 trusted inputs have a writer, PTY echo, VT, frame, browser arrival, fill and
PNG observation. All native trace records identify the one measurement session
`89008dcf-1aae-463d-b747-d2ec21e05b78`. Frame correlation scans for the unique echoed
codepoint, rather than decoding and asserting row geometry. Thus arrival is the
first key-bearing wire message, not a separately verified accepted-row timestamp.
Submillisecond cross-clock values are not precise enough for causal attribution:
one submit-to-arrival sample was -0.012 ms.

### Layer timings

The following table uses the statistical median and nearest-rank p95. Percentiles
of different stages must not be added together.

| Boundary | Median | p95 | Maximum |
| --- | ---: | ---: | ---: |
| Trusted event → browser fetch | 0.200 ms | 0.400 ms | 10.200 ms |
| Fetch → HTTP writer begin | 1.064 ms | 2.223 ms | 2.251 ms |
| HTTP writer begin → done | 0.044 ms | 0.094 ms | 0.114 ms |
| Writer done → Python input read | 0.021 ms | 0.049 ms | 0.122 ms |
| Python input read → echo write done | 0.026 ms | 0.048 ms | 0.096 ms |
| Echo done → PTY reader | 0.005 ms | 0.024 ms | 0.046 ms |
| PTY reader → VT applied | 0.076 ms | 0.169 ms | 0.229 ms |
| VT applied → frame cut | 0.047 ms | 4.456 ms | 8.074 ms |
| Frame cut → submit | 0.021 ms | 0.065 ms | 0.067 ms |
| Frame submit → browser message | 4.839 ms | 14.753 ms | 15.382 ms |
| Browser message → matching fill | 16.250 ms | 17.100 ms | 23.400 ms |
| Matching fill → sampled compositor | 10.853 ms | 19.082 ms | 27.563 ms |
| Trusted event → matching fill | 26.000 ms | 34.500 ms | 36.200 ms |
| Trusted event → sampled compositor | 38.028 ms | 55.063 ms | 55.260 ms |

Cumulative event → frame-submit median/p95/max was 1.725/9.712/11.733 ms;
event → browser-message was 9.900/16.200/18.200 ms. The frontend fill/compositor
boundaries, not the PTY input write or VT parsing, therefore account for most of
the measured end-to-end delay. Submit-to-arrival also contributes up to almost
one frame; that interval includes backend WebSocket scheduling, network delivery
and browser message dispatch, not just network transit.

The browser's rAF gap p95/max was 16.7/16.8 ms. The 100 ms sentinel p95/max was
108.6/113.6 ms. Output grew by 65,158 bytes over 3,545.654 ms, or 18,376.863 bytes/s.
These are instrumented current-run observations, not throughput or heartbeat
non-regression evidence.

### Attribution and implementation decision

The measured dominant boundary is **browser key-bearing message arrival to
canvas fill**, followed by compositor observation. This is not evidence of an
input-writer stall. Source inspection confirms the browser WebSocket handler
calls the terminal handler synchronously (`canvasTerminalTransport.ts`,
`deliveredFrame`), while `CanvasTerminal.tsx`, `scheduleRepaint`, coalesces work into
one `requestAnimationFrame`. The backend grid WebSocket waits on the frame watch
channel and sends the latest frame, with a full-frame repair for sequence gaps
(`mcp_http/session.rs`, grid WebSocket send task).

No production defect is isolated by this trace. In particular, the earlier
suspicion that the first key frame was dropped is not established: for the first
key the matching fill precedes the next key-bearing wire frame. rAF timestamps
are frame timestamps, not callback-entry timestamps, so their difference from a
fill cannot be called paint execution time. The trace does not split accepted
frame handling, rAF queueing, callback entry, renderer execution and compositor
scheduling finely enough to prove an unnecessary extra turn.

**No fix was made and no regression RED was fabricated.** Painting synchronously
on every arrival or bypassing the serializer would trade away existing batching,
damage and ordering contracts without evidence of a specific defect. The current
numbers still fail the under-one-frame requirement and the below-33-ms fill p95
target. This report narrows the investigation boundary; it does not claim that
all latency inside that boundary is avoidable or that the story can close.

### Plain-shell A/B

The successful probe uses the same `/bin/sh -c` workload on a real 40×120 PTY:
print READY, wait for `go`, run `seq 1 1000`, print END, then wait to keep the final
screen alive. The plain arm uses `subprocess.Popen` with PTY descriptors; the TUIC
arm uses the isolated backend session and browser. Startup is outside the timed
interval. Both output streams include the expected final marker. The browser
observer sees the production canvas draw that marker.

| Pair | Plain input → final PTY bytes | TUIC HTTP submission → polled output | TUIC submission → final canvas fill | Fill / plain |
| --- | ---: | ---: | ---: | ---: |
| 1 | 9.785 ms | 19 ms | 23.300 ms | 2.381× |
| 2 | 8.607 ms | 35 ms | 41.200 ms | 4.787× |
| 3 | 8.458 ms | 16 ms | 13.700 ms | 1.620× |

The median fill/plain ratio is 2.381×, not a pass of a 2× target. There are only
three pairs. The TUIC output endpoint is polled, so its result is an upper bound
with polling/API overhead; a fill can precede that observation. Plain PTY-read
completion and canvas-fill completion are deliberately different endpoints.
These numbers are a same-workload processing/display comparison, not equivalent
physical presentation clocks, an ego `/help` comparison, or a compositor A/B.

The original manual nested-fork probe, including a `pty.fork` variant, hung in
child cleanup (`waitpid`, recorded by faulthandler). No ego or shell defect is
inferred. The replacement avoids Python fork-child setup and successfully captures
both arms. A first replacement attempt exited the shell immediately after END:
backend bytes arrived but the canvas observer saw only `go`. Keeping the shell
alive after END resolved that harness lifetime error. A stale wrapper tab also
required a new, uniquely named browser session. These rejected attempts are not
mixed into the three successful pairs.

### Evidence, checks and remaining scope

Additional evidence under `~/Gits/.tmp/2040-web/layer/`:

- `run/{probe,frames,summary,throughput}.json`, `run/match-*.png`: trusted trace and
  matching compositor frames.
- `native.log`, `echo.jsonl`, `observer.patch`: native and Python observation.
- `analyze.py`, `timeline.json`, `stages.json`, `trace-events.json`: all per-key
  correlations and aggregate stage timings.
- `seq-pairs.json`, `seq-output-*.json`, `seq-last.json`: successful shell A/B.
- `seq-burst.mjs`, `run-seq.py`, `seq-lifecycle.json`: acquisition and cleanup.
- `burst-stack.log` and rejected probe logs: diagnosed harness failures.

The final lifecycle reports zero remaining measurement sessions, owned tab and
named daemon closure, and exact instance stop exit 0. The make supervisor exit 2
wraps its requested child termination. All temporary production observers were
restored, and no installed application binary was replaced. No application suite,
GREEN run, mutation, cross-target test or Safari verification was claimed.

The main-story read-only-root denial remains binding. No alternate tool was used
to modify its criteria. Original help, one-frame and combined non-regression
criteria remain unsupported; the parent must reconcile scope and worklog wording,
not mark those requirements passed from these measurements.


## Follow-up: accepted frame → actual rAF callback → paint

A second coordinator assignment requested callback-entry timestamps to distinguish
normal frame scheduling from an unnecessary second rAF. It used the same isolated
instance, 20 trusted keys and controlled output. No production fix was present.

The temporary frontend observer records `performance.now()` after decoded rows
are installed and the frame becomes current (`accepted`), at `scheduleRepaint`,
when a new rAF is queued, at the actual callback entry, and before/after
`paintFrame`. Each event includes the accepted-frame number, queued-rAF number
and circled key decoded from the authoritative first row. A separate every-frame
rAF sentinel records both its browser frame timestamp and actual callback-entry
time. This removes the earlier ambiguity between rAF timestamp and execution.
All temporary source additions were restored byte-for-byte after the run.

### Result: no extra rAF in this workload

**All 20 accepted key frames were painted in the first subsequent rAF opportunity
observed by the sentinel, in that callback itself.** There was exactly one
sentinel callback between each acceptance and the matching production callback,
with no intervening production callback. No key required a second queued rAF.
Nineteen acceptances queued a new callback immediately; one reused an already
pending callback and painted in it. No key was held behind a separate non-urgent
paint queue.

| Boundary | Median | p95 | Maximum |
| --- | ---: | ---: | ---: |
| Browser message → accepted decoded frame | 0.100 ms | 0.200 ms | 0.300 ms |
| Accepted frame → schedule call | 0.000 ms | 0.100 ms | 0.100 ms |
| Accepted frame → actual rAF entry | 15.950 ms | 16.300 ms | 16.400 ms |
| Queued rAF → actual entry | 16.000 ms | 16.400 ms | 16.600 ms |
| rAF frame timestamp → actual entry | 0.100 ms | 0.200 ms | 0.200 ms |
| Actual rAF entry → matching fill completion | 0.200 ms | 0.300 ms | 12.500 ms |
| Whole paint call | 0.300 ms | 0.500 ms | 12.900 ms |
| Trusted event → browser message | 11.150 ms | 19.000 ms | 24.100 ms |
| Trusted event → matching fill | 27.450 ms | 40.700 ms | 47.500 ms |

The key-bearing frames arrived just after the previous sentinel callback:
acceptance minus previous callback-entry median/p95 was 0.700/1.300 ms, with a
9.000 ms maximum. This explains why the next legitimate rAF cost nearly a full
frame in most samples. It is not an application double-rAF. The source's one-rAF
coalescing mechanism behaves as written under this capture. The trace cannot
attribute why browser messages are phase-aligned this way; browser delivery,
backend scheduling and capture effects are not separated by this frontend probe.

The first key had a 12.900 ms paint and 12.500 ms entry-to-matching-fill interval;
the other 19 whole paints were at most 0.500 ms. That first paint is recorded, not
silently excluded. Its cause is not isolated: the observer does not identify a
font-cache, canvas, compositor or OS cause. It is not evidence of an extra queued
paint opportunity.

For a concrete first-key timeline (milliseconds from browser time origin):
wire arrival 5160.200; accepted 5160.400; queued 5160.500; actual callback entry
5176.200 (browser rAF timestamp 5176.000); matching fill completion 5188.700;
paint end 5189.100. The long interval occurs inside this same paint, not in a
second callback. Key 15 demonstrates the pending-callback path: queued at
7309.600, accepted at 7318.500, callback at 7326.200, fill at 7326.400.

There were 20 trusted inputs, 20 fills and 20 matching compositor PNGs. Sampled
compositor p95/max was 52.242/55.231 ms; rAF gap p95/max was 16.7/16.8 ms; the
100 ms sentinel p95/max was 101.7/106.0 ms. Observed output was 18,691.179 bytes/s.
These are another instrumented observation, not an optimization before/after or
non-regression comparison. In particular, fill p95 40.7 ms does not improve on the
earlier 34.5 ms and does not meet the target.

### Decision and evidence

No avoidable extra scheduling turn was found. No production fix or RED test is
justified for a double-rAF or deferred-queue bug that this trace does not show.
Removing rAF batching would be a design trade-off (paint outside the browser's
frame callback), not a supported repair of an extra wait. The earlier three-pair
`seq` ratio remains the measured comparison; it was not rerun as a claimed
post-fix result because there is no fix. Story criteria 3/4/24 still require the
coordinator's scope decision; none was marked met.

Evidence is under `~/Gits/.tmp/2040-web/fine/`: `observer.patch`,
`CanvasTerminal.tsx.original`, `measure-fine.mjs`, `run.py`,
`run/{probe,summary,frames,throughput}.json`, actual `run/match-*.png`,
`analyze.py`, `timeline.json` and `analysis.json`. The 444 frontend trace events
identify only session `21c6d582-39ba-4eb9-85ba-88653c78a740`. The per-key timeline
retains queued/accepted frame IDs, all intermediate callbacks and same-callback
fill assertions. `lifecycle.json` records zero remaining PTY sessions, owned tab
and named daemon closed, exact instance stop exit 0, and expected supervisor
termination. No daily instance, shared Chrome process or installed binary was
stopped or replaced.


## Follow-up: server send, browser ACK and randomized input phases

The coordinator asked whether an rAF-timed client acknowledgement holds the next
echo frame, and requested randomized phases if no such coupling exists. The new
trace includes native frame cut, watch submission, WebSocket send begin and send
completion, browser WebSocket sends, HTTP request paths, decoded acceptance and
actual rAF entry. It runs 20 trusted keys per arm with seeded pseudo-random delays
of 103–265 ms between input pairs, rather than a fixed 150 ms. A second arm uses
the same randomized schedule with compositor screencast disabled, to test whether
continuous PNG capture alone creates the phase alignment. The controlled PTY
output remains unchanged. No production fix was present.

### Source and observed client traffic exclude an ACK gate

- Browser `WsTransport.ackFrame` is a no-op by design
  (`canvasTerminalTransport.ts:466`). It does not send credit from paint or rAF.
- Desktop `TauriTransport.ackFrame` invokes `ack_terminal_frame`, but it is not
  the browser transport used here. The desktop gate cannot stop the ticker when
  browser watch subscribers exist (`pty.rs`, ticker gate/watcher branches), and
  `send_grid_frame` publishes to the watch before consulting the desktop channel
  gate.
- The grid WebSocket sender waits on `frame_rx.changed()` and sends the latest
  frame. It repairs skipped sequence numbers with a full frame. Its receive loop
  ignores text control messages and does not require an ACK (`mcp_http/session.rs`,
  grid stream sender/receiver). These are bounded newest-frame semantics, not a
  client-paint credit protocol.
- The observer recorded **zero browser WebSocket sends in both 20-key arms**.
  HTTP paths during measurement contain input writes, occasional full-frame
  reconciliation requests, foreground observation and unrelated config reads;
  there is no ACK, visibility or resize request per echo. All request paths and
  times are retained in `server-analysis.json`.

### The server sends promptly; randomized phases do not remove alignment

Each table cell is median / p95 / maximum milliseconds, with 20 keys per arm.

| Boundary | Randomized + PNG capture | Randomized, no screencast |
| --- | ---: | ---: |
| Event → frame watch submission | 1.590 / 2.487 / 4.439 | 2.611 / 6.636 / 7.673 |
| Frame cut → watch submission | 0.011 / 0.066 / 0.067 | 0.015 / 0.056 / 0.064 |
| Watch submission → WS send begin | 0.020 / 0.080 / 0.129 | 0.017 / 0.065 / 0.066 |
| WS send begin → send completion | 0.031 / 0.084 / 0.093 | 0.024 / 0.076 / 0.089 |
| WS send completion → browser message | 6.434 / 16.535 / 16.752 | 7.796 / 15.253 / 16.734 |
| Accepted frame → actual rAF entry | 15.850 / 16.400 / 16.400 | 15.650 / 16.300 / 17.100 |
| Event → matching fill | 24.350 / 34.500 / 34.800 | 25.800 / 34.700 / 37.700 |

Send completion is completion of the awaited WebSocket send operation, not a
packet-capture timestamp. Its interval to JS message handling still includes
socket/network delivery, browser ingress and browser task dispatch. Native and
browser timestamps are epoch-aligned; do not read submillisecond cross-clock
values as precise hardware timings.

The actual key event phases varied across the frame period: phase since previous
sentinel callback ranged 1.700–15.700 ms with capture and 1.700–16.500 ms without
capture (19 keys each; the first precedes the sentinel's first callback). Native
send completion phases also span the period: 0.083–16.369 ms and 0.150–16.613 ms,
respectively. The server is not waiting to send just after client rAF.

But browser message phases remain tightly after the previous callback:
median/p95/range 0.650/1.500/0.300–1.600 ms with capture and
1.000/2.100/0.300–2.500 ms without capture. Consequently, the accepted-to-rAF wait
**does not become uniform with an approximately 8 ms median**. Randomizing input
and removing screencast did not eliminate it. All 40 keys still paint in the first
subsequent sentinel-observed rAF opportunity, in that callback.

This rules out the proposed application ACK/credit coupling and also contradicts
the alternative claim that fixed test cadence alone explains the alignment. The
remaining observed phase concentration arises after server send completion and
before JS message delivery, followed by the legitimate next rAF. This trace does
not distinguish socket delivery from Chromium ingress/task scheduling, and does
not prove a repairable TUIC code defect inside that interval. It would be wrong
to force the evidence into either of the proposed explanations or claim the
random-phase control passed a uniformity result that it did not produce.

No speculative ACK bypass, paint bypass or RED was added. There is no post-fix
`seq` comparison because there is no fix. The unresolved transport/browser
interval and unmet timing bounds remain explicit for the coordinator's next
scope decision. The earlier three successful shell pairs remain the available
shell comparison, not a new result.

### Evidence and cleanup

Evidence is under `~/Gits/.tmp/2040-web/phase/`: `native.log`, `observer.patch`,
`measure-fine.mjs`, `run.py`, `analyze-fine.py`, `analyze-server.py` and
`lifecycle.json`. Each of `capture/` and `no-capture/` holds `probe.json`,
`summary.json`, `timeline.json`, `analysis.json`, `server-timeline.json` and
`server-analysis.json`. The capture arm also retains actual compositor PNGs;
the no-capture arm deliberately has no presentation samples. Session IDs are
`104aea93-3631-473c-ac78-78babd4c4554` and
`778ce40a-529e-4fa5-8be0-0ea79fac2abf`, respectively.

All temporary Rust and frontend observers were restored byte-for-byte. The final
lifecycle reports zero measurement sessions, owned tab and named daemon closure,
and exact isolated instance stop exit 0. No shared Chrome or daily app process
was stopped. No source fix, suite, GREEN, mutation or cross-target claim is made.


## Follow-up: TCP_NODELAY and standalone browser controls

The coordinator requested the server socket policy plus a standalone WebSocket
baseline in the same Chrome. The result distinguishes unsolicited push from the
trusted-input/request/echo path; treating those as equivalent would give a false
TUIC attribution.

### TCP_NODELAY is not selected by this HTTP listener

The actual run logs identify the HTTP-only listener. Its source constructs
`axum_server::from_tcp(listener)` at `src-tauri/src/mcp_http/mod.rs:2990`, then
calls `server.serve(svc)` at line 2997, without selecting an acceptor. The lockfile
pins `axum-server` 0.8.0 (`src-tauri/Cargo.lock:560`). Its installed source is under
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/axum-server-0.8.0/`:

- `src/server.rs:70`: `Server::from_listener(TcpListener::from_std(listener)?)`.
- `src/server.rs:165–166`: `from_listener` chooses `DefaultAcceptor::new()`.
- `src/accept.rs:42`: that acceptor returns `Ok((stream, service))`, unchanged.
- `src/accept.rs:63`: the separate opt-in `NoDelayAcceptor` calls
  `stream.set_nodelay(true)`; the measured HTTP branch did not select it.

The temporary experiment selected
`server.acceptor(axum_server::accept::NoDelayAcceptor::new())` on that HTTP-only
branch and reran the same 20-key randomized, no-screencast arm. This change and all
observers were restored after acquisition. The TLS branch was not measured and
no TLS-wide claim is made.

| Observation (median / p95 / maximum) | Original HTTP arm | Temporary NODELAY arm |
| --- | ---: | ---: |
| Server WS send completion → browser message | 7.796 / 15.253 / 16.734 ms | 9.817 / 16.098 / 16.262 ms |
| Browser arrival phase after previous rAF | 1.000 / 2.100 / 2.500 ms | 0.900 / 2.100 / 2.200 ms |
| Accepted frame → actual rAF entry | 15.650 / 16.300 / 17.100 ms | 15.600 / 16.200 / 16.300 ms |
| Trusted event → matching fill | 25.800 / 34.700 / 37.700 ms | 29.100 / 34.200 / 34.300 ms |

NODELAY did not remove the phase concentration or meet the fill-p95 target. These
small sequential samples do not establish an overall performance regression or
improvement. They do reject NODELAY as an explanation of the observed phase lock.
No NODELAY production change was retained.

### Standalone controls outside TUIC

A dependency-free Node HTTP/WebSocket server and tiny page live only under
`~/Gits/.tmp/2040-web/baseline/`. The server performs the WebSocket handshake,
writes server frames, explicitly sets `socket.setNoDelay(true)`, and records send
begin/completion. The page records message arrival and actual rAF callback entry.
It loads no TUIC JavaScript. Each arm uses the same shared Chrome engine, its own
wrapper-owned tab, visible document and 1400×900 CSS viewport. No screencast runs
while these samples are collected. There are 20 messages in each arm.

Three controls avoid a misleading text-only or inactive-input comparison:

1. **Unsolicited text:** the minimal server sends JSON at seeded randomized
   intervals, with no user input or application code.
2. **Unsolicited binary under traffic:** ArrayBuffer messages, 1,510-byte key-like
   payloads and 57,239-byte background frames every 100 ms. The page decodes JSON
   and ignores background frames. This controls binary framing and approximate
   grid wire traffic without TUIC rendering.
3. **Trusted input → HTTP → binary echo:** the same tiny binary page adds a plain
   input and a keydown handler that POSTs the key to `/input`; the Node server
   immediately writes it to the WebSocket. CDP supplies 20 trusted keydown events
   at the same randomized intervals. There is no terminal, Rust, Solid store,
   grid decoder, frame gate or TUIC code. Background traffic is unchanged.

| Standalone control | Arrival phase median / p95 / range | Message → next rAF median / p95 / range |
| --- | ---: | ---: |
| Unsolicited text | 8.050 / 15.700 / 1.500–15.800 ms | 8.650 / 14.600 / 0.800–15.200 ms |
| Unsolicited binary + background | 9.850 / 16.100 / 0.600–16.300 ms | 6.850 / 15.400 / 0.300–16.200 ms |
| Trusted-key HTTP/binary echo | 1.100 / 2.100 / 0.200–2.100 ms | 15.150 / 16.300 / 5.300–16.400 ms |

The first two unsolicited controls are spread across the frame period. The third
**reproduces the tight post-rAF arrival concentration without TUIC**. All 20
input events in that control report `isTrusted=true`. Its server-send → browser
arrival median/p95/max is 1.590/14.748/15.857 ms, versus 0.196/0.246/0.279 ms for
unsolicited text and 0.418/0.767/0.823 ms for unsolicited binary. The input/echo
control therefore reproduces both delayed delivery and the nearly-full-period
next-rAF wait outside the application.

This is decisive against attributing the alignment to TUIC merely because the
unsolicited baseline is uniform. A key-driven round trip is not an unsolicited
push. The observed residual is reproducible in the standalone Chrome input/HTTP/
WebSocket path. The controls do not isolate Chromium's precise internal scheduling
policy or separate the trusted-input effect from the input-triggered HTTP request;
no undocumented browser mechanism is asserted as fact. They do show that TUIC
business logic, its grid queue, its frame acknowledgements and its socket NODELAY
policy are not required to produce this phase concentration.

No repairable TUIC scheduling defect was isolated, so no production fix or RED was
fabricated. Painting outside rAF would remain a product scheduling trade-off,
not repair of a TUIC-specific extra queue proven by this evidence. The original
one-frame/help/non-regression criteria remain unmet; no post-fix `seq` ratio is
claimed. The coordinator owns the scope decision.

### Evidence and cleanup

`~/Gits/.tmp/2040-web/nodelay/` retains the exact observer patch, original source
backups, native log, acquisition and analyzers; `no-capture/` holds all per-key
frontend/server timelines and distributions. Its session was
`535a280b-09b1-4883-858f-d0044e33705d`.

`baseline/`, `baseline/binary/` and `baseline/echo/` each retain standalone server
and runner source, raw `probe.json`, screenshot, owned browser identity, server
PID/port record and `lifecycle.json`. `baseline/analysis.json` compares all three.
The first baseline runner attempt blocked its own Node HTTP event loop while
waiting synchronously for browser navigation. It timed out; its owned tab/daemon
were closed. Moving the server to a recorded child process removed that harness
deadlock. That attempt supplied no timing samples.

All successful controls closed their owned tabs/daemons and stopped their recorded
server child. The NODELAY lifecycle records zero remaining terminal sessions and
exact isolated instance stop exit 0. All temporary production source was restored
byte-for-byte, and no daily app/shared Chrome process was stopped or replaced.


## Native WebKit follow-up — 2026-10-23

The first attempt below is historical. Its startup block was resolved with the
coordinator-authorized default ego launch. The successful acquisition follows it.

The coordinator authorized the isolated native target with instance
`agent-test-2040` and port `19890`. All inspection used its Unix socket, not the
orchestrator's debug interface. The application is **CanvasTerminal**, not xterm:
its keyboard target is a hidden `input[aria-hidden=true]`, and its output path is
binary grid installation followed by the custom canvas renderer. There is no
xterm textarea or xterm write callback to measure.

### Historical prerequisite failure — superseded by acquisition below

The successful native preflight reports WebKit UA
`AppleWebKit/605.1.15`, a 640×420 CSS viewport and DPR 2. This is the UA string,
not an independently verified WebKit build number. Six successive actual rAF
callbacks ran at performance-clock values 652, 669, 686, 704, 719 and 735 ms.
Thus the native WebView did execute frames during this preflight. This is not a
keystroke sample or proof that frames continued for an entire workload.

An earlier screen-lock check returned true. That attempt stopped without timing
claims. Later unlocked runs reached JavaScript evaluation and native frame
callbacks. One evidence-helper error used `#root`; the verified mount point is
`#app`. This selector was corrected. Another launch passed ego directly as the
HTTP session's shell command, but TUIC appends `-l` to a shell. Ego rejected that
argument. The corrected launcher is a Python script that ignores the shell's
extra argument and execs the intended ego argv. Neither failed setup supplied
latency samples.

The final real TUIC PTY session was
`d0ea609f-84fc-4a09-9439-5d67269235d6`. It ran installed ego 0.1.2 with an
isolated EGO_HOME and argv:

```text
ego --bare --offline --thinking off -m ollama/gemma4:e4b-mlx
```

It printed the ego/model/mode banner and shortcut hints, including `/help`, but
no live composer or help body was observed before cleanup. Synthetic keydown
for `/help` plus Enter produced no new PTY output in the following two seconds.
The first unique CJK idle-key probe then failed its 1500 ms echo acquisition
bound. Its event was `isTrusted=false`. No 200-key acquisition ran. This is a
failed measurement prerequisite, **not** evidence of 1500 ms steady-state input
latency or proof that TUIC dropped the key. The probe did not establish that ego
had completed startup and opened its live composer.

### Independent startup control

A separate plain-PTY control used macOS `script` with a subprocess-owned PTY,
24×80 geometry, the same executable/flags and isolated EGO_HOME. The exec
launcher recorded all three standard streams as TTYs. The control printed the
banner at about 0.4 seconds; `/help` was written at 5.014 seconds. No additional
output was captured before the 20-second observation ended. No manual fork or
fork-child terminal setup was used. This reproduces the unavailable help output
outside TUIC for the observed interval. It does not prove a root cause, rule out
provider/startup dependencies, or establish that ego would never become ready.

No model generation, credential read or login was used. A responsive ego
composer and help output are still required before this assignment can provide
valid samples. It would be incorrect to call 200 keys submitted after Enter
“while help prints” without first observing and correlating real help output.

### Limits and retained evidence

There is no native p50/p95 input result, no verified help-overlap sample, no
same-help-byte plain-shell replay, and no supported answer to whether native
WebKit has the Chrome phase concentration. The existing Chrome results remain
separate evidence. Future synthetic native results must explicitly exclude the
OS keyboard path; canvas fill and rAF remain proxies, not physical scanout.

`~/Gits/.tmp/2040-web/native/` retains `probe.json`, `ego-help-probe.json`,
`native.log`, `plain-ego.json`, `plain-ego-script.log`, the exec launcher's
TTY/PID record, the observer patch and original source backups, and the
acquisition scripts. Temporary Rust observations timestamped actual PTY reads;
temporary frontend observations covered decoded acceptance and canvas/rAF
boundaries. They were not retained as production code. Raw terminal text in
this private evidence comes only from the isolated probe.

Build-slot contention exceeded one evidence helper's one-hour setup budget.
That run was a setup timeout, not an application failure. The later run used a
longer setup budget and reached the native preflight. Setup/build wait and
acquisition bounds are not application latency samples.

The final lifecycle deleted the owned TUIC PTY, ran matching
`make agent-test-stop` with exit 0, and observed the supervisor exit. The
plain-PTY process and its recorded ego PID are gone. The native PID file is
absent. Both temporarily modified production files were restored byte-for-byte.
No shared Chrome, daily app, installed executable or other agent was stopped.

### Successful native acquisition — 200 idle and 201 output-overlapping keys

The coordinator subsequently authorized installed ego with its default home and
remote model, without reading credential stores. The exec wrapper launched ego
with no arguments, removed the EGO_HOME override and inherited TUIC variables,
and ignored the shell's `-l`. The observed model was
`openai-codex/gpt-6-sol`. A separate default-ego plain-PTY control reached a live
composer at about 0.847 seconds and emitted real `/help` chunks about 100 and
200 ms after submission. The isolated offline startup block is no longer current.

The successful native session was `8997b835-d267-4bcb-9418-884b618b75c1`, on the
same authorized instance and port. Its viewport was 640×420 CSS pixels, DPR 2,
with 15×38 codepoint cells. The WebKit UA remained `AppleWebKit/605.1.15`.
Six readiness callbacks ran at 2146, 2163, 2179, 2197, 2213 and 2229 ms.
The main sentinel measured 7,958 frame intervals: median 17 ms, p95 19 ms,
range 10–23 ms.

All input events were synthetic (`isTrusted=false`). They target the real hidden
input but **skip OS keyboard routing**. Unique CJK keys identify native PTY
reads, installed semantic grid rows and actual `fillText` calls. Temporary
frontend observers recorded acceptance, actual rAF entry and paint boundaries;
the Rust observer timestamped actual PTY reads. The next callback after a glyph
fill is an observation proxy, not compositor presentation or physical scanout.

The main arm collected 200 idle keys and 200 `/help` key candidates, one
outstanding echo at a time. Ctrl+U cleared drafts without submitting a model
prompt. Each `/help` letter was paced 25 ms, and the full rendered composer had
to equal `/help` before Enter. The unique key followed about 125–129 ms later.
Exactly 198 main candidates overlapped real help output; two commands finished
before the key and are excluded. All three smoke help keys also overlapped,
giving **201 verified overlap samples (198 main + 3 smoke)** from this same
instance/run and observer source.

For every included help sample, the native reader saw the unique echo before
the final help footer. The first help body preceded the key event by at least
21.937 ms. The same-native-clock echo/footer ordering corroborates the overlap;
frontend/native clock alignment is not the sole evidence. All 400 main keys
correlated with reader, semantic acceptance and glyph fill. The final log has
204 help headings (initial + 3 smoke + 200 main) and no `Working` chunks.

#### Native distributions

Values are milliseconds, shown as median / p95 / maximum. Native reader values
are rounded to one decimal: WebKit's integer-ms timestamps and cross-clock
alignment do not support sub-ms physical attribution. Apparent reader-to-grid
negative values down to −1.574 ms demonstrate this uncertainty (approximately
±2 ms), not reversed causality.

| Boundary | 200 idle | 201 verified help-overlap |
| --- | ---: | ---: |
| Keydown → actual PTY reader | 1.4 / 1.9 / 2.0 | 1.8 / 2.5 / 2.9 |
| Keydown → semantic grid accepted | 11 / 37 / 48 | 1 / 12 / 26 |
| Accepted → actual rAF entry | 6 / 16 / 18 | 8 / 16 / 17 |
| Keydown → glyph fill | **20.5 / 42 / 58** | **11 / 26 / 36** |
| Keydown → next rAF after fill | 37 / 60 / 73 | 27 / 43 / 53 |
| Help Enter → final footer read | not applicable | 170.1 / 183.6 / 199.5 |

All 400 main keys and all three included smoke help keys painted in the first
subsequent sentinel-observed production rAF, with zero strictly intervening
sentinel callbacks. Integer timestamp ties limit finer ordering. This is not
an end-to-end one-frame pass: idle and help glyph-fill p95 exceed the measured
19 ms frame-interval p95.

Acceptance phase after the preceding rAF was spread through the native period:
idle median/p95/range **9/15/0–17 ms**, help **8/14/0–16 ms**. This does not
reproduce Chrome's tight approximately 1 ms post-rAF acceptance concentration.
Native median acceptance-to-rAF is 6–8 ms rather than approximately 16 ms.
However, input trust, IPC versus HTTP/WS, viewport/DPR, workload and sample counts
differ. This is a phase comparison, not OS-path or browser-engine equivalence.
No extra native full-period queue was isolated, and no speculative scheduling or
NODELAY fix is justified by these results.

#### Exact captured-byte plain-shell replay

The initial real `/help` supplied exactly **7,148 bytes**, including actual echo,
status and cursor-control bytes. Every read in that initial command re-encoded
to its logged byte length. SHA-256:

```text
3ee421cc671f61bd8af5c4002de0a5fb10bc72c0f15da6d97800036b4f395fe6
```

A real 15×38 plain-shell PTY waited for `go`, then used `cat` to emit that exact
file and stayed alive. Echo and output postprocessing were disabled to prevent
CRLF mutation. A two-trial calibration preceded 20 trials; each verified byte
count, equality and hash. Submission-to-last-byte duration was median
**2.885 ms**, p95 **3.329 ms**, range **2.565–3.444 ms**.

The captured native command took **103.946 ms** from Enter to its last PTY read
and **142 ms** to the verified final help `paint_end` (accepted at 4096 ms,
paint ended at 4097 ms, submission at 3955 ms). Ratios to the shell median are
**36.028×** for reader completion and **49.218×** for canvas completion. The
latter compares different endpoints; the shell replay has no renderer or
compositor endpoint. This is one captured native command against 20 exact-byte
replays, not 20 paired native commands. Neither comparison meets 2×.

The native producer already consumes about 104 ms before the last PTY read.
The independent default-ego control also delayed help output. Thus the replay
is a byte-throughput control, not proof that all excess is caused by TUIC.
The 201 overlap commands are a separate arm; their footer distribution must not
be substituted for repeated measurements of the initial command's final paint.

#### Failed acquisitions, diagnostics and limits

Failed attempts are retained but excluded from the successful distribution:

- Early `/health` did not imply frontend readiness. The helper now retries fresh
  DOM evaluations with a two-second per-attempt budget and a 90-second readiness
  budget. It uses `#app`, not the earlier mistaken `#root`.
- An initial rapid synthetic burst reordered `/help` into `h/elp`. Ego entered
  `Working`, so a provider turn was attempted. No provider result was captured
  in the inspected trace; billing and completion were not independently checked.
  Its nominal idle arm is invalid and is not included here. It also lost the
  main export because a console-warning prefix confused JSON parsing. The
  helper now parses after the final `\n---\n`, paces input and verifies the
  complete rendered slash command before Enter. This harness failure does not
  establish an ordinary physical-typing defect. The earlier statement that no
  model generation was used applies only to that historical isolated attempt,
  not to all acquisition attempts.
- A later guard caught a stale `| /hel` draft and aborted without submitting it.
- Default ego then failed to parse a string decimal in `models.toml`. The
  coordinator reported that another agent created and removed that configuration;
  the subsequent run succeeded. This peer did not read or modify that store.
- Logs include `canvas desync healed` and `Settings reload failed` warnings.
  They are ancillary observations, not established latency root causes, and
  were not silently fixed.

Some later native reads split UTF-8 sequences and the observer's lossy text does
not re-encode to the original byte count (`raw_utf8_exact=false` for the whole
trace). Do not replay the entire decoded log as original bytes. This does not
invalidate the individually verified initial 7,148-byte capture or the complete
key correlations. Observer logging and row snapshots can perturb timing; there
is no claim of controlled unrelated machine load or statistical engine parity.
The reader-to-grid residual is not fully attributed without frame-cut/submit
observations in this native arm.

#### Evidence, cleanup and story status

`~/Gits/.tmp/2040-web/native/` retains `main.json`, `smoke.json`, `probe.json`,
`native.log`, `reader.json`, `timeline.json`, `analysis.json`, `analyze.py`,
`analysis-validation.txt`, `captured-help.bin`, `replay.py`, `replay-2.json` and
`replay-20.json`. The observer patch, original sources, acquisition helpers and
named failed-attempt artifacts remain available. Re-running `analyze.py`
validates counts, all correlations, overlap ordering, first-next-rAF results,
zero Working chunks and replay hashes without rebuilding or reacquiring.

The lifecycle deleted the owned session and stopped the exact instance with
exit 0. The make supervisor returned 2 after the sanctioned app termination;
the lifecycle/background wrapper returned 0. Temporary CanvasTerminal and PTY
sources were restored byte-for-byte. Final checks found the recorded supervisor,
helper, ego and all 20 replay PIDs absent, and the native PID file absent.
The evidence Python scripts passed AST parsing, and the analyzer assertions
passed. No production change ships in this report.

**Criterion 3 remains unchecked and fails the measured one-frame and 2× targets.**
Criteria 4 and 24 and throughput/heartbeat non-regression remain unsupported.
No RED/GREEN, full suite, mutation, cross-target or production-fix claim is made.
The coordinator owns the read-only main story and the next scope/fix decision.

### Same-producer plain-PTY control — corrected help comparison

The coordinator requested a bounded follow-up because replaying captured bytes
removes ego from the baseline. The 36×/49× ratios above are valid descriptions
of that byte-replay comparison, **not TUIC overhead or a valid same-producer
2× verdict**. This section supersedes the earlier help-target failure verdict.
It does not supersede the measured one-frame echo failure.

The installed ego was launched directly by Python `subprocess.Popen` into a
15×38 `pty.openpty` pair, with a controlling terminal. Its argv contained only
`/Users/stefano.straus/.local/bin/ego`, the same as the native acquisition. It
used the same default config, observed model `openai-codex/gpt-6-sol`, and cwd
`~/Gits/.tmp/2040-web/native`. EGO_HOME and inherited TUIC variables were removed.
No TUIC process, IPC or canvas was in this control. The coordinator explicitly
authorized normal ego runtime session/history persistence, but no manual config,
credential or models edits. None of those stores was read or edited by the
helper.

A cursor-only VT responder answered DSR requests from the actual output cursor
position; no DSR was intentionally left to time out. This is a plain PTY with a
small terminal-protocol responder, not a full terminal renderer. It is not a
physical terminal or compositor baseline. Startup and a quiet settling period
were outside the timing window. The helper typed only `/help`, pacing letters
25 ms apart, checked the full displayed draft after removing SGR/CSI escapes,
and only then sent Enter. It waited for the final help divider and a 250 ms
quiet period to identify the last read; that quiet period is **not included**
in the durations. A two-command calibration preceded the 20-command main run.
No prompt text other than `/help` was submitted; the successful raw logs contain
zero `Working` chunks.

#### Body identity and endpoints

Independently scheduled composer/status paints can occur between help chunks.
Consequently, the total raw output is not byte-identical to the original
7,148-byte native capture: the main trials contain 7,281 or 7,414 bytes. The
helper retained these complete raw bytes, their hashes and read timestamps.

For a strict **help-body** check, it selected the help printer's exact records
(`CR + CSI K + wrapped text + CRLF`) from `εgo: εgo commands` through the final
34-character divider, excluding independently interleaved composer/status paint
sequences. This is an explicit framing extraction, not whitespace or content
normalization. Every main trial's selected records equal the original native
capture's selected records byte-for-byte: **6,792 bytes**, SHA-256:

```text
78992e537b2435f84ac7a6f56b1062e33e15de137bfca9339b93b1ce7185d051
```

| Plain installed ego, N=20 | Median | p95 | Maximum |
| --- | ---: | ---: | ---: |
| Enter → last help byte (final divider read) | **205.189 ms** | **306.988 ms** | **307.472 ms** |
| Enter → last PTY read, including final composer | 205.486 ms | 307.244 ms | 307.549 ms |

The help-byte minimum was 202.383 ms; the all-output minimum was 202.670 ms.
Against the **205.189 ms plain-producer help-byte median**:

| Native endpoint from initial captured command | Native time | Native − plain median | Native / plain median |
| --- | ---: | ---: | ---: |
| Enter → last native PTY read | 103.946 ms | **−101.243 ms** | **0.507×** |
| Enter → final native canvas paint_end | 142 ms | **−63.189 ms** | **0.692×** |

Using the plain all-output median instead gives −101.540/−63.486 ms and
0.506×/0.691× respectively. **Both comparisons are within 2× in this control.**
The negative differences mean the sampled native command completed sooner than
the plain control median; they are not a physical negative processing overhead
or proof that TUIC accelerates ego. One native command versus 20 later plain
commands is not a paired estimate of stable application overhead. The paint
comparison still includes a native renderer endpoint absent from the plain PTY.
Terminal-response implementation and unpaired timing are remaining confounders.
No TUIC-only cause or statistical non-regression is inferred.

Two helper attempts failed before the successful calibration: the first did not
recognize `/help` across SGR sequences and correctly withheld Enter; the second
caught interleaved status paint in its initial contiguous-body comparison. Its
raw evidence led to the explicit help-record extraction above. Neither failed
attempt submitted a provider prompt. All four recorded ego PIDs (two failed
attempts, calibration and main) were terminated and reaped; SIGTERM did not stop
the interactive loop within three seconds, so the helper killed only its own
recorded child. Final PID checks found all four absent.

Evidence under `~/Gits/.tmp/2040-web/native/`: `plain-producer.py`,
`plain-producer-2.json`, `plain-producer-20.json`,
`plain-producer-analysis.json`, `plain-producer-20-bg.log`,
`plain-producer-draft-guard.json` and `plain-producer-interleaved-status.json`.
The main background job exited 0; its ego child exit was −9 after owned cleanup.
No application build or suite was run for this documentation follow-up.

Criterion 3 stays unchecked: its help portion is within 2× against the requested
same-producer control, but its one-frame echo portion is still unmet. Criteria
4/24 and throughput/heartbeat non-regression remain unsupported. The coordinator
owns all story reconciliation; this peer changed no criteria.

### Same-producer echo control — residual still exceeds one frame

The coordinator requested the corresponding echo baseline, since native
keydown-to-fill also includes ego's echo production. The same installed-ego
plain-PTY harness and launch contract above were used, with a cursor-only DSR
responder and no TUIC. The main run verified terminal local flags 65: both
`ECHO` and `ICANON` were off. Thus the returned unique key is not kernel input
echo. Only `/help` was submitted; CJK probe drafts were cleared with Ctrl+U,
never Enter. Raw output contains zero `Working` chunks.

A two-idle/two-help calibration preceded 200 idle keys and 200 help candidates.
The native input schedule was retained: one outstanding unique key, 22 ms after
echo, Ctrl+U and 30 ms idle delay; help letters 25 ms apart, a verified full
`/help` draft before Enter, and a key 125–129 ms after Enter, then clear and
220 ms delay. The plain harness waits for echo bytes rather than a canvas fill,
so total iteration duration is not identical to the native renderer workload.

All 200 idle probes echoed. Of 200 help candidates, **177** had a real help
header before the key and the echoed key at or before the final footer, all on
the same plain-harness monotonic clock. Header precedence was at least 17.181 ms.
The other **23 candidates are excluded** from output-overlap statistics; they
are not relabelled as overlapping. Native overlap statistics still use the
previous 201 verified keys, not these 177 different-run keys.

#### Requested producer subtraction

Milliseconds, median / p95 / maximum:

| Measurement | Idle | Verified help overlap |
| --- | ---: | ---: |
| Plain ego key write → echoed key byte read | **0.125 / 2.084 / 6.330** (N=200) | **0.093 / 2.994 / 11.365** (N=177) |
| Native keydown → glyph fill | 20.5 / 42 / 58 (N=200) | 11 / 26 / 36 (N=201) |
| Native statistic − corresponding plain statistic | **20.375 / 39.916 / 51.670** | **10.907 / 23.006 / 24.635** |
| Native semantic acceptance → actual rAF | **6 / 16 / 18** | **8 / 16 / 17** |

The subtraction row is the requested comparative overhead estimate, **not a
measured distribution of per-key overhead**. In particular, subtracting two
p95 values does not produce the p95 of their difference, and subtracting two
maxima does not produce a worst-case overhead. These are separate runs with
different input paths and terminal-response implementations. They do show that
the sampled plain producer does not consume anything close to the native
42/26 ms p95 key-to-fill times.

The accepted-to-rAF p95 is 16 ms for both native arms, just below a 16.7 ms
frame, but its maxima are 18/17 ms. More importantly, this starts **after grid
acceptance** and cannot establish that TUIC's whole output share is one frame.

#### Paired native output boundary

The existing native timeline also permits a stronger within-run measurement:
for each key, subtract its actual native PTY read timestamp from its glyph-fill
timestamp. Ego has already emitted the echo at that read boundary. This
includes subsequent native output processing, grid acceptance and paint wait,
without subtracting a different run's producer distribution:

| Actual native PTY read → matching glyph fill | Median | p95 | Maximum |
| --- | ---: | ---: | ---: |
| Idle, N=200 | **19.2 ms** | **40.8 ms** | **56.3 ms** |
| Verified help overlap, N=201 | **9.1 ms** | **23.7 ms** | **33.9 ms** |

These cross-clock values retain the earlier approximately ±2 ms uncertainty;
small negative minima are clock artifacts. The p95 excess over 16.7 ms is much
larger than that uncertainty in both arms. **The measured TUIC output-side
residual does not stay under one 16.7 ms frame at p95**, even after the echo has
reached its PTY reader. It is therefore not valid to declare the full echo
criterion passed merely from accepted-to-rAF p95 or from removing producer
cost. This does not identify the remaining reader-to-grid cause or authorize a
speculative scheduling fix.

`plain-echo.py`, `plain-echo-2.json`, `plain-echo-200.json`,
`plain-echo-200-bg.log`, `analyze-echo.py` and `plain-echo-analysis.json` under
`~/Gits/.tmp/2040-web/native/` retain source, every raw read in hex, samples,
terminal flags, overlap decisions and reproducible distributions. Analyzer
assertions verified counts, disabled kernel echo/canonical mode, absent
`Working` chunks and cleanup. Both recorded ego PIDs are absent after owned
terminate/kill/wait cleanup. The main background wrapper exited 0; the child
exited −9 after cleanup. No production source, story criterion, manual default
config or credential file was changed; no build or application suite ran.

The same-producer help comparison remains within 2×. The echo half of criterion
3 remains unsupported as a pass and exceeds one frame in these measurements.
The coordinator owns criterion reconciliation and any next investigation.


## Native reader-to-acceptance investigation and receipt feedback fix (2026-10-23)

### Contract and scope

A desktop receipt releases flow control but must not generate terminal damage.
A caught-up desktop receives one complete recovery snapshot; browsers retain the
matching ordinary delta. Real damage and recovery debt must survive a paused
channel. The production state comes from the real PTY reader, Alacritty damage,
the shared frame ticker and the counted desktop gate. The installed default ego
and native WebKit remain observed external systems, not hand-written fixtures.

The retained fix removes two proven feedback paths, not the deliberate 33/50 ms
rate floors or two-bypasses-per-100-ms input budget. No story criterion changed.
The one-frame echo requirement remains unmet. The pacing site contains an inline
dated deferral for that remaining policy trade-off.

### Stage localization

A temporary observer traced read completion, VT parsing, reader completion,
ticker decision, VT-lock acquisition, serialization, IPC send, frontend receipt,
semantic acceptance and actual rAF. All observers were removed after acquisition.

The first stage arm used 20 idle and 20 help candidates. Its idle reader-complete
to frame-cut median/p95/max was **0.054/21.718/30.910 ms**. This was not a fixed
13 ms parse or IPC cost. The refined pre-fix arm measured **0.068/34.846/37.628
ms** at that same boundary. All 20 idle keys correlated through every stage.
The largest observed VT-lock wait in that refinement was 1.366 ms and the largest
serialization interval was 0.072 ms. They do not explain its slow tail.

Final stage table, milliseconds, median / p95 / maximum:

| Boundary | Idle, N=200 | Help candidates, N=200 unless noted |
| --- | ---: | ---: |
| Reader → VT parse complete | 0.052 / 0.081 / 0.123 | 0.049 / 0.072 / 0.187 |
| VT parse complete → reader complete | 0.049 / 0.072 / 0.117 | 0.045 / 0.066 / 0.085 |
| Reader complete → ticker frame cut | **0.062 / 20.026 / 38.767** | **13.882 / 23.664 / 24.157** (N=199) |
| Frame cut → IPC begin | 0.010 / 0.013 / 0.087 | 0.010 / 0.014 / 0.077 (N=199) |
| IPC begin → end | 0.018 / 0.030 / 0.116 | 0.020 / 0.033 / 0.136 |
| Frontend receipt → semantic acceptance | 0 / 1 / 1 | 0 / 1 / 1 |
| Acceptance → actual rAF | 7 / 17 / 17 | 6 / 16 / 17 |

The help stage table includes candidates rather than claiming every key overlaps
output. One final help key has no ticker cut but does have IPC and frontend
correlations; another frame producer can serialize outside this observer seam.
It remains unclassified, not silently dropped. IPC-end to frontend-receipt values
can be slightly negative because the clocks retain approximately ±2 ms error;
no physical negative latency or sub-millisecond transport attribution is claimed.

Production seams: `ChunkProcessor::process_chunk` applies VT data; the reader
marks applied output; `spawn_reader_thread` gates and paces the shared ticker
before taking the VT lock; `send_grid_frame_to_consumers` publishes ordered
browser deltas and admits desktop delivery; `ack_terminal_frame_core` releases
the counted gate. The measured slow stage is ticker admission, not decode.

### Proven defects and rejected hypotheses

1. **Empty-frame bookkeeping was not the measured mechanism.** The refined
   pre-fix arm recorded 533 ticker cuts and zero empty cuts. Alacritty's
   `Term::damage()` always includes the current cursor, so nominally unchanged
   serializations can still contain a 43-byte cursor span. No empty-frame quota
   patch was retained.
2. **A receipt manufactured new output.** The ACK command marked `frame_dirty`
   after receipt progress. A subsequent serialization included the cursor and
   sent another frame, whose receipt repeated the cycle. This could keep a quiet
   terminal in sustained-output mode. ACKs now only release the gate. Paused
   ticks retain actual pending damage instead of depending on invented ACK damage.
3. **Recovery recreated its own debt.** An ACK-only trial exposed a second path:
   the ticker sent a private full recovery frame, then offered its matching delta
   to the now-closed desktop gate. That delta was withheld and marked missed
   again. The trial captured **1095** private-6356-byte-repair/43-byte-delta cycles,
   including cycles long after reader output stopped. The final path preserves
   private recovery but publishes that matching delta only to browser consumers.
   The same sequence count is **zero** in the final acquisition. No quota,
   timer, wire format, additional send thread or persistent state was added.

A trial that broadcast the recovery full frame to all clients also broke the
cycle, but was not retained: the existing browser-delta regression explicitly
protects browsers from desktop recovery overship. The final fix preserves that
contract and its unchanged assertion. The ACK-only trial was not an echo win.

### Native echo remeasurement

Same authorized `agent-test-2040` instance/port, custom CanvasTerminal, default
installed ego with no argv flags, synthetic native keydown events, guarded
`/help` submission, unique CJK drafts and Ctrl+U clearing. No OS keyboard or
physical-presentation claim. The final main arm contains 200 idle keys and 200
help candidates. **196** main help keys and all **3** smoke help keys have a
same-reader-clock help header before the key and an echo before the final help
footer. The other four main candidates are excluded from overlap statistics.

Milliseconds, median / p95 / maximum:

| Measurement | Earlier pre-fix control | Final fix |
| --- | ---: | ---: |
| Idle key → glyph fill, N=200 each | 20.5 / 42 / 58 | **16 / 34 / 51** |
| Idle reader → glyph fill, N=200 each | 19.179 / 40.838 / 56.301 | **14.164 / 32.232 / 48.621** |
| Verified-help key → glyph fill | 11 / 26 / 36 (N=201) | **23 / 39 / 41** (N=199) |
| Verified-help reader → glyph fill | 9.129 / 23.746 / 33.878 | **20.807 / 37.065 / 38.930** |
| Verified-help key → reader | 1.836 / 2.493 / 2.871 | 2.357 / 2.938 / 3.245 |
| Verified-help Enter → footer reader | 170.055 / 183.572 / 199.453 | 182.756 / 200.147 / 207.549 |

The more closely instrumented 20-key pre-fix arm had idle key-to-fill p95 45 ms
and verified-help p95 37 ms (N=22, including smoke). Workloads and unrelated
machine load are not paired or controlled; the large help-arm variation prevents
claiming a general echo speedup. Final p95 is above one frame in both arms and
above the separate under-33-ms visible-input target. The two feedback bugs are
fixed, but they were not the entire remaining echo delay. With priority credits
spent, the existing floor and gate can still admit reader-complete output later.
All final keys painted on the first strictly subsequent sentinel-observed rAF.

### Matching paced bulk and heartbeat control

The native control emits 12 UTF-8 rows per batch, sleeps 100 ms between batches,
and ends with an accepted `BULK-END-2040` marker. First run 20 batches, then 120;
only batch count changes. All output bytes match the known producer stream
exactly before and after, including the PTY's CRLF translation. The 120-batch
body is **181474 bytes / 1440 rows** in both runs, SHA-256
`b85c17e2ff4daf1a919b760eb2d114622e4a0a108c320ae94ef6c516c203a20b`.

| Control | Before | Final |
| --- | ---: | ---: |
| 20-batch reader throughput | 14070.628 B/s | 14013.341 B/s |
| 120-batch reader throughput | **13988.454 B/s** | **14006.104 B/s** |
| 120-batch final reader → marker acceptance | 25.237 ms | 20.402 ms |
| 120-batch rAF interval p95 / maximum | 18 / 21 ms | 18 / 27 ms |
| 120-batch 100-ms timer p95 / maximum | 106 / 106 ms | 105 / 106 ms |
| 20-batch rAF interval p95 / maximum | 19 / 22 ms | 18 / 24 ms |
| 20-batch 100-ms timer p95 / maximum | 106 / 111 ms | 105 / 106 ms |

This preserves the measured paced workload's bytes and rate, with unchanged main
rAF p95 and timer maximum. The rAF maximum increased by 6 ms in the main control;
it is not a claim that every heartbeat statistic improved. This is a paced
approximately 14 KB/s workload, not an unbounded saturation benchmark or proof of
all throughput/heartbeat acceptance criteria. The first marker can be coalesced
out of the visible viewport; throughput therefore uses actual first/final reader
timestamps, not an absent first-acceptance timestamp. After each measured footer,
owned session deletion interrupted the sleeping producer and logged a 173-byte
`KeyboardInterrupt` traceback. It is retained and excluded after the measured
footer; assertions validate both the measured body and that cleanup suffix.

### Tests, failed validation and remaining gates

New regressions drive the real reader/ticker/channel and the ACK core called by
the production command, not a copied policy model:

- `desktop_receipts_do_not_generate_output`: ordinary receipts and real withheld
  browser-delivered damage must produce no extra quiet frames; subsequent real
  echo must still arrive. The table also catches recovery recreating its own debt.
- `paused_desktop_retains_reader_output_until_late_receipt`: three actually sent
  and abandoned frames pause the real gate; newly read output remains withheld,
  then a late real receipt releases it without new PTY bytes.

The targeted filter also covers unchanged input arming, closed-gate protection,
DEC-2026 buffering, echo priority, stalled-desktop/browser independence and private
full recovery/browser delta. Native **8/8 PASS**, nextest run
`5087faa1-7e4e-49df-a86d-96bd9d2b754b`, 1.327 s test time after a 1m47s build.
No application test noise occurred; the linker warned that `__eh_frame` exceeded
its compact-unwind size limit. The command used `build-slot.sh`, mbx and
`scripts/with-test-tmp.sh` with repository-local TMPDIR and default features.

Earlier validation attempts are not hidden: rb exited **78** before tests because
this command path requires native macOS coverage first; the first native compile
exited **101** because Tauri's optional test module is not enabled. No dependency
or feature override was added. Tests instead call the extracted production ACK
core; the corrected native targeted run passed. No cross-target or full-suite
result, clippy, mutation or critic pass is claimed.

**RED is not run.** The latest admitted adversarial-TDD instructions forbid
in-story RED and require coordinator after-the-fact RED. The requested earlier
RED-first evidence therefore remains unavailable. Pre-fix revision for the
coordinator is `118ba8df8ef1b0a7f422beca1f92fb963b285d4a`; replay the new quiet
receipt/recovery regression against that behavior (the shared core adapter may
need the mechanical test-port when replaying). Do not treat compilation failure
as behavioral RED. A separate critic is still required for this concurrency fix.

Evidence under `~/Gits/.tmp/2040-web/`: `native-stages/` (first localization),
`native-empty-stages/` (empty-cut discriminator), `native-bulk-before/`,
`native-ack-fixed/` (failed ACK-only trial), `native-unified-fixed/` (rejected
shared-full trial), and **`native-final-fixed/`** (retained implementation).
Each retains helper source, raw logs, JSON acquisition and observer patch.
`compare-receipt-fix.py` and `receipt-fix-comparison.json` reproduce exact byte,
throughput, stage and heartbeat comparisons; final `analyze.py` verifies every
reader/acceptance/paint correlation and records overlap exclusions. No synthetic
observer is retained in production. Source restoration was verified; recorded
supervisors and ego PIDs are absent, and the instance desktop PID file is absent.
Only the named isolated instance was stopped; the daily backend was not restarted.
This Rust change requires Boss's manual rebuild/restart to reach that backend.

**Result: proven receipt/recovery defects fixed; the overall latency assignment
and story acceptance remain incomplete.** Idle echo improved in this control,
help-overlap echo did not. Criteria 3, 4 and 24 must not be marked passed from
this evidence. The main story store remains untouched and the coordinator owns
criterion reconciliation, after-the-fact RED and the separate critic.


## Admission follow-up contract (2026-10-23)

The consumer must receive already-applied post-input damage through the existing
shared ticker, including damage that races an earlier frame send. The reader's
`InputFramePriority::applied` generation and the ticker's real serialization/send
accounting are the source of pending state. Native observations are recorded from
the installed ego and the isolated custom CanvasTerminal; synthetic key input and
fill callbacks do not prove physical keyboard-to-display latency.

The main story store is read-only to this peer. This section records the contract
for the coordinator's worklog without changing criteria. Keep the two bypasses per
100 ms, 8 ms spacing, 100 ms expiry, desktop gate, DEC-2026 and bulk floors intact.

### Exact idle admission rule, not a generic timer hypothesis

The follow-up starts at merged `main` `554b1fa02`, which includes the receipt fix.
Coordinator mail reports critic PASS and behavioral RED for
`desktop_receipts_do_not_generate_output` on `118ba8df`; the paused-damage test
passes that baseline and is a guard, not historical RED. Those are coordinator
results, not new peer runs.

The original final acquisition can be explained without another native run.
`analyze-admission.py` replays the recorded `accounting` events to reconstruct
`last_sent` and the two rolling credit timestamps. It validates 3,357 eligible
production decisions outside a 0.25 ms log-overhead boundary; all agree.

- `pty/frame_priority.rs:32-46`: a pending request cannot precede either the last
  send plus 8 ms or the oldest retained bypass plus 100 ms. A new input extends
  expiry but does not clear either credit (`:55-59`, `:111-114`).
- `pty/frame_priority.rs:75-87`: `applied` does not unpark when rolling quota is
  unavailable. With no wake, `pty.rs:3761,3780-3790` falls back to the 16 ms
  maintenance park. This is a timeout request, not an OS scheduling guarantee.
- `pty.rs:3970-3986`: periodic output can still send when its ordinary floor has
  elapsed, even if priority quota is unavailable. It need not wait for a credit.

Original idle N=200, reader-complete to first matching cut, ms median/p95/max:

| Group | N | Median | p95 | Max |
|---|---:|---:|---:|---:|
| All | 200 | 0.062 | 20.026 | 38.767 |
| Rolling quota unavailable at reader completion | 62 | 11.230 | 23.546 | 38.767 |
| Rolling quota available | 138 | 0.053 | 0.090 | 0.162 |
| No matched gate or floor deferral | 195 | 0.061 | 19.400 | 23.546 |

These groups are not independent causal classes: a quota-blocked sample can also
hit a floor. Only five idle samples have an observed floor deferral and none has
an observed gate deferral. The 33 ms animation floor is not the primary cause of
this p95. Repeated unique glyphs and Ctrl+U clears both produce real post-input
frames and spend the same rolling budget.

The p95 sample, `亲`, is decisive. In the recorded frontend-origin timeline,
prior bypasses occurred at 22292.989 and 22324.822 ms. Reader completion was
22359.316 ms. The oldest credit cannot return until 22392.989 ms: 33.673 ms later.
Spacing is already satisfied. No early wake is eligible. A periodic decision cuts
at 22379.342 ms, after 20.026 ms, with `min_interval=0`, `floor_pending=false`,
`priority_due=false`, `periodic_tick=true`; the ordinary frame beats credit return.
This is the existing two-per-100-ms policy plus maintenance scheduling, not time
spent in parsing, serialization or IPC.

### Separate send-race correction within the same bounds

The same trace also shows a different help-output case. For `侳`, the first echo
is applied at UTC `01:35:53.241687`. A concurrent help chunk removes it before the
first cut. That newer chunk completes at `.241909`, before IPC ends at `.241937`;
a repaint with the echo completes at `.242008`. The matching echo cut is not until
`.265844`. The intermediate request was already pending, so `applied` coalesced
its wake. `sent` preserved the newer output count, but the ticker retained no
retry after sending. There was no floor at the matching cut and quota was free.
This is not evidence that the first transient echo remained continuously on screen.

The bounded correction changes only the existing send-accounting result:
`InputFramePriority::sent` returns the remaining request's deadline after quota
accounting, and `pty.rs:4020` retains it in the existing `priority_retry` slot.
No timer, thread, state machine, credit, expiration, floor, transport, or gate is
added or relaxed. When no newer output raced the send, the result remains `None`.
A closed gate is still handled before admission; DEC-2026 buffering is unchanged.

The concrete worst case remains two bypasses per rolling 100 ms, at least 8 ms
after the previous send. If the next credit would return at or after expiry, no
retry is retained. The new table regression covers free spacing, a new input with
spent credits, and insufficient remaining expiry. The existing newer-damage
regression now also checks the returned deadline and retirement after delivery.
No old expectation is inverted or weakened.

### Follow-up native run and limits

Evidence: `~/Gits/.tmp/2040-web/native-retry-fixed/`. Same installed ego path,
no argv flags or manual config edits; default model `openai-codex/gpt-6-sol`,
version banner `ego 0.1.2`. The same helper acquires 3+3 smoke and 200+200 main
idle/help samples in one ego session, then 20/120-batch bulk controls. Only `/help`
is submitted; unique glyph drafts are cleared without Enter. All 400 main keys
correlate reader, cut, IPC and acceptance. Verified overlap is 184 main plus two
smoke help samples; 16 main candidates are excluded. No Working chunks appear.

The requested merge also brought frontend changes. Although the window stays
640x420 CSS, DPR 2 and the same WebKit UA, the fitted grid is now **14x37**, not the
prior **15x38**. Help wraps differently. System-load conditions are not controlled:
matched idle floor deferrals rise from 5 to 56. Therefore the before/after numbers
below are measurements, not a clean causal A/B or proof that the patch improves
all latency populations. No second experiment is invented to hide these limits.

Ms median/p95/max (before = prior receipt-fixed acquisition):

| Boundary | Before | Follow-up |
|---|---:|---:|
| Idle key to fill, N=200 each | 16 / 34 / 51 | 17 / 42 / 60 |
| Idle reader-complete to cut | 0.062 / 20.026 / 38.767 | 4.061 / 35.820 / 46.499 |
| Help-candidate reader-complete to cut | 13.882 / 23.664 / 24.157 (N=199) | 8.668 / 12.037 / 24.127 (N=200) |
| Verified-help key to fill | 23 / 39 / 41 (N=199) | 21 / 28 / 40 (N=186) |
| Verified-help reader to fill | 20.807 / 37.065 / 38.930 | 19.269 / 26.402 / 38.719 |
| Verified-help accepted to rAF | 6 / 16 / 17 | 9 / 15 / 17 |

The follow-up records 336 retained retries. The 1,453 bypass cuts have a minimum
recorded preceding-send interval of 8,006 us; no 99.75 ms log-timestamp window has
more than two. The slightly shorter validation window avoids claiming exact
monotonic boundaries from epoch logging. These observations supplement, not
replace, the deterministic quota/spacing/expiry tests. No empty cuts or private
repair/cursor-delta feedback signature appear (3,212 cuts).

Replayed follow-up admission accounting agrees with 3,341 eligible decisions.
97/200 idle echoes have quota unavailable at reader completion; 56 have matched
floor deferrals and one has a matched gate deferral. Help has no quota-blocked
reader completions, 14 matched gate deferrals and one matched floor deferral.
Help samples without either matched deferral cut at p95 12.016 ms, max 12.058 ms.
The remaining idle policy tail is not fixed. Both overall one-frame and combined
under-33-ms p95 requirements remain unmet.

The exact 120-batch body remains 181,474 bytes, SHA-256
`b85c17e2ff4daf1a919b760eb2d114622e4a0a108c320ae94ef6c516c203a20b`.
The 20-batch body remains 30,274 bytes with its previous hash. Reader sums through
the footer match both exactly; the validated 173-byte interruption suffix occurs
only during owned cleanup and is excluded after that boundary.

| Paced control | Before | Follow-up |
|---|---:|---:|
| Main reader B/s | 14006.104 | 13963.171 |
| Main reader span, ms | 12956.780 | 12996.618 |
| Main final-reader to end acceptance, ms | 20.402 | 17.242 |
| Main rAF p95/max, ms | 18 / 27 | 18 / 21 |
| Main 100 ms timer p95/max, ms | 105 / 106 | 105 / 106 |
| Calibration rAF p95/max, ms | 18 / 24 | 18 / 47 |
| Calibration timer p95/max, ms | 105 / 106 | 105 / 105 |

Bytes are lossless and the paced rate remains about 14 KB/s, but the measured main
rate is **0.307% lower**, not identical. Calibration rAF max is worse. These are not
proof of zero throughput loss, all-statistic heartbeat non-regression, saturation
capacity, or unchanged CPU cost. The producer sleeps between batches; its reader
span includes scheduling drift. Criteria 3, 4 and 24 remain untouched and unproved.

### Policy choices, not changes made here

1. Keep two bypasses per 100 ms: the long-run priority allowance stays 20/s;
   repeated key/clear traffic can exhaust it even with an open gate and no floor.
   This follow-up preserves that policy.
2. Raise the allowance to three or four per 100 ms: 30/s (+50%) or 40/s (+100%)
   priority allowance. This is a policy change, not a repair of lost pending work;
   continuous output can consume these credits too. No native trial was run.
3. Exempt every fresh input from quota while retaining 8 ms spacing: a theoretical
   125/s priority ceiling instead of a 20/s long-run allowance. This violates the
   existing rolling-budget contract and needs throughput/CPU/heartbeat evidence.
4. Lower the 50 ms saturation floor to the 16 ms maintenance interval: nominal
   background send cap rises from 20/s to 62.5/s (3.125x). Lowering 33 ms to 16 ms
   changes about 30.3/s to 62.5/s. Actual park scheduling can exceed its requested
   timeout; these are arithmetic policy ceilings, not benchmark results.

None of these alone guarantees key-to-fill within 16.7 ms: even a quota-eligible
frame can wait 8 ms after the preceding send and then up to a display frame for
frontend rAF. No policy change is made. Recommendation: retain the bounded
send-race correction for independent review, but do not call the story complete
or certify throughput/heartbeat criteria from this run.

### Validation and ownership

One final native targeted nextest run: **13/13 PASS**, run ID
`f49c30d7-9e71-48f6-bd75-05ece8ce0c04`, 1.337 s tests after 3m52s build.
Command uses build-slot, mbx, repository test-temp wrapper and only the five
frame-priority tests plus the eight prior grid/priority regressions. The new table
was then changed only to collect all case failures before asserting, rather than
stop at the first failing case. Only that invalidated test was rerun: **1/1 PASS**,
run ID `db696403-c6c3-4226-9a08-f7eb3142403b`, 0.037 s after 1m45s build;
`retry-test-table-final.log` retains the command and result. No production code
changed after the 13-test run. Log:
`~/Gits/.tmp/2040-web/retry-tests-native.log`. The linker warns about the large
`__eh_frame` compact-unwind table; no test stdout/stderr noise. mbx reports cache
bypass diagnostics and automatic disk-pressure GC; no cache was manually cleared.

No in-story RED, full suite, clippy, mutation or cross-target run is claimed for
this follow-up. New pre-fix behavior is `554b1fa02`. For after-the-fact RED, the
coordinator must port the old `sent` signature mechanically to return `None`
without scheduling a retry; compilation failure is not behavioral RED. The
modified newer-damage test protects the lost deadline; the table protects bounds.
An independent concurrency critic must still check the ticker integration.

The native helper's initial observer preparation failed on a changed frontend
anchor before any build or acquisition; the exact current source was read and
the temporary observer anchor corrected. The lifecycle then completed normally:
helper 41679, supervisor 41686 and ego 64779 are absent, stop returns 0, supervisor
exits 2 after sanctioned shutdown, and the desktop PID file is absent. Temporary
observers are restored; the only retained Rust behavior change is bounded retry
retention. The daily app was not restarted. Boss must manually rebuild/restart
when ready to load this Rust change.

Analysis reproducers: `analyze-admission.py`, `analyze-retry-admission.py`,
`compare-retry-fix.py`, and their JSON outputs under `~/Gits/.tmp/2040-web/`.
The production story store remains read-only to this peer.

Static checks before commit: changed-file rustfmt check, `git diff --check`, and
instruction-link validation pass (two pairs). `story-close.sh --check` correctly
refuses unchecked criterion 3: help ratio plus echo under one frame while rows
print. No criterion is checked to make that gate pass.


## Controlled A–B–A–B admission follow-up (2026-10-23)

### Contract and comparison boundary

The coordinator requested one fixed-geometry A–B–A–B acquisition and no policy
change. A uses `554b1fa02`; B uses `8b7f24d908f181908b53e5f2b529088bdb10e43a`.
The observable effects are key-to-canvas-fill and reader-complete-to-matching-cut.
The real state comes from native reader, serializer, IPC and binary-grid traces,
plus observed input generations and send accounting. Only `/help` is submitted;
unique glyph drafts are cleared with Ctrl+U. The decision rule is B idle p95 no
worse than A within A's run-to-run spread, with better help p95. Do not substitute
this rule for the original one-frame, under-33-ms or throughput criteria.

Evidence root: `~/Gits/.tmp/2040-web/native-controlled-abab/`. The pre-run
`contract.json` records this boundary. The series uses the same named isolated
`agent-test-2040` instance on port 19890. Native binary replacement requires four
sequential `make agent-test` lifecycles, not one continuously running process.
Each lifecycle launches the same installed ego with no arguments, default home
and configuration, and the same cwd and harness. The banner is `ego 0.1.2`, model
`openai-codex/gpt-6-sol`, in all four runs. This is not a shared ego conversation
across native process restarts.

Only `pty.rs` and `pty/frame_priority.rs` are temporarily selected from A or B;
the frontend and all other runtime source are identical. `series-state.json`
records pre-observer and instrumented hashes. A1/A2 hashes match, B1/B2 hashes
match, and all four frontend hashes match. Identical common observers capture
native stages, credit state, input generations and grid geometry. B additionally
records its retained deadline. Temporary observers are restored after acquisition.
No implementation, credit, floor, gate, timer or state-machine change is retained.

Every smoke and main binary receive header reports **37 columns × 14 rows**.
Every run is **640×420 CSS pixels, DPR 2**, with the same WebKit UA. Each run has
3 idle + 3 help smoke samples, then **200 idle + 200 help** main samples. No extra
trial is selected or discarded. The lock check passes before each lifecycle and
again after its build. Only these owned instances are used; no daily-app restart.

### Per-run results

Times are ms, median / p95 / max. These are synthetic frontend key events and
actual canvas `fillText`, not OS keyboard-to-physical-scanout measurements.

| Run | Idle key→fill, N=200 | Idle reader-complete→cut, N=200 | Help-candidate key→fill, N=200 | Help-candidate reader-complete→cut |
|---|---:|---:|---:|---:|
| A1 | 9.5 / 27 / 36 | 0.065 / 18.320 / 21.998 | 23 / 40 / 42 | 13.033 / 23.745 / 24.009 (N=199) |
| B1 | 16 / 43 / 55 | 0.080 / 31.967 / 41.392 | 20 / 27 / 40 | 8.995 / 12.065 / 24.107 (N=199) |
| A2 | 18 / 43 / 58 | 0.862 / 35.367 / 41.097 | 21 / 38 / 42 | 10.940 / 23.617 / 24.131 (N=200) |
| B2 | 19.5 / 42 / 57 | 6.001 / 35.658 / 40.848 | 20 / 29 / 40 | 9.315 / 18.247 / 24.221 (N=199) |

Verified main help overlaps, excluding non-overlaps rather than relabeling them:

| Run | Verified N | Key→fill | Reader-complete→cut |
|---|---:|---:|---:|
| A1 | 189 | 23 / 40 / 42 | 14.425 / 23.712 / 24.009 (N=188) |
| B1 | 195 | 20 / 27 / 40 | 9.058 / 12.065 / 24.107 (N=194) |
| A2 | 185 | 21 / 38 / 42 | 11.350 / 23.653 / 24.131 (N=185) |
| B2 | 190 | 20 / 29 / 40 | 9.366 / 12.200 / 24.221 (N=189) |

All 1,600 main samples have reader, IPC, frontend acceptance and fill correlations.
Three help candidates have no matching shared `frame_cut` observer: A1 `侙`,
B1 `伦`, B2 `佯`. Their IPC and fill observations remain present; their missing
cut stage is not assigned an invented timestamp. Cut distributions explicitly
exclude them. No Working chunks appear. Counts of main samples with a strictly
intervening sentinel rAF are A1/B1/A2/B2 = 0/1/2/3; do not reuse the previous run's
zero-intervening-rAF claim. Cross-clock uncertainty remains about ±2 ms; epoch
accounting replay uses a 0.25 ms classification tolerance.

### Quota provenance and the remaining load confound

| Main idle observation | A1 | B1 | A2 | B2 |
|---|---:|---:|---:|---:|
| Quota blocked at reader completion, /200 | 59 | 82 | 94 | 97 |
| Quota blocked at actual input arm, /200 | 63 | 82 | 97 | 101 |
| Matched floor deferrals | 0 | 53 | 59 | 61 |
| Matched gate deferrals | 0 | 0 | 1 | 0 |
| Accounted sends in idle block plus 55 ms tail | 400 | 388 | 389 | 393 |
| Sends with min_interval=50 ms | 0 | 338 | 389 | 393 |
| Sends retaining newer pending output | 0 | 0 | 0 | 0 |
| Retained-retry records | 0 | 0 | 0 | 0 |

Arm counts join the observed reader's `input_generation` to the actual `arm`
record and its monotonic credit ages; they are not hand-set state. Reader counts
replay real bypass accounting. Eligible decision checks all agree: A1 3,292,
B1 3,335, A2 3,410, B2 3,392.

The two B idle blocks have **no retained retries at all**, including their 55 ms
cleanup tails, and no sends that preserve newer pending output. None of their
quota-blocked idle echoes has a retained-retry bypass in its two occupied credit
slots. The proposed mechanism—retained retries consume the next idle key's
credit—is therefore **not observed in these runs**. This does not prove that
such credit competition is impossible in every workload.

Retained retries do spend bounded bypass credits during help: a conservative
provenance match identifies 125 B1 and 136 B2 nonperiodic bypasses after matured
retained deadlines, with no intervening decision or input generation. The B main
help intervals contain 367/369 retained-retry records. All four help populations
have zero quota-blocked echo reader completions and zero blocked echo input arms.
Thus no measured next help key needed a credit that those retained retries spent.
This is credit occupancy evidence, not a fully simulated counterfactual schedule.

**Do not attribute the earlier 34→42 ms idle p95 change to grid geometry.** The
old comparison was confounded. This fixed-grid series itself has A idle p95
27→43 ms without the retained-retry change. The runtime's existing saturation
floor is inactive throughout A1 idle and active for most/all of B1/A2/B2 idle.
Those floor observations distinguish the runs; they do not isolate every source
of scheduling variation or prove which process caused machine load. No artificial
load was created, and system load was not held constant.

### Decision and limits

**LANDABLE under the coordinator's stated p95/run-spread rule.** A idle p95 spans
27–43 ms, a 16 ms spread; B is 43/42 ms, within that range. With the saturation
floor active, A2 is 43 ms and B1/B2 are 43/42 ms. Help p95 is consistently better:
A 40/38 ms versus B 27/29 ms, also for verified-overlap subsets. No bounded fix or
policy change is proposed from this evidence, because the suspected idle retry
credit competition is not reproduced.

This is not a universal non-regression certificate. There are only two runs per
binary, machine load is uncontrolled, and common observers perturb timing.
The one-frame and combined-under-33-ms idle requirements still fail. No new bulk,
CPU or all-statistic heartbeat comparison is run; the previous adverse paced
control results remain valid limitations. Criteria 3/4/24 remain untouched and
unproved. Landing is the coordinator's decision, not an action taken by this peer.

Coordinator-reported evidence, not rerun here: `critic-2040r2` passes on `8b7f24d9`
with no production findings, crate 6,128, headless 5,837 and clippy 0. The old
`sent` behavioral replay fails two of five frame-priority tests. This supersedes
the earlier note that the follow-up critic and RED were pending. No additional
unit suite, clippy or mutation run is needed for this measurement-only follow-up.

### Reproduction and lifecycle

Launch through the existing background wrapper, with the mbx shim on PATH:

```sh
python3 ~/Gits/.tmp/2040-web/native-controlled-abab/series.py
```

The helper checks the build freeze and lock state, stages only the compared
source files, uses `make agent-test AGENT_TEST_INSTANCE=agent-test-2040
AGENT_TEST_PORT=19890`, deletes its owned PTY, stops that instance and restores
source. Build times A1/B1/A2/B2: 4m36s / 2m22s / 2m15s / 2m13s. Each mbx doctor
reports zero failures and zero warnings. Whole series duration is 1,411.626 s
(about 23m32s), including builds, not application latency. The initial generated
Python harness had a quoting syntax error caught before launch; it was corrected
before any acquisition. No failed timing run is hidden.

Offline analysis uses each run's retained `analyze.py` and `analyze-stages.py`,
then `analyze-control.py` and `finalize-analysis.py` in the evidence root. They
produce `comparison.json`, `quota-timeline.json`, `quota-sends.json` and stage
summaries. These retain all candidates, exclusions and provenance.

Background helper 93501 and wrapper 93497, supervisors 93568/26202/49391/79048,
and the last observed ego PID 86493 are absent at final inspection. Every PTY
delete completes without a recorded error; each sanctioned stop returns 0 and
its supervisor returns 2. The named desktop PID file is absent. The tree is clean
before this report amendment, with all temporary production observers restored.
The daily app remains untouched and still requires Boss's manual rebuild/restart
to load the Rust candidate if it is landed.
