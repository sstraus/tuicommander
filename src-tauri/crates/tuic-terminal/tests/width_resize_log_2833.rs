use tuic_terminal::vt_log::VtLogBuffer;

/// Stream ROW001..ROW150 line by line, applying `resizes` after the given row number.
fn stream_with_resizes(start: (u16, u16), resizes: &[(usize, u16, u16)]) -> String {
    let mut log = VtLogBuffer::new(start.0, start.1, 20_000);
    for n in 1..=150usize {
        log.process(format!("ROW{n:03}\r\n").as_bytes());
        for &(after, rows, cols) in resizes {
            if after == n {
                log.resize(rows, cols);
            }
        }
    }
    // Push every ROW out of the screen: the durable log alone is what format=log serves.
    for n in 151..=200usize {
        log.process(format!("FILL{n:03}\r\n").as_bytes());
    }
    let (lines, _) = log.lines_since_owned(0, 20_000);
    lines
        .into_iter()
        .map(|l| l.text())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Catches: reflow after a width change re-commits rows that were already logged.
#[test]
fn width_change_while_streaming_logs_each_row_once() {
    let mut failures = Vec::new();
    for rows in [10u16, 30] {
        for first in 40..=100 {
            for gap in [1usize, 3, 7, 20] {
                let resizes = [
                    (first, 10, 120),
                    (first + gap, 10, 60),
                    (first + 2 * gap, 30, 120),
                ];
                let text = stream_with_resizes((rows, 120), &resizes);
                let dup: Vec<usize> = (1..=150)
                    .filter(|n| text.matches(&format!("ROW{n:03}")).count() != 1)
                    .collect();
                if !dup.is_empty() {
                    failures.push(format!("rows {rows} first {first} gap {gap}: {dup:?}"));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} cases: {:?}",
        failures.len(),
        &failures[..failures.len().min(5)]
    );
}
