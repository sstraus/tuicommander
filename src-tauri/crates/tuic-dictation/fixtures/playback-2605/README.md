# Speech gap investigation, 2605-53fc

## Result

The native scheduling gap did **not** reproduce in these runs. No playback fix,
buffer change, or regression attribution is supported by this evidence.
The historical cause remains unresolved. This is an investigation result, not
completion of the original fix criteria.

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
- No regression test or fix is claimed. External-command synthesis was traced
  in source but not executed.

## Provenance and next boundary

No native scheduling regression commit is identified: the reported failure
was not reproduced. Claiming either a commit or "never worked" would exceed
the evidence. The native queue already renders while the previous buffer plays.

There is a separate source-proven browser defect: a binary downlink calls
`stopPlayback` and replaces the active buffer, and `BrowserOutput::play`
replaces its expected deadline. Multiple queued replies can truncate each
other. That replacement is present in `f03ec66af5`, which introduced browser
voice. It is not evidence that this defect caused the historical report and
was not changed here.

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

## Final artifact check

After adding failure exit codes, the example rebuilt natively in 6.92 seconds.
The Pocket `--direct` smoke check returned zero and all three utterances
reported `Finished`; see `native-final-smoke.txt`. This does not convert the
earlier failed Edge utterance into a pass.
