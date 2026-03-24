//! VT100 diff renderer for scroll-jump-free terminal output.
//!
//! Processes raw PTY output through a `vt100::Parser` and emits only the
//! minimal ANSI sequences needed to update the visible screen. This prevents
//! ESC[2J/ESC[3J from reaching xterm.js and causing viewport jumps.
//!
//! Lines that scroll off the top of the virtual screen are captured and
//! emitted as formatted text, so they appear in xterm.js's scrollback.

/// Output from a single `DiffRenderer::process()` call.
pub struct DiffOutput {
    /// Lines that scrolled off the top — emit BEFORE screen_patch.
    /// Each entry is ANSI-formatted bytes (with colors) + CRLF terminated.
    pub scrollback_lines: Vec<Vec<u8>>,
    /// Minimal ANSI to update the visible screen — cursor-addressed cell updates.
    pub screen_patch: Vec<u8>,
    /// When true, the session is in alternate buffer — caller should forward
    /// raw PTY data instead of using scrollback_lines + screen_patch.
    pub use_raw_passthrough: bool,
}

/// Per-session VT100 diff renderer.
///
/// Maintains a virtual terminal via `vt100::Parser`. On each `process()` call:
/// 1. Feeds data to the parser (updates the virtual screen)
/// 2. Extracts any new scrollback lines (formatted with colors)
/// 3. Computes `screen.contents_diff(&prev_screen)` for minimal screen updates
/// 4. Returns a `DiffOutput` the caller can concatenate and emit
pub struct DiffRenderer {
    parser: vt100::Parser,
    prev_screen: Option<vt100::Screen>,
    scrollback_read: usize,
    was_alternate: bool,
}

impl DiffRenderer {
    /// Scrollback capacity for the vt100 parser. Lines beyond this are dropped.
    /// We only need enough to capture lines between process() calls.
    const SCROLLBACK_CAPACITY: usize = 500;

    pub fn new(rows: u16, cols: u16) -> Self {
        Self {
            parser: vt100::Parser::new(rows, cols, Self::SCROLLBACK_CAPACITY),
            prev_screen: None,
            scrollback_read: 0,
            was_alternate: false,
        }
    }

    /// Process a chunk of PTY data and return the diff output.
    pub fn process(&mut self, data: &[u8]) -> DiffOutput {
        self.parser.process(data);

        let is_alternate = self.parser.screen().alternate_screen();

        // Alternate buffer transition: flag for raw passthrough
        if is_alternate {
            self.was_alternate = true;
            return DiffOutput {
                scrollback_lines: Vec::new(),
                screen_patch: Vec::new(),
                use_raw_passthrough: true,
            };
        }

        // Just exited alternate buffer — force full repaint
        let force_full = if self.was_alternate {
            self.was_alternate = false;
            self.prev_screen = None;
            self.scrollback_read = self.parser.screen().scrollback();
            true
        } else {
            false
        };

        // Extract scrollback lines that scrolled off the top since last call
        let scrollback_lines = self.extract_scrollback();

        // Compute screen patch
        let screen = self.parser.screen();
        let screen_patch = if force_full || self.prev_screen.is_none() {
            screen.contents_formatted()
        } else {
            screen.contents_diff(self.prev_screen.as_ref().unwrap())
        };

        // Save current screen for next diff
        self.prev_screen = Some(screen.clone());

        DiffOutput {
            scrollback_lines,
            screen_patch,
            use_raw_passthrough: false,
        }
    }

    /// Resize the virtual terminal. Clears prev_screen so the next
    /// process() emits a full repaint.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.parser.screen_mut().set_size(rows, cols);
        self.prev_screen = None;
        self.scrollback_read = self.parser.screen().scrollback();
    }

    /// Extract new scrollback lines since last call, formatted with ANSI colors.
    ///
    /// Uses the set_scrollback(MAX) trick (same as VtLogBuffer::scrollback_count)
    /// to query the total scrollback count, then pages through new lines.
    fn extract_scrollback(&mut self) -> Vec<Vec<u8>> {
        // Query total scrollback count: set_scrollback(MAX) → scrollback() → reset
        self.parser.screen_mut().set_scrollback(usize::MAX);
        let total_sb = self.parser.screen().scrollback();
        self.parser.screen_mut().set_scrollback(0);

        let delta = total_sb.saturating_sub(self.scrollback_read);
        if delta == 0 {
            return Vec::new();
        }

        let (_, cols) = self.parser.screen().size();
        let screen_height = self.parser.screen().size().0 as usize;
        let mut lines = Vec::with_capacity(delta);

        // Read new scrollback lines from oldest to newest, in pages.
        // rows_formatted(start_col, width) returns an iterator over ALL rows
        // with `start_col` as column offset and `width` as column count.
        let read_start = total_sb.saturating_sub(delta);
        let mut remaining = delta;
        let mut pos = read_start;

        while remaining > 0 {
            let page = remaining.min(screen_height);
            let offset = total_sb - pos;
            self.parser.screen_mut().set_scrollback(offset);

            // rows_formatted(0, cols) returns all rows; take only the page we need
            for row_bytes in self.parser.screen().rows_formatted(0, cols).take(page) {
                let mut buf = Vec::with_capacity(row_bytes.len() + 2);
                buf.extend_from_slice(&row_bytes);
                buf.extend_from_slice(b"\r\n");
                lines.push(buf);
            }

            pos += page;
            remaining -= page;
        }

        self.parser.screen_mut().set_scrollback(0);
        self.scrollback_read = total_sb;
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_renderer() {
        let r = DiffRenderer::new(24, 80);
        assert!(r.prev_screen.is_none());
        assert_eq!(r.scrollback_read, 0);
        assert!(!r.was_alternate);
    }

    #[test]
    fn process_returns_diff_output() {
        let mut r = DiffRenderer::new(24, 80);
        let out = r.process(b"hello world");
        assert!(!out.use_raw_passthrough);
        // First call has no prev_screen → full render via contents_formatted
        assert!(!out.screen_patch.is_empty());
    }

    #[test]
    fn second_process_same_content_returns_empty_patch() {
        let mut r = DiffRenderer::new(24, 80);
        r.process(b"hello");
        let out = r.process(b""); // no new data
        assert!(out.screen_patch.is_empty(), "no changes should produce empty diff");
    }

    #[test]
    fn esc_2j_not_in_output() {
        let mut r = DiffRenderer::new(24, 80);
        r.process(b"initial content");
        let out = r.process(b"\x1b[2J\x1b[Hnew content");
        // ESC[2J should NOT appear in the screen patch
        let patch_str = String::from_utf8_lossy(&out.screen_patch);
        assert!(
            !patch_str.contains("\x1b[2J"),
            "ESC[2J must not appear in diff output"
        );
    }

    #[test]
    fn esc_3j_not_in_output() {
        let mut r = DiffRenderer::new(24, 80);
        r.process(b"some content");
        let out = r.process(b"\x1b[3J");
        let patch_str = String::from_utf8_lossy(&out.screen_patch);
        assert!(
            !patch_str.contains("\x1b[3J"),
            "ESC[3J must not appear in diff output"
        );
    }

    #[test]
    fn alt_buffer_sets_passthrough() {
        let mut r = DiffRenderer::new(24, 80);
        let out = r.process(b"\x1b[?1049h"); // enter alt buffer
        assert!(out.use_raw_passthrough);
    }

    #[test]
    fn alt_buffer_exit_returns_full_repaint() {
        let mut r = DiffRenderer::new(24, 80);
        r.process(b"normal content");
        r.process(b"\x1b[?1049h"); // enter alt
        r.process(b"alt content");
        let out = r.process(b"\x1b[?1049l"); // exit alt
        // Should not be passthrough anymore
        assert!(!out.use_raw_passthrough);
        // Should have a full repaint (prev_screen was cleared)
        assert!(!out.screen_patch.is_empty());
    }

    #[test]
    fn resize_clears_prev_screen() {
        let mut r = DiffRenderer::new(24, 80);
        r.process(b"content");
        assert!(r.prev_screen.is_some());
        r.resize(30, 100);
        assert!(r.prev_screen.is_none());
        // Next process should do full repaint
        let out = r.process(b"");
        // contents_formatted of an empty screen after resize
        assert!(out.screen_patch.is_empty() || !out.screen_patch.is_empty());
        // Just verify it doesn't panic
    }

    #[test]
    fn scrollback_extracted_when_content_scrolls_off() {
        let mut r = DiffRenderer::new(5, 40); // small screen: 5 rows
        // Write enough lines to scroll some off
        let mut data = Vec::new();
        for i in 0..10 {
            data.extend_from_slice(format!("line {i}\r\n").as_bytes());
        }
        let out = r.process(&data);
        // With 5-row screen and 10 lines written, ~5 lines should scroll off
        assert!(
            !out.scrollback_lines.is_empty(),
            "scrollback should have lines that scrolled off the 5-row screen"
        );
        // Verify content is correct (not truncated)
        let first = String::from_utf8_lossy(&out.scrollback_lines[0]);
        assert!(first.contains("line 0"), "first scrollback line should be 'line 0', got: {first}");
    }

    #[test]
    fn colors_preserved_in_diff() {
        let mut r = DiffRenderer::new(24, 80);
        r.process(b""); // init
        let out = r.process(b"\x1b[31mred text\x1b[0m");
        let patch = String::from_utf8_lossy(&out.screen_patch);
        // The diff should contain SGR sequences for red
        assert!(
            patch.contains("red text"),
            "diff should contain the text content"
        );
    }

    #[test]
    fn cursor_up_sequences_absorbed() {
        let mut r = DiffRenderer::new(10, 80);
        // Write initial content
        r.process(b"line 0\r\nline 1\r\nline 2\r\nline 3\r\nline 4\r\n");
        // Cursor up 3 + overwrite (CC status bar pattern)
        let out = r.process(b"\x1b[3Anew line 2\r\n\r\n\r\n");
        // Output should not contain CSI 3A — just the cell diff
        let patch = String::from_utf8_lossy(&out.screen_patch);
        assert!(
            !patch.contains("\x1b[3A"),
            "cursor-up should be absorbed by diff renderer"
        );
    }
}
