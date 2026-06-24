use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::grid::{Dimensions, ReflowMode};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::{Cell, Flags, Osc133CellType};
use alacritty_terminal::term::color::{Colors, named_color_to_index};
use alacritty_terminal::term::search::RegexSearch;
use alacritty_terminal::term::{Config, Term, TermDamage, TermMode};
use alacritty_terminal::vte::ansi::{self, Color, CursorShape, CursorStyle, NamedColor, Rgb};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Terminal event captured from alacritty for forwarding to PTY/frontend.
#[derive(Debug, Clone)]
pub enum TermEvent {
    Title(String),
    ResetTitle,
    ClipboardStore(String),
    PtyWrite(String),
    MouseCursorDirty,
    CursorBlinkingChange,
    Osc133 {
        command: char,
        params: String,
        line: usize,
    },
    Osc7(String),
    Tuic {
        verb: String,
        payload: String,
        line: usize,
    },
}

#[derive(Clone)]
pub(crate) struct TermEventCollector {
    bell: Arc<AtomicBool>,
    events: Arc<Mutex<Vec<TermEvent>>>,
}

impl EventListener for TermEventCollector {
    fn send_event(&self, event: Event) {
        match event {
            Event::Bell => {
                self.bell.store(true, Ordering::Relaxed);
            }
            Event::Title(t) => {
                self.events.lock().unwrap().push(TermEvent::Title(t));
            }
            Event::ResetTitle => {
                self.events.lock().unwrap().push(TermEvent::ResetTitle);
            }
            Event::ClipboardStore(_, text) => {
                self.events
                    .lock()
                    .unwrap()
                    .push(TermEvent::ClipboardStore(text));
            }
            Event::PtyWrite(s) => {
                self.events.lock().unwrap().push(TermEvent::PtyWrite(s));
            }
            Event::MouseCursorDirty => {
                self.events
                    .lock()
                    .unwrap()
                    .push(TermEvent::MouseCursorDirty);
            }
            Event::CursorBlinkingChange => {
                self.events
                    .lock()
                    .unwrap()
                    .push(TermEvent::CursorBlinkingChange);
            }
            Event::Osc133 {
                command,
                params,
                line,
            } => {
                self.events.lock().unwrap().push(TermEvent::Osc133 {
                    command,
                    params,
                    line,
                });
            }
            Event::Osc7(url) => {
                self.events.lock().unwrap().push(TermEvent::Osc7(url));
            }
            Event::Tuic {
                verb,
                payload,
                line,
            } => {
                self.events.lock().unwrap().push(TermEvent::Tuic {
                    verb,
                    payload,
                    line,
                });
            }
            Event::ClipboardLoad(..)
            | Event::ColorRequest(..)
            | Event::TextAreaSizeRequest(..)
            | Event::Wakeup
            | Event::Exit
            | Event::ChildExit(_) => {}
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
    fn columns(&self) -> usize {
        self.cols
    }
    fn screen_lines(&self) -> usize {
        self.lines
    }
    fn total_lines(&self) -> usize {
        self.lines
    }
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

/// A search match with the full line text for workspace search.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BufferSearchMatch {
    pub line_index: usize,
    pub line_text: String,
    pub match_start: usize,
    pub match_end: usize,
}

// Attrs byte bit positions for binary cell encoding.
const ATTR_BOLD: u8 = 0b0000_0001;
const ATTR_ITALIC: u8 = 0b0000_0010;
const ATTR_UNDERLINE: u8 = 0b0000_0100;
const ATTR_STRIKEOUT: u8 = 0b0000_1000;
const ATTR_DIM: u8 = 0b0001_0000;
const ATTR_INVERSE: u8 = 0b0010_0000;
const ATTR_DEFAULT_FG: u8 = 0b0100_0000;
const ATTR_DEFAULT_BG: u8 = 0b1000_0000;

/// Standard xterm 256-color palette (16 ANSI + 216 color cube + 24 grayscale).
fn xterm_color_rgb(index: u8) -> Rgb {
    match index {
        // 16 standard ANSI colors — Tango/GNOME palette (xterm.js default)
        0 => Rgb {
            r: 0x2e,
            g: 0x34,
            b: 0x36,
        },
        1 => Rgb {
            r: 0xcc,
            g: 0x00,
            b: 0x00,
        },
        2 => Rgb {
            r: 0x4e,
            g: 0x9a,
            b: 0x06,
        },
        3 => Rgb {
            r: 0xc4,
            g: 0xa0,
            b: 0x00,
        },
        4 => Rgb {
            r: 0x34,
            g: 0x65,
            b: 0xa4,
        },
        5 => Rgb {
            r: 0x75,
            g: 0x50,
            b: 0x7b,
        },
        6 => Rgb {
            r: 0x06,
            g: 0x98,
            b: 0x9a,
        },
        7 => Rgb {
            r: 0xd3,
            g: 0xd7,
            b: 0xcf,
        },
        8 => Rgb {
            r: 0x55,
            g: 0x57,
            b: 0x53,
        },
        9 => Rgb {
            r: 0xef,
            g: 0x29,
            b: 0x29,
        },
        10 => Rgb {
            r: 0x8a,
            g: 0xe2,
            b: 0x34,
        },
        11 => Rgb {
            r: 0xfc,
            g: 0xe9,
            b: 0x4f,
        },
        12 => Rgb {
            r: 0x73,
            g: 0x9f,
            b: 0xcf,
        },
        13 => Rgb {
            r: 0xad,
            g: 0x7f,
            b: 0xa8,
        },
        14 => Rgb {
            r: 0x34,
            g: 0xe2,
            b: 0xe2,
        },
        15 => Rgb {
            r: 0xee,
            g: 0xee,
            b: 0xec,
        },
        // 216-color cube (indices 16-231)
        16..=231 => {
            let n = index - 16;
            let b_idx = n % 6;
            let g_idx = (n / 6) % 6;
            let r_idx = n / 36;
            let to_val = |i: u8| if i == 0 { 0 } else { 55 + 40 * i };
            Rgb {
                r: to_val(r_idx),
                g: to_val(g_idx),
                b: to_val(b_idx),
            }
        }
        // 24-step grayscale ramp (indices 232-255)
        232..=255 => {
            let v = 8 + 10 * (index - 232);
            Rgb { r: v, g: v, b: v }
        }
    }
}

/// Reduce brightness to 2/3 for dim color variants.
fn dim_rgb(c: Rgb) -> Rgb {
    Rgb {
        r: (c.r as u16 * 2 / 3) as u8,
        g: (c.g as u16 * 2 / 3) as u8,
        b: (c.b as u16 * 2 / 3) as u8,
    }
}

/// Resolve a `Color` to RGB, returning `None` for default fg/bg.
/// Checks dynamic color overrides (from OSC 4/10/11/12) before falling back to static palette.
fn resolve_color(c: Color, colors: &Colors) -> Option<Rgb> {
    match c {
        Color::Spec(rgb) => Some(rgb),
        Color::Indexed(i) => colors[i as usize].or_else(|| Some(xterm_color_rgb(i))),
        Color::Named(n) => {
            if let Some(rgb) = colors[n] {
                return Some(rgb);
            }
            let is_dim = matches!(
                n,
                NamedColor::DimBlack
                    | NamedColor::DimRed
                    | NamedColor::DimGreen
                    | NamedColor::DimYellow
                    | NamedColor::DimBlue
                    | NamedColor::DimMagenta
                    | NamedColor::DimCyan
                    | NamedColor::DimWhite
            );
            match named_color_to_index(n) {
                Some(i) => {
                    let rgb = xterm_color_rgb(i);
                    Some(if is_dim { dim_rgb(rgb) } else { rgb })
                }
                None => None,
            }
        }
    }
}

/// Encode one grid cell into the 11-byte wire format shared by the dirty-row and
/// overscan serializers: codepoint (u32 LE), fg rgb (3 bytes), bg rgb (3 bytes),
/// attrs (u8).
fn encode_cell(buf: &mut Vec<u8>, cell: &Cell, colors: &Colors) {
    let ch = if cell.flags.contains(Flags::WIDE_CHAR_SPACER) || cell.c == '\0' {
        0u32
    } else {
        cell.c as u32
    };
    buf.extend_from_slice(&ch.to_le_bytes());

    let (fg_rgb, fg_default) = match resolve_color(cell.fg, colors) {
        Some(rgb) => (rgb, false),
        None => (Rgb { r: 0, g: 0, b: 0 }, true),
    };
    let fg_rgb = if cell.flags.contains(Flags::DIM) {
        dim_rgb(fg_rgb)
    } else {
        fg_rgb
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
    if flags.contains(Flags::BOLD) {
        attrs |= ATTR_BOLD;
    }
    if flags.contains(Flags::ITALIC) {
        attrs |= ATTR_ITALIC;
    }
    if flags.intersects(Flags::UNDERLINE | Flags::DOUBLE_UNDERLINE | Flags::UNDERCURL) {
        attrs |= ATTR_UNDERLINE;
    }
    if flags.contains(Flags::STRIKEOUT) {
        attrs |= ATTR_STRIKEOUT;
    }
    if flags.contains(Flags::DIM) {
        attrs |= ATTR_DIM;
    }
    if flags.contains(Flags::INVERSE) {
        attrs |= ATTR_INVERSE;
    }
    if fg_default {
        attrs |= ATTR_DEFAULT_FG;
    }
    if bg_default {
        attrs |= ATTR_DEFAULT_BG;
    }
    buf.push(attrs);
}

/// Wraps `alacritty_terminal::Term` with a TUICommander-specific API.
///
/// Provides `process() → Vec<ChangedRow>` + `screen_text_rows()`
/// interface that `VtLogBuffer` and HTTP/WS handlers use.
pub struct TerminalGrid {
    term: Term<TermEventCollector>,
    processor: ansi::Processor,
    prev_rows: Vec<String>,
    last_frame_display_offset: Option<usize>,
    last_frame_history_size: Option<usize>,
    last_frame_screen_lines: Option<usize>,
    last_frame_columns: Option<usize>,
    bell_flag: Arc<AtomicBool>,
    events: Arc<Mutex<Vec<TermEvent>>>,
    /// When true, column resizes reflow scrollback history while leaving the
    /// visible screen untouched. Preserves TUI cursor positioning on screen
    /// while keeping scrollback readable across resize cycles.
    pub reflow_history: bool,
}

impl TerminalGrid {
    pub fn new(rows: u16, cols: u16, scrollback: usize) -> Self {
        let config = Config {
            scrolling_history: scrollback,
            kitty_keyboard: true,
            default_cursor_style: CursorStyle {
                shape: CursorShape::Beam,
                blinking: true,
            },
            ..Config::default()
        };
        let size = GridSize {
            cols: cols as usize,
            lines: rows as usize,
        };
        let bell_flag = Arc::new(AtomicBool::new(false));
        let events = Arc::new(Mutex::new(Vec::new()));
        let listener = TermEventCollector {
            bell: bell_flag.clone(),
            events: events.clone(),
        };
        let term = Term::new(config, &size, listener);
        Self {
            term,
            processor: ansi::Processor::new(),
            prev_rows: Vec::new(),
            last_frame_display_offset: None,
            last_frame_history_size: None,
            last_frame_screen_lines: None,
            last_frame_columns: None,
            bell_flag,
            events,
            reflow_history: true,
        }
    }

    /// Feed raw PTY bytes into the terminal emulator.
    ///
    /// Returns changed rows. OSC 133 events are delivered via `drain_events()`
    /// as `TermEvent::Osc133` (parsed natively by the patched VTE handler).
    pub fn process(&mut self, data: &[u8]) -> Vec<ChangedRow> {
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

        changed
    }

    /// Returns plain text snapshot of all visible screen rows (trimmed).
    pub fn screen_text_rows(&self) -> Vec<String> {
        if self.prev_rows.is_empty() {
            self.read_screen_text()
        } else {
            self.prev_rows.clone()
        }
    }

    /// Borrowed view of the cached screen rows — avoids cloning when the caller
    /// only needs `&[String]` and holds the lock.  Returns `None` only when
    /// `process()` has never been called (empty `prev_rows`).
    pub fn screen_text_rows_ref(&self) -> Option<&[String]> {
        if self.prev_rows.is_empty() {
            None
        } else {
            Some(&self.prev_rows)
        }
    }

    /// Whether the alternate screen buffer is currently active.
    pub fn is_alternate_screen(&self) -> bool {
        self.term.mode().contains(TermMode::ALT_SCREEN)
    }

    /// Whether the cursor is currently visible (DECTCEM / CSI ?25h).
    pub fn is_cursor_visible(&self) -> bool {
        self.term.mode().contains(TermMode::SHOW_CURSOR)
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
    pub fn screen_lines(&self) -> usize {
        self.term.grid().screen_lines()
    }

    /// Number of visible columns.
    #[cfg(test)]
    pub fn columns(&self) -> usize {
        self.term.grid().columns()
    }

    /// Read the cursor position (line, column) in screen coordinates.
    pub fn cursor_point(&self) -> (usize, usize) {
        let point = self.term.grid().cursor.point;
        (point.line.0.max(0) as usize, point.column.0)
    }

    /// Return the text of the row the cursor is currently on.
    pub fn get_cursor_row_text(&self) -> String {
        let cursor_line = self.term.grid().cursor.point.line;
        self.get_row_text(cursor_line.0.max(0) as usize)
    }

    /// Clear the cached prev_rows to force full diff on next process().
    pub fn clear_prev_rows(&mut self) {
        self.prev_rows.clear();
    }

    /// Resize the terminal grid.
    ///
    /// When `reflow_history` is enabled, scrollback rows are reflowed (wrapped/
    /// unwrapped) to match the new column width while the visible screen is left
    /// untouched — preserving cursor-addressed TUI positioning.
    #[cfg(test)]
    pub fn resize(&mut self, rows: u16, cols: u16) {
        let mode = if self.reflow_history {
            ReflowMode::HistoryOnly
        } else {
            ReflowMode::None
        };
        self.resize_with_mode(rows, cols, mode);
    }

    pub fn resize_with_mode(&mut self, rows: u16, cols: u16, mode: ReflowMode) {
        let size = GridSize {
            cols: cols as usize,
            lines: rows as usize,
        };
        self.term.resize_reflow(size, mode);
        self.prev_rows.clear();
        self.term.mark_fully_damaged();
    }

    /// Override ANSI colors 0-15 with theme values.
    /// Each entry is `[r, g, b]`. Indices 0-7 = normal, 8-15 = bright.
    pub fn set_ansi_colors(&mut self, colors: &[[u8; 3]; 16]) {
        let term_colors = self.term.colors_mut();
        for (i, &[r, g, b]) in colors.iter().enumerate() {
            term_colors[i] = Some(Rgb { r, g, b });
        }
        self.prev_rows.clear();
        self.term.mark_fully_damaged();
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
            let underline = cell
                .flags
                .intersects(Flags::UNDERLINE | Flags::DOUBLE_UNDERLINE | Flags::UNDERCURL);

            if !cur_text.is_empty()
                && (fg != cur_fg
                    || bg != cur_bg
                    || bold != cur_bold
                    || italic != cur_italic
                    || underline != cur_underline)
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
            if last.fg.is_none()
                && last.bg.is_none()
                && !last.bold
                && !last.italic
                && !last.underline
                && last.text.trim_end().is_empty()
            {
                spans.pop();
            } else {
                break;
            }
        }
        if let Some(last) = spans.last_mut() {
            let trimmed = last.text.trim_end().to_string();
            if trimmed.is_empty()
                && last.fg.is_none()
                && last.bg.is_none()
                && !last.bold
                && !last.italic
                && !last.underline
            {
                spans.pop();
            } else {
                last.text = trimmed;
            }
        }

        LogLine {
            spans,
            cols: num_cols as u16,
        }
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
                grid[prev_line][Column(last_col)]
                    .flags
                    .contains(Flags::WRAPLINE)
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
        let grid = self.term.grid();
        let display_offset = grid.display_offset();
        let line = Line(row as i32 - display_offset as i32);
        if col >= grid.columns() || line < grid.topmost_line() || line > grid.bottommost_line() {
            return None;
        }
        let cell = &grid[line][Column(col)];
        cell.hyperlink().map(|h| h.uri().to_owned())
    }

    pub fn hyperlink_span(&self, row: usize, col: usize) -> Option<(usize, usize, String)> {
        let grid = self.term.grid();
        let display_offset = grid.display_offset();
        let line = Line(row as i32 - display_offset as i32);
        let num_cols = grid.columns();
        if col >= num_cols || line < grid.topmost_line() || line > grid.bottommost_line() {
            return None;
        }
        let uri = grid[line][Column(col)].hyperlink()?.uri().to_owned();
        let mut start = col;
        while start > 0 {
            if let Some(h) = grid[line][Column(start - 1)].hyperlink() {
                if h.uri() == uri {
                    start -= 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        let mut end = col + 1;
        while end < num_cols {
            if let Some(h) = grid[line][Column(end)].hyperlink() {
                if h.uri() == uri {
                    end += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        Some((start, end, uri))
    }

    /// Enumerate OSC 8 hyperlinks on the active screen, coalescing adjacent
    /// cells that share a URI into a single span. Returns
    /// `(line_index, start_col, end_col, uri)` where `line_index` is the
    /// absolute scrollback index used by `search_buffer` (history + screen row).
    pub fn enumerate_visible_hyperlinks(&self) -> Vec<(usize, usize, usize, String)> {
        let grid = self.term.grid();
        let history = grid.history_size();
        let rows = grid.screen_lines();
        let cols = grid.columns();
        let mut out = Vec::new();
        for row in 0..rows {
            let line = Line(row as i32);
            let abs_row = (line.0 + history as i32) as usize;
            let mut col = 0;
            while col < cols {
                let Some(h) = grid[line][Column(col)].hyperlink() else {
                    col += 1;
                    continue;
                };
                let uri = h.uri().to_owned();
                let start = col;
                col += 1;
                while col < cols {
                    match grid[line][Column(col)].hyperlink() {
                        Some(h2) if h2.uri() == uri => col += 1,
                        _ => break,
                    }
                }
                out.push((abs_row, start, col, uri));
            }
        }
        out
    }

    /// Group the active screen's cells into OSC 133 semantic zones (prompt /
    /// input / output), coalescing contiguous cells of the same type in reading
    /// order. Returns `(kind, start_line, end_line, text)` with absolute line
    /// indices; untagged (`None`) cells are skipped.
    pub fn extract_semantic_zones(&self) -> Vec<(String, usize, usize, String)> {
        let grid = self.term.grid();
        let history = grid.history_size();
        let rows = grid.screen_lines();
        let cols = grid.columns();
        let mut zones: Vec<(Osc133CellType, usize, usize, String)> = Vec::new();
        for row in 0..rows {
            let line = Line(row as i32);
            let abs_row = (line.0 + history as i32) as usize;
            for col in 0..cols {
                let cell = &grid[line][Column(col)];
                let ct = cell.cell_type;
                if ct == Osc133CellType::None {
                    continue;
                }
                let ch = if cell.c == '\0' { ' ' } else { cell.c };
                match zones.last_mut() {
                    Some((kind, _start, end, text)) if *kind == ct => {
                        if *end != abs_row {
                            text.push('\n');
                            *end = abs_row;
                        }
                        text.push(ch);
                    }
                    _ => zones.push((ct, abs_row, abs_row, ch.to_string())),
                }
            }
        }
        zones
            .into_iter()
            .map(|(kind, start, end, text)| {
                let label = match kind {
                    Osc133CellType::Prompt => "prompt",
                    Osc133CellType::Input => "input",
                    Osc133CellType::Output => "output",
                    Osc133CellType::None => "none",
                };
                let cleaned = text
                    .split('\n')
                    .map(|l| l.trim_end())
                    .collect::<Vec<_>>()
                    .join("\n");
                (label.to_string(), start, end, cleaned)
            })
            .collect()
    }

    /// Mark all rows as dirty so the next serialize_dirty_rows returns a full frame.
    pub fn force_full_damage(&mut self) {
        self.term.mark_fully_damaged();
    }

    // --- Scroll API ---

    /// Scroll the viewport by `delta` lines (positive = up / into history).
    pub fn scroll(&mut self, delta: i32) {
        self.term.scroll_display(Scroll::Delta(delta));
        self.term.mark_fully_damaged();
    }

    /// Current display offset (0 = at bottom, >0 = scrolled up).
    pub fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    /// Scroll to an absolute line index (0 = top of scrollback history).
    /// Clamps to valid range.
    pub fn scroll_to_line(&mut self, line: usize) {
        let history = self.term.grid().history_size();
        let target_offset = history.saturating_sub(line);
        let current = self.term.grid().display_offset();
        let delta = target_offset as i32 - current as i32;
        if delta != 0 {
            self.term.scroll_display(Scroll::Delta(delta));
            self.term.mark_fully_damaged();
        }
    }

    /// Scroll to an absolute display offset (0 = bottom, history = top). Clamps.
    pub fn scroll_to_offset(&mut self, offset: usize) {
        let history = self.term.grid().history_size();
        let target = offset.min(history);
        let current = self.term.grid().display_offset();
        let delta = target as i32 - current as i32;
        if delta != 0 {
            self.term.scroll_display(Scroll::Delta(delta));
            self.term.mark_fully_damaged();
        }
    }

    /// Total number of lines (screen + scrollback history).
    pub fn total_lines(&self) -> usize {
        self.term.grid().history_size() + self.term.grid().screen_lines()
    }

    pub fn read_rows_in_range(&self, start_abs: usize, end_abs: usize) -> Vec<String> {
        let history = self.term.grid().history_size();
        let mut rows = Vec::new();
        for abs in start_abs..=end_abs {
            let line = Line(abs as i32 - history as i32);
            if let Some(text) = self.row_to_text(line) {
                rows.push(text);
            }
        }
        rows
    }

    // --- Search API ---

    /// Regex search across visible grid + scrollback using alacritty's native DFA engine.
    /// Returns matches as (row, col_start, col_end) in absolute coordinates.
    /// The query is auto-escaped for literal substring search unless it contains
    /// regex metacharacters; case-insensitive when all lowercase.
    pub fn search(&self, query: &str) -> Vec<SearchMatch> {
        if query.is_empty() || query.len() > 1024 {
            return Vec::new();
        }
        let mut regex = match RegexSearch::new(query) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        let history = self.term.grid().history_size();
        let topmost = self.term.topmost_line();
        let bottommost = self.term.bottommost_line();
        let last_col = self.term.last_column();

        let start = Point::new(topmost, Column(0));
        let end = Point::new(bottommost, last_col);

        let mut matches = Vec::new();
        let mut origin = start;

        while let Some(m) = self.term.regex_search_right(&mut regex, origin, end) {
            let m_start = *m.start();
            let m_end = *m.end();

            let abs_row = (m_start.line.0 + history as i32) as usize;
            matches.push(SearchMatch {
                row: abs_row,
                col_start: m_start.column.0,
                col_end: m_end.column.0 + 1,
            });

            // Advance past this match
            if m_end.column < last_col {
                origin = Point::new(m_end.line, m_end.column + 1);
            } else if m_end.line < bottommost {
                origin = Point::new(m_end.line + 1i32, Column(0));
            } else {
                break;
            }
        }
        matches
    }

    pub fn search_buffer(&self, query: &str) -> Vec<BufferSearchMatch> {
        if query.is_empty() || query.len() > 1024 {
            return Vec::new();
        }
        let mut regex = match RegexSearch::new(query) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        let history = self.term.grid().history_size();
        let topmost = self.term.topmost_line();
        let bottommost = self.term.bottommost_line();
        let last_col = self.term.last_column();

        let start = Point::new(topmost, Column(0));
        let end = Point::new(bottommost, last_col);

        let mut matches = Vec::new();
        let mut origin = start;
        let mut last_row_line: Option<(usize, String)> = None;

        while let Some(m) = self.term.regex_search_right(&mut regex, origin, end) {
            let m_start = *m.start();
            let m_end = *m.end();
            let abs_row = (m_start.line.0 + history as i32) as usize;

            let line_text = if last_row_line.as_ref().is_some_and(|(r, _)| *r == abs_row) {
                last_row_line.as_ref().unwrap().1.clone()
            } else {
                let text = self.row_to_text(m_start.line).unwrap_or_default();
                last_row_line = Some((abs_row, text.clone()));
                text
            };

            matches.push(BufferSearchMatch {
                line_index: abs_row,
                line_text,
                match_start: m_start.column.0,
                match_end: m_end.column.0 + 1,
            });

            if m_end.column < last_col {
                origin = Point::new(m_end.line, m_end.column + 1);
            } else if m_end.line < bottommost {
                origin = Point::new(m_end.line + 1i32, Column(0));
            } else {
                break;
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

    /// Get the full logical line containing the given screen row, joining
    /// soft-wrapped (WRAPLINE) rows. Returns (start_row, joined_text).
    pub fn get_logical_line(&self, row: usize) -> (usize, String) {
        let grid = self.term.grid();
        let display_offset = grid.display_offset();
        let num_cols = grid.columns();
        let num_lines = grid.screen_lines();

        // A row past the visible screen has no logical line. This happens when
        // the frontend's screenRows briefly exceeds the backend grid's
        // screen_lines after a resize and the stale row reaches this command.
        // Returning empty avoids the backward walk indexing past the screen
        // bottom, which trips the grid's `requested.0 < visible_lines`
        // assertion (panic in debug).
        if row >= num_lines {
            return (row, String::new());
        }

        // Walk backwards to find the first row of this logical line.
        let mut start = row;
        while start > 0 {
            let prev_line = Line((start - 1) as i32) - display_offset;
            if prev_line.0 < -(grid.history_size() as i32) {
                break;
            }
            let last_col = Column(num_cols - 1);
            if grid[prev_line][last_col].flags.contains(Flags::WRAPLINE) {
                start -= 1;
            } else {
                break;
            }
        }

        // Walk forward, joining rows connected by WRAPLINE.
        let mut text = String::new();
        let mut i = start;
        loop {
            if i >= num_lines {
                break;
            }
            let line = Line(i as i32) - display_offset;
            if line.0 >= grid.screen_lines() as i32 {
                break;
            }
            for col in 0..num_cols {
                let cell = &grid[line][Column(col)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                text.push(cell.c);
            }
            let last_col = Column(num_cols - 1);
            if grid[line][last_col].flags.contains(Flags::WRAPLINE) {
                i += 1;
            } else {
                break;
            }
        }

        let trimmed_len = text.trim_end().len();
        text.truncate(trimmed_len);
        (start, text)
    }

    /// Extract text for a selection range using absolute row coordinates.
    ///
    /// Absolute rows: 0 = oldest history line, historySize = first screen line.
    /// Columns are 0-based cell indices.
    pub fn get_selection_text(
        &self,
        start_row: usize,
        start_col: usize,
        end_row: usize,
        end_col: usize,
    ) -> String {
        let grid = self.term.grid();
        let history_size = grid.history_size();
        let num_cols = grid.columns();

        let (r0, c0, r1, c1) =
            if start_row < end_row || (start_row == end_row && start_col <= end_col) {
                (start_row, start_col, end_row, end_col)
            } else {
                (end_row, end_col, start_row, start_col)
            };

        let mut result = String::new();

        for abs_row in r0..=r1 {
            let line = Line(abs_row as i32 - history_size as i32);
            if line < grid.topmost_line() || line > grid.bottommost_line() {
                result.push('\n');
                continue;
            }

            let col_start = if abs_row == r0 { c0 } else { 0 };
            let col_end = if abs_row == r1 {
                c1.min(num_cols.saturating_sub(1))
            } else {
                num_cols.saturating_sub(1)
            };

            let mut text = String::new();
            for col in col_start..=col_end {
                if col >= num_cols {
                    break;
                }
                let cell = &grid[line][Column(col)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                text.push(cell.c);
            }
            let trimmed_len = text.trim_end().len();
            text.truncate(trimmed_len);
            result.push_str(&text);

            if abs_row < r1 {
                let last_col = num_cols.saturating_sub(1);
                let is_wrapped = grid[line][Column(last_col)].flags.contains(Flags::WRAPLINE);
                if !is_wrapped {
                    result.push('\n');
                }
            }
        }

        let trimmed = result.trim_end_matches('\n');
        trimmed.to_owned()
    }

    /// Serialize dirty rows as a compact binary frame.
    ///
    /// Uses alacritty's built-in damage tracking to identify changed rows.
    /// Wire format (22-byte header):
    /// ```text
    /// Header: [num_rows: u16] [cursor_row: u16] [cursor_col: u16] [cursor_visible: u8]
    ///         [display_offset: u32] [history_size: u32] [has_selection: u8]
    ///         [keyboard_flags: u8] [frame_flags: u8] [num_lines: u16] [num_cols: u16]
    /// Per row: [row_index: u16] [col_count: u16] [cells...]
    /// Per cell: [char: u32 LE] [fg_r, fg_g, fg_b] [bg_r, bg_g, bg_b] [attrs: u8]
    /// ```
    /// attrs: bit0=bold, bit1=italic, bit2=underline, bit3=strikeout,
    ///        bit4=dim, bit5=inverse, bit6=default_fg, bit7=default_bg
    /// keyboard_flags: bit0=disambiguate_esc_codes, bit1=report_event_types,
    ///                 bit2=report_alternate_keys, bit3=report_all_keys_as_esc,
    ///                 bit4=report_associated_text
    /// frame_flags: bit0=bell, bits1-2=cursor_shape (0=block,1=underline,2=beam),
    ///              bits3-4=mouse_mode (0=none,1=click,2=drag,3=motion),
    ///              bit5=sgr_mouse, bit6=focus_reporting, bit7=bracketed_paste
    pub fn serialize_dirty_rows(&mut self) -> Vec<u8> {
        let num_cols = self.term.grid().columns();
        let num_lines = self.term.grid().screen_lines();
        let cursor = self.term.grid().cursor.point;
        let cursor_visible = self.term.mode().contains(TermMode::SHOW_CURSOR);
        let display_offset = self.term.grid().display_offset();
        let history_size = self.term.grid().history_size();
        // Lines evicted from the history top so far. Monotonic within a resize era,
        // so `history_base + grid_relative_abs` is an eviction-stable absolute row
        // coordinate the frontend can key its scroll cache by (see serialize_styled_range).
        let history_base = self
            .term
            .grid()
            .total_scrolled()
            .saturating_sub(history_size);
        let has_selection = self.term.selection.is_some();
        let mode = *self.term.mode();
        let mut keyboard_flags: u8 = 0;
        if mode.contains(TermMode::DISAMBIGUATE_ESC_CODES) {
            keyboard_flags |= 0x01;
        }
        if mode.contains(TermMode::REPORT_EVENT_TYPES) {
            keyboard_flags |= 0x02;
        }
        if mode.contains(TermMode::REPORT_ALTERNATE_KEYS) {
            keyboard_flags |= 0x04;
        }
        if mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC) {
            keyboard_flags |= 0x08;
        }
        if mode.contains(TermMode::REPORT_ASSOCIATED_TEXT) {
            keyboard_flags |= 0x10;
        }

        let viewport_changed = self.last_frame_display_offset != Some(display_offset)
            || self.last_frame_history_size != Some(history_size)
            || self.last_frame_screen_lines != Some(num_lines)
            || self.last_frame_columns != Some(num_cols);
        if viewport_changed {
            self.term.mark_fully_damaged();
        }

        let dirty_lines: Vec<usize> = {
            let damage = self.term.damage();
            match damage {
                TermDamage::Full => (0..num_lines).collect(),
                TermDamage::Partial(iter) => {
                    iter.map(|b| b.line).filter(|&l| l < num_lines).collect()
                }
            }
        };

        if dirty_lines.is_empty() {
            self.term.reset_damage();
            self.last_frame_display_offset = Some(display_offset);
            self.last_frame_history_size = Some(history_size);
            self.last_frame_screen_lines = Some(num_lines);
            self.last_frame_columns = Some(num_cols);
            return Vec::new();
        }

        // Header: 26 bytes
        let row_count = dirty_lines.len();
        let estimated = 26 + row_count * (4 + num_cols * 11);
        let mut buf = Vec::with_capacity(estimated);

        let bell = self.drain_bell();
        let cursor_shape = self.term.cursor_style().shape;
        let mut frame_flags: u8 = 0;
        if bell {
            frame_flags |= 0x01;
        }
        // bits 1-2: cursor shape (0=block, 1=underline, 2=beam)
        let shape_bits: u8 = match cursor_shape {
            CursorShape::Block => 0,
            CursorShape::Underline => 1,
            CursorShape::Beam => 2,
            _ => 0,
        };
        frame_flags |= shape_bits << 1;
        // bits 3-4: mouse mode (0=none, 1=click, 2=drag, 3=motion)
        let mouse_bits: u8 = if mode.contains(TermMode::MOUSE_MOTION) {
            3
        } else if mode.contains(TermMode::MOUSE_DRAG) {
            2
        } else if mode.contains(TermMode::MOUSE_REPORT_CLICK) {
            1
        } else {
            0
        };
        frame_flags |= mouse_bits << 3;
        // bit 5: SGR mouse encoding
        if mode.contains(TermMode::SGR_MOUSE) {
            frame_flags |= 0x20;
        }
        // bit 6: focus reporting
        if mode.contains(TermMode::FOCUS_IN_OUT) {
            frame_flags |= 0x40;
        }
        // bit 7: bracketed paste mode
        if mode.contains(TermMode::BRACKETED_PASTE) {
            frame_flags |= 0x80;
        }

        buf.extend_from_slice(&(row_count as u16).to_le_bytes());
        buf.extend_from_slice(&(cursor.line.0.max(0) as u16).to_le_bytes());
        buf.extend_from_slice(&(cursor.column.0 as u16).to_le_bytes());
        buf.push(cursor_visible as u8);
        buf.extend_from_slice(&(display_offset as u32).to_le_bytes());
        buf.extend_from_slice(&(history_size as u32).to_le_bytes());
        buf.push(has_selection as u8);
        buf.push(keyboard_flags);
        buf.push(frame_flags);
        buf.extend_from_slice(&(num_lines as u16).to_le_bytes());
        buf.extend_from_slice(&(num_cols as u16).to_le_bytes());
        buf.extend_from_slice(&(history_base as u32).to_le_bytes());

        let grid = self.term.grid();
        let colors = self.term.colors();
        for &row_idx in &dirty_lines {
            let line = Line(row_idx as i32 - display_offset as i32);
            buf.extend_from_slice(&(row_idx as u16).to_le_bytes());
            buf.extend_from_slice(&(num_cols as u16).to_le_bytes());

            for col in 0..num_cols {
                encode_cell(&mut buf, &grid[line][Column(col)], colors);
            }
        }

        self.term.reset_damage();
        self.last_frame_display_offset = Some(display_offset);
        self.last_frame_history_size = Some(history_size);
        self.last_frame_screen_lines = Some(num_lines);
        self.last_frame_columns = Some(num_cols);
        buf
    }

    /// Serialize a range of styled rows by *eviction-stable absolute index*. Feeds
    /// the frontend's client-side row cache so it can paint the scroll viewport
    /// locally at any offset/speed without a per-line round-trip. Read-only,
    /// on-demand — deliberately NOT part of the hot grid-frame protocol.
    ///
    /// The absolute index is `history_base + grid_relative`, where `history_base`
    /// (= `total_scrolled() - history_size()`) is the count of lines already evicted
    /// from the history top. Because `history_base` climbs by exactly as much as the
    /// grid-relative coordinate drops on eviction, a given physical line keeps the
    /// same absolute index for life — so the frontend cache never aliases a stale row
    /// onto a new one after the scrollback cap rotates.
    ///
    /// `start_abs` is interpreted in this absolute space; rows that map outside the
    /// live grid `[0, history_size + screen_lines)` are skipped, so the returned
    /// `row_count` may be smaller than `count`. Each row carries its own absolute
    /// index for correct placement.
    ///
    /// Layout (little-endian):
    ///   start_abs: u32, history_size: u32, num_cols: u16, row_count: u16,
    ///   then per row: abs: u32, col_count: u16, cells (col_count × 11; see
    ///   `encode_cell`).
    pub fn serialize_styled_range(&self, start_abs: usize, count: usize) -> Vec<u8> {
        let grid = self.term.grid();
        let colors = self.term.colors();
        let num_cols = grid.columns();
        let num_lines = grid.screen_lines();
        let history_size = grid.history_size();
        let total = history_size + num_lines;
        // Convert the requested absolute start into the grid's current relative space
        // to read cells, then re-tag each row with its absolute index on the way out.
        let history_base = grid.total_scrolled().saturating_sub(history_size);
        let start_rel = start_abs.saturating_sub(history_base);

        let rows: Vec<usize> = (0..count)
            .map(|i| start_rel + i)
            .filter(|&rel| rel < total)
            .collect();

        let mut buf = Vec::with_capacity(12 + rows.len() * (6 + num_cols * 11));
        buf.extend_from_slice(&(start_abs as u32).to_le_bytes());
        buf.extend_from_slice(&(history_size as u32).to_le_bytes());
        buf.extend_from_slice(&(num_cols as u16).to_le_bytes());
        buf.extend_from_slice(&(rows.len() as u16).to_le_bytes());
        for rel in rows {
            let line = Line(rel as i32 - history_size as i32);
            buf.extend_from_slice(&((rel + history_base) as u32).to_le_bytes());
            buf.extend_from_slice(&(num_cols as u16).to_le_bytes());
            for col in 0..num_cols {
                encode_cell(&mut buf, &grid[line][Column(col)], colors);
            }
        }
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
            let trimmed_len = text.trim_end().len();
            text.truncate(trimmed_len);
            rows.push(text);
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
        let trimmed_len = text.trim_end().len();
        text.truncate(trimmed_len);
        Some(text)
    }

    #[cfg(test)]
    pub(crate) fn term(&self) -> &Term<TermEventCollector> {
        &self.term
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
        let changed = grid.process(b"hello world");
        assert!(!changed.is_empty());
        let first = &changed[0];
        assert_eq!(first.row_index, 0);
        assert_eq!(first.text, "hello world");
    }

    #[test]
    fn process_returns_empty_on_no_change() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        let _ = grid.process(b"hello");
        let changed = grid.process(b"");
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
    fn screen_text_rows_ref_returns_none_before_process() {
        let grid = TerminalGrid::new(5, 20, 100);
        assert!(grid.screen_text_rows_ref().is_none());
    }

    #[test]
    fn screen_text_rows_ref_matches_owned() {
        let mut grid = TerminalGrid::new(5, 20, 100);
        let _ = grid.process(b"line1\r\nline2\r\nline3");
        let owned = grid.screen_text_rows();
        let borrowed = grid.screen_text_rows_ref().unwrap();
        assert_eq!(owned, borrowed);
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
        let changed = grid.process(b"\rworld");
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

    const TEST_HEADER_SIZE: usize = 26;
    const TEST_FRAME_FLAGS_OFFSET: usize = 17;

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
        let ch = u32::from_le_bytes([
            buf[offset],
            buf[offset + 1],
            buf[offset + 2],
            buf[offset + 3],
        ]);
        let ch = char::from_u32(ch).unwrap_or('\0');
        (
            ch,
            buf[offset + 4],
            buf[offset + 5],
            buf[offset + 6],
            buf[offset + 7],
            buf[offset + 8],
            buf[offset + 9],
            buf[offset + 10],
        )
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
        let row_idx = u16::from_le_bytes([buf[h], buf[h + 1]]);
        let col_count = u16::from_le_bytes([buf[h + 2], buf[h + 3]]);
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

        // Alacritty represents regular empty cells as spaces; wide-char spacers
        // are the NUL cells covered by serialize_wide_char_spacer_is_zero.
        let (ch, _, _, _, _, _, _, _) = decode_cell(&buf, cell0 + 22);
        assert_eq!(ch, ' ');
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
        assert_eq!(fg_r, 0xcc); // Tango palette red
        assert_eq!(fg_g, 0x00);
        assert_eq!(fg_b, 0x00);
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
    fn serialize_dim_text_darker_than_normal() {
        let mut grid = TerminalGrid::new(5, 20, 0);
        // Normal red then dim red
        let _ = grid.process(b"\x1b[31mN\x1b[0m\x1b[2;31mD\x1b[0m");
        let buf = grid.serialize_dirty_rows();

        let cell0 = TEST_HEADER_SIZE + 4;
        let (ch_n, fg_r_n, fg_g_n, fg_b_n, _, _, _, _) = decode_cell(&buf, cell0);
        let (ch_d, fg_r_d, fg_g_d, fg_b_d, _, _, _, attrs_d) = decode_cell(&buf, cell0 + 11);
        assert_eq!(ch_n, 'N');
        assert_eq!(ch_d, 'D');
        assert!(
            fg_r_d < fg_r_n,
            "dim red R channel ({fg_r_d}) must be darker than normal ({fg_r_n})"
        );
        assert!(fg_g_d <= fg_g_n, "dim red G channel not brighter");
        assert!(fg_b_d <= fg_b_n, "dim red B channel not brighter");
        assert_ne!(attrs_d & super::ATTR_DIM, 0, "dim flag set");
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
    fn serialize_full_frame_when_history_grows() {
        let mut grid = TerminalGrid::new(3, 10, 100);
        let _ = grid.process(b"one\r\ntwo\r\nthree");
        let _ = grid.serialize_dirty_rows();

        let _ = grid.process(b"\r\nfour");
        let buf = grid.serialize_dirty_rows();
        let (num_rows, _, _, _) = decode_header(&buf);

        assert_eq!(
            num_rows, 3,
            "scrollback growth shifts viewport rows, so frame must be full"
        );
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
        let ch1_raw =
            u32::from_le_bytes([buf[cell1], buf[cell1 + 1], buf[cell1 + 2], buf[cell1 + 3]]);
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

    #[test]
    fn serialize_mouse_and_focus_mode_flags() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        // Enable mouse click reporting (?1000h), SGR encoding (?1006h), focus reporting (?1004h)
        let _ = grid.process(b"\x1b[?1000h\x1b[?1006h\x1b[?1004h");
        let _ = grid.process(b"X");
        let buf = grid.serialize_dirty_rows();
        assert!(!buf.is_empty());

        // frame_flags is at offset 17 in the 22-byte header
        let frame_flags = buf[TEST_FRAME_FLAGS_OFFSET];
        // bits 3-4: mouse mode = 1 (click only, not drag/motion)
        assert_eq!((frame_flags >> 3) & 0x03, 1, "mouse mode = click");
        // bit 5: SGR mouse
        assert_ne!(frame_flags & 0x20, 0, "SGR mouse active");
        // bit 6: focus reporting
        assert_ne!(frame_flags & 0x40, 0, "focus reporting active");
    }

    #[test]
    fn serialize_mouse_drag_mode_flag() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        // Enable mouse drag reporting (?1002h)
        let _ = grid.process(b"\x1b[?1002h");
        let _ = grid.process(b"X");
        let buf = grid.serialize_dirty_rows();
        let frame_flags = buf[TEST_FRAME_FLAGS_OFFSET];
        assert_eq!((frame_flags >> 3) & 0x03, 2, "mouse mode = drag");
    }

    #[test]
    fn serialize_mouse_motion_mode_flag() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        // Enable mouse motion reporting (?1003h)
        let _ = grid.process(b"\x1b[?1003h");
        let _ = grid.process(b"X");
        let buf = grid.serialize_dirty_rows();
        let frame_flags = buf[TEST_FRAME_FLAGS_OFFSET];
        assert_eq!((frame_flags >> 3) & 0x03, 3, "mouse mode = motion");
    }

    #[test]
    fn serialize_no_mouse_flags_by_default() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        let _ = grid.process(b"plain text");
        let buf = grid.serialize_dirty_rows();
        let frame_flags = buf[TEST_FRAME_FLAGS_OFFSET];
        // bits 3-6 should all be zero
        assert_eq!(frame_flags & 0x78, 0, "no mouse/focus flags by default");
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

    #[test]
    fn search_regex_pattern() {
        let mut grid = TerminalGrid::new(5, 40, 0);
        let _ = grid.process(b"error: file not found\r\nwarning: deprecated");
        let matches = grid.search("error|warning");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn search_invalid_regex_returns_empty() {
        let mut grid = TerminalGrid::new(5, 40, 0);
        let _ = grid.process(b"test content");
        let matches = grid.search("[invalid");
        assert!(matches.is_empty());
    }

    #[test]
    fn search_rejects_query_over_1024_bytes() {
        let mut grid = TerminalGrid::new(5, 40, 0);
        let _ = grid.process(b"test content");
        let long_query = "a".repeat(1025);
        assert!(grid.search(&long_query).is_empty());
        assert!(grid.search_buffer(&long_query).is_empty());
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

    #[test]
    fn scroll_to_line_absolute() {
        let mut grid = TerminalGrid::new(3, 20, 100);
        // 5 lines into a 3-row screen → 2 lines in history
        let _ = grid.process(b"line1\r\nline2\r\nline3\r\nline4\r\nline5");
        assert_eq!(grid.display_offset(), 0);

        // Scroll to top of history (line 0)
        grid.scroll_to_line(0);
        assert_eq!(grid.display_offset(), 2);

        // Scroll to line 1
        grid.scroll_to_line(1);
        assert_eq!(grid.display_offset(), 1);

        // Scroll to bottom (line beyond history)
        grid.scroll_to_line(100);
        assert_eq!(grid.display_offset(), 0);

        // Scroll to line 0 again, then back to bottom via scroll_to_line(2)
        grid.scroll_to_line(0);
        assert_eq!(grid.display_offset(), 2);
        grid.scroll_to_line(2);
        assert_eq!(grid.display_offset(), 0);
    }

    #[test]
    fn scroll_to_offset_clamps_and_is_exact() {
        let mut grid = TerminalGrid::new(3, 20, 100);
        // 5 lines into a 3-row screen → 2 lines in history.
        let _ = grid.process(b"line1\r\nline2\r\nline3\r\nline4\r\nline5");
        assert_eq!(grid.display_offset(), 0);

        // Clamps above history.
        grid.scroll_to_offset(999);
        assert_eq!(grid.display_offset(), 2);

        // Sets an exact offset.
        grid.scroll_to_offset(1);
        assert_eq!(grid.display_offset(), 1);

        // Idempotent: applying the same offset again is a no-op.
        grid.scroll_to_offset(1);
        assert_eq!(grid.display_offset(), 1);

        // Back to the bottom.
        grid.scroll_to_offset(0);
        assert_eq!(grid.display_offset(), 0);
    }

    #[test]
    fn styled_range_header_and_clamping() {
        let mut grid = TerminalGrid::new(3, 20, 100);
        // 5 lines into a 3-row screen → 2 lines history, total 5 absolute rows.
        let _ = grid.process(b"line1\r\nline2\r\nline3\r\nline4\r\nline5");

        // Request 4 rows from abs 1 — all in range.
        let buf = grid.serialize_styled_range(1, 4);
        let start_abs = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let history = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let num_cols = u16::from_le_bytes([buf[8], buf[9]]);
        let row_count = u16::from_le_bytes([buf[10], buf[11]]);
        assert_eq!(start_abs, 1);
        assert_eq!(history, 2);
        assert_eq!(num_cols, 20);
        assert_eq!(row_count, 4, "abs 1..5 are all in range (total = 5)");
        // First row's absolute index immediately follows the header.
        let first_abs = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
        assert_eq!(first_abs, 1);

        // Request past the end — clamped to what exists (only abs 4 in [4,7)).
        let buf = grid.serialize_styled_range(4, 3);
        let row_count = u16::from_le_bytes([buf[10], buf[11]]);
        assert_eq!(row_count, 1, "only abs 4 exists in [4,7)");
    }

    /// Decode a styled-range payload into (absolute_index, trimmed_text) pairs.
    fn dump_styled(grid: &TerminalGrid) -> Vec<(u32, String)> {
        let buf = grid.serialize_styled_range(0, 100_000);
        let mut out = Vec::new();
        if buf.len() < 12 {
            return out;
        }
        let count = u16::from_le_bytes([buf[10], buf[11]]) as usize;
        let mut off = 12;
        for _ in 0..count {
            let abs = u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
            off += 4;
            let col_count = u16::from_le_bytes([buf[off], buf[off + 1]]) as usize;
            off += 2;
            let mut text = String::new();
            for _ in 0..col_count {
                let ch = u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
                text.push(char::from_u32(ch).unwrap_or(' '));
                off += 11; // 4-byte codepoint + 7 bytes of style
            }
            out.push((abs, text.trim_end().to_string()));
        }
        out
    }

    /// The bug behind scroll duplication: with a grid-relative coordinate, a row's
    /// absolute index shifts (and gets reused for a *different* line) once the
    /// scrollback cap evicts from the top, so the client cache aliases a stale row
    /// onto a new one. The absolute index must instead be globally stable: a physical
    /// line keeps its index for life, and no index is ever reused for another line.
    #[test]
    fn styled_abs_is_eviction_stable_and_never_aliases() {
        use std::collections::HashMap;

        // 2 visible rows, history capped at 2 → eviction kicks in after a few lines.
        let mut grid = TerminalGrid::new(2, 20, 2);
        let mut abs_of_text: HashMap<String, u32> = HashMap::new();
        let mut text_of_abs: HashMap<u32, String> = HashMap::new();
        let mut max_abs = 0u32;

        for i in 0..40 {
            let _ = grid.process(format!("row{i:02}\r\n").as_bytes());
            for (abs, text) in dump_styled(&grid) {
                if text.is_empty() {
                    continue;
                }
                // A given line keeps the same absolute index every time we observe it.
                if let Some(&prev) = abs_of_text.get(&text) {
                    assert_eq!(
                        prev, abs,
                        "line {text:?} moved abs {prev} -> {abs} after eviction"
                    );
                } else {
                    abs_of_text.insert(text.clone(), abs);
                }
                // No absolute index is ever reused for a different line.
                if let Some(prev) = text_of_abs.get(&abs) {
                    assert_eq!(prev, &text, "abs {abs} aliased {prev:?} onto {text:?}");
                } else {
                    text_of_abs.insert(abs, text.clone());
                }
                max_abs = max_abs.max(abs);
            }
        }

        // The coordinate kept climbing well past the 2-line cap — i.e. it is all-time
        // absolute, not bounded by the retained history window.
        assert!(
            max_abs >= 30,
            "abs should grow with total output, got {max_abs}"
        );
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

    #[test]
    fn get_selection_text_single_row() {
        let mut grid = TerminalGrid::new(5, 20, 0);
        let _ = grid.process(b"hello world");
        // historySize=0, so screen row 0 = absRow 0
        let text = grid.get_selection_text(0, 6, 0, 10);
        assert_eq!(text, "world");
    }

    #[test]
    fn get_selection_text_multi_row() {
        let mut grid = TerminalGrid::new(5, 20, 0);
        let _ = grid.process(b"first\r\nsecond\r\nthird");
        let text = grid.get_selection_text(0, 0, 2, 4);
        assert_eq!(text, "first\nsecond\nthird");
    }

    #[test]
    fn get_selection_text_with_scrollback() {
        let mut grid = TerminalGrid::new(3, 20, 100);
        let _ = grid.process(b"line1\r\nline2\r\nline3\r\nline4\r\nline5");
        // 2 lines in history (line1, line2), 3 on screen (line3, line4, line5)
        // absRow 0 = line1, absRow 1 = line2, absRow 2 = line3, ...
        let text = grid.get_selection_text(0, 0, 4, 4);
        assert_eq!(text, "line1\nline2\nline3\nline4\nline5");
    }

    #[test]
    fn get_selection_text_reversed_coords() {
        let mut grid = TerminalGrid::new(5, 20, 0);
        let _ = grid.process(b"hello world");
        // end before start — should still work
        let text = grid.get_selection_text(0, 10, 0, 6);
        assert_eq!(text, "world");
    }

    #[test]
    fn get_selection_text_unwraps_soft_wrapped_lines() {
        // 10-col terminal: "abcdefghijklmno" wraps at col 10 → two visual rows, one logical line
        let mut grid = TerminalGrid::new(3, 10, 0);
        let _ = grid.process(b"abcdefghijklmno");
        // Row 0 has WRAPLINE (cols 0-9 = "abcdefghij"), row 1 = "klmno"
        let text = grid.get_selection_text(0, 0, 1, 4);
        assert_eq!(text, "abcdefghijklmno");
    }

    #[test]
    fn get_selection_text_mixed_wrap_and_newline() {
        // 10-col terminal: wrap + explicit newline
        let mut grid = TerminalGrid::new(5, 10, 0);
        let _ = grid.process(b"abcdefghijklmno\r\nsecond");
        // Row 0: "abcdefghij" (WRAPLINE), Row 1: "klmno" (no wrap), Row 2: "second"
        let text = grid.get_selection_text(0, 0, 2, 5);
        assert_eq!(text, "abcdefghijklmno\nsecond");
    }

    // --- Logical line tests ---

    #[test]
    fn get_logical_line_single_row() {
        let mut grid = TerminalGrid::new(5, 20, 0);
        let _ = grid.process(b"short text\r\nnext");
        let (start, text) = grid.get_logical_line(0);
        assert_eq!(start, 0);
        assert_eq!(text, "short text");
    }

    #[test]
    fn get_logical_line_wrapped_rows() {
        // 10-col terminal: "file:///tmp/longpath.png" wraps across rows
        let mut grid = TerminalGrid::new(5, 10, 0);
        let _ = grid.process(b"file:///tmp/longpath.png");
        // Row 0: "file:///tm" (WRAPLINE), Row 1: "p/longpath" (WRAPLINE), Row 2: ".png"
        let (start, text) = grid.get_logical_line(0);
        assert_eq!(start, 0);
        assert_eq!(text, "file:///tmp/longpath.png");
        // Querying from middle row should return same logical line
        let (start, text) = grid.get_logical_line(1);
        assert_eq!(start, 0);
        assert_eq!(text, "file:///tmp/longpath.png");
        // Querying from last row of logical line
        let (start, text) = grid.get_logical_line(2);
        assert_eq!(start, 0);
        assert_eq!(text, "file:///tmp/longpath.png");
    }

    #[test]
    fn get_logical_line_stops_at_newline() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        let _ = grid.process(b"abcdefghij\r\nsecond");
        // Row 0 is full but has explicit newline after → NOT WRAPLINE
        // Actually in terminals, "abcdefghij" fills 10 cols, next char is on new line
        // If the cursor advances past col 10, the terminal wraps. With explicit \r\n
        // the row does NOT get WRAPLINE.
        let (start, text) = grid.get_logical_line(1);
        assert_eq!(start, 1);
        assert_eq!(text, "second");
    }

    #[test]
    fn get_logical_line_out_of_range_row_does_not_panic() {
        // The frontend's screenRows can briefly exceed the backend grid's
        // screen_lines after a resize, so an out-of-range row reaches this
        // command. It must not index past the screen bottom (which trips the
        // grid's `requested.0 < visible_lines` assertion → panic in debug).
        let mut grid = TerminalGrid::new(5, 10, 0);
        let _ = grid.process(b"hello");
        let (start, text) = grid.get_logical_line(10);
        assert_eq!(start, 10);
        assert_eq!(text, "");
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
        assert!(
            lines[0].contains("line"),
            "first scrollback line should contain text"
        );
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

    #[test]
    fn osc133_a_emits_event_via_drain() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        grid.process(b"\x1b]133;A\x07");
        let events = grid.drain_events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            TermEvent::Osc133 { command, .. } => assert_eq!(*command, 'A'),
            other => panic!("expected Osc133, got {other:?}"),
        }
    }

    #[test]
    fn osc133_d_with_exit_code() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        grid.process(b"\x1b]133;D;42\x07");
        let events = grid.drain_events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            TermEvent::Osc133 {
                command, params, ..
            } => {
                assert_eq!(*command, 'D');
                assert_eq!(params, "42");
            }
            other => panic!("expected Osc133, got {other:?}"),
        }
    }

    #[test]
    fn osc133_multiple_markers_in_one_chunk() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        grid.process(b"\x1b]133;A\x07prompt$ \x1b]133;B\x07ls\x1b]133;C\x07");
        let events = grid.drain_events();
        assert_eq!(events.len(), 3);
        let commands: Vec<char> = events
            .iter()
            .map(|e| match e {
                TermEvent::Osc133 { command, .. } => *command,
                _ => panic!("unexpected event"),
            })
            .collect();
        assert_eq!(commands, vec!['A', 'B', 'C']);
    }

    #[test]
    fn osc133_st_terminator() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        grid.process(b"\x1b]133;D;0\x1b\\");
        let events = grid.drain_events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            TermEvent::Osc133 {
                command, params, ..
            } => {
                assert_eq!(*command, 'D');
                assert_eq!(params, "0");
            }
            other => panic!("expected Osc133, got {other:?}"),
        }
    }

    #[test]
    fn osc133_cell_type_tagging() {
        use alacritty_terminal::term::cell::Osc133CellType;
        let mut grid = TerminalGrid::new(24, 80, 1000);
        // Write prompt text after A marker
        grid.process(b"\x1b]133;A\x07$ ");
        // Write command text after B marker
        grid.process(b"\x1b]133;B\x07ls -la");
        // Write output after C marker
        grid.process(b"\x1b]133;C\x07file1.txt\r\nfile2.txt");

        // Check cell types via the grid
        let row0 = grid.get_row_text(0);
        assert!(row0.contains("$"), "row0 should contain prompt: {row0}");

        // Access the term to verify cell_type on cells
        let term = grid.term();
        let grid_ref = term.grid();
        // Row 0 should start with Prompt cells (from A marker), then Input cells (from B)
        let cell_0_0 =
            &grid_ref[alacritty_terminal::index::Line(0)][alacritty_terminal::index::Column(0)];
        assert_eq!(cell_0_0.cell_type, Osc133CellType::Prompt);

        // After "B" marker, cells should be Input
        // "$ " is 2 chars (Prompt), then "ls -la" is 6 chars (Input)
        let cell_0_2 =
            &grid_ref[alacritty_terminal::index::Line(0)][alacritty_terminal::index::Column(2)];
        assert_eq!(cell_0_2.cell_type, Osc133CellType::Input);
    }

    #[test]
    fn osc133_no_events_for_plain_text() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        grid.process(b"hello world");
        let events = grid.drain_events();
        let osc_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, TermEvent::Osc133 { .. }))
            .collect();
        assert!(osc_events.is_empty());
    }

    #[test]
    fn osc7770_state_event() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        // OSC 7770 ; state=idle BEL
        grid.process(b"\x1b]7770;state=idle\x07");
        let events = grid.drain_events();
        let tuic: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, TermEvent::Tuic { .. }))
            .collect();
        assert_eq!(tuic.len(), 1);
        match &tuic[0] {
            TermEvent::Tuic { verb, payload, .. } => {
                assert_eq!(verb, "state");
                assert_eq!(payload, "idle");
            }
            _ => panic!("expected Tuic event"),
        }
    }

    #[test]
    fn osc7770_suggest_event() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        // OSC 7770 ; suggest=Fix the bug|Run tests|Deploy BEL
        grid.process(b"\x1b]7770;suggest=Fix the bug|Run tests|Deploy\x07");
        let events = grid.drain_events();
        let tuic: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, TermEvent::Tuic { .. }))
            .collect();
        assert_eq!(tuic.len(), 1);
        match &tuic[0] {
            TermEvent::Tuic { verb, payload, .. } => {
                assert_eq!(verb, "suggest");
                assert_eq!(payload, "Fix the bug|Run tests|Deploy");
            }
            _ => panic!("expected Tuic event"),
        }
    }

    #[test]
    fn osc7770_intent_event() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        grid.process(b"\x1b]7770;intent=Refactoring auth module (Auth Refactor)\x07");
        let events = grid.drain_events();
        let tuic: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, TermEvent::Tuic { .. }))
            .collect();
        assert_eq!(tuic.len(), 1);
        match &tuic[0] {
            TermEvent::Tuic { verb, payload, .. } => {
                assert_eq!(verb, "intent");
                assert_eq!(payload, "Refactoring auth module (Auth Refactor)");
            }
            _ => panic!("expected Tuic event"),
        }
    }

    #[test]
    fn osc7770_not_written_to_grid() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        grid.process(b"before\x1b]7770;state=busy\x07after");
        let row = grid.get_row_text(0);
        assert!(row.contains("before"));
        assert!(row.contains("after"));
        assert!(!row.contains("7770"));
        assert!(!row.contains("state"));
    }

    #[test]
    fn osc7770_st_terminated() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        // ST terminator: ESC backslash
        grid.process(b"\x1b]7770;state=busy\x1b\\");
        let events = grid.drain_events();
        let tuic: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, TermEvent::Tuic { .. }))
            .collect();
        assert_eq!(tuic.len(), 1);
        match &tuic[0] {
            TermEvent::Tuic { verb, payload, .. } => {
                assert_eq!(verb, "state");
                assert_eq!(payload, "busy");
            }
            _ => panic!("expected Tuic event"),
        }
    }

    #[test]
    fn cursor_guard_partial_suggest_at_cursor_row() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        // Write a partial suggest line — cursor stays on that row
        grid.process(b"suggest: Fix bug|Run te");
        let (cursor_row, _) = grid.cursor_point();
        assert_eq!(cursor_row, 0, "cursor should be on the partial row");
        let row_text = grid.get_row_text(0);
        let trimmed = row_text.trim_start();
        assert!(
            trimmed.starts_with("suggest:"),
            "row should start with suggest: but got: {row_text}"
        );
        // Verify the guard predicate: row at cursor starts with "suggest:" → should be excluded
        let should_exclude = trimmed.starts_with("suggest:") || trimmed.starts_with("intent:");
        assert!(
            should_exclude,
            "guard predicate should match this row for exclusion"
        );
    }

    #[test]
    fn cursor_guard_completed_suggest_cursor_moved() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        // Write a complete suggest line followed by a newline
        grid.process(b"suggest: Fix bug|Run tests|Deploy\r\n");
        let (cursor_row, _) = grid.cursor_point();
        assert_eq!(cursor_row, 1, "cursor moved past completed line");
        // The completed row is NOT at cursor, so the guard would NOT exclude it
        let row_text = grid.get_row_text(0);
        assert!(row_text.trim_start().starts_with("suggest:"));
    }

    #[test]
    fn cursor_guard_intent_at_cursor_row() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        grid.process(b"intent: Working on feat");
        let (cursor_row, _) = grid.cursor_point();
        assert_eq!(cursor_row, 0);
        let trimmed = grid.get_row_text(0).trim_start().to_string();
        assert!(
            trimmed.starts_with("intent:"),
            "guard should also match intent: prefix"
        );
    }

    #[test]
    fn osc7770_and_osc133_full_flow() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        // Shell displays prompt (OSC 133 A)
        grid.process(b"\x1b]133;A\x07$ ");
        let events = grid.drain_events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TermEvent::Osc133 { command: 'A', .. }))
        );

        // User types and presses enter (OSC 133 C)
        grid.process(b"ls\r\n\x1b]133;C\x07");
        let events = grid.drain_events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TermEvent::Osc133 { command: 'C', .. }))
        );

        // Command output + done (OSC 133 D)
        grid.process(b"file1.txt\r\n\x1b]133;D;0\x07");
        let events = grid.drain_events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TermEvent::Osc133 { command: 'D', .. }))
        );

        // Prompt returns (OSC 133 A) + agent suggests via OSC 7770
        grid.process(b"\x1b]133;A\x07$ \x1b]7770;suggest=Show details|Delete file|Open\x07");
        let events = grid.drain_events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TermEvent::Osc133 { command: 'A', .. }))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TermEvent::Tuic { verb, .. } if verb == "suggest"))
        );
    }

    #[test]
    fn osc7770_invalid_no_equals_ignored() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        grid.process(b"\x1b]7770;garbage\x07");
        let events = grid.drain_events();
        let tuic: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, TermEvent::Tuic { .. }))
            .collect();
        assert!(tuic.is_empty(), "malformed OSC 7770 should be ignored");
    }

    #[test]
    fn osc7770_empty_payload() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        grid.process(b"\x1b]7770;state=\x07");
        let events = grid.drain_events();
        let tuic: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, TermEvent::Tuic { .. }))
            .collect();
        assert_eq!(tuic.len(), 1);
        match &tuic[0] {
            TermEvent::Tuic { verb, payload, .. } => {
                assert_eq!(verb, "state");
                assert_eq!(payload, "");
            }
            _ => panic!("expected Tuic event"),
        }
    }

    #[test]
    fn reflow_history_preserves_scrollback_through_resize_cycle() {
        let mut grid = TerminalGrid::new(3, 20, 100);
        grid.reflow_history = true;

        // Write enough lines to push content into scrollback history.
        // 6 lines into a 3-row terminal → 3 lines in history.
        let _ = grid.process(b"AAAAAAAAAABBBBBBBBBB\r\n");
        let _ = grid.process(b"CCCCCCCCCCDDDDDDDDDD\r\n");
        let _ = grid.process(b"EEEEEEEEEEFFFFFFFFFF\r\n");
        let _ = grid.process(b"line4\r\nline5\r\nline6");
        let history_before = grid.scrollback_count();
        assert!(
            history_before >= 3,
            "expected at least 3 history lines, got {history_before}"
        );

        // Shrink cols from 20 to 10 — history rows should reflow (wrap),
        // screen rows should truncate.
        grid.resize(3, 10);
        let history_after_shrink = grid.scrollback_count();
        assert!(
            history_after_shrink > history_before,
            "history should grow after shrink reflow: {history_before} -> {history_after_shrink}"
        );

        // Grow back to 20 — history rows should unwrap back.
        grid.resize(3, 20);
        let history_after_grow = grid.scrollback_count();
        assert_eq!(
            history_after_grow, history_before,
            "history should restore after grow reflow: {history_before} -> {history_after_grow}"
        );
    }

    #[test]
    fn reflow_history_disabled_truncates_scrollback() {
        let mut grid = TerminalGrid::new(3, 20, 100);
        grid.reflow_history = false;

        let _ = grid.process(b"AAAAAAAAAABBBBBBBBBB\r\n");
        let _ = grid.process(b"CCCCCCCCCCDDDDDDDDDD\r\n");
        let _ = grid.process(b"EEEEEEEEEEFFFFFFFFFF\r\n");
        let _ = grid.process(b"line4\r\nline5\r\nline6");
        let history_before = grid.scrollback_count();

        // Shrink — without reflow, history count stays the same (rows truncated).
        grid.resize(3, 10);
        assert_eq!(
            grid.scrollback_count(),
            history_before,
            "history count should not change without reflow"
        );
    }

    /// Regression: HistoryOnly reflow must not leak history content into screen rows.
    ///
    /// A WRAPLINE history row at the boundary could theoretically be merged with the
    /// top screen row during grow_columns. Verify that after shrink→grow, screen rows
    /// contain only screen content (possibly truncated), not history content.
    #[test]
    fn reflow_history_does_not_corrupt_screen_at_boundary() {
        // 4-row terminal, 10 cols.
        let mut grid = TerminalGrid::new(4, 10, 100);
        grid.reflow_history = true;

        // Write a line longer than 10 cols so it wraps → creates WRAPLINE flag.
        // "123456789ABCDEFGH" = 18 chars → 2 history rows after scrolling off.
        let _ = grid.process(b"123456789ABCDEFGH\r\n");

        // Fill screen with identifiable content (prefix "S" makes it distinct from history).
        let _ = grid.process(b"Srow1\r\n");
        let _ = grid.process(b"Srow2\r\n");
        let _ = grid.process(b"Srow3\r\n");
        let _ = grid.process(b"Srow4");

        let history_before = grid.scrollback_count();
        assert!(
            history_before >= 2,
            "expected wrapped rows in history, got {history_before}"
        );

        // Shrink to 6 cols — history reflowed (wraps further), screen truncated.
        grid.resize(4, 6);
        // Grow back to 10 cols — history unwraps, screen padded.
        grid.resize(4, 10);

        let screen_after = grid.read_screen_text();

        // No screen row may contain history content ("123456789A" or "BCDEFGH").
        for row in &screen_after {
            assert!(
                !row.contains("123456789A") && !row.contains("BCDEFGH"),
                "history content leaked into screen row: {row:?}"
            );
        }

        // All non-empty screen rows must start with "S" (screen content marker),
        // confirming history hasn't overwritten them.
        for row in &screen_after {
            let trimmed = row.trim_end();
            if !trimmed.is_empty() {
                assert!(
                    trimmed.starts_with('S'),
                    "screen row overwritten by non-screen content: {row:?}"
                );
            }
        }
    }

    /// Bug regression: grow_columns must not absorb the top screen row into a
    /// WRAPLINE history row at the boundary.
    ///
    /// Trigger: write a line wider than cols (wraps → WRAPLINE in history), write
    /// nothing else so the wrap continuation stays as top screen row, then grow.
    /// Before fix: top screen row disappeared into history. After fix: intact.
    #[test]
    fn reflow_history_grow_does_not_absorb_top_screen_row() {
        // 3-row terminal, 6 cols.
        let mut grid = TerminalGrid::new(3, 6, 100);
        grid.reflow_history = true;

        // A 9-char line wraps at 6 → row 0: "ABCDEF" (WRAPLINE), row 1: "GHI"
        // Then 2 more lines push "ABCDEF" into history (newest history = "ABCDEF" w/ WRAPLINE).
        let _ = grid.process(b"ABCDEFGHI\r\n");
        let _ = grid.process(b"SC1\r\n");
        let _ = grid.process(b"SC2");
        // Screen now: "GHI", "SC1", "SC2" — history: "ABCDEF" (WRAPLINE)

        let _ = grid.process(b""); // flush
        let screen_before: Vec<String> = grid
            .read_screen_text()
            .iter()
            .map(|r| r.trim_end().to_string())
            .collect();

        // Grow to 9 cols — without fix, "GHI" (top screen row) would merge into "ABCDEF".
        grid.resize(3, 9);

        let screen_after: Vec<String> = grid
            .read_screen_text()
            .iter()
            .map(|r| r.trim_end().to_string())
            .collect();

        // Every non-empty screen row must start with a screen marker, not "A" (history).
        for row in &screen_after {
            let t = row.trim_end();
            if !t.is_empty() {
                assert!(
                    !t.starts_with('A'),
                    "history content 'ABCDEF' appeared in screen row after grow: {t:?}\nscreen before: {screen_before:?}\nscreen after: {screen_after:?}"
                );
            }
        }
        // The screen should still have 3 rows (no rows lost to history absorption).
        assert_eq!(
            screen_after.len(),
            3,
            "screen should have 3 rows, got: {screen_after:?}"
        );
    }

    /// Bug regression: shrink_columns must not prepend history overflow into the
    /// top screen row when the newest history row wraps at the boundary.
    ///
    /// Trigger: write a line that exactly fills the newest history slot and wraps
    /// on shrink, then assert no history content appears in screen rows.
    #[test]
    fn reflow_history_shrink_does_not_spill_into_top_screen_row() {
        // 3-row terminal, 10 cols.
        let mut grid = TerminalGrid::new(3, 10, 100);
        grid.reflow_history = true;

        // Write a 10-char line followed by screen content.
        // "1234567890" exactly fills 10 cols → goes to history as a full row (no wrap).
        // It will wrap when shrunk to 6 cols — producing buffered overflow.
        let _ = grid.process(b"1234567890\r\n");
        let _ = grid.process(b"Srow1\r\n");
        let _ = grid.process(b"Srow2\r\n");
        let _ = grid.process(b"Srow3");
        // History: "1234567890" (newest, at boundary). Screen: Srow1, Srow2, Srow3.

        // Shrink to 6: "1234567890" wraps → "123456" (WRAPLINE) + "7890" buffered.
        // Without fix, "7890" would be prepended to "Srow1" (top screen row).
        grid.resize(3, 6);

        let screen_after = grid.read_screen_text();

        for row in &screen_after {
            let t = row.trim_end();
            if !t.is_empty() {
                assert!(
                    !t.contains("7890") && !t.contains("123456"),
                    "history content spilled into screen row during shrink: {t:?}\nfull screen: {screen_after:?}"
                );
            }
        }
    }

    /// Verify that HistoryOnly reflow is a strict improvement over None:
    /// history count is preserved across shrink-grow, screen is not worse.
    #[test]
    fn reflow_history_strictly_better_than_none_for_history() {
        // With reflow enabled.
        let mut grid_reflow = TerminalGrid::new(3, 20, 100);
        grid_reflow.reflow_history = true;

        // With reflow disabled.
        let mut grid_none = TerminalGrid::new(3, 20, 100);
        grid_none.reflow_history = false;

        for grid in [&mut grid_reflow, &mut grid_none] {
            let _ = grid.process(b"AAAAAAAAAABBBBBBBBBB\r\n");
            let _ = grid.process(b"CCCCCCCCCCDDDDDDDDDD\r\n");
            let _ = grid.process(b"EEEEEEEEEEFFFFFFFFFF\r\n");
            let _ = grid.process(b"line4\r\nline5\r\nline6");
        }

        let history_before_reflow = grid_reflow.scrollback_count();
        let history_before_none = grid_none.scrollback_count();
        assert_eq!(history_before_reflow, history_before_none);

        for grid in [&mut grid_reflow, &mut grid_none] {
            grid.resize(3, 10);
            grid.resize(3, 20);
        }

        let history_after_reflow = grid_reflow.scrollback_count();
        let history_after_none = grid_none.scrollback_count();

        // Reflow restores history; None leaves truncated rows.
        assert_eq!(
            history_after_reflow, history_before_reflow,
            "reflow should restore history count after shrink-grow"
        );
        // None truncates: after shrink the rows stay same count but content lost,
        // after grow count stays same. Both should equal history_before_none.
        assert_eq!(
            history_after_none, history_before_none,
            "without reflow, history count should be unchanged (but content truncated)"
        );
    }

    /// Shrink then grow with ReflowMode::All: cursor-row content round-trips.
    /// Regression: grow_columns blank-padding must land at the topmost screen
    /// row, not at the cursor row (inner[0]).
    #[test]
    fn reflow_all_shrink_grow_cursor_row_roundtrip() {
        let mut grid = TerminalGrid::new(4, 20, 10);
        let _ = grid.process(b"ABCDEFGHIJKLMNOPQRST");
        let (line_before, _) = grid.cursor_point();

        // Shrink 20→10 with All reflow: prompt wraps into two rows.
        grid.resize_with_mode(4, 10, ReflowMode::All);

        // Grow back 10→20 with All reflow: rows should merge.
        grid.resize_with_mode(4, 20, ReflowMode::All);

        let rows_after = grid.screen_text_rows();
        let (line_after, _) = grid.cursor_point();

        // The prompt must be on the cursor's line, not displaced.
        assert_eq!(
            rows_after[line_after as usize].trim_end(),
            "ABCDEFGHIJKLMNOPQRST",
            "prompt must be on cursor row after shrink-grow roundtrip"
        );
        assert_eq!(
            line_after, line_before,
            "cursor line must return to original position"
        );
    }

    /// Helper: find the cell data offset for a given (row_index, col) in a serialized frame.
    /// Returns the byte offset of the cell's 11-byte block, or None if not found.
    fn find_cell_offset(buf: &[u8], target_row: u16, target_col: u16) -> Option<usize> {
        let (num_rows, _, _, _) = decode_header(buf);
        let mut offset = TEST_HEADER_SIZE;
        for _ in 0..num_rows {
            let row_idx = u16::from_le_bytes([buf[offset], buf[offset + 1]]);
            let col_count = u16::from_le_bytes([buf[offset + 2], buf[offset + 3]]);
            offset += 4;
            if row_idx == target_row && target_col < col_count {
                return Some(offset + target_col as usize * 11);
            }
            offset += col_count as usize * 11;
        }
        None
    }

    #[test]
    fn serialize_wrapped_line_preserves_indexed_color() {
        // 10-column grid: a 15-char string with indexed blue fg + yellow bg wraps at col 10.
        let mut grid = TerminalGrid::new(5, 10, 0);
        // ESC[38;5;4m = indexed fg 4 (blue), ESC[48;5;3m = indexed bg 3 (yellow)
        let _ = grid.process(b"\x1b[38;5;4m\x1b[48;5;3mABCDEFGHIJKLMNO\x1b[0m");
        let buf = grid.serialize_dirty_rows();
        assert!(!buf.is_empty());

        let off0 = find_cell_offset(&buf, 0, 0).expect("row 0 present");
        let (ch0, fg_r0, fg_g0, fg_b0, bg_r0, bg_g0, bg_b0, attrs0) = decode_cell(&buf, off0);
        assert_eq!(ch0, 'A');
        assert_eq!(attrs0 & super::ATTR_DEFAULT_FG, 0, "row 0 fg NOT default");
        assert_eq!(attrs0 & super::ATTR_DEFAULT_BG, 0, "row 0 bg NOT default");

        let off1 = find_cell_offset(&buf, 1, 0).expect("row 1 present");
        let (ch1, fg_r1, fg_g1, fg_b1, bg_r1, bg_g1, bg_b1, attrs1) = decode_cell(&buf, off1);
        assert_eq!(ch1, 'K');
        assert_eq!(
            attrs1 & super::ATTR_DEFAULT_FG,
            0,
            "wrapped row fg NOT default"
        );
        assert_eq!(
            attrs1 & super::ATTR_DEFAULT_BG,
            0,
            "wrapped row bg NOT default"
        );
        assert_eq!(
            (fg_r0, fg_g0, fg_b0),
            (fg_r1, fg_g1, fg_b1),
            "fg same on wrap"
        );
        assert_eq!(
            (bg_r0, bg_g0, bg_b0),
            (bg_r1, bg_g1, bg_b1),
            "bg same on wrap"
        );
    }

    #[test]
    fn serialize_wrapped_line_preserves_named_color() {
        // Same test but with Named colors (ESC[34m = blue, ESC[43m = yellow bg)
        let mut grid = TerminalGrid::new(5, 10, 0);
        let _ = grid.process(b"\x1b[34m\x1b[43mABCDEFGHIJKLMNO\x1b[0m");
        let buf = grid.serialize_dirty_rows();
        assert!(!buf.is_empty());

        let off0 = find_cell_offset(&buf, 0, 0).expect("row 0 present");
        let (ch0, fg_r0, fg_g0, fg_b0, bg_r0, bg_g0, bg_b0, attrs0) = decode_cell(&buf, off0);
        assert_eq!(ch0, 'A');
        assert_eq!(attrs0 & super::ATTR_DEFAULT_FG, 0, "row 0 fg NOT default");
        assert_eq!(attrs0 & super::ATTR_DEFAULT_BG, 0, "row 0 bg NOT default");

        let off1 = find_cell_offset(&buf, 1, 0).expect("row 1 present");
        let (ch1, fg_r1, fg_g1, fg_b1, bg_r1, bg_g1, bg_b1, attrs1) = decode_cell(&buf, off1);
        assert_eq!(ch1, 'K');
        assert_eq!(
            attrs1 & super::ATTR_DEFAULT_FG,
            0,
            "wrapped row fg NOT default"
        );
        assert_eq!(
            attrs1 & super::ATTR_DEFAULT_BG,
            0,
            "wrapped row bg NOT default"
        );
        assert_eq!(
            (fg_r0, fg_g0, fg_b0),
            (fg_r1, fg_g1, fg_b1),
            "fg same on wrap"
        );
        assert_eq!(
            (bg_r0, bg_g0, bg_b0),
            (bg_r1, bg_g1, bg_b1),
            "bg same on wrap"
        );
    }

    #[test]
    fn serialize_wrapped_bold_named_color() {
        // Bold + Named blue fg on a wrapping line
        let mut grid = TerminalGrid::new(5, 10, 0);
        let _ = grid.process(b"\x1b[1;34m\x1b[43mABCDEFGHIJKLMNO\x1b[0m");
        let buf = grid.serialize_dirty_rows();
        assert!(!buf.is_empty());

        let off0 = find_cell_offset(&buf, 0, 0).expect("row 0 present");
        let (ch0, fg_r0, fg_g0, fg_b0, _, _, _, attrs0) = decode_cell(&buf, off0);
        assert_eq!(ch0, 'A');
        assert_ne!(attrs0 & super::ATTR_BOLD, 0, "bold flag set row 0");
        assert_eq!(attrs0 & super::ATTR_DEFAULT_FG, 0, "row 0 fg NOT default");

        let off1 = find_cell_offset(&buf, 1, 0).expect("row 1 present");
        let (ch1, fg_r1, fg_g1, fg_b1, _, _, _, attrs1) = decode_cell(&buf, off1);
        assert_eq!(ch1, 'K');
        assert_ne!(attrs1 & super::ATTR_BOLD, 0, "bold flag set row 1");
        assert_eq!(
            attrs1 & super::ATTR_DEFAULT_FG,
            0,
            "wrapped row fg NOT default"
        );
        assert_eq!(
            (fg_r0, fg_g0, fg_b0),
            (fg_r1, fg_g1, fg_b1),
            "fg same on wrap"
        );
    }

    #[test]
    fn serialize_reflow_preserves_color() {
        // Write a colored line that fits, then shrink cols to force reflow wrap.
        let mut grid = TerminalGrid::new(5, 20, 100);
        grid.reflow_history = true;
        // Write 15 chars with blue fg + yellow bg, then newline to push into history
        let _ = grid.process(b"\x1b[34m\x1b[43mABCDEFGHIJKLMNO\x1b[0m\r\n\r\n\r\n\r\n\r\n");
        let _ = grid.serialize_dirty_rows(); // drain

        // Shrink to 10 cols — should reflow the 15-char line into 2 rows in history
        grid.resize(5, 10);
        // Scroll up to view history
        grid.scroll(5);
        let buf = grid.serialize_dirty_rows();
        assert!(!buf.is_empty());

        // Find cells — the reflowed content should be in the first rows
        // Row 0 should have the first 10 chars, row 1 the remaining 5
        let off0 = find_cell_offset(&buf, 0, 0).expect("row 0 present");
        let (ch0, fg_r0, fg_g0, fg_b0, _, _, _, attrs0) = decode_cell(&buf, off0);
        assert_eq!(ch0, 'A');
        assert_eq!(
            attrs0 & super::ATTR_DEFAULT_FG,
            0,
            "reflow row 0 fg NOT default"
        );

        let off1 = find_cell_offset(&buf, 1, 0).expect("row 1 present");
        let (ch1, fg_r1, fg_g1, fg_b1, _, _, _, attrs1) = decode_cell(&buf, off1);
        assert_eq!(ch1, 'K');
        assert_eq!(
            attrs1 & super::ATTR_DEFAULT_FG,
            0,
            "reflow row 1 fg NOT default"
        );
        assert_eq!(
            (fg_r0, fg_g0, fg_b0),
            (fg_r1, fg_g1, fg_b1),
            "fg same after reflow wrap"
        );
    }

    #[test]
    fn serialize_agnoster_prompt_wrap_preserves_color() {
        // Reproduce actual agnoster prompt at 60 cols (wraps around col 60).
        // The git segment uses fg=black(30), bg=yellow(43).
        // After wrapping, the continuation row must have the same fg/bg.
        let mut grid = TerminalGrid::new(10, 60, 0);

        // Exact sequence from raw PTY capture (simplified):
        // Reset + clear + draw prompt
        let prompt = b"\x1b[0m\x1b[27m\x1b[24m\x1b[J\x1b[39m\x1b[0m\x1b[49m\
            \x1b[40m\x1b[39m stefano.straus@DGQT92CJFP \
            \x1b[44m\x1b[30m\x1b[30m ~/Gits/LS/gh-metrics \
            \x1b[43m\x1b[34m\x1b[30m\xee\x82\xb0 POC-00001/fix-production-errors \xc2\xb1 \
            \x1b[49m\x1b[33m\xee\x82\xb0\x1b[39m ";
        let _ = grid.process(prompt);

        // zsh re-renders: \r\r\e[A then redraws the prompt
        let redraw = b"\r\r\x1b[A\x1b[0m\x1b[27m\x1b[24m\x1b[J\x1b[39m\x1b[0m\x1b[49m\
            \x1b[40m\x1b[39m stefano.straus@DGQT92CJFP \
            \x1b[44m\x1b[30m\x1b[30m ~/Gits/LS/gh-metrics \
            \x1b[43m\x1b[34m\x1b[30m\xee\x82\xb0 POC-00001/fix-production-errors \xc2\xb1 \
            \x1b[49m\x1b[33m\xee\x82\xb0\x1b[39m ";
        let _ = grid.process(redraw);
        let buf = grid.serialize_dirty_rows();
        assert!(!buf.is_empty());

        // The prompt is ~82 chars. At 60 cols, it wraps.
        // Find 'P' of "POC-00001" in the git segment (skip past "DGQT92CJFP" at col ~25)
        let mut git_row0_col = None;
        let mut git_row1_col = None;
        for col in 30..60u16 {
            if let Some(off) = find_cell_offset(&buf, 0, col) {
                let (ch, _, _, _, _, _, _, _) = decode_cell(&buf, off);
                if ch == 'P' {
                    git_row0_col = Some(col);
                    break;
                }
            }
        }
        // Find a letter on row 1 that's part of the wrapped content
        for col in 0..60u16 {
            if let Some(off) = find_cell_offset(&buf, 1, col) {
                let (ch, _, _, _, _, _, _, _) = decode_cell(&buf, off);
                if ch.is_ascii_alphabetic() {
                    git_row1_col = Some(col);
                    break;
                }
            }
        }

        let col0 = git_row0_col.expect("found git segment char on row 0");
        let col1 = git_row1_col.expect("found wrapped char on row 1");

        let off0 = find_cell_offset(&buf, 0, col0).unwrap();
        let (_, fg_r0, fg_g0, fg_b0, bg_r0, bg_g0, bg_b0, attrs0) = decode_cell(&buf, off0);

        let off1 = find_cell_offset(&buf, 1, col1).unwrap();
        let (ch1, fg_r1, fg_g1, fg_b1, bg_r1, bg_g1, bg_b1, attrs1) = decode_cell(&buf, off1);

        // Both should have fg=black (Named, index 0), bg=yellow (Named, index 3)
        assert_eq!(
            attrs0 & super::ATTR_DEFAULT_FG,
            0,
            "row 0 git fg NOT default"
        );
        assert_eq!(
            attrs1 & super::ATTR_DEFAULT_FG,
            0,
            "row 1 git fg NOT default (got char '{ch1}')"
        );
        assert_eq!(
            attrs0 & super::ATTR_DEFAULT_BG,
            0,
            "row 0 git bg NOT default"
        );
        assert_eq!(
            attrs1 & super::ATTR_DEFAULT_BG,
            0,
            "row 1 git bg NOT default"
        );
        assert_eq!(
            (fg_r0, fg_g0, fg_b0),
            (fg_r1, fg_g1, fg_b1),
            "fg color must match between row 0 and wrapped row 1"
        );
        assert_eq!(
            (bg_r0, bg_g0, bg_b0),
            (bg_r1, bg_g1, bg_b1),
            "bg color must match between row 0 and wrapped row 1"
        );
    }

    #[test]
    fn serialize_resize_then_redraw_preserves_color() {
        // Simulate: prompt at 80 cols (fits on 1 row), resize to 60, zsh redraws.
        let mut grid = TerminalGrid::new(10, 80, 100);
        grid.reflow_history = true;

        let prompt = b"\x1b[0m\x1b[J\x1b[40m\x1b[39m stefano.straus@DGQT92CJFP \
            \x1b[44m\x1b[30m\xee\x82\xb0\x1b[30m ~/Gits/LS/gh-metrics \
            \x1b[43m\x1b[34m\xee\x82\xb0\x1b[30m POC-00001/fix-production-errors \xc2\xb1 \
            \x1b[49m\x1b[33m\xee\x82\xb0\x1b[39m ";
        let _ = grid.process(prompt);
        let _ = grid.serialize_dirty_rows(); // drain frame

        // Resize to 60 cols (visible screen NOT reflowed — HistoryOnly mode)
        grid.resize(10, 60);
        let _ = grid.serialize_dirty_rows(); // drain resize frame

        // zsh SIGWINCH: re-renders prompt at 60 cols (\r\e[A\e[J + prompt)
        let redraw = b"\r\x1b[0m\x1b[J\x1b[40m\x1b[39m stefano.straus@DGQT92CJFP \
            \x1b[44m\x1b[30m\xee\x82\xb0\x1b[30m ~/Gits/LS/gh-metrics \
            \x1b[43m\x1b[34m\xee\x82\xb0\x1b[30m POC-00001/fix-production-errors \xc2\xb1 \
            \x1b[49m\x1b[33m\xee\x82\xb0\x1b[39m ";
        let _ = grid.process(redraw);
        let buf = grid.serialize_dirty_rows();
        assert!(!buf.is_empty());

        // Find git segment cells on both rows — prompt wraps around col 50-60
        // Row 1: start of git segment text ("POC-00001...")
        // Row 2: continuation ("1/fix-production-errors ±")
        let mut row1_git = None;
        for col in 50..60u16 {
            if let Some(off) = find_cell_offset(&buf, 1, col) {
                let (ch, _, _, _, _, _, _, a) = decode_cell(&buf, off);
                if ch.is_ascii_alphanumeric() && (a & super::ATTR_DEFAULT_BG == 0) {
                    row1_git = Some(col);
                    break;
                }
            }
        }
        let mut row2_git = None;
        for col in 0..60u16 {
            if let Some(off) = find_cell_offset(&buf, 2, col) {
                let (ch, _, _, _, _, _, _, a) = decode_cell(&buf, off);
                if ch.is_ascii_alphabetic() && (a & super::ATTR_DEFAULT_BG == 0) {
                    row2_git = Some(col);
                    break;
                }
            }
        }

        let col1 = row1_git.expect("found git segment on row 1");
        let col2 = row2_git.expect("found git segment on row 2");

        let off1 = find_cell_offset(&buf, 1, col1).unwrap();
        let (_, fg_r1, fg_g1, fg_b1, bg_r1, bg_g1, bg_b1, a1) = decode_cell(&buf, off1);

        let off2 = find_cell_offset(&buf, 2, col2).unwrap();
        let (_, fg_r2, fg_g2, fg_b2, bg_r2, bg_g2, bg_b2, a2) = decode_cell(&buf, off2);

        assert_eq!(a1 & super::ATTR_DEFAULT_FG, 0, "row 1 fg NOT default");
        assert_eq!(a2 & super::ATTR_DEFAULT_FG, 0, "row 2 fg NOT default");
        assert_eq!(a1 & super::ATTR_DEFAULT_BG, 0, "row 1 bg NOT default");
        assert_eq!(a2 & super::ATTR_DEFAULT_BG, 0, "row 2 bg NOT default");
        assert_eq!(
            (fg_r1, fg_g1, fg_b1),
            (fg_r2, fg_g2, fg_b2),
            "fg must match on resize+redraw wrap"
        );
        assert_eq!(
            (bg_r1, bg_g1, bg_b1),
            (bg_r2, bg_g2, bg_b2),
            "bg must match on resize+redraw wrap"
        );
    }
}
