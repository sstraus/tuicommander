# Voice Dictation

TUICommander includes local voice-to-text using Whisper AI. All processing happens on your machine — no cloud services.

## Setup

1. Open **Settings → Services → Voice Dictation**
2. Enable dictation
3. Download a Whisper model (recommended: `large-v3-turbo`, ~1.6 GB)
4. Wait for download to complete (progress shown in UI)
5. Optionally configure language and hotkey

## Usage

**Push-to-talk workflow:**

1. **Hold** the dictation hotkey (default: `F5`) or the mic button in the status bar
2. **Speak** your text
3. **Release** the key/button
4. Transcribed text is inserted into the focused input element (textarea, input, or contenteditable). If no text input has focus, the text falls back to the active terminal PTY. The focus target is captured at key-press time.

The hotkey works globally — even when TUICommander is not focused.

## Models

| Model | Size | Quality |
|-------|------|---------|
| small | ~488 MB | Good |
| small.en | ~488 MB | Good (English-only) |
| large-v2 | ~3.0 GB | Highest accuracy (slow) |
| **large-v3-turbo** | **~1.6 GB** | **Best (recommended, default)** |

Models are downloaded to `<config_dir>/models/` and cached between sessions.

## Languages

Auto-detect (default), or set explicitly:
English, Spanish, French, German, Italian, Portuguese, Dutch, Japanese, Chinese, Korean, Russian.

## Text Corrections

Configure word replacements applied after transcription:

| Spoken | Replaced with |
|--------|---------------|
| "new line" | `\n` |
| "tab" | `\t` |
| "period" | `.` |

Add custom corrections in Settings → Services → Dictation → Corrections.

## Audio Device

Select which microphone to use from the dropdown in dictation settings. Lists all available input devices.

## Platform Notes

- **macOS:** GPU-accelerated transcription via Metal
- **Windows:** GPU-accelerated transcription via Vulkan
- **Linux:** CPU-only (optional CUDA/Vulkan build feature)
- Microphone permission is requested on first use (not at app startup)

## Status Indicators

| Indicator | Meaning |
|-----------|---------|
| Mic button (status bar) | Click/hold to start recording |
| Recording animation | Audio is being captured |
| Processing spinner | Whisper is transcribing |
| Model downloading | Progress bar with percentage |

## Hotkey Configuration

Change the push-to-talk hotkey in Settings → Services → Dictation. The hotkey is registered globally via Tauri's global-shortcut plugin.

Default: `F5`

### Auto-Send

Enable 'Auto-send' in Settings > Services > Dictation to automatically press Enter after the transcribed text is inserted into the terminal. Useful when dictating commands.
