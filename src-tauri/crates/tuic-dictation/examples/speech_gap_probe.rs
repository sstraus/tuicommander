//! Isolated, silent native playback measurement for story 2605-53fc.
//! No microphone, app instance, configuration write, or installed binary is used.
use parking_lot::Mutex;
use rodio::{Source, buffer::SamplesBuffer};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use tuic_dictation::{
    speaker::{DeviceOutput, Output, Speaker, Utterance},
    speech::{
        Speech, SpeechAudio, SpeechCancel, SpeechError, edge::EdgeSpeech, pocket::PocketSpeech,
    },
};

#[derive(Default)]
struct Timeline {
    events: Mutex<Vec<(f64, String)>>,
}
impl Timeline {
    fn event(&self, origin: Instant, message: String) {
        self.events
            .lock()
            .push((origin.elapsed().as_secs_f64() * 1000.0, message));
    }
}
struct Engine {
    inner: Arc<dyn Speech>,
    origin: Instant,
    timeline: Arc<Timeline>,
    next: AtomicUsize,
}
impl Speech for Engine {
    fn synthesize(
        &self,
        text: &str,
        voice: &str,
        cancel: &SpeechCancel,
    ) -> Result<SpeechAudio, SpeechError> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.timeline
            .event(self.origin, format!("synthesis-start id={id}"));
        let result = self.inner.synthesize(text, voice, cancel);
        match &result {
            Ok(audio) => self.timeline.event(
                self.origin,
                format!(
                    "synthesis-end id={id} duration_ms={:.3}",
                    audio.duration_seconds() * 1000.0
                ),
            ),
            Err(error) => self.timeline.event(
                self.origin,
                format!("synthesis-error id={id} error={error}"),
            ),
        }
        result
    }
}
struct Marked {
    inner: SamplesBuffer,
    id: usize,
    origin: Instant,
    timeline: Arc<Timeline>,
    read: usize,
    total: usize,
    last: Instant,
    maximum_gap: Duration,
}
impl Iterator for Marked {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next();
        if sample.is_some() {
            if self.read == 0 {
                self.timeline
                    .event(self.origin, format!("source-start id={}", self.id));
            }
            // Sampling once per 10 ms of source audio. This is mixer consumption,
            // not a hardware DAC timestamp or an exact CPAL callback timestamp.
            if self
                .read
                .is_multiple_of((self.sample_rate().get() as usize / 100).max(1))
            {
                if self.read > 0 {
                    self.maximum_gap = self.maximum_gap.max(self.last.elapsed());
                }
                self.last = Instant::now();
            }
            self.read += 1;
            if self.read == self.total {
                self.timeline.event(
                    self.origin,
                    format!(
                        "source-end id={} max_consumption_gap_ms={:.3}",
                        self.id,
                        self.maximum_gap.as_secs_f64() * 1000.0
                    ),
                );
            }
        }
        // Keep the real sample count and rate, but do not speak in Boss's room.
        sample.map(|_| 0.0)
    }
}
impl Source for Marked {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}
struct MeasuredOutput {
    // The production DeviceOutput is used for the direct control. The measured
    // adapter repeats its exact SamplesBuffer -> Player -> device mixer path,
    // adding source-consumption timestamps and an output error callback.
    direct: Option<DeviceOutput>,
    _stream: Option<rodio::MixerDeviceSink>,
    player: Option<rodio::Player>,
    origin: Instant,
    timeline: Arc<Timeline>,
    next: AtomicUsize,
}
impl MeasuredOutput {
    fn open(origin: Instant, timeline: Arc<Timeline>, direct: bool) -> Result<Self, String> {
        let (device, stream, player) = if direct {
            (Some(DeviceOutput::open(None)?), None, None)
        } else {
            let events = Arc::clone(&timeline);
            let stream = rodio::DeviceSinkBuilder::from_default_device()
                .map_err(|e| e.to_string())?
                .with_error_callback(move |error| {
                    events.event(origin, format!("output-error {error}"))
                })
                .open_stream()
                .map_err(|e| e.to_string())?;
            println!("device_config={:?}", stream.config());
            let player = rodio::Player::connect_new(stream.mixer());
            (None, Some(stream), Some(player))
        };
        Ok(Self {
            direct: device,
            _stream: stream,
            player,
            origin,
            timeline,
            next: AtomicUsize::new(0),
        })
    }
}
impl Output for MeasuredOutput {
    fn play(&self, audio: &SpeechAudio) -> Result<(), String> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.timeline.event(
            self.origin,
            format!("append id={id} prior_speaking={}", self.is_speaking()),
        );
        if let Some(output) = &self.direct {
            let silent = SpeechAudio {
                samples: vec![0.0; audio.samples.len()],
                sample_rate: audio.sample_rate,
            };
            return output.play(&silent);
        }
        let source = SamplesBuffer::new(
            1.try_into().unwrap(),
            audio.sample_rate.try_into().unwrap(),
            audio.samples.clone(),
        );
        self.player.as_ref().unwrap().append(Marked {
            inner: source,
            id,
            origin: self.origin,
            timeline: Arc::clone(&self.timeline),
            read: 0,
            total: audio.samples.len(),
            last: Instant::now(),
            maximum_gap: Duration::ZERO,
        });
        Ok(())
    }
    fn stop(&self) {
        if let Some(d) = &self.direct {
            d.stop();
        } else {
            self.player.as_ref().unwrap().stop();
            self.player.as_ref().unwrap().play();
        }
    }
    fn is_speaking(&self) -> bool {
        self.direct.as_ref().map_or_else(
            || !self.player.as_ref().unwrap().empty(),
            Output::is_speaking,
        )
    }
    fn pause(&self) {
        if let Some(d) = &self.direct {
            d.pause();
        } else {
            self.player.as_ref().unwrap().pause();
        }
    }
    fn resume(&self) {
        if let Some(d) = &self.direct {
            d.resume();
        } else {
            self.player.as_ref().unwrap().play();
        }
    }
    fn can_pause(&self) -> bool {
        true
    }
}
struct Load {
    stop: Arc<AtomicBool>,
    workers: Vec<thread::JoinHandle<()>>,
}
impl Load {
    fn start(count: usize) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let workers = (0..count)
            .map(|index| {
                let stop = Arc::clone(&stop);
                thread::spawn(move || {
                    #[cfg(target_os = "macos")]
                    if index + 1 == count {
                        unsafe extern "C" {
                            fn pthread_set_qos_class_self_np(class: u32, relative: i32) -> i32;
                        }
                        // SAFETY: changes only this owned worker's scheduling class.
                        unsafe {
                            pthread_set_qos_class_self_np(0x09, 0);
                        }
                    }
                    let until = Instant::now() + Duration::from_secs(90);
                    let mut value = 1.0f64;
                    while !stop.load(Ordering::Relaxed) && Instant::now() < until {
                        for _ in 0..1000 {
                            value = std::hint::black_box(value.sin() + 1.01);
                        }
                    }
                    std::hint::black_box((value, index));
                })
            })
            .collect();
        Self { stop, workers }
    }
}
impl Drop for Load {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for worker in self.workers.drain(..) {
            worker.join().unwrap();
        }
    }
}
fn main() -> std::process::ExitCode {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("TLS provider");
    let args: Vec<String> = std::env::args().collect();
    let engine_name = args.get(1).map(String::as_str).unwrap_or("pocket");
    let loaded = args.iter().any(|arg| arg == "--load");
    let direct = args.iter().any(|arg| arg == "--direct");
    let count = if loaded {
        (thread::available_parallelism().unwrap().get() / 2).max(1)
    } else {
        0
    };
    println!("engine={engine_name} load_workers={count} direct_device_output={direct}");
    let (engine, voice): (Arc<dyn Speech>, &str) = if engine_name == "edge" {
        (Arc::new(EdgeSpeech::new()), "it-IT-DiegoNeural")
    } else {
        let dir = args
            .get(2)
            .filter(|s| !s.starts_with("--"))
            .expect("pocket requires explicit bundle directory");
        (
            Arc::new(PocketSpeech::new(std::path::PathBuf::from(dir))),
            "giovanni",
        )
    };
    let origin = Instant::now();
    let timeline = Arc::new(Timeline::default());
    let output = match MeasuredOutput::open(origin, Arc::clone(&timeline), direct) {
        Ok(output) => Arc::new(output),
        Err(error) => {
            println!("device-open-error={error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let _load = Load::start(count);
    let engine = Arc::new(Engine {
        inner: engine,
        origin,
        timeline: Arc::clone(&timeline),
        next: AtomicUsize::new(0),
    });
    let echo = Arc::new(Mutex::new(tuic_dictation::echo::EchoGuard::new(Box::new(
        tuic_dictation::echo::PassThrough,
    ))));
    let tapped = Arc::new(tuic_dictation::echo::FarEndTap::new(output.clone(), echo));
    let speaker = Speaker::new(engine, tapped, 0);
    // Production defaults from DictationConfig. No capture stream is opened,
    // so this measures the tap's append cost, not acoustic echo cancellation.
    speaker.set_loudness(tuic_dictation::loudness::Loudness {
        volume_db: -18.0,
        levelling: 0.67,
    });
    let ids: Vec<_> = [
        "Ciao, sono pronto.",
        "Le chiavi sono sul tavolo della cucina.",
        "Possiamo controllare i risultati insieme.",
    ]
    .iter()
    .map(|text| speaker.say(0, text, voice).unwrap())
    .collect();
    let until = Instant::now() + Duration::from_secs(90);
    while Instant::now() < until
        && !ids.iter().all(|id| {
            matches!(
                speaker.utterance(*id),
                Some(Utterance::Finished | Utterance::Failed(_) | Utterance::Interrupted)
            )
        })
    {
        thread::sleep(Duration::from_millis(2));
    }
    let succeeded = ids
        .iter()
        .all(|id| speaker.utterance(*id) == Some(Utterance::Finished));
    for id in ids {
        println!("utterance={id} state={:?}", speaker.utterance(id));
    }
    timeline.event(origin, "all-drained-or-deadline".to_string());
    drop(speaker);
    for (ms, message) in timeline.events.lock().iter() {
        println!("{ms:.3}ms {message}");
    }
    if succeeded {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
