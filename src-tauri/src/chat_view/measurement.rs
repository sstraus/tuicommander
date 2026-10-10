//! Opt-in measurement: source stays outside the repository and is read at runtime.

#[cfg(unix)]
use super::*;

#[cfg(unix)]
fn peak_rss_bytes() -> u64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: getrusage writes a correctly aligned rusage for this process.
    assert_eq!(
        unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) },
        0
    );
    // SAFETY: successful getrusage initialized usage.
    let peak = u64::try_from(unsafe { usage.assume_init() }.ru_maxrss).unwrap();
    if cfg!(target_os = "macos") {
        peak
    } else {
        peak * 1024
    }
}

// Catches: the real reader/adapter/log path produces empty snapshots or exceeds its retained byte budget.
#[test]
#[cfg(unix)]
#[ignore = "Requires TUIC_CHAT_TRANSCRIPT: an authorized real Claude JSONL file, read-only"]
fn view_real_transcript_throughput() {
    let path =
        PathBuf::from(std::env::var_os("TUIC_CHAT_TRANSCRIPT").expect("set TUIC_CHAT_TRANSCRIPT"));
    let bytes = std::fs::metadata(&path).expect("source metadata").len();
    let mut attached = View::new(path.clone(), MAX_LOG_ENTRIES, MAX_LOG_BYTES);
    let start = Instant::now();
    attached
        .advance(TAIL_WINDOW_BYTES)
        .expect("read production attach window");
    let (_, updates) = attached.log.since(None, 0);
    let attach = start.elapsed();
    let attach_peak = peak_rss_bytes();
    assert!(
        !updates.is_empty(),
        "real transcript produced no attach snapshot"
    );
    drop(updates);
    // Hold the file's complete contents through the same path for the full
    // throughput measurement. Production attaches use only the 2 MiB window.
    let mut full = View::new(path, MAX_LOG_ENTRIES, MAX_LOG_BYTES);
    let start = Instant::now();
    full.advance(bytes).expect("read full real transcript");
    let (_, updates) = full.log.since(None, 0);
    let elapsed = start.elapsed();
    assert!(
        !updates.is_empty(),
        "real transcript produced no full snapshot"
    );
    assert!(
        full.log.bytes <= 4 * 1024 * 1024,
        "retained log exceeds its production byte budget"
    );
    println!(
        "CHAT_VIEW_MEASUREMENT source_bytes={bytes} attach_window_bytes={} attach_ms={:.3} attach_peak_rss_bytes={attach_peak} full_ms={:.3} full_mib_per_second={:.3} full_peak_rss_bytes={} updates={} retained_entries={} retained_bytes={} unknown_rows={} malformed_rows={}",
        TAIL_WINDOW_BYTES,
        attach.as_secs_f64() * 1000.0,
        elapsed.as_secs_f64() * 1000.0,
        bytes as f64 / 1048576.0 / elapsed.as_secs_f64(),
        peak_rss_bytes(),
        full.log.next_seq,
        full.log.entries.len(),
        full.log.bytes,
        full.adapter.stats.unknown_rows,
        full.adapter.stats.malformed_rows
    );
}
