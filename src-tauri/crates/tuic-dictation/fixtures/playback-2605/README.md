# Speech gap investigation, 2605-53fc

## Result

The native scheduling gap did **not** reproduce in these runs. No native playback fix, buffer change, or native regression attribution is
supported by this evidence. The separate browser fix is described below.
The historical cause remains unresolved. This is an investigation result, not
completion of the original fix criteria.

**Continuation:** the intentional capture-loop pause mechanism **did** reproduce
inside known active, muted native playback using synthetic voiced near-end PCM
and a canned unaddressed transcript. Pocket and Edge both halted source
consumption for about 1.6 s, then resumed and finished. This is distinct from a
spontaneous scheduling gap and does not establish a historical activation cause.
See “Capture-loop hold continuation” below.

Source baseline: `cfe67bee74f8e48e6a9e38fe79442a1668df635a`. The worktree was
fast-forwarded from `17422efd5c` before the investigation. Measurements ran on
the Mac on 2026-11-01. The running app was not changed, restarted, or attached.
Only its existing log files were read.

## End-to-end path

1. The bound model calls the MCP `voice` tool. Desktop and HTTP owner requests
   use the same `commands::speak` core.
2. `open_speaker_for` resolves the engine, voice, language and output owner.
   `LoggedSpeech` records synthesis start and completion, not playback.
3. `Speaker::say` adds a generation-stamped utterance to the bounded queue.
4. The `speech-queue` worker calls `Speech::synthesize`:
   - Pocket generates text chunks into one latent sequence, then decodes it.
   - Edge collects MP3 responses, then decodes them.
   - External speech waits for its command and reads its output WAV.
   All three return a complete `SpeechAudio` before playback starts. They do
   not deliver synthesis chunks to the device as each chunk finishes.
5. `render_loop` runs the offline loudness stage, checks the generation, and
   calls the output under the queue lock. `FarEndTap` records the echo reference
   before the inner output receives the audio.
6. Desktop `DeviceOutput` appends a `SamplesBuffer` to the rodio `Player`, whose
   mixer feeds the CPAL output stream. It returns before playback drains.
   The worker can synthesize the next utterance while earlier audio plays.
   Its 25 ms poll observes completion; it does not schedule buffer boundaries.
7. A browser owner instead uses `BrowserOutput`, broadcasts `Downlink::Speak`,
   and sends PCM over the audio WebSocket. `browserVoice.ts` creates an
   `AudioBufferSourceNode` in the client context. That is a separate playback
   implementation, not the native device mixer.
8. Capture can call `Speaker::pause` before the activation-phrase verdict.
   Rejected or unaddressed voice resumes playback; an accepted interruption
   calls `hush`. These holds can create silence inside a fully rendered buffer.

## What the existing logs establish

`historical-pause-excerpt.txt` contains only synthesis and pause/resume lines
from the app log for 2026-10-31, 18:19:57–18:20:56 UTC. It contains no reply text.

- Engine: Pocket TTS; voice: `giovanni`; language: Italian.
- Synthesis: 18:19:57.421285 to 18:20:01.445833 UTC, or 4024.548 ms.
- The next synthesis: 18:20:51.362506 to 18:20:53.451998 UTC, or 2089.492 ms.
- Twelve paired capture-loop holds lasted 1530.054–2092.294 ms.

The pause log does not report whether audio was active. It also does not retain
capture PCM, echo residuals, the reply duration, or an output-owner field.
Therefore these lines identify a candidate interruption mechanism, but do not
prove that these holds caused the reported stutter or identify a desktop versus
browser owner. They do not prove an echo-canceller failure.

## Isolated native experiment

`examples/speech_gap_probe.rs` drives the real `Speaker`, Pocket or Edge engine,
production-default loudness (-18 dBFS, 0.67 levelling), and `FarEndTap`.
No microphone or application instance is opened. The tap uses `PassThrough`
and receives no capture frames, so the experiment does not test acoustic echo
cancellation or barge-in.

Two output modes are available:

- Default: the same `SamplesBuffer -> Player -> default device mixer` path as
  `DeviceOutput`, with an iterator that records source-consumption boundaries
  and an output-error callback. It returns zero-valued samples to the hardware.
- `--direct`: the production `DeviceOutput`, supplied with zero-valued PCM of
  the synthesized length and rate. This verifies append/drain state but does
  not expose per-source consumption timestamps.

Three utterances are queued together:

1. `Ciao, sono pronto.`
2. `Le chiavi sono sul tavolo della cucina.`
3. `Possiamo controllare i risultati insieme.`

The loaded case used seven owned workers on this 14-core Mac, with a request for
background QoS on the last worker. Each worker has a 90-second ceiling, checks a
stop flag, and is joined before process exit. There is no persistent load
process. The syscall return value was not retained, so the requested QoS is not
claimed as verified. Synthetic load was explicitly authorized for this task;
it must not be run on a shared Mac without authorization.

### Final path results

`native-full-path.txt` is the exact probe output after the loudness and tap
stages were enabled. Times are relative to each process's monotonic origin.
All numbers below are milliseconds.

| Engine / load | id | Synthesis start | Synthesis done | Append | Source start | Source end | Gap after prior source |
|---|---:|---:|---:|---:|---:|---:|---:|
| Pocket / none | 0 | 112.173 | 479.463 | 479.604 | 494.262 | 1177.082 | — |
| Pocket / none | 1 | 479.618 | 752.901 | 753.616 | 1177.111 | 3481.276 | 0.029 |
| Pocket / none | 2 | 753.638 | 1049.888 | 1050.620 | 3481.292 | 5998.315 | 0.016 |
| Pocket / seven workers | 0 | 65.451 | 383.336 | 383.518 | 404.820 | 1343.510 | — |
| Pocket / seven workers | 1 | 383.529 | 653.557 | 653.999 | 1343.515 | 3732.955 | 0.005 |
| Pocket / seven workers | 2 | 654.009 | 978.852 | 979.646 | 3732.961 | 6463.506 | 0.006 |
| Edge / none | 0 | 70.775 | 2820.598 | 2821.080 | 2842.228 | 5487.590 | — |
| Edge / none | 1 | 2821.132 | 4037.442 | 4038.734 | 5487.596 | 8773.111 | 0.006 |
| Edge / none | 2 | 4038.810 | 5381.837 | 5383.052 | 8773.120 | 12186.296 | 0.009 |
| Edge / seven workers | 0 | 65.908 | 1689.658 | 1690.138 | 1727.850 | 4373.209 | — |
| Edge / seven workers | 1 | 1690.177 | 2822.612 | 2823.201 | 4373.223 | 7658.580 | 0.014 |
| Edge / seven workers | 2 | 2823.230 | 3820.634 | 3821.250 | 7658.593 | 11071.907 | 0.013 |

The default output resolved to stereo F32, 48 kHz, fixed 2048-frame buffers.
The largest measured source-consumption interval was 42.681 ms. A 2048-frame
buffer at 48 kHz spans 42.667 ms, so this interval is consistent with block
consumption. It is not evidence of a 42 ms inserted silence. No measured output
error callback fired. Source boundaries were 0.005–0.029 ms apart, and each
later utterance was appended while the previous output remained active.

The Pocket direct-output control finished all three utterances. The Edge
direct-output control finished two; its third synthesis failed with
`Network is down (os error 50)`. This failure is retained, not counted as a
successful run. The earlier direct-output control in `native-initial.txt`
finished all three Edge utterances, before the tap and loudness were added.
The final probe returns a nonzero exit code on a failed or unfinished utterance;
the recorded version still returned zero and reported the failure in its state.

`native-initial.txt` records the first six scenarios, without the loudness/tap
stages. All eighteen utterances finished. Default-mode source boundaries were
0.006–0.025 ms apart. These are separate runs, not a RED/GREEN pair.

### Measurement limits

- Source start/end are mixer iterator consumption timestamps. They are not
  acoustic playback or DAC timestamps. Rodio can prefetch source samples.
- The 10 ms source-sampling interval is a consumption-cadence proxy, not an
  exact CPAL callback entry/exit trace. Only stream errors use the CPAL-backed
  output-error callback.
- Pocket sampling is stochastic. Different rendered durations mean these runs
  cannot support a precise loaded/unloaded synthesis-speed comparison.
- No real browser/WebSocket timing run, `BrowserOutput` runtime probe, microphone
  capture, echo-canceller replay, or acoustic loopback was run.
- The initial native investigation claims no regression test or fix.
  External-command synthesis was traced in source but not executed.

## Provenance and next boundary

No native scheduling regression commit is identified: the reported failure
was not reproduced. Claiming either a commit or "never worked" would exceed
the evidence. The native queue already renders while the previous buffer plays.

The original browser path had a separate source-proven defect: a binary
downlink called `stopPlayback` and replaced the active buffer, and
`BrowserOutput::play` replaced its expected deadline. Multiple queued replies
could truncate each other. That replacement is present in `f03ec66af5`, which
introduced browser voice. The continuation fixes this defect below; it is not
evidence that this defect caused the historical report.

The next useful experiment is a recorded capture/echo-residual replay with
pause/resume and device consumption on one timeline, or an isolated browser
playback run. Do not remove the activation-verdict hold or increase buffers on
the basis of this no-reproduction result.

## Repeat

Build only the example, through the authorized native build slot and mbx:

```sh
export PATH="$HOME/Library/Application Support/mbx/bin:$PATH"
# Invoke this command through the orchestrator build-slot wrapper.
cargo build --manifest-path src-tauri/Cargo.toml -p tuic-dictation --example speech_gap_probe
```

Run the artifact path reported by Cargo. Do not replace an installed binary.
The bundle argument must name an existing downloaded Italian Pocket bundle;
no models are downloaded. Edge needs internet access.

```sh
<artifact> pocket <italian-bundle-directory>
<artifact> edge
<artifact> pocket <italian-bundle-directory> --direct
<artifact> edge --direct
```

`--load` enables the bounded synthetic load and requires explicit authority.
Use repository-local `TMPDIR`, `TMP`, `TEMP`, and `TUIC_TEST_TMP_ROOT` when
running the dev-dependency-enabled example. No application restart is needed:
this change adds a standalone diagnostic, not a playback behavior change.

## Capture-loop hold continuation (2026-11-01)

The retained probe now accepts `--capture-hold` (measured output only, no
`--load`). This is **synthetic near-end PCM and a canned recognition verdict**,
not microphone input, a Whisper recognition run, or an acoustic/AEC reproduction.
It waits until real rendered audio has actually been consumed by the native
mixer, then feeds the production `continuous::tick` through `VoiceEndpoint`:
300 ms of 440 Hz voiced PCM at amplitude 0.5, followed by silence, in 50 ms
chunks at wall-clock cadence. Default `SegmenterConfig` is unchanged. The
activation phrase is `senti mac`; the synthetic endpoint returns an explicitly
unaddressed canned transcript. The real `Speaker`, Pocket/Edge synthesis,
loudness, `FarEndTap`, rodio player and default-device stream remain in use.
Hardware-bound samples remain zero, as in the prior measured probe.

### What a capture hold actually does

- `continuous::tick` cleans near-end PCM with the shared guard, then detects
  the rising edge of `Segmenter::has_voice`: an unbroken default 200 ms run,
  not merely scattered energy or the first voiced frame. A closed utterance
  qualifying inside one drained chunk can also trigger it. The pause call does
  **not** check whether the speaker currently has active audio; historical
  pause log lines therefore cannot establish active playback.
- It calls `Interruptible::pause`, wired here to real `Speaker::pause`, and
  records the tick's `now_ms`. `Speaker` records `loop_held` for pause-capable
  outputs, then pauses the output. This does not discard the queue, cancel
  synthesis, change generation, or wait for a verdict. The worker continues
  rendering and appending; new audio appended to a paused player waits too.
- `FarEndTap::pause` pauses the inner device first, then opens a far-end pause
  window. Capture positions in that window receive silent far-end reference
  without consuming queued reply samples. Already-recorded backlog remains
  before the window. Resume closes that window before resuming the device.
- A rejected, empty, stale or otherwise non-admitted final transcript resumes
  the held reply; an admitted/activated turn clears the hold and calls `hush`
  instead, ending the old generation and stopping/discarding its audio.
  Nonqualifying discarded capture also resumes, as does `Capture::release`
  at runtime exit. Transcription-error exits rely on runtime release.
- The default trailing quiet is 1500 ms. Thus a hold can naturally span the
  remaining speech, that quiet interval, and synchronous recognition latency.
  A tick resumes a hold once elapsed time is **greater than** 8000 ms. This
  check is not a hard wall-clock deadline during synchronous transcription:
  it runs only when the loop reaches it, before prefix/final recognition.
- `Speaker::resume` clears only the loop hold and preserves a user hold.
  `SpeakerStatus.paused` reflects the **user** hold, not this automatic hold;
  native `is_speaking` is queue-nonempty even while paused. Neither is a DAC
  activity measurement.

The probe records pause/resume, every capture tick's cumulative iterator
consumption, append timing (including whether held), source start/end, and
final utterance states. Its success condition requires one real capture
pause/resume, an unchanged settled consumption count for at least 1000 ms,
post-resume consumption growth, and all three utterances `Finished`.
The first 150 ms after requesting pause are excluded from the plateau check
because the mixer can prefetch audio. No microphone, app launch, load workers,
production Rust changes, Git mutation, or broad tests are part of this run.

### Observed hold runs

Exact commands and environment are retained in `hold-commands.txt`; raw
monotonic timelines are `hold-pocket.txt`, `hold-edge.txt` and
`hold-pocket-control.txt`. All three exited 0, with every utterance `Finished`.
Both hold runs report `load_workers=0`, one pause/resume, a stable consumption
plateau, and consumption restarting afterward. The stream was stereo F32,
48 kHz, fixed 2048 frames. No output-error event was recorded.

| Measurement (ms from process origin) | Pocket hold | Edge hold |
|---|---:|---:|
| First source starts | 551.261 | 1230.878 |
| Synthetic capture starts, consumption already confirmed | 637.636 | 1316.848 |
| Capture-triggered output pause | 788.447 | 1467.797 |
| Output resume after final canned rejection | 2389.340 | 3067.804 |
| Pause request to resume request | 1600.893 | 1600.007 |
| Second append, while held | 810.886 | 2056.169 |
| Third append | 1131.725 (held) | 4378.521 (resumed) |
| First source maximum consumption interval | 1663.983 | 1663.909 |
| Last source ends | 8145.955 | 12196.266 |

Both runs had consumed 6401 source samples when pause was requested, then
settled at 6480 (79 more prefetched samples). Every held tick from index 4
through index 34 retained that same count, including across appends. After
resume, tick 36 recorded 7458 and tick 37 recorded 8482. This demonstrates a
mid-buffer consumption halt and continuation, not synthesis starvation or
replacement/truncation. Pocket's later source transitions were 0.008 ms apart;
Edge's were 0.006 and 0.012 ms apart. The no-hold Pocket control's maximum
consumption interval was 42.675 ms and its transitions were 0.006/0.011 ms.
Stochastic Pocket durations differ, so this is not a waveform-identical
RED/GREEN pair or a performance comparison.

The synthetic hold is about 1.6 seconds because the 200 ms threshold lands on
the fourth 50 ms chunk, with 100 ms of voiced PCM and 1500 ms of trailing quiet
still to consume. Chunk arrival timestamps mark the end of each chunk; the
first synthetic chunk is ready at capture-start. A one-second wake-prefix
recognition also ran and returned the canned unaddressed text; this does not
release the hold. The final 1800 ms utterance recognition did release it.
Actual recognizer latency was deliberately **not** simulated.

This establishes the intentional pause mechanism **during known active native
playback**. It does **not** establish why any historical hold activated, whether
historical playback was active, whether residual echo crossed the voice gate,
or whether the original report was native or browser playback. The twelve
historical 1530–2092 ms holds remain insufficient evidence for those claims.

Build evidence is retained as `hold-build.stderr` and `hold-build.jsonl`
(progress lines/dependency artifact JSON omitted). The first build failed
before compilation because the repository-local mbx Unix-socket path exceeded
`SUN_LEN`; shortening only the temporary socket prefix to `$HOME/Gits/.tmp`
resolved that ordinary path-length failure. The admitted native example build
then succeeded in 23.19 seconds through mbx (371 cache hits, one miss), without
an extra target directory. No permission/confinement bypass was involved.

### Final continuation artifact check

The retained probe was refined to enforce the 1000 ms plateau using monotonic
wall time rather than a tick-count proxy. The identical authorized build command
was rerun (`hold-final-build.stderr`/`.jsonl`), then the identical Pocket hold
command was rerun (`hold-final-pocket.txt`). Exit 0; all three utterances
`Finished`; one pause/resume; plateau and resumed growth assertions passed.
Pause/resume were 852.641/2451.913 ms (1599.272 ms hold); the third utterance
appended while held at 1072.979 ms. Count again settled at 6480 and grew to
7458/8482 after resume; maximum first-source interval was 1663.915 ms.
The earlier Edge/control measurements precede only this assertion refinement,
not a change to synthesis, capture input, output or hold behavior.

## Final artifact check

After adding failure exit codes, the example rebuilt natively in 6.92 seconds.
The Pocket `--direct` smoke check returned zero and all three utterances
reported `Finished`; see `native-final-smoke.txt`. This does not convert the
earlier failed Edge utterance into a pass.

## Browser truncation continuation

The browser now schedules each complete reply after the preceding buffer on
the audio clock. It does not wait for `onended` to schedule the next reply.
Pause preserves the active offset and later buffers; resume schedules their
remaining durations in order. Stop and session teardown invalidate all nodes,
so delayed callbacks cannot acknowledge interrupted or rescheduled audio.

The server adds queued durations rather than replacing the fallback deadline.
Appends and pause/resume/stop share one state lock. Binary downlinks now carry
`u32` rate + `u64` reply sequence + PCM. Per-reply ACKs echo the sequence as a
decimal string; an old or duplicate ACK cannot clear a later append or turn.
Client and backend ship together. Existing browser tabs must reload after the
backend update; no old-format compatibility path was added.

`browser-red.txt` retains the exact targeted Vitest failure with production
`browserVoice.ts` restored temporarily to investigation commit
`4dfd3f99999798b7f0190104a5540eba9cd982df` (and restored afterward). The
regression fails at the first assertion: the first source's `stop()` was called
once when the second downlink arrived. The changed frame header is not the
failure oracle. The current targeted file passes all 24 tests; `tsc --noEmit`
passes. Coverage includes audio-clock scheduling without ended delivery,
queued hold/resume, stop/teardown, stale callbacks and 64-bit sequence decoding.
These are transport unit tests with injected audio-node/socket dependencies,
not a browser/WebSocket acoustic measurement.

The intentional native capture hold can produce a mid-buffer silence of the
observed scale. Why any historical hold activated, whether real audio was then
playing, and whether this explains Boss's original report remain unproven.
No activation policy, native buffer size or synthesis scheduler was changed.

### Final scoped verification

- `rust-red.txt`: on investigation commit `4dfd3f99999798b7f0190104a5540eba9cd982df`,
  the ACK driver is adapted to the old anonymous method signature. The same
  behavioral assertion fails: `first ACK must keep the second reply`.
- `rust-green.txt`: final remote `cargo nextest run --lib --features dictation`
  selected browser-link tests, the binary-header test and owner-routing test:
  **16 run, 16 passed**, exit 0. The rest of the library was deliberately
  filtered out. Compilation took 4m 41s; tests took 0.092s.
- The initial remote sync rejected a changing tree before execution. A second
  attempt without the opt-in dictation feature selected no tests and failed.
  After enabling the feature, RED succeeded; the first GREEN compile found a
  stale tuple-pattern match in the owner-routing test. That match was updated
  and the final named run above passed. These failed setup/compile attempts
  are not reported as successful tests.
- Final frontend evidence: 24 tests pass in the touched Vitest file;
  `tsc --noEmit`, changed Rust/TypeScript format checks and `git diff --check`
  pass. Read-only independent static review found no actionable defects;
  this does not replace the coordinator's critic or hardware acceptance.
