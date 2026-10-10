use regex::Regex;
use tuic_terminal::terminal_grid::TerminalGrid;
use unicode_width::UnicodeWidthChar;

// Catches a dot/space redraw shifting any row of the real wrapped Bash block.
#[test]
fn real_claude_dot_pulse_does_not_shift_wrapped_block_1654() {
    let capture: serde_json::Value =
        serde_json::from_str(include_str!("../src/fixtures/claude-dot-blink-1654.json"))
            .expect("recorded Claude capture");
    let raw = capture["data"].as_str().expect("raw PTY output");
    assert_eq!('⏺'.width(), Some(1));
    assert_eq!(' '.width(), Some(1));
    let pulse = Regex::new(
        r"^\x1b\[\?2026h\x1b\[2D\x1b\[[56]B\r\x1b\[1[45]A\x1b\[38;2;153;153;153m[ ⏺]\x1b\[39m\r(?:\r\n)+\x1b\[2C\x1b\[[56]A\x1b\[\?2026l$",
    )
    .expect("pure dot frame pattern");
    let mut grid = TerminalGrid::new(35, 60, 1000);
    let mut checked = 0;
    for frame in raw.split_inclusive("\x1b[?2026l") {
        let before: Vec<_> = (0..35).map(|row| grid.get_row_text(row)).collect();
        let _ = grid.process(frame.as_bytes());
        if pulse.is_match(frame) {
            let after: Vec<_> = (0..35).map(|row| grid.get_row_text(row)).collect();
            let start = before
                .iter()
                .position(|row| row.contains("Bash(sleep 20; echo"))
                .expect("recorded Bash block is visible");
            let before_block: Vec<_> = before[start..start + 4]
                .iter()
                .map(|row| row.chars().skip(1).collect::<String>())
                .collect();
            let after_block: Vec<_> = after[start..start + 4]
                .iter()
                .map(|row| row.chars().skip(1).collect::<String>())
                .collect();
            assert!(before_block[1].contains("DOT_BLINK_CAPTURE_1654_"));
            assert!(before_block[2].contains("EFGHIJKLMNOPQRSTUVWXYZ_0123456789_"));
            assert!(before_block[3].contains("uvwxyz_ABCDEFGHIJKLMNOPQRSTUVWXYZ_012…"));
            assert_eq!(
                after_block, before_block,
                "pulse {checked} shifted the wrapped block: {frame:?}"
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 20,
        "must replay real dot and blank pulses; got {checked}"
    );
}
