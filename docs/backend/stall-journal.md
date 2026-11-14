# Persistent stall evidence

The stall journal retains selected observations after the application log ring wraps or the application restarts. It does not identify a cause or prove that the frontend and an independent HTTP request ran concurrently.

## Storage and limits

The journal is stored beside the backend logs in the instance config directory:

- `logs/stall-journal.jsonl`: current records, up to 2 MiB.
- `logs/stall-journal.previous.jsonl`: the previous file, up to 2 MiB.

Rotation replaces the previous file. The writer uses a lossy non-blocking queue of 128 records. Disk writes and rotation run on the existing tracing-appender worker mechanism, not on the calling thread. Graceful shutdown flushes the queue. A full queue, disk error, abrupt process termination, or rotation can lose evidence. This is bounded diagnostic retention, not an audit log.

Each JSON line has `schema_version: 1`, a backend `timestamp_ms`, a fixed `kind`, and numeric `metrics`. No log message, source string, repository path, branch, terminal ID, breadcrumb detail, or response body is stored. Unknown fields are discarded.

## Observations

| Kind | Producer | Metadata |
| --- | --- | --- |
| `git_refresh` | A slow `git.refreshBatch` | Total/body/flush milliseconds, active repository count, response worktree count, store worktree count, JSON response bytes |
| `git_stats` | A slow `git.statsBatch` | Total/body/flush milliseconds |
| `ui_freeze` | Existing frontend `UI freeze` report | Timer gap milliseconds |
| `heartbeat_gap` | Backend `Frontend unresponsive` report | Silence seconds, last heartbeat receipt wall timestamp, backend observation wall timestamp |

Frontend measurements retain the existing `perfDebug` gate. The heartbeat watchdog remains always on and desktop-only. Response bytes mean the UTF-8 encoding of the parsed structure response serialized as JSON, not transport framing or wire compression. Size calculation runs only after a slow structural batch, outside its timed body/flush region. This diagnostic calculation can add work when enabled.

The heartbeat receipt timestamp is taken in Rust when the heartbeat command runs. The watchdog's observation timestamp comes from its independent backend thread. Neither is an HTTP round-trip measurement, and neither distinguishes WebView blocking, delayed IPC delivery, and resource starvation on its own. Sleep rebaselining does not invent a received heartbeat timestamp.

The journal contains only events that the existing producers report. It does not change frontend sleep/background filtering or retroactively recover events lost before this build.

## Build an isolated replay input

Copy the journal files after graceful shutdown, or take a stable copy while the app is running. Do not point a workload harness at a live desktop instance.

```sh
node scripts/build-stall-replay.mjs replay.json stall-journal.previous.jsonl stall-journal.jsonl
```

Supply the previous file first; omit it when it does not exist. The builder rejects malformed/truncated JSON, unsupported versions, missing topology counts, oversized input, and more than 10,000 generated worktrees across the selected records. Select a smaller set of complete JSON lines if necessary.

The output contains sanitized observations and synthetic structure responses with deterministic checkout names. A refresh harness can use each `refreshes[].structure` as its `getRepoStructure` result and `repo_count` as the number of active repositories. `response_bytes` is the observed size, not the size of the generated response. The builder does not pad responses to that size or sleep for the observed timings.

**This is a metadata-derived synthetic topology, not the original workload.** Counts do not retain terminal state, branch status, string lengths, or the reactive graph. A count-based replay may fail to reproduce the stall. The original real-stall RED/GREEN criteria stay open until a captured workload reproduces the heartbeat gap and the same workload proves the fix. Do not turn the observed duration into an artificial busy loop and call it a reproduction.

## Opt-in terminal frame workload recorder

The numeric journal above cannot reconstruct canvas work. The separate frame
recorder retains terminal text and session IDs, including text that may be
private. It is **off by default**, does not persist its enable flag, and requires
`perfDebug` as well as explicit consent. Do not enable it on someone else's
terminal. Files are not anonymized; review them before sharing.

After loading this build, enable through the existing debug tool (local or token-authenticated callers):

```js
// debug action=invoke_js, script:
window.__TUIC__.setPerfDebug(true);
return window.__TUIC__.frameWorkload.enable(true);
```

`frameWorkload.status()` returns enabled state, retained bytes/events and dropped
count. `frameWorkload.enable(false)` clears retained data. No per-frame JSON,
base64, disk or IPC work runs: arrivals copy binary bytes, and paint entries retain
an immutable row-map snapshot plus font/weight, DPR, canvas geometry, colors,
damage rows, full-repaint and projection flags. Binary arrivals include hidden
state; visibility observer transitions are separate events. The decoder creates
new immutable row arrays, so later deltas cannot change retained paint inputs.

Retention is global across terminals: at most 256 events, approximately 4 MiB of
conservatively accounted payload, and ten seconds relative to the latest admitted event.
A dump after a long stall retains the last active window rather than expiring
the stalled paint before it can be saved. Object/allocator overhead is not an exact heap measurement. Oversized
events are dropped; pressure evicts oldest events. Every paint snapshot is
self-contained, even if its original delta baseline was evicted. A finite window
may be much shorter than ten seconds under pressure. This is lossy evidence,
not a lossless recording of every frame. Row-cost accounting is cached by weak
identity. Disabling the master gate stops capture; disabling the recorder also
releases the window.

A grid paint exceeding 200 ms or a main-thread freeze reported by the existing
freeze detector schedules a dump after JS regains control, at most once per
30 seconds, with one dump in flight. Accepting a dump freezes bounded event
references before yielding, so later arrivals cannot evict its triggering paint.
The active window can retain another 4 MiB while that capture is pending;
encoding and file writes remain deferred. It cannot save during an unrecovered
main-thread hang, and the freeze detector's sleep/background exclusions remain.
Manual `debug action=frame_workload_dump` bypasses the cooldown, not the opt-in
gates. Like `invoke_js`, acceptance is asynchronous: read the actual path or
error in `debug action=logs source=eval_js`, not just the submission receipt.
Browser clients can call `await window.__TUIC__.frameWorkload.dump()`.

The common IPC/HTTP writer stores at most 6 MiB per recording under the instance
config directory: `captures/frame-workload.json` and
`captures/frame-workload.previous.json`. Rotation writes a complete temporary
file first (up to one additional 6 MiB during writing), then replaces the previous
recording. Files use mode 0600 when created on Unix. Graceful writes are synced;
power failure or an I/O error can still lose the latest recording. Disk work and
validation run on the backend blocking pool; concurrent writers fail instead of
queuing payload copies. Encoding occurs only at dump and may itself cause a
short main-thread pause. Serialized overflow drops oldest events and increments
the dropped count. These content files are separate from the numeric-only journal.

### Paint replay (not the heartbeat RED)

Copy a recording away from the live desktop. In a dedicated browser/WebKit
harness with this frontend loaded, call:

```js
const canvases = new Map();
const canvasFor = session => {
  if (!canvases.has(session)) canvases.set(session, document.createElement('canvas'));
  return canvases.get(session);
};
const report = await window.__TUIC__.frameWorkload.replay(recording, canvasFor, true);
```

Use dedicated canvases, never a live terminal's canvas. The optional third
argument retains bounded event spacing. The harness loads the recorded font,
restores little-endian cell arrays/extras and projection, and calls the production
`createGridRenderer().paintGrid`. It reports paint milliseconds per retained
paint, excluding decode, font loading, resize and pacing. Binary arrival bytes
remain available for separate decode/merge analysis; the paint replay uses exact
merged snapshots, not an inferred delta baseline. It does not replay overlays,
input or backend traffic. The first retained incremental paint need not have its
previous pixels, though its grid paint operation and cells are retained.

`src/components/Terminal/__tests__/fixtures/frame-workload-smoke.json` is a tiny
**synthetic smoke recording**, not recorded agent output and not the original
stall RED. Unit tests with a context stub prove wiring only, not text-shaping
performance. Native WebKit replay with the actual recorded font, paired frontend
heartbeat/input and independent backend timings is still required for story
1638's real-stall RED/GREEN. This recorder and timing harness do not fix or prove
the cause of that stall.


### Generated workload research harness

`scripts/stall-workload/run.mjs` generates paced synthetic output in isolated
headless PTYs, captures the production CanvasTerminal pipeline through worktree
Vite, and compares exact git-exported renderers with identical retained paint
snapshots. It records Chrome event-loop/rAF gaps and independent backend request
latency. It does not generate a fixture of the historical stall or compare the
whole application at two revisions. The bounded recorder can retain only a
short suffix under this workload.

See [the harness runbook](../../scripts/stall-workload/README.md) and
[the 2026-10-20 measurements](../../scripts/stall-workload/RESULTS.md). The measured
30-second output run did not reproduce the long heartbeat outage. The prepared
`desktop-replay.mjs` helper is for the verified isolated `make agent-test`
instance after story 2086 lands; it has not yet been run in WKWebView. The
original real-stall RED/GREEN remain open.
