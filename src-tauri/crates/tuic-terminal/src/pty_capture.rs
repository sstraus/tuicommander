//! Raw PTY capture tap — turns "it happened again" into a fixture.
//!
//! State recognition (working / idle / awaiting) is decided from bytes an agent
//! writes once and never repeats. When a detection breaks, the evidence is
//! already gone: the per-session output ring holds the last 8 KB, which a single
//! Ink repaint overruns in seconds. Every regression therefore had to be
//! re-diagnosed by reasoning about code instead of by replaying what actually
//! arrived — and each new agent harness broke a detector in a way no test could
//! have caught, because no test had the bytes.
//!
//! This records the raw stream to disk so a failure observed on screen becomes a
//! file that `pty::tests` can replay forever. Off by default; the check when off
//! is one relaxed atomic load per chunk.
//!
//! ```text
//! curl -X POST localhost:9876/diagnostics/capture -H 'content-type: application/json' \
//!      -d '{"enabled":true}'                    # all sessions
//!      -d '{"enabled":true,"session_id":"<id>"}' # one session
//! curl localhost:9876/diagnostics/capture        # state + files written
//! ```
//!
//! Captures land in `<app config dir>/captures/<session-id>.tcap` by default.
//! `TUIC_CAPTURE_DIR` selects an absolute output directory when the process
//! must keep captures elsewhere. Files stop
//! growing at [`MAX_CAPTURE_BYTES`] each: a fixture is a moment, not a session
//! transcript, and an unattended tap must not fill Boss's disk.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// Legacy fixture signature retained by the decoder for existing captures.
const CAPTURE_MAGIC_V1: &[u8] = b"TUICCAP1\n";
const CAPTURE_MAGIC: &[u8] = b"TUICCAP2\n";
const GEOMETRY_BYTES: usize = 4; // rows:u16 + cols:u16
const RECORD_HEADER_BYTES: usize = 13; // direction:u8 + elapsed_us:u64 + len:u32

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureDirection {
    Output = 0,
    Input = 1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureRecord {
    pub direction: CaptureDirection,
    pub elapsed_us: u64,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedCapture {
    /// Terminal dimensions in effect when the first record was written.
    /// TUICCAP1 and legacy raw fixtures did not carry this metadata.
    pub geometry: Option<(u16, u16)>,
    pub records: Vec<CaptureRecord>,
}

/// Per-session cap. Comfortably covers a full screen repaint plus the prompt
/// that follows it, which is all a state-detection fixture needs.
pub const MAX_CAPTURE_BYTES: u64 = 512 * 1024;

static ENABLED: AtomicBool = AtomicBool::new(false);

struct CaptureState {
    /// When set, only this session is recorded. `None` records every session.
    session_filter: Option<String>,
    /// Open capture files, how many bytes each has taken, and whether the header
    /// holds a real geometry yet.
    files: HashMap<String, (std::fs::File, u64, bool)>,
    dir: Option<PathBuf>,
    started_at: std::time::Instant,
}

static STATE: Mutex<Option<CaptureState>> = Mutex::new(None);

/// Whether the tap is recording. Cheap enough to call per chunk.
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Start or stop recording. Starting always begins a fresh set of files:
/// a capture that silently appended to a previous run's bytes would replay as
/// one impossible stream.
pub fn set_enabled(enabled: bool, session_filter: Option<String>, dir: PathBuf) {
    let mut guard = STATE.lock();
    if enabled {
        *guard = Some(CaptureState {
            session_filter,
            files: HashMap::new(),
            dir: Some(dir),
            started_at: std::time::Instant::now(),
        });
    } else {
        *guard = None;
    }
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// Record one raw chunk. Called from the PTY read path, so it must never panic
/// and never block on anything but its own short-lived lock: a capture that can
/// take a session down is worse than no capture.
pub fn record_with_geometry(session_id: &str, data: &[u8], geometry: Option<(u16, u16)>) {
    record_direction(session_id, CaptureDirection::Output, data, geometry);
}

/// Record bytes written by the user/remote transport to the PTY. Keeping input
/// in the same timeline is what makes bare-Enter and timer/input races
/// reproducible; raw agent output alone cannot encode the missing CLEAR event.
pub fn record_input(session_id: &str, data: &[u8]) {
    record_input_with_geometry(session_id, data, None);
}

pub fn record_input_with_geometry(session_id: &str, data: &[u8], geometry: Option<(u16, u16)>) {
    record_direction(session_id, CaptureDirection::Input, data, geometry);
}

/// Current recording status, without choosing an application configuration directory.
pub fn capture_status() -> serde_json::Value {
    let guard = STATE.lock();
    match guard.as_ref() {
        Some(state) => serde_json::json!({
            "enabled": true,
            "session_filter": state.session_filter,
            "dir": state.dir.as_ref().map(|d| d.display().to_string()),
            "sessions": state.files.iter()
                .map(|(id, (_, len, _))| serde_json::json!({ "session_id": id, "bytes": len }))
                .collect::<Vec<_>>(),
        }),
        None => serde_json::json!({ "enabled": false }),
    }
}

fn record_direction(
    session_id: &str,
    direction: CaptureDirection,
    data: &[u8],
    geometry: Option<(u16, u16)>,
) {
    if !is_enabled() {
        return;
    }
    let mut guard = STATE.lock();
    let Some(state) = guard.as_mut() else {
        return;
    };
    if let Some(filter) = &state.session_filter
        && filter != session_id
    {
        return;
    }
    let Some(dir) = state.dir.clone() else {
        return;
    };
    let elapsed_us = state.started_at.elapsed().as_micros().min(u64::MAX as u128) as u64;
    let entry = match state.files.get_mut(session_id) {
        Some(entry) => entry,
        None => {
            if std::fs::create_dir_all(&dir).is_err() {
                return;
            }
            let path = dir.join(format!("{session_id}.tcap"));
            let Ok(mut file) = std::fs::File::create(&path) else {
                return;
            };
            if file.write_all(CAPTURE_MAGIC).is_err() {
                return;
            }
            let (rows, cols) = geometry.unwrap_or((0, 0));
            if file.write_all(&rows.to_le_bytes()).is_err()
                || file.write_all(&cols.to_le_bytes()).is_err()
            {
                return;
            }
            tracing::info!("[capture] recording {session_id} to {}", path.display());
            state.files.entry(session_id.to_string()).or_insert((
                file,
                (CAPTURE_MAGIC.len() + GEOMETRY_BYTES) as u64,
                geometry.is_some(),
            ))
        }
    };
    // HTTP input can open a capture before any output reports the terminal size.
    // Fill the header from the first record that knows it, not 0x0 forever.
    if !entry.2
        && let Some((rows, cols)) = geometry
    {
        let mut header = [0u8; GEOMETRY_BYTES];
        header[..2].copy_from_slice(&rows.to_le_bytes());
        header[2..].copy_from_slice(&cols.to_le_bytes());
        let patched = entry
            .0
            .seek(SeekFrom::Start(CAPTURE_MAGIC.len() as u64))
            .and_then(|_| entry.0.write_all(&header))
            .and_then(|_| entry.0.seek(SeekFrom::End(0)));
        if patched.is_err() {
            return;
        }
        entry.2 = true;
    }
    if entry.1 >= MAX_CAPTURE_BYTES {
        return;
    }
    let room = (MAX_CAPTURE_BYTES - entry.1) as usize;
    if room <= RECORD_HEADER_BYTES {
        return;
    }
    let payload_len = data.len().min(room - RECORD_HEADER_BYTES);
    let mut header = [0u8; RECORD_HEADER_BYTES];
    header[0] = direction as u8;
    header[1..9].copy_from_slice(&elapsed_us.to_le_bytes());
    header[9..13].copy_from_slice(&(payload_len as u32).to_le_bytes());
    if entry.0.write_all(&header).is_ok() && entry.0.write_all(&data[..payload_len]).is_ok() {
        entry.1 += (RECORD_HEADER_BYTES + payload_len) as u64;
    }
}

/// Decode a framed capture. Legacy `.raw` fixtures are returned as one output
/// record so the existing corpus remains usable while new captures preserve the
/// real read/write boundaries.
pub fn decode(bytes: &[u8]) -> Result<Vec<CaptureRecord>, String> {
    Ok(decode_capture(bytes)?.records)
}

/// Decode records plus capture metadata. TUICCAP1 remains byte-compatible and
/// reports unknown geometry; raw fixtures remain one output record.
pub fn decode_capture(bytes: &[u8]) -> Result<DecodedCapture, String> {
    if !bytes.starts_with(CAPTURE_MAGIC) && !bytes.starts_with(CAPTURE_MAGIC_V1) {
        return Ok(DecodedCapture {
            geometry: None,
            records: vec![CaptureRecord {
                direction: CaptureDirection::Output,
                elapsed_us: 0,
                data: bytes.to_vec(),
            }],
        });
    }
    let (mut cursor, geometry) = if bytes.starts_with(CAPTURE_MAGIC) {
        if bytes.len() < CAPTURE_MAGIC.len() + GEOMETRY_BYTES {
            return Err("truncated capture geometry".to_string());
        }
        let cursor = CAPTURE_MAGIC.len();
        let rows = u16::from_le_bytes(bytes[cursor..cursor + 2].try_into().unwrap());
        let cols = u16::from_le_bytes(bytes[cursor + 2..cursor + 4].try_into().unwrap());
        (
            cursor + GEOMETRY_BYTES,
            (rows > 0 && cols > 0).then_some((rows, cols)),
        )
    } else {
        (CAPTURE_MAGIC_V1.len(), None)
    };
    let mut records = Vec::new();
    while cursor < bytes.len() {
        if bytes.len() - cursor < RECORD_HEADER_BYTES {
            return Err("truncated capture record header".to_string());
        }
        let direction = match bytes[cursor] {
            0 => CaptureDirection::Output,
            1 => CaptureDirection::Input,
            value => return Err(format!("invalid capture direction {value}")),
        };
        let elapsed_us = u64::from_le_bytes(bytes[cursor + 1..cursor + 9].try_into().unwrap());
        let len = u32::from_le_bytes(bytes[cursor + 9..cursor + 13].try_into().unwrap()) as usize;
        cursor += RECORD_HEADER_BYTES;
        let end = cursor
            .checked_add(len)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| "truncated capture record payload".to_string())?;
        records.push(CaptureRecord {
            direction,
            elapsed_us,
            data: bytes[cursor..end].to_vec(),
        });
        cursor = end;
    }
    Ok(DecodedCapture { geometry, records })
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn disabled_tap_writes_nothing() {
        let _guard = TEST_LOCK.lock();
        let scratch = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        let dir = scratch.path().join("disabled");
        set_enabled(false, None, dir.clone());
        record_with_geometry("session-a", b"hello", Some((24, 80)));
        assert!(!dir.exists());
    }

    #[test]
    fn capture_respects_the_session_filter_and_the_size_cap() {
        let _guard = TEST_LOCK.lock();
        let scratch = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        let dir = scratch.path().join("filter");
        set_enabled(true, Some("wanted".into()), dir.clone());

        record_with_geometry("wanted", b"waiting", Some((63, 160)));
        record_with_geometry("other", b"must not be recorded", Some((24, 80)));
        record_with_geometry(
            "wanted",
            &vec![b'x'; (MAX_CAPTURE_BYTES + 1024) as usize],
            Some((63, 160)),
        );
        set_enabled(false, None, dir.clone());

        let wanted = std::fs::read(dir.join("wanted.tcap")).expect("filtered session recorded");
        assert!(wanted.starts_with(CAPTURE_MAGIC));
        assert_eq!(wanted.len() as u64, MAX_CAPTURE_BYTES);
        assert!(!dir.join("other.tcap").exists());
    }

    // Catches: a capture whose first record is geometry-less HTTP input latches a 0x0 header.
    #[test]
    fn input_first_capture_records_the_first_known_geometry() {
        let _guard = TEST_LOCK.lock();
        let scratch = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        let dir = scratch.path().join("input-first");
        set_enabled(true, None, dir.clone());

        record_input("http", b"ls\r");
        record_with_geometry("http", b"listing", Some((41, 110)));
        record_with_geometry("http", b"resized", Some((50, 120)));
        set_enabled(false, None, dir.clone());

        let capture = decode_capture(&std::fs::read(dir.join("http.tcap")).unwrap()).unwrap();
        assert_eq!(capture.geometry, Some((41, 110)));
        let data: Vec<&[u8]> = capture.records.iter().map(|r| r.data.as_slice()).collect();
        assert_eq!(data, [&b"ls\r"[..], b"listing", b"resized"]);
    }

    #[test]
    fn restarting_the_tap_truncates() {
        let _guard = TEST_LOCK.lock();
        let scratch = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        let dir = scratch.path().join("restart");

        set_enabled(true, None, dir.clone());
        record_with_geometry("s", b"first run", Some((24, 80)));
        set_enabled(false, None, dir.clone());
        set_enabled(true, None, dir.clone());
        record_with_geometry("s", b"second", Some((24, 80)));
        set_enabled(false, None, dir.clone());

        let records = decode(&std::fs::read(dir.join("s.tcap")).unwrap()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].data, b"second");
    }

    #[test]
    fn framed_capture_preserves_output_input_order_and_boundaries() {
        let _guard = TEST_LOCK.lock();
        let scratch = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        let dir = scratch.path().join("framed");
        set_enabled(true, Some("s".into()), dir.clone());
        record_with_geometry("s", b"question?", Some((63, 160)));
        record_input_with_geometry("s", b"\r", Some((63, 160)));
        record_with_geometry("s", b"working", Some((63, 160)));
        set_enabled(false, None, dir.clone());

        let records = decode(&std::fs::read(dir.join("s.tcap")).unwrap()).unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].direction, CaptureDirection::Output);
        assert_eq!(records[0].data, b"question?");
        assert_eq!(records[1].direction, CaptureDirection::Input);
        assert_eq!(records[1].data, b"\r");
        assert_eq!(records[2].direction, CaptureDirection::Output);
        assert_eq!(records[2].data, b"working");
        assert!(
            records
                .windows(2)
                .all(|pair| pair[0].elapsed_us <= pair[1].elapsed_us)
        );
    }

    #[test]
    fn capture_exposes_geometry_and_decodes_tuiccap1() {
        let _guard = TEST_LOCK.lock();
        let scratch = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        let dir = scratch.path().join("geometry");
        set_enabled(true, Some("s".into()), dir.clone());
        record_with_geometry("s", b"frame", Some((63, 160)));
        set_enabled(false, None, dir.clone());

        let decoded = decode_capture(&std::fs::read(dir.join("s.tcap")).unwrap()).unwrap();
        assert_eq!(decoded.geometry, Some((63, 160)));
        assert_eq!(decoded.records[0].data, b"frame");

        let mut v1 = CAPTURE_MAGIC_V1.to_vec();
        v1.push(CaptureDirection::Output as u8);
        v1.extend_from_slice(&7_u64.to_le_bytes());
        v1.extend_from_slice(&3_u32.to_le_bytes());
        v1.extend_from_slice(b"old");
        let decoded = decode_capture(&v1).unwrap();
        assert_eq!(decoded.geometry, None);
        assert_eq!(decoded.records[0].elapsed_us, 7);
        assert_eq!(decoded.records[0].data, b"old");
    }
}
