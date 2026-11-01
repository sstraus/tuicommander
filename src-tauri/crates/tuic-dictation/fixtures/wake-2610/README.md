# Recorded synthetic wake speech (2610-9b43)

These fixtures are output from the installed macOS speech engine, not simulated
Whisper transcripts. They test synthetic speech only, not microphone quality,
room noise, accents or human wake-word accuracy.

- Source: macOS 27.2, build 26B123, installed Samantha voice.
- Generation date: 2026-10-31.
- Format: WAV, signed 16-bit little-endian PCM, mono, 16 kHz.
- No microphone, speaker playback or running TUICommander instance was used.

Commands (the AIFF intermediate files were stored under the checkout-local
external evidence directory):

```sh
say -v Samantha -r 180 -o addressed.aiff \
  'Computer, please show me the current branch and explain the latest change.'
say -v Samantha -r 180 -o unaddressed.aiff \
  'Please show me the current branch and explain the latest change.'
afconvert -f WAVE -d LEI16@16000 -c 1 addressed.aiff addressed.wav
afconvert -f WAVE -d LEI16@16000 -c 1 unaddressed.aiff unaddressed.wav
```

| File | Frames | Duration | SHA-256 |
| --- | ---: | ---: | --- |
| `addressed.wav` | 65198 | 4.074875 s | `748cca2bef6b792aa4a6e6f7bdd1c0d90556e05ac3ac95d0b61dc446216dc6e2` |
| `unaddressed.wav` | 53096 | 3.3185 s | `d9f8c5da922ba29692c38eb1fd6ef7ec60c279418a50883ed9d9ff2d9754aba9` |

Native measurements must name the installed model, probe lengths, sample count,
wall time and process CPU time. Audio-timeline replay acknowledgement is not
physical microphone-to-display latency. Model loading and cold first inference
must be reported separately from warm p50/p95. Process CPU time does not include
Metal GPU execution cost.

## Native measurement

Run the example through the repository build scheduler and mbx, with an existing
model path. The final argument `paced` feeds the audio on its real timeline;
without it, replay runs as fast as recognition allows. Nothing opens a
microphone, plays sound or contacts a TUICommander instance.

```sh
cargo run --manifest-path src-tauri/Cargo.toml -p tuic-dictation \
  --example wake_probe_latency -- /path/to/ggml-large-v3-turbo.bin \
  src-tauri/crates/tuic-dictation/fixtures/wake-2610 21
# Use 5 paced instead of 21 for wall-clock audio replay.
```

### Recorded results, 2026-10-31

Apple M4 Max, Metal backend, installed `large-v3-turbo`, explicit English,
production default speech gates. One cold repetition plus 20 warm repetitions
per fixture. Percentiles below exclude the first repetition of each fixture;
p95 is the nearest-rank percentile. CPU is process user + system time from
`getrusage(RUSAGE_SELF)`, not total machine CPU or Metal GPU time.

| Fixture / prefix | Warm n | Wall p50 / p95 | CPU p50 / p95 |
| --- | ---: | ---: | ---: |
| Addressed / 1 s | 20 | 401 / 423 ms | 5.58 / 7.90 ms |
| Unaddressed / 1 s | 20 | 455 / 501 ms | 8.48 / 13.35 ms |
| Unaddressed / 2 s | 20 | 473 / 521 ms | 11.61 / 18.92 ms |
| Unaddressed / 3 s | 20 | 484 / 531 ms | 15.30 / 26.76 ms |
| Unaddressed / 4 s | 20 | 481 / 535 ms | 17.49 / 28.16 ms |

The addressed fixture acknowledged in 21/21 accelerated replays at its first
1,000 ms prefix, which Whisper transcribed as `computer.`. The unaddressed
fixture never acknowledged and was rejected by final admission in 21/21.
Its four extra probes cost 1.894 / 2.084 s wall and 53.1 / 86.8 ms CPU per
utterance (p50 / p95), before the final pass. This is a bounded but material GPU
and capture-loop cost, not free keyword spotting.

Cold model loading took 7.741 s (including Metal library setup); the first
1 s probe took 755 ms. A later process loaded the warmed model in 618 ms.
These are measured startup samples, not percentiles or guarantees.

Five further real-time paced replays per fixture measured audio-file-start to
backend `wake_confirmed`: addressed p50 **1,352 ms**, p95 **1,365 ms**, before
the 4.075 s recording finished. All five unaddressed replays stayed neutral
and were dropped. This excludes microphone buffering, transport and UI polling,
and is not measured wake-word-end-to-display latency. The current UI reads the
phase every fifth 75 ms monitor tick; that display delay was not measured here.

Raw logs and harness snapshots for this run were retained in the task evidence
directory outside the checkout. The example emits JSON rows with individual
probe timings, transcripts, skips and replay outcomes for independent analysis.
