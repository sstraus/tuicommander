//! Bounded real speaker-to-microphone evidence capture; never starts the app.
use cpal::traits::{DeviceTrait, HostTrait};
use parking_lot::Mutex;
use std::{
    io::Write,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tuic_dictation::{
    audio::AudioCapture,
    echo::{Canceller, EchoGuard, FarEndTap, webrtc::WebRtc},
    permission::{self, MicPermission},
    speaker::{DeviceOutput, Output},
    speech::{Speech, SpeechCancel, pocket::PocketSpeech},
};

struct Recorded {
    inner: WebRtc,
    reference: Arc<Mutex<Vec<f32>>>,
}
impl Canceller for Recorded {
    fn cancel(&mut self, far: &[f32], near: &mut [f32]) {
        self.reference.lock().extend_from_slice(far);
        self.inner.cancel(far, near);
    }
}
fn save(path: &Path, samples: &[f32]) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = std::fs::File::create_new(path)?;
    for sample in samples {
        file.write_all(&sample.to_le_bytes())?;
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let dir = Path::new(args.get(1).ok_or("supply a new evidence directory")?);
    std::fs::create_dir(dir)?;
    let permission = permission::check();
    println!("microphone_permission={}", permission.as_str());
    if permission != MicPermission::Authorized {
        return Err("microphone permission unavailable; no prompt requested".into());
    }
    let host = cpal::default_host();
    println!(
        "input={:?} output={:?}",
        host.default_input_device()
            .and_then(|d| d.description().ok())
            .map(|d| d.name().to_owned()),
        host.default_output_device()
            .and_then(|d| d.description().ok())
            .map(|d| d.name().to_owned())
    );
    let bundle = std::env::var("TUIC_POCKET_BUNDLE_DIR")?;
    let mut audio = PocketSpeech::new(bundle.into()).synthesize("Questa è una prova breve della riproduzione vocale. Il microfono registra soltanto questa prova e la prova termina automaticamente.", "giovanni", &SpeechCancel::new())?;
    tuic_dictation::loudness::process(
        &mut audio,
        tuic_dictation::loudness::Loudness {
            volume_db: -18.0,
            levelling: 0.67,
        },
    );
    let reference = Arc::new(Mutex::new(Vec::new()));
    let guard = Arc::new(Mutex::new(EchoGuard::new(Box::new(Recorded {
        inner: WebRtc::new()?,
        reference: reference.clone(),
    }))));
    let mut capture = AudioCapture::start_with_device(None)?;
    let buffer = capture.buffer_handle();
    guard
        .lock()
        .attach_capture(Box::new(move || buffer.lock().len()));
    let output = FarEndTap::new(Arc::new(DeviceOutput::open(None)?), guard.clone());
    let origin = Instant::now();
    let mut raw = Vec::new();
    let mut cleaned = Vec::new();
    let mut played = false;
    let mut timeline =
        String::from("elapsed_ms,raw_samples,clean_samples,reference_remaining,dropped\n");
    while origin.elapsed() < Duration::from_secs(14) {
        let batch = capture.drain_all();
        raw.extend_from_slice(&batch);
        cleaned.extend(guard.lock().clean(&batch));
        if !played && origin.elapsed() >= Duration::from_secs(1) {
            println!(
                "play_ms={} duration_s={}",
                origin.elapsed().as_millis(),
                audio.duration_seconds()
            );
            output.play(&audio)?;
            played = true;
        }
        let snapshot = guard.lock().snapshot();
        timeline.push_str(&format!(
            "{},{},{},{},{}\n",
            origin.elapsed().as_millis(),
            raw.len(),
            cleaned.len(),
            snapshot.queued_samples,
            capture.dropped_samples()
        ));
        std::thread::sleep(Duration::from_millis(50));
    }
    output.stop();
    capture.stop_stream();
    save(&dir.join("microphone-16000-f32le.pcm"), &raw)?;
    save(&dir.join("cleaned-16000-f32le.pcm"), &cleaned)?;
    save(&dir.join("reference-16000-f32le.pcm"), &reference.lock())?;
    save(&dir.join("rendered-f32le.pcm"), &audio.samples)?;
    std::fs::write(dir.join("timeline.csv"), timeline)?;
    println!(
        "complete raw={} cleaned={} rendered_rate={} dropped={}",
        raw.len(),
        cleaned.len(),
        audio.sample_rate,
        capture.dropped_samples()
    );
    Ok(())
}
