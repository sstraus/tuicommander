# Isolated stall workload research

This macOS harness generates **synthetic agent-like output**, not output from an
AI model and not the historical workload from story 1638-c94c. It does not run
CPU busy loops. It uses paced Python output, real PTYs, the headless backend,
worktree Vite, and production `CanvasTerminal` capture. It never starts a desktop
application or touches the installed executable.

## Generate and compare

Prerequisites: worktree dependencies, a native worktree `tuic-remote` built with
`--no-default-features`, the existing ab-stealth wrapper and agent-browser.
Build a native exploration binary only through the authorized build-slot/mbx
path. Do not use the protected main checkout's target directory.

Run through build-slot, in the background through the managed runner:

```sh
STALL_SECONDS=30 STALL_SESSIONS=4 STALL_VISIBLE=1 \
  "$HOME/Gits/personal/orchestrator/tools/build/build-slot.sh" \
  node scripts/stall-workload/run.mjs "$HOME/Gits/.tmp/stall-workload-example"
```

The output directory must not exist. Start with three seconds and one session
before increasing the size. Bounds: 1–30 seconds, 1–4 sessions. At each 100 ms
tick, each PTY writes 12 source-like lines with ANSI colors, combining text,
wide characters and status glyphs. One terminal is visible by default; the
others have `display:none`, as in the retained desktop observation. The visible
container is 1878×1260 CSS pixels; production font measurement and resize choose
the actual rows and columns. The report records actual paint geometry and DPR.

`STALL_MAIN` defaults to the local development `master` ref (the evidence label remains `main`). `STALL_PRE_FIX` defaults to
`950715de69`, a pre-fix revision named in the story. That revision predates the
**orphan IPC boundary fix**; it is not an established canvas-root-cause revision.
The harness exports source from both refs without switching branches. It then
replays the same recorded immutable snapshots three times through each exact
renderer/projection pair. Other frontend modules and the backend remain at the
harness revision. **This is a renderer-only comparison, not a whole-revision
application comparison.** Paint times exclude decoding and font loading.

Outputs:

- `frame-workload.json`: actual production recorder snapshots and arrivals from
  the generated workload. `source:live` means that the recorder saw live frames;
  it does **not** mean that an AI agent or historical stall produced them.
- `provenance.json`: pinned refs, generator hash, recording hash and scope.
- `report.json`: raw paint times, 100 ms event-loop sentinel gaps, rAF gaps and
  independently timed backend `/sessions` requests, with wall timestamps.
- `captures/*.tcap`: actual PTY captures, capped at 512 KiB each. Keep these
  outside the committed parser corpus unless you also regenerate their oracles.
- Backend and browser transcripts. The named browser tab/daemon, PTYs, Vite and
  backend are stopped in `finally`, including on SIGINT or SIGTERM. The first
  signal cancels pending work and sets exit status 130 or 143; later signals
  leave that same teardown running. The shared Chrome process is not stopped.
  SIGKILL cannot run cleanup.

Cancellation regression (no browser, desktop app or generated workload):

```sh
"$HOME/Gits/personal/orchestrator/tools/build/build-slot.sh" \
  scripts/with-test-tmp.sh node --test scripts/stall-workload/run.test.mjs
```

The test starts the actual runner with a small child that holds its health
request open. It checks the child PID is gone, the teardown completed once, and
signal exit status is retained, including repeated signals during teardown.

Summarize the raw data with nearest-rank quantiles:

```sh
node scripts/stall-workload/summarize.mjs "$HOME/Gits/.tmp/stall-workload-example/report.json"
```

The recorder is intentionally lossy. Its 4 MiB budget can retain much less than
one second for this geometry. Repeating a retained suffix does not reconstruct
the omitted output. A full-repaint snapshot can be self-contained; the first
incremental snapshot need not include the prior pixels. The existing replay
limitations in `docs/backend/stall-journal.md` still apply.

The browser's sentinel is **not** desktop `frontend_heartbeat`, Tauri delivery or
input continuity. Dump encoding and revision-module loading are separate phases
from active output. A timer gap during dump must not be attributed to terminal
painting. No threshold in this harness marks story RED/GREEN.

The headless remote router does not expose `/debug/frame-workload`; the harness
saves recorder dumps itself. This does not validate the backend writer route.
The backend gets a Gits-local HOME, config and temporary directory. No production
configuration, credentials or repositories are copied.

## Later WKWebView paint replay

After story 2086 lands, start its isolated `make agent-test` instance through the
approved workflow. The following single command replays a saved recording in
that already-running WebView:

```sh
node scripts/stall-workload/desktop-replay.mjs \
  "$HOME/Gits/.tmp/stall-workload-example/frame-workload.json" \
  "$HOME/Gits/.tmp/stall-workload-example/wkwebview-report.json" agent-test 19876 \
  "<socket field from the agent-test dry-run JSON>"
```

The optional final argument selects the named instance's Unix socket for control
and independent backend probes. TCP can require Basic authentication even on
loopback; do not read token files or log in for this harness. Obtain the socket
from `make agent-test AGENT_TEST_DRY_RUN=1` with the same instance and port
overrides used at launch. The helper was executed on native WKWebView on
2026-10-20; see `DESKTOP-RESULTS.md` for the distributions and incomplete attempt.
It checks
the expected named socket and the actual listener executable before submitting
JS. It refuses production ports and never launches, restarts or stops an app.
It serves the capture temporarily on loopback, uses dedicated canvases, collects
timer/rAF gaps, native `frontend_heartbeat` command round trips and independent
HTTP timings, and waits for an actual result callback rather than treating eval
acceptance as completion. The helper does not replay PTY arrivals or prove input
continuity, and it does not replay the historical stall. On a permanent WebView
hang, an outer timeout can report failure but cannot make that thread run its
cleanup until it resumes. Failure reports retain the eval receipt, independent
backend probes and recent instance logs, plus any measurements received in an
error callback; acceptance alone is never completion. Check callback-error and
no-callback timeout persistence without a desktop:

```sh
scripts/with-test-tmp.sh node --test scripts/stall-workload/desktop-replay.test.mjs
```

## Recorded result: 2026-10-20

See `scripts/stall-workload/RESULTS.md` for the measured distributions,
provenance and the limits of the comparison. No 30-second heartbeat outage was
reproduced. The story's original real-stall RED/GREEN remain open.
