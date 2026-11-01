# False barge-in investigation, 2605-53fc, part 2

## Result: cause unproven

The complete retained 2026-10-31 log contains **28 capture holds**, not only the
12 paired holds in the earlier 18:20 excerpt. Of these, 26 resume and two end
with an accepted-turn hush. All 28 have unknown playback activity at the hold.
There are no matching hold, resume, hush, synthesis or echo-warning events in
the retained 2026-11-01 log snapshot through 01:53:56.060438 UTC. This is a
snapshot result, not a claim about later events or other instances.

No behavior change or synthetic regression test is justified by these logs.
They show sustained post-cancellation activity, but not its acoustic source.
Echo, nearby speech, capture discontinuity, and AEC convergence remain possible.
A test that supplies an invented late reference would test that assumption,
not reproduce a measured historical cause. The intentional hold stays intact.

Source inspected: `9bc0c1398b083b2d486b4d1a9a2bc60b4e05de6c` (main baseline).
The earlier investigation and hold-intent audit were read from `1e94858d68`
and branch `fix/voice-tts-stutter`. No running application, microphone, output
device, installed binary, or runtime setting was opened or changed.

## Retained evidence and classification

`historical-events.txt` contains selected original lines from the default
instance's `logs/tuic.log.2026-10-31` and `logs/tuic.log.2026-11-01` under
`~/Library/Application Support/com.tuic.commander/`. Each file header records
the source byte count, SHA-256 of the complete bytes inspected, and last event
time. The source files were read once each. The November log can grow afterward.
The selection includes all pause/resume/hush, accepted/dropped/wake transcript,
synthesis, echo-warning, capture-error and CPU-spike lines. It contains no PCM.

[`hold-table.md`](hold-table.md) classifies **every** hold with exact UTC
start/end, duration, resolution and available activity evidence. Each pause has
one later resume or hush on capture thread 185532, with no overlapping pause.

The following trigger classification applies to every table row:

- Logged trigger: `speech: paused, voice activity`.
- Code-derived trigger: rising sustained voice after `EchoGuard::clean`, or a
  qualifying closed utterance in one drained batch; not an actual logged VAD
  measurement. No RMS, reference energy, residual energy, correlation, delay,
  capture backlog or longest-run value is recorded at these edges.
- Recognition is absent for all 26 resumed holds. The generic resume text
  does **not** prove a rejected activation phrase: empty/stale results,
  discarded capture, the pause limit and loop release use the same message.
- Row 2 hush is followed by `heard="computer a che punto"`; row 9 hush by
  `heard="Computer, ma che mutazione"`. Both have `speaking=false`,
  `into_playback_ms=None`, `interrupted=0` **at hush**, not at hold start.
  Neither proves an audible interruption or even active playback earlier.
- All 26 resume rows have unknown playback at both edges. Pause is logged
  even when the output has nothing to play. Synthesis completion is not a
  playback-active measurement, and no rendered duration is logged.

Rows 10, 11, 12, 27 and 28 resume after 8010.654–8042.797 ms. These durations
are consistent with the 8000 ms loop limit; the shared resume log cannot prove
which call site fired. Three further accepted-turn hush events follow holds
that already resumed (18:17:51, 18:18:24 and 18:21:59). They are not additional
holds or evidence that those earlier holds resolved as hush.

Pocket TTS / giovanni / Italian is logged for synthesis at
18:16:07.794264–18:16:13.158549,
18:19:57.421285–18:20:01.445833 and
18:20:51.362506–18:20:53.451998. Audio owner, output device, actual consumption,
reply PCM and reply duration are absent. Holds 1–3 occur before the first logged
synthesis that day, so the pause message cannot be treated as playback proof.

### Load and capture evidence

- Capture logs `A buffer underrun or overrun occurred` at 17:18:39.532816,
  17:54:16.842839 and 18:15:20.991656 UTC. The last precedes the 18:16 holds,
  but is not a per-hold failure or a count of missing samples.
- The app CPU watchdog reports 244.9% at 18:16:15.625845 and 88.9% at
  18:20:05.882399 (inside row 16). These are app-process measurements, not
  whole-machine load, individual audio-thread scheduling or child CPU.
- No selected far-end-full or AEC refusal/fallback warning is present in either
  inspected day's log. This is not proof of correct reference alignment, nor
  proof that initialization warnings did not occur on an earlier day.

Thus load and an input-stream error are measured. Their causal relationship
to any hold is not measured. A reported room with nobody speaking is not a
recording of what the microphone delivered to VAD.

## What decides a hold

Locations below refer to the source baseline above.

1. `continuous.rs:1692–1741`: the runtime drains capture, then cleans it with the
   shared echo guard before pushing it into the segmenter. A hold is requested
   on a false-to-true `has_voice` edge (or qualifying closed utterance), only
   when no hold is tracked and an `Interruptible` port exists. It does not
   require active output. `speaker.rs:567–576` logs, records the loop hold if
   the output can pause, and calls the output's pause method.
2. `continuous.rs:139–155, 185–195, 277–337`: 16 kHz mono capture is measured in
   20 ms / 320-sample frames. Default activity is RMS >= 0.01. A voice needs a
   200 ms sustained run. A dip up to 60 ms can bridge only after three active
   frames confirm it. Total active time alone is not the barge-in trigger.
   Pre-roll is 300 ms; 1500 ms quiet closes an utterance; max length is 30 s.
3. `continuous.rs:1759–1847`: elapsed hold > 8000 ms resumes before recognition.
   A configured activation phrase allows serial prefix recognition at whole
   seconds up to four seconds; final recognition still owns admission.
   Accepted `HeldBack`/`Activated` outcomes hush; other outcomes resume.
   `Capture::release` resumes on runtime exit. The timeout is checked by ticks,
   not a separate hard deadline while synchronous inference blocks the loop.
   Recognition confidence/RMS gates are downstream of the initial hold.
4. `echo.rs:300–394`: `EchoGuard` pairs each 10 ms / 160-sample capture frame
   with a 16 kHz far-end frame. The reference holds up to 120 s of queued audio;
   overflow drops newest samples and warns. Partial capture below 10 ms is
   carried. Alignment is by sample count, not a playback clock.
5. `echo.rs:426–465`: `FarEndTap::play` registers the complete reference before
   calling the real output. A routine late registration **after** playback is
   therefore not the shipping ordering. When the reference is empty, it pads
   with silence for capture backlog plus partial carry. Subsequent queued
   replies append. Pause calls the device first, then marks a capture-position
   silence window if pause is supported; resume closes the window first, then
   resumes the output. Already-recorded capture remains before the window.
6. `echo/webrtc.rs:49–96`: AEC3 analyzes each reference frame, then processes
   capture. Delay is estimated (`stream_delay_ms: None`). Noise suppression,
   gain control and high-pass filtering remain off. The estimator needs to
   converge; the code provides no estimate/residual telemetry at a hold.
7. `commands.rs:2373–2375, 2460–2493`: desktop drains its buffer to a local Vec;
   the backlog probe reads the remaining endpoint buffer. Browser capture has
   the same drain/probe split (`browser.rs:117–123`). These probes are not
   acoustic capture or output timestamps.

## How load could affect this path — hypotheses, not attribution

A delayed tick alone is not sufficient evidence of broken AEC: retained capture
and far-end audio are still processed in frame order. More capture backlog at
reply start is explicitly padded. Inline inference can, however, delay a hold
verdict and make its wall-clock duration longer. A single delayed drain can
also contain the voice edge and utterance closure, so it can pause and resume
within one tick.

Specific unmeasured ways load could break the assumed sample alignment:

- `audio.rs:281–363` uses `try_lock` in capture callbacks and counts dropped
  samples on contention or buffer overflow. `EchoGuard` sees surviving samples,
  not a timed gap marker. For example, 1600 missing samples represent 100 ms
  at 16 kHz; without a gap marker its reference cursor is 100 ms behind the
  physical capture timeline. No per-hold drop delta was logged. A device
  underrun/overrun may also break continuity, with unknown loss here.
- There is a scheduling window between endpoint drain and guard clean. If a
  new reply is registered in that window, the backlog probe cannot count the
  batch already moved into the tick's local Vec. A 50 ms drained batch is
  800 samples; a 1 s inference backlog is 16000 samples. Missing that padding
  makes the reference early relative to capture, not a reference registered
  after playback. No log identifies this interleaving in the incident.
- A scheduler delay between `note_rendered` and the actual inner `play` makes
  the registered reference early by that delay. OS/device buffering and browser
  delivery add more unknown delay. AEC can estimate some delay, but this audit
  does not establish its convergence time or tolerable range for these signals.
- Nonlinear speaker distortion, reverberation, real external speech, and
  startup convergence can leave sustained residual above 0.01 even without
  any reference-order bug. No retained PCM distinguishes these cases.

These are different failure models. Choosing a guard delay, changing the VAD
floor, removing the reversible hold, or inventing a late-reference RED case
would choose a cause that the evidence has not established.

## Smallest useful diagnostic proposal — not implemented

Add one structured hold-edge event and one reason-coded resolution event per
hold, with a local hold ID. Reuse the existing tick and pause/resume sites;
no threshold, inference schedule or hold policy change is needed.

At the edge, record owner/generation, output queued/nonempty state and
pause capability (explicitly **not** acoustic activity), raw capture RMS and
peak, cleaned 20 ms RMS maximum and active-frame count, sustained-run length,
drained sample count, endpoint backlog, partial-frame count, far-end queued
samples/cursor, last render age, callback/drop counters and tick/drain-to-clean
latency. Keep aggregates for the batch, not per-frame log lines. Snapshot raw
energy before cleaning. Snapshot output state before the pause changes it.

At resolution, record elapsed time and the actual call-site reason: accepted,
activated, rejected, empty, stale, discarded, limit, or release; include final
recognition duration/skip reason without adding transcript text. This alone
would separate the five ~8 s cases from final-recognition rejection and prove
whether a hold was requested while output was queued.

To test **echo attribution**, a follow-up opt-in bounded capture is necessary:
retain the actual raw near-end, matching far-end and cleaned frames plus
capture sequence/gap and output timing on one monotonic timeline. Dump only an
episode window around the edge, through the existing instance-local diagnostic
storage. PCM is sensitive; keep it off by default, bounded, and require explicit
recording authority. Numeric RMS logs alone cannot prove speech is echo.

Do not collect this by enabling or opening the daily microphone during this
assignment. After a real false hold is captured, replay its measured ordering
through `EchoGuard` and `continuous::tick`, use the production AEC3 adapter, and
assert the observable pause count. Include aligned-echo, genuine overlapping
near-end speech, and pause/resume-tail controls. Only then select a fix and a
RED/GREEN pair. No diagnostic behavior has been added in this branch.

## Verification and scope

- Python snapshot/extraction checked all retained log bytes, paired all 28
  pause edges without overlap or an unmatched edge, and calculated the table
  directly from microsecond UTC timestamps.
- Source inspection covered the capture callback, desktop/browser drain and
  backlog probes, EchoGuard/FarEndTap, AEC3 adapter, segmenter, tick verdicts,
  Speaker hold/hush and the existing intentional-hold tests.
- No Rust build, Rust test, Clippy, synthetic load, playback probe, application
  launch, microphone access or HTTP attachment was run. This is a
  documentation/evidence-only result; there are no RED/GREEN output lines.
- No production source, config, transport, dependencies, or existing test was
  changed. No app restart is needed for these artifacts.

## Approved diagnostic continuation

The investigation above is the evidence-only snapshot committed as `053be5678`.
The coordinator subsequently approved the numeric edge/resolution diagnostic.
Production now emits the events described in `docs/backend/dictation.md`, with
no new transcript or PCM retention and no change to the hold/admission rules.
This does not retroactively supply the missing historical measurements or prove
a false-trigger cause. Runtime deployment still needs Boss's manual Rust restart.

### Diagnostic verification

`diagnostic-tests.txt` retains the exact named feature-enabled rb command and
result lines at source commit `f2db9811a024c17f375781828409422f65750a35`.
The Linux test-profile build completed in 7m 04s. Nextest ran 22 selected tests
across the domain and application library binaries: all 22 passed, in 0.094s;
outer exit 0 and `RB_BOX_DIRTY=no`. No native/macOS execution is claimed.

New tests observe the actual structured tracing fields from production `tick`:

- `barge_in_hold_events_snapshot_the_edge_once_before_a_rejected_hold_resumes`
  catches a missing/duplicated edge, confusion between raw and cleaned energy,
  lost inference time, uncorrelated resolution and unavailable probes reported
  as false/zero. A deliberately delayed canned recognizer is internal test
  timing, not measured Whisper performance.
- `barge_in_hold_events_name_the_actual_resolution_without_changing_the_verdict`
  covers accepted, activated, empty, limit and release reasons, preserving the
  original pause/hush/resume calls and avoiding a duplicate on cleanup.
- `barge_in_hold_events_distinguish_a_stale_final_recognition` catches a result
  withdrawn during recognition being mislabeled as ordinary rejection.
- `barge_in_hold_snapshot_counts_browser_capture_loss_and_drain` drives the real
  browser capture buffer and checks cumulative batches, overflow samples,
  post-drain backlog and unknown rather than blocking on a busy buffer.

The other selected cases cover existing hold limits, user pause, reference
pause/backlog alignment, capture loss and browser queue ordering. No test
expectation was weakened or changed. Format checks and `git diff --check` pass.
No retries, Clippy, full suites, synthetic load, PCM recording or daily-instance
access were needed. The original native failure cause and regression provenance
remain unproven; the original story criteria therefore remain unchecked.
