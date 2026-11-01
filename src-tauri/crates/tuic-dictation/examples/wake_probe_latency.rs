//! Native measurement harness for story 2610-9b43.
//! Uses recorded macOS say output and the production capture/recognition path.
#[cfg(unix)]
mod native {
    use parking_lot::Mutex;
    use std::{path::Path, sync::Arc, time::Instant};
    use tuic_dictation::{
        continuous::*,
        echo,
        transcribe::{Transcriber, VoiceGates, WhisperTranscriber},
    };

    fn cpu_ms() -> f64 {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
        // SAFETY: getrusage initializes this valid output pointer on success.
        assert_eq!(
            unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) },
            0
        );
        // SAFETY: getrusage succeeded above.
        let usage = unsafe { usage.assume_init() };
        (usage.ru_utime.tv_sec + usage.ru_stime.tv_sec) as f64 * 1000.0
            + (usage.ru_utime.tv_usec + usage.ru_stime.tv_usec) as f64 / 1000.0
    }

    fn wave(path: &Path) -> Vec<f32> {
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        let mut offset = 12;
        let mut valid_format = false;
        while offset + 8 <= bytes.len() {
            let size =
                u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
            let chunk = &bytes[offset + 8..offset + 8 + size];
            match &bytes[offset..offset + 4] {
                b"fmt " => {
                    assert_eq!(u16::from_le_bytes(chunk[0..2].try_into().unwrap()), 1);
                    assert_eq!(u16::from_le_bytes(chunk[2..4].try_into().unwrap()), 1);
                    assert_eq!(u32::from_le_bytes(chunk[4..8].try_into().unwrap()), 16_000);
                    assert_eq!(u16::from_le_bytes(chunk[14..16].try_into().unwrap()), 16);
                    valid_format = true;
                }
                b"data" => {
                    assert!(valid_format);
                    return chunk
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|sample| i16::from_le_bytes(*sample) as f32 / 32768.0)
                        .collect();
                }
                _ => {}
            }
            offset += 8 + size + size % 2;
        }
        panic!("missing WAV data");
    }

    struct Endpoint<'a> {
        pending: Vec<f32>,
        transcriber: &'a WhisperTranscriber,
        mode: Arc<Mutex<HandsFree>>,
        fixture: &'a str,
        repetition: usize,
    }
    impl VoiceEndpoint for Endpoint<'_> {
        fn drain(&mut self) -> Result<Vec<f32>, String> {
            Ok(std::mem::take(&mut self.pending))
        }
        fn connected(&self) -> bool {
            true
        }
        fn transcribe(&self, audio: &[f32]) -> Result<Transcript, String> {
            let kind = if self.mode.lock().phase().as_wire() == "transcribing" {
                "final"
            } else {
                "probe"
            };
            let cpu_start = cpu_ms();
            let started = Instant::now();
            let result = self
                .transcriber
                .transcribe(audio, Some("en"), VoiceGates::default())?;
            println!(
                "{}",
                serde_json::json!({"kind":kind, "fixture":self.fixture, "repetition":self.repetition, "audio_ms":audio.len() as f64 / 16.0, "wall_ms":started.elapsed().as_secs_f64()*1000.0, "cpu_ms":cpu_ms()-cpu_start, "text":result.text, "skip_reason":result.skip_reason})
            );
            Ok(if result.skip_reason.is_some() {
                Transcript::default()
            } else {
                Transcript {
                    text: result.text,
                    language: result.language,
                }
            })
        }
    }
    struct Port;
    impl TargetProbe for Port {
        fn accepts(&self, _: &str) -> bool {
            true
        }
    }
    impl VoiceSink for Port {
        fn write(&self, _: &str, _: &str) -> Result<VoiceWrite, String> {
            Ok(VoiceWrite::Written)
        }
    }
    pub fn run() {
        let model = std::env::args().nth(1).expect("model path");
        let fixtures = std::env::args().nth(2).expect("fixture directory");
        let repetitions: usize = std::env::args()
            .nth(3)
            .unwrap_or("20".into())
            .parse()
            .unwrap();
        let paced = std::env::args().nth(4).as_deref() == Some("paced");
        let load_started = Instant::now();
        let load_cpu = cpu_ms();
        let transcriber = WhisperTranscriber::load(Path::new(&model)).unwrap();
        println!(
            "{}",
            serde_json::json!({"kind":"model_load", "wall_ms":load_started.elapsed().as_secs_f64()*1000.0, "cpu_ms":cpu_ms()-load_cpu})
        );
        for fixture in ["addressed", "unaddressed"] {
            let mut audio = wave(&Path::new(&fixtures).join(format!("{fixture}.wav")));
            audio.extend(vec![0.0; 32_000]);
            for repetition in 0..repetitions {
                let mut mode = HandsFree::new(1_000);
                mode.set_activation("computer", ACTIVATION_WINDOW_MS);
                mode.arm("measurement", "recorded-audio", true).unwrap();
                let mode = Arc::new(Mutex::new(mode));
                let echo = Arc::new(Mutex::new(echo::EchoGuard::new(Box::new(
                    echo::PassThrough,
                ))));
                let mut capture = Capture::new(
                    SegmenterConfig::default(),
                    DEVICE_SILENCE_TIMEOUT_MS,
                    0,
                    echo,
                    None,
                );
                let mut endpoint = Endpoint {
                    pending: Vec::new(),
                    transcriber: &transcriber,
                    mode: mode.clone(),
                    fixture,
                    repetition,
                };
                let mut ack_audio_ms = None;
                let mut ack_wall_ms = None;
                let started = Instant::now();
                for (index, chunk) in audio.chunks(800).enumerate() {
                    endpoint.pending = chunk.to_vec();
                    let now_ms = (index as u64 + 1) * 50;
                    if paced
                        && let Some(remaining) =
                            std::time::Duration::from_millis(now_ms).checked_sub(started.elapsed())
                    {
                        std::thread::sleep(remaining);
                    }
                    let _ = tick(&mut capture, &mode, &mut endpoint, &Port, &Port, now_ms);
                    if mode.lock().phase().as_wire() == "wake_confirmed" && ack_audio_ms.is_none() {
                        ack_audio_ms = Some(now_ms);
                        ack_wall_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
                    }
                }
                println!(
                    "{}",
                    serde_json::json!({"kind":"replay", "fixture":fixture, "repetition":repetition, "ack_audio_ms":ack_audio_ms, "ack_wall_ms":ack_wall_ms, "paced":paced, "compute_wall_ms":started.elapsed().as_secs_f64()*1000.0, "dropped_turns":mode.lock().dropped_turns()})
                );
                assert_eq!(
                    ack_audio_ms.is_some(),
                    fixture == "addressed",
                    "real recognition wake distinction"
                );
            }
        }
    }
}

#[cfg(unix)]
fn main() {
    native::run();
}

#[cfg(not(unix))]
fn main() {
    eprintln!("This benchmark requires a Unix process CPU clock");
    std::process::exit(2);
}
