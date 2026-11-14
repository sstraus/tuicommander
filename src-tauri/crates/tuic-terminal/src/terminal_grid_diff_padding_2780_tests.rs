//! Story 2780-e44d: a coloured diff row reflowed to a narrower width.
//!
//! Claude Code pads every diff row with background-coloured spaces up to the
//! terminal width. When the pane later gets narrower, full reflow wraps that
//! padding like text, so each row grows extra coloured rows and a partial
//! coloured block. The bytes are recorded Claude Code output, see the README.

use super::TerminalGrid;
use alacritty_terminal::grid::{Dimensions, ReflowMode};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::vte::ansi::{Color, Rgb};

const ROWS: &[u8] = include_bytes!("fixtures/claude-diff-rows-2780/rows.raw");
/// Width at which the rows were recorded: the padding ends at this column.
const RECORDED_COLS: u16 = 229;
const ADDED_BG: Color = Color::Spec(Rgb { r: 2, g: 40, b: 0 });
/// The rows start at `ESC [6G`: the first five cells keep the default background.
const INDENT: usize = 5;

/// Background of every cell from the oldest history row to the last screen row:
/// a narrower reflow pushes rows into history.
fn backgrounds(grid: &TerminalGrid) -> Vec<Vec<Color>> {
    let g = grid.term().grid();
    let top = -(g.history_size() as i32);
    (top..g.screen_lines() as i32)
        .map(|line| {
            (0..g.columns())
                .map(|col| g[Line(line)][Column(col)].bg)
                .collect()
        })
        .collect()
}

/// Text cells of each recorded diff row (indent included), independent of the VT.
fn recorded_text_widths() -> Vec<usize> {
    let text = String::from_utf8_lossy(ROWS);
    text.split("\r\r\n")
        .filter(|row| !row.is_empty())
        .map(|row| {
            let mut visible = String::new();
            let mut chars = row.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '\u{1b}' {
                    // CSI: skip to the final byte.
                    chars.next();
                    for c in chars.by_ref() {
                        if c.is_ascii_alphabetic() {
                            break;
                        }
                    }
                } else {
                    visible.push(c);
                }
            }
            INDENT + visible.trim_end().chars().count()
        })
        .collect()
}

/// Catches: reflow wrapping background-only padding into extra coloured rows
/// (and a partial coloured block) after the pane gets narrower.
#[test]
fn recorded_diff_rows_reflow_without_extra_coloured_rows() {
    let widths = recorded_text_widths();
    assert_eq!(widths.len(), 12, "fixture holds twelve diff rows");
    for cols in [94usize, 60, 228] {
        let mut grid = TerminalGrid::new(80, RECORDED_COLS, 1000);
        let _ = grid.process(ROWS);
        grid.resize_with_mode(80, cols as u16, ReflowMode::All);

        // Each diff row needs only as many rows as its text, never fewer than one.
        let expected: usize = widths.iter().map(|w| w.div_ceil(cols).max(1)).sum();
        let rows = backgrounds(&grid);
        let coloured: Vec<usize> = (0..rows.len())
            .filter(|&i| rows[i].contains(&ADDED_BG))
            .collect();
        assert_eq!(
            coloured,
            (0..expected).collect::<Vec<_>>(),
            "cols={cols}: coloured rows must be exactly the rows the text needs"
        );

        // Each of those rows is coloured to the right edge, as the child drew it.
        let mut line = 0;
        for width in &widths {
            for part in 0..width.div_ceil(cols).max(1) {
                let first = if part == 0 { INDENT } else { 0 };
                for (col, bg) in rows[line].iter().enumerate().take(cols).skip(first) {
                    assert_eq!(
                        *bg, ADDED_BG,
                        "cols={cols}: row {line} col {col} lost the diff background"
                    );
                }
                line += 1;
            }
        }
    }
}

/// Catches: filling a wrapped tail with a background that never reached the
/// old right edge (a coloured block in the middle of a plain row).
#[test]
fn reflow_does_not_extend_a_background_that_stopped_before_the_edge() {
    let mut grid = TerminalGrid::new(6, 20, 100);
    // Green spaces at columns 2..6, then plain text up to column 14.
    let _ = grid.process(b"ab\x1b[42m    \x1b[0mplain text\r\n");
    grid.resize_with_mode(6, 10, ReflowMode::All);
    let green = Color::Named(alacritty_terminal::vte::ansi::NamedColor::Green);
    let rows: Vec<Vec<bool>> = backgrounds(&grid)
        .iter()
        .map(|row| row.iter().map(|bg| *bg == green).collect())
        .collect();
    let mut first = vec![false; 10];
    first[2..6].fill(true);
    let mut expected = vec![vec![false; 10]; rows.len()];
    expected[0] = first;
    assert_eq!(
        rows, expected,
        "only the block is green; the wrapped text row stays plain"
    );
}

/// Catches: the edge fill added on the narrow resize survives the widen back
/// as an extra partial coloured row under a long diff row.
#[test]
fn recorded_diff_rows_round_trip_adds_no_coloured_rows() {
    for cols in [94u16, 60] {
        let mut grid = TerminalGrid::new(80, RECORDED_COLS, 1000);
        let _ = grid.process(ROWS);
        grid.resize_with_mode(80, cols, ReflowMode::All);
        grid.resize_with_mode(80, RECORDED_COLS, ReflowMode::All);
        let coloured: Vec<usize> = backgrounds(&grid)
            .iter()
            .enumerate()
            .filter(|(_, row)| row.contains(&ADDED_BG))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            coloured,
            (0..12).collect::<Vec<_>>(),
            "229 -> {cols} -> 229: each diff row must be one coloured row again"
        );
    }
}

/// Catches: padding dropped at the narrow resize is never restored, so after the
/// pane widens back a short diff row is coloured only up to the narrow width.
#[test]
fn recorded_diff_rows_round_trip_keep_background_to_the_edge() {
    let mut grid = TerminalGrid::new(80, RECORDED_COLS, 1000);
    let _ = grid.process(ROWS);
    grid.resize_with_mode(80, 94, ReflowMode::All);
    grid.resize_with_mode(80, RECORDED_COLS, ReflowMode::All);
    let rows = backgrounds(&grid);
    for (line, row) in rows.iter().take(12).enumerate() {
        for (col, bg) in row.iter().enumerate().skip(INDENT) {
            assert_eq!(
                *bg, ADDED_BG,
                "229 -> 94 -> 229: row {line} col {col} lost the diff background"
            );
        }
    }
}

/// Catches: the grow fill paints a row that never reached the old right edge
/// (a block that ended exactly on the narrow width) out to the new edge (#2783-9c6e).
#[test]
fn widen_does_not_extend_a_block_that_ended_on_the_narrow_edge() {
    let mut grid = TerminalGrid::new(6, 100, 100);
    let _ = grid.process(b"\x1b[42m");
    let _ = grid.process(&[b' '; 50]);
    let _ = grid.process(b"\x1b[0m\r\n");
    grid.resize_with_mode(6, 50, ReflowMode::All);
    grid.resize_with_mode(6, 100, ReflowMode::All);
    let green = Color::Named(alacritty_terminal::vte::ansi::NamedColor::Green);
    let row: Vec<bool> = backgrounds(&grid)[0]
        .iter()
        .map(|bg| *bg == green)
        .collect();
    let mut expected = vec![false; 100];
    expected[..50].fill(true);
    assert_eq!(
        row, expected,
        "100 -> 50 -> 100: the block stays 50 cells wide"
    );
}
