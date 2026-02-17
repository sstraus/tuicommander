# Voice Dictation

**Module:** `src-tauri/src/dictation/`

Local voice-to-text using Whisper with Metal acceleration on macOS. Push-to-talk workflow: hold hotkey to record, release to transcribe.

## Module Structure

| File | Purpose |
|------|---------|
| `mod.rs` | `DictationState` — shared state for all dictation operations |
| `audio.rs` | Audio capture from microphone via CPAL |
| `commands.rs` | Tauri command handlers |
| `model.rs` | Whisper model download and management |
| `transcribe.rs` | Whisper transcription via whisper-rs |
| `corrections.rs` | Post-processing text corrections |

## Tauri Commands

### Recording

| Command | Description |
|---------|-------------|
| `start_dictation()` | Start recording audio from selected device |
| `stop_dictation_and_transcribe()` | Stop recording, transcribe with Whisper, return text |
| `inject_text(text)` | Apply corrections to text (called after transcription) |

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
    pub transcriber: Mutex<Option<WhisperTranscriber>>,
    pub active_model: Mutex<Option<String>>,
    pub corrections: Mutex<TextCorrector>,
    pub recording: AtomicBool,
    pub processing: AtomicBool,
}
```

Managed as Tauri state alongside `AppState`.

## Workflow

```
User holds hotkey (Cmd+Shift+D)
    │
    ▼
start_dictation()
    ├── Select audio device
    ├── Start CPAL audio capture
    └── Set recording = true
    │
User releases hotkey
    │
    ▼
stop_dictation_and_transcribe()
    ├── Stop audio capture
    ├── Set recording = false, processing = true
    ├── Load Whisper model (lazy, cached)
    ├── Transcribe audio buffer
    ├── Set processing = false
    └── Return raw text
    │
    ▼
inject_text(text)
    ├── Apply correction map (e.g., "new line" → "\n")
    └── Return corrected text
    │
    ▼
Frontend writes corrected text to active PTY
```

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
