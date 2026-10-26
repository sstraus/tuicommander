use super::{grid_gate, terminal_grid};
use crate::chrome::find_scrollback_chrome_cutoff;
use crate::output_parser;
use alacritty_terminal::grid::ReflowMode;
use serde::Serialize;
use std::collections::VecDeque;

// ---------------------------------------------------------------------------
// VtLogBuffer — VT100-aware log extractor for mobile/REST consumers
// ---------------------------------------------------------------------------

/// Default maximum log lines retained per session.
pub const VT_LOG_BUFFER_CAPACITY: usize = 10_000;

/// Mark the agent prompt and footer rows in a batch without dropping history.
pub fn mark_agent_chrome(lines: &mut [LogLine]) {
    let texts: Vec<String> = lines.iter().map(|line| line.text()).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    if let Some(cutoff) = find_scrollback_chrome_cutoff(&refs) {
        for line in &mut lines[cutoff..] {
            line.chrome = true;
        }
    }
}

/// Terminal color extracted from vt100 cells.
///
/// Serializes as `{"idx": N}` for 256-color palette or `{"rgb": [r,g,b]}` for
/// 24-bit color.  Default color is omitted (serialized as `null` / skipped).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogColor {
    Idx(u8),
    Rgb(u8, u8, u8),
}

impl LogColor {
    pub fn from_ansi_color(c: alacritty_terminal::vte::ansi::Color) -> Option<Self> {
        use alacritty_terminal::vte::ansi::{Color, NamedColor};
        match c {
            Color::Named(n) => match n {
                NamedColor::Foreground
                | NamedColor::Background
                | NamedColor::Cursor
                | NamedColor::BrightForeground
                | NamedColor::DimForeground => None,
                NamedColor::Black => Some(LogColor::Idx(0)),
                NamedColor::Red => Some(LogColor::Idx(1)),
                NamedColor::Green => Some(LogColor::Idx(2)),
                NamedColor::Yellow => Some(LogColor::Idx(3)),
                NamedColor::Blue => Some(LogColor::Idx(4)),
                NamedColor::Magenta => Some(LogColor::Idx(5)),
                NamedColor::Cyan => Some(LogColor::Idx(6)),
                NamedColor::White => Some(LogColor::Idx(7)),
                NamedColor::BrightBlack => Some(LogColor::Idx(8)),
                NamedColor::BrightRed => Some(LogColor::Idx(9)),
                NamedColor::BrightGreen => Some(LogColor::Idx(10)),
                NamedColor::BrightYellow => Some(LogColor::Idx(11)),
                NamedColor::BrightBlue => Some(LogColor::Idx(12)),
                NamedColor::BrightMagenta => Some(LogColor::Idx(13)),
                NamedColor::BrightCyan => Some(LogColor::Idx(14)),
                NamedColor::BrightWhite => Some(LogColor::Idx(15)),
                NamedColor::DimBlack => Some(LogColor::Idx(0)),
                NamedColor::DimRed => Some(LogColor::Idx(1)),
                NamedColor::DimGreen => Some(LogColor::Idx(2)),
                NamedColor::DimYellow => Some(LogColor::Idx(3)),
                NamedColor::DimBlue => Some(LogColor::Idx(4)),
                NamedColor::DimMagenta => Some(LogColor::Idx(5)),
                NamedColor::DimCyan => Some(LogColor::Idx(6)),
                NamedColor::DimWhite => Some(LogColor::Idx(7)),
            },
            Color::Indexed(i) => Some(LogColor::Idx(i)),
            Color::Spec(rgb) => Some(LogColor::Rgb(rgb.r, rgb.g, rgb.b)),
        }
    }
}

/// A contiguous run of text with uniform formatting attributes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct LogSpan {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fg: Option<LogColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<LogColor>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub underline: bool,
}

/// A single log line composed of styled spans.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct LogLine {
    pub spans: Vec<LogSpan>,
    #[serde(skip_serializing_if = "is_zero_u16")]
    pub cols: u16,
    /// True when this line is agent UI chrome (prompt box, footer, status bar)
    /// rather than agent output. Set once, at capture time, by
    /// [`mark_agent_chrome`]; readers skip these lines instead of the buffer
    /// dropping them, so a misclassification hides text rather than destroying
    /// it. Never serialized — consumers receive the already-filtered view.
    #[serde(skip)]
    pub chrome: bool,
    /// True when the row this line ends on soft-wraps: its text continues in
    /// the next log line (or the first screen row). Lets a reader that works
    /// on text (redaction) rejoin a logical line that scrolled into the log
    /// across separate `process` calls. Serialized so mobile readers use the
    /// same exact boundary instead of guessing from row lengths.
    pub wrapped: bool,
    /// True when part of this line's logical line never reached the log (rows
    /// evicted, or scrolled off while capture was suspended). Readers cannot
    /// redact what they cannot see whole, so they hide such a line instead.
    /// Never serialized.
    #[serde(skip)]
    pub partial: bool,
}

fn is_zero_u16(v: &u16) -> bool {
    *v == 0
}

impl LogLine {
    /// Returns the plain-text content (all span texts concatenated).
    pub fn text(&self) -> String {
        let mut s = String::new();
        for span in &self.spans {
            s.push_str(&span.text);
        }
        s
    }

    /// Strip suggestion tokens from span text. Intent rows stay visible so
    /// PWA/REST consumers can tint the complete logical line, like the desktop.
    pub fn strip_structural_tokens(&mut self) {
        // The grammar lives in `output_parser`, next to the regexes that READ
        // these tokens. A second copy here knew only the two Ink bullets and
        // drifted: the parser learned Codex's `•`/`◦` and the ack prefix, this
        // did not, so tokens TUIC had consumed were still shown to the user.
        for span in &mut self.spans {
            if span.text.contains("suggest:") {
                let replaced = output_parser::strip_plain_prefix_tokens(&span.text);
                span.text = replaced.into_owned();
            }
        }
        // Remove spans that became empty after stripping
        self.spans.retain(|s| !s.text.is_empty());
    }
}

/// Strip structural tokens from a run of rows, including the wrapped tail rows
/// of a bracketed `suggest: [ … ]` that [`LogLine::strip_structural_tokens`]
/// cannot see from one row. The tail rows are dropped; the head row is emptied
/// as for any single-row token (#1380-05d8).
fn strip_structural_blocks(lines: &mut Vec<LogLine>) {
    let texts: Vec<String> = lines.iter().map(LogLine::text).collect();
    let mut tail = vec![false; lines.len()];
    for (i, head) in texts.iter().enumerate() {
        if tail[i] || !head.contains("suggest:") {
            continue;
        }
        let rows =
            output_parser::suggest_tail_rows(head, texts[i + 1..].iter().map(String::as_str));
        tail[i + 1..=i + rows].fill(true);
    }
    let mut is_tail = tail.into_iter();
    lines.retain(|_| !is_tail.next().unwrap_or(false));
    for line in lines {
        line.strip_structural_tokens();
    }
}

/// A screen row that changed after a `VtLogBuffer::process()` call.
///
/// Consumers (output parsers) iterate these to detect status lines, intent
/// tokens, and other structured events — regardless of whether the terminal
/// is in normal or alternate screen mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedRow {
    /// Zero-based row index on the visible screen.
    pub row_index: usize,
    /// Clean text content of the row (ANSI sequences stripped by the vt100 parser).
    pub text: String,
}

/// Per-session VT-aware log buffer.
///
/// Wraps a `TerminalGrid` (backed by `alacritty_terminal`) with scrollback
/// capture. Lines that scroll off the top of the screen are captured from the
/// grid's history and stored in a bounded `VecDeque<LogLine>` for REST and
/// WebSocket consumers.
///
/// **Thread safety:** Not `Sync` — lives behind a mutex in the app state.
pub struct VtLogBuffer {
    grid: terminal_grid::TerminalGrid,
    /// Finalized log lines (oldest first).
    log: VecDeque<LogLine>,
    /// Maximum number of log lines retained in our own buffer.
    capacity: usize,
    /// Whether the previous `process()` call saw the alternate screen active.
    was_alternate: bool,
    /// Primary-screen origin at the last extraction, including evicted rows.
    /// History length stops growing at the cap and cannot serve as this cursor.
    scrollback_origin: usize,
    /// Monotonically increasing count of all log lines ever pushed (not bounded
    /// by capacity). Used as stable cursor for paginated reads.
    total_pushed: usize,
    /// Widest cols seen so far. The grid never shrinks below this —
    /// prevents Ink re-renders from fragmenting scrollback mid-word.
    max_cols: u16,
    /// Actual PTY cols (what the child process / Ink sees). May be smaller
    /// than grid cols when a side panel narrows the terminal. Stamped
    /// onto LogLine.cols so the frontend can detect narrow-captured lines.
    pty_cols: u16,
    /// When true, scrollback capture is paused — a side panel just
    /// narrowed the terminal and Ink is flooding scrollback with
    /// narrow re-renders. Cleared when pty_cols widens again.
    suppress_capture: bool,
    /// The next line to be pushed continues a wrapped row that was scrolled
    /// off without being logged, so its head is missing.
    next_head_lost: bool,
    /// Caller-computed secrets of the retained log, valid while `total_pushed`
    /// and the wrap flags are unchanged.
    secret_cache: Option<(usize, Vec<String>)>,
    /// Last reader-originated visible ego composer-row change.
    last_composer_paint_at: Option<std::time::Instant>,
}

/// Internal scrollback capacity for the terminal grid. Must be large enough
/// that it never fills up between consecutive `process()` calls — in practice
/// even a `cat huge_file` sends data in ~4KB PTY read chunks.
const GRID_SCROLLBACK: usize = 10_000;

impl VtLogBuffer {
    pub fn new(rows: u16, cols: u16, capacity: usize) -> Self {
        let grid = terminal_grid::TerminalGrid::new(rows, cols, GRID_SCROLLBACK);
        Self {
            grid,
            log: VecDeque::new(),
            capacity,
            was_alternate: false,
            scrollback_origin: 0,
            total_pushed: 0,
            max_cols: cols,
            pty_cols: cols,
            suppress_capture: false,
            next_head_lost: false,
            secret_cache: None,
            last_composer_paint_at: None,
        }
    }

    /// Last child paint of the native ego composer, ordered with input stamps.
    pub fn last_composer_paint_at(&self) -> Option<std::time::Instant> {
        self.last_composer_paint_at
    }

    pub fn set_ansi_colors(&mut self, colors: &[[u8; 3]; 16]) {
        self.grid.set_ansi_colors(colors);
    }

    /// Reflow scrollback on a column resize, or truncate it.
    ///
    /// Live-settable rather than construction-only because the Settings toggle
    /// must reach sessions that already exist: a user who narrows a terminal,
    /// loses history and then finds the setting would otherwise have to restart
    /// every session for it to mean anything (#660-d087).
    pub fn set_reflow_history(&mut self, on: bool) {
        self.grid.reflow_history = on;
    }

    /// Feed reader-originated child PTY bytes into the terminal grid.
    ///
    /// Returns the screen rows that changed since the previous call.  Changed
    /// rows are detected for **both** normal and alternate screen so that
    /// output parsers can match status lines and intent tokens emitted by
    /// agents that use the alternate screen (e.g. Claude Code / Ink).
    ///
    /// Log extraction reads new primary-screen scrollback lines from the grid's
    /// history. Alternate history may exist for interactive scrolling, but it is
    /// deliberately excluded from the durable log. The same exclusion applies
    /// while mouse reporting is on the primary screen (`grok --no-alt-screen`):
    /// the app owns the viewport and its SU/line dumps are not shell output.
    pub fn process(&mut self, data: &[u8]) -> Vec<ChangedRow> {
        self.process_inner(data, true)
    }

    /// Apply local cosmetic/repair bytes, never child paint evidence.
    pub fn process_local(&mut self, data: &[u8]) -> Vec<ChangedRow> {
        self.process_inner(data, false)
    }

    fn process_inner(&mut self, data: &[u8], child_output: bool) -> Vec<ChangedRow> {
        let started_at = std::time::Instant::now();
        // Resize clears the diff cache. Compare against the actual pre-read
        // screen then, so a cursor-only first read cannot invent text damage.
        let uncached_rows = self
            .grid
            .screen_text_rows_ref()
            .is_none()
            .then(|| self.grid.screen_text_rows());
        let is_alternate = self.grid.is_alternate_screen();

        // TerminalGrid::process handles changed-row detection internally,
        // but we need to detect screen switches for prev_rows reset.
        if is_alternate != self.was_alternate {
            // Force full diff by clearing TerminalGrid's prev_rows
            self.grid.clear_prev_rows();
        }

        let changed = self.grid.process(data);
        if !child_output {
            if !changed.is_empty() {
                self.last_composer_paint_at = None;
            }
        } else if !changed.is_empty()
            && let Some(rows) = self.grid.screen_text_rows_ref()
            && let Some((prompt, _)) = crate::chrome::find_ego_composer(rows)
            && changed.iter().any(|row| {
                row.row_index == prompt
                    && uncached_rows
                        .as_ref()
                        .is_none_or(|before| before.get(prompt) != Some(&row.text))
            })
        {
            self.last_composer_paint_at = Some(started_at);
        }

        let is_alternate = self.grid.is_alternate_screen();
        let inline_tui = !is_alternate && self.grid.is_mouse_reporting();

        // --- Log extraction: read new scrollback lines from grid ---
        // The grid accumulates scrollback automatically when lines scroll
        // off the top of the normal screen. We just read the delta.
        //
        // When suppress_capture is set (side panel halved the terminal
        // width), Ink re-renders push fragmented junk into scrollback.
        // Skip capture but keep scrollback_origin in sync. Inline TUIs use
        // the same keep-cursor-in-sync path so disabling mouse mode does
        // not flush the TUI history into the log.
        if !is_alternate {
            let total_sb = self.grid.primary_screen_origin();
            let delta = total_sb.saturating_sub(self.scrollback_origin);
            if delta > 0 {
                if !self.suppress_capture && !inline_tui {
                    let mut new_lines = self.grid.read_scrollback_log_lines(delta);
                    mark_agent_chrome(&mut new_lines);
                    let pty_cols = self.pty_cols;
                    for mut ll in new_lines {
                        ll.cols = pty_cols;
                        self.push_log_line(ll);
                    }
                } else {
                    self.note_capture_gap();
                }
            }
            // Reconciliation may rewind the origin; resynchronize even without new rows.
            self.scrollback_origin = total_sb;
        }

        self.was_alternate = is_alternate;
        changed
    }

    /// Resize with full reflow so visible logical lines keep their content
    /// and natural soft-wrap continuity for terminal and redaction readers.
    /// Alt screen and reflow_history=false disable reflow entirely.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.last_composer_paint_at = None;
        let prev = self.pty_cols;
        self.pty_cols = cols;
        if cols > self.max_cols {
            self.max_cols = cols;
        }
        // Suppress scrollback capture when cols drops by >50% (side panel).
        // Ignore the very first resize (total_pushed==0, xterm fit addon).
        if cols.saturating_mul(2) < prev && self.total_pushed > 0 {
            self.suppress_capture = true;
        } else if cols.saturating_mul(2) >= self.max_cols {
            self.suppress_capture = false;
        }
        let mode = if !self.grid.reflow_history || self.grid.is_alternate_screen() {
            ReflowMode::None
        } else {
            ReflowMode::All
        };
        self.grid.resize_with_mode(rows, cols, mode);
        // A resize can change the inactive primary grid's history length while an
        // alternate-screen app is active. Keep the durable-log cursor in the
        // primary coordinate space; syncing it to alt history suppresses normal
        // shell capture after exit until primary history catches up.
        self.scrollback_origin = self.grid.primary_screen_origin();
        self.note_capture_gap();
    }

    /// All finalized log lines (oldest first).
    #[allow(dead_code)]
    pub fn lines(&self) -> &VecDeque<LogLine> {
        &self.log
    }

    /// Returns log lines starting at absolute `offset`, up to `limit` lines.
    /// Offset is in the same coordinate space as `total_lines()` — monotonically
    /// increasing, not relative to the current buffer contents.
    /// Returns `(lines, new_offset)` where `new_offset = total_lines()`.
    ///
    /// Chrome lines (agent prompt box and footer) occupy offset slots but are
    /// omitted from the result, so the returned count can be smaller than
    /// `limit`. Callers deriving a window start from the result length must use
    /// [`Self::oldest_offset`] instead.
    pub fn lines_since_owned(&self, offset: usize, limit: usize) -> (Vec<LogLine>, usize) {
        let oldest = self.oldest_offset();
        let total = self.total_pushed;
        if offset >= total {
            return (Vec::new(), total);
        }
        // Clamp to oldest retained line if the requested offset was evicted
        let effective = offset.max(oldest);
        let skip = effective - oldest;
        let mut slice: Vec<LogLine> = self
            .log
            .iter()
            .skip(skip)
            .take(limit)
            .filter(|line| !line.chrome)
            .cloned()
            .collect();
        strip_structural_blocks(&mut slice);
        (slice, total)
    }

    /// Current visible screen rows.
    ///
    /// Returns the cached snapshot from the grid (from the last `process()` call)
    /// when available — no re-parsing needed.
    pub fn screen_rows(&self) -> Vec<String> {
        self.grid.screen_text_rows()
    }

    /// Per screen row: true when the row soft-wraps into the next one.
    pub fn screen_row_wraps(&self) -> Vec<bool> {
        self.grid.screen_row_wraps()
    }

    /// `compute(self)` for the retained log, recomputed only after a push or a
    /// capture gap: polling agents read far more often than the log changes.
    pub fn cached_log_secrets(
        &mut self,
        compute: impl FnOnce(&Self) -> Vec<String>,
    ) -> Vec<String> {
        if let Some((generation, secrets)) = &self.secret_cache
            && *generation == self.total_pushed
        {
            return secrets.clone();
        }
        let secrets = compute(self);
        self.secret_cache = Some((self.total_pushed, secrets.clone()));
        secrets
    }

    /// Text of the history rows that soft-wrap into the first screen row.
    pub fn screen_head_context(&self) -> String {
        self.grid.screen_head_context()
    }

    /// Offset of the first line of the logical line that contains `offset`:
    /// walks back while the previous retained line soft-wraps into it.
    fn logical_line_start(&self, offset: usize) -> usize {
        let oldest = self.oldest_offset();
        if offset >= self.total_pushed {
            return offset; // nothing new to read, so nothing to complete
        }
        let mut start = offset.max(oldest);
        while start > oldest && self.log[start - 1 - oldest].wrapped {
            start -= 1;
        }
        start
    }

    /// Like [`Self::lines_since_owned`], but whole logical lines: the window
    /// starts at the head of the line holding `offset` and, when its last row
    /// wraps, runs on to the end of that line. Lines with pieces missing from
    /// the log are replaced by `[REDACTED]`, together with their continuation.
    /// Returns `(lines, start, end)` in source-row positions, including skipped
    /// chrome slots. `end` is the next page boundary, not the total log size.
    pub fn lines_since_logical(&self, offset: usize, limit: usize) -> (Vec<LogLine>, usize, usize) {
        if offset >= self.total_pushed {
            return (Vec::new(), self.total_pushed, self.total_pushed);
        }
        let start = self.logical_line_start(offset);
        let oldest = self.oldest_offset();
        let mut end = start.saturating_add(limit.max(1)).min(self.total_pushed);
        while end < self.total_pushed && self.log[end - 1 - oldest].wrapped {
            end += 1;
        }
        let (mut lines, _) = self.lines_since_owned(start, end - start);
        let mut hiding = false;
        let mut previous_wrapped = false;
        for line in &mut lines {
            hiding = line.partial || (hiding && previous_wrapped);
            previous_wrapped = line.wrapped;
            if hiding {
                line.spans = vec![LogSpan {
                    text: "[REDACTED]".to_string(),
                    ..LogSpan::default()
                }];
            }
        }
        (lines, start, end)
    }

    /// Borrowed view of cached screen rows — avoids cloning when caller holds the lock.
    pub fn screen_rows_ref(&self) -> Option<&[String]> {
        self.grid.screen_text_rows_ref()
    }

    pub fn cursor_point(&self) -> (usize, usize) {
        self.grid.cursor_point()
    }

    pub fn logical_prefix_at_cursor(&self) -> Option<terminal_grid::LogicalPrefix> {
        self.grid.logical_prefix_at_cursor()
    }

    pub fn logical_line_at_row(&self, row: usize) -> Option<terminal_grid::LogicalPrefix> {
        self.grid.logical_line_at_row(row)
    }

    pub fn physical_prefix_at_cursor(&self) -> Option<terminal_grid::LogicalPrefix> {
        self.grid.physical_prefix_at_cursor()
    }

    /// Current visible screen rows as styled LogLines (with ANSI color attributes).
    /// Used by mobile/REST to render screen content with colors.
    pub fn screen_log_lines(&self) -> Vec<LogLine> {
        let mut lines = self.grid.screen_log_lines();
        strip_structural_blocks(&mut lines);
        // Trim trailing empty lines
        while let Some(last) = lines.last() {
            if last.spans.is_empty() {
                lines.pop();
            } else {
                break;
            }
        }
        lines
    }

    /// Extract the user-typed text from the prompt line, excluding ghost/suggestion text.
    /// Uses the cursor position as the boundary — everything after the cursor is suggestion.
    /// Falls back to dim-detection when the cursor is not on a prompt row.
    pub fn prompt_input_text(&self) -> Option<String> {
        self.grid.prompt_input_text()
    }

    /// Total log lines ever pushed (monotonically increasing).
    /// Use as a stable cursor for paginated reads — does not decrease when
    /// old lines are evicted from the bounded buffer.
    pub fn total_lines(&self) -> usize {
        self.total_pushed
    }

    /// Absolute offset of the oldest retained line. Lines before this have
    /// been evicted by buffer rotation and are no longer available.
    pub fn oldest_offset(&self) -> usize {
        self.total_pushed - self.log.len()
    }

    /// Serialize damaged grid rows into a binary frame for Tauri Channel streaming.
    /// Delegates to the inner TerminalGrid; the frame is empty when no rows changed.
    ///
    /// The order the frame is cut in is stamped here, inside the vt lock the
    /// caller holds, because that is the only place the serialize order exists:
    /// every producer releases the lock before handing the bytes to
    /// `send_grid_frame`. See [`grid_gate::GridFrame`].
    pub fn serialize_dirty_rows(&mut self) -> grid_gate::GridFrame {
        grid_gate::GridFrame::cut(self.grid.serialize_dirty_rows())
    }

    /// A whole-screen frame for ONE subscriber that leaves the shared damage,
    /// the viewport state and the bell untouched. See
    /// [`terminal_grid::TerminalGrid::serialize_full_frame`].
    pub fn serialize_full_frame(&self) -> Vec<u8> {
        self.grid.serialize_full_frame()
    }

    /// Whether a DEC 2026 synchronized update is currently open.
    pub fn is_sync_update_active(&self) -> bool {
        self.grid.is_sync_update_active()
    }

    /// Flush a synchronized update whose deadline has passed; `true` when it
    /// produced new damage to serialize.
    pub fn flush_sync_timeout_if_needed(&mut self) -> bool {
        self.grid.flush_sync_timeout_if_needed()
    }

    /// Drain a still-buffered synchronized update regardless of its deadline.
    pub fn force_stop_sync_if_buffered(&mut self) -> bool {
        self.grid.force_stop_sync_if_buffered()
    }

    pub fn is_alternate_screen(&self) -> bool {
        self.grid.is_alternate_screen()
    }

    pub fn is_mouse_reporting(&self) -> bool {
        self.grid.is_mouse_reporting()
    }

    pub fn is_cursor_visible(&self) -> bool {
        self.grid.is_cursor_visible()
    }

    pub fn grid_force_full_damage(&mut self) {
        self.grid.force_full_damage();
    }

    pub fn grid_drain_events(&self) -> Vec<terminal_grid::TermEvent> {
        self.grid.drain_events()
    }

    // --- Scroll delegates ---

    pub fn grid_scroll(&mut self, delta: i32) {
        self.grid.scroll(delta);
    }

    pub fn grid_scroll_to_line(&mut self, line: usize) {
        self.grid.scroll_to_line(line);
    }

    pub fn grid_scroll_to_offset(&mut self, offset: usize) {
        self.grid.scroll_to_offset(offset);
    }

    pub fn grid_scroll_to_anchor(&mut self, anchor: Option<usize>) {
        self.grid.scroll_to_anchor(anchor);
    }

    pub fn grid_display_offset(&self) -> usize {
        self.grid.display_offset()
    }

    pub fn grid_serialize_styled_range(&self, start_abs: usize, count: usize) -> Vec<u8> {
        self.grid.serialize_styled_range(start_abs, count)
    }

    pub fn grid_total_lines(&self) -> usize {
        self.grid.total_lines()
    }

    pub fn read_rows_in_range(&self, start_abs: usize, end_abs: usize) -> Vec<String> {
        self.grid.read_rows_in_range(start_abs, end_abs)
    }

    pub fn grid_screen_lines(&self) -> usize {
        self.grid.screen_lines()
    }

    pub fn grid_columns(&self) -> usize {
        self.grid.columns()
    }

    pub fn grid_history_size(&self) -> usize {
        self.grid.scrollback_count()
    }

    pub fn grid_screen_origin(&self) -> usize {
        self.grid.screen_origin()
    }

    // --- Search delegate ---

    pub fn grid_search(&self, query: &str) -> Vec<terminal_grid::SearchMatch> {
        self.grid.search(query)
    }

    pub fn grid_search_buffer(&self, query: &str) -> Vec<terminal_grid::BufferSearchMatch> {
        self.grid.search_buffer(query)
    }

    // --- Row text delegate ---

    pub fn grid_get_row_text(&self, row: usize) -> String {
        self.grid.get_row_text(row)
    }

    pub fn grid_get_logical_line(&self, row: usize) -> (usize, String) {
        self.grid.get_logical_line(row)
    }

    pub fn grid_get_cursor_line(&self) -> String {
        self.grid.get_cursor_row_text()
    }

    pub fn grid_get_selection_text(
        &self,
        start_row: usize,
        start_col: usize,
        end_row: usize,
        end_col: usize,
        history_base: Option<usize>,
    ) -> Result<String, String> {
        self.grid.get_selection_text_with_history_base(
            start_row,
            start_col,
            end_row,
            end_col,
            history_base,
        )
    }

    pub fn grid_get_lines(&self, start: usize, end: usize) -> Vec<String> {
        // `start`/`end` are ABSOLUTE row indices (0 = oldest scrollback line),
        // end-exclusive. get_row_text() treats its arg as a viewport-relative
        // screen row, so it returned the wrong lines whenever scrollback existed.
        // read_rows_in_range does the correct absolute→grid conversion (inclusive end).
        let total = self.grid.total_lines();
        let clamped_end = end.min(total);
        if start >= clamped_end {
            return Vec::new();
        }
        self.grid.read_rows_in_range(start, clamped_end - 1)
    }

    pub fn grid_hyperlink_at(&self, row: usize, col: usize) -> Option<String> {
        self.grid.hyperlink_at(row, col)
    }

    pub fn grid_hyperlink_span(&self, row: usize, col: usize) -> Option<(usize, usize, String)> {
        self.grid.hyperlink_span(row, col)
    }

    // --- private helpers ---

    /// Roughly how much heap the captured log lines hold, for `memory_report`.
    ///
    /// Reported apart from [`Self::grid_bytes`] because the two are bounded by
    /// different things and only one of them can run away. The line *count* is
    /// capped at `VT_LOG_BUFFER_CAPACITY`, but a span's text is whatever the
    /// PTY emitted — 10,000 lines of a multi-megabyte JSON blob is 10,000 lines
    /// and gigabytes. Summed together with the grid, a climb here is
    /// indistinguishable from a terminal simply filling its scrollback.
    ///
    /// Walks the log once, which is fine on demand and is why the report is not
    /// on the diagnostics tick.
    pub fn log_bytes(&self) -> usize {
        self.log
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|s| s.text.len() + std::mem::size_of::<LogSpan>())
                    .sum::<usize>()
            })
            .sum()
    }

    /// Roughly how much heap the terminal grid behind this buffer holds.
    ///
    /// Hard-bounded: `GRID_SCROLLBACK` rows × columns × one `Cell`. A session
    /// that fills its scrollback reaches this ceiling and stops.
    pub fn grid_bytes(&self) -> usize {
        self.grid.approx_bytes()
    }

    fn push_log_line(&mut self, mut line: LogLine) {
        line.partial |= std::mem::take(&mut self.next_head_lost);
        let mut evicted_wrapped = false;
        if self.log.len() >= self.capacity
            && let Some(evicted) = self.log.pop_front()
        {
            evicted_wrapped = evicted.wrapped;
        }
        self.log.push_back(line);
        self.total_pushed += 1;
        // The oldest retained line now continues a line whose head is gone.
        if evicted_wrapped && let Some(front) = self.log.front_mut() {
            front.partial = true;
        }
    }

    /// Rows were scrolled off (or history was rewritten by a resize) without
    /// being logged: the log tail lost its continuation and the next line will
    /// have lost its head. Flag both so readers hide them rather than leak.
    fn note_capture_gap(&mut self) {
        self.secret_cache = None;
        if let Some(last) = self.log.back_mut()
            && last.wrapped
        {
            last.wrapped = false;
            last.partial = true;
        }
        self.next_head_lost = self.grid.newest_history_row_wraps();
    }
}

// ---------------------------------------------------------------------------
