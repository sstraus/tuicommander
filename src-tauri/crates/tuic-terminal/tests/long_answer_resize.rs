use tuic_terminal::pty_capture::{CaptureDirection, decode_capture};
use tuic_terminal::vt_log::VtLogBuffer;

/// Replay recorded output with resizes at their recorded byte offsets.
/// Returns the canonical grid text and the durable log plus screen text.
fn replay(
    bytes: &[u8],
    (rows, cols): (u16, u16),
    resizes: &[(usize, u16, u16)],
    chunk: usize,
) -> (String, String) {
    let mut log = VtLogBuffer::new(rows, cols, 20_000);
    let mut position = 0;
    for &(end, rows, cols) in resizes {
        for piece in bytes[position..end].chunks(chunk) {
            log.process(piece);
        }
        log.resize(rows, cols);
        position = end;
    }
    for piece in bytes[position..].chunks(chunk) {
        log.process(piece);
    }
    let grid = log
        .read_rows_in_range(0, log.grid_total_lines() - 1)
        .join("\n");
    let (lines, _) = log.lines_since_owned(0, 20_000);
    let text = lines
        .into_iter()
        .chain(log.screen_log_lines())
        .map(|l| l.text())
        .collect::<Vec<_>>()
        .join("\n");
    (grid, text)
}

/// Catches: resizing while an answer streams discards non-chrome log rows.
#[test]
fn recorded_long_answer_survives_resize_in_grid_and_log() {
    let bytes = include_bytes!("fixtures/long-answer-2726/output.raw");
    let events: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/long-answer-2726/resizes.json")).unwrap();
    let resizes: Vec<(usize, u16, u16)> = events
        .iter()
        .map(|event| {
            (
                event["offset"].as_u64().unwrap() as usize,
                event["rows"].as_u64().unwrap() as u16,
                event["cols"].as_u64().unwrap() as u16,
            )
        })
        .collect();
    for chunk in [4096, 61] {
        let (grid, text) = replay(bytes, (24, 100), &resizes, chunk);
        for (label, output) in [("grid", grid), ("log", text)] {
            let missing: Vec<_> = (1..=160)
                .filter(|n| !output.contains(&format!("ROW{n:03}")))
                .collect();
            assert!(
                missing.is_empty(),
                "{label}, chunk {chunk}, missing rows {missing:?}"
            );
            for n in 21..=30 {
                assert!(output.contains(&format!("CODE{n:03}")), "{label}: code {n}");
            }
            for n in 1..=12 {
                assert!(
                    output.contains(&format!("BULLET{n:03}")),
                    "{label}: bullet {n}"
                );
            }
        }
    }
}

/// Catches: logging the rows a resize moved into history duplicates them in
/// the log when Claude Code reprints them and the grid merges the reprint.
#[test]
fn recorded_claude_resize_reprint_is_logged_once() {
    for (name, capture, timeline) in [
        (
            "streaming",
            include_bytes!("../src/fixtures/claude-resize-1407/streaming.tcap").as_slice(),
            include_str!("../src/fixtures/claude-resize-1407/streaming-resizes.json"),
        ),
        (
            "idle",
            include_bytes!("../src/fixtures/claude-resize-1407/idle.tcap").as_slice(),
            include_str!("../src/fixtures/claude-resize-1407/idle-resizes.json"),
        ),
    ] {
        let decoded = decode_capture(capture).expect("recorded capture");
        let geometry = decoded.geometry.expect("actual initial PTY geometry");
        let output: Vec<u8> = decoded
            .records
            .into_iter()
            .filter(|record| record.direction == CaptureDirection::Output)
            .flat_map(|record| record.data)
            .collect();
        let resizes: Vec<(usize, u16, u16)> =
            serde_json::from_str(timeline).expect("recorded resize byte boundaries");
        for chunk in [1, 61, 4096] {
            let (_, text) = replay(&output, geometry, &resizes, chunk);
            for number in 1..=20 {
                let marker = format!("R1407-{number:03}");
                assert_eq!(
                    text.matches(&marker).count(),
                    1,
                    "{name}, chunk {chunk}, log duplicate or missing {marker}"
                );
            }
        }
    }
}
