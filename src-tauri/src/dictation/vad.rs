/// Voice Activity Detection — energy-based, ported from whisper.cpp `common.cpp`.
///
/// Compares the absolute energy of the last `last_ms` milliseconds against the
/// entire buffer.  Returns `true` when the tail is **silent** (energy ratio
/// below `vad_thold`).

/// Returns `true` when the tail of `pcm` is silent.
///
/// * `pcm`         – 16-bit float PCM samples (will NOT be mutated)
/// * `sample_rate` – e.g. 16 000
/// * `last_ms`     – how many trailing milliseconds to compare (default 1 000)
/// * `vad_thold`   – energy ratio threshold (default 0.6)
/// * `freq_thold`  – high-pass cutoff Hz; 0.0 disables (default 100.0)
pub fn vad_simple(
    pcm: &[f32],
    sample_rate: u32,
    last_ms: u32,
    vad_thold: f32,
    freq_thold: f32,
) -> bool {
    let n_samples = pcm.len();
    let n_samples_last = ((sample_rate as f32 * last_ms as f32) / 1000.0) as usize;

    if n_samples_last >= n_samples {
        // Not enough audio to compare — assume not silent (safe default).
        return false;
    }

    // Copy for high-pass filtering (avoid mutating caller's data).
    let mut filtered = pcm.to_vec();
    if freq_thold > 0.0 {
        high_pass_filter(&mut filtered, freq_thold, sample_rate as f32);
    }

    let mut energy_all: f32 = 0.0;
    let mut energy_last: f32 = 0.0;
    for (i, &s) in filtered.iter().enumerate() {
        energy_all += s.abs();
        if i >= n_samples - n_samples_last {
            energy_last += s.abs();
        }
    }
    energy_all /= n_samples as f32;
    energy_last /= n_samples_last as f32;

    // If overall energy is near-zero (all silence), report silent.
    if energy_all < 1e-10 {
        return true;
    }

    // Silent when the tail's energy is below the threshold ratio of the whole.
    energy_last < vad_thold * energy_all
}

/// First-order high-pass filter (in-place).
/// RC filter: `y[n] = alpha * (y[n-1] + x[n] - x[n-1])`
fn high_pass_filter(data: &mut [f32], cutoff: f32, sample_rate: f32) {
    if data.len() < 2 {
        return;
    }
    let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff);
    let dt = 1.0 / sample_rate;
    let alpha = rc / (rc + dt);
    let mut y = 0.0f32;
    let mut prev_x = data[0];
    for i in 1..data.len() {
        let x = data[i];
        y = alpha * (y + x - prev_x);
        prev_x = x;
        data[i] = y;
    }
    data[0] = 0.0; // first sample has no predecessor
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    const SR: u32 = 16_000;

    /// Helper: generate a sine wave at `freq` Hz for `duration_ms` milliseconds.
    fn sine_wave(freq: f32, duration_ms: u32) -> Vec<f32> {
        let n = (SR as f32 * duration_ms as f32 / 1000.0) as usize;
        (0..n)
            .map(|i| (2.0 * PI * freq * i as f32 / SR as f32).sin() * 0.5)
            .collect()
    }

    /// Helper: silence (zeros) for `duration_ms`.
    fn silence(duration_ms: u32) -> Vec<f32> {
        vec![0.0; (SR as f32 * duration_ms as f32 / 1000.0) as usize]
    }

    #[test]
    fn test_silence_detected() {
        // All-zero input → tail is silent → returns true
        let pcm = silence(2000);
        assert!(vad_simple(&pcm, SR, 1000, 0.6, 100.0));
    }

    #[test]
    fn test_speech_detected() {
        // Continuous sine wave → tail has energy → returns false (speech present)
        let pcm = sine_wave(440.0, 2000);
        assert!(!vad_simple(&pcm, SR, 1000, 0.6, 100.0));
    }

    #[test]
    fn test_speech_then_silence() {
        // First half speech, second half silence → tail is silent → returns true
        let mut pcm = sine_wave(440.0, 1500);
        pcm.extend(silence(1500));
        assert!(vad_simple(&pcm, SR, 1000, 0.6, 100.0));
    }

    #[test]
    fn test_short_audio_returns_false() {
        // Less than last_ms of audio → can't determine → returns false
        let pcm = silence(500); // 500ms < 1000ms last_ms
        assert!(!vad_simple(&pcm, SR, 1000, 0.6, 100.0));
    }

    #[test]
    fn test_high_pass_removes_dc() {
        // DC offset (constant value) should be nearly zeroed after high-pass
        let mut data = vec![1.0; 16000]; // 1s of DC at 1.0
        high_pass_filter(&mut data, 100.0, SR as f32);
        // After convergence, the output should be near zero
        let tail_energy: f32 = data[8000..].iter().map(|s| s.abs()).sum::<f32>()
            / data[8000..].len() as f32;
        assert!(
            tail_energy < 0.01,
            "DC should be removed, but tail energy = {tail_energy}"
        );
    }
}
