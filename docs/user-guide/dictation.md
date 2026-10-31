# Voice Dictation

TUICommander includes local voice-to-text using Whisper AI. All processing happens on your machine — no cloud services.

## Setup

1. Open **Settings → Voice**
2. Enable dictation
3. Download a Whisper model under **Speech recognition** (recommended: `large-v3-turbo`, ~1.6 GB)
4. Wait for download to complete (progress shown in UI)
5. Optionally configure the hotkey, the microphone and the language

The page has one titled section per job, speech-to-text first and
text-to-speech last: **Dictation** (enable, hotkey, auto-send), **Speech
recognition** (input device, Whisper model, language, and voice tuning at the
bottom), **Auto-Corrections**, **Hands-free conversation**, and **Spoken
replies**. Each section keeps its own advanced settings at its bottom; there is
no shared "Advanced" section.

## Usage

**Push-to-talk workflow:**

1. **Hold** the dictation hotkey (default: `F5`) or the mic button in the status bar
2. **Speak** your text
3. **Release** the key/button
4. Transcribed text is inserted into the focused input element (textarea, input, or contenteditable). If no text input has focus, the text falls back to the active terminal PTY. The focus target is captured at key-press time.

The hotkey works globally — even when TUICommander is not focused.

Pauses do not discard preceding speech from the live preview. Streaming passes
non-zero audio windows through the same speech gates used by the final
transcription; the final pass processes the complete retained recording.

## Hands-Free Conversation

Push-to-talk sends one dictation to whatever has focus. Hands-free is the other
mode: it binds **one** terminal, keeps the microphone open, and sends each
utterance by itself.

To start it on the terminal you are looking at, open the Command Palette and
run **Start hands-free conversation**. While it runs, the same entry reads
**Stop hands-free conversation**. The binding does not follow focus: switch
tabs and the conversation stays with the terminal you started it on. The
action has no default shortcut; bind `toggle-hands-free` in Settings →
Keyboard Shortcuts if you want one. You can also choose a terminal in
**Settings → Voice → Hands-free conversation** and select **Start
conversation**.

Hands-free conversation uses an accent **Start conversation** action and a distinct **Stop conversation** action. The state row shows a coloured indicator with **Running** or **Stopped**, alongside the backend phase when available.

While it runs, the panel shows the state, the bound terminal, where the audio
comes from, and the text that is about to be sent. The dictation hotkey stops
the conversation, and so does **Stop conversation**.

With an activation phrase, **Listening** means the microphone is waiting or
hearing speech that has not yet matched. **Go ahead** means local recognition
matched your phrase during the open utterance. Prefix checks cover its first
four seconds; their speed depends on your model and machine. **Transcribing**
means the utterance has ended and the final pass is running. A partial wake
match is only an acknowledgement: the final transcript still decides whether
the speech is addressed here and can be sent.

You do not need to watch the screen: two short rising notes mean your turn was
sent to the agent, and two softer falling notes mean the activation phrase was
missing and the turn was dropped. Only the device you are talking into plays
them. Turn them off with the **Earcons** setting.

| Control | What it does |
|---------|--------------|
| Activation phrase | When set, only speech that opens with this phrase is sent, and the phrase is removed first. Case, punctuation and accents do not matter, and the usual transcription variants (a joined `Sentimac`, an extra letter as in `Mack`) still count. Leave it empty to send every utterance. The match runs on your machine. The field suggests `computer`: it is distinctive, Whisper transcribes it reliably, and ordinary speech rarely contains it. |
| Hold-back before sending | How long a finished utterance stays visible before it goes to the terminal, so you can stop one you did not mean. With an activation phrase, the effective delay is at least five seconds to include a continuation. It applies to the next conversation, not to the one already running. |
| Earcons | Plays the short sounds described below when a turn is sent or dropped. On by default. |
| Notify model when hands-free changes | Tells the agent that it can answer out loud when the conversation starts, and to go back to text when it ends. |
| Start notice | Shown while **Notify model** is on. The text the agent reads when the conversation starts; the built-in text is shown in grey. Write your own instructions here; leave it empty or press **Reset to default** to send the built-in text. Line breaks are sent as spaces. When the dictation language is set explicitly, TUICommander adds "Reply in &lt;language&gt;." to the notice itself. |

Speech is typed straight into the bound terminal. It does not wait for the
agent to finish: a turn is typed at once, even while the agent is working, the
same as when you type into a busy agent by hand. It does not use the Compose
queue, so commands you queued there keep their order and their timing. A
permission prompt, or text you are typing in the terminal, **keeps** your turn
until it is gone — your speech is never typed into a dialog. A kept turn stays
visible in the hands-free panel, and anything you say meanwhile is added to it,
so it arrives as one message in the order you said it. The conversation ends by
itself when the bound terminal closes or the audio device goes away.

Say the activation phrase on its own to open a short window in which the
following turns need no phrase.
After a phrase addressed with the activation phrase, you can pause and
continue speaking within five seconds without repeating it. The pending text
is sent as one message; the hold-back is at least five seconds while the
phrase is configured, even if you saved a shorter delay.

## Spoken Replies

The agent can answer out loud. By default the voice is a **Microsoft Edge**
neural voice: nothing to download, many languages, a voice list filtered by the
dictation language and a **Listen** button to hear each one. It needs the
internet, and the text of each reply is sent to Microsoft's online speech
service. Offline, the Voice section says so instead of staying silent.

Echo cancellation, hush, volume and levelling work the same for every engine.
Expert mode adds a **Speech engine** setting: **Pocket TTS** runs fully on this
machine (**Settings → Voice → Spoken replies** then lists one shared runtime
library plus a bundle per language, each with its own voices; English, French,
German, Italian, Portuguese and Spanish), and **External command** pipes the
reply text to a program you name. Installations that already chose a Pocket
voice or downloaded Pocket keep Pocket.

Replies are spoken in the language of the conversation. When the dictation
language is set to auto-detect, nothing is spoken until somebody speaks — the
language of the first utterance decides.
The agent is told which language to reply in once: in the start notice when the
language is set explicitly, otherwise on your first spoken turn as
`(reply in Italian)`. Later turns in the same language are sent exactly as you
said them; the hint returns only when you switch language.
The language list marks every language with no bundle as "no spoken
replies". You can still dictate in it, but replies stay silent, and Whisper
expects you to speak that language — choose the one you speak, or Auto-detect.

Talk over a reply and it stops within about a fifth of a second, and what you
say becomes the next turn. Echo cancellation runs on the captured audio, so the reply coming out of
your own speaker does not interrupt itself and does not become a turn.

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

Add custom corrections in Settings → Voice → Auto-Corrections.

## Audio Device

Select which microphone to use from **Input device** in Settings → Voice → Speech recognition. Lists all available input devices.

## Voice Tuning

Settings > Voice > Speech recognition → Voice tuning records a test phrase and shows the result in
the panel — nothing is sent to a terminal. Use it to set two gates that decide
whether captured audio counts as speech.

| Control | What it does |
|---------|--------------|
| Level gate | Audio quieter than this never reaches Whisper. The marker on the meter is the gate; the bar is your live level. |
| Speech confidence gate | Discards a transcript when Whisper itself reports it probably heard no speech. Lower is stricter; 100% turns it off. |

When a recording produces no text, the panel says why ("Rejected: ..."). Without
that line a gate that swallowed your speech and a microphone that captured
nothing look identical.

**Symptom → fix:**

- Random "Grazie" / "Thank you" appear when you are not speaking — usually a
  distant microphone that keeps picking up room noise. Raise the level gate until
  room noise sits below the marker, then lower the speech confidence gate.
- Quiet speech is dropped — lower the level gate, or raise the speech confidence
  gate toward 100%.

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
| Live level meter in the dictation preview | The selected microphone is receiving sound; it appears immediately while recording |
| Processing spinner | Whisper is transcribing |
| Model downloading | Progress bar with percentage |

## Hotkey Configuration

Change the push-to-talk hotkey in Settings → Voice. The hotkey is registered globally via Tauri's global-shortcut plugin.

Default: `F5`

### Auto-Send

Enable **Auto-send** in Settings → Voice to submit the terminal input after transcription. In terminal **Chat** mode, dictation inserts at the captured Compose selection and uses the normal Compose Send action exactly once, including draft cleanup after success. This works with Compose or transcript focus. With Auto-send off, the text stays in Compose for review. Other editors, search fields and form inputs never submit automatically.

Mobile **Dictate into draft** uses browser speech recognition and stays review-only; tap Send when ready. The separate hands-free conversation sends through its existing server-owned path.

## AI Chat voice targets

In Settings → Voice → Hands-free conversation, choose an **AI Chat** entry in **Target**, then select **Start conversation**. Attached chats are available whether the panel is docked or detached. Spoken turns use the same ACP prompt queue as typed turns. The chat can answer through the existing voice tool and speech engine. Changing the voice target stops the previous conversation; select Start to bind the new target. Closing the bound tab or its last panel view, selecting another chat, or stopping ego also stops its voice binding. Moving the panel between docked and detached views does not change the target.
