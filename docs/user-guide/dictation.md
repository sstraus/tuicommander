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
4. Text is transcribed and typed into the active terminal

The hotkey works globally — even when TUICommander is not focused.

## Models

| Model | Size | Quality |
|-------|------|---------|
| tiny | ~75 MB | Low (fast, inaccurate) |
| base | ~140 MB | Fair |
| small | ~460 MB | Good |
| medium | ~1.5 GB | Very good |
| **large-v3-turbo** | **~1.6 GB** | **Best (recommended)** |

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
- **Linux/Windows:** CPU-only transcription
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
