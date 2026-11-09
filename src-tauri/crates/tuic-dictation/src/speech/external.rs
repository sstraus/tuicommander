//! A speech engine the user supplies, behind the same port as the bundled one.
//!
//! The bundled Pocket TTS bundles cover six languages. Japanese, Chinese,
//! Korean and Russian are not among them, and no engine stays the best one for
//! long. This adapter makes the engine a setting instead of a product limit:
//! TUICommander hands a command the text and a path to write, and reads the
//! audio back.
//!
//! The two adapters know nothing about each other. Both implement
//! [`Speech`](super::Speech); whoever constructs one decides which.
//!
//! ## The template
//!
//! The configured command is argv, not a shell line. Three markers say where
//! the pieces go, and each is replaced inside the argument that holds it:
//!
//! | Marker | Meaning |
//! |---|---|
//! | `{out}` | the file the command must write. **Required** — it is how the audio comes back |
//! | `{text}` | the text to speak. Optional: with no `{text}` anywhere, the text is written to the command's stdin |
//! | `{voice}` | the voice identifier, as the engine spells it |
//!
//! ```text
//! ["piper", "--model", "ja_JP-test-medium.onnx", "--output_file", "{out}"]
//! ["say", "-v", "{voice}", "-o", "{out}", "--data-format=LEF32@22050", "{text}"]
//! ```
//!
//! The first reads its text on stdin, the second takes it as an argument, and
//! neither had to be anticipated here.
//!
//! ## No shell
//!
//! The command is spawned directly. The text never becomes part of a command
//! line a shell parses, so a transcript containing `;` or a backtick is an
//! ordinary argument rather than a second command. What the user writes in the
//! template is their own business: **it runs as the user, with the user's
//! environment and permissions**, exactly like a shell alias they wrote. That
//! is the point of the feature and it is stated in the documentation rather
//! than quietly restricted here.
//!
//! ## Audio
//!
//! The command must write a RIFF/WAVE file: 16-bit PCM or 32-bit float, any
//! sample rate, any channel count (mixed down to mono). Those two encodings
//! are what speech CLIs emit; a third is a few lines here the day one turns
//! up, and until then an unsupported file is named in the error rather than
//! guessed at.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use super::{Speech, SpeechAudio, SpeechCancel, SpeechError, budget_seconds};

type Result<T> = std::result::Result<T, SpeechError>;

const TEXT_MARKER: &str = "{text}";
const OUT_MARKER: &str = "{out}";
const VOICE_MARKER: &str = "{voice}";

/// How often the wait loop looks up from the child to check the clock and the
/// cancel flag. Short enough that an abandoned reply stops while the user is
/// still drawing breath, long enough not to spin a core.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// The smallest timeout any request gets, regardless of how short the text is.
/// A cold engine loads a model before it says the first word, and punishing a
/// one-word reply for that would make the feature useless on the first run.
const TIMEOUT_FLOOR: Duration = Duration::from_secs(30);

/// Multiples of the audio budget a command may spend producing it. An engine
/// slower than a quarter of real time, after the floor above, is a problem the
/// user needs to be told about rather than waited out.
const TIMEOUT_FACTOR: f32 = 4.0;

/// How much of the command's stderr travels in the error. Enough for the line
/// an engine prints when it cannot find its model; not enough to put a log
/// file in a toast.
const STDERR_TAIL_BYTES: usize = 400;

/// A user-configured command used as a speech engine.
#[derive(Debug)]
pub struct ExternalSpeech {
    /// argv, markers included. Validated once, at construction.
    template: Vec<String>,
    /// Set only by [`ExternalSpeech::with_timeout`]; otherwise the timeout is
    /// derived from the text.
    timeout: Option<Duration>,
}

impl ExternalSpeech {
    /// Check the template before anything is spawned.
    ///
    /// A template that cannot work is a settings problem, so it is reported as
    /// [`SpeechError::ModelUnavailable`] — the same shape as a missing model,
    /// because it is the same situation from the user's side: this engine is
    /// not usable until something is fixed.
    pub fn new(template: Vec<String>) -> Result<Self> {
        let Some(program) = template.first() else {
            return Err(unusable("no command is configured"));
        };
        if program.trim().is_empty() {
            return Err(unusable("the command is blank"));
        }
        if contains_marker(program) {
            return Err(unusable(&format!(
                "the program itself contains a marker ({program}); markers belong in the arguments"
            )));
        }
        if !template.iter().any(|arg| arg.contains(OUT_MARKER)) {
            return Err(unusable(
                "no {out} in the command; without it there is nowhere for the audio to come back from",
            ));
        }
        Ok(Self {
            template,
            timeout: None,
        })
    }

    /// Override the derived timeout.
    ///
    /// The tests use this so a timeout can be proven in a second rather than
    /// in [`TIMEOUT_FLOOR`].
    #[cfg(test)]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    fn timeout_for(&self, text: &str) -> Duration {
        self.timeout.unwrap_or_else(|| {
            Duration::from_secs_f32(budget_seconds(text) * TIMEOUT_FACTOR).max(TIMEOUT_FLOOR)
        })
    }

    /// Does the template take the text as an argument, or on stdin?
    fn text_is_an_argument(&self) -> bool {
        self.template.iter().any(|arg| arg.contains(TEXT_MARKER))
    }

    /// Fill the markers in.
    fn argv(&self, text: &str, voice: &str, out: &Path) -> Result<Vec<String>> {
        let wants_voice = self.template.iter().any(|arg| arg.contains(VOICE_MARKER));
        if wants_voice && !is_plain_identifier(voice) {
            // The voice reaches argv. A template that asks for it gets a name,
            // not a second argument smuggled in through a space.
            return Err(SpeechError::UnknownVoice(voice.to_string()));
        }
        let out = out.to_string_lossy();
        Ok(self
            .template
            .iter()
            .map(|arg| {
                arg.replace(OUT_MARKER, &out)
                    .replace(VOICE_MARKER, voice)
                    .replace(TEXT_MARKER, text)
            })
            .collect())
    }
}

impl Speech for ExternalSpeech {
    fn synthesize(&self, text: &str, voice: &str, cancel: &SpeechCancel) -> Result<SpeechAudio> {
        if cancel.is_cancelled() {
            return Err(SpeechError::Cancelled);
        }
        let workspace = tempfile::Builder::new()
            .prefix("tuic-speech-")
            .tempdir()
            .map_err(|e| {
                SpeechError::Failed(format!("no temporary directory for the audio: {e}"))
            })?;
        let out = workspace.path().join("speech.wav");
        let argv = self.argv(text, voice, &out)?;
        let program = argv[0].clone();

        let stdin = if self.text_is_an_argument() {
            Stdio::null()
        } else {
            Stdio::piped()
        };
        let mut command = Command::new(&argv[0]);
        command
            .args(&argv[1..])
            .stdin(stdin)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn().map_err(|e| SpeechError::ModelUnavailable {
            what: program.clone(),
            reason: e.to_string(),
        })?;

        if !self.text_is_an_argument()
            && let Some(mut pipe) = child.stdin.take()
            && let Err(e) = pipe.write_all(text.as_bytes()).and_then(|()| pipe.flush())
        {
            // A command that closes stdin without reading is not necessarily
            // broken — it may already have what it needs — so this is only
            // fatal if the command then fails too. Record it for that message.
            tracing::debug!("speech: {program} did not take the text on stdin: {e}");
        }

        // Drained on its own thread. A command that writes more than a pipe
        // buffer of diagnostics would otherwise block forever on the write,
        // and we would report a timeout for a command that was trying to tell
        // us what was wrong.
        let stderr = child.stderr.take().map(|mut pipe| {
            std::thread::spawn(move || {
                let mut buffer = String::new();
                let _ = pipe.read_to_string(&mut buffer);
                buffer
            })
        });

        let status = wait_for(&mut child, cancel, self.timeout_for(text), &program)?;
        let stderr = stderr
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default();

        if !status.success() {
            return Err(SpeechError::Failed(format!(
                "{program} {}{}",
                exit_description(&status),
                stderr_tail(&stderr),
            )));
        }

        let bytes = std::fs::read(&out).map_err(|e| {
            SpeechError::Failed(format!(
                "{program} reported success but left no readable audio at {}: {e}{}",
                out.display(),
                stderr_tail(&stderr),
            ))
        })?;
        if bytes.is_empty() {
            return Err(SpeechError::Failed(format!(
                "{program} reported success but wrote an empty file{}",
                stderr_tail(&stderr),
            )));
        }
        let audio =
            decode_wav(&bytes).map_err(|e| SpeechError::Failed(format!("{program} wrote {e}")))?;

        let budget = budget_seconds(text);
        if audio.duration_seconds() > budget {
            // The same ceiling the bundled adapter stops itself at. An engine
            // that reads a sentence and returns ten minutes is misconfigured —
            // wrong model, wrong text, a prompt echoed back — and playing it is
            // worse than saying so.
            return Err(SpeechError::Runaway {
                budget_seconds: budget,
            });
        }
        Ok(audio)
    }
}

/// Wait for the command, watching the clock and the cancel flag.
fn wait_for(
    child: &mut Child,
    cancel: &SpeechCancel,
    timeout: Duration,
    program: &str,
) -> Result<std::process::ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(e) => {
                stop(child);
                return Err(SpeechError::Failed(format!(
                    "{program} could not be waited on: {e}"
                )));
            }
        }
        if cancel.is_cancelled() {
            stop(child);
            return Err(SpeechError::Cancelled);
        }
        if Instant::now() >= deadline {
            stop(child);
            return Err(SpeechError::Failed(format!(
                "{program} did not finish within {:.0}s and was stopped",
                timeout.as_secs_f32()
            )));
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Stop the owned command tree, then reap the direct child. Killing only a
/// shell wrapper leaves its engine running (and holding stderr open).
fn stop(child: &mut Child) {
    #[cfg(unix)]
    {
        // SAFETY: synthesize creates a dedicated group whose ID is this child's
        // PID. No other application process is part of that owned group.
        unsafe { libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL) };
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        let _ = Command::new(Path::new(&root).join("System32").join("taskkill.exe"))
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    // Also covers a failed tree kill; always collect the direct child.
    let _ = child.kill();
    let _ = child.wait();
}

fn exit_description(status: &std::process::ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exited with status {code}"),
        None => "was killed by a signal".to_string(),
    }
}

/// The tail rather than the head: a command that fails after printing progress
/// says what went wrong in its last lines.
fn stderr_tail(stderr: &str) -> String {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let tail = match trimmed.char_indices().nth_back(STDERR_TAIL_BYTES) {
        Some((at, _)) => format!("…{}", &trimmed[at..]),
        None => trimmed.to_string(),
    };
    format!(": {}", tail.replace('\n', " "))
}

fn unusable(reason: &str) -> SpeechError {
    SpeechError::ModelUnavailable {
        what: "speech command".to_string(),
        reason: reason.to_string(),
    }
}

fn contains_marker(arg: &str) -> bool {
    arg.contains(TEXT_MARKER) || arg.contains(OUT_MARKER) || arg.contains(VOICE_MARKER)
}

fn is_plain_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

// ---------------------------------------------------------------------------
// WAV
// ---------------------------------------------------------------------------

/// What a file that is not usable audio is wrong about. Every variant
/// `Display`s as the end of "…wrote {this}", so the caller's message names the
/// command and this names the defect.
#[derive(Debug, PartialEq)]
enum WavError {
    NotRiff,
    Truncated,
    NoFormat,
    NoData,
    Unsupported { format: u16, bits: u16 },
    Empty,
    Silent(&'static str),
}

impl std::fmt::Display for WavError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotRiff => write!(f, "something that is not a RIFF/WAVE file"),
            Self::Truncated => write!(f, "a truncated WAVE file"),
            Self::NoFormat => write!(f, "a WAVE file with no fmt chunk"),
            Self::NoData => write!(f, "a WAVE file with no data chunk"),
            Self::Unsupported { format, bits } => write!(
                f,
                "a WAVE file in an encoding this build cannot read (format {format}, {bits}-bit); \
                 16-bit PCM and 32-bit float are supported"
            ),
            Self::Empty => write!(f, "a WAVE file with no samples"),
            Self::Silent(what) => write!(f, "a WAVE file declaring {what}"),
        }
    }
}

/// Minimal RIFF/WAVE reader.
///
/// Deliberately not a dependency: what is needed is the two encodings above out
/// of a chunk walk, and a decoder crate would bring a format matcher and a
/// resampler to do it.
fn decode_wav(bytes: &[u8]) -> std::result::Result<SpeechAudio, WavError> {
    if bytes.len() < 12 {
        return Err(WavError::Truncated);
    }
    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(WavError::NotRiff);
    }

    let mut format: Option<WavFormat> = None;
    let mut data: Option<&[u8]> = None;
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32_at(bytes, at + 4) as usize;
        let body_at = at + 8;
        // The last chunk of a streamed file is often written with a size the
        // writer never went back to fix. Take what is there rather than
        // rejecting a file that holds real audio.
        let body_end = body_at.saturating_add(size).min(bytes.len());
        let body = &bytes[body_at.min(bytes.len())..body_end];
        match id {
            b"fmt " => format = Some(parse_format(body)?),
            b"data" => data = Some(body),
            _ => {}
        }
        // Chunks are word-aligned; an odd size is followed by a pad byte.
        at = body_at + size + (size % 2);
    }

    let format = format.ok_or(WavError::NoFormat)?;
    let data = data.ok_or(WavError::NoData)?;
    if format.channels == 0 {
        return Err(WavError::Silent("no channels"));
    }
    if format.sample_rate == 0 {
        return Err(WavError::Silent("a sample rate of zero"));
    }

    let interleaved = match (format.encoding, format.bits) {
        (1, 16) => data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|s| f32::from(i16::from_le_bytes(*s)) / 32768.0)
            .collect::<Vec<f32>>(),
        (3, 32) => data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|s| f32::from_le_bytes(*s))
            .collect(),
        (encoding, bits) => {
            return Err(WavError::Unsupported {
                format: encoding,
                bits,
            });
        }
    };
    if interleaved.is_empty() {
        return Err(WavError::Empty);
    }

    let channels = usize::from(format.channels);
    let samples = if channels == 1 {
        interleaved
    } else {
        // Mixed down rather than refused: the port carries mono, and an engine
        // that only speaks in stereo is not a reason to have no voice.
        interleaved
            .chunks_exact(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    };
    if samples.is_empty() {
        return Err(WavError::Empty);
    }
    Ok(SpeechAudio {
        samples,
        sample_rate: format.sample_rate,
    })
}

struct WavFormat {
    encoding: u16,
    channels: u16,
    sample_rate: u32,
    bits: u16,
}

fn parse_format(body: &[u8]) -> std::result::Result<WavFormat, WavError> {
    if body.len() < 16 {
        return Err(WavError::Truncated);
    }
    let mut encoding = u16_at(body, 0);
    // WAVE_FORMAT_EXTENSIBLE says nothing itself; the real encoding is the
    // first field of the subformat GUID. Engines that write more than two
    // channels, or that write through a library that always extends, land
    // here with ordinary PCM inside.
    if encoding == 0xFFFE && body.len() >= 26 {
        encoding = u16_at(body, 24);
    }
    Ok(WavFormat {
        encoding,
        channels: u16_at(body, 2),
        sample_rate: u32_at(body, 4),
        bits: u16_at(body, 14),
    })
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuic_test_support::{
        chain, copy_file_script, fail_with_stderr_script, host_shell, sleep_script, touch_script,
    };

    /// A RIFF/WAVE file, built the way an engine would write one.
    fn wav(encoding: u16, bits: u16, channels: u16, rate: u32, payload: &[u8]) -> Vec<u8> {
        let block_align = channels * bits / 8;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + payload.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&encoding.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * u32::from(block_align)).to_le_bytes());
        out.extend_from_slice(&block_align.to_le_bytes());
        out.extend_from_slice(&bits.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(payload);
        out
    }

    fn pcm16(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    /// One second of a 16 kHz tone-ish ramp, as 16-bit PCM — long enough to be
    /// real audio, short enough to stay inside any budget.
    fn one_second_wav() -> Vec<u8> {
        let samples: Vec<i16> = (0..16_000).map(|n| ((n % 400) * 50) as i16).collect();
        wav(1, 16, 1, 16_000, &pcm16(&samples))
    }

    fn template(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_string()).collect()
    }

    /// A command that copies `source` to wherever the port asks for the audio.
    fn copying_engine(source: &Path) -> Vec<String> {
        let (shell, flag) = host_shell();
        template(&[
            shell,
            flag,
            &copy_file_script(&source.to_string_lossy(), OUT_MARKER),
        ])
    }

    #[test]
    fn a_command_with_nowhere_to_write_is_rejected_before_it_runs() {
        // Without {out} there is no way to get audio back, so spawning it could
        // only ever waste the user's time and then fail.
        let error = ExternalSpeech::new(template(&["say", "{text}"])).unwrap_err();
        let SpeechError::ModelUnavailable { what, reason } = error else {
            panic!("expected a setup error, got {error:?}");
        };
        assert_eq!(what, "speech command");
        assert!(
            reason.contains("{out}"),
            "the reason must name what is missing: {reason}"
        );
    }

    #[test]
    fn an_unconfigured_engine_says_so_rather_than_spawning_nothing() {
        let error = ExternalSpeech::new(Vec::new()).unwrap_err();
        assert!(
            matches!(error, SpeechError::ModelUnavailable { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn a_marker_in_the_program_is_a_mistake_worth_naming() {
        // `{text}` as the program would try to execute the user's sentence.
        let error = ExternalSpeech::new(template(&["{text}", "{out}"])).unwrap_err();
        let SpeechError::ModelUnavailable { reason, .. } = error else {
            panic!("expected a setup error");
        };
        assert!(
            reason.contains("markers belong in the arguments"),
            "{reason}"
        );
    }

    #[test]
    fn the_markers_are_filled_in_where_the_user_put_them() {
        // Including inside a longer argument: an engine that spells its output
        // as `--output=PATH` must be reachable.
        let engine = ExternalSpeech::new(template(&[
            "piper",
            "--voice={voice}",
            "--output={out}",
            "{text}",
        ]))
        .unwrap();

        let argv = engine
            .argv("Ciao", "alba", Path::new("/tmp/x/speech.wav"))
            .unwrap();

        assert_eq!(
            argv,
            vec![
                "piper".to_string(),
                "--voice=alba".to_string(),
                "--output=/tmp/x/speech.wav".to_string(),
                "Ciao".to_string(),
            ]
        );
    }

    #[test]
    fn text_that_looks_like_a_command_stays_one_argument() {
        // No shell is involved, so this is only about the substitution not
        // splitting. A transcript really can contain a semicolon.
        let engine = ExternalSpeech::new(template(&["say", "-o", "{out}", "{text}"])).unwrap();

        let argv = engine
            .argv("ferma; rm -rf ~", "any", Path::new("/tmp/speech.wav"))
            .unwrap();

        assert_eq!(argv.len(), 4);
        assert_eq!(argv[3], "ferma; rm -rf ~");
    }

    #[test]
    fn a_voice_the_template_asks_for_is_a_name_not_another_argument() {
        let engine =
            ExternalSpeech::new(template(&["say", "-v", "{voice}", "-o", "{out}"])).unwrap();

        let error = engine
            .argv("Ciao", "alba --rate 500", Path::new("/tmp/speech.wav"))
            .unwrap_err();

        assert_eq!(error, SpeechError::UnknownVoice("alba --rate 500".into()));
    }

    #[test]
    fn a_template_that_never_mentions_a_voice_does_not_care_what_it_is_called() {
        // An engine with one voice takes whatever the caller passes, including
        // the empty string a UI with no voice picker would send.
        let engine = ExternalSpeech::new(template(&["piper", "--output={out}"])).unwrap();

        assert!(
            engine
                .argv("Ciao", "", Path::new("/tmp/speech.wav"))
                .is_ok()
        );
    }

    #[test]
    fn the_text_goes_to_stdin_exactly_when_the_template_does_not_ask_for_it() {
        let argument = ExternalSpeech::new(template(&["say", "-o", "{out}", "{text}"])).unwrap();
        let stdin = ExternalSpeech::new(template(&["piper", "--output={out}"])).unwrap();

        assert!(argument.text_is_an_argument());
        assert!(!stdin.text_is_an_argument());
    }

    #[test]
    fn the_timeout_grows_with_the_text_but_never_below_the_floor() {
        let engine = ExternalSpeech::new(template(&["say", "-o", "{out}"])).unwrap();

        // A short reply still gets the floor: the engine may be loading a model.
        assert_eq!(engine.timeout_for("Sì."), TIMEOUT_FLOOR);

        let long = "a".repeat(4_000);
        assert!(
            engine.timeout_for(&long) > TIMEOUT_FLOOR,
            "a long text must be allowed more time than the floor"
        );
    }

    #[test]
    fn sixteen_bit_audio_arrives_as_the_samples_that_were_written() {
        let bytes = wav(1, 16, 1, 22_050, &pcm16(&[0, 16_384, -16_384, 32_767]));

        let audio = decode_wav(&bytes).unwrap();

        assert_eq!(audio.sample_rate, 22_050);
        assert_eq!(audio.samples.len(), 4);
        assert!((audio.samples[1] - 0.5).abs() < 1e-4, "{:?}", audio.samples);
        assert!((audio.samples[2] + 0.5).abs() < 1e-4, "{:?}", audio.samples);
    }

    #[test]
    fn float_audio_is_taken_as_written_rather_than_rescaled() {
        let payload: Vec<u8> = [0.0f32, 0.25, -0.75]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();

        let audio = decode_wav(&wav(3, 32, 1, 24_000, &payload)).unwrap();

        assert_eq!(audio.samples, vec![0.0, 0.25, -0.75]);
        assert_eq!(audio.sample_rate, 24_000);
    }

    #[test]
    fn a_stereo_engine_is_mixed_down_rather_than_refused() {
        // The port carries mono. Left and right averaged is not a hi-fi
        // decision, it is the difference between a voice and an error.
        let bytes = wav(1, 16, 2, 16_000, &pcm16(&[32_767, -32_768, 16_384, 16_384]));

        let audio = decode_wav(&bytes).unwrap();

        assert_eq!(audio.samples.len(), 2);
        assert!(audio.samples[0].abs() < 1e-4, "opposite channels cancel");
        assert!((audio.samples[1] - 0.5).abs() < 1e-4, "{:?}", audio.samples);
    }

    #[test]
    fn an_extensible_header_is_read_through_to_the_encoding_inside_it() {
        // 0xFFFE says "look at the GUID"; the audio is ordinary 16-bit PCM.
        let payload = pcm16(&[16_384, -16_384]);
        let mut bytes = wav(1, 16, 1, 16_000, &payload);
        // Rewrite the fmt chunk as extensible: 40 bytes, encoding 0xFFFE, and
        // the real encoding at the head of the subformat GUID.
        let mut extended = bytes[..20].to_vec();
        extended[16..20].copy_from_slice(&40u32.to_le_bytes());
        extended.extend_from_slice(&0xFFFEu16.to_le_bytes());
        extended.extend_from_slice(&bytes[22..36]);
        extended.extend_from_slice(&22u16.to_le_bytes()); // cbSize
        extended.extend_from_slice(&16u16.to_le_bytes()); // valid bits
        extended.extend_from_slice(&0u32.to_le_bytes()); // channel mask
        extended.extend_from_slice(&1u16.to_le_bytes()); // PCM, head of the GUID
        extended.extend_from_slice(&[0u8; 14]);
        extended.extend_from_slice(&bytes[36..]);
        bytes = extended;

        let audio = decode_wav(&bytes).unwrap();

        assert_eq!(audio.samples.len(), 2);
        assert!((audio.samples[0] - 0.5).abs() < 1e-4, "{:?}", audio.samples);
    }

    #[test]
    fn a_file_that_is_not_audio_is_named_rather_than_played() {
        assert_eq!(
            decode_wav(b"<!DOCTYPE html>").unwrap_err(),
            WavError::NotRiff
        );
        assert_eq!(decode_wav(b"RIF").unwrap_err(), WavError::Truncated);
    }

    #[test]
    fn an_encoding_this_build_cannot_read_says_which_one_it_was() {
        // 24-bit PCM is real and unsupported; the message has to be actionable
        // rather than "failed".
        let error = decode_wav(&wav(1, 24, 1, 16_000, &[0; 9])).unwrap_err();

        assert_eq!(
            error,
            WavError::Unsupported {
                format: 1,
                bits: 24
            }
        );
        assert!(error.to_string().contains("24-bit"), "{error}");
    }

    #[test]
    fn a_header_with_no_samples_behind_it_is_not_silence_to_play() {
        assert_eq!(
            decode_wav(&wav(1, 16, 1, 16_000, &[])).unwrap_err(),
            WavError::Empty
        );
        assert_eq!(
            decode_wav(&wav(1, 16, 0, 16_000, &pcm16(&[1, 2]))).unwrap_err(),
            WavError::Silent("no channels")
        );
        assert_eq!(
            decode_wav(&wav(1, 16, 1, 0, &pcm16(&[1, 2]))).unwrap_err(),
            WavError::Silent("a sample rate of zero")
        );
    }

    #[test]
    fn a_cancelled_request_never_spawns_the_command() {
        let engine = ExternalSpeech::new(template(&["definitely-not-a-program", "{out}"])).unwrap();
        let cancel = SpeechCancel::new();
        cancel.cancel();

        // A missing program would be ModelUnavailable; cancellation outranks it
        // because nothing was attempted.
        assert_eq!(
            engine.synthesize("Ciao", "any", &cancel).unwrap_err(),
            SpeechError::Cancelled
        );
    }

    #[test]
    fn a_command_that_is_not_installed_is_a_setup_problem() {
        let engine =
            ExternalSpeech::new(template(&["tuic-no-such-speech-engine", "-o", "{out}"])).unwrap();

        let error = engine
            .synthesize("Ciao", "any", &SpeechCancel::new())
            .unwrap_err();

        let SpeechError::ModelUnavailable { what, .. } = error else {
            panic!("expected a setup error, got {error:?}");
        };
        assert_eq!(what, "tuic-no-such-speech-engine");
    }

    #[test]
    fn a_real_command_renders_audio_through_the_port() {
        let workspace = tempfile::tempdir().unwrap();
        let source = workspace.path().join("voice.wav");
        std::fs::write(&source, one_second_wav()).unwrap();
        let engine = ExternalSpeech::new(copying_engine(&source)).unwrap();

        let audio = engine
            .synthesize("Ciao", "any", &SpeechCancel::new())
            .unwrap();

        assert_eq!(audio.sample_rate, 16_000);
        assert_eq!(audio.samples.len(), 16_000);
    }

    #[test]
    fn a_command_that_fails_carries_its_own_diagnosis() {
        let (shell, flag) = host_shell();
        let script = chain(
            &fail_with_stderr_script("model not found", 3),
            &touch_script(OUT_MARKER),
        );
        let engine = ExternalSpeech::new(template(&[shell, flag, &script])).unwrap();

        let error = engine
            .synthesize("Ciao", "any", &SpeechCancel::new())
            .unwrap_err();

        let SpeechError::Failed(reason) = error else {
            panic!("expected a failure, got {error:?}");
        };
        assert!(
            reason.contains("model not found"),
            "the engine's own words: {reason}"
        );
        assert!(reason.contains('3'), "and its exit status: {reason}");
    }

    #[test]
    fn a_command_that_succeeds_without_writing_anything_is_still_a_failure() {
        // The silent success this criterion exists to prevent: exit 0, no audio.
        let (shell, flag) = host_shell();
        let engine =
            ExternalSpeech::new(template(&[shell, flag, &touch_script(OUT_MARKER)])).unwrap();

        let error = engine
            .synthesize("Ciao", "any", &SpeechCancel::new())
            .unwrap_err();

        let SpeechError::Failed(reason) = error else {
            panic!("expected a failure, got {error:?}");
        };
        assert!(reason.contains("empty"), "{reason}");
    }

    #[test]
    fn a_command_that_writes_something_else_is_a_failure_that_says_what() {
        let workspace = tempfile::tempdir().unwrap();
        let source = workspace.path().join("not-audio.wav");
        std::fs::write(&source, b"<!DOCTYPE html>\n").unwrap();
        let engine = ExternalSpeech::new(copying_engine(&source)).unwrap();

        let error = engine
            .synthesize("Ciao", "any", &SpeechCancel::new())
            .unwrap_err();

        let SpeechError::Failed(reason) = error else {
            panic!("expected a failure, got {error:?}");
        };
        assert!(reason.contains("not a RIFF/WAVE"), "{reason}");
    }

    // Keep the sleeper's PID so timeout/cancellation tests observe the actual
    // subprocess, not just the shell's exit status.
    fn sleeping_script(workspace: &Path) -> String {
        #[cfg(unix)]
        {
            format!(
                "{} & sleeper=$!; echo $sleeper > '{}'; wait $sleeper; {}",
                sleep_script(),
                workspace.join("sleeper.pid").display(),
                touch_script(OUT_MARKER),
            )
        }
        #[cfg(windows)]
        {
            let _ = workspace;
            chain(&sleep_script(), &touch_script(OUT_MARKER))
        }
    }

    // Catches: stopping only the shell leaves its sleep child running.
    #[cfg(unix)]
    fn assert_sleeper_stopped(workspace: &Path) {
        let pid = std::fs::read_to_string(workspace.join("sleeper.pid"))
            .expect("shell must publish the sleeper PID");
        let pid: u32 = pid.trim().parse().expect("sleeper PID must be numeric");
        let deadline = Instant::now() + Duration::from_secs(2);
        // `info` is `None` once the process is gone or a zombie: the states `ps` printed as empty or Z.
        while tuic_core::process_info::info(pid).is_some() {
            if Instant::now() >= deadline {
                // Clean up the exact child this fixture started even on the RED run.
                tuic_core::process_info::kill(pid);
                panic!("sleep child {pid} survived stopping its shell");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_command_that_never_finishes_is_stopped_and_reported() {
        let (shell, flag) = host_shell();
        let workspace = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        let script = sleeping_script(workspace.path());
        let engine = ExternalSpeech::new(template(&[shell, flag, &script]))
            .unwrap()
            .with_timeout(Duration::from_millis(300));

        let started = Instant::now();
        let error = engine
            .synthesize("Ciao", "any", &SpeechCancel::new())
            .unwrap_err();

        #[cfg(unix)]
        assert_sleeper_stopped(workspace.path());
        let SpeechError::Failed(reason) = error else {
            panic!("expected a failure, got {error:?}");
        };
        assert!(reason.contains("did not finish"), "{reason}");
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "the timeout must be the one configured, not the floor"
        );
    }

    #[test]
    fn an_abandoned_request_stops_the_command_rather_than_waiting_it_out() {
        let (shell, flag) = host_shell();
        let workspace = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        let script = sleeping_script(workspace.path());
        let engine = ExternalSpeech::new(template(&[shell, flag, &script]))
            .unwrap()
            .with_timeout(Duration::from_secs(60));
        let cancel = SpeechCancel::new();

        let caller = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            caller.cancel();
        });

        let started = Instant::now();
        let error = engine.synthesize("Ciao", "any", &cancel).unwrap_err();

        #[cfg(unix)]
        assert_sleeper_stopped(workspace.path());
        assert_eq!(error, SpeechError::Cancelled);
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "cancelling must not wait for the timeout"
        );
    }

    #[test]
    fn audio_longer_than_the_text_can_justify_is_a_runaway() {
        // An engine pointed at the wrong model, or one that read a prompt back,
        // returns minutes for a word. Playing that is worse than saying so.
        let workspace = tempfile::tempdir().unwrap();
        let source = workspace.path().join("long.wav");
        let samples: Vec<i16> = vec![64; 16_000 * 30];
        std::fs::write(&source, wav(1, 16, 1, 16_000, &pcm16(&samples))).unwrap();
        let engine = ExternalSpeech::new(copying_engine(&source)).unwrap();

        let error = engine
            .synthesize("Sì.", "any", &SpeechCancel::new())
            .unwrap_err();

        assert_eq!(
            error,
            SpeechError::Runaway {
                budget_seconds: budget_seconds("Sì.")
            }
        );
    }
}
