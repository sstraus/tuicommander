# Voice Dictation

**Module:** `src-tauri/src/dictation/`

Local voice-to-text using Whisper with Metal acceleration on macOS. Push-to-talk workflow with streaming partial results: hold hotkey to record, see partial transcriptions in real-time, release to finalize.

## Module Structure

| File | Purpose |
|------|---------|
| `mod.rs` | `DictationState` — shared state for all dictation operations |
| `audio.rs` | Audio capture from microphone via CPAL (VecDeque ring buffer) |
| `commands.rs` | Tauri command handlers |
| `model.rs` | Whisper model download and management |
| `transcribe.rs` | `Transcriber` trait + `WhisperTranscriber` implementation via whisper-rs |
| `streaming.rs` | Streaming transcription loop with adaptive windows and VAD |
| `vad.rs` | Voice Activity Detection (energy-based, ported from whisper.cpp) |
| `corrections.rs` | Post-processing text corrections |

## Tauri Commands

### Recording

| Command | Description |
|---------|-------------|
| `start_dictation()` | Start recording + streaming transcription |
| `stop_dictation_and_transcribe()` | Stop streaming, final pass on full captured audio, return `TranscribeResponse { text, skip_reason, duration_s }` |
| `inject_text(text)` | Apply corrections to text (called after transcription) |

### Tauri Events

| Event | Direction | Payload |
|-------|-----------|---------|
| `dictation-partial` | Rust → Frontend | `String` — partial transcription text |
| `dictation-download-progress` | Rust → Frontend | `{ downloaded, total, percent }` |

### Model Management

| Command | Description |
|---------|-------------|
| `get_model_info()` | List available Whisper models with download status |
| `download_whisper_model(model_name)` | Download model (emits progress events) |
| `delete_whisper_model(model_name)` | Delete a downloaded model |

### Configuration

| Command | Description |
|---------|-------------|
| `get_dictation_status()` | Model status, recording state, processing state |
| `get_dictation_config()` | Load dictation configuration |
| `set_dictation_config(config)` | Save dictation configuration |
| `get_correction_map()` | Load text correction dictionary |
| `set_correction_map(map)` | Save text correction dictionary |
| `list_audio_devices()` | List available audio input devices |

## DictationState

```rust
pub struct DictationState {
    pub audio: Mutex<Option<AudioCapture>>,
    pub active_model: Mutex<Option<String>>,
    pub corrections: Mutex<TextCorrector>,
    pub recording: AtomicBool,
    pub processing: AtomicBool,
    pub streaming: Mutex<Option<StreamingSession>>,
    pub transcriber_arc: Mutex<Option<Arc<dyn Transcriber>>>,
    pub accumulated_partials: Arc<Mutex<String>>,
}
```

Managed as Tauri state alongside `AppState`.

## Transcriber Trait

```rust
pub trait Transcriber: Send + Sync {
    fn transcribe(&self, audio: &[f32], language: Option<&str>) -> Result<TranscribeResult, String>;
}
```

`WhisperTranscriber` implements this trait using whisper-rs. The trait abstraction enables mock implementations for testing without requiring a Whisper model.

## Recording Guard (TOCTOU)

`start_dictation()` uses `compare_exchange(false, true, AcqRel, Acquire)` on the `recording` flag to prevent TOCTOU races from concurrent IPC calls. If two calls arrive simultaneously, only the first succeeds; the second returns `"Already recording"`. A drop guard resets `recording = false` on any early error return.

## Streaming Architecture

```
User holds hotkey
    │
    ▼
start_dictation()
    ├── Load/reuse WhisperTranscriber (Arc-wrapped)
    ├── Start CPAL AudioCapture → VecDeque<f32> buffer
    ├── Start StreamingSession (background thread)
    │       │
    │       ├── Poll audio buffer (50ms interval)
    │       ├── Accumulate in step_buf
    │       ├── When step_buf >= window size:
    │       │       ├── VAD check → skip if silence
    │       │       ├── Build window: [keep_tail | step_buf]
    │       │       ├── whisper_full(window)
    │       │       └── Send partial via mpsc::channel
    │       └── Adaptive growth: 1.5s → 2.0s → 2.5s → 3.0s (max)
    │
    ├── Spawn event forwarder thread
    │       └── mpsc::Receiver → emit("dictation-partial")
    │
    └── Set recording = true
    │
User releases hotkey
    │
    ▼
stop_dictation_and_transcribe()  [async]
    ├── Set recording=false, processing=true (synchronous, UI updates immediately)
    ├── Stop cpal stream (buffer preserved)
    ├── Signal StreamingSession stop → join thread
    ├── Collect ALL audio (processed + unprocessed + capture buffer remainder)
    ├── spawn_blocking: Final transcription on full captured audio (if >= 0.5s)
    │   ├── ProcessingGuard (drop guard) clears processing=false on completion/panic
    │   ├── Apply text corrections
    │   └── Return TranscribeResponse
    └── Return TranscribeResponse { text, skip_reason, duration_s }
    │
    ▼
Frontend injects text into focus target
```

## VAD (Voice Activity Detection)

Ported from whisper.cpp `common.cpp` `vad_simple()`:

- **Algorithm:** Compare absolute energy of last `last_ms` (1000ms) vs entire buffer
- **High-pass filter:** First-order RC at 100Hz removes ambient noise (HVAC, fans)
- **Threshold:** `vad_thold = 0.6` — if `energy_last / energy_all < 0.6`, silence detected
- **Relative:** Microphone gain doesn't affect detection (ratio-based)

## Streaming Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| `INITIAL_STEP_MS` | 1500 | First window size (fast first partial) |
| `MAX_STEP_MS` | 3000 | Maximum window size |
| `STEP_GROWTH_MS` | 500 | Growth per iteration |
| `KEEP_MS` | 200 | Overlap from previous window |
| `POLL_INTERVAL_MS` | 50 | Audio buffer polling interval |
| `MAX_BUFFER_S` | 30 | Force flush on very long recordings |
| `VAD_THRESHOLD` | 0.6 | Energy ratio threshold |
| `VAD_FREQ_THRESHOLD` | 100.0 | High-pass cutoff Hz |

## Audio Pipeline

```
Microphone → CPAL callback → try_lock() → VecDeque<f32> ← drain_samples() ← StreamingSession
                                                                                    │
                                                                              whisper_full()
                                                                                    │
                                                                              mpsc::channel
                                                                                    │
                                                                         event forwarder thread
                                                                                    │
                                                                          "dictation-partial"
                                                                                    │
                                                                         DictationToast (UI)
```

Key design: `try_lock()` in the CPAL callback ensures the real-time audio thread **never blocks**. On contention, samples are silently dropped — acceptable for dictation at 16kHz mono (~64KB/s).

## Audio Resampling

`process_audio_chunk()` converts raw microphone input to the 16kHz mono f32 PCM format required by Whisper:

1. **I16 → F32 conversion** — If the audio device provides `I16` samples, they are normalized to `[-1.0, 1.0]` by dividing by `i16::MAX`.
2. **Stereo → mono** — Multi-channel frames are averaged (`frame.sum() / channels`).
3. **Nearest-neighbor resampling to 16kHz** — For sample rates other than 16kHz (e.g., 48kHz), the output length is calculated as `input_len * (16000 / sample_rate)` and samples are picked by index mapping (`src_idx = i / ratio`).

Pre-allocated scratch buffers (`mono_buf`, `resample_buf`) are captured in the CPAL closure to avoid per-callback heap allocations. The buffer is capped at 30 seconds (480k samples) to prevent unbounded growth.

## Model Storage

Models stored in: `<config_dir>/models/`

Available models (GGML format):

| Model | Size | Quality |
|-------|------|---------|
| `tiny` | ~75 MB | Low |
| `base` | ~140 MB | Fair |
| `small` | ~460 MB | Good |
| `medium` | ~1.5 GB | Very good |
| `large-v3-turbo` | ~1.6 GB | Best (recommended) |

## Text Corrections

User-configurable dictionary for post-processing:

```json
{
  "new line": "\n",
  "tab": "\t",
  "period": ".",
  "comma": ","
}
```

Stored in dictation config. Applied after transcription, before injecting into terminal.

## Platform Notes

- **macOS:** Metal acceleration via whisper-rs (GPU-accelerated)
- **Linux/Windows:** CPU-only (Metal feature conditionally compiled)
- Microphone permissions deferred until first use (avoids startup permission popup)

## Microphone Permission Detection

**Module:** `src-tauri/src/dictation/permission.rs`

On macOS, microphone access is gated by the TCC (Transparency, Consent, and Control) framework. The `MicPermission` enum tracks the current state:

| State | Meaning |
|-------|---------|
| `NotDetermined` | User hasn't been asked yet — system will prompt on first access |
| `Authorized` | User granted access |
| `Denied` | User denied access — must be changed in System Settings |
| `Restricted` | System policy prevents access (e.g., MDM) |

**API:**
- `MicPermission::check()` — queries `AVCaptureDevice` authorization status via Objective-C bridge (`objc2`, `objc2-av-foundation`)
- `MicPermission::open_settings()` — opens macOS System Settings at the Privacy & Security > Microphone pane

**Platform behavior:**
- **macOS:** Full TCC integration via AVFoundation
- **Linux/Windows:** Always returns `Authorized` (no TCC framework)
