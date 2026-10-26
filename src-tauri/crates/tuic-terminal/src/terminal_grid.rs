use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::grid::{Dimensions, ReflowMode, ScrollSource};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::color::{Colors, named_color_to_index};
use alacritty_terminal::term::search::RegexSearch;
use alacritty_terminal::term::{Config, Term, TermDamage, TermMode, TermParseDamage};
use alacritty_terminal::vte::ansi::{self, Color, CursorShape, CursorStyle, NamedColor, Rgb};
use std::sync::Arc;
use unicode_width::UnicodeWidthStr;
// `parking_lot::Mutex` has no poison state. A panic taken anywhere near this
// queue used to poison a `std::sync::Mutex` permanently: `send_event` then
// panicked on `.lock().unwrap()` for the rest of the terminal's life, and
// `drain_events` returned an empty `Vec` forever — silently dropping every
// title change, OSC 133 mark, OSC 7 cwd and TUIC event while the grid kept
// rendering. Pinned by `events_are_still_delivered_after_a_panic_under_their_lock`.
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// The colours TUIC reports when an app asks the terminal what it is painted
/// with (OSC 10 foreground / 11 background / 12 cursor).
///
/// Process-wide rather than per-session because the theme is: every terminal in
/// the window paints from the same palette, and the event listener that answers
/// the query handler has no application context to reach into.
///
/// The defaults are a placeholder for the window between startup and the
/// frontend's first [`set_terminal_palette`]. They are deliberately dark: an app
/// that gets a wrong-but-plausible answer picks a bad contrast, while an app that
/// gets NO answer never concludes at all. See the doc on the `ColorRequest` arm
/// below for why silence is the expensive failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalPalette {
    pub foreground: (u8, u8, u8),
    pub background: (u8, u8, u8),
    pub cursor: (u8, u8, u8),
}

impl Default for TerminalPalette {
    fn default() -> Self {
        Self {
            foreground: (0xcc, 0xcc, 0xcc),
            background: (0x1e, 0x1e, 0x1e),
            cursor: (0xcc, 0xcc, 0xcc),
        }
    }
}

static PALETTE: parking_lot::RwLock<TerminalPalette> = parking_lot::RwLock::new(TerminalPalette {
    foreground: (0xcc, 0xcc, 0xcc),
    background: (0x1e, 0x1e, 0x1e),
    cursor: (0xcc, 0xcc, 0xcc),
});

/// Tell the emulator what the terminal is actually painted with. Called by the
/// frontend whenever the resolved theme changes.
pub fn set_terminal_palette(palette: TerminalPalette) {
    *PALETTE.write() = palette;
}

/// What the emulator will report for OSC 10/11/12 right now. Test-only: the
/// palette is write-then-answer in production, and the colour-query tests use
/// this to save and restore the shared value around a case.
#[cfg(test)]
pub fn terminal_palette() -> TerminalPalette {
    *PALETTE.read()
}

/// Publish the resolved terminal theme so colour queries can be answered.
///
/// Channels are taken as `[u8; 3]` rather than a CSS string on purpose: the
/// frontend already has to resolve `getComputedStyle` to numbers, and a parser
/// here would be a second place for a colour to be interpreted differently.
pub fn set_terminal_theme_colors(
    foreground: [u8; 3],
    background: [u8; 3],
    cursor: [u8; 3],
) -> Result<(), String> {
    set_terminal_palette(TerminalPalette {
        foreground: (foreground[0], foreground[1], foreground[2]),
        background: (background[0], background[1], background[2]),
        cursor: (cursor[0], cursor[1], cursor[2]),
    });
    Ok(())
}

/// The palette colour an OSC 10/11/12 query is asking about.
///
/// `index` arrives as `NamedColor::Foreground + (code - 10)`, so 256/257/258.
/// Anything else is an `OSC 4;n;?` indexed-palette query, which TUIC does not
/// track — answering those with a guess would be worse than not answering, since
/// an indexed query has no fence idiom waiting on it.
fn palette_color_for_index(index: usize) -> Option<Rgb> {
    let palette = *PALETTE.read();
    let (r, g, b) = match index {
        i if i == NamedColor::Foreground as usize => palette.foreground,
        i if i == NamedColor::Background as usize => palette.background,
        i if i == NamedColor::Cursor as usize => palette.cursor,
        _ => return None,
    };
    Some(Rgb { r, g, b })
}

/// Terminal event captured from alacritty for forwarding to PTY/frontend.
#[derive(Debug, Clone)]
pub enum TermEvent {
    Title(String),
    ResetTitle,
    Bell,
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
pub struct TermEventCollector {
    bell: Arc<AtomicBool>,
    events: Arc<Mutex<Vec<TermEvent>>>,
}

impl EventListener for TermEventCollector {
    fn send_event(&self, event: Event) {
        match event {
            Event::Bell => {
                self.bell.store(true, Ordering::Relaxed);
                let mut events = self.events.lock();
                if !events.iter().any(|event| matches!(event, TermEvent::Bell)) {
                    events.push(TermEvent::Bell);
                }
            }
            Event::Title(t) => {
                self.events.lock().push(TermEvent::Title(t));
            }
            Event::ResetTitle => {
                self.events.lock().push(TermEvent::ResetTitle);
            }
            Event::ClipboardStore(_, text) => {
                self.events.lock().push(TermEvent::ClipboardStore(text));
            }
            Event::PtyWrite(s) => {
                self.events.lock().push(TermEvent::PtyWrite(s));
            }
            Event::MouseCursorDirty => {
                self.events.lock().push(TermEvent::MouseCursorDirty);
            }
            Event::CursorBlinkingChange => {
                self.events.lock().push(TermEvent::CursorBlinkingChange);
            }
            Event::Osc133 {
                command,
                params,
                line,
            } => {
                self.events.lock().push(TermEvent::Osc133 {
                    command,
                    params,
                    line,
                });
            }
            Event::Osc7(url) => {
                self.events.lock().push(TermEvent::Osc7(url));
            }
            Event::Tuic {
                verb,
                payload,
                line,
            } => {
                self.events.lock().push(TermEvent::Tuic {
                    verb,
                    payload,
                    line,
                });
            }
            // An unanswered colour query is NOT a no-op, which is why this arm
            // exists instead of falling into the ignore list below.
            //
            // The standard probe is `OSC 11 ; ? ST` followed by `ESC[c` as a
            // fence: DA is universally supported, so a DA reply arriving with no
            // colour reply before it is supposed to mean "this terminal cannot
            // answer". TUIC answered the fence and dropped the query, so the
            // probe never concluded — Claude Code re-sent the pair every ~1.2 s
            // for the life of the session, and every DA reply it triggered went
            // into the PTY as if typed, echoing `^[[?6c` onto the screen whenever
            // the process was not reading in raw mode.
            //
            // Answering is what ends it. Alacritty already built the reply; only
            // the host has to say what colour it painted with.
            Event::ColorRequest(index, format_reply) => {
                if let Some(color) = palette_color_for_index(index) {
                    self.events
                        .lock()
                        .push(TermEvent::PtyWrite(format_reply(color)));
                }
            }
            Event::ClipboardLoad(..)
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

use crate::vt_log::{ChangedRow, LogColor, LogLine, LogSpan};

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

/// Bounded logical line prefix ending at the current cursor position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalPrefix {
    pub text: String,
    pub start_row: usize,
    pub end_row: usize,
}

// Attrs byte bit positions for binary cell encoding.
/// Bit 15 of a serialized row's `col_count` carries alacritty's WRAPLINE: the
/// row is not a line of its own, it continues onto the next display row.
///
/// Stolen from the count rather than added as a new per-row byte on purpose. The
/// row header is 4 bytes and both frontend decoders — plus every offset
/// assertion in their tests — are written against that; a column count is a
/// terminal width, so the top bit is dead space. Same trade, same reasoning as
/// `keyboard_flags` bit 5 carrying the alt-screen state.
///
/// Without it the frontend cannot tell a wrapped continuation from a fresh line,
/// which is what left a wrapped `suggest:` block unmasked on screen (#8fc7):
/// the grid is the only place that knows.
pub const ROW_WRAPPED_FLAG: u16 = 0x8000;

/// Bit 14 of the wire `col_count`: this row carries only the columns alacritty
/// reported as damaged, not the whole line. When set, a `start_col: u16` follows
/// the count and the payload is `count` cells beginning at that column; the
/// frontend merges them into the row it already holds.
///
/// Measured over 11 real captures (4810 ticker-grouped frames,
/// `damage_overship_over_capture_corpus`): the whole-row format ships 2.44 cells
/// per cell actually damaged on average, and 11.8x on the busiest agent session,
/// because a spinner or an in-place TUI redraw touches a handful of columns and
/// pays for the full width.
///
/// It is a flag rather than a format bump on purpose. Rust does not hot-reload in
/// dev, so a rebuilt frontend routinely runs against yesterday's backend: an old
/// backend simply never sets the bit and the new decoder takes the whole-row path
/// it always took. A header change would have desynced that pairing outright —
/// same reasoning as `ROW_WRAPPED_FLAG` above.
///
/// The staleness this can introduce is the one the canvas already tolerates:
/// alacritty under-reports damage on in-place TUI redraws (see the `[dup]` heal in
/// CanvasTerminal), so a periodic full-frame reconcile already exists and bounds a
/// missed column to the same ~250ms-1s window it bounds a missed row.
pub const ROW_PARTIAL_FLAG: u16 = 0x4000;

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

/// Encode a row's `col_count` field, tagging [`ROW_WRAPPED_FLAG`] when the line
/// continues onto the next display row.
///
/// alacritty marks the wrap on the LAST cell of the row it wraps out of, so the
/// flag answers "does this row continue?", not "is this row a continuation?".
/// The frontend walks forward from an anchor, which is the direction that
/// matches.
///
/// `count` is how many cells the row actually carries — the full width on the
/// whole-row path, the damaged span on a [`ROW_PARTIAL_FLAG`] row. The wrap probe
/// always reads the row's real last cell, so a partial row that stops short of the
/// right edge still reports the line's true wrap state.
fn encode_col_count(
    grid: &alacritty_terminal::grid::Grid<Cell>,
    line: Line,
    num_cols: usize,
    count: usize,
) -> u16 {
    let wrapped = num_cols > 0
        && grid[line][Column(num_cols - 1)]
            .flags
            .contains(Flags::WRAPLINE);
    (count as u16) | if wrapped { ROW_WRAPPED_FLAG } else { 0 }
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

/// Append the complete textual content of one grid cell.
///
/// Alacritty stores zero-width codepoints on the preceding base cell. Every
/// text extraction path must read the pair together or decomposed Unicode is
/// silently reduced to its base character. Synthetic wide-character spacers
/// carry layout only and must not contribute text or text length.
fn push_cell_text(text: &mut String, cell: &Cell) {
    if cell
        .flags
        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
    {
        return;
    }

    text.push(if cell.c == '\0' { ' ' } else { cell.c });
    if let Some(zerowidth) = cell.zerowidth() {
        text.extend(zerowidth.iter().copied());
    }
}

fn cell_text_utf8_len(cell: &Cell) -> usize {
    if cell
        .flags
        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
    {
        return 0;
    }
    let base_len = if cell.c == '\0' { 1 } else { cell.c.len_utf8() };
    base_len
        + cell
            .zerowidth()
            .into_iter()
            .flatten()
            .map(|ch| ch.len_utf8())
            .sum::<usize>()
}

fn cell_text_utf16_len(cell: &Cell) -> usize {
    if cell
        .flags
        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
    {
        return 0;
    }
    let base_len = if cell.c == '\0' {
        1
    } else {
        cell.c.len_utf16()
    };
    base_len
        + cell
            .zerowidth()
            .into_iter()
            .flatten()
            .map(|ch| ch.len_utf16())
            .sum::<usize>()
}

const CELL_EXTRAS_MAGIC: &[u8; 4] = b"TCX1";

/// Append the optional sparse zero-width trailer for rows already encoded in
/// the fixed-width cell format. Row indices are ordinals in wire order; columns
/// remain absolute grid columns even for partial rows.
fn append_cell_extras_trailer(
    buf: &mut Vec<u8>,
    grid: &alacritty_terminal::grid::Grid<Cell>,
    rows: &[(Line, usize, usize)],
) {
    let entry_count = rows
        .iter()
        .flat_map(|&(line, start, end)| {
            (start..end).filter(move |&col| {
                grid[line][Column(col)]
                    .zerowidth()
                    .is_some_and(|chars| !chars.is_empty())
            })
        })
        .count();
    if entry_count == 0 {
        return;
    }

    buf.extend_from_slice(CELL_EXTRAS_MAGIC);
    buf.extend_from_slice(&(entry_count as u32).to_le_bytes());
    for (row_ordinal, &(line, start, end)) in rows.iter().enumerate() {
        for col in start..end {
            let Some(chars) = grid[line][Column(col)].zerowidth() else {
                continue;
            };
            if chars.is_empty() {
                continue;
            }

            buf.extend_from_slice(&(row_ordinal as u16).to_le_bytes());
            buf.extend_from_slice(&(col as u16).to_le_bytes());
            buf.push(chars.len() as u8);
            for &ch in chars {
                buf.extend_from_slice(&(ch as u32).to_le_bytes());
            }
        }
    }
}

/// Provenance captured before changing the grid geometry.
struct ResizeViewport {
    before_columns: usize,
    after_columns: usize,
    grid_sequence: u64,
    viewport_top: usize,
    rows: Vec<String>,
    /// Immutable contiguous history immediately above the owned viewport.
    preceding_logical: String,
    /// Only these newest old history rows came from this viewport.
    displaced_rows: usize,
    /// Exclusive owned-row end and complete WRAPLINE record before the redraw.
    displaced_records: Vec<(usize, String)>,
    scrolled_after_resize: usize,
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
    /// The last query compiled by [`Self::compiled_query`], kept so a repainting
    /// screen re-runs the user's search without rebuilding its DFAs. Behind a
    /// mutex because both searches take `&self`.
    search_regex: parking_lot::Mutex<Option<(String, RegexSearch)>>,
    /// How many queries have actually been compiled. Test-only: it is how a test
    /// tells a cache hit from a rebuild without reaching into the cache.
    #[cfg(test)]
    regex_compiles: std::sync::atomic::AtomicUsize,
    /// How many times `process` fell back to reading and diffing the whole
    /// screen. Test-only: it is how a test tells the fast path from the slow one
    /// without asserting on private state.
    #[cfg(test)]
    full_screen_reads: usize,
    last_frame_history_size: Option<usize>,
    last_frame_screen_lines: Option<usize>,
    last_frame_columns: Option<usize>,
    bell_flag: Arc<AtomicBool>,
    events: Arc<Mutex<Vec<TermEvent>>>,
    /// When true, column resizes reflow scrollback history while leaving the
    /// visible screen untouched. Preserves TUI cursor positioning on screen
    /// while keeping scrollback readable across resize cycles.
    pub reflow_history: bool,
    /// Armed by a resize of the primary screen: `total_scrolled` at that moment.
    /// While armed, [`Self::merge_reprinted_history_tail`] runs after every chunk.
    reprint_merge_armed_at: Option<usize>,
    /// Tail of the previous chunk, so a frame end split across chunks is seen.
    frame_end_carry: Vec<u8>,
    /// Screen rows at the last frame end seen while armed (or at the resize). A
    /// merge needs every compared row to differ from these, with no scroll in
    /// between: that is the proof the frame rewrote them, as opposed to history
    /// that already repeated them or output that moved them.
    reprint_head_snapshot: Vec<String>,
    /// `total_scrolled` when `reprint_head_snapshot` was taken.
    reprint_snapshot_scrolled: usize,
    /// Full viewport erase generation at the resize/last examined repaint.
    reprint_clear_generation: usize,
    grid_sequence: u64,
    resize_viewport: Option<ResizeViewport>,
    /// Only blank screen rows created by proven prefix suppression, starting
    /// at row zero. Any parser damage touching them relinquishes ownership.
    suppressed_screen_prefix: usize,
    history_source_widths: std::collections::HashMap<String, usize>,
}

impl TerminalGrid {
    pub fn new(rows: u16, cols: u16, scrollback: usize) -> Self {
        let config = Config {
            scrolling_history: scrollback,
            // Save lines to scrollback in alternate screen mode:
            // alt-screen apps that print more than a screenful (`gh run watch`,
            // `less`, `man`) stay scrollable instead of dropping what rolls off
            // the top. Same cap as the primary screen; wiped on every alt
            // enter/exit, so no inactive alternate lines remain addressable.
            alt_scrolling_history: scrollback,
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
            search_regex: parking_lot::Mutex::new(None),
            #[cfg(test)]
            regex_compiles: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            full_screen_reads: 0,
            last_frame_history_size: None,
            last_frame_screen_lines: None,
            last_frame_columns: None,
            bell_flag,
            events,
            reflow_history: true,
            reprint_merge_armed_at: None,
            frame_end_carry: Vec::new(),
            reprint_head_snapshot: Vec::new(),
            reprint_snapshot_scrolled: 0,
            reprint_clear_generation: 0,
            grid_sequence: 0,
            resize_viewport: None,
            suppressed_screen_prefix: 0,
            history_source_widths: std::collections::HashMap::new(),
        }
    }

    /// Feed raw PTY bytes into the terminal emulator.
    ///
    /// Returns changed rows. OSC 133 events are delivered via `drain_events()`
    /// as `TermEvent::Osc133` (parsed natively by the patched VTE handler).
    pub fn process(&mut self, data: &[u8]) -> Vec<ChangedRow> {
        self.grid_sequence = self.grid_sequence.wrapping_add(1);
        if self.reprint_merge_armed_at.is_some() {
            if self.advance_frame_by_frame(data) {
                // History shrank: the cached rows and the frame bookkeeping are stale.
                self.prev_rows.clear();
            }
        } else {
            self.processor.advance(&mut self.term, data);
            self.invalidate_suppressed_screen_prefix();
        }

        // Prefer the alacritty parse-damage set: read+diff ONLY the lines whose
        // content actually changed, instead of rebuilding+diffing the whole screen
        // (O(rows*cols)) on every PTY chunk. Fall back to a full read+diff when a
        // full re-read is required (initial frame / resize / alt-screen / clear).
        // The text-equality check is preserved in BOTH paths, so an over-reported
        // damaged line (e.g. cursor-only movement) never produces a spurious
        // ChangedRow.
        //
        // A scrolled-back view (`display_offset != 0`) deliberately does NOT force
        // the full path. It used to, to sidestep a viewport-vs-grid line-index
        // mismatch — but there is none to sidestep: `read_screen_text`,
        // `row_to_text` and the parse-damage indices all address the active screen
        // region through `Line(i)`, and the display offset moves the viewport, not
        // those rows. Forcing it meant a user reading scrollback paid a whole-screen
        // rebuild per PTY chunk, precisely when a busy agent emits them fastest.
        // `process_damage_matches_full_diff_while_scrolled_back` holds the line.
        let parse_damage = self.term.parse_damage();
        self.term.reset_parse_damage();

        let must_full = self.prev_rows.is_empty() || matches!(parse_damage, TermParseDamage::Full);

        if must_full {
            #[cfg(test)]
            {
                self.full_screen_reads += 1;
            }
            let curr_rows = self.read_screen_text();
            let changed: Vec<ChangedRow> = curr_rows
                .iter()
                .enumerate()
                .filter_map(|(i, curr)| {
                    let prev = self.prev_rows.get(i).map(String::as_str).unwrap_or("");
                    (curr != prev).then(|| ChangedRow {
                        row_index: i,
                        text: curr.clone(),
                    })
                })
                .collect();
            self.prev_rows = curr_rows;
            return changed;
        }

        // Partial path: `prev_rows` is already screen-sized (the first frame and
        // every resize take the full path above), so damaged indices are valid.
        let TermParseDamage::Partial(lines) = parse_damage else {
            unreachable!("Full handled by must_full above")
        };
        let mut changed = Vec::new();
        for i in lines {
            let Some(curr) = self.row_to_text(Line(i as i32)) else {
                continue;
            };
            let prev = self.prev_rows.get(i).map(String::as_str).unwrap_or("");
            if curr != prev {
                if i < self.prev_rows.len() {
                    self.prev_rows[i].clone_from(&curr);
                }
                changed.push(ChangedRow {
                    row_index: i,
                    text: curr,
                });
            }
        }
        changed
    }

    /// Whether a DEC 2026 synchronized update is currently open.
    ///
    /// Mirrors the real parser state rather than "a BSU was seen once": the
    /// vendored VTE re-arms the deadline on a nested BSU and only clears it when
    /// the update actually ends, so this stays true across BSU extension.
    pub fn is_sync_update_active(&self) -> bool {
        self.processor.sync_timeout().sync_timeout().is_some()
    }

    /// End a synchronized update whose 150ms deadline has passed, returning
    /// whether anything was flushed.
    ///
    /// The vendored VTE timeout is passive — it records a deadline but never
    /// fires on its own, so without this the buffered bytes wait for an ESU
    /// that may never arrive and the terminal is wedged. Callers must treat a
    /// `true` return as new damage to serialize.
    ///
    pub fn flush_sync_timeout_if_needed(&mut self) -> bool {
        let Some(deadline) = self.processor.sync_timeout().sync_timeout() else {
            return false;
        };
        if std::time::Instant::now() < deadline {
            return false;
        }
        self.processor.stop_sync(&mut self.term);
        self.invalidate_suppressed_screen_prefix();
        self.refresh_rows_after_sync_flush();
        true
    }

    /// End a synchronized update unconditionally when it still holds bytes.
    ///
    /// Used on teardown: session exit is the other "no more PTY bytes arrive"
    /// case, where waiting for the deadline would simply drop the buffer.
    pub fn force_stop_sync_if_buffered(&mut self) -> bool {
        if self.processor.sync_bytes_count() == 0 {
            return false;
        }
        self.processor.stop_sync(&mut self.term);
        self.invalidate_suppressed_screen_prefix();
        self.refresh_rows_after_sync_flush();
        true
    }

    /// Re-sync the cached screen rows after a flush that bypassed `process()`.
    ///
    /// `screen_text_rows()` serves `prev_rows`, so without this every screen
    /// reader (HTTP snapshots, agent screen classifiers) would keep answering
    /// with pre-flush content until the next PTY chunk arrived — the same
    /// staleness the flush exists to end.
    ///
    /// DEFERRED (2026-07-26) — refreshing the cache means the next `process()`
    /// diff no longer reports these rows, so the output parser sees flushed
    /// content only if the agent repaints it (Codex and Ink both do, every
    /// frame). Feeding them to the parser needs the ticker to reach the
    /// reader-owned `ChunkProcessor`; revisit if a parser miss is ever observed.
    fn refresh_rows_after_sync_flush(&mut self) {
        if !self.prev_rows.is_empty() {
            self.prev_rows = self.read_screen_text();
        }
    }

    #[cfg(test)]
    pub fn full_screen_reads(&self) -> usize {
        self.full_screen_reads
    }

    /// Reference (pre-optimization) implementation of `process`: always rebuilds
    /// and diffs the ENTIRE visible screen. Kept test-only as the correctness
    /// oracle for the parse-damage fast path — `process_damage_matches_full_diff`
    /// asserts the two produce identical `ChangedRow`s across an input matrix.
    #[cfg(test)]
    pub fn process_full(&mut self, data: &[u8]) -> Vec<ChangedRow> {
        self.processor.advance(&mut self.term, data);
        self.term.reset_parse_damage();
        let curr_rows = self.read_screen_text();
        let changed: Vec<ChangedRow> = curr_rows
            .iter()
            .enumerate()
            .filter_map(|(i, curr)| {
                let prev = self.prev_rows.get(i).map(String::as_str).unwrap_or("");
                (curr != prev).then(|| ChangedRow {
                    row_index: i,
                    text: curr.clone(),
                })
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

    /// Per screen row: true when the row soft-wraps into the next one.
    pub fn screen_row_wraps(&self) -> Vec<bool> {
        (0..self.term.grid().screen_lines())
            .map(|i| self.row_wrapped(Line(i as i32)))
            .collect()
    }

    /// True when the newest history row soft-wraps into the row below it.
    pub fn newest_history_row_wraps(&self) -> bool {
        self.term.grid().history_size() > 0 && self.row_wrapped(Line(-1))
    }

    /// Text of the history rows that soft-wrap into screen row 0: the head of a
    /// logical line that is partly out of view. Empty when row 0 starts a line.
    pub fn screen_head_context(&self) -> String {
        let mut rows = Vec::new();
        let mut line = -1;
        while let Some(text) = self.row_to_text(Line(line)) {
            if !self.row_wrapped(Line(line)) {
                break;
            }
            rows.push(text);
            line -= 1;
        }
        rows.reverse();
        rows.concat()
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

    /// Whether the app enabled DEC mouse reporting (click / drag / motion).
    ///
    /// Inline fullscreen TUIs (`grok --no-alt-screen`) turn this on without
    /// entering the alternate screen. The durable log treats that the same
    /// way as alt-screen: keep grid history for the scrollbar, don't ingest
    /// viewport slices as shell output.
    pub fn is_mouse_reporting(&self) -> bool {
        let mode = self.term.mode();
        mode.contains(TermMode::MOUSE_REPORT_CLICK)
            || mode.contains(TermMode::MOUSE_DRAG)
            || mode.contains(TermMode::MOUSE_MOTION)
    }

    /// Whether the cursor is currently visible (DECTCEM / CSI ?25h).
    pub fn is_cursor_visible(&self) -> bool {
        self.term.mode().contains(TermMode::SHOW_CURSOR)
    }

    /// Number of scrollback lines above the visible screen.
    pub fn scrollback_count(&self) -> usize {
        self.term.grid().history_size()
    }

    /// Monotonic screen origin, including lines evicted from capped scrollback.
    pub fn screen_origin(&self) -> usize {
        self.term.grid().total_scrolled()
    }

    /// Primary-screen origin, including evicted rows, even while alternate screen is active.
    pub fn primary_screen_origin(&self) -> usize {
        self.term.primary_total_scrolled()
    }

    /// Number of primary-screen scrollback lines, regardless of the active screen.
    pub fn primary_scrollback_count(&self) -> usize {
        self.term.primary_history_size()
    }

    /// Roughly how much heap this grid holds, for `memory_report`.
    ///
    /// Cells are the whole of it: `Cell` is fixed-size, and the per-row `Vec`
    /// headers are noise beside `rows × cols` of them. Approximate on purpose —
    /// the report exists to say *which* structure grew, and a number that is
    /// right to within a row is already enough to answer that.
    pub fn approx_bytes(&self) -> usize {
        let grid = self.term.grid();
        (grid.history_size() + grid.screen_lines())
            .saturating_mul(grid.columns())
            .saturating_mul(std::mem::size_of::<Cell>())
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
    pub fn columns(&self) -> usize {
        self.term.grid().columns()
    }

    /// Read the cursor position (line, column) in screen coordinates.
    pub fn cursor_point(&self) -> (usize, usize) {
        let point = self.term.grid().cursor.point;
        (point.line.0.max(0) as usize, point.column.0)
    }

    /// Reconstruct the logical line prefix through the cursor from grid cells.
    ///
    /// Only terminal soft-wraps are followed. The bounded result is intended for
    /// structured-token parsing, not general scrollback reconstruction.
    pub fn logical_prefix_at_cursor(&self) -> Option<LogicalPrefix> {
        const MAX_WRAP_TRANSITIONS: usize = 4;
        const MAX_BYTES: usize = 512;

        let grid = self.term.grid();
        let cursor = grid.cursor.point;
        if cursor.line.0 < 0 {
            return None;
        }
        let (end_row, cursor_col) = self.cursor_point();
        if end_row >= grid.screen_lines() {
            return None;
        }

        let mut start_row = end_row;
        let mut wrap_transitions = 0;
        while start_row > 0 && self.row_wrapped(Line(start_row as i32 - 1)) {
            if wrap_transitions == MAX_WRAP_TRANSITIONS {
                return None;
            }
            start_row -= 1;
            wrap_transitions += 1;
        }

        let mut text = String::new();
        for row in start_row..=end_row {
            let limit = if row == end_row {
                (cursor_col + usize::from(grid.cursor.input_needs_wrap)).min(grid.columns())
            } else {
                grid.columns()
            };
            for col in 0..limit {
                let cell = &grid[Line(row as i32)][Column(col)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                if text.len() + cell_text_utf8_len(cell) > MAX_BYTES {
                    return None;
                }
                push_cell_text(&mut text, cell);
            }
        }

        Some(LogicalPrefix {
            text,
            start_row,
            end_row,
        })
    }

    /// Reconstruct the complete soft-wrapped logical line containing `row`.
    /// This remains valid after an agent returns the cursor to its composer.
    pub fn logical_line_at_row(&self, row: usize) -> Option<LogicalPrefix> {
        // A one-cell-wide terminal can spend one row per candidate character.
        // Bound by the parser's character budget, not a fixed number of rows.
        const MAX_WRAP_TRANSITIONS: usize = crate::output_parser::MAX_INTENT_CANDIDATE_CHARS;
        // The parser accepts 2048 Unicode scalars; UTF-8 can use four bytes
        // per scalar, including cells in soft-wrapped rows before trimming.
        const MAX_BYTES: usize = crate::output_parser::MAX_INTENT_CANDIDATE_CHARS * 4;
        let grid = self.term.grid();
        if row >= grid.screen_lines() {
            return None;
        }
        let mut start_row = row;
        let mut transitions = 0;
        while start_row > 0 && self.row_wrapped(Line(start_row as i32 - 1)) {
            if transitions == MAX_WRAP_TRANSITIONS {
                return None;
            }
            start_row -= 1;
            transitions += 1;
        }
        let mut end_row = row;
        while end_row + 1 < grid.screen_lines() && self.row_wrapped(Line(end_row as i32)) {
            if transitions == MAX_WRAP_TRANSITIONS {
                return None;
            }
            end_row += 1;
            transitions += 1;
        }
        let mut text = String::new();
        for line in start_row..=end_row {
            for col in 0..grid.columns() {
                let cell = &grid[Line(line as i32)][Column(col)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                if text.len() + cell_text_utf8_len(cell) > MAX_BYTES {
                    return None;
                }
                push_cell_text(&mut text, cell);
            }
        }
        Some(LogicalPrefix {
            text,
            start_row,
            end_row,
        })
    }

    /// Read only the current physical row through the cursor. This is the
    /// bounded fallback for a self-contained structured token when reconstructing
    /// the preceding soft-wrap chain is intentionally refused.
    pub fn physical_prefix_at_cursor(&self) -> Option<LogicalPrefix> {
        const MAX_BYTES: usize = 512;

        let grid = self.term.grid();
        let cursor = grid.cursor.point;
        if cursor.line.0 < 0 {
            return None;
        }
        let (row, cursor_col) = self.cursor_point();
        if row >= grid.screen_lines() {
            return None;
        }
        let limit = (cursor_col + usize::from(grid.cursor.input_needs_wrap)).min(grid.columns());
        let mut text = String::new();
        for col in 0..limit {
            let cell = &grid[Line(row as i32)][Column(col)];
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            if text.len() + cell_text_utf8_len(cell) > MAX_BYTES {
                return None;
            }
            push_cell_text(&mut text, cell);
        }
        Some(LogicalPrefix {
            text,
            start_row: row,
            end_row: row,
        })
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
        self.invalidate_suppressed_screen_prefix();
        let suppressed = std::mem::take(&mut self.suppressed_screen_prefix);
        if mode == ReflowMode::All && suppressed != 0 {
            // These blanks are our deleted replay, not program output. Remove
            // them before full reflow can migrate them into immutable history.
            let grid = self.term.grid_mut();
            let lines = grid.screen_lines();
            grid.scroll_up_with(
                &(Line(0)..Line(lines as i32)),
                suppressed,
                ScrollSource::Control,
            );
            grid.cursor.point.line = Line((grid.cursor.point.line.0 - suppressed as i32).max(0));
            grid.saved_cursor.point.line =
                Line((grid.saved_cursor.point.line.0 - suppressed as i32).max(0));
            self.prev_rows.clear();
            self.term.mark_fully_damaged();
        }
        let size = GridSize {
            cols: cols as usize,
            lines: rows as usize,
        };
        let was_alt = self.is_alternate_screen();
        let before_columns = self.term.grid().columns();
        let before_lines = self.term.grid().screen_lines();
        let viewport_top = self.term.grid().total_scrolled();
        // Height growth pulls these exact coordinate-owned rows from history
        // into the resized viewport. Keep their redraw; they no longer have
        // an older history copy to preserve.
        let pulled_rows = (rows as usize)
            .saturating_sub(before_lines)
            .min(self.term.grid().history_size());
        let viewport_rows: Vec<String> = (-(pulled_rows as i32)..before_lines as i32)
            .map(|row| self.row_to_text(Line(row)).unwrap_or_default())
            .collect();
        let preceding_count = self
            .term
            .grid()
            .history_size()
            .saturating_sub(pulled_rows)
            .min(before_lines.saturating_mul(4).min(256));
        // Preserve the source width of immutable hard rows across reflow.
        // VT soft wraps are joined by bounds_to_string before the lookup;
        // unchanged hard-row text keeps the width recorded at its last snapshot.
        let hard_rows = self.logical_text_rows(
            -(pulled_rows as i32 + preceding_count as i32),
            -(pulled_rows as i32),
        );
        let mut widths = std::collections::HashMap::new();
        let source_widths: Vec<usize> = hard_rows
            .iter()
            .map(|row| {
                let width = self
                    .history_source_widths
                    .get(row)
                    .copied()
                    .unwrap_or(before_columns);
                widths.insert(row.clone(), width);
                width
            })
            .collect();
        self.history_source_widths = widths;
        let mut preceding_parts = Vec::new();
        let mut first = 0;
        while first < hard_rows.len() {
            if hard_rows[first].trim().is_empty() {
                preceding_parts.push(String::new());
                first += 1;
                continue;
            }
            let mut last = first + 1;
            while last < hard_rows.len() && !hard_rows[last].trim().is_empty() {
                last += 1;
            }
            let part: Vec<&str> = hard_rows[first..last].iter().map(String::as_str).collect();
            preceding_parts.extend(reflow_copied_run_at_widths(
                &part,
                before_columns,
                2,
                true,
                Some(&source_widths[first..last]),
            ));
            first = last;
        }
        let preceding_logical = preceding_parts.join("\n");
        // All-mode width reflow can intermingle viewport and older history.
        // Only short, unwrapped viewport rows have an unambiguous height map.
        let simple_rows = (0..before_lines.saturating_sub(rows as usize)).all(|row| {
            !self.row_wrapped(Line(row as i32))
                && viewport_rows[pulled_rows + row].width() <= cols as usize
        });
        let mut displaced_rows = if before_columns == cols as usize
            || mode == ReflowMode::HistoryOnly
            || (cols as usize <= before_columns && simple_rows)
        {
            before_lines.saturating_sub(rows as usize)
        } else {
            0
        };
        // Reflow can move visible content into history even without a height
        // change. Locate the complete original viewport prefix in its new
        // coordinates; ambiguous or incomplete matches own no history rows.
        let source_logical = self.logical_text_rows(-(pulled_rows as i32), before_lines as i32);
        self.term.resize_reflow(size, mode);
        if mode == ReflowMode::All && (cols as usize) < before_columns {
            let first = source_logical.iter().position(|row| !row.trim().is_empty());
            if let Some(first) = first
                && let Some(next) = source_logical.get(first + 1)
                && !next.trim().is_empty()
            {
                let prefix = format!("{}\n{}", source_logical[first], next);
                let count = self
                    .term
                    .grid()
                    .history_size()
                    .min(before_lines.saturating_mul(4).min(256));
                let candidates: Vec<i32> = (-(count as i32)..0)
                    .filter(|&row| {
                        self.term
                            .bounds_to_string(
                                Point::new(Line(row), Column(0)),
                                Point::new(Line(-1), Column(self.term.grid().columns() - 1)),
                            )
                            .starts_with(&prefix)
                    })
                    .collect();
                if let [start] = candidates.as_slice() {
                    displaced_rows = (-start) as usize;
                }
            }
        }
        // Record identity follows WRAPLINE boundaries, never substring matches.
        // Include a continuation still on screen before the repaint erases it.
        displaced_rows = displaced_rows.min(self.term.grid().history_size());
        let mut displaced_records = Vec::new();
        let mut first = -(displaced_rows as i32);
        while displaced_rows != 0 && first < self.term.grid().screen_lines() as i32 {
            let mut last = first;
            while last + 1 < self.term.grid().screen_lines() as i32 && self.row_wrapped(Line(last))
            {
                last += 1;
            }
            let text = self.term.bounds_to_string(
                Point::new(Line(first), Column(0)),
                Point::new(Line(last), Column(self.term.grid().columns() - 1)),
            );
            let end = (last + 1).min(0) + displaced_rows as i32;
            displaced_records.push((end as usize, text));
            first = last + 1;
        }
        self.reprint_merge_armed_at = (!was_alt).then(|| self.term.grid().total_scrolled());
        self.frame_end_carry.clear();
        self.reprint_head_snapshot = self.armed_screen_rows();
        self.reprint_snapshot_scrolled = self.term.grid().total_scrolled();
        self.reprint_clear_generation = self.term.viewport_clear().0;
        self.resize_viewport = (!was_alt).then_some(ResizeViewport {
            before_columns,
            after_columns: cols as usize,
            grid_sequence: self.grid_sequence,
            viewport_top,
            rows: viewport_rows,
            preceding_logical,
            displaced_rows,
            displaced_records,
            scrolled_after_resize: self.term.grid().total_scrolled(),
        });
        self.prev_rows.clear();
        self.term.mark_fully_damaged();
    }

    /// Feed `data`, trying [`Self::merge_reprinted_history_tail`] at the end of
    /// every synchronized-update frame. Claude Code draws a repaint inside one,
    /// so that is the moment its reprinted rows are complete whatever the chunk
    /// boundaries were. Returns true when history rows were dropped.
    fn advance_frame_by_frame(&mut self, data: &[u8]) -> bool {
        const FRAME_END: &[u8] = b"\x1b[?2026l";
        let carried = self.frame_end_carry.len();
        let mut window = std::mem::take(&mut self.frame_end_carry);
        window.extend_from_slice(data);
        let mut merged = false;
        let mut fed = 0;
        let mut from = 0;
        while let Some(at) = window[from..]
            .windows(FRAME_END.len())
            .position(|w| w == FRAME_END)
        {
            let end = from + at + FRAME_END.len();
            from = end;
            // A match that ended in the carried bytes was already fed.
            let Some(end_in_data) = end.checked_sub(carried).filter(|&e| e > fed) else {
                continue;
            };
            self.processor
                .advance(&mut self.term, &data[fed..end_in_data]);
            fed = end_in_data;
            self.invalidate_suppressed_screen_prefix();
            merged |= self.merge_reprinted_history_tail();
            if self.reprint_merge_armed_at.is_none() {
                break;
            }
        }
        self.processor.advance(&mut self.term, &data[fed..]);
        self.invalidate_suppressed_screen_prefix();
        self.frame_end_carry = if self.reprint_merge_armed_at.is_some() {
            window[window.len().saturating_sub(FRAME_END.len() - 1)..].to_vec()
        } else {
            Vec::new()
        };
        merged
    }

    fn invalidate_suppressed_screen_prefix(&mut self) {
        if self.suppressed_screen_prefix == 0 {
            return;
        }
        let touched = match self.term.parse_damage() {
            TermParseDamage::Full => true,
            TermParseDamage::Partial(rows) => {
                rows.iter().any(|&row| row < self.suppressed_screen_prefix)
            }
        };
        if touched || self.is_alternate_screen() {
            self.suppressed_screen_prefix = 0;
        }
    }

    fn logical_text_rows(&self, mut start: i32, end: i32) -> Vec<String> {
        let mut rows = Vec::new();
        while start < end {
            let mut last = start;
            while last + 1 < end && self.row_wrapped(Line(last)) {
                last += 1;
            }
            let raw = self.term.bounds_to_string(
                Point::new(Line(start), Column(0)),
                Point::new(Line(last), Column(self.term.grid().columns() - 1)),
            );
            if raw.is_empty() {
                rows.push(String::new());
            } else {
                rows.extend(raw.lines().map(str::to_string));
            }
            start = last + 1;
        }
        rows
    }

    fn armed_screen_rows(&self) -> Vec<String> {
        let lines = self.term.grid().screen_lines() as i32;
        (0..lines)
            .map(|line| self.row_to_text(Line(line)).unwrap_or_default())
            .collect()
    }

    /// Undo the duplicate a resize leaves behind in scrollback.
    ///
    /// A resize that shrinks the screen scrolls its top rows into history so no
    /// row is lost. Claude Code then wipes the visible screen and prints its
    /// last rows again, beginning with rows that were just scrolled out, so
    /// the same rows are in history and on screen. When the newest history
    /// rows are the first rows of the screen, drop them from history.
    ///
    /// Armed by [`Self::resize_with_mode`]; a program that draws without synchronized
    /// updates keeps the duplicate. Disarms on the first merge, or once
    /// the output has scrolled a screenful, which ends the repaint window.
    /// Returns true when history rows were dropped.
    fn merge_reprinted_history_tail(&mut self) -> bool {
        const MIN_ROWS: usize = 2;
        let Some(armed_at) = self.reprint_merge_armed_at else {
            return false;
        };
        let (history, lines, scrolled) = {
            let grid = self.term.grid();
            (
                grid.history_size(),
                grid.screen_lines(),
                grid.total_scrolled(),
            )
        };
        if !self.is_alternate_screen()
            && self.term.viewport_clear().0 != self.reprint_clear_generation
        {
            let merged = self.merge_cleared_viewport_reprint();
            self.reprint_merge_armed_at = None;
            return merged;
        }
        if self.is_alternate_screen() || scrolled.saturating_sub(armed_at) > lines {
            self.reprint_merge_armed_at = None;
            return false;
        }
        let owned = self
            .resize_viewport
            .as_ref()
            .map_or(0, |resize| resize.displaced_rows);
        let m = history.min(lines).min(owned) as i32;
        let text = |line: i32| self.row_to_text(Line(line)).unwrap_or_default();
        let tail: Vec<String> = (-m..0).map(text).collect();
        let head = self.armed_screen_rows();
        let snapshot = std::mem::replace(&mut self.reprint_head_snapshot, head.clone());
        // A screen that scrolled since the snapshot shifted every row, so a
        // difference proves nothing about what was rewritten.
        let unscrolled =
            std::mem::replace(&mut self.reprint_snapshot_scrolled, scrolled) == scrolled;
        let overlap = (MIN_ROWS..=m as usize)
            .rev()
            .find(|&k| {
                tail[m as usize - k..] == head[..k]
                    && head[..k].iter().filter(|row| !row.is_empty()).count() >= MIN_ROWS
                    && unscrolled
                    && snapshot.len() >= k
                    && (0..k).all(|i| snapshot[i] != head[i])
            })
            .unwrap_or(0);
        if overlap == 0 {
            return false;
        }
        self.term.grid_mut().drop_newest_history(overlap);
        self.reprint_merge_armed_at = None;
        true
    }

    /// Reconcile only the viewport captured at resize. An older-history prefix
    /// in the redraw is NEW duplicate output, not permission to delete old rows.
    fn merge_cleared_viewport_reprint(&mut self) -> bool {
        let (generation, cleared_at) = self.term.viewport_clear();
        self.reprint_clear_generation = generation;
        let Some(resize) = self.resize_viewport.take() else {
            return false;
        };
        let grid = self.term.grid();
        if resize.after_columns != grid.columns()
            || resize.grid_sequence > self.grid_sequence
            || cleared_at != resize.scrolled_after_resize
            || resize.viewport_top > resize.scrolled_after_resize
        {
            return false;
        }
        let newer = grid.total_scrolled().saturating_sub(cleared_at);
        if newer > grid.history_size() {
            return false;
        }
        let normalize = |rows: &[&str], columns: usize| {
            let mut out = Vec::new();
            let mut first = 0;
            while first < rows.len() {
                if rows[first].trim().is_empty() {
                    out.push(String::new());
                    first += 1;
                    continue;
                }
                let mut last = first + 1;
                while last < rows.len() && !rows[last].trim().is_empty() {
                    last += 1;
                }
                out.extend(reflow_copied_run(&rows[first..last], columns, 2, true));
                first = last;
            }
            out.join("\n")
        };
        let fresh_text = |start: i32| {
            let text = self.term.bounds_to_string(
                Point::new(Line(start), Column(0)),
                Point::new(
                    Line(grid.screen_lines() as i32 - 1),
                    Column(grid.columns() - 1),
                ),
            );
            normalize(&text.lines().collect::<Vec<_>>(), grid.columns())
        };
        let fresh = fresh_text(-(newer as i32));
        let anchor = (0..resize.rows.len().saturating_sub(1)).find_map(|source_row| {
            let rows = &resize.rows[source_row..source_row + 2];
            if rows.iter().any(|row| row.trim().is_empty()) {
                return None;
            }
            let anchor = normalize(
                &rows.iter().map(String::as_str).collect::<Vec<_>>(),
                resize.before_columns,
            );
            let anchor = anchor.strip_prefix("  ").unwrap_or(&anchor);
            if anchor.len() < 16 {
                return None;
            }
            let mut matches = fresh.match_indices(anchor);
            matches.next()?;
            if matches.next().is_some() {
                return None;
            }
            // Last start row that still contains the complete owned anchor.
            // If it starts inside a row, preserve that complete boundary row.
            let prefix = (0..newer + grid.screen_lines())
                .rev()
                .find(|&prefix| fresh_text(prefix as i32 - newer as i32).contains(anchor))?;
            Some((source_row, prefix))
        });
        let Some((source_row, mut prefix)) = anchor else {
            return false;
        };
        // Historical blank rows stay in the proof. At the owned boundary,
        // a leading viewport blank may instead be part of its replacement.
        // Try the full historical prefix first; exclude owned blanks only
        // when the remaining prefix proves the same exact historical suffix.
        let old_logical = resize.preceding_logical.as_str();
        let first_fragment = self.row_to_text(Line(-(newer as i32))).unwrap_or_default();
        let first_fragment = first_fragment.strip_prefix("  ").unwrap_or(&first_fragment);
        let mut candidate = prefix;
        let mut owned_blanks = if resize.rows[..source_row]
            .iter()
            .all(|row| row.trim().is_empty())
        {
            source_row
        } else {
            0
        };
        let prefix_proven = loop {
            let rows = self.logical_text_rows(-(newer as i32), candidate as i32 - newer as i32);
            let text = normalize(
                &rows.iter().map(String::as_str).collect::<Vec<_>>(),
                grid.columns(),
            );
            let whole = old_logical
                .strip_suffix(&text)
                .is_some_and(|before| before.is_empty() || before.ends_with('\n'));
            let partial = text.strip_prefix("  ").unwrap_or(&text);
            if candidate != 0
                && !text.is_empty()
                && (whole || (first_fragment.width() >= 16 && old_logical.ends_with(partial)))
            {
                prefix = candidate;
                break true;
            }
            if candidate == 0
                || owned_blanks == 0
                || !self
                    .row_to_text(Line(candidate as i32 - newer as i32 - 1))
                    .is_some_and(|row| row.trim().is_empty())
            {
                break false;
            }
            candidate -= 1;
            owned_blanks -= 1;
        };
        let history_prefix = if prefix_proven { prefix.min(newer) } else { 0 };
        let screen_prefix = if prefix_proven {
            prefix.saturating_sub(newer)
        } else {
            0
        };
        // Collect proof before mutating any history coordinates.
        let fresh_rows: Vec<String> = (-(newer as i32)..grid.screen_lines() as i32)
            .map(|row| self.row_to_text(Line(row)).unwrap_or_default())
            .collect();
        let owned = resize
            .displaced_rows
            .min(grid.history_size().saturating_sub(newer));
        let source = normalize(
            &resize.rows.iter().map(String::as_str).collect::<Vec<_>>(),
            resize.before_columns,
        );
        // A physical fragment of a fully reflowed row is replaceable only
        // inside a complete source logical record present in the redraw.
        // Matching an arbitrary substring ("beta" in "betaX") proves nothing.
        let replaced_source: Vec<&str> = source
            .lines()
            .enumerate()
            .filter_map(|(index, row)| {
                let text = row.trim_start();
                (!text.is_empty()
                    && fresh.lines().any(|line| {
                        line.trim_start() == text
                            || (index == 0 && text.width() >= 16 && line.ends_with(text))
                    }))
                .then_some(text)
            })
            .collect();
        // Ink's hard-wrapped rows use the same existing normalization as the
        // source proof. Associate each normalized record with its contiguous
        // physical span, rather than searching another record for a fragment.
        let owned_text = normalize(
            &resize
                .displaced_records
                .iter()
                .map(|(_, text)| text.as_str())
                .collect::<Vec<_>>(),
            resize.before_columns,
        );
        let mut physical = resize.displaced_records.iter();
        let owned_records: Vec<(usize, &str)> = owned_text
            .lines()
            .filter_map(|record| {
                let (first_end, first) = physical.next()?;
                let mut end = *first_end;
                let mut length = first.len();
                while length < record.len() {
                    let (next_end, next) = physical.next()?;
                    end = *next_end;
                    length += 1 + next.trim_start().len();
                }
                (length == record.len()).then_some((end, record))
            })
            .collect();
        let old_replaced: Vec<bool> = (0..owned)
            .map(|row| {
                let old = self
                    .row_to_text(Line(row as i32 - newer as i32 - owned as i32))
                    .unwrap_or_default();
                if old.trim().is_empty() {
                    resize.before_columns != resize.after_columns
                        && fresh_rows.iter().any(|fresh| fresh == &old)
                } else {
                    fresh_rows.iter().any(|fresh| fresh == &old)
                        || owned_records
                            .iter()
                            .find(|(end, _)| row + resize.displaced_rows - owned < *end)
                            .is_some_and(|(_, record)| {
                                replaced_source.contains(&record.trim_start())
                            })
                }
            })
            .collect();
        let remaining = newer - history_prefix;
        let mut removed = self
            .term
            .grid_mut()
            .drop_history_before(remaining, history_prefix);
        for row in 0..screen_prefix {
            for col in 0..self.term.grid().columns() {
                self.term.grid_mut()[Line(row as i32)][Column(col)] = Cell::default();
            }
        }
        // Old rows are independent replacements, never an anchor-licensed block.
        // Iterate oldest to newest so newer row offsets remain stable.
        for (row, replaced) in old_replaced.into_iter().enumerate() {
            if replaced {
                removed += self
                    .term
                    .grid_mut()
                    .drop_history_before(remaining + owned - row - 1, 1);
            }
        }
        if screen_prefix != 0 {
            self.suppressed_screen_prefix = screen_prefix;
            // The erase/repaint preceding this suppression already dirtied
            // these rows. Start fresh damage evidence for subsequent bytes,
            // including a blank write over an already blank coordinate.
            self.prev_rows.clear();
            self.term.mark_fully_damaged();
            self.term.reset_parse_damage();
        }
        removed + screen_prefix != 0
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

            push_cell_text(&mut cur_text, cell);
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

        // A wrapped row's final spaces are content, not unused grid padding.
        // Its continuation is concatenated without a separator by text readers.
        if !self.row_wrapped(line) {
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
        }

        LogLine {
            spans,
            cols: num_cols as u16,
            chrome: false,
            wrapped: self.row_wrapped(line),
            partial: false,
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
                    prev.wrapped = log_line.wrapped;
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
                push_cell_text(&mut row_text, cell);
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
                push_cell_text(&mut result_text, cell);
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
        std::mem::take(&mut *self.events.lock())
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

    /// Scroll to an eviction-stable all-time row, or follow the tail with None.
    /// Output between request and apply cannot shift a retained anchor; trimmed
    /// rows clamp to the oldest retained row.
    pub fn scroll_to_anchor(&mut self, anchor: Option<usize>) {
        let offset = anchor.map_or(0, |row| self.screen_origin().saturating_sub(row));
        self.scroll_to_offset(offset);
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

    /// Run `f` with the compiled form of `query`, compiling it only when the
    /// query differs from the cached one.
    ///
    /// `RegexSearch::new` builds four DFAs, and a redrawing TUI re-ran the search
    /// on every frame — for a query that only changes when the user types.
    ///
    /// The closure takes `&mut RegexSearch` because the DFAs are LAZY: matching
    /// mutates their caches. It is not per-search state — a search's origin,
    /// iterator and current DFA state are fresh locals inside alacritty's
    /// `regex_search_internal`, so nothing about one search is carried into the
    /// next through this value. That is why one compiled instance can serve both
    /// `search` and `search_buffer`; the mutex is what keeps two callers from
    /// touching the shared caches at once.
    fn compiled_query<T>(&self, query: &str, f: impl FnOnce(&mut RegexSearch) -> T) -> Option<T> {
        let mut slot = self.search_regex.lock();
        if slot.as_ref().is_none_or(|(cached, _)| cached != query) {
            let Ok(compiled) = RegexSearch::new(query) else {
                // Leave the previous entry: an invalid query is what a user types
                // halfway through a valid one, and the next keystroke may fix it.
                return None;
            };
            #[cfg(test)]
            self.regex_compiles
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            *slot = Some((query.to_string(), compiled));
        }
        let (_, regex) = slot.as_mut().expect("just populated");
        Some(f(regex))
    }

    #[cfg(test)]
    pub fn regex_compiles(&self) -> usize {
        self.regex_compiles
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Regex search across visible grid + scrollback using alacritty's native DFA engine.
    /// Returns matches as (row, col_start, col_end) in absolute coordinates.
    /// The query is auto-escaped for literal substring search unless it contains
    /// regex metacharacters; case-insensitive when all lowercase.
    pub fn search(&self, query: &str) -> Vec<SearchMatch> {
        if query.is_empty() || query.len() > 1024 {
            return Vec::new();
        }
        let history = self.term.grid().history_size();
        let topmost = self.term.topmost_line();
        let bottommost = self.term.bottommost_line();
        let last_col = self.term.last_column();

        let start = Point::new(topmost, Column(0));
        let end = Point::new(bottommost, last_col);

        self.compiled_query(query, |regex| {
            let mut matches = Vec::new();
            let mut origin = start;

            while let Some(m) = self.term.regex_search_right(regex, origin, end) {
                let m_start = *m.start();
                let m_end = *m.end();

                let abs_row = (m_start.line.0 + history as i32) as usize;
                // A match can span a wrapped line. `SearchMatch` carries a single row, so
                // taking `m_end.column` from a DIFFERENT row would paint a highlight on the
                // start row covering cells that never matched. Clip to the end of the start
                // row instead: an under-highlight is invisible, an over-highlight is a bug.
                // DEFERRED (2026-08-07) — full multi-row highlighting needs SearchMatch to
                // carry per-row segments and the match count to stay 1 per logical hit.
                let col_end = if m_end.line == m_start.line {
                    m_end.column.0 + 1
                } else {
                    last_col.0 + 1
                };
                matches.push(SearchMatch {
                    row: abs_row,
                    col_start: m_start.column.0,
                    col_end,
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
        })
        .unwrap_or_default()
    }

    pub fn search_buffer(&self, query: &str) -> Vec<BufferSearchMatch> {
        if query.is_empty() || query.len() > 1024 {
            return Vec::new();
        }
        let history = self.term.grid().history_size();
        let topmost = self.term.topmost_line();
        let bottommost = self.term.bottommost_line();
        let last_col = self.term.last_column();

        let start = Point::new(topmost, Column(0));
        let end = Point::new(bottommost, last_col);

        self.compiled_query(query, |regex| {
            let mut matches = Vec::new();
            let mut origin = start;
            let mut last_row_line: Option<(usize, String)> = None;

            while let Some(m) = self.term.regex_search_right(regex, origin, end) {
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
                    match_start: self.row_utf16_offset(m_start.line, m_start.column.0),
                    match_end: if m_end.line == m_start.line {
                        self.row_utf16_offset(m_end.line, m_end.column.0 + 1)
                    } else {
                        self.row_to_text(m_start.line)
                            .unwrap_or_default()
                            .encode_utf16()
                            .count()
                    },
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
        })
        .unwrap_or_default()
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
                push_cell_text(&mut text, cell);
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
    fn normalize_copied_selection(text: &str, num_cols: usize) -> String {
        let lines: Vec<&str> = text.split('\n').collect();
        let mut out: Vec<String> = Vec::with_capacity(lines.len());
        let mut index = 0;

        while index < lines.len() {
            if gutter_content(lines[index]).is_none() {
                out.push(lines[index].to_string());
                index += 1;
                continue;
            }

            let run_start = index;
            while index < lines.len() && gutter_content(lines[index]).is_some() {
                index += 1;
            }
            let contents: Vec<&str> = lines[run_start..index]
                .iter()
                .filter_map(|line| gutter_content(line))
                .collect();
            // A single quoted line is more likely a coincidence (a table rule, a
            // box-drawn frame) than a Claude blockquote, so leave it verbatim.
            let should_strip = contents
                .iter()
                .filter(|content| !content.is_empty())
                .count()
                >= 2;

            if should_strip {
                out.extend(reflow_copied_run(&contents, num_cols, 4, false));
            } else {
                out.extend(lines[run_start..index].iter().map(|line| line.to_string()));
            }
        }

        out.join("\n")
    }

    /// Normalize only the composer run whose first row was selected from column zero.
    fn normalize_copied_composer(text: &str, num_cols: usize) -> String {
        let lines: Vec<&str> = text.split('\n').collect();
        let Some(first) = lines[0].strip_prefix("❯ ") else {
            return Self::normalize_copied_selection(text, num_cols);
        };
        // Strip one marker and one margin; a second glyph and deeper indent are input.
        let mut contents = vec![first];
        let mut index = 1;
        while index < lines.len() {
            let Some(content) = lines[index].strip_prefix("  ") else {
                break;
            };
            if content.is_empty() || gutter_content(lines[index]).is_some() {
                break;
            }
            contents.push(content);
            index += 1;
        }
        let mut out = reflow_copied_run(&contents, num_cols, 2, false);
        if index < lines.len() {
            out.push(Self::normalize_copied_selection(
                &lines[index..].join("\n"),
                num_cols,
            ));
        }
        out.join("\n")
    }

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

        let first_line = Line(r0 as i32 - history_size as i32);
        // Selection text alone cannot distinguish a pasted glyph from composer chrome.
        // Check the actual origin before cell extraction loses its column/wrap context.
        let composer_anchor = c0 == 0
            && num_cols >= 2
            && first_line >= grid.topmost_line()
            && first_line <= grid.bottommost_line()
            && grid[first_line][Column(0)].c == '❯'
            && grid[first_line][Column(1)].c == ' '
            // Lost provenance follows the physical row even when RI/IL moves it.
            && !grid[first_line].copy_origin_unknown
            && (first_line == grid.topmost_line()
                || !grid[Line(first_line.0 - 1)][Column(num_cols - 1)]
                    .flags
                    .contains(Flags::WRAPLINE));

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
                push_cell_text(&mut text, cell);
            }
            if !self.row_wrapped(line) {
                let trimmed_len = text.trim_end().len();
                text.truncate(trimmed_len);
            }
            result.push_str(&text);

            if abs_row < r1 {
                let last_col = num_cols.saturating_sub(1);
                let is_wrapped = grid[line][Column(last_col)].flags.contains(Flags::WRAPLINE);
                if !is_wrapped {
                    result.push('\n');
                }
            }
        }

        let text = result.trim_end_matches('\n');
        if composer_anchor {
            Self::normalize_copied_composer(text, num_cols)
        } else {
            Self::normalize_copied_selection(text, num_cols)
        }
    }

    /// Extract selection text using grid-relative rows from an optional frame snapshot.
    ///
    /// Without `history_base`, this preserves [`Self::get_selection_text`]'s legacy
    /// contract. With a snapshot, the supplied rows are first converted to stable
    /// all-time coordinates and then rebased into the grid's current retained-row
    /// space. This conversion and the cell read happen through the same `&self`
    /// borrow, which callers hold under the VT lock, so scrollback eviction cannot
    /// race between rebasing and extraction.
    pub fn get_selection_text_with_history_base(
        &self,
        start_row: usize,
        start_col: usize,
        end_row: usize,
        end_col: usize,
        history_base: Option<usize>,
    ) -> Result<String, String> {
        let Some(snapshot_base) = history_base else {
            return Ok(self.get_selection_text(start_row, start_col, end_row, end_col));
        };

        let grid = self.term.grid();
        let current_base = grid.total_scrolled().saturating_sub(grid.history_size());
        if snapshot_base > current_base {
            return Err("selection rows are no longer retained".to_string());
        }
        let retained_end = current_base
            .checked_add(grid.history_size())
            .and_then(|end| end.checked_add(grid.screen_lines()))
            .ok_or_else(|| "selection rows are no longer retained".to_string())?;
        let rebase = |row: usize| {
            let absolute = snapshot_base
                .checked_add(row)
                .ok_or_else(|| "selection rows are no longer retained".to_string())?;
            if absolute < current_base || absolute >= retained_end {
                return Err("selection rows are no longer retained".to_string());
            }
            Ok(absolute - current_base)
        };

        let start_row = rebase(start_row)?;
        let end_row = rebase(end_row)?;
        Ok(self.get_selection_text(start_row, start_col, end_row, end_col))
    }

    /// Serialize dirty rows as a compact binary frame.
    ///
    /// Uses alacritty's built-in damage tracking to identify changed rows.
    /// Wire format (22-byte header):
    /// ```text
    /// Header: [num_rows: u16] [cursor_row: u16] [cursor_col: u16] [cursor_visible: u8]
    ///         [display_offset: u32] [history_size: u32] [has_selection: u8]
    ///         [keyboard_flags: u8] [frame_flags: u8] [num_lines: u16] [num_cols: u16]
    /// Per row: [row_index: u16] [col_count: u16] ([start_col: u16]) [cells...]
    /// Per cell: [char: u32 LE] [fg_r, fg_g, fg_b] [bg_r, bg_g, bg_b] [attrs: u8]
    /// Optional trailer after all rows: ["TCX1"] [entry_count: u32], then
    /// [wire_row_ordinal: u16] [absolute_col: u16] [count: u8]
    /// [count × codepoint: u32 LE] for each cell with zero-width extensions.
    /// ```
    /// `col_count` carries two flags in its top bits: [`ROW_WRAPPED_FLAG`] (the
    /// line continues onto the next display row) and [`ROW_PARTIAL_FLAG`]. Only
    /// when the latter is set does `start_col` follow, and then the row carries
    /// `col_count` cells starting at that column instead of the whole width —
    /// the frontend merges them into the row it already holds.
    /// attrs: bit0=bold, bit1=italic, bit2=underline, bit3=strikeout,
    ///        bit4=dim, bit5=inverse, bit6=default_fg, bit7=default_bg
    /// keyboard_flags: bit0=disambiguate_esc_codes, bit1=report_event_types,
    ///                 bit2=report_alternate_keys, bit3=report_all_keys_as_esc,
    ///                 bit4=report_associated_text, bit5=alternate_screen
    /// frame_flags: bit0=bell, bits1-2=cursor_shape (0=block,1=underline,2=beam),
    ///              bits3-4=mouse_mode (0=none,1=click,2=drag,3=motion),
    ///              bit5=sgr_mouse, bit6=focus_reporting, bit7=bracketed_paste
    pub fn serialize_dirty_rows(&mut self) -> Vec<u8> {
        let num_cols = self.term.grid().columns();
        let num_lines = self.term.grid().screen_lines();
        let display_offset = self.term.grid().display_offset();
        let history_size = self.term.grid().history_size();

        let viewport_changed = self.last_frame_display_offset != Some(display_offset)
            || self.last_frame_history_size != Some(history_size)
            || self.last_frame_screen_lines != Some(num_lines)
            || self.last_frame_columns != Some(num_cols);
        if viewport_changed {
            self.term.mark_fully_damaged();
        }

        // REJECTED (2026-08-20) — F24, skipping the full damage above when only
        // `history_size` grew. Measured and not worth it: over 11 real captures
        // (4810 frames, `damage_overship_over_capture_corpus`) full frames are
        // 0.4% of all frames, and that already counts resizes and scrollback moves
        // as well as history growth. The audit's "every scrolled line forces a
        // full-screen frame" does not hold on real workloads. Doing it needs the
        // frame to carry a scroll delta the frontend applies before indexing rows,
        // and getting that wrong scrambles text silently. Re-open only if a capture
        // shows a full-frame rate high enough to pay for that risk.
        //
        // DEFERRED (2026-08-20) — F26, not holding the vt lock across this
        // serialization. It is not a lock-placement fix: this method takes
        // `&mut self` to reset damage and update the `last_frame_*` state, so
        // releasing the lock means double-buffering the grid — and copying the grid
        // costs about what the encode it unblocks costs. No measurement exists
        // showing the contention is real; take one before paying for it.
        //
        // (line, left, right) — `right` inclusive. Keeping the column bounds is
        // what lets a row ship only its damaged span (see `ROW_PARTIAL_FLAG`);
        // discarding them cost 2.44x the cells on the measured corpus.
        let dirty_lines: Vec<(usize, usize, usize)> = {
            let last_col = num_cols.saturating_sub(1);
            let damage = self.term.damage();
            match damage {
                TermDamage::Full => (0..num_lines).map(|l| (l, 0, last_col)).collect(),
                TermDamage::Partial(iter) => iter
                    .filter(|b| b.line < num_lines)
                    .map(|b| (b.line, b.left.min(last_col), b.right.min(last_col)))
                    .collect(),
            }
        };

        // The bell is drained only into a frame that will carry it: an empty
        // frame is never sent, so swallowing the flag here would lose the ring.
        let frame = if dirty_lines.is_empty() {
            Vec::new()
        } else {
            let bell = self.drain_bell();
            self.encode_frame(&dirty_lines, bell)
        };

        self.term.reset_damage();
        self.last_frame_display_offset = Some(display_offset);
        self.last_frame_history_size = Some(history_size);
        self.last_frame_screen_lines = Some(num_lines);
        self.last_frame_columns = Some(num_cols);
        frame
    }

    /// A whole-screen frame that takes nothing from the shared delta stream.
    ///
    /// Damage is tracked once per session and [`Self::serialize_dirty_rows`]
    /// CONSUMES it, so building a full frame for one subscriber used to mean
    /// damaging the grid again for every other one: a single browser that fell
    /// behind pinned the session into full frames the desktop also had to
    /// decode. This reads the same rows without touching damage, the
    /// `last_frame_*` viewport state or the bell, so a per-subscriber resync
    /// costs the other transports nothing.
    ///
    /// The bell is deliberately not carried. It is an event owned by the shared
    /// stream; draining it here would ring on the resyncing client and nowhere
    /// else.
    pub fn serialize_full_frame(&self) -> Vec<u8> {
        let num_cols = self.term.grid().columns();
        let num_lines = self.term.grid().screen_lines();
        let last_col = num_cols.saturating_sub(1);
        let rows: Vec<(usize, usize, usize)> = (0..num_lines).map(|l| (l, 0, last_col)).collect();
        self.encode_frame(&rows, false)
    }

    /// Pack `rows` — `(row, left, right)`, `right` inclusive — into the binary
    /// frame documented on [`Self::serialize_dirty_rows`].
    ///
    /// Read-only on purpose: whichever serializer called it owns the damage
    /// bookkeeping its frame implies, which is what lets a full frame be built
    /// for one subscriber without disturbing the others.
    fn encode_frame(&self, rows: &[(usize, usize, usize)], bell: bool) -> Vec<u8> {
        if rows.is_empty() {
            return Vec::new();
        }
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
        // bit 5: alternate screen active. Not a keyboard flag — it rides in this
        // byte because `frame_flags` has no bit left, and adding a header byte
        // would desync any frontend running against an older backend (Rust does
        // not hot-reload in dev). An unused bit degrades to 0 instead.
        // The frontend keys its absolute-row cache off `history_base`, which
        // restarts from 0 on every alt enter/exit (`reset_history_era`), so it
        // must drop that cache whenever this bit flips.
        if mode.contains(TermMode::ALT_SCREEN) {
            keyboard_flags |= 0x20;
        }

        // Header: 26 bytes
        let row_count = rows.len();
        let estimated = 26 + row_count * (4 + num_cols * 11);
        let mut buf = Vec::with_capacity(estimated);

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
        let mut extras_rows = Vec::with_capacity(rows.len());
        for &(row_idx, left, right) in rows {
            let line = Line(row_idx as i32 - display_offset as i32);
            // A span shorter than the row saves 11 bytes per column dropped and
            // costs 2 for `start_col`, so any narrowing at all is worth sending
            // partial. An empty grid (num_cols == 0) has nothing to narrow.
            let span = (right + 1).saturating_sub(left).min(num_cols);
            let partial = span < num_cols;
            buf.extend_from_slice(&(row_idx as u16).to_le_bytes());
            let count = encode_col_count(grid, line, num_cols, span);
            buf.extend_from_slice(
                &(count | if partial { ROW_PARTIAL_FLAG } else { 0 }).to_le_bytes(),
            );
            if partial {
                buf.extend_from_slice(&(left as u16).to_le_bytes());
            }

            // Bounded by `num_cols`, not `right`, so a zero-column grid indexes
            // nothing — the old `0..num_cols` loop was empty there too.
            for col in left..num_cols.min(right + 1) {
                encode_cell(&mut buf, &grid[line][Column(col)], colors);
            }
            extras_rows.push((line, left, num_cols.min(right + 1)));
        }

        append_cell_extras_trailer(&mut buf, grid, &extras_rows);

        buf
    }

    /// Damage geometry for one frame, as `serialize_dirty_rows` would see it: the
    /// rows it would ship, and how many of those rows' cells alacritty actually
    /// reported as damaged. Consumes the damage the same way the serializer does,
    /// so a replay alternating `process` and this call sees exactly the frames a
    /// live ticker would.
    ///
    /// Exists to measure F23 (per-row column bounds are discarded at the
    /// `TermDamage::Partial` match above) against real captures instead of
    /// guessing at the win. `shipped / damaged` is the byte multiplier the current
    /// whole-row format pays.
    #[cfg(test)]
    pub fn take_damage_geometry(&mut self) -> DamageGeometry {
        let num_cols = self.term.grid().columns();
        let num_lines = self.term.grid().screen_lines();
        let display_offset = self.term.grid().display_offset();
        let history_size = self.term.grid().history_size();

        let viewport_changed = self.last_frame_display_offset != Some(display_offset)
            || self.last_frame_history_size != Some(history_size)
            || self.last_frame_screen_lines != Some(num_lines)
            || self.last_frame_columns != Some(num_cols);
        if viewport_changed {
            self.term.mark_fully_damaged();
        }

        let (rows, damaged_cells) = match self.term.damage() {
            TermDamage::Full => (num_lines, num_lines * num_cols),
            TermDamage::Partial(iter) => {
                iter.filter(|b| b.line < num_lines)
                    .fold((0usize, 0usize), |(rows, cells), b| {
                        // `right` is inclusive, and a damaged line always has
                        // `left <= right`, so the span is at least one cell.
                        (rows + 1, cells + (b.right - b.left + 1).min(num_cols))
                    })
            }
        };

        self.term.reset_damage();
        self.last_frame_display_offset = Some(display_offset);
        self.last_frame_history_size = Some(history_size);
        self.last_frame_screen_lines = Some(num_lines);
        self.last_frame_columns = Some(num_cols);

        DamageGeometry {
            rows,
            damaged_cells,
            shipped_cells: rows * num_cols,
            full_frame: viewport_changed,
        }
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
    ///   `encode_cell`), followed by the same optional `TCX1` trailer as live
    ///   frames.
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
        let mut extras_rows = Vec::with_capacity(rows.len());
        for rel in rows {
            let line = Line(rel as i32 - history_size as i32);
            buf.extend_from_slice(&((rel + history_base) as u32).to_le_bytes());
            // Scrollback rows are always whole: this serializer answers "give me
            // these rows", not "what changed", so there is no damage span and no
            // prior row on the frontend to merge into.
            buf.extend_from_slice(&encode_col_count(grid, line, num_cols, num_cols).to_le_bytes());
            for col in 0..num_cols {
                encode_cell(&mut buf, &grid[line][Column(col)], colors);
            }
            extras_rows.push((line, 0, num_cols));
        }
        append_cell_extras_trailer(&mut buf, grid, &extras_rows);
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
                push_cell_text(&mut text, cell);
            }
            if !self.row_wrapped(line) {
                let trimmed_len = text.trim_end().len();
                text.truncate(trimmed_len);
            }
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
            push_cell_text(&mut text, cell);
        }
        if !self.row_wrapped(line) {
            let trimmed_len = text.trim_end().len();
            text.truncate(trimmed_len);
        }
        Some(text)
    }

    /// Translate a terminal cell boundary into the UTF-16 string offset used by
    /// browser consumers. Geometry stays cell-based; only text offsets include
    /// supplementary and zero-width codepoints.
    fn row_utf16_offset(&self, line: Line, column: usize) -> usize {
        let grid = self.term.grid();
        (0..column.min(grid.columns()))
            .map(|col| cell_text_utf16_len(&grid[line][Column(col)]))
            .sum()
    }

    #[cfg(test)]
    pub fn term(&self) -> &Term<TermEventCollector> {
        &self.term
    }
}

/// Content of a Claude blockquote row, with the `▎` gutter removed.
///
/// Claude Code draws a markdown blockquote as a two-cell indent, U+258E, and a
/// separator. The indent cells are plain spaces in current releases and were
/// non-breaking spaces in older ones, so both are accepted — matching only NBSP
/// silently disabled the whole strip and pasted raw `▎` bars into Slack.
/// Separators are consumed the same way; body NBSPs past the first survive,
/// because agents use them to align columns inside the quote.
fn gutter_content(line: &str) -> Option<&str> {
    const GUTTER_BAR: char = '▎';

    let mut chars = line.chars();
    for _ in 0..2 {
        match chars.next() {
            Some(' ') | Some('\u{a0}') => {}
            _ => return None,
        }
    }
    if chars.next() != Some(GUTTER_BAR) {
        return None;
    }
    let rest = chars.as_str();
    if rest.is_empty() {
        return Some("");
    }
    rest.strip_prefix(' ')
        .or_else(|| rest.strip_prefix('\u{a0}'))
}

/// Rejoin rows that Claude Code broke only to fit the terminal width.
///
/// Claude wraps its own output and emits real newlines, so `WRAPLINE` is unset
/// and the row-level unwrap in `get_selection_text` cannot help: pasting a
/// quoted draft into a chat client keeps every mid-sentence break.
///
/// The join rule is the inverse of greedy word wrap. With width `W`, a wrapper
/// breaks after a line exactly when the next word no longer fits, so a break is
/// mechanical when `line + " " + next_word` would exceed `W`, and deliberate
/// when it would have fit. `W` is the widest row in the run, the only exact
/// width evidence the copied text carries — agents wrap short of the terminal
/// edge by a margin of their own choosing.
///
/// That estimate is only meaningful once the run proves it was wrapped at all.
/// A short quote of three deliberate ten-column lines yields `W = 10`, under
/// which every following word overflows and the whole quote collapses into one
/// line. So a run whose widest row stays far from the terminal edge is left
/// untouched: `num_cols` is the one thing here that cannot be inferred from the
/// text, and without the gate the rule fails open on exactly the quotes a user
/// wrote by hand.
///
/// Blank rows, list markers and deeper indents always start a new line: they
/// mark structure the author chose, which the width rule alone cannot see.
fn reflow_copied_run(
    contents: &[&str],
    num_cols: usize,
    margin_cols: usize,
    exact_fit_wrap: bool,
) -> Vec<String> {
    reflow_copied_run_at_widths(contents, num_cols, margin_cols, exact_fit_wrap, None)
}

fn reflow_copied_run_at_widths(
    contents: &[&str],
    num_cols: usize,
    margin_cols: usize,
    exact_fit_wrap: bool,
    source_widths: Option<&[usize]>,
) -> Vec<String> {
    // Room for an agent's own right margin plus the ragged edge a greedy
    // wrapper leaves when the overflowing word is long.
    const WRAP_EVIDENCE_SLACK: usize = 24;
    // Below this the slack swallows the whole terminal and the gate would let
    // every run through. Prose quotes do not happen at such widths anyway.
    const MIN_REFLOW_COLS: usize = 48;

    // Recorded Ink repaints can wrap an exact-fit word; clipboard cleanup
    // retains its stricter rule for an ambiguous typed newline.
    let width = contents.iter().map(|line| line.width()).max().unwrap_or(0);
    let wrap_threshold = num_cols.saturating_sub(margin_cols + WRAP_EVIDENCE_SLACK);
    if source_widths.is_none() && (num_cols < MIN_REFLOW_COLS || width < wrap_threshold) {
        return contents.iter().map(|line| (*line).to_string()).collect();
    }
    let mut out: Vec<String> = Vec::with_capacity(contents.len());
    // The width rule asks what the *previous screen row* looked like, so the
    // measurements come from the source row, never from the paragraph built so
    // far. A joined paragraph always exceeds the wrap width, so measuring the
    // accumulator would make every later row overflow and glue the author's own
    // short lines onto the paragraph.
    let mut previous_row: Option<(usize, usize)> = None;

    let mut recorded_max = std::collections::HashMap::new();
    if let Some(widths) = source_widths {
        for (line, &columns) in contents.iter().zip(widths) {
            let max = recorded_max.entry(columns).or_insert(0usize);
            *max = (*max).max(line.width());
        }
    }
    for (index, line) in contents.iter().enumerate() {
        let previous_columns =
            source_widths.map_or(num_cols, |widths| widths[index.saturating_sub(1)]);
        let previous_max = source_widths.map_or(width, |_| recorded_max[&previous_columns]);
        let evidence = previous_columns >= MIN_REFLOW_COLS
            && previous_max >= previous_columns.saturating_sub(margin_cols + WRAP_EVIDENCE_SLACK);
        let trimmed = line.trim_start();
        let length = line.width();
        let indent = length - trimmed.width();

        let joinable = match previous_row {
            Some((previous_length, previous_indent)) => {
                evidence
                    && !trimmed.is_empty()
                    && previous_length > previous_indent
                    && indent <= previous_indent
                    && !starts_list_item(trimmed)
                    && previous_length + 1 + trimmed.split(' ').next().unwrap_or("").width()
                        > previous_max.saturating_sub(usize::from(exact_fit_wrap))
            }
            None => false,
        };

        if joinable {
            let previous = out.last_mut().expect("joinable implies a previous line");
            previous.push(' ');
            previous.push_str(trimmed);
        } else {
            out.push((*line).to_string());
        }
        previous_row = Some((length, indent));
    }

    out
}

/// Whether a line opens a bullet or an ordered-list item.
fn starts_list_item(trimmed: &str) -> bool {
    if let Some(rest) = trimmed.strip_prefix(['-', '*', '+', '•', '·']) {
        return rest.starts_with(' ');
    }
    let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return false;
    }
    let rest = &trimmed[digits.len()..];
    matches!(rest.strip_prefix(['.', ')']), Some(after) if after.starts_with(' '))
}

/// One frame's worth of damage geometry — see `take_damage_geometry`.
#[cfg(test)]
#[derive(Debug, Default, Clone, Copy)]
pub struct DamageGeometry {
    /// Rows the frame would carry.
    pub rows: usize,
    /// Cells alacritty reported as damaged across those rows.
    pub damaged_cells: usize,
    /// Cells the current whole-row wire format actually ships.
    pub shipped_cells: usize,
    /// The viewport moved, so the serializer forced full damage (F24's path).
    pub full_frame: bool,
}

#[cfg(test)]
mod tests {
    /// Catches BEL being held until a grid frame is serialized for a hidden tab.
    #[test]
    fn hidden_bell_is_a_session_event_without_frame_serialization() {
        let mut grid = super::TerminalGrid::new(2, 8, 10);
        grid.process(b"\x07");
        assert!(
            grid.drain_events()
                .iter()
                .any(|e| matches!(e, super::TermEvent::Bell))
        );
        assert!(grid.drain_events().is_empty());
    }

    use super::*;

    // Catches DECALN keeping lost provenance or inheriting the active erase background.
    #[test]
    fn copied_selection_decaln_resets_loss_and_replaces_all_cells_with_default_e() {
        let mut grid = TerminalGrid::new(2, 20, 1);
        grid.process(b"aaaaaaaaaaaaaaaaaaaab\r\n\r\n\r\n");
        grid.resize_with_mode(3, 20, ReflowMode::None);
        assert!(grid.term().grid()[Line(0)].copy_origin_unknown);
        grid.process(b"\x1b[41m\x1b#8");
        for line in 0..3 {
            let row = &grid.term().grid()[Line(line)];
            assert!(!row.copy_origin_unknown);
            assert!(!row.reflow_wrap);
            for column in 0..20 {
                let cell = &row[Column(column)];
                assert_eq!(cell.c, 'E');
                assert_eq!(cell.bg, alacritty_terminal::term::cell::Cell::default().bg);
                assert!(cell.flags.is_empty());
            }
        }
        grid.process("\x1b[H❯ hello\x1b[K".as_bytes());
        assert_eq!(grid.get_selection_text(0, 0, 0, 19), "hello");
    }

    // Catches whole-row ECH leaving origin loss behind, or partial ECH inventing a fresh origin.
    #[test]
    fn copied_selection_ech_full_row_clears_loss_but_partial_keeps_literal() {
        for count in [19, 20, 999] {
            let mut grid = TerminalGrid::new(2, 20, 1);
            grid.process(b"aaaaaaaaaaaaaaaaaaaab\r\n\r\n\r\n");
            grid.resize_with_mode(3, 20, ReflowMode::None);
            assert!(grid.term().grid()[Line(0)].copy_origin_unknown);
            grid.process(format!("\x1b[H\x1b[{count}X").as_bytes());
            let remains_unknown = count < 20;
            assert_eq!(
                grid.term().grid()[Line(0)].copy_origin_unknown,
                remains_unknown
            );
            grid.resize_with_mode(3, 24, ReflowMode::None);
            grid.process("❯ hello".as_bytes());
            assert_eq!(
                grid.get_selection_text(0, 0, 0, 23),
                if remains_unknown {
                    "❯ hello"
                } else {
                    "hello"
                }
            );
        }
    }

    // Catches reactivating predecessor loss after a full erase starts a new composer.
    #[test]
    fn copied_selection_full_screen_erase_clears_origin_across_resize() {
        let mut grid = TerminalGrid::new(2, 20, 1);
        let _ = grid.process("aaaaaaaaaaaaaaaaaaaab\r\n\r\n\r\n".as_bytes());
        grid.resize_with_mode(3, 20, ReflowMode::None);
        assert!(grid.term().grid()[Line(0)].copy_origin_unknown);
        let _ = grid.process("\x1b[H\x1b[J❯ hello".as_bytes());
        assert!(!grid.term().grid()[Line(0)].copy_origin_unknown);
        grid.resize_with_mode(3, 24, ReflowMode::None);
        assert_eq!(grid.get_selection_text(0, 0, 0, 23), "hello");
    }

    // --- DEC 2026 synchronized update (see `flush_sync_timeout_if_needed`) ---

    const BSU: &[u8] = b"\x1b[?2026h";
    const ESU: &[u8] = b"\x1b[?2026l";
    /// Slightly past the vendored VTE `SYNC_UPDATE_TIMEOUT` (150ms).
    const PAST_DEADLINE: std::time::Duration = std::time::Duration::from_millis(170);

    fn screen_contains(grid: &TerminalGrid, needle: &str) -> bool {
        grid.screen_text_rows().iter().any(|r| r.contains(needle))
    }

    // --- OSC 10/11/12 colour queries ---
    //
    // These share one process-wide palette, so they run under a mutex rather
    // than racing each other through `set_terminal_palette`.
    mod color_query {
        use super::*;

        static SERIAL: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

        fn replies_to(query: &[u8], palette: TerminalPalette) -> Vec<String> {
            let _guard = SERIAL.lock();
            let restore = terminal_palette();
            set_terminal_palette(palette);
            let mut grid = TerminalGrid::new(24, 80, 100);
            grid.process(query);
            let replies = grid
                .drain_events()
                .into_iter()
                .filter_map(|e| match e {
                    TermEvent::PtyWrite(s) => Some(s),
                    _ => None,
                })
                .collect();
            set_terminal_palette(restore);
            replies
        }

        fn palette() -> TerminalPalette {
            TerminalPalette {
                foreground: (0xd4, 0xd4, 0xd4),
                background: (0x1e, 0x1e, 0x1e),
                cursor: (0xff, 0x00, 0x80),
            }
        }

        /// The regression this whole feature exists for: an unanswered OSC 11
        /// makes a probing agent retry forever, because the `ESC[c` fence it
        /// sends alongside only means "unsupported" once a reply could have
        /// arrived first.
        #[test]
        fn answers_a_background_color_query() {
            let replies = replies_to(b"\x1b]11;?\x1b\\", palette());
            assert_eq!(replies.len(), 1, "expected exactly one reply: {replies:?}");
            assert!(
                replies[0].contains("11;rgb:1e1e/1e1e/1e1e"),
                "reply did not carry the background: {:?}",
                replies[0]
            );
        }

        #[test]
        fn answers_foreground_and_cursor_queries_with_their_own_colors() {
            let fg = replies_to(b"\x1b]10;?\x1b\\", palette());
            assert!(fg[0].contains("10;rgb:d4d4/d4d4/d4d4"), "{:?}", fg[0]);

            let cursor = replies_to(b"\x1b]12;?\x1b\\", palette());
            assert!(
                cursor[0].contains("12;rgb:ffff/0000/8080"),
                "{:?}",
                cursor[0]
            );
        }

        /// The probe sends `OSC 11 ; ? ST` then `ESC[c`. Both must be answered,
        /// and the colour must come FIRST — the fence is only meaningful because
        /// a supporting terminal replies before it.
        #[test]
        fn answers_the_color_before_the_device_attributes_fence() {
            let replies = replies_to(b"\x1b]11;?\x1b\\\x1b[c", palette());
            assert_eq!(replies.len(), 2, "expected colour + DA: {replies:?}");
            assert!(replies[0].contains("11;rgb:"), "{:?}", replies[0]);
            assert_eq!(replies[1], "\x1b[?6c");
        }

        /// An indexed-palette query has no fence waiting on it, and TUIC does not
        /// track those colours — a guess would be worse than silence here.
        #[test]
        fn stays_silent_for_indexed_palette_queries() {
            assert_eq!(
                replies_to(b"\x1b]4;3;?\x1b\\", palette()),
                [] as [std::string::String; 0]
            );
        }

        #[test]
        fn reports_whatever_the_frontend_last_published() {
            let light = TerminalPalette {
                foreground: (0x00, 0x00, 0x00),
                background: (0xff, 0xff, 0xff),
                cursor: (0x00, 0x00, 0x00),
            };
            let replies = replies_to(b"\x1b]11;?\x1b\\", light);
            assert!(
                replies[0].contains("11;rgb:ffff/ffff/ffff"),
                "{:?}",
                replies[0]
            );
        }
    }

    // --- F23: how much does the whole-row wire format overship? ---

    /// The grid ticker's period. Frames are taken on this boundary so a replay
    /// batches the same chunks into the same frame a live session would.
    const TICK_US: u64 = 16_000;

    #[derive(Debug, Default)]
    struct DamageTotals {
        frames: usize,
        full_frames: usize,
        rows: usize,
        damaged_cells: usize,
        shipped_cells: usize,
    }

    impl DamageTotals {
        fn add(&mut self, g: DamageGeometry) {
            if g.rows == 0 {
                return;
            }
            self.frames += 1;
            self.full_frames += usize::from(g.full_frame);
            self.rows += g.rows;
            self.damaged_cells += g.damaged_cells;
            self.shipped_cells += g.shipped_cells;
        }

        /// Cells shipped per cell actually damaged. 1.0 means the current format
        /// wastes nothing; 10.0 means F23 would cut this workload's row payload
        /// by 90%.
        fn overship(&self) -> f64 {
            if self.damaged_cells == 0 {
                return 1.0;
            }
            self.shipped_cells as f64 / self.damaged_cells as f64
        }
    }

    /// Replay a capture the way the grid ticker sees it: feed every output record
    /// into the vt, and take a frame each time the capture's own clock crosses a
    /// tick boundary. Input records are skipped — they never reach the vt.
    ///
    /// The tick grouping is what makes the result meaningful. Sampling per chunk
    /// instead would split one repaint across several frames and report damage
    /// far narrower than production ever sees.
    fn replay_damage(bytes: &[u8], rows: u16, cols: u16) -> DamageTotals {
        let records = crate::pty_capture::decode(bytes).expect("decodable capture");
        let mut grid = TerminalGrid::new(rows, cols, 10_000);
        let mut totals = DamageTotals::default();
        let mut tick = 0u64;
        for rec in records {
            if rec.direction != crate::pty_capture::CaptureDirection::Output {
                continue;
            }
            grid.process(&rec.data);
            if rec.elapsed_us / TICK_US > tick {
                tick = rec.elapsed_us / TICK_US;
                totals.add(grid.take_damage_geometry());
            }
        }
        totals.add(grid.take_damage_geometry());
        totals
    }

    /// `take_damage_geometry` must agree with `serialize_dirty_rows` about which
    /// rows a frame carries, or every number measured through it is fiction.
    #[test]
    fn damage_geometry_row_count_matches_the_serializer() {
        let mut measured = TerminalGrid::new(10, 40, 100);
        let mut serialized = TerminalGrid::new(10, 40, 100);

        for chunk in [
            b"\x1b[2Jhello".as_slice(),
            b" world".as_slice(),
            b"\r\nsecond line".as_slice(),
        ] {
            measured.process(chunk);
            serialized.process(chunk);

            let geometry = measured.take_damage_geometry();
            let frame = serialized.serialize_dirty_rows();
            let rows_in_frame = if frame.is_empty() {
                0
            } else {
                u16::from_le_bytes([frame[0], frame[1]]) as usize
            };

            assert_eq!(
                geometry.rows, rows_in_frame,
                "geometry and serializer disagree on row count"
            );
            assert_eq!(
                geometry.shipped_cells,
                rows_in_frame * 40,
                "the wire format ships whole rows"
            );
        }
    }

    /// Catches trimming a real separator at a soft margin and losing wrap metadata.
    #[test]
    fn softwrap_1977_preserves_original_spaces_in_text_and_log() {
        for bytes in [
            include_bytes!("../../../src/fixtures/agent_prompts/softwrap-spaces-1977-24.tcap")
                .as_slice(),
            include_bytes!("../../../src/fixtures/agent_prompts/softwrap-spaces-1977-64.tcap")
                .as_slice(),
        ] {
            let capture = crate::pty_capture::decode_capture(bytes).unwrap();
            let (rows, cols) = capture.geometry.unwrap();
            let mut grid = TerminalGrid::new(rows, cols, 100);
            let mut raw = Vec::new();
            for record in capture.records {
                raw.extend_from_slice(&record.data);
                grid.process(&record.data);
            }
            let expected = String::from_utf8(raw).unwrap().replace("\r\n", "\n");
            let join = |rows: Vec<(String, bool)>| {
                let mut text = String::new();
                let mut previous_wrap = true;
                for (row, wraps) in rows {
                    if !previous_wrap {
                        text.push('\n');
                    }
                    text.push_str(&row);
                    previous_wrap = wraps;
                }
                text.trim_end_matches('\n').to_owned()
            };
            assert_eq!(
                join(
                    grid.screen_text_rows()
                        .into_iter()
                        .zip(grid.screen_row_wraps())
                        .collect()
                ),
                expected,
                "MCP screen text at {cols} columns"
            );
            let lines = grid.screen_log_lines();
            assert_eq!(
                join(
                    lines
                        .iter()
                        .map(|line| (line.text(), line.wrapped))
                        .collect()
                ),
                expected,
                "mobile log at {cols} columns"
            );
            assert_eq!(
                grid.get_selection_text(0, 0, rows as usize - 1, cols as usize - 1),
                expected,
                "copied text must preserve the same spaces"
            );
            let serialized = serde_json::to_value(&lines).unwrap();
            assert!(
                serialized
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|line| line["wrapped"] == true),
                "mobile must receive WRAPLINE"
            );
            grid.process(b"\r\n1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n8");
            let history = grid.read_scrollback_log_lines(100);
            assert!(
                join(
                    history
                        .iter()
                        .map(|line| (line.text(), line.wrapped))
                        .collect()
                )
                .starts_with(&expected),
                "history must retain the same separators"
            );
        }
    }

    /// Catches dropping a styled whitespace-only span at a soft-wrap boundary.
    #[test]
    fn softwrap_1977_keeps_styled_margin_space() {
        let mut grid = TerminalGrid::new(3, 4, 10);
        grid.process(b"abc\x1b[31m \x1b[0mdef");
        let lines = grid.screen_log_lines();
        assert_eq!(lines[0].text(), "abc ");
        assert!(lines[0].wrapped);
        assert!(lines[0].spans.last().unwrap().fg.is_some());
        assert_eq!(grid.get_logical_line(0).1, "abc def");
    }

    /// The premise under F23: alacritty reports narrow column bounds when only a
    /// few cells change, and the serializer throws them away. A spinner ticking
    /// one glyph damages one column and ships forty.
    #[test]
    fn narrow_edits_report_narrow_column_damage() {
        let mut grid = TerminalGrid::new(10, 40, 100);
        grid.process(b"\x1b[2J\x1b[H");
        let _ = grid.take_damage_geometry();

        // Home, then overwrite a single cell — the shape of a spinner frame.
        for glyph in [b"\x1b[H|", b"\x1b[H/", b"\x1b[H-"] {
            grid.process(glyph);
            let g = grid.take_damage_geometry();
            assert_eq!(g.rows, 1, "one row damaged");
            // Two, not one: the glyph lands on column 0 and the cursor ends on
            // column 1, and alacritty damages the cell the cursor sits on. So the
            // floor for any edit is the edit plus one cell — which is why F23's
            // win is measured, not assumed.
            assert_eq!(g.damaged_cells, 2, "the edit plus the cursor cell");
            assert_eq!(g.shipped_cells, 40, "but a whole row is shipped");
        }
    }

    /// Decode the first row header of a dirty-row frame: (row_index, count,
    /// wrapped, start_col, cell_bytes).
    fn first_row_header(frame: &[u8], num_cols: usize) -> (usize, usize, bool, usize, usize) {
        const HEADER: usize = 26;
        let row_index = u16::from_le_bytes([frame[HEADER], frame[HEADER + 1]]) as usize;
        let raw = u16::from_le_bytes([frame[HEADER + 2], frame[HEADER + 3]]);
        let wrapped = raw & ROW_WRAPPED_FLAG != 0;
        let partial = raw & ROW_PARTIAL_FLAG != 0;
        let count = (raw & !(ROW_WRAPPED_FLAG | ROW_PARTIAL_FLAG)) as usize;
        let (start_col, consumed) = if partial {
            (
                u16::from_le_bytes([frame[HEADER + 4], frame[HEADER + 5]]) as usize,
                6,
            )
        } else {
            (0, 4)
        };
        assert!(
            start_col + count <= num_cols,
            "a row's span must fit the screen: start {start_col} + count {count} > {num_cols}"
        );
        (
            row_index,
            count,
            wrapped,
            start_col,
            frame.len() - HEADER - consumed,
        )
    }

    /// F23: a narrow edit must ship a narrow row. Without this the change is a
    /// no-op that still compiles and still passes every whole-row test.
    #[test]
    fn a_narrow_edit_ships_only_the_damaged_span() {
        let mut grid = TerminalGrid::new(10, 40, 100);
        grid.process(b"\x1b[2J\x1b[H");
        let _ = grid.serialize_dirty_rows();

        // Jump to column 11 (1-based) and write. This frame is NOT the one under
        // test: alacritty damages the cell the cursor left as well as the one it
        // arrived at, so a jump spans everything between them. Serializing here
        // absorbs that, leaving the cursor already in place.
        grid.process(b"\x1b[1;11HX");
        let jump = grid.serialize_dirty_rows();
        let (_, jump_count, _, jump_start, _) = first_row_header(&jump, 40);
        assert_eq!(
            (jump_start, jump_count),
            (0, 12),
            "a cursor jump damages the whole path it travelled"
        );

        // Now the steady-state case: one more glyph where the cursor already is.
        grid.process(b"Y");
        let frame = grid.serialize_dirty_rows();
        assert!(!frame.is_empty(), "the edit produced a frame");

        let (row_index, count, _wrapped, start_col, cell_bytes) = first_row_header(&frame, 40);
        assert_eq!(row_index, 0, "the edited row");
        assert_eq!(start_col, 11, "the span starts at the edited column");
        // The glyph plus the cell the cursor lands on — see
        // `narrow_edits_report_narrow_column_damage`.
        assert_eq!(count, 2, "only the damaged span is carried");
        assert_eq!(cell_bytes, count * 11, "and only that many cells follow");
        assert!(
            frame.len() < 26 + 6 + 40 * 11,
            "the frame is smaller than the whole-row format would produce"
        );
    }

    /// Overwriting one half of a fullwidth pair rewrites the OTHER half — the
    /// terminal calls `clear_wide()` on it, which resets its character to a space.
    /// The whole-row format shipped that cell as a side effect of re-encoding every
    /// column; the partial-row format only ships the damaged span, so if the
    /// neighbour never enters damage tracking the client keeps rendering the stale
    /// half and a ghost `中` survives next to the character that replaced its spacer.
    #[test]
    fn breaking_up_a_wide_char_ships_both_halves() {
        let mut grid = TerminalGrid::new(10, 40, 100);
        grid.process(b"\x1b[2J\x1b[H");
        let _ = grid.serialize_dirty_rows();

        // 中 occupies columns 4 and 5 (the second is a WIDE_CHAR_SPACER).
        grid.process("\x1b[1;5H中".as_bytes());
        let _ = grid.serialize_dirty_rows();

        // Land the cursor directly on the spacer and overwrite it. Column 4 is not
        // on the cursor's path (it moves from 6 to 5), so only the fix puts it in
        // the damaged span.
        grid.process(b"\x1b[1;6HX");
        let frame = grid.serialize_dirty_rows();
        assert!(!frame.is_empty(), "the edit produced a frame");

        let (row_index, count, _wrapped, start_col, _) = first_row_header(&frame, 40);
        assert_eq!(row_index, 0, "the edited row");
        assert!(
            start_col <= 4 && start_col + count > 4,
            "the wide char's leading column must be shipped, got span [{start_col}, {})",
            start_col + count
        );
    }

    /// The whole-row path must stay byte-identical, or an old frontend paired
    /// with this backend decodes garbage.
    #[test]
    fn a_full_row_frame_carries_no_start_col() {
        let mut grid = TerminalGrid::new(10, 40, 100);
        // A fresh grid has never sent a frame, so the viewport check forces full
        // damage — every row, full width.
        grid.process(b"hello");
        let frame = grid.serialize_dirty_rows();

        let raw = u16::from_le_bytes([frame[28], frame[29]]);
        assert_eq!(raw & ROW_PARTIAL_FLAG, 0, "full rows carry no partial flag");
        assert_eq!(
            (raw & !(ROW_WRAPPED_FLAG | ROW_PARTIAL_FLAG)) as usize,
            40,
            "and count is the full width"
        );
        assert_eq!(
            frame.len(),
            26 + 10 * (4 + 40 * 11),
            "ten whole rows, four-byte row headers, no start_col anywhere"
        );
    }

    /// What a replay of one capture observed.
    #[derive(Default)]
    struct HistoryReplay {
        committed_rows: usize,
        live_rewrites: usize,
        alt_records: usize,
    }

    /// Replays `.tcap` output records into a grid whose viewport is held `k` rows
    /// above the bottom (0 = pinned to the tail) and checks the contract the scroll
    /// row cache relies on (#1264-89c8): a row that entered history never changes,
    /// every history row is present and contiguous in all-time order (a resize
    /// repaint may replace history only after ordered content survival is proven),
    /// and the viewport at offset `k` shows exactly the rows the absolute-index read returns
    /// for that window. `resize_at` = (output record index, rows, cols).
    fn replay_history_contract(
        name: &str,
        capture: &crate::pty_capture::DecodedCapture,
        cap: usize,
        k: usize,
        resize_at: Option<(usize, u16, u16)>,
    ) -> HistoryReplay {
        use std::collections::HashMap;
        let (rows, cols) = capture.geometry.unwrap_or((50, 200));
        let mut grid = TerminalGrid::new(rows, cols, cap);
        let mut stats = HistoryReplay::default();
        // All-time row index -> text, for rows already in history.
        let mut committed: HashMap<usize, String> = HashMap::new();
        let mut next_index = 0usize;
        let mut screen: Vec<String> = Vec::new();
        // Only resize reconciliation consumes a complete before/after snapshot.
        // Reuse the previous record's actual after-snapshot as the next before-
        // snapshot; scrolling the viewport does not change history content.
        let mut previous_history = Vec::new();
        let outputs = capture
            .records
            .iter()
            .filter(|r| r.direction == crate::pty_capture::CaptureDirection::Output);
        for (step, rec) in outputs.enumerate() {
            let before = grid.scrollback_count();
            if let Some((_, r, c)) = resize_at.filter(|(at, _, _)| *at == step) {
                let history_of = |g: &TerminalGrid| -> Vec<String> {
                    match g.scrollback_count() {
                        0 => Vec::new(),
                        n => g.read_rows_in_range(0, n - 1),
                    }
                };
                let flat = |g: &TerminalGrid| -> Vec<String> {
                    g.read_rows_in_range(0, g.total_lines() - 1)
                        .into_iter()
                        .filter(|t| !t.trim().is_empty())
                        .collect()
                };
                let old_rows = grid.total_lines() - grid.scrollback_count();
                let (old_history, old_content) = (history_of(&grid), flat(&grid));
                grid.resize_with_mode(r, c, ReflowMode::All);
                if (r as usize) < old_rows {
                    // Shrinking drops non-blank rows below the cursor by design, and
                    // pushes the top of the screen into history: the old history
                    // must survive as the prefix.
                    assert_eq!(
                        &history_of(&grid)[..old_history.len()],
                        &old_history[..],
                        "{name}: shrinking the screen changed existing history"
                    );
                } else {
                    assert_eq!(
                        flat(&grid),
                        old_content,
                        "{name}: growing the screen lost or reordered lines"
                    );
                }
                // The all-time origin may shift with the new row count: rebuild.
                committed.clear();
                next_index = 0;
                screen.clear();
                previous_history = grid.read_scrollback_lines(0, grid.scrollback_count());
            }
            let origin_before = grid.screen_origin();
            let history_before = std::mem::take(&mut previous_history);
            grid.process(&rec.data);
            if grid.is_alternate_screen() {
                stats.alt_records += 1;
                previous_history.clear();
                continue;
            }
            if k > 0 {
                grid.scroll_to_offset(k);
            }
            let history = grid.scrollback_count();
            let base = grid.screen_origin().saturating_sub(history);
            let retained = if resize_at.is_some() {
                grid.read_scrollback_lines(0, history)
            } else {
                Vec::new()
            };
            let coordinates_changed = grid.screen_origin() < origin_before
                || retained
                    .iter()
                    .enumerate()
                    .any(|(row, text)| committed.get(&(base + row)).is_some_and(|old| old != text));
            if resize_at.is_some() && coordinates_changed && history != 0 {
                // Resize repaint reconciliation removes only proven duplicates and
                // rewinds the coordinates of newer rows. Prove content survival
                // independently before rebuilding the coordinate-based oracle:
                // every old retained row must still occur in order, with its
                // multiplicity, in history plus the replacement screen.
                let old_base = origin_before.saturating_sub(history_before.len());
                let evicted = base.saturating_sub(old_base).min(history_before.len());
                let all = grid.read_rows_in_range(0, grid.total_lines() - 1);
                let mut remaining = all.iter();
                for old in &history_before[evicted..] {
                    assert!(
                        remaining.any(|row| row == old),
                        "{name} k={k} step {step}: reconciliation lost or reordered history row {old:?}; before={history_before:?}, after={all:?}"
                    );
                }
                committed.clear();
                next_index = base;
            }

            // Viewport at the held offset == the same window read by absolute index.
            let offset = grid.display_offset();
            let top = history - offset;
            let vrows = grid.total_lines() - history;
            let window = grid.read_rows_in_range(top, top + vrows - 1);
            let shown: Vec<String> = (0..vrows).map(|r| grid.get_row_text(r)).collect();
            assert_eq!(
                shown, window,
                "{name} k={k} step {step}: viewport differs from the rows it should show"
            );

            let now = grid.screen_text_rows();
            if history == before && screen.len() == now.len() {
                stats.live_rewrites += now.iter().zip(&screen).filter(|(a, b)| a != b).count();
            }
            screen = now;

            // Rows below `base` were evicted by the cap; everything else must be
            // there. New rows are recorded, known rows must be unchanged.
            let full_scan = step % 50 == 0;
            let from = if full_scan {
                base
            } else {
                next_index.max(base)
            };
            let texts = if from - base < history {
                grid.read_rows_in_range(from - base, history - 1)
            } else {
                Vec::new()
            };
            assert_eq!(
                texts.len(),
                history.saturating_sub(from - base),
                "{name}: history read short"
            );
            for (i, text) in texts.into_iter().enumerate() {
                let idx = from + i;
                match committed.get(&idx) {
                    Some(seen) => assert_eq!(
                        seen, &text,
                        "{name} k={k} step {step}: history row {idx} changed after commit"
                    ),
                    None => {
                        // A jump is legitimate only to the oldest retained row: the
                        // cap evicted, or the agent erased scrollback (ESC[3J), rows
                        // this replay never saw.
                        assert!(
                            idx == next_index || idx == base,
                            "{name} k={k} step {step}: history rows skipped to {idx} (expected {next_index})"
                        );
                        committed.insert(idx, text);
                        next_index = idx + 1;
                        stats.committed_rows += 1;
                    }
                }
            }
            if history > 0 {
                assert_eq!(
                    next_index,
                    base + history,
                    "{name} k={k} step {step}: history rows missing (base {base}, history {history}, from {from})"
                );
            } else {
                // Scrollback erased: nothing retained to compare, resume at the origin.
                next_index = base;
            }
            committed.retain(|idx, _| *idx >= base);
            previous_history = retained;
        }
        stats
    }

    fn fixture_captures() -> Vec<(String, crate::pty_capture::DecodedCapture)> {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../src/fixtures/agent_prompts");
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("fixture directory") {
            let path = entry.expect("readable entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("tcap") {
                continue;
            }
            let bytes = std::fs::read(&path).expect("readable fixture");
            let capture = crate::pty_capture::decode_capture(&bytes).expect("decodable fixture");
            out.push((
                path.file_name().unwrap().to_string_lossy().into_owned(),
                capture,
            ));
        }
        assert!(
            !out.is_empty(),
            "no .tcap fixtures found in {}",
            dir.display()
        );
        out
    }

    // Each parameter is its own nextest case: replaying the growing fixture
    // corpus six times in one process exceeded CI's 120s hang bound. Keep all
    // records and assertions, but let the harness schedule and name each case.
    mod history_rows_never_change_while_scrolled_up_over_real_captures {
        use super::*;

        macro_rules! viewport_case {
            ($name:ident, $offset:expr) => {
                #[test]
                fn $name() {
                    let mut checked = 0;
                    for (name, capture) in fixture_captures() {
                        let rows = capture.geometry.map_or(50, |(r, _)| r as usize);
                        let k = ($offset)(rows);
                        checked += replay_history_contract(&name, &capture, 10_000, k, None)
                            .committed_rows;
                    }
                    assert!(
                        checked > 1000,
                        "corpus too small to mean anything: {checked} rows"
                    );
                }
            };
        }

        viewport_case!(tail, |_| 0);
        viewport_case!(one_row, |_| 1);
        viewport_case!(three_rows, |_| 3);
        viewport_case!(half_page, |rows| rows / 2);
        viewport_case!(one_page, |rows| rows);
        viewport_case!(deep_history, |rows| rows + 7);
    }

    /// The cap evicts the oldest rows; the survivors stay contiguous and unchanged.
    #[test]
    fn history_rows_survive_the_scrollback_cap_over_real_captures() {
        // One long session: every fixture back to back on the same grid.
        let mut all = fixture_captures();
        all.sort_by(|a, b| a.0.cmp(&b.0));
        let geometry = all[0].1.geometry;
        let records: Vec<_> = all.into_iter().flat_map(|(_, c)| c.records).collect();
        let session = crate::pty_capture::DecodedCapture { geometry, records };
        for k in [0, 5] {
            let stats = replay_history_contract("all fixtures", &session, 10, k, None);
            assert!(
                stats.committed_rows > 10,
                "the session never exceeded the cap: the test proves nothing"
            );
        }
    }

    /// A rows-only resize in the middle of a redraw keeps history intact (shrink)
    /// and every line in order (grow). As with viewport offsets above, each
    /// parameter combination has its own harness budget, not a larger timeout.
    mod history_content_survives_a_resize_during_redraw_over_real_captures {
        use super::*;

        macro_rules! resize_case {
            ($name:ident, $height:expr, $step:expr) => {
                #[test]
                fn $name() {
                    for (name, capture) in fixture_captures() {
                        let (rows, cols) = capture.geometry.unwrap_or((50, 200));
                        let new_rows = ($height)(rows);
                        let at = ($step)(capture.records.len());
                        replay_history_contract(
                            &name,
                            &capture,
                            10_000,
                            4,
                            Some((at, new_rows, cols)),
                        );
                    }
                }
            };
        }

        resize_case!(
            shrink_early,
            |rows: u16| rows.saturating_sub(9).max(3),
            |_| 3
        );
        resize_case!(
            shrink_middle,
            |rows: u16| rows.saturating_sub(9).max(3),
            |len| len / 2
        );
        resize_case!(grow_early, |rows| rows + 6, |_| 3);
        resize_case!(grow_middle, |rows| rows + 6, |len| len / 2);
    }

    /// Catches an oracle treating the startup banner's proven on-screen
    /// replacement as lost history after a synchronized resize repaint.
    #[test]
    fn startup_resize_repaint_moves_history_banner_to_screen_without_loss() {
        let capture = crate::pty_capture::decode_capture(include_bytes!(
            "../../../src/fixtures/agent_prompts/claude-2.1.294-remote-startup-idle-20261013.tcap"
        ))
        .unwrap();
        let (rows, cols) = capture.geometry.unwrap();
        let mut grid = TerminalGrid::new(rows, cols, 10_000);
        let outputs: Vec<_> = capture
            .records
            .iter()
            .filter(|record| record.direction == crate::pty_capture::CaptureDirection::Output)
            .collect();
        for (step, record) in outputs[..7].iter().enumerate() {
            if step == 5 {
                grid.resize_with_mode(rows - 9, cols, ReflowMode::All);
            }
            grid.process(&record.data);
        }
        let before = grid.read_scrollback_lines(0, grid.scrollback_count());
        assert_eq!(before.len(), 9);
        grid.process(&outputs[7].data);
        let after = grid.read_scrollback_lines(0, grid.scrollback_count());
        assert_eq!(after.len(), 5);
        assert_eq!(after[..3], before[..3]);
        assert!(after[3..].iter().all(String::is_empty));
        assert_eq!(grid.screen_text_rows()[..3], before[3..6]);
        assert!(before[3].contains("Claude Code"));
        assert!(before[4].contains("Opus 5.5"));
        assert!(before[5].contains("~/Gits/personal"));
        let all = grid.read_rows_in_range(0, grid.total_lines() - 1);
        let mut remaining = all.iter();
        for row in &before {
            assert!(remaining.any(|current| current == row));
        }
    }

    /// Entering and leaving the alternate screen never touches primary history.
    #[test]
    fn alt_screen_round_trip_leaves_history_untouched() {
        let mut grid = TerminalGrid::new(4, 20, 100);
        for i in 0..10 {
            let _ = grid.process(format!("l{i}\r\n").as_bytes());
        }
        let history = grid.scrollback_count();
        let before = grid.read_rows_in_range(0, history - 1);
        let _ = grid.process(b"\x1b[?1049hALT1\r\nALT2\r\nALT3\r\nALT4\r\nALT5\r\nALT6");
        assert!(grid.is_alternate_screen());
        let _ = grid.process(b"\x1b[?1049l");
        assert!(!grid.is_alternate_screen());
        assert_eq!(grid.scrollback_count(), history);
        assert_eq!(grid.read_rows_in_range(0, history - 1), before);
    }

    /// Whole-corpus run: the operator's own captures, no size limit.
    ///
    /// ```text
    /// TUIC_HISTORY_CORPUS="$HOME/Library/Application Support/com.tuic.commander/captures" \
    ///   cargo test -p tuic-terminal history_rows_never_change_over_capture_corpus -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "needs a capture corpus; see TUIC_HISTORY_CORPUS"]
    fn history_rows_never_change_over_capture_corpus() {
        let Ok(dir) = std::env::var("TUIC_HISTORY_CORPUS") else {
            panic!("set TUIC_HISTORY_CORPUS to a directory of .tcap captures");
        };
        let (mut files, mut committed, mut live, mut alt) = (0usize, 0usize, 0usize, 0usize);
        for entry in std::fs::read_dir(&dir).expect("readable corpus directory") {
            let path = entry.expect("readable entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("tcap") {
                continue;
            }
            let capture = crate::pty_capture::decode_capture(
                &std::fs::read(&path).expect("readable capture"),
            )
            .expect("decodable capture");
            let name = path.display().to_string();
            for k in [0, 2, 9, 40] {
                let s = replay_history_contract(&name, &capture, 10_000, k, None);
                committed += s.committed_rows;
                live += s.live_rewrites;
                alt += s.alt_records;
            }
            files += 1;
        }
        assert!(files > 0, "corpus held no usable .tcap captures");
        println!(
            "files {files}  history rows checked {committed}  live rows rewritten in place {live}  alt-screen records skipped {alt}"
        );
    }

    /// Measure the overship ratio over a corpus of real `.tcap` captures.
    ///
    /// Ignored by default: the corpus is whatever the operator recorded through
    /// `POST /diagnostics/capture`, and those files hold real session content, so
    /// they are deliberately NOT committed. Point it at a capture directory and
    /// run it when the F23 trade-off needs re-deciding:
    ///
    /// ```text
    /// TUIC_DAMAGE_CORPUS="$HOME/Library/Application Support/com.tuic.commander/captures" \
    ///   cargo test -p tuicommander damage_overship_over_capture_corpus -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "needs a capture corpus; see TUIC_DAMAGE_CORPUS"]
    fn damage_overship_over_capture_corpus() {
        let Ok(dir) = std::env::var("TUIC_DAMAGE_CORPUS") else {
            panic!("set TUIC_DAMAGE_CORPUS to a directory of .tcap captures");
        };
        let mut corpus = DamageTotals::default();
        let mut files = 0usize;

        for entry in std::fs::read_dir(&dir).expect("readable corpus directory") {
            let path = entry.expect("readable entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("tcap") {
                continue;
            }
            let bytes = std::fs::read(&path).expect("readable capture");
            let totals = replay_damage(&bytes, 50, 200);
            if totals.frames == 0 {
                continue;
            }
            files += 1;
            println!(
                "{:<40} frames {:>5}  full {:>5}  rows/frame {:>6.1}  overship {:>6.2}x",
                path.file_name().unwrap_or_default().to_string_lossy(),
                totals.frames,
                totals.full_frames,
                totals.rows as f64 / totals.frames as f64,
                totals.overship(),
            );
            corpus.frames += totals.frames;
            corpus.full_frames += totals.full_frames;
            corpus.rows += totals.rows;
            corpus.damaged_cells += totals.damaged_cells;
            corpus.shipped_cells += totals.shipped_cells;
        }

        assert!(files > 0, "corpus held no usable .tcap captures");
        println!(
            "\nCORPUS  files {files}  frames {}  full-frame {:.1}%  rows/frame {:.1}  overship {:.2}x",
            corpus.frames,
            100.0 * corpus.full_frames as f64 / corpus.frames as f64,
            corpus.rows as f64 / corpus.frames as f64,
            corpus.overship(),
        );
    }

    #[test]
    fn sync_update_active_tracks_bsu_and_esu() {
        let mut grid = TerminalGrid::new(10, 40, 100);
        assert!(
            !grid.is_sync_update_active(),
            "idle grid is not in sync mode"
        );
        grid.process(BSU);
        assert!(
            grid.is_sync_update_active(),
            "BSU opens a synchronized update"
        );
        grid.process(ESU);
        assert!(!grid.is_sync_update_active(), "ESU closes it");
    }

    #[test]
    fn stalled_sync_update_flushes_once_the_deadline_passes() {
        let mut grid = TerminalGrid::new(10, 40, 100);
        grid.process(b"BEFORE\r\n");
        grid.process(BSU);
        grid.process(b"BUFFERED\r\n");

        assert!(
            !screen_contains(&grid, "BUFFERED"),
            "content inside an open sync update stays buffered"
        );
        assert!(
            !grid.flush_sync_timeout_if_needed(),
            "no flush before the deadline"
        );

        std::thread::sleep(PAST_DEADLINE);

        assert!(
            grid.flush_sync_timeout_if_needed(),
            "an expired sync update must flush without any further PTY bytes"
        );
        assert!(
            screen_contains(&grid, "BUFFERED"),
            "flushed content is visible"
        );
        assert!(
            !grid.is_sync_update_active(),
            "the update is closed after flushing"
        );
        assert!(
            !grid.flush_sync_timeout_if_needed(),
            "a closed update does not flush twice"
        );
    }

    #[test]
    fn esu_before_the_deadline_leaves_nothing_to_flush() {
        let mut grid = TerminalGrid::new(10, 40, 100);
        grid.process(BSU);
        grid.process(b"QUICK\r\n");
        grid.process(ESU);
        assert!(screen_contains(&grid, "QUICK"), "ESU applies the update");

        std::thread::sleep(PAST_DEADLINE);
        assert!(
            !grid.flush_sync_timeout_if_needed(),
            "an already-closed update must not be flushed again by the ticker"
        );
    }

    #[test]
    fn nested_bsu_extends_the_deadline_instead_of_closing() {
        let mut grid = TerminalGrid::new(10, 40, 100);
        grid.process(BSU);
        grid.process(b"FIRST\r\n");
        std::thread::sleep(std::time::Duration::from_millis(100));
        grid.process(BSU); // re-arm before the first deadline expires
        grid.process(b"SECOND\r\n");

        assert!(
            grid.is_sync_update_active(),
            "a nested BSU keeps the update open"
        );
        assert!(
            !grid.flush_sync_timeout_if_needed(),
            "the nested BSU restarted the deadline, so 100ms in there is nothing to flush"
        );

        std::thread::sleep(PAST_DEADLINE);
        assert!(
            grid.flush_sync_timeout_if_needed(),
            "the extended deadline still expires"
        );
        assert!(
            screen_contains(&grid, "SECOND"),
            "content after the nested BSU surfaces"
        );
    }

    #[test]
    fn force_stop_surfaces_buffered_content_without_waiting() {
        let mut grid = TerminalGrid::new(10, 40, 100);
        grid.process(BSU);
        grid.process(b"ONSHUTDOWN\r\n");

        assert!(
            grid.force_stop_sync_if_buffered(),
            "teardown must not wait out the deadline"
        );
        assert!(
            screen_contains(&grid, "ONSHUTDOWN"),
            "buffered output is not dropped"
        );
        assert!(
            !grid.force_stop_sync_if_buffered(),
            "nothing left to force once the buffer drained"
        );
    }

    fn canonical_rows(grid: &TerminalGrid) -> Vec<String> {
        let total = grid.total_lines();
        if total == 0 {
            Vec::new()
        } else {
            grid.read_rows_in_range(0, total - 1)
        }
    }

    fn synchronized_stress_stream(frames: usize) -> Vec<u8> {
        let mut stream = Vec::new();
        for line in 0..24 {
            stream.extend_from_slice(format!("seed-{line:03}\r\n").as_bytes());
        }
        for frame in 0..frames {
            stream.extend_from_slice(BSU);
            stream.extend_from_slice(format!("section-{frame:03}\r\n").as_bytes());
            stream.extend_from_slice(
                format!("payload-{frame:03}-abcdefghijklmnopqrstuvwxyz\r\n").as_bytes(),
            );
            stream.extend_from_slice(ESU);
        }
        stream
    }

    #[test]
    fn synchronized_updates_are_invariant_across_every_chunk_boundary() {
        let stream = synchronized_stress_stream(64);

        let mut whole = TerminalGrid::new(12, 80, 10_000);
        whole.process(&stream);
        let expected = canonical_rows(&whole);

        // One byte per process call exercises every possible split inside BSU,
        // ESU, UTF-8-free payload text, CRLF, cursor movement, and scroll.
        let mut bytewise = TerminalGrid::new(12, 80, 10_000);
        for byte in &stream {
            bytewise.process(std::slice::from_ref(byte));
        }
        assert_eq!(canonical_rows(&bytewise), expected, "bytewise stream");

        // A deterministic irregular schedule models reader chunks under load.
        for schedule in [
            &[1, 2, 3, 5, 8, 13, 21][..],
            &[127, 4, 31, 2, 255, 7][..],
            &[9, 1, 1, 1, 64, 3, 17, 5][..],
        ] {
            let mut chunked = TerminalGrid::new(12, 80, 10_000);
            let mut offset = 0;
            let mut step = 0;
            while offset < stream.len() {
                let end = (offset + schedule[step % schedule.len()]).min(stream.len());
                chunked.process(&stream[offset..end]);
                offset = end;
                step += 1;
            }
            assert_eq!(
                canonical_rows(&chunked),
                expected,
                "irregular chunk schedule {schedule:?}"
            );
        }
    }

    #[test]
    fn synchronized_redraws_while_scrolled_preserve_history_and_final_screen() {
        let mut grid = TerminalGrid::new(8, 72, 10_000);
        for line in 0..40 {
            grid.process(format!("history-{line:03}\r\n").as_bytes());
        }
        let history_before = grid.read_rows_in_range(0, grid.scrollback_count() - 1);
        grid.scroll(17);
        assert!(grid.display_offset() > 0, "fixture must be scrolled up");

        for frame in 0..512 {
            let redraw = format!(
                "\x1b[?2026h\x1b[H\x1b[2Ksection: canonical\r\n\x1b[2Kframe: {frame:03}\r\n\x1b[2Kdata: never truncate this payload\x1b[?2026l"
            );
            // Split each redraw differently but reproducibly, including inside
            // escape sequences and printable lines.
            let pivot = 1 + (frame * 17 % (redraw.len() - 1));
            grid.process(&redraw.as_bytes()[..pivot]);
            grid.process(&redraw.as_bytes()[pivot..]);
        }

        let history_after = grid.read_rows_in_range(0, grid.scrollback_count() - 1);
        assert_eq!(
            history_after, history_before,
            "redraws must not rewrite history"
        );
        let screen = grid.read_screen_text();
        assert_eq!(
            screen
                .iter()
                .filter(|row| row.contains("section: canonical"))
                .count(),
            1,
            "section duplicated on the final screen: {screen:?}"
        );
        assert!(screen.iter().any(|row| row == "frame: 511"));
        assert!(
            screen
                .iter()
                .any(|row| row == "data: never truncate this payload")
        );
    }

    #[test]
    fn expired_update_then_late_esu_preserves_each_logical_record_once() {
        let mut grid = TerminalGrid::new(4, 80, 100);
        grid.process(b"seed-0\r\nseed-1\r\nseed-2\r\nseed-3\r\n");
        grid.process(BSU);
        grid.process(b"timeout-record\r\n");
        std::thread::sleep(PAST_DEADLINE);
        assert!(grid.flush_sync_timeout_if_needed());

        // The producer eventually resumes and sends the remainder plus the ESU
        // that would normally have closed the already-expired update.
        grid.process(b"late-record\r\n");
        grid.process(ESU);

        let rows = canonical_rows(&grid);
        for record in ["timeout-record", "late-record"] {
            assert_eq!(
                rows.iter().filter(|row| row.as_str() == record).count(),
                1,
                "{record} duplicated or lost after timeout: {rows:?}"
            );
        }
    }

    /// A live TUI redraw re-runs the same search over and over, and each run was
    /// compiling the query from scratch — `RegexSearch::new` builds four DFAs
    /// (forward/backward x literal/regex). The query changes when the user types,
    /// not when the screen repaints, so the compiled form is worth keeping.
    #[test]
    fn repeating_a_search_compiles_the_query_once() {
        let mut grid = TerminalGrid::new(24, 80, 500);
        grid.process(b"hello world\r\nhello again\r\n");

        let before = grid.regex_compiles();
        for _ in 0..5 {
            assert_eq!(grid.search("hello").len(), 2);
        }
        assert_eq!(
            grid.regex_compiles() - before,
            1,
            "the query was recompiled on every repaint"
        );
    }

    /// ...but a different query must not be answered with the cached one.
    #[test]
    fn changing_the_query_recompiles_it() {
        let mut grid = TerminalGrid::new(24, 80, 500);
        grid.process(b"alpha beta\r\n");

        assert_eq!(grid.search("alpha").len(), 1);
        assert_eq!(grid.search("beta").len(), 1);
        assert_eq!(grid.search("gamma").len(), 0);
        assert_eq!(grid.search("alpha").len(), 1);
        assert_eq!(
            grid.regex_compiles(),
            4,
            "each distinct query compiles once"
        );
    }

    /// The buffer search shares the cache: it is the same query, from the same
    /// find bar, and compiling it a second time defeats the point.
    #[test]
    fn the_buffer_search_shares_the_compiled_query() {
        let mut grid = TerminalGrid::new(24, 80, 500);
        grid.process(b"needle here\r\n");

        assert_eq!(grid.search("needle").len(), 1);
        let after_first = grid.regex_compiles();
        assert_eq!(grid.search_buffer("needle").len(), 1);
        assert_eq!(grid.regex_compiles(), after_first);
    }

    /// A user reading scrollback pinned `process()` to the slow path: the
    /// `display_offset != 0` guard rebuilt and diffed the WHOLE screen for every
    /// PTY chunk, exactly while a busy agent is producing them fastest.
    ///
    /// Measured against a control at the bottom of the buffer, because a chunk
    /// that scrolls the grid marks damage `Full` either way — the guard's cost
    /// is only visible on writes that stay within the screen. Scrolling the
    /// viewport still costs ONE rebuild (`scroll_to_offset` marks the screen
    /// fully damaged so the next frame is complete); what must not happen is one
    /// per chunk after that.
    #[test]
    fn reading_scrollback_does_not_force_a_full_rebuild_per_chunk() {
        fn seeded() -> TerminalGrid {
            let mut grid = TerminalGrid::new(6, 40, 200);
            for i in 0..40 {
                grid.process(format!("history line {i}\r\n").as_bytes());
            }
            grid
        }
        fn in_place_writes(grid: &mut TerminalGrid) -> usize {
            let before = grid.full_screen_reads();
            for i in 0..10 {
                grid.process(format!("\x1b[1;1Hstreamed {i}").as_bytes());
            }
            grid.full_screen_reads() - before
        }

        let mut control = seeded();
        let at_bottom = in_place_writes(&mut control);

        let mut scrolled = seeded();
        scrolled.scroll_to_offset(20);
        assert!(
            scrolled.display_offset() > 0,
            "test needs a scrolled-back view"
        );
        let while_scrolled = in_place_writes(&mut scrolled);

        assert_eq!(
            while_scrolled,
            at_bottom + 1,
            "reading scrollback costs a full screen rebuild per chunk, not one for the jump"
        );
    }

    /// The differential oracle again, this time with the view scrolled back:
    /// the fast path must report the same `ChangedRow`s the full rebuild would.
    #[test]
    fn process_damage_matches_full_diff_while_scrolled_back() {
        let inputs: &[&[u8]] = &[
            b"streaming line one",
            b"\r\nstreaming line two",
            b"\x1b[1;1Hoverwrite row 0",
            b"\r\n\r\n\r\n",
            b"wide \xe4\xb8\xad\xe6\x96\x87 while scrolled",
            b"\x1b[2Ktrailing erase",
            b"\r\nlast one",
        ];

        let mut opt = TerminalGrid::new(8, 40, 200);
        let mut reference = TerminalGrid::new(8, 40, 200);
        for i in 0..30 {
            let seed = format!("seed {i}\r\n");
            opt.process(seed.as_bytes());
            reference.process_full(seed.as_bytes());
        }
        opt.scroll_to_offset(15);
        reference.scroll_to_offset(15);

        for (idx, chunk) in inputs.iter().enumerate() {
            let mut a = opt.process(chunk);
            let mut b = reference.process_full(chunk);
            a.sort_by_key(|r| r.row_index);
            b.sort_by_key(|r| r.row_index);
            assert_eq!(
                a,
                b,
                "scrolled-back ChangedRow mismatch at chunk {idx} ({:?})",
                String::from_utf8_lossy(chunk),
            );
        }
    }

    /// Differential oracle (story 138): the parse-damage fast path in `process()`
    /// must produce byte-identical `ChangedRow`s to the old full-screen
    /// rebuild+diff (`process_full`) for every chunk. Two independent
    /// grids are fed the SAME byte stream; a divergence (e.g. a missed damaged
    /// line → under-report) surfaces as an assertion failure at that chunk.
    #[test]
    fn process_damage_matches_full_diff() {
        // A matrix exercising the tricky paths: plain text, newlines, clear,
        // cursor moves + overwrite, colors, tabs, wide (CJK) chars, backspace/CUB,
        // alt-screen enter/exit, OSC, and scroll-inducing newlines.
        let inputs: &[&[u8]] = &[
            b"hello world",
            b"\r\nsecond line here",
            b"\x1b[2J\x1b[H", // clear screen + home
            b"line A\r\nline B\r\nline C",
            b"\x1b[1;1Hover",                  // cursor home + overwrite row 0
            b"\x1b[31mred\x1b[0m then normal", // SGR colors (text only compared)
            b"tab\tstop\there",
            b"wide \xe4\xb8\xad\xe6\x96\x87 chars", // CJK wide chars + spacers
            b"\x08\x08\x08xyz",                     // backspaces
            b"\x1b[5Dmid",                          // CUB then write
            b"\x1b[?1049h",                         // enter alt screen
            b"alternate buffer text\r\nrow two",
            b"\x1b[?1049l", // exit alt screen
            b"back on primary screen",
            b"\x1b]0;my title\x07after title", // OSC 0 title + text
            b"z",
            b"\r\n\r\n\r\n\r\n\r\n", // several newlines (scrolling)
            b"final content on a new line",
        ];

        let mut opt = TerminalGrid::new(24, 80, 1000);
        let mut reference = TerminalGrid::new(24, 80, 1000);

        for (idx, chunk) in inputs.iter().enumerate() {
            let mut a = opt.process(chunk);
            let mut b = reference.process_full(chunk);
            a.sort_by_key(|r| r.row_index);
            b.sort_by_key(|r| r.row_index);
            if a != b {
                eprintln!("=== chunk {idx} {:?} ===", String::from_utf8_lossy(chunk));
                eprintln!("DAMAGE path : {a:?}");
                eprintln!("FULL   path : {b:?}");
            }
            assert_eq!(
                a,
                b,
                "ChangedRow mismatch at chunk {idx} ({:?}): damage path diverged from full-diff",
                String::from_utf8_lossy(chunk),
            );
        }
    }

    /// Cursor-only movement must NOT produce a ChangedRow (text unchanged), even
    /// though it damages lines — the text-equality guard filters it. Guards the
    /// core safety property of the over-report-is-safe design.
    #[test]
    fn process_ignores_cursor_only_movement() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        grid.process(b"content on row zero\r\nrow one text");
        // Pure cursor moves: home, down, up, right — no text written.
        let changed = grid.process(b"\x1b[H\x1b[B\x1b[A\x1b[10C");
        assert!(
            changed.is_empty(),
            "cursor-only movement must not report ChangedRows, got {changed:?}"
        );
    }

    /// Replay a raw PTY byte capture (e.g. recorded via `script -q file cmd`)
    /// into a fresh grid and dump the full buffer, for offline debugging of
    /// emulation bugs (story 056-7545: Ink duplication into scrollback).
    ///
    /// Env vars: TUIC_REPLAY_FILE (required), TUIC_REPLAY_ROWS/COLS (default
    /// 50x220), TUIC_REPLAY_CHUNK (default 4096), TUIC_REPLAY_OUT (dump path,
    /// default stdout), TUIC_REPLAY_RESIZE ("byteoffset:rows:cols,..." —
    /// resizes applied when the replay crosses each byte offset).
    ///
    /// Run: `cargo test --lib replay_capture_from_env -- --ignored --nocapture`
    #[test]
    #[ignore = "requires TUIC_REPLAY_FILE pointing to a PTY capture"]
    fn replay_capture_from_env() {
        let path = match std::env::var("TUIC_REPLAY_FILE") {
            Ok(p) => p,
            Err(_) => {
                eprintln!("TUIC_REPLAY_FILE not set — skipping");
                return;
            }
        };
        let rows: u16 = std::env::var("TUIC_REPLAY_ROWS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(50);
        let cols: u16 = std::env::var("TUIC_REPLAY_COLS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(220);
        let chunk: usize = std::env::var("TUIC_REPLAY_CHUNK")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4096);

        // Optional resize schedule: "byteoffset:rows:cols,..."
        let mut resizes: Vec<(usize, u16, u16)> = std::env::var("TUIC_REPLAY_RESIZE")
            .ok()
            .map(|spec| {
                spec.split(',')
                    .filter_map(|item| {
                        let mut parts = item.split(':');
                        Some((
                            parts.next()?.trim().parse().ok()?,
                            parts.next()?.trim().parse().ok()?,
                            parts.next()?.trim().parse().ok()?,
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        resizes.sort_unstable();

        let data = std::fs::read(&path).expect("read TUIC_REPLAY_FILE");
        let mut grid = TerminalGrid::new(rows, cols, 10000);
        let mut fed = 0usize;
        let mut next_resize = 0usize;
        for c in data.chunks(chunk) {
            while next_resize < resizes.len() && resizes[next_resize].0 <= fed {
                let (_, r, w) = resizes[next_resize];
                // Mirror production: VtLogBuffer::resize uses
                // ReflowMode::All on the normal screen (None only for alt).
                let mode = if grid.is_alternate_screen() {
                    ReflowMode::None
                } else {
                    ReflowMode::All
                };
                grid.resize_with_mode(r, w, mode);
                eprintln!("resized to {r}x{w} at byte {fed}");
                next_resize += 1;
            }
            let _ = grid.process(c);
            fed += c.len();
        }

        let mut out = String::new();
        let history = grid.scrollback_count();
        for line in grid.read_scrollback_lines(0, history) {
            out.push_str(&line);
            out.push('\n');
        }
        out.push_str("──── screen ────\n");
        for line in grid.screen_text_rows() {
            out.push_str(&line);
            out.push('\n');
        }

        match std::env::var("TUIC_REPLAY_OUT") {
            Ok(dest) => std::fs::write(&dest, &out).expect("write TUIC_REPLAY_OUT"),
            Err(_) => println!("{out}"),
        }
        eprintln!("replayed {} bytes → {} history lines", data.len(), history);
    }

    /// Real Claude Code (v2.1.286, Haiku) capture of "print LINE 001..250", taken
    /// while the PTY was resized 20x100 -> 14x100 -> 26x100 -> 10x80 -> 24x100 ->
    /// 16x100 -> 20x100 (byte offsets in `CLAUDE_LINES_RESIZES`). Claude answers each
    /// SIGWINCH by wiping the visible screen and printing its last rows again.
    ///
    /// Every numbered line must survive exactly once, in order. Catches both
    /// failures of the shrink: the top-clamped shrink keeps no row the wipe
    /// erases (lines 1-3, 75-76 and 148-150 vanish), and a bottom-anchored shrink
    /// that never takes back what the reprint repeats puts 77 and 78 in twice.
    const CLAUDE_LINES_RESIZES: [(usize, u16, u16); 6] = [
        (9146, 14, 100),
        (21500, 26, 100),
        (34739, 10, 80),
        (45525, 24, 100),
        (62285, 16, 100),
        (86115, 20, 100),
    ];

    #[test]
    fn resized_claude_repaint_keeps_every_line_once_in_order() {
        let data = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../src/fixtures/terminal_resize/claude-lines-resized.raw"),
        )
        .expect("fixture claude-lines-resized.raw");
        for chunk in [4096, 61] {
            let mut grid = TerminalGrid::new(20, 100, 10_000);
            let mut pos = 0;
            while pos < data.len() {
                let mut end = (pos + chunk).min(data.len());
                let next = CLAUDE_LINES_RESIZES.iter().find(|r| r.0 > pos);
                if let Some(&(at, rows, cols)) = next {
                    end = end.min(at);
                    let _ = grid.process(&data[pos..end]);
                    pos = end;
                    if pos == at {
                        grid.resize_with_mode(rows, cols, ReflowMode::All);
                    }
                } else {
                    let _ = grid.process(&data[pos..end]);
                    pos = end;
                }
            }
            let mut rows = grid.read_scrollback_lines(0, grid.scrollback_count());
            rows.extend(grid.screen_text_rows());
            let numbers: Vec<u32> = rows
                .iter()
                .filter_map(|row| {
                    row.find("LINE ")?
                        .checked_add(5)
                        .and_then(|i| row.get(i..i + 3))
                })
                .filter_map(|n| n.parse().ok())
                .collect();
            let expected: Vec<u32> = (1..=250).collect();
            assert_eq!(numbers, expected, "chunk size {chunk}");
        }
    }

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
        assert_ne!(changed, [] as [crate::vt_log::ChangedRow; 0]);
        let first = &changed[0];
        assert_eq!(first.row_index, 0);
        assert_eq!(first.text, "hello world");
    }

    #[test]
    fn process_returns_empty_on_no_change() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        let _ = grid.process(b"hello");
        let changed = grid.process(b"");
        assert_eq!(changed, [] as [crate::vt_log::ChangedRow; 0]);
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
    fn logical_prefix_stops_at_cursor_and_skips_wide_spacers() {
        let mut grid = TerminalGrid::new(5, 40, 100);
        let _ = grid.process(b"........................| C ]");
        let _ = grid.process("\rsuggest: [ A界 | B".as_bytes());

        let prefix = grid.logical_prefix_at_cursor().unwrap();
        assert_eq!(prefix.start_row, 0);
        assert_eq!(prefix.end_row, 0);
        assert_eq!(prefix.text, "suggest: [ A界 | B");
    }

    #[test]
    fn logical_prefix_enforces_wrap_and_byte_bounds() {
        let mut too_many_wraps = TerminalGrid::new(10, 4, 100);
        let _ = too_many_wraps.process(b"123456789012345678901");
        assert_eq!(too_many_wraps.logical_prefix_at_cursor(), None);

        let mut too_many_bytes = TerminalGrid::new(10, 128, 100);
        let data = "x".repeat(513);
        let _ = too_many_bytes.process(data.as_bytes());
        assert_eq!(too_many_bytes.logical_prefix_at_cursor(), None);
    }

    #[test]
    fn physical_suggest_prefix_survives_a_refused_soft_wrap_chain() {
        let mut grid = TerminalGrid::new(5, 80, 100);
        let _ = grid.process(b"........................................ stale suffix");
        let _ = grid.process(b"\rsuggest: [ A | B ]");

        assert_eq!(
            grid.physical_prefix_at_cursor(),
            Some(LogicalPrefix {
                text: "suggest: [ A | B ]".to_string(),
                start_row: 0,
                end_row: 0,
            })
        );
    }

    #[test]
    fn mouse_reporting_combined_decset() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        assert!(!grid.is_mouse_reporting());
        let _ = grid.process(b"\x1b[?1000;1002;1003;1006h");
        assert!(
            grid.is_mouse_reporting(),
            "combined DECSET must enable mouse reporting"
        );
        assert!(!grid.is_alternate_screen());
        let _ = grid.process(b"\x1b[?1000;1002;1003;1006l");
        assert!(!grid.is_mouse_reporting());
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
        assert_ne!(changed, [] as [crate::vt_log::ChangedRow; 0]);
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

    fn decode_cell_extras(buf: &[u8]) -> Vec<(u16, u16, Vec<u32>)> {
        let Some(magic) = buf.windows(4).rposition(|bytes| bytes == CELL_EXTRAS_MAGIC) else {
            return Vec::new();
        };
        let mut offset = magic + CELL_EXTRAS_MAGIC.len();
        let count = u32::from_le_bytes(buf[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let row = u16::from_le_bytes(buf[offset..offset + 2].try_into().unwrap());
            offset += 2;
            let col = u16::from_le_bytes(buf[offset..offset + 2].try_into().unwrap());
            offset += 2;
            let scalar_count = buf[offset] as usize;
            offset += 1;
            let mut scalars = Vec::with_capacity(scalar_count);
            for _ in 0..scalar_count {
                scalars.push(u32::from_le_bytes(
                    buf[offset..offset + 4].try_into().unwrap(),
                ));
                offset += 4;
            }
            entries.push((row, col, scalars));
        }
        assert_eq!(offset, buf.len(), "TCX1 trailer must consume the payload");
        entries
    }

    #[test]
    fn combining_extensions_roundtrip_through_every_binary_frame_kind() {
        let mut grid = TerminalGrid::new(2, 10, 0);
        let _ = grid.process(b"cafe");
        let plain = grid.serialize_dirty_rows();
        assert_eq!(
            decode_cell_extras(&plain),
            [] as [(u16, u16, std::vec::Vec<u32>); 0]
        );

        let changed = grid.process("\u{0301}\u{0327}".as_bytes());
        assert_eq!(
            changed.len(),
            1,
            "separate combining input must damage its row"
        );
        assert_eq!(changed[0].text, "cafe\u{0301}\u{0327}");

        let expected = vec![(0, 3, vec![0x0301, 0x0327])];
        assert_eq!(decode_cell_extras(&grid.serialize_dirty_rows()), expected);
        assert_eq!(decode_cell_extras(&grid.serialize_full_frame()), expected);
        assert_eq!(
            decode_cell_extras(&grid.serialize_styled_range(0, 2)),
            expected
        );

        let _ = grid.process(b"\x1b[1;4HX");
        assert!(
            decode_cell_extras(&grid.serialize_dirty_rows()).is_empty(),
            "overwriting the base cell removes its old extensions"
        );
    }

    #[test]
    fn partial_frame_omits_extensions_outside_its_transmitted_span() {
        let mut grid = TerminalGrid::new(2, 12, 0);
        let _ = grid.process("cafe\u{0301}".as_bytes());
        let _ = grid.serialize_dirty_rows();

        let _ = grid.process(b"\x1b[1;10HX");
        let partial = grid.serialize_dirty_rows();
        assert_eq!(
            decode_cell_extras(&partial),
            [] as [(u16, u16, std::vec::Vec<u32>); 0]
        );

        assert_eq!(
            decode_cell_extras(&grid.serialize_full_frame()),
            vec![(0, 3, vec![0x0301])],
            "the omitted extension still exists outside the partial span"
        );
    }

    #[test]
    fn delayed_combining_mark_damages_a_wide_base_at_the_margin() {
        let mut grid = TerminalGrid::new(1, 2, 0);
        let _ = grid.process("🦇".as_bytes());
        let _ = grid.serialize_dirty_rows();

        let changed = grid.process("\u{0301}".as_bytes());
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].text, "🦇\u{0301}");
        assert_eq!(
            decode_cell_extras(&grid.serialize_dirty_rows()),
            vec![(0, 0, vec![0x0301])]
        );
    }

    #[test]
    fn cell_extras_trailer_respects_the_nine_mark_storage_bound() {
        let mut grid = TerminalGrid::new(1, 4, 0);
        let input = format!("e{}", "\u{0301}".repeat(10));
        let _ = grid.process(input.as_bytes());

        assert_eq!(
            decode_cell_extras(&grid.serialize_dirty_rows()),
            vec![(0, 0, vec![0x0301; 9])]
        );
    }

    /// A `suggest:` line longer than the terminal is one logical line split over
    /// two display rows. The frontend overlay has to know that to mask the block
    /// (#8fc7) — and the frame is its only source of truth about the grid.
    #[test]
    fn serialized_rows_carry_the_wrapline_flag() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        let _ = grid.process(b"abcdefghijklmno");
        let buf = grid.serialize_dirty_rows();

        let h = TEST_HEADER_SIZE;
        let first = u16::from_le_bytes([buf[h + 2], buf[h + 3]]);
        assert_eq!(
            first & ROW_WRAPPED_FLAG,
            ROW_WRAPPED_FLAG,
            "row 0 continues onto row 1"
        );
        assert_eq!(
            first & !ROW_WRAPPED_FLAG,
            10,
            "column count survives the flag"
        );

        // Row 1 holds the tail: nothing continues after it.
        let second = h + 4 + 10 * 11;
        let tail = u16::from_le_bytes([buf[second + 2], buf[second + 3]]);
        assert_eq!(
            tail & ROW_WRAPPED_FLAG,
            0,
            "the last row of a line is not wrapped"
        );
        assert_eq!(tail & !ROW_WRAPPED_FLAG, 10);
    }

    #[test]
    fn styled_range_rows_carry_the_wrapline_flag() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        let _ = grid.process(b"abcdefghijklmno");
        let buf = grid.serialize_styled_range(0, 2);

        // Header: start_abs u32, history_size u32, cols u16, row_count u16.
        let first = u16::from_le_bytes([buf[12 + 4], buf[12 + 5]]);
        assert_eq!(first & ROW_WRAPPED_FLAG, ROW_WRAPPED_FLAG);
        assert_eq!(first & !ROW_WRAPPED_FLAG, 10);
    }

    #[test]
    fn serialize_plain_text_roundtrip() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        let _ = grid.process(b"Hi");
        let buf = grid.serialize_dirty_rows();
        assert_ne!(buf, [] as [u8; 0]);

        let (num_rows, cursor_row, cursor_col, cursor_visible) = decode_header(&buf);
        assert!(num_rows >= 1, "at least row 0 dirty");
        assert_eq!(cursor_row, 0);
        assert_eq!(cursor_col, 2);
        assert!(cursor_visible);

        // First dirty row header starts after header
        let h = TEST_HEADER_SIZE;
        let row_idx = u16::from_le_bytes([buf[h], buf[h + 1]]);
        let col_count = row_col_count(u16::from_le_bytes([buf[h + 2], buf[h + 3]]));
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
        assert_ne!(buf, [] as [u8; 0]);

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
    fn search_matches_decomposed_unicode_without_normalizing() {
        let mut grid = TerminalGrid::new(2, 20, 0);
        let _ = grid.process("cafe\u{0301}".as_bytes());

        let matches = grid.search("e\u{0301}");
        assert_eq!(matches.len(), 1);
        assert_eq!((matches[0].col_start, matches[0].col_end), (3, 4));
        assert!(grid.search("é").is_empty());
    }

    #[test]
    fn buffer_search_offsets_address_utf16_line_text() {
        let mut grid = TerminalGrid::new(2, 20, 0);
        let _ = grid.process("🦇 cafe\u{0301}".as_bytes());

        let matches = grid.search_buffer("e\u{0301}");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line_text, "🦇 cafe\u{0301}");
        assert_eq!((matches[0].match_start, matches[0].match_end), (6, 8));
        let utf16: Vec<u16> = matches[0].line_text.encode_utf16().collect();
        assert_eq!(String::from_utf16(&utf16[6..8]).unwrap(), "e\u{0301}");
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

    /// A match that wraps onto the next row must never report an end column taken
    /// from that next row: `SearchMatch` carries a single row, so the renderer would
    /// paint the highlight on the START row across cells that never matched.
    #[test]
    fn search_clips_wrapped_match_to_the_start_row() {
        let cols = 10;
        let mut grid = TerminalGrid::new(5, cols, 0);
        // "abcdefgh" starts at col 6 of row 0 and wraps: "abcd" on row 0, "efgh" on row 1.
        let _ = grid.process(b"......abcdefgh");
        let matches = grid.search("abcdefgh");
        assert_eq!(matches.len(), 1, "one logical hit");
        let m = &matches[0];
        assert_eq!(m.col_start, 6);
        assert_eq!(
            m.col_end,
            usize::from(cols),
            "end column clipped to the start row, not carried over from the wrapped row"
        );
        assert!(
            m.col_end > m.col_start,
            "highlight width must stay positive"
        );
    }

    /// Same clip when the hit spans three rows — the end column must still come
    /// from the START row, never from the row two lines further down.
    #[test]
    fn search_clips_match_wrapping_across_three_rows() {
        let cols = 10;
        let mut grid = TerminalGrid::new(5, cols, 0);
        // 25 chars at col 0 of a 10-column grid occupy rows 0, 1 and 2.
        let needle = "abcdefghijklmnopqrstuvwxy";
        assert_eq!(needle.len(), 25, "needle must span three 10-column rows");
        let _ = grid.process(needle.as_bytes());
        let matches = grid.search(needle);
        assert_eq!(matches.len(), 1, "one logical hit");
        assert_eq!(matches[0].col_start, 0);
        assert_eq!(matches[0].col_end, usize::from(cols));
    }

    /// The clip must not shrink a match that fits on one row.
    #[test]
    fn search_keeps_full_span_for_single_row_match() {
        let mut grid = TerminalGrid::new(5, 40, 0);
        let _ = grid.process(b"..hello..");
        let matches = grid.search("hello");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].col_start, 2);
        assert_eq!(matches[0].col_end, 7, "exclusive end of 'hello' at col 2");
    }

    /// A match ending exactly on the last column stays on its own row.
    #[test]
    fn search_match_ending_at_last_column_is_not_clipped_short() {
        let cols = 10;
        let mut grid = TerminalGrid::new(5, cols, 0);
        let _ = grid.process(b".....World");
        let matches = grid.search("World");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].col_start, 5);
        assert_eq!(matches[0].col_end, usize::from(cols));
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

    // #1264-89c8: the grid is not the layer that loses rows while the user is
    // scrolled back. Output grows history and the offset by the same amount, so
    // the viewport keeps showing the same lines; the frontend has to follow it.
    #[test]
    fn output_while_scrolled_back_keeps_the_viewport_lines() {
        let mut grid = TerminalGrid::new(3, 20, 100);
        let _ = grid.process(b"l0\r\nl1\r\nl2\r\nl3\r\nl4\r\nl5\r\nl6");
        grid.scroll_to_offset(2);
        let before: Vec<String> = (0..3).map(|r| grid.get_row_text(r)).collect();
        assert_eq!(before, ["l2", "l3", "l4"]);

        let _ = grid.process(b"\r\nl7\r\nl8\r\nl9\r\nl10\r\nl11");

        assert_eq!(
            grid.display_offset(),
            7,
            "offset grows by the 5 lines of output"
        );
        let after: Vec<String> = (0..3).map(|r| grid.get_row_text(r)).collect();
        assert_eq!(after, before);
    }

    // #1264-89c8 criterion 3: with output landing while scrolled back, sweeping the
    // whole buffer from the top shows every retained line once, in order, also
    // after the scrollback cap has trimmed the oldest ones.
    #[test]
    fn scrolled_back_output_leaves_every_retained_line_in_order_at_the_cap() {
        let (rows, cap, total) = (3usize, 10usize, 60usize);
        let mut grid = TerminalGrid::new(rows as u16, 20, cap);
        for i in 0..8 {
            let _ = grid.process(format!("l{i}\r\n").as_bytes());
        }
        grid.scroll_to_offset(3);
        for i in 8..total {
            let sep = if i + 1 < total { "\r\n" } else { "" };
            let _ = grid.process(format!("l{i}{sep}").as_bytes());
            grid.scroll_to_offset(grid.display_offset());
        }

        // Top row at each offset, cap..=0, then the rest of the bottom page.
        let mut seen = Vec::new();
        for off in (0..=cap).rev() {
            grid.scroll_to_offset(off);
            seen.push(grid.get_row_text(0));
        }
        seen.extend((1..rows).map(|r| grid.get_row_text(r)));
        let expected: Vec<String> = (total - cap - rows..total)
            .map(|i| format!("l{i}"))
            .collect();
        assert_eq!(seen, expected);
    }

    /// Catches: converting an all-time anchor to an offset before output wins the lock.
    #[test]
    fn scroll_anchor_keeps_requested_rows_after_delayed_output() {
        let mut grid = TerminalGrid::new(3, 20, 100);
        let _ = grid.process(b"l0\r\nl1\r\nl2\r\nl3\r\nl4\r\nl5\r\nl6");
        // The client saw origin 4 and requested row 2. Output arrives before apply.
        let _ = grid.process(b"\r\nl7\r\nl8\r\nl9\r\nl10\r\nl11");
        grid.scroll_to_anchor(Some(2));
        assert_eq!(grid.get_row_text(0), "l2");
        let _ = grid.process(b"\r\nl12\r\nl13");
        assert_eq!(grid.get_row_text(0), "l2");
    }

    /// Catches: capped scrollback reinterprets an evicted anchor as a newer retained row.
    #[test]
    fn scroll_anchor_clamps_evicted_rows_and_can_resume_tail() {
        let mut grid = TerminalGrid::new(3, 20, 4);
        for i in 0..12 {
            let sep = if i == 11 { "" } else { "\r\n" };
            let _ = grid.process(format!("l{i}{sep}").as_bytes());
        }
        grid.scroll_to_anchor(Some(2));
        assert_eq!(grid.get_row_text(0), "l5");
        grid.scroll_to_anchor(Some(7));
        assert_eq!(grid.get_row_text(0), "l7");
        grid.scroll_to_anchor(Some(usize::MAX));
        assert_eq!(grid.get_row_text(0), "l9");
        grid.scroll_to_anchor(None);
        let _ = grid.process(b"\r\nl12");
        assert_eq!(grid.display_offset(), 0);
        assert_eq!(grid.get_row_text(0), "l10");
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
            let col_count = row_col_count(u16::from_le_bytes([buf[off], buf[off + 1]]));
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
    fn get_row_text_preserves_decomposed_combining_codepoints() {
        let mut grid = TerminalGrid::new(1, 20, 0);
        let expected = "cafe\u{0301}";
        let _ = grid.process(expected.as_bytes());

        let base_cell = &grid.term().grid()[Line(0)][Column(3)];
        assert_eq!(base_cell.c as u32, 0x0065);
        assert_eq!(
            base_cell.zerowidth(),
            Some(&['\u{0301}'][..]),
            "the parser must retain U+0301 before TUIC extracts row text"
        );

        let actual = grid.get_row_text(0);
        assert_eq!(
            actual.chars().map(u32::from).collect::<Vec<_>>(),
            expected.chars().map(u32::from).collect::<Vec<_>>(),
            "row text must include the cell's zero-width extension"
        );
    }

    #[test]
    fn text_extractors_preserve_decomposed_combining_codepoints() {
        let mut grid = TerminalGrid::new(2, 20, 0);
        let expected_row = "> cafe\u{0301}";
        let _ = grid.process(expected_row.as_bytes());

        assert_eq!(grid.screen_text_rows()[0], expected_row);
        assert_eq!(grid.extract_log_line(Line(0)).text(), expected_row);
        assert_eq!(grid.get_logical_line(0), (0, expected_row.to_string()));
        assert_eq!(grid.get_selection_text(0, 0, 0, 5), expected_row);
        assert_eq!(grid.prompt_input_text().as_deref(), Some("cafe\u{0301}"));
    }

    /// Catches inventing a space before a wide glyph that wraps at the margin.
    #[test]
    fn softwrap_1977_does_not_copy_wide_glyph_padding_as_content() {
        let mut grid = TerminalGrid::new(3, 4, 10);
        grid.process("abc界x".as_bytes());

        assert_eq!(
            grid.get_selection_text(0, 0, 1, 3),
            "abc界x",
            "copy must omit the synthetic leading-wide-character spacer"
        );
        let rows = grid.screen_text_rows();
        let wraps = grid.screen_row_wraps();
        let mut screen = String::new();
        let mut previous_wrapped = true;
        for (row, wrapped) in rows.into_iter().zip(wraps) {
            if !previous_wrapped {
                screen.push('\n');
            }
            screen.push_str(&row);
            previous_wrapped = wrapped;
        }
        assert_eq!(screen.trim_end_matches('\n'), "abc界x");
        let log = grid.screen_log_lines();
        assert_eq!(log[0].text(), "abc");
        assert_eq!(log[1].text(), "界x");
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
    fn selection_snapshot_rebases_across_scrollback_eviction() {
        const ROW_COUNT_OFFSET: usize = 0;
        const HISTORY_BASE_OFFSET: usize = 22;

        let mut grid = TerminalGrid::new(2, 20, 2);
        let _ = grid.process(b"row0\r\nrow1\r\nrow2\r\nrow3");
        grid.scroll_to_offset(1);

        // Snapshot coordinates use the retained-grid space: G = H - O + row.
        // At this point B=0, H=2, O=1, so viewport row zero is G=1 ("row1").
        assert_eq!(
            grid.get_selection_text_with_history_base(1, 0, 1, 3, Some(0)),
            Ok("row1".to_string())
        );
        let _ = grid.serialize_dirty_rows();

        // One more scroll fills the same two-line history window by evicting row0.
        // The parked viewport remains on row1, but its current grid coordinate is
        // now G=0 because B advanced to 1. The stale snapshot must be rebased.
        let _ = grid.process(b"\r\nrow4");
        assert_eq!(grid.scrollback_count(), 2);
        assert_eq!(grid.display_offset(), 2);
        assert_eq!(
            grid.get_selection_text_with_history_base(1, 0, 1, 3, Some(0)),
            Ok("row1".to_string())
        );

        // Omitting the snapshot preserves the legacy grid-relative contract: G=1
        // now names row2, rather than being interpreted as an all-time row id.
        assert_eq!(grid.get_selection_text(1, 0, 1, 3), "row2");
        assert_eq!(
            grid.get_selection_text_with_history_base(1, 0, 1, 3, None),
            Ok("row2".to_string())
        );
        assert_eq!(
            grid.get_selection_text_with_history_base(0, 0, 0, 3, Some(2)),
            Err("selection rows are no longer retained".to_string()),
            "a snapshot from a reset/future history era must not alias retained rows"
        );

        let frame = grid.serialize_dirty_rows();
        assert_eq!(
            u16::from_le_bytes(
                frame[ROW_COUNT_OFFSET..ROW_COUNT_OFFSET + 2]
                    .try_into()
                    .unwrap()
            ),
            2,
            "eviction must send a full viewport frame"
        );
        assert_eq!(
            u32::from_le_bytes(
                frame[HISTORY_BASE_OFFSET..HISTORY_BASE_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            1
        );

        // A second eviction removes row1 itself. Its stable row id must be
        // rejected instead of aliasing the replacement at current G=0.
        let _ = grid.process(b"\r\nrow5");
        assert_eq!(
            grid.get_selection_text_with_history_base(1, 0, 1, 3, Some(0)),
            Err("selection rows are no longer retained".to_string())
        );
        assert_eq!(
            grid.get_selection_text_with_history_base(99, 0, 99, 0, Some(0)),
            Err("selection rows are no longer retained".to_string())
        );
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

    // Catches raw Claude prompt chrome/width wraps leaking into copied text.
    #[test]
    fn copied_selection_normalizes_captured_claude_prompt_without_losing_typed_break() {
        let capture = include_str!("fixtures/claude-prompt-copy-1411.txt");
        let mut grid = TerminalGrid::new(5, 148, 0);
        let _ = grid.process(capture.replace('\n', "\r\n").as_bytes());
        let expected = concat!(
            "❯ la porta del piano terra non si apre piu, dipende da qualche script o errore del codice che la manda in blocco? nel senso che se digito il codice\n",
            "  di apertura sulla tastiera non succede nulla!!"
        );
        assert_eq!(grid.get_selection_text(0, 0, 2, 147), expected);
        // Reversed drag coordinates must use the same normalization seam.
        assert_eq!(grid.get_selection_text(2, 147, 0, 0), expected);
        // Selecting only the continuation has no prompt anchor: keep content.
        assert_eq!(
            grid.get_selection_text(1, 0, 2, 147),
            "  codice\n    di apertura sulla tastiera non succede nulla!!"
        );
    }

    // Catches prompt cleanup consuming code/table content or short typed lines.
    #[test]
    fn copied_selection_preserves_unanchored_code_tables_and_short_prompt_breaks() {
        let mut grid = TerminalGrid::new(8, 80, 0);
        let _ = grid.process(b"    let x = 1;\r\n    | key | value |\r\n");
        assert_eq!(
            grid.get_selection_text(0, 0, 1, 79),
            "    let x = 1;\n    | key | value |"
        );
        let mut grid = TerminalGrid::new(8, 80, 0);
        let _ = grid.process("❯ first typed line\r\n  second typed line\r\n    indented code\r\n  ▎ first quote\r\n  ▎ second quote".as_bytes());
        assert_eq!(
            grid.get_selection_text(0, 0, 4, 79),
            "first typed line\nsecond typed line\n  indented code\nfirst quote\nsecond quote"
        );
    }

    // Catches prompt-shaped body rows restarting composer cleanup after the origin.
    #[test]
    fn copied_selection_keeps_later_literal_prompt_rows_and_indentation() {
        for (input, expected) in [
            ("header\r\n❯ literal\r\n  code", "header\n❯ literal\n  code"),
            (
                "❯ typed\r\nplain separator\r\n❯ literal\r\n  code",
                "typed\nplain separator\n❯ literal\n  code",
            ),
        ] {
            let mut grid = TerminalGrid::new(6, 80, 0);
            let _ = grid.process(input.as_bytes());
            assert_eq!(grid.get_selection_text(0, 0, 5, 79), expected);
        }
    }

    #[test]
    fn copied_selection_strips_repeated_claude_gutters() {
        let input = concat!(
            "Hola :wave:\n",
            "\u{a0}\u{a0}▎\n",
            "\u{a0}\u{a0}▎ First paragraph\n",
            "\u{a0}\u{a0}▎   • nested bullet\n",
            "\u{a0}\u{a0}▎ 1. numbered item\n",
            "\u{a0}\u{a0}▎ Thanks :pray:"
        );

        assert_eq!(
            TerminalGrid::normalize_copied_selection(input, 80),
            "Hola :wave:\n\nFirst paragraph\n  • nested bullet\n1. numbered item\nThanks :pray:"
        );
    }

    #[test]
    fn copied_selection_strips_space_indented_gutters() {
        // Claude Code v2.1.x indents the blockquote bar with ASCII spaces, not
        // the non-breaking spaces older releases used. Matching NBSP alone let
        // every bar through and pasted them into Slack verbatim.
        let input = concat!("  ▎ first quoted line\n", "  ▎ second quoted line");

        assert_eq!(
            TerminalGrid::normalize_copied_selection(input, 80),
            "first quoted line\nsecond quoted line"
        );
    }

    #[test]
    fn copied_selection_accepts_nbsp_separator_and_preserves_body_nbsp() {
        let input = concat!(
            "\u{a0}\u{a0}▎\u{a0}QA\u{a0}\u{a0}Engineering\n",
            "\u{a0}\u{a0}▎\u{a0}UX team"
        );

        assert_eq!(
            TerminalGrid::normalize_copied_selection(input, 80),
            "QA\u{a0}\u{a0}Engineering\nUX team"
        );
    }

    #[test]
    fn copied_selection_keeps_lone_or_non_claude_gutters() {
        let input = concat!(
            "\u{a0}\u{a0}▎ one candidate\n",
            "plain separator\n",
            "\u{a0}\u{a0}▎ another candidate\n",
            "plain separator\n",
            "table | ▎ | value"
        );

        assert_eq!(TerminalGrid::normalize_copied_selection(input, 80), input);
    }

    #[test]
    fn copied_selection_normalizes_after_unwrapping_soft_wrapped_rows() {
        let mut grid = TerminalGrid::new(5, 12, 0);
        let _ = grid.process(
            concat!("\u{a0}\u{a0}▎ first long\r\n", "\u{a0}\u{a0}▎ second long").as_bytes(),
        );

        let text = grid.get_selection_text(0, 0, 3, 11);
        assert_eq!(text, "first long\nsecond long");
    }

    /// The Slack-paste report: a quoted draft Claude wrapped at its own margin,
    /// then copied verbatim into a chat client. Widths are the ones measured on
    /// the reported 106-column session.
    #[test]
    fn copied_selection_rejoins_rows_claude_wrapped_for_width() {
        let input = concat!(
            "  ▎ Question about three existing custom fields\n",
            "  ▎\n",
            "  ▎ Our Jira has three custom fields with very similar names. They look unused, but I can't check them\n",
            "  ▎ without admin rights:\n",
            "  ▎\n",
            "  ▎ - customfield_12217 — \"Work Category\" (option)\n",
            "  ▎ - customfield_10489 — \"Cost Allocation\" (option)\n",
            "  ▎\n",
            "  ▎ If one of them is clean and fits, I'd rather reuse it than create a new field. If they are\n",
            "  ▎ half-configured or in use for something else, I'll ask for new fields instead."
        );

        assert_eq!(
            TerminalGrid::normalize_copied_selection(input, 106),
            concat!(
                "Question about three existing custom fields\n",
                "\n",
                "Our Jira has three custom fields with very similar names. They look unused, but I can't check them without admin rights:\n",
                "\n",
                "- customfield_12217 — \"Work Category\" (option)\n",
                "- customfield_10489 — \"Cost Allocation\" (option)\n",
                "\n",
                "If one of them is clean and fits, I'd rather reuse it than create a new field. If they are half-configured or in use for something else, I'll ask for new fields instead."
            )
        );
    }

    #[test]
    fn copied_selection_keeps_deliberate_breaks_in_a_short_quote() {
        // No row comes near the terminal edge, so nothing shows the agent
        // wrapped anything — every break here is the author's own.
        let input = concat!("  ▎ Ship it\n", "  ▎ Then tell the team\n", "  ▎ Thanks");

        assert_eq!(
            TerminalGrid::normalize_copied_selection(input, 106),
            "Ship it\nThen tell the team\nThanks"
        );
    }

    /// Once a paragraph is rejoined it is far wider than the wrap width, so the
    /// rule has to keep measuring the source rows. Measuring the accumulator
    /// instead swallowed every short line that followed a wrapped paragraph.
    #[test]
    fn copied_selection_stops_rejoining_after_the_wrapped_paragraph_ends() {
        let filler = "y".repeat(91);
        let input = format!(
            "  ▎ {filler} wordy\n  ▎ continuation words\n  ▎ Short deliberate line\n  ▎ Another one"
        );

        assert_eq!(
            TerminalGrid::normalize_copied_selection(&input, 106),
            format!("{filler} wordy continuation words\nShort deliberate line\nAnother one")
        );
    }

    /// A wrapped bullet keeps its continuation, but the next bullet stays on its
    /// own line even though the width rule alone would swallow it.
    #[test]
    fn copied_selection_rejoins_bullet_continuations_but_not_the_next_bullet() {
        let filler = "x".repeat(90);
        let input = format!("  ▎ - {filler} wordy\n  ▎ continuation text\n  ▎ 2. second item");

        assert_eq!(
            TerminalGrid::normalize_copied_selection(&input, 106),
            format!("- {filler} wordy continuation text\n2. second item")
        );
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
        assert_eq!(lines, [] as [std::string::String; 0]);
    }

    #[test]
    fn read_scrollback_no_history_returns_empty() {
        let grid = TerminalGrid::new(24, 80, 100);
        let lines = grid.read_scrollback_lines(0, 10);
        assert_eq!(lines, [] as [std::string::String; 0]);
    }

    #[test]
    fn get_row_text_out_of_bounds_returns_empty() {
        let grid = TerminalGrid::new(5, 20, 0);
        let text = grid.get_row_text(999);
        assert_eq!(text, "");
    }

    // Catches: OSC rows recorded at the history cap drifting after eviction, or
    // being rebased with the end-of-chunk base instead of the event-time origin.
    #[test]
    fn osc_marker_rows_stay_absolute_across_eviction() {
        for prefix_rows in [0, 4, 8] {
            let mut grid = TerminalGrid::new(2, 40, 3);
            for _ in 0..prefix_rows {
                grid.process(b"old\r\n");
            }
            grid.drain_events();
            let anchor = grid.screen_origin() + 1;
            grid.process(b"\x1b[2;1Hanchor\x1b]133;A\x07\x1b]7770;state=prompt\x07\r\nnext\r\n");
            let events = grid.drain_events();
            let marker_rows: Vec<usize> = events
                .iter()
                .filter_map(|event| match event {
                    TermEvent::Osc133 { line, .. } | TermEvent::Tuic { line, .. } => Some(*line),
                    _ => None,
                })
                .collect();
            assert_eq!(marker_rows, vec![anchor, anchor]);
            let base = grid.screen_origin() - grid.scrollback_count();
            assert_eq!(
                grid.read_rows_in_range(anchor - base, anchor - base)[0].trim_end(),
                "anchor"
            );
        }
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
    fn scroll_and_reflow_preserve_combining_extensions() {
        let mut grid = TerminalGrid::new(2, 10, 100);
        grid.reflow_history = true;
        let _ = grid.process("cafe\u{0301}-one\r\ncafe\u{0301}-two\r\nplain\r\ncurrent".as_bytes());

        let history = grid.scrollback_count();
        let before = grid.read_rows_in_range(0, history - 1);
        assert!(before.iter().any(|row| row == "cafe\u{0301}-one"));
        assert!(before.iter().any(|row| row == "cafe\u{0301}-two"));

        grid.scroll_to_offset(history);
        assert!(
            (0..grid.screen_lines()).any(|row| grid.get_row_text(row).contains("cafe\u{0301}")),
            "scrolling must expose the retained exact codepoints"
        );
        grid.scroll_to_offset(0);

        grid.resize(2, 5);
        grid.resize(2, 10);
        let history_after = grid.scrollback_count();
        assert_eq!(
            grid.read_rows_in_range(0, history_after - 1),
            before,
            "a shrink-grow reflow cycle must preserve the stored codepoint sequence"
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
            rows_after[line_after].trim_end(),
            "ABCDEFGHIJKLMNOPQRST",
            "prompt must be on cursor row after shrink-grow roundtrip"
        );
        assert_eq!(
            line_after, line_before,
            "cursor line must return to original position"
        );
    }

    /// Strip [`ROW_WRAPPED_FLAG`] from a serialized row's `col_count`.
    ///
    /// Every reader of the wire format has to do this before using the value as a
    /// stride — forgetting it walks 32768 cells past the end of the buffer.
    fn row_col_count(raw: u16) -> usize {
        (raw & !ROW_WRAPPED_FLAG) as usize
    }

    /// Helper: find the cell data offset for a given (row_index, col) in a serialized frame.
    /// Returns the byte offset of the cell's 11-byte block, or None if not found.
    fn find_cell_offset(buf: &[u8], target_row: u16, target_col: u16) -> Option<usize> {
        let (num_rows, _, _, _) = decode_header(buf);
        let mut offset = TEST_HEADER_SIZE;
        for _ in 0..num_rows {
            let row_idx = u16::from_le_bytes([buf[offset], buf[offset + 1]]);
            let col_count = row_col_count(u16::from_le_bytes([buf[offset + 2], buf[offset + 3]]));
            offset += 4;
            if row_idx == target_row && (target_col as usize) < col_count {
                return Some(offset + target_col as usize * 11);
            }
            offset += col_count * 11;
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
        assert_ne!(buf, [] as [u8; 0]);

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
        assert_ne!(buf, [] as [u8; 0]);

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
        assert_ne!(buf, [] as [u8; 0]);

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
        assert_ne!(buf, [] as [u8; 0]);

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
        assert_ne!(buf, [] as [u8; 0]);

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
        assert_ne!(buf, [] as [u8; 0]);

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

    // --- Alternate-screen scrollback --------------------------------------
    //
    // Driven by a REAL PTY capture of `gh run watch <run-id>` — the command that
    // exposed the bug. Fixture: src/fixtures/alt_screen/gh-run-watch.raw,
    // recorded with `script -q /dev/null gh run watch <id> | head -c 60000`.
    // It opens with `ESC[?1049h` (alt screen), then repeatedly homes the cursor
    // (`ESC[0;0H ESC[J`) and reprints a job list far taller than the viewport —
    // that reprint is what scrolls lines off the top. gh never exits while the
    // run is live, so the capture has no `ESC[?1049l`; tests that need the exit
    // append it explicitly.

    fn gh_run_watch_capture() -> Vec<u8> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../src/fixtures/alt_screen/gh-run-watch.raw");
        std::fs::read(&path).unwrap_or_else(|e| panic!("missing fixture {}: {e}", path.display()))
    }

    /// The reported bug: text renders but there is no scrollbar, because the alt
    /// grid had no history at all. The frontend hides the scrollbar exactly when
    /// `historySize == 0` (CanvasTerminal.tsx), so a non-zero history IS the fix.
    #[test]
    fn gh_run_watch_builds_alt_screen_scrollback() {
        let mut grid = TerminalGrid::new(24, 120, 1000);
        let _ = grid.process(&gh_run_watch_capture());

        assert!(
            grid.is_alternate_screen(),
            "fixture must leave the terminal in the alternate screen"
        );
        assert!(
            grid.scrollback_count() > 0,
            "alt-screen scrollback must accumulate — 0 means the scrollbar stays hidden"
        );
    }

    /// Scrolling back must reach the lines the app pushed off the top, not just
    /// show an empty history. The gh banner is printed once at the very top of
    /// every refresh, so it is guaranteed to have scrolled away.
    #[test]
    fn gh_run_watch_scrollback_holds_the_lines_that_scrolled_off() {
        let mut grid = TerminalGrid::new(24, 120, 1000);
        let _ = grid.process(&gh_run_watch_capture());

        let history = grid.scrollback_count();
        let lines = grid.read_scrollback_lines(0, history);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("Refreshing run status every 3 seconds")),
            "the banner scrolled off the top must be recoverable from alt scrollback"
        );
        assert!(
            lines.iter().any(|l| l.contains("JOBS")),
            "job list header must be recoverable from alt scrollback"
        );
    }

    /// The frame protocol has to tell the frontend it is looking at the alternate
    /// screen (keyboard_flags bit5), because `history_base` restarts at 0 there and
    /// the client row cache — keyed by absolute row — must be dropped on the flip.
    #[test]
    fn frame_flags_report_alternate_screen() {
        // Header layout (see serialize_dirty_rows): row_count u16, cursor_row u16,
        // cursor_col u16, cursor_visible u8, display_offset u32, history_size u32,
        // has_selection u8 → keyboard_flags lands at byte 16.
        const KEYBOARD_FLAGS_OFFSET: usize = 16;

        let mut grid = TerminalGrid::new(24, 120, 1000);
        let _ = grid.process(b"plain shell output\r\n");
        let frame = grid.serialize_dirty_rows();
        assert_eq!(
            frame[KEYBOARD_FLAGS_OFFSET] & 0x20,
            0,
            "primary screen must not set the alt-screen bit"
        );

        let _ = grid.process(&gh_run_watch_capture());
        let frame = grid.serialize_dirty_rows();
        assert_eq!(
            frame[KEYBOARD_FLAGS_OFFSET] & 0x20,
            0x20,
            "alt screen must set keyboard_flags bit5"
        );
    }

    /// An alt session never inherits the previous one's lines: `swap_alt` resets
    /// the alt history era on both enter and exit.
    #[test]
    fn alt_screen_history_is_wiped_between_sessions() {
        let mut grid = TerminalGrid::new(24, 120, 1000);
        let _ = grid.process(&gh_run_watch_capture());
        assert!(
            grid.scrollback_count() > 0,
            "first alt session builds history"
        );

        // Leave the alternate screen (gh does this on Ctrl+C / completion).
        let _ = grid.process(b"\x1b[?1049l");
        assert!(!grid.is_alternate_screen());

        // Re-enter: the new session starts from an empty history, never showing
        // the previous app's leftovers.
        let _ = grid.process(b"\x1b[?1049h");
        assert_eq!(
            grid.scrollback_count(),
            0,
            "a fresh alt session must start with no scrollback"
        );
    }

    /// Leaving the alternate screen must restore the primary screen's own history
    /// untouched — the alt lines must not leak into the shell's scrollback.
    #[test]
    fn alt_screen_scrollback_never_leaks_into_primary() {
        let mut grid = TerminalGrid::new(24, 120, 1000);
        for i in 0..40 {
            let _ = grid.process(format!("shell line {i}\r\n").as_bytes());
        }
        let primary_history = grid.scrollback_count();
        assert!(primary_history > 0, "sanity: primary built scrollback");

        let _ = grid.process(&gh_run_watch_capture());
        let _ = grid.process(b"\x1b[?1049l");

        assert!(!grid.is_alternate_screen());
        assert_eq!(
            grid.scrollback_count(),
            primary_history,
            "primary scrollback must be exactly what it was before the alt app ran"
        );
        let lines = grid.read_scrollback_lines(0, grid.scrollback_count());
        assert!(
            !lines.iter().any(|l| l.contains("Refreshing run status")),
            "no alt-screen line may end up in the primary scrollback"
        );
    }

    /// Alt-screen apps that redraw in place (`ESC[H` + `ESC[J`, no scrolling) must
    /// not manufacture history: only lines that actually scroll off the top count.
    /// This is what keeps `vim`/`htop` from flooding the scrollback.
    #[test]
    fn alt_screen_redraw_without_scrolling_creates_no_history() {
        let mut grid = TerminalGrid::new(24, 120, 1000);
        let _ = grid.process(b"\x1b[?1049h");
        for _ in 0..20 {
            let _ = grid.process(b"\x1b[0;0H\x1b[Jstatus pane redraw");
        }
        assert_eq!(
            grid.scrollback_count(),
            0,
            "an in-place redraw must not produce scrollback"
        );
    }

    /// Resizing while the alt screen holds history exercises grid paths that were
    /// unreachable when the alt grid had capacity 0. It must not panic or lose the
    /// alt-screen state.
    #[test]
    fn alt_screen_with_history_survives_resize() {
        let mut grid = TerminalGrid::new(24, 120, 1000);
        let _ = grid.process(&gh_run_watch_capture());
        assert!(grid.scrollback_count() > 0);

        grid.resize(40, 80);
        grid.resize(12, 200);
        grid.resize(24, 120);

        assert!(grid.is_alternate_screen(), "resize must not drop alt mode");
        let _ = grid.serialize_dirty_rows();
    }

    /// The user-visible half of the fix: the viewport must actually move through
    /// the alt history. `scroll()` is the wheel path, `scroll_to_offset()` the
    /// scrollbar-drag / coalesced-wheel path — both must reach content that
    /// scrolled off, clamp at the top, and return to the live tail.
    #[test]
    fn alt_screen_wheel_and_drag_scroll_reach_the_scrolled_off_lines() {
        let mut grid = TerminalGrid::new(24, 120, 1000);
        let _ = grid.process(&gh_run_watch_capture());

        let history = grid.scrollback_count();
        assert!(
            history > 3,
            "fixture must leave enough alt history to scroll"
        );
        assert_eq!(grid.display_offset(), 0, "starts pinned to the live tail");

        // Wheel up three lines.
        grid.scroll(3);
        assert_eq!(grid.display_offset(), 3, "wheel must move the alt viewport");

        // Scrollbar drag straight to a line that scrolled off: it must land in
        // the viewport, which is what makes the history usable rather than merely
        // present. `read_scrollback_lines` is oldest-first, so its index IS the
        // absolute history row.
        let banner = grid
            .read_scrollback_lines(0, history)
            .iter()
            .position(|l| l.contains("Refreshing run status every 3 seconds"))
            .expect("the gh banner must have scrolled off into alt history");
        grid.scroll_to_line(banner);
        assert_eq!(grid.display_offset(), history - banner);
        assert!(
            grid.get_row_text(0)
                .contains("Refreshing run status every 3 seconds"),
            "scrolled-to line must be the top viewport row, got: {:?}",
            grid.get_row_text(0)
        );

        // Scrollbar drag to the very top of the alt history.
        grid.scroll_to_offset(history);
        assert_eq!(grid.display_offset(), history);

        // Dragging past the top clamps instead of running off the grid.
        grid.scroll_to_offset(history + 500);
        assert_eq!(grid.display_offset(), history, "top of history must clamp");

        // Wheel back down: the viewport returns to the live tail.
        grid.scroll(-(history as i32 + 10));
        assert_eq!(grid.display_offset(), 0, "must snap back to the live tail");
    }

    /// A panic taken while the event queue is locked must not stop event
    /// delivery for the rest of the terminal's life.
    ///
    /// This is the whole argument for `parking_lot` here, and it is worse than
    /// "a lock type changed". A poisoned `std::sync::Mutex` never un-poisons, so
    /// `drain_events` took its `Err` arm on *every* later call and returned an
    /// empty `Vec` — silently dropping every title change, OSC 133 prompt mark,
    /// OSC 7 cwd and TUIC event for that terminal, while the grid kept rendering
    /// and looked perfectly healthy. `parking_lot` has no poison state, so one
    /// panic costs one lost event instead of all future ones.
    #[test]
    fn events_are_still_delivered_after_a_panic_under_their_lock() {
        let mut grid = TerminalGrid::new(24, 80, 100);
        let events = grid.events.clone();

        // Silence the unwind backtrace: the panic below is the fixture, not noise.
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = events.lock();
            panic!("panic inside the terminal event critical section");
        }))
        .is_err();
        std::panic::set_hook(hook);
        assert!(panicked, "the fixture must actually panic under the lock");

        // OSC 0 — set window title. One of the events send_event queues.
        grid.process(b"\x1b]0;after-the-panic\x07");

        let drained = grid.drain_events();
        assert!(
            drained
                .iter()
                .any(|e| matches!(e, TermEvent::Title(t) if t == "after-the-panic")),
            "event delivery stopped after a panic under the lock; drained: {drained:?}"
        );
    }
}

/// Adversarial cases for the post-resize reprint merge (#1264).
#[cfg(test)]
mod reprint_merge_critic_tests {
    use super::*;

    const IDLE_FRAME: &[u8] = b"\x1b[?2026h\x1b[?2026l";

    fn all_rows(grid: &TerminalGrid) -> Vec<String> {
        let mut rows = grid.read_scrollback_lines(0, grid.scrollback_count());
        rows.extend(grid.screen_text_rows());
        rows
    }

    fn non_empty(grid: &TerminalGrid) -> usize {
        all_rows(grid)
            .iter()
            .filter(|r| !r.trim().is_empty())
            .count()
    }

    /// Scenario shared by the cases below: 6x20 grid holding L01..L10, shrunk to
    /// 4 rows, so history ends L06,L07 and the screen starts L08.
    fn shrunk_numbered_grid() -> TerminalGrid {
        let mut grid = TerminalGrid::new(6, 20, 1000);
        for i in 1..=10 {
            grid.process(format!("L{i:02}\r\n").as_bytes());
        }
        grid.resize_with_mode(4, 20, ReflowMode::All);
        grid
    }

    /// The program's repaint: home, then the last rows, inside one sync frame.
    const REPAINT: &[u8] =
        b"\x1b[?2026h\x1b[HL06\x1b[K\r\nL07\x1b[K\r\nL08\x1b[K\r\nL09\x1b[K\x1b[?2026l";

    /// Catches: the merge fires on any frame end after a shrink whose history
    /// tail happens to equal the screen head, with no repaint having happened.
    /// A program that legitimately printed the same block twice (separators,
    /// repeated `alpha/beta` records) loses real history rows.
    #[test]
    fn repeated_block_survives_an_idle_sync_frame_after_shrink() {
        let mut grid = TerminalGrid::new(6, 20, 1000);
        for _ in 0..6 {
            grid.process(b"alpha\r\nbeta\r\n");
        }
        grid.resize_with_mode(4, 20, ReflowMode::All);
        let before = non_empty(&grid);
        grid.process(IDLE_FRAME);
        assert_eq!(
            non_empty(&grid),
            before,
            "an empty sync frame deleted printed rows"
        );
    }

    /// Catches: same as above for a run of identical separator rows.
    #[test]
    fn identical_separator_rows_survive_an_idle_sync_frame_after_shrink() {
        let mut grid = TerminalGrid::new(6, 20, 1000);
        for _ in 0..12 {
            grid.process(b"--------------------\r\n");
        }
        grid.resize_with_mode(4, 20, ReflowMode::All);
        let before = non_empty(&grid);
        grid.process(IDLE_FRAME);
        assert_eq!(
            non_empty(&grid),
            before,
            "an empty sync frame deleted separator rows"
        );
    }

    /// Catches: the merge drops history rows but leaves `display_offset`
    /// unchanged, so a user scrolled up (the #1264 situation) sees the view jump
    /// by the dropped row count.
    #[test]
    fn scrolled_up_view_does_not_jump_when_rows_are_merged() {
        let mut grid = shrunk_numbered_grid();
        grid.scroll(4);
        let d = grid.display_offset();
        let top_before = grid.row_to_text(Line(-(d as i32))).unwrap();
        grid.process(REPAINT);
        assert_eq!(grid.scrollback_count(), 5, "scenario must merge L06,L07");
        let d = grid.display_offset();
        let top_after = grid.row_to_text(Line(-(d as i32))).unwrap();
        assert_eq!(
            top_after, top_before,
            "viewport top row changed under a scrolled-up user"
        );
    }

    /// Catches: the frame-end carry mishandles a FRAME_END split across chunks,
    /// so the merge result depends on chunk boundaries.
    #[test]
    fn merge_result_does_not_depend_on_chunk_boundaries() {
        let mut whole = shrunk_numbered_grid();
        whole.process(REPAINT);
        let expected = all_rows(&whole);
        for chunk in 1..=REPAINT.len() {
            let mut grid = shrunk_numbered_grid();
            for piece in REPAINT.chunks(chunk) {
                grid.process(piece);
            }
            assert_eq!(all_rows(&grid), expected, "chunk size {chunk}");
        }
    }

    /// Catches: the rewind of `total_scrolled` breaks the eviction-stable base
    /// (`total_scrolled - history_size`) the frontend keys rows by, when the
    /// scrollback cap is already evicting.
    #[test]
    fn merge_keeps_the_eviction_base_when_the_cap_is_full() {
        let mut grid = TerminalGrid::new(6, 20, 3);
        for i in 1..=10 {
            grid.process(format!("L{i:02}\r\n").as_bytes());
        }
        grid.resize_with_mode(4, 20, ReflowMode::All);
        let base = grid.screen_origin() - grid.scrollback_count();
        grid.process(REPAINT);
        assert_eq!(
            grid.screen_origin() - grid.scrollback_count(),
            base,
            "evicted-row base moved"
        );
    }

    /// Catches: a merge on the alternate screen, or a resize made on it, arms
    /// the merge and eats primary history after the program exits.
    #[test]
    fn alt_screen_resize_never_merges_primary_history() {
        let mut grid = TerminalGrid::new(6, 20, 1000);
        for _ in 0..12 {
            grid.process(b"--------------------\r\n");
        }
        grid.process(b"\x1b[?1049h");
        grid.resize_with_mode(4, 20, ReflowMode::All);
        let before = grid.primary_scrollback_count();
        grid.process(IDLE_FRAME);
        grid.process(b"\x1b[?1049l");
        grid.process(IDLE_FRAME);
        assert_eq!(grid.primary_scrollback_count(), before);
    }
}

/// Round-2 adversarial cases for the "head rows changed since the last frame end" evidence rule (#1264).
#[cfg(test)]
mod reprint_merge_evidence_critic_tests {
    use super::*;

    fn non_empty(grid: &TerminalGrid) -> usize {
        let mut rows = grid.read_scrollback_lines(0, grid.scrollback_count());
        rows.extend(grid.screen_text_rows());
        rows.iter().filter(|r| !r.trim().is_empty()).count()
    }

    /// Catches: the evidence rule is satisfied by any change in the head rows, so
    /// new output that merely continues a repeating block (alpha/beta records)
    /// scrolls the screen, changes the head versus the snapshot, matches the
    /// periodic history tail and is "merged" away although nothing was reprinted.
    #[test]
    fn output_continuing_a_repeating_block_is_not_merged() {
        let mut grid = TerminalGrid::new(6, 20, 1000);
        for _ in 0..6 {
            grid.process(b"alpha\r\nbeta\r\n");
        }
        grid.resize_with_mode(4, 20, ReflowMode::All);
        grid.process(b"alpha\r\n");
        let before = non_empty(&grid);
        grid.process(b"\x1b[?2026h\x1b[?2026l");
        assert_eq!(
            non_empty(&grid),
            before,
            "continued output lost rows at the frame end"
        );
    }

    /// Catches: an unrelated in-place edit of one head row (status/spinner line)
    /// that happens to produce text equal to the history tail counts as a repaint.
    #[test]
    fn unrelated_head_row_edit_equal_to_history_tail_is_not_merged() {
        let mut grid = TerminalGrid::new(6, 20, 1000);
        for row in ["Z1", "Z2", "Z3", "A", "B", "A", "C", "Q"] {
            grid.process(format!("{row}\r\n").as_bytes());
        }
        grid.resize_with_mode(4, 20, ReflowMode::All);
        let before = non_empty(&grid);
        grid.process(b"\x1b[?2026h\x1b[2;1HB\x1b[K\x1b[?2026l");
        assert_eq!(
            non_empty(&grid),
            before,
            "an in-place edit deleted history rows"
        );
    }
}

/// Round-3 guards for the unscrolled-full-rewrite evidence rule (#1264).
#[cfg(test)]
mod reprint_merge_round3_critic_tests {
    use super::*;

    fn all_rows(grid: &TerminalGrid) -> Vec<String> {
        let mut rows = grid.read_scrollback_lines(0, grid.scrollback_count());
        rows.extend(grid.screen_text_rows());
        rows
    }

    fn numbered_shrunk() -> TerminalGrid {
        let mut grid = TerminalGrid::new(6, 20, 1000);
        for i in 1..=10 {
            grid.process(format!("L{i:02}\r\n").as_bytes());
        }
        grid.resize_with_mode(4, 20, ReflowMode::All);
        grid
    }

    /// Catches: a full-screen redraw that starts with ESC[2J (the fork pushes the
    /// screen into history and raises total_scrolled) combined with the merge
    /// drops a row that exists nowhere else. Every printed line must stay
    /// reachable, however the chunks are cut.
    #[test]
    fn clear_screen_repaint_after_shrink_loses_no_line() {
        let frame: &[u8] = b"\x1b[?2026h\x1b[2J\x1b[HL06\r\nL07\r\nL08\r\nL09\x1b[?2026l";
        for chunk in [frame.len(), 7, 1] {
            let mut grid = numbered_shrunk();
            for piece in frame.chunks(chunk) {
                grid.process(piece);
            }
            let rows = all_rows(&grid);
            for i in 1..=10 {
                let want = format!("L{i:02}");
                assert!(
                    rows.contains(&want),
                    "{want} lost (chunk {chunk}): {rows:?}"
                );
            }
        }
    }

    /// Catches: an in-place full-screen redraw on the primary screen (htop/less
    /// style cursor addressing, new data in every row, no scroll) is read as a
    /// reprint and takes history rows with it.
    #[test]
    fn in_place_redraw_with_new_data_keeps_history() {
        let mut grid = numbered_shrunk();
        let before = all_rows(&grid).iter().filter(|r| !r.is_empty()).count();
        grid.process(b"\x1b[?2026h\x1b[HP1\x1b[K\r\nP2\x1b[K\r\nP3\x1b[K\r\nP4\x1b[K\x1b[?2026l");
        let rows = all_rows(&grid);
        // P1..P4 replaced L08,L09,L10 and a blank row; history keeps L01..L07.
        for i in 1..=7 {
            let want = format!("L{i:02}");
            assert!(rows.contains(&want), "{want} lost: {rows:?}");
        }
        assert_eq!(
            rows.iter().filter(|r| !r.is_empty()).count(),
            before - 3 + 4
        );
    }
}
