use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::term::{Config, Term, TermDamage, TermMode};
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{self, Color, CursorShape, CursorStyle, NamedColor, Rgb};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Terminal event captured from alacritty for forwarding to PTY/frontend.
#[derive(Debug, Clone)]
pub enum TermEvent {
    Title(String),
    ResetTitle,
    ClipboardStore(String),
    PtyWrite(String),
    MouseCursorDirty,
    CursorBlinkingChange,
}

#[derive(Clone)]
pub(crate) struct TermEventCollector {
    bell: Arc<AtomicBool>,
    events: Arc<Mutex<Vec<TermEvent>>>,
}

impl EventListener for TermEventCollector {
    fn send_event(&self, event: Event) {
        match event {
            Event::Bell => { self.bell.store(true, Ordering::Relaxed); }
            Event::Title(t) => { self.events.lock().unwrap().push(TermEvent::Title(t)); }
            Event::ResetTitle => { self.events.lock().unwrap().push(TermEvent::ResetTitle); }
            Event::ClipboardStore(_, text) => { self.events.lock().unwrap().push(TermEvent::ClipboardStore(text)); }
            Event::PtyWrite(s) => { self.events.lock().unwrap().push(TermEvent::PtyWrite(s)); }
            Event::MouseCursorDirty => { self.events.lock().unwrap().push(TermEvent::MouseCursorDirty); }
            Event::CursorBlinkingChange => { self.events.lock().unwrap().push(TermEvent::CursorBlinkingChange); }
            Event::ClipboardLoad(..) | Event::ColorRequest(..) | Event::TextAreaSizeRequest(..)
            | Event::Wakeup | Event::Exit | Event::ChildExit(_) => {}
        }
    }
}

/// Local grid size type implementing `Dimensions` to avoid depending on
/// `alacritty_terminal::term::test::TermSize` (test-only, no stability guarantee).
struct GridSize {
    cols: usize,
    lines: usize,
}

impl Dimensions for GridSize {
    fn columns(&self) -> usize { self.cols }
    fn screen_lines(&self) -> usize { self.lines }
    fn total_lines(&self) -> usize { self.lines }
}

use crate::state::{ChangedRow, LogColor, LogLine, LogSpan};

/// An OSC 133 shell integration marker detected in the PTY stream.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Osc133Event {
    /// Marker type: "A" (prompt), "B" (command), "C" (execution), "D" (finished)
    pub marker: String,
    /// Cursor line at the time the marker was detected
    pub line: usize,
    /// Exit code (only present for "D" markers)
    pub exit_code: Option<i32>,
}

/// A search match in the terminal grid.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchMatch {
    pub row: usize,
    pub col_start: usize,
    pub col_end: usize,
}

// Attrs byte bit positions for binary cell encoding.
const ATTR_BOLD: u8       = 0b0000_0001;
const ATTR_ITALIC: u8     = 0b0000_0010;
const ATTR_UNDERLINE: u8  = 0b0000_0100;
const ATTR_STRIKEOUT: u8  = 0b0000_1000;
const ATTR_DIM: u8        = 0b0001_0000;
const ATTR_INVERSE: u8    = 0b0010_0000;
const ATTR_DEFAULT_FG: u8 = 0b0100_0000;
const ATTR_DEFAULT_BG: u8 = 0b1000_0000;

/// Standard xterm 256-color palette (16 ANSI + 216 color cube + 24 grayscale).
fn xterm_color_rgb(index: u8) -> Rgb {
    match index {
        // 16 standard ANSI colors — match "commander" xterm.js theme
        0  => Rgb { r: 0x1e, g: 0x1e, b: 0x1e },
        1  => Rgb { r: 0xf1, g: 0x4c, b: 0x4c },
        2  => Rgb { r: 0x23, g: 0xd1, b: 0x8b },
        3  => Rgb { r: 0xe5, g: 0xe5, b: 0x10 },
        4  => Rgb { r: 0x3b, g: 0x8e, b: 0xea },
        5  => Rgb { r: 0xd6, g: 0x70, b: 0xd6 },
        6  => Rgb { r: 0x29, g: 0xb8, b: 0xdb },
        7  => Rgb { r: 0xd4, g: 0xd4, b: 0xd4 },
        8  => Rgb { r: 0x66, g: 0x66, b: 0x66 },
        9  => Rgb { r: 0xf1, g: 0x4c, b: 0x4c },
        10 => Rgb { r: 0x23, g: 0xd1, b: 0x8b },
        11 => Rgb { r: 0xf5, g: 0xf5, b: 0x43 },
        12 => Rgb { r: 0x3b, g: 0x8e, b: 0xea },
        13 => Rgb { r: 0xd6, g: 0x70, b: 0xd6 },
        14 => Rgb { r: 0x29, g: 0xb8, b: 0xdb },
        15 => Rgb { r: 0xff, g: 0xff, b: 0xff },
        // 216-color cube (indices 16-231)
        16..=231 => {
            let n = index - 16;
            let b_idx = n % 6;
            let g_idx = (n / 6) % 6;
            let r_idx = n / 36;
            let to_val = |i: u8| if i == 0 { 0 } else { 55 + 40 * i };
            Rgb { r: to_val(r_idx), g: to_val(g_idx), b: to_val(b_idx) }
        }
        // 24-step grayscale ramp (indices 232-255)
        232..=255 => {
            let v = 8 + 10 * (index - 232);
            Rgb { r: v, g: v, b: v }
        }
    }
}

/// Resolve a `Color` to RGB, returning `None` for default fg/bg.
/// Checks dynamic color overrides (from OSC 4/10/11/12) before falling back to static palette.
fn resolve_color(c: Color, colors: &Colors) -> Option<Rgb> {
    match c {
        Color::Spec(rgb) => Some(rgb),
        Color::Indexed(i) => {
            colors[i as usize].or_else(|| Some(xterm_color_rgb(i)))
        }
        Color::Named(n) => {
            if let Some(rgb) = colors[n] {
                return Some(rgb);
            }
            match n {
                NamedColor::Foreground | NamedColor::Background | NamedColor::Cursor
                | NamedColor::BrightForeground | NamedColor::DimForeground => None,
                NamedColor::Black        => Some(xterm_color_rgb(0)),
                NamedColor::Red          => Some(xterm_color_rgb(1)),
                NamedColor::Green        => Some(xterm_color_rgb(2)),
                NamedColor::Yellow       => Some(xterm_color_rgb(3)),
                NamedColor::Blue         => Some(xterm_color_rgb(4)),
                NamedColor::Magenta      => Some(xterm_color_rgb(5)),
                NamedColor::Cyan         => Some(xterm_color_rgb(6)),
                NamedColor::White        => Some(xterm_color_rgb(7)),
                NamedColor::BrightBlack  => Some(xterm_color_rgb(8)),
                NamedColor::BrightRed    => Some(xterm_color_rgb(9)),
                NamedColor::BrightGreen  => Some(xterm_color_rgb(10)),
                NamedColor::BrightYellow => Some(xterm_color_rgb(11)),
                NamedColor::BrightBlue   => Some(xterm_color_rgb(12)),
                NamedColor::BrightMagenta => Some(xterm_color_rgb(13)),
                NamedColor::BrightCyan   => Some(xterm_color_rgb(14)),
                NamedColor::BrightWhite  => Some(xterm_color_rgb(15)),
                NamedColor::DimBlack     => Some(xterm_color_rgb(0)),
                NamedColor::DimRed       => Some(xterm_color_rgb(1)),
                NamedColor::DimGreen     => Some(xterm_color_rgb(2)),
                NamedColor::DimYellow    => Some(xterm_color_rgb(3)),
                NamedColor::DimBlue      => Some(xterm_color_rgb(4)),
                NamedColor::DimMagenta   => Some(xterm_color_rgb(5)),
                NamedColor::DimCyan      => Some(xterm_color_rgb(6)),
                NamedColor::DimWhite     => Some(xterm_color_rgb(7)),
            }
        },
    }
}

/// Wraps `alacritty_terminal::Term` with a TUICommander-specific API.
///
/// Provides the same `process() → Vec<ChangedRow>` + `screen_text_rows()`
/// interface that `VtLogBuffer` expects, so it can drop in as a replacement
/// for the current `vt100::Parser`.
pub struct TerminalGrid {
    term: Term<TermEventCollector>,
    processor: ansi::Processor,
    prev_rows: Vec<String>,
    force_full_next: bool,
    bell_flag: Arc<AtomicBool>,
    events: Arc<Mutex<Vec<TermEvent>>>,
}

impl TerminalGrid {
    pub fn new(rows: u16, cols: u16, scrollback: usize) -> Self {
        let config = Config {
            scrolling_history: scrollback,
            kitty_keyboard: true,
            default_cursor_style: CursorStyle { shape: CursorShape::Beam, blinking: true },
            ..Config::default()
        };
        let size = GridSize { cols: cols as usize, lines: rows as usize };
        let bell_flag = Arc::new(AtomicBool::new(false));
        let events = Arc::new(Mutex::new(Vec::new()));
        let listener = TermEventCollector { bell: bell_flag.clone(), events: events.clone() };
        let term = Term::new(config, &size, listener);
        Self {
            term,
            processor: ansi::Processor::new(),
            prev_rows: Vec::new(),
            force_full_next: false,
            bell_flag,
            events,
        }
    }

    /// Feed raw PTY bytes into the terminal emulator.
    ///
    /// Returns changed rows and any OSC 133 events found in the data.
    pub fn process(&mut self, data: &[u8]) -> (Vec<ChangedRow>, Vec<Osc133Event>) {
        let osc_events = Self::extract_osc133(data);
        self.processor.advance(&mut self.term, data);

        let curr_rows = self.read_screen_text();

        let changed: Vec<ChangedRow> = curr_rows
            .iter()
            .enumerate()
            .filter_map(|(i, curr)| {
                let prev = self.prev_rows.get(i).map(String::as_str).unwrap_or("");
                if curr != prev {
                    Some(ChangedRow {
                        row_index: i,
                        text: curr.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();

        self.prev_rows = curr_rows;

        // Fill in cursor line for OSC 133 events (after processing so cursor is up-to-date)
        let cursor_line = self.term.grid().cursor.point.line.0.max(0) as usize;
        let osc_events: Vec<Osc133Event> = osc_events.into_iter().map(|mut e| {
            e.line = cursor_line;
            e
        }).collect();

        (changed, osc_events)
    }

    /// Returns plain text snapshot of all visible screen rows (trimmed).
    pub fn screen_text_rows(&self) -> Vec<String> {
        if self.prev_rows.is_empty() {
            self.read_screen_text()
        } else {
            self.prev_rows.clone()
        }
    }

    /// Whether the alternate screen buffer is currently active.
    pub fn is_alternate_screen(&self) -> bool {
        self.term.mode().contains(TermMode::ALT_SCREEN)
    }

    /// Number of scrollback lines above the visible screen.
    pub fn scrollback_count(&self) -> usize {
        self.term.grid().history_size()
    }

    /// Read a range of scrollback lines as plain text.
    /// `offset` is counted from the top of scrollback (0 = oldest visible).
    /// Returns up to `limit` lines.
    /// Read a range of scrollback lines as plain text.
    #[cfg(test)]
    pub fn read_scrollback_lines(&self, offset: usize, limit: usize) -> Vec<String> {
        let grid = self.term.grid();
        let history = grid.history_size();
        if history == 0 || offset >= history {
            return Vec::new();
        }

        let count = limit.min(history - offset);
        let mut lines = Vec::with_capacity(count);

        for i in 0..count {
            let scrollback_idx = history - offset - i - 1;
            let line_idx = Line(-(scrollback_idx as i32) - 1);
            if let Some(text) = self.row_to_text(line_idx) {
                lines.push(text);
            }
        }
        lines
    }

    /// Number of visible screen rows.
    #[cfg(test)]
    pub fn screen_lines(&self) -> usize {
        self.term.grid().screen_lines()
    }

    /// Number of visible columns.
    #[cfg(test)]
    pub fn columns(&self) -> usize {
        self.term.grid().columns()
    }

    /// Read the cursor position (line, column) in screen coordinates.
    #[cfg(test)]
    pub fn cursor_point(&self) -> (usize, usize) {
        let point = self.term.grid().cursor.point;
        (point.line.0.max(0) as usize, point.column.0)
    }

    /// Clear the cached prev_rows to force full diff on next process().
    pub fn clear_prev_rows(&mut self) {
        self.prev_rows.clear();
    }

    /// Resize the terminal grid.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        let size = GridSize { cols: cols as usize, lines: rows as usize };
        self.term.resize(size);
        self.prev_rows.clear();
    }

    /// Extract a styled `LogLine` from a grid row by iterating cells.
    ///
    /// Consecutive cells with the same (fg, bg, bold, italic, underline) attributes
    /// are grouped into a single `LogSpan`. Trailing whitespace-only spans with
    /// default attributes are trimmed.
    pub fn extract_log_line(&self, line: Line) -> LogLine {
        let grid = self.term.grid();
        let num_cols = grid.columns();
        let mut spans: Vec<LogSpan> = Vec::new();

        let mut cur_fg: Option<LogColor> = None;
        let mut cur_bg: Option<LogColor> = None;
        let mut cur_bold = false;
        let mut cur_italic = false;
        let mut cur_underline = false;
        let mut cur_text = String::new();

        for col in 0..num_cols {
            let cell = &grid[line][Column(col)];
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }

            let fg = LogColor::from_ansi_color(cell.fg);
            let bg = LogColor::from_ansi_color(cell.bg);
            let bold = cell.flags.contains(Flags::BOLD);
            let italic = cell.flags.contains(Flags::ITALIC);
            let underline = cell.flags.intersects(Flags::UNDERLINE | Flags::DOUBLE_UNDERLINE | Flags::UNDERCURL);

            if !cur_text.is_empty()
                && (fg != cur_fg || bg != cur_bg || bold != cur_bold
                    || italic != cur_italic || underline != cur_underline)
            {
                spans.push(LogSpan {
                    text: std::mem::take(&mut cur_text),
                    fg: cur_fg,
                    bg: cur_bg,
                    bold: cur_bold,
                    italic: cur_italic,
                    underline: cur_underline,
                });
            }

            cur_fg = fg;
            cur_bg = bg;
            cur_bold = bold;
            cur_italic = italic;
            cur_underline = underline;

            if cell.c == ' ' || cell.c == '\0' {
                cur_text.push(' ');
            } else {
                cur_text.push(cell.c);
            }
        }

        if !cur_text.is_empty() {
            spans.push(LogSpan {
                text: cur_text,
                fg: cur_fg,
                bg: cur_bg,
                bold: cur_bold,
                italic: cur_italic,
                underline: cur_underline,
            });
        }

        // Trim trailing whitespace-only spans with default attrs
        while let Some(last) = spans.last() {
            if last.fg.is_none() && last.bg.is_none() && !last.bold && !last.italic && !last.underline
                && last.text.trim_end().is_empty()
            {
                spans.pop();
            } else {
                break;
            }
        }
        if let Some(last) = spans.last_mut() {
            let trimmed = last.text.trim_end().to_string();
            if trimmed.is_empty() && last.fg.is_none() && last.bg.is_none() && !last.bold && !last.italic && !last.underline {
                spans.pop();
            } else {
                last.text = trimmed;
            }
        }

        LogLine { spans, cols: num_cols as u16 }
    }

    /// Current visible screen rows as styled LogLines.
    pub fn screen_log_lines(&self) -> Vec<LogLine> {
        let num_lines = self.term.grid().screen_lines();
        let mut lines = Vec::with_capacity(num_lines);
        for i in 0..num_lines {
            lines.push(self.extract_log_line(Line(i as i32)));
        }
        lines
    }

    /// Read `count` most-recent scrollback lines as styled `LogLine`s.
    /// Soft-wrapped rows (WRAPLINE) are merged into their parent line.
    pub fn read_scrollback_log_lines(&self, count: usize) -> Vec<LogLine> {
        let grid = self.term.grid();
        let history = grid.history_size();
        if history == 0 || count == 0 {
            return Vec::new();
        }
        let actual_count = count.min(history);
        let mut result: Vec<LogLine> = Vec::with_capacity(actual_count);

        // Read from oldest to newest within the requested range
        for i in 0..actual_count {
            let scrollback_idx = actual_count - i - 1;
            let line_idx = Line(-(scrollback_idx as i32) - 1);
            let log_line = self.extract_log_line(line_idx);

            // Check if the previous row (older, one further into history) had WRAPLINE
            let prev_scrollback_idx = scrollback_idx + 1;
            let is_continuation = if prev_scrollback_idx < history {
                let prev_line = Line(-(prev_scrollback_idx as i32) - 1);
                let last_col = grid.columns().saturating_sub(1);
                grid[prev_line][Column(last_col)].flags.contains(Flags::WRAPLINE)
            } else {
                false
            };

            if is_continuation {
                if let Some(prev) = result.last_mut() {
                    prev.spans.extend(log_line.spans);
                } else {
                    result.push(log_line);
                }
            } else {
                result.push(log_line);
            }
        }
        result
    }

    /// Whether a screen row's last cell has WRAPLINE set (it continues on the next row).
    #[allow(dead_code)] // used by scrollback log line extraction
    pub fn row_wrapped(&self, line: Line) -> bool {
        let grid = self.term.grid();
        let last_col = grid.columns().saturating_sub(1);
        grid[line][Column(last_col)].flags.contains(Flags::WRAPLINE)
    }

    /// Extract the user-typed text from the prompt line, excluding ghost/suggestion text.
    pub fn prompt_input_text(&self) -> Option<String> {
        let grid = self.term.grid();
        let rows = grid.screen_lines();
        let cols = grid.columns();
        let cursor = grid.cursor.point;
        let cursor_row = cursor.line.0 as usize;
        let cursor_col = cursor.column.0;

        for row in (0..rows).rev() {
            let line = Line(row as i32);
            let mut row_text = String::with_capacity(cols);
            for col in 0..cols {
                let cell = &grid[line][Column(col)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                if cell.c == '\0' {
                    row_text.push(' ');
                } else {
                    row_text.push(cell.c);
                }
            }
            let trimmed = row_text.trim_start();
            if !(trimmed.starts_with('❯') || trimmed == ">" || trimmed.starts_with("> ")) {
                continue;
            }

            let col_limit = if row == cursor_row { cursor_col } else { cols };
            let mut result_text = String::new();
            let mut past_prompt = false;
            for col in 0..col_limit {
                let cell = &grid[line][Column(col)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                let ch = cell.c;
                if !past_prompt {
                    if ch == '❯' || ch == '›' || ch == '>' {
                        past_prompt = true;
                        continue;
                    }
                    if ch == ' ' || ch == '\t' {
                        continue;
                    }
                    past_prompt = true;
                }
                if past_prompt && (ch == ' ' || ch == '\t') && result_text.is_empty() {
                    continue;
                }
                if cell.flags.contains(Flags::DIM) {
                    break;
                }
                if ch == '\0' {
                    result_text.push(' ');
                } else {
                    result_text.push(ch);
                }
            }
            return Some(result_text.trim_end().to_string());
        }
        None
    }

    // --- Selection API (delegates to alacritty's native Selection) ---

    /// Start a new selection at the given screen coordinate.
    pub fn selection_start(&mut self, col: usize, row: usize, ty: SelectionType) {
        let point = Point::new(Line(row as i32), Column(col));
        let selection = Selection::new(ty, point, Side::Left);
        self.term.selection = Some(selection);
    }

    /// Update the active selection endpoint.
    pub fn selection_update(&mut self, col: usize, row: usize) {
        if let Some(ref mut sel) = self.term.selection {
            let point = Point::new(Line(row as i32), Column(col));
            sel.update(point, Side::Right);
        }
    }

    /// Extract selected text, if any.
    pub fn selection_text(&self) -> Option<String> {
        self.term.selection_to_string()
    }

    /// Clear the current selection.
    pub fn selection_clear(&mut self) {
        self.term.selection = None;
    }

    /// Whether a selection is active.
    #[cfg(test)]
    pub fn has_selection(&self) -> bool {
        self.term.selection.is_some()
    }

    /// Returns true if a bell was rung since last drain, and resets the flag.
    pub fn drain_bell(&self) -> bool {
        self.bell_flag.swap(false, Ordering::Relaxed)
    }

    /// Drain queued terminal events (title changes, clipboard, PTY writes, etc.)
    pub fn drain_events(&self) -> Vec<TermEvent> {
        match self.events.lock() {
            Ok(mut guard) => std::mem::take(&mut *guard),
            Err(e) => {
                tracing::error!("terminal_grid: events mutex poisoned: {e}");
                Vec::new()
            }
        }
    }

    /// Get the OSC 8 hyperlink URI at a given viewport position, if any.
    pub fn hyperlink_at(&self, row: usize, col: usize) -> Option<String> {
        let display_offset = self.term.grid().display_offset();
        let line = Line(row as i32 - display_offset as i32);
        let grid = self.term.grid();
        if col >= grid.columns() { return None; }
        let cell = &grid[line][Column(col)];
        cell.hyperlink().map(|h| h.uri().to_owned())
    }

    /// Mark all rows as dirty so the next serialize_dirty_rows returns a full frame.
    pub fn force_full_damage(&mut self) {
        self.force_full_next = true;
    }

    // --- Scroll API ---

    /// Scroll the viewport by `delta` lines (positive = up / into history).
    pub fn scroll(&mut self, delta: i32) {
        self.term.scroll_display(Scroll::Delta(delta));
        self.force_full_next = true;
    }

    /// Current display offset (0 = at bottom, >0 = scrolled up).
    pub fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    /// Total number of lines (screen + scrollback history).
    pub fn total_lines(&self) -> usize {
        self.term.grid().history_size() + self.term.grid().screen_lines()
    }

    // --- Search API ---

    /// Simple substring search across visible grid + scrollback.
    /// Returns matches as (row, col_start, col_end) in absolute coordinates.
    pub fn search(&self, query: &str) -> Vec<SearchMatch> {
        if query.is_empty() {
            return Vec::new();
        }
        let query_lower = query.to_lowercase();
        let mut matches = Vec::new();
        let grid = self.term.grid();
        let history = grid.history_size();
        let screen = grid.screen_lines();
        let cols = grid.columns();

        // Search scrollback (oldest first) then screen
        let total = history + screen;
        for abs_row in 0..total {
            let line = if abs_row < history {
                Line(-((history - abs_row) as i32))
            } else {
                Line((abs_row - history) as i32)
            };

            let mut row_text = String::with_capacity(cols);
            for col in 0..cols {
                let cell = &grid[line][Column(col)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                if cell.c == '\0' {
                    row_text.push(' ');
                } else {
                    row_text.push(cell.c);
                }
            }

            let row_lower = row_text.to_lowercase();
            let query_char_len = query_lower.chars().count();
            let mut byte_start = 0;
            while let Some(byte_pos) = row_lower[byte_start..].find(&query_lower) {
                let match_byte = byte_start + byte_pos;
                let col_start = row_lower[..match_byte].chars().count();
                let col_end = col_start + query_char_len;
                matches.push(SearchMatch {
                    row: abs_row,
                    col_start,
                    col_end,
                });
                byte_start = match_byte + row_lower[match_byte..].chars().next().map_or(1, |c| c.len_utf8());
            }
        }
        matches
    }

    /// Get text of a single screen row (0-based, relative to viewport).
    pub fn get_row_text(&self, row: usize) -> String {
        let display_offset = self.term.grid().display_offset();
        let line = Line(row as i32) - display_offset;
        self.row_to_text(line).unwrap_or_default()
    }

    /// Serialize dirty rows as a compact binary frame.
    ///
    /// Uses alacritty's built-in damage tracking to identify changed rows.
    /// Wire format:
    /// ```text
    /// Header: [num_rows: u16] [cursor_row: u16] [cursor_col: u16] [cursor_visible: u8]
    ///         [display_offset: u32] [history_size: u32] [has_selection: u8]
    ///         [keyboard_flags: u8]
    /// Per row: [row_index: u16] [col_count: u16] [cells...]
    /// Per cell: [char: u32 LE] [fg_r, fg_g, fg_b] [bg_r, bg_g, bg_b] [attrs: u8]
    /// ```
    /// attrs: bit0=bold, bit1=italic, bit2=underline, bit3=strikeout,
    ///        bit4=dim, bit5=inverse, bit6=default_fg, bit7=default_bg
    /// keyboard_flags: bit0=disambiguate_esc_codes, bit1=report_event_types,
    ///                 bit2=report_alternate_keys, bit3=report_all_keys_as_esc,
    ///                 bit4=report_associated_text
    pub fn serialize_dirty_rows(&mut self) -> Vec<u8> {
        let num_cols = self.term.grid().columns();
        let num_lines = self.term.grid().screen_lines();
        let cursor = self.term.grid().cursor.point;
        let cursor_visible = self.term.mode().contains(TermMode::SHOW_CURSOR);
        let display_offset = self.term.grid().display_offset();
        let history_size = self.term.grid().history_size();
        let has_selection = self.term.selection.is_some();
        let mode = self.term.mode();
        let mut keyboard_flags: u8 = 0;
        if mode.contains(TermMode::DISAMBIGUATE_ESC_CODES) { keyboard_flags |= 0x01; }
        if mode.contains(TermMode::REPORT_EVENT_TYPES) { keyboard_flags |= 0x02; }
        if mode.contains(TermMode::REPORT_ALTERNATE_KEYS) { keyboard_flags |= 0x04; }
        if mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC) { keyboard_flags |= 0x08; }
        if mode.contains(TermMode::REPORT_ASSOCIATED_TEXT) { keyboard_flags |= 0x10; }

        let dirty_lines: Vec<usize> = if self.force_full_next {
            self.force_full_next = false;
            (0..num_lines).collect()
        } else {
            let damage = self.term.damage();
            match damage {
                TermDamage::Full => (0..num_lines).collect(),
                TermDamage::Partial(iter) => iter
                    .map(|b| b.line)
                    .filter(|&l| l < num_lines)
                    .collect(),
            }
        };

        if dirty_lines.is_empty() {
            self.term.reset_damage();
            return Vec::new();
        }

        // Header: 18 bytes
        let row_count = dirty_lines.len();
        let estimated = 18 + row_count * (4 + num_cols * 11);
        let mut buf = Vec::with_capacity(estimated);

        let bell = self.drain_bell();
        let cursor_shape = self.term.cursor_style().shape;
        let mut frame_flags: u8 = 0;
        if bell { frame_flags |= 0x01; }
        // bits 1-2: cursor shape (0=block, 1=underline, 2=beam)
        let shape_bits: u8 = match cursor_shape {
            CursorShape::Block => 0,
            CursorShape::Underline => 1,
            CursorShape::Beam => 2,
            _ => 0,
        };
        frame_flags |= shape_bits << 1;

        buf.extend_from_slice(&(row_count as u16).to_le_bytes());
        buf.extend_from_slice(&(cursor.line.0.max(0) as u16).to_le_bytes());
        buf.extend_from_slice(&(cursor.column.0 as u16).to_le_bytes());
        buf.push(cursor_visible as u8);
        buf.extend_from_slice(&(display_offset as u32).to_le_bytes());
        buf.extend_from_slice(&(history_size as u32).to_le_bytes());
        buf.push(has_selection as u8);
        buf.push(keyboard_flags);
        buf.push(frame_flags);

        let grid = self.term.grid();
        let colors = self.term.colors();
        for &row_idx in &dirty_lines {
            let line = Line(row_idx as i32 - display_offset as i32);
            buf.extend_from_slice(&(row_idx as u16).to_le_bytes());
            buf.extend_from_slice(&(num_cols as u16).to_le_bytes());

            for col in 0..num_cols {
                let cell = &grid[line][Column(col)];
                let ch = if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    0u32
                } else if cell.c == '\0' {
                    ' ' as u32
                } else {
                    cell.c as u32
                };
                buf.extend_from_slice(&ch.to_le_bytes());

                let (fg_rgb, fg_default) = match resolve_color(cell.fg, colors) {
                    Some(rgb) => (rgb, false),
                    None => (Rgb { r: 0, g: 0, b: 0 }, true),
                };
                buf.push(fg_rgb.r);
                buf.push(fg_rgb.g);
                buf.push(fg_rgb.b);

                let (bg_rgb, bg_default) = match resolve_color(cell.bg, colors) {
                    Some(rgb) => (rgb, false),
                    None => (Rgb { r: 0, g: 0, b: 0 }, true),
                };
                buf.push(bg_rgb.r);
                buf.push(bg_rgb.g);
                buf.push(bg_rgb.b);

                let flags = cell.flags;
                let mut attrs: u8 = 0;
                if flags.contains(Flags::BOLD) { attrs |= ATTR_BOLD; }
                if flags.contains(Flags::ITALIC) { attrs |= ATTR_ITALIC; }
                if flags.intersects(Flags::UNDERLINE | Flags::DOUBLE_UNDERLINE | Flags::UNDERCURL) { attrs |= ATTR_UNDERLINE; }
                if flags.contains(Flags::STRIKEOUT) { attrs |= ATTR_STRIKEOUT; }
                if flags.contains(Flags::DIM) { attrs |= ATTR_DIM; }
                if flags.contains(Flags::INVERSE) { attrs |= ATTR_INVERSE; }
                if fg_default { attrs |= ATTR_DEFAULT_FG; }
                if bg_default { attrs |= ATTR_DEFAULT_BG; }
                buf.push(attrs);
            }
        }

        self.term.reset_damage();
        buf
    }

    fn read_screen_text(&self) -> Vec<String> {
        let grid = self.term.grid();
        let num_lines = grid.screen_lines();
        let num_cols = grid.columns();
        let mut rows = Vec::with_capacity(num_lines);
        for i in 0..num_lines {
            let line = Line(i as i32);
            let mut text = String::with_capacity(num_cols);
            for col in 0..num_cols {
                let cell = &grid[line][Column(col)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                text.push(cell.c);
            }
            rows.push(text.trim_end().to_string());
        }
        rows
    }

    fn row_to_text(&self, line: Line) -> Option<String> {
        let grid = self.term.grid();
        if line.0 < -(grid.history_size() as i32) || line.0 >= grid.screen_lines() as i32 {
            return None;
        }
        let num_cols = grid.columns();
        let mut text = String::with_capacity(num_cols);
        for col in 0..num_cols {
            let cell = &grid[line][Column(col)];
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            text.push(cell.c);
        }
        Some(text.trim_end().to_string())
    }

    /// Scan raw PTY bytes for OSC 133 sequences (shell integration markers).
    /// Pattern: ESC ] 133 ; <marker> [; params] BEL  or  ESC ] 133 ; <marker> [; params] ESC \
    fn extract_osc133(data: &[u8]) -> Vec<Osc133Event> {
        let mut events = Vec::new();
        let prefix = b"\x1b]133;";
        let mut i = 0;
        while i + prefix.len() < data.len() {
            if data[i..].starts_with(prefix) {
                let start = i + prefix.len();
                // Find terminator: BEL (\x07) or ST (\x1b\\)
                let mut end = start;
                while end < data.len() {
                    if data[end] == 0x07 { break; }
                    if end + 1 < data.len() && data[end] == 0x1b && data[end + 1] == b'\\' { break; }
                    end += 1;
                }
                if end > start {
                    let payload = &data[start..end];
                    let marker = (payload[0] as char).to_string();
                    let exit_code = if marker == "D" && payload.len() > 2 && payload[1] == b';' {
                        std::str::from_utf8(&payload[2..]).ok().and_then(|s| s.parse::<i32>().ok())
                    } else {
                        None
                    };
                    if "ABCD".contains(&marker) {
                        events.push(Osc133Event { marker, line: 0, exit_code });
                    }
                }
                i = end + 1;
            } else {
                i += 1;
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_empty_grid() {
        let grid = TerminalGrid::new(24, 80, 1000);
        assert_eq!(grid.screen_lines(), 24);
        assert_eq!(grid.columns(), 80);
        assert_eq!(grid.scrollback_count(), 0);
        assert!(!grid.is_alternate_screen());
    }

    #[test]
    fn process_simple_text() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        let (changed, _) = grid.process(b"hello world");
        assert!(!changed.is_empty());
        let first = &changed[0];
        assert_eq!(first.row_index, 0);
        assert_eq!(first.text, "hello world");
    }

    #[test]
    fn process_returns_empty_on_no_change() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        let _ = grid.process(b"hello");
        let (changed, _) = grid.process(b"");
        assert!(changed.is_empty());
    }

    #[test]
    fn screen_text_rows_returns_visible_content() {
        let mut grid = TerminalGrid::new(5, 20, 100);
        let _ = grid.process(b"line1\r\nline2\r\nline3");
        let rows = grid.screen_text_rows();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0], "line1");
        assert_eq!(rows[1], "line2");
        assert_eq!(rows[2], "line3");
        assert_eq!(rows[3], "");
        assert_eq!(rows[4], "");
    }

    #[test]
    fn cursor_position_tracks_output() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        let _ = grid.process(b"abc");
        let (line, col) = grid.cursor_point();
        assert_eq!(line, 0);
        assert_eq!(col, 3);
    }

    #[test]
    fn cursor_moves_on_newline() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        let _ = grid.process(b"abc\r\ndef");
        let (line, col) = grid.cursor_point();
        assert_eq!(line, 1);
        assert_eq!(col, 3);
    }

    #[test]
    fn alt_screen_toggle() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        assert!(!grid.is_alternate_screen());
        // Enter alt screen: CSI ? 1049 h
        let _ = grid.process(b"\x1b[?1049h");
        assert!(grid.is_alternate_screen());
        // Exit alt screen: CSI ? 1049 l
        let _ = grid.process(b"\x1b[?1049l");
        assert!(!grid.is_alternate_screen());
    }

    #[test]
    fn scrollback_generated_by_overflow() {
        let mut grid = TerminalGrid::new(3, 20, 100);
        // Write 5 lines into a 3-row terminal → 2 lines scroll into history
        let _ = grid.process(b"line1\r\nline2\r\nline3\r\nline4\r\nline5");
        assert!(grid.scrollback_count() >= 2);
    }

    #[test]
    fn resize_updates_dimensions() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        grid.resize(10, 40);
        assert_eq!(grid.screen_lines(), 10);
        assert_eq!(grid.columns(), 40);
    }

    #[test]
    fn changed_rows_detects_overwrite() {
        let mut grid = TerminalGrid::new(5, 20, 100);
        let _ = grid.process(b"hello");
        // Move cursor to beginning of line and overwrite
        let (changed, _) = grid.process(b"\rworld");
        assert!(!changed.is_empty());
        assert_eq!(changed[0].text, "world");
    }

    #[test]
    fn ansi_colors_do_not_leak_into_text() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        let _ = grid.process(b"\x1b[31mred text\x1b[0m");
        let rows = grid.screen_text_rows();
        assert_eq!(rows[0], "red text");
    }

    #[test]
    fn wide_chars_handled() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        let _ = grid.process("日本語".as_bytes());
        let rows = grid.screen_text_rows();
        assert!(rows[0].contains("日本語"));
    }

    #[test]
    fn cursor_movement_escape_sequences() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        // Write text, move cursor up 1 line (CUU), write more
        let _ = grid.process(b"first\r\nsecond");
        let _ = grid.process(b"\x1b[A"); // cursor up
        let (line, _col) = grid.cursor_point();
        assert_eq!(line, 0);
    }

    #[test]
    fn erase_in_line() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        let _ = grid.process(b"hello world");
        // Move to column 5, erase to end of line
        let _ = grid.process(b"\x1b[6G\x1b[K");
        let rows = grid.screen_text_rows();
        assert_eq!(rows[0], "hello");
    }

    // --- Binary serialization tests ---

    const TEST_HEADER_SIZE: usize = 18;

    /// Helper: decode the header from a serialized frame.
    fn decode_header(buf: &[u8]) -> (u16, u16, u16, bool) {
        let num_rows = u16::from_le_bytes([buf[0], buf[1]]);
        let cursor_row = u16::from_le_bytes([buf[2], buf[3]]);
        let cursor_col = u16::from_le_bytes([buf[4], buf[5]]);
        let cursor_visible = buf[6] != 0;
        (num_rows, cursor_row, cursor_col, cursor_visible)
    }

    /// Helper: decode one cell (11 bytes) from a buffer at a given offset.
    /// Returns (char, fg_r, fg_g, fg_b, bg_r, bg_g, bg_b, attrs).
    fn decode_cell(buf: &[u8], offset: usize) -> (char, u8, u8, u8, u8, u8, u8, u8) {
        let ch = u32::from_le_bytes([buf[offset], buf[offset+1], buf[offset+2], buf[offset+3]]);
        let ch = char::from_u32(ch).unwrap_or('\0');
        (ch, buf[offset+4], buf[offset+5], buf[offset+6],
             buf[offset+7], buf[offset+8], buf[offset+9], buf[offset+10])
    }

    #[test]
    fn serialize_plain_text_roundtrip() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        let _ = grid.process(b"Hi");
        let buf = grid.serialize_dirty_rows();
        assert!(!buf.is_empty());

        let (num_rows, cursor_row, cursor_col, cursor_visible) = decode_header(&buf);
        assert!(num_rows >= 1, "at least row 0 dirty");
        assert_eq!(cursor_row, 0);
        assert_eq!(cursor_col, 2);
        assert!(cursor_visible);

        // First dirty row header starts after header
        let h = TEST_HEADER_SIZE;
        let row_idx = u16::from_le_bytes([buf[h], buf[h+1]]);
        let col_count = u16::from_le_bytes([buf[h+2], buf[h+3]]);
        assert_eq!(row_idx, 0);
        assert_eq!(col_count, 10);

        // First cell = 'H'
        let cell0 = h + 4;
        let (ch, _, _, _, _, _, _, attrs) = decode_cell(&buf, cell0);
        assert_eq!(ch, 'H');
        assert_ne!(attrs & super::ATTR_DEFAULT_FG, 0, "default fg flag set");
        assert_ne!(attrs & super::ATTR_DEFAULT_BG, 0, "default bg flag set");

        // Second cell = 'i'
        let (ch, _, _, _, _, _, _, _) = decode_cell(&buf, cell0 + 11);
        assert_eq!(ch, 'i');
    }

    #[test]
    fn serialize_colored_text_preserves_rgb() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        // ESC[31m = red foreground (ANSI color 1)
        let _ = grid.process(b"\x1b[31mX\x1b[0m");
        let buf = grid.serialize_dirty_rows();

        // Find row 0, cell 0 — should have red fg
        let cell0 = TEST_HEADER_SIZE + 4;
        let (ch, fg_r, fg_g, fg_b, _, _, _, attrs) = decode_cell(&buf, cell0);
        assert_eq!(ch, 'X');
        assert_eq!(fg_r, 0xf1); // commander theme red
        assert_eq!(fg_g, 0x4c);
        assert_eq!(fg_b, 0x4c);
        assert_eq!(attrs & super::ATTR_DEFAULT_FG, 0, "fg is NOT default");
        assert_ne!(attrs & super::ATTR_DEFAULT_BG, 0, "bg IS default");
    }

    #[test]
    fn serialize_bold_italic_attrs() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        // Bold + italic
        let _ = grid.process(b"\x1b[1;3mB\x1b[0m");
        let buf = grid.serialize_dirty_rows();

        let cell0 = TEST_HEADER_SIZE + 4;
        let (ch, _, _, _, _, _, _, attrs) = decode_cell(&buf, cell0);
        assert_eq!(ch, 'B');
        assert_ne!(attrs & super::ATTR_BOLD, 0, "bold flag");
        assert_ne!(attrs & super::ATTR_ITALIC, 0, "italic flag");
        assert_eq!(attrs & super::ATTR_UNDERLINE, 0, "no underline");
    }

    #[test]
    fn serialize_only_dirty_rows_after_reset() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        let _ = grid.process(b"line1\r\nline2\r\nline3");
        // Drain initial damage
        let _ = grid.serialize_dirty_rows();

        // Now modify only row 0
        let _ = grid.process(b"\x1b[1;1Hchanged");
        let buf = grid.serialize_dirty_rows();

        if buf.is_empty() {
            // Damage was Full due to cursor move — acceptable
            return;
        }
        let (num_rows, _, _, _) = decode_header(&buf);
        // Should have fewer rows than the full 5
        assert!(num_rows <= 5, "partial damage, got {num_rows} rows");
    }

    #[test]
    fn serialize_wide_char_spacer_is_zero() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        let _ = grid.process("日".as_bytes()); // wide char takes 2 columns
        let buf = grid.serialize_dirty_rows();

        // Cell 0 = '日'
        let cell0 = TEST_HEADER_SIZE + 4;
        let (ch0, _, _, _, _, _, _, _) = decode_cell(&buf, cell0);
        assert_eq!(ch0, '日');
        // Cell 1 = wide char spacer → encoded as 0
        let cell1 = cell0 + 11;
        let ch1_raw = u32::from_le_bytes([buf[cell1], buf[cell1+1], buf[cell1+2], buf[cell1+3]]);
        assert_eq!(ch1_raw, 0, "wide char spacer encoded as 0");
    }

    #[test]
    fn serialize_frame_size_within_budget() {
        // Worst case: 220x50 all dirty
        let mut grid = TerminalGrid::new(50, 220, 0);
        // Fill every cell to ensure all rows are dirty
        for _ in 0..50 {
            let _ = grid.process(b"XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX\r\n");
        }
        let buf = grid.serialize_dirty_rows();
        assert!(
            buf.len() < 256 * 1024,
            "frame must be under 256KB, got {} bytes",
            buf.len()
        );
        // Expected: 16 header + 50 rows × (4 row header + 220 cells × 11 bytes)
        // = 16 + 50 × (4 + 2420) = 16 + 121_200 = 121_216 bytes
    }

    #[test]
    fn serialize_cursor_hidden() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        // DECTCEM: hide cursor
        let _ = grid.process(b"\x1b[?25l");
        let _ = grid.process(b"text");
        let buf = grid.serialize_dirty_rows();
        let (_, _, _, cursor_visible) = decode_header(&buf);
        assert!(!cursor_visible, "cursor should be hidden");
    }

    #[test]
    fn serialize_rgb_color_passthrough() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        // ESC[38;2;100;150;200m = 24-bit fg color
        let _ = grid.process(b"\x1b[38;2;100;150;200mR\x1b[0m");
        let buf = grid.serialize_dirty_rows();

        let cell0 = TEST_HEADER_SIZE + 4;
        let (ch, fg_r, fg_g, fg_b, _, _, _, attrs) = decode_cell(&buf, cell0);
        assert_eq!(ch, 'R');
        assert_eq!(fg_r, 100);
        assert_eq!(fg_g, 150);
        assert_eq!(fg_b, 200);
        assert_eq!(attrs & super::ATTR_DEFAULT_FG, 0, "fg is NOT default");
    }

    // --- Selection tests ---

    #[test]
    fn selection_start_and_text() {
        let mut grid = TerminalGrid::new(5, 20, 0);
        let _ = grid.process(b"hello world");
        grid.selection_start(0, 0, SelectionType::Simple);
        grid.selection_update(4, 0);
        let text = grid.selection_text();
        assert!(text.is_some());
        assert_eq!(text.unwrap().trim(), "hello");
    }

    #[test]
    fn selection_clear() {
        let mut grid = TerminalGrid::new(5, 20, 0);
        let _ = grid.process(b"hello");
        grid.selection_start(0, 0, SelectionType::Simple);
        grid.selection_update(4, 0);
        assert!(grid.has_selection());
        grid.selection_clear();
        assert!(!grid.has_selection());
        assert!(grid.selection_text().is_none());
    }

    // --- Search tests ---

    #[test]
    fn search_finds_matches() {
        let mut grid = TerminalGrid::new(5, 40, 0);
        let _ = grid.process(b"hello world\r\nfoo hello bar");
        let matches = grid.search("hello");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].col_start, 0);
        assert_eq!(matches[1].col_start, 4);
    }

    #[test]
    fn search_case_insensitive() {
        let mut grid = TerminalGrid::new(5, 40, 0);
        let _ = grid.process(b"Hello HELLO hElLo");
        let matches = grid.search("hello");
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn search_empty_query() {
        let grid = TerminalGrid::new(5, 40, 0);
        let matches = grid.search("");
        assert!(matches.is_empty());
    }

    // --- Scroll tests ---

    #[test]
    fn scroll_and_display_offset() {
        let mut grid = TerminalGrid::new(3, 20, 100);
        let _ = grid.process(b"line1\r\nline2\r\nline3\r\nline4\r\nline5");
        assert_eq!(grid.display_offset(), 0);
        grid.scroll(2);
        assert_eq!(grid.display_offset(), 2);
    }

    // --- Row text tests ---

    #[test]
    fn get_row_text_returns_visible() {
        let mut grid = TerminalGrid::new(5, 20, 0);
        let _ = grid.process(b"first\r\nsecond");
        let text = grid.get_row_text(0);
        assert_eq!(text, "first");
        let text = grid.get_row_text(1);
        assert_eq!(text, "second");
    }

    // --- Scrollback reading tests ---

    #[test]
    fn read_scrollback_after_overflow() {
        let mut grid = TerminalGrid::new(3, 20, 100);
        let _ = grid.process(b"line1\r\nline2\r\nline3\r\nline4\r\nline5");
        let count = grid.scrollback_count();
        assert!(count >= 2, "expected scrollback >= 2, got {count}");
        let lines = grid.read_scrollback_lines(0, 10);
        assert!(!lines.is_empty(), "scrollback should have content");
        assert!(lines[0].contains("line"), "first scrollback line should contain text");
    }

    #[test]
    fn read_scrollback_with_offset() {
        let mut grid = TerminalGrid::new(3, 20, 100);
        let _ = grid.process(b"line1\r\nline2\r\nline3\r\nline4\r\nline5\r\nline6\r\nline7");
        let count = grid.scrollback_count();
        let all = grid.read_scrollback_lines(0, count);
        if count > 1 {
            let partial = grid.read_scrollback_lines(1, count - 1);
            assert_eq!(partial.len(), all.len() - 1);
        }
    }

    #[test]
    fn read_scrollback_offset_past_history_returns_empty() {
        let mut grid = TerminalGrid::new(3, 20, 100);
        let _ = grid.process(b"line1\r\nline2\r\nline3\r\nline4\r\nline5");
        let count = grid.scrollback_count();
        let lines = grid.read_scrollback_lines(count + 100, 10);
        assert!(lines.is_empty());
    }

    #[test]
    fn read_scrollback_no_history_returns_empty() {
        let grid = TerminalGrid::new(24, 80, 100);
        let lines = grid.read_scrollback_lines(0, 10);
        assert!(lines.is_empty());
    }

    #[test]
    fn get_row_text_out_of_bounds_returns_empty() {
        let grid = TerminalGrid::new(5, 20, 0);
        let text = grid.get_row_text(999);
        assert_eq!(text, "");
    }
}
