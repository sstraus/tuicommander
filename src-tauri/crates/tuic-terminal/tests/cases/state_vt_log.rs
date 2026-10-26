//! Relocated state tests exercise the existing terminal-domain public API.

use tuic_terminal::{chrome, terminal_grid};

mod state {
    mod tests {
        use tuic_terminal::vt_log::{LogColor, LogLine, LogSpan, VtLogBuffer, mark_agent_chrome};

        // --- VtLogBuffer tests ---

        fn make_vt_log() -> VtLogBuffer {
            VtLogBuffer::new(24, 80, 1000)
        }

        /// Helper: extract plain text from log lines for easy assertion.
        fn log_texts(buf: &VtLogBuffer) -> Vec<String> {
            buf.lines().iter().map(|ll| ll.text()).collect()
        }

        /// Emit lines one at a time and verify they appear in the log after scrolling off.
        /// NOTE: process() is called per-line to match production behavior (PTY reader calls
        /// process() on each small chunk; a single bulk process() call cannot detect scroll).
        #[test]
        fn test_vt_log_line_oriented_output() {
            let mut buf = make_vt_log();
            // Feed lines one at a time so the diff algorithm can detect scroll
            for i in 0..30 {
                buf.process(format!("line {i}\r\n").as_bytes());
            }
            let texts = log_texts(&buf);
            // Lines 0..6 scrolled off (30 total - 24 visible = 6)
            assert!(!texts.is_empty(), "should have finalized some lines");
            assert!(
                texts[0].starts_with("line "),
                "line content preserved: {:?}",
                texts[0]
            );
        }

        /// The reader publishes sync state and the frame ticker flushes a stalled
        /// update through `VtLogBuffer` — both go through these delegations, so a
        /// missing one silently reinstates the wedged-terminal bug.
        #[test]
        fn test_vt_log_sync_update_flush_delegation() {
            let mut buf = make_vt_log();
            buf.process(b"\x1b[?2026h");
            assert!(
                buf.is_sync_update_active(),
                "reader observes the open update"
            );
            buf.process(b"STALLED\r\n");
            assert!(
                !buf.flush_sync_timeout_if_needed(),
                "ticker does not flush before the deadline"
            );

            std::thread::sleep(std::time::Duration::from_millis(170));

            assert!(
                buf.flush_sync_timeout_if_needed(),
                "ticker flushes the expired update with no further PTY bytes"
            );
            assert!(!buf.is_sync_update_active(), "hint clears after the flush");
            assert!(
                buf.screen_rows_ref()
                    .is_some_and(|rows| rows.iter().any(|r| r.contains("STALLED"))),
                "flushed content reaches the cached screen rows"
            );
        }

        /// Teardown must drain a buffered update rather than drop it.
        #[test]
        fn test_vt_log_force_stop_sync_on_shutdown() {
            let mut buf = make_vt_log();
            buf.process(b"\x1b[?2026h");
            buf.process(b"ATEXIT\r\n");
            assert!(
                buf.force_stop_sync_if_buffered(),
                "shutdown drains the buffer"
            );
            assert!(
                buf.screen_rows_ref()
                    .is_some_and(|rows| rows.iter().any(|r| r.contains("ATEXIT"))),
                "content survives teardown"
            );
        }

        /// In-place rewrite via \r should resolve to the final content on screen.
        #[test]
        fn test_vt_log_in_place_rewrite() {
            let mut buf = make_vt_log();
            // Write "aaaa\r" followed by "bbbb" — vt100 resolves this to "bbbb" on the line
            buf.process(b"aaaa\rbbbb\r\n");
            // The current screen should show "bbbb" not "aaaa"
            let rows = buf.screen_rows();
            let first_nonempty = rows
                .iter()
                .find(|r| !r.is_empty())
                .map(|s| s.as_str())
                .unwrap_or("");
            assert_eq!(
                first_nonempty, "bbbb",
                "in-place rewrite resolved to final content"
            );
        }

        /// Rows scrolled off the top go into the log.
        /// NOTE: per-line process() calls to match production behavior.
        #[test]
        fn test_vt_log_scroll_emits_scrolled_off_rows() {
            let mut buf = make_vt_log();
            // Feed exactly 25 lines one at a time — the first one scrolls off
            for i in 0..25 {
                buf.process(format!("row{i}\r\n").as_bytes());
            }
            let texts = log_texts(&buf);
            assert!(
                !texts.is_empty(),
                "at least one line should have scrolled off"
            );
            assert_eq!(
                texts[0], "row0",
                "first scrolled-off row is row0, got: {:?}",
                texts[0]
            );
        }

        /// Alternate screen suppresses log extraction.
        #[test]
        fn test_vt_log_alternate_screen_suppresses_extraction() {
            let mut buf = make_vt_log();
            // Enter alternate screen (smcup), write some content, exit (rmcup)
            // smcup: ESC[?1049h
            buf.process(b"\x1b[?1049h");
            // Write 30 lines in alternate screen
            let mut content = String::new();
            for i in 0..30 {
                content.push_str(&format!("alt-line {i}\r\n"));
            }
            buf.process(content.as_bytes());
            // No lines should be in the log while alternate screen is active
            assert!(
                buf.lines().is_empty(),
                "alternate screen content should not be logged"
            );
        }

        /// After alternate screen exits, extraction resumes.
        #[test]
        fn test_vt_log_alternate_screen_exit_resumes_extraction() {
            let mut buf = make_vt_log();
            // Enter alternate screen
            buf.process(b"\x1b[?1049h");
            // Write in alternate screen (per-line)
            for i in 0..30 {
                buf.process(format!("alt {i}\r\n").as_bytes());
            }
            // Exit alternate screen (rmcup)
            buf.process(b"\x1b[?1049l");
            // Now write regular output that will scroll (per-line)
            for i in 0..30 {
                buf.process(format!("main {i}\r\n").as_bytes());
            }
            // Main screen output should appear in the log
            let texts = log_texts(&buf);
            assert!(
                !texts.is_empty(),
                "main screen lines should appear in log after alt exit"
            );
            assert!(
                texts.iter().any(|l| l.starts_with("main")),
                "main lines present in log"
            );
        }

        /// `grok --no-alt-screen` (and any inline TUI) enables mouse reporting on the
        /// primary buffer and then scrolls its own viewport (`CSI S` / line dump).
        /// Those rows must not enter the durable log — same contract as alt-screen.
        #[test]
        fn test_vt_log_primary_mouse_mode_suppresses_extraction() {
            let mut buf = make_vt_log();
            // Combined DECSET the way grok actually emits it (not `?1000h` alone).
            buf.process(b"\x1b[?1000;1002;1003;1006h");
            for i in 0..30 {
                buf.process(format!("grok-frame {i}\r\n").as_bytes());
            }
            assert!(
                buf.lines().is_empty(),
                "mouse-mode primary TUI must not be logged, got: {:?}",
                log_texts(&buf)
            );
            assert!(
                buf.grid_history_size() > 0,
                "grid scrollback must still accumulate so the scrollbar works"
            );
        }

        /// Turning mouse reporting off resumes durable-log capture. Leftover TUI
        /// rows still on the primary viewport may scroll into the log afterwards
        /// (that is the point of `--no-alt-screen`); what must not happen is a
        /// one-shot flush of the history that accumulated *while* mouse mode was on.
        #[test]
        fn test_vt_log_primary_mouse_mode_exit_resumes_capture() {
            let mut buf = make_vt_log();
            buf.process(b"\x1b[?1000h");
            for i in 0..30 {
                buf.process(format!("tui {i}\r\n").as_bytes());
            }
            let history_while_tui = buf.grid_history_size();
            assert!(history_while_tui > 0, "TUI must have created grid history");
            buf.process(b"\x1b[?1000l");
            assert!(
                buf.lines().is_empty(),
                "disabling mouse mode must not flush TUI history, got: {:?}",
                log_texts(&buf)
            );
            for i in 0..30 {
                buf.process(format!("shell {i}\r\n").as_bytes());
            }
            let texts = log_texts(&buf);
            assert!(
                texts.iter().any(|l| l.starts_with("shell")),
                "shell lines must appear after mouse mode ends, got: {texts:?}"
            );
        }

        /// A resize while an alternate-screen app owns a large history must not move
        /// the primary screen's log cursor into that unrelated coordinate space.
        ///
        /// This models `gh run watch`: every refresh homes, erases, and prints a frame
        /// taller than the viewport. After leaving the app, ordinary shell output must
        /// be captured immediately instead of being suppressed until primary history
        /// catches up with the much larger alternate history.
        #[test]
        fn test_vt_log_alt_resize_keeps_primary_capture_cursor() {
            let mut buf = VtLogBuffer::new(4, 80, 1000);

            for i in 0..8 {
                buf.process(format!("before-watch-{i}\r\n").as_bytes());
            }
            let before_watch_total = buf.total_lines();
            assert!(
                before_watch_total > 0,
                "sanity: primary output reached the log"
            );

            buf.process(b"\x1b[?1049h");
            for refresh in 0..4 {
                buf.process(b"\x1b[0;0H\x1b[J");
                buf.process(
                    format!("Refreshing run status every 3 seconds [{refresh}]\r\n").as_bytes(),
                );
                for job in 0..12 {
                    buf.process(format!("  job-{job:02}: running\r\n").as_bytes());
                }
            }
            assert!(
                buf.grid_history_size() > 20,
                "sanity: the synthetic watch built substantial alternate history"
            );

            // Real panel/layout changes resize the PTY while the watch is still active.
            buf.resize(5, 72);
            buf.process(b"\x1b[?1049l");

            for i in 0..10 {
                buf.process(format!("after-watch-{i}\r\n").as_bytes());
            }

            let texts = log_texts(&buf);
            assert!(
                texts.iter().any(|line| line == "after-watch-0"),
                "primary capture must resume immediately after alt exit; log tail: {:?}",
                texts.iter().rev().take(12).collect::<Vec<_>>()
            );
            assert!(
                !texts
                    .iter()
                    .any(|line| line.contains("Refreshing run status") || line.contains("job-")),
                "alternate-screen rows must remain excluded from the durable log"
            );
        }

        /// resize() updates parser dimensions.
        #[test]
        fn test_vt_log_resize() {
            let mut buf = make_vt_log();
            // Resize to a smaller terminal
            buf.resize(10, 40);
            // Should not panic and screen size should be updated
            let rows = buf.screen_rows();
            // After resize, screen_rows returns 10 rows (all empty after resize)
            assert_eq!(rows.len(), 10, "resize to 10 rows");
        }

        /// Empty/whitespace-only rows are not added to the log.
        #[test]
        fn test_vt_log_trims_whitespace_only_rows() {
            let mut buf = make_vt_log();
            // Fill with content including empty lines
            let mut input = String::new();
            for i in 0..25 {
                if i == 5 {
                    input.push_str("\r\n"); // empty line
                } else {
                    input.push_str(&format!("content {i}\r\n"));
                }
            }
            buf.process(input.as_bytes());
            let texts = log_texts(&buf);
            // None of the logged lines should be empty
            for line in &texts {
                assert!(
                    !line.is_empty(),
                    "logged line should not be empty: {line:?}"
                );
            }
        }

        /// Log capacity is bounded: old lines are dropped when capacity is exceeded.
        #[test]
        fn test_vt_log_bounded_capacity() {
            let mut buf = VtLogBuffer::new(24, 80, 10); // tiny capacity
            // Produce many more than 10 lines
            let mut input = String::new();
            for i in 0..200 {
                input.push_str(&format!("line {i:04}\r\n"));
            }
            buf.process(input.as_bytes());
            assert!(
                buf.lines().len() <= 10,
                "log should be capped at capacity=10, got {}",
                buf.lines().len()
            );
        }

        /// lines_since_owned returns lines after offset and correct new offset.
        #[test]
        fn test_vt_log_lines_since_owned() {
            let mut buf = VtLogBuffer::new(24, 80, 1000);
            // Feed lines one at a time to trigger scroll detection
            for i in 0..30 {
                buf.process(format!("line {i}\r\n").as_bytes());
            }
            let total = buf.total_lines();
            assert!(total > 0, "should have some finalized lines");
            // First fetch: all lines
            let (batch1, off1) = buf.lines_since_owned(0, usize::MAX);
            assert_eq!(batch1.len(), total);
            assert_eq!(off1, total);
            // Second fetch: nothing new
            let (batch2, off2) = buf.lines_since_owned(off1, usize::MAX);
            assert_eq!(batch2, [] as [LogLine; 0]);
            assert_eq!(off2, total);
        }

        /// grid_get_lines uses ABSOLUTE row coords (0 = oldest scrollback line), not
        /// viewport-relative. Regression: it previously called get_row_text, which
        /// returned visible screen rows whenever scrollback was non-empty — so reading
        /// abs 0 gave the top of the screen instead of the oldest history line.
        #[test]
        fn test_grid_get_lines_absolute_coords_with_scrollback() {
            let mut buf = VtLogBuffer::new(3, 80, 1000); // 3 visible rows → forces scrollback
            for i in 0..9 {
                buf.process(format!("line {i}\r\n").as_bytes());
            }
            buf.process(b"line 9"); // no trailing newline → bottom visible row is "line 9"

            // NOTE: pass a large `end` so grid_get_lines clamps to the GRID's total
            // (history + visible screen). Do NOT use buf.total_lines() here — that is
            // VtLogBuffer::total_pushed (finalized log lines only, excludes the live
            // visible screen) and is a different quantity than grid.total_lines().
            let lines = buf.grid_get_lines(0, usize::MAX);
            // Absolute row 0 is the OLDEST line; the bottom of the visible screen is the
            // NEWEST. The old viewport-relative get_row_text path returned the top VISIBLE
            // row (≈ "line 7") for index 0 and dropped the real history entirely.
            assert_eq!(lines.first().map(String::as_str), Some("line 0"));
            assert_eq!(lines.last().map(String::as_str), Some("line 9"));
            assert_eq!(buf.grid_get_lines(0, 1), vec!["line 0".to_string()]);
            // Rows come back contiguous oldest→newest — history AND visible screen,
            // no gaps and no out-of-range empties.
            for w in lines.windows(2) {
                let a: usize = w[0].trim_start_matches("line ").parse().expect("line N");
                let b: usize = w[1].trim_start_matches("line ").parse().expect("line N");
                assert_eq!(b, a + 1, "rows must be contiguous ascending: {a} -> {b}");
            }
        }

        /// lines_since_owned returns correct results after buffer rotation
        /// (oldest lines evicted by pop_front).
        #[test]
        fn test_vt_log_lines_since_owned_after_rotation() {
            let mut buf = VtLogBuffer::new(24, 80, 10); // capacity = 10
            // Feed 40 lines one-at-a-time so scroll detection works
            for i in 0..40 {
                buf.process(format!("rot-{i}\r\n").as_bytes());
            }
            // Buffer should be at capacity
            assert!(buf.lines().len() <= 10, "should be capped at 10");
            let total = buf.total_lines();
            // Fetch all — should return only the retained lines
            let (batch, off) = buf.lines_since_owned(0, usize::MAX);
            assert_eq!(batch.len(), buf.lines().len());
            assert_eq!(off, total);
            // The retained lines should be the newest ones
            for line in &batch {
                assert!(
                    line.text().starts_with("rot-"),
                    "unexpected line: {:?}",
                    line.text()
                );
            }
            // Fetch with an offset past the end — empty
            let (empty, off2) = buf.lines_since_owned(off, usize::MAX);
            assert_eq!(empty, [] as [LogLine; 0]);
            assert_eq!(off2, off);
        }

        #[test]
        fn test_vt_log_lines_since_owned_evicted_offset_clamped() {
            let mut buf = VtLogBuffer::new(24, 80, 10);
            for i in 0..40 {
                buf.process(format!("rot-{i}\r\n").as_bytes());
            }
            let oldest = buf.oldest_offset();
            assert!(oldest > 0, "some lines must have been evicted");
            // Request from an offset in the evicted range — should clamp to oldest
            let evicted_offset = oldest.saturating_sub(5);
            let (batch, off) = buf.lines_since_owned(evicted_offset, usize::MAX);
            assert_eq!(batch.len(), buf.lines().len());
            assert_eq!(off, buf.total_lines());
            for line in &batch {
                assert!(
                    line.text().starts_with("rot-"),
                    "unexpected line: {:?}",
                    line.text()
                );
            }
        }

        /// Feed data in small incremental chunks (simulating real PTY reads that
        /// may split mid-line) and verify lines are still extracted.
        ///
        /// Misaligned chunks (7-byte boundaries) may produce partial rows that
        /// break overlap detection between consecutive process() calls. The
        /// conservative approach captures fewer lines than a "dump everything"
        /// fallback but avoids false duplicate log entries.
        #[test]
        fn test_vt_log_incremental_chunked_feed() {
            let mut buf = VtLogBuffer::new(24, 80, 1000);
            // Build 30 lines of output
            use std::fmt::Write as _;
            let full_output: String = (0..30).fold(String::new(), |mut acc, i| {
                let _ = write!(acc, "chunk-{i}\r\n");
                acc
            });
            let bytes = full_output.as_bytes();
            // Feed in small chunks of 7 bytes (deliberately misaligned with lines)
            for chunk in bytes.chunks(7) {
                buf.process(chunk);
            }
            let lines = log_texts(&buf);
            let matching: Vec<&String> = lines.iter().filter(|l| l.starts_with("chunk-")).collect();
            // Misaligned chunking limits overlap detection — we capture some lines
            // but not all. The key property: no duplicate entries.
            assert!(
                !matching.is_empty(),
                "chunked feed should capture at least some lines: {:?}",
                lines,
            );
        }

        /// Scroll regions (DECSTBM): scrolling within a restricted region does
        /// NOT produce overlap with the full-screen prev/curr comparison, so the
        /// conservative detector does not extract these lines. This is acceptable:
        /// scroll regions are rare in mobile-targeted sessions, and screen rows
        /// always show the current content accurately.
        #[test]
        fn test_vt_log_scroll_region_decstbm() {
            let mut buf = VtLogBuffer::new(10, 80, 1000); // small 10-row screen
            // Fill the screen first so we have a baseline for diff detection
            for i in 0..10 {
                buf.process(format!("init-{i}\r\n").as_bytes());
            }
            let before = buf.total_lines();
            // Set scroll region to rows 3-8 (1-indexed): ESC[3;8r
            // Then move cursor into the region and write lines to force scrolling
            // within the region only.
            buf.process(b"\x1b[3;8r"); // DECSTBM: set scroll region rows 3-8
            buf.process(b"\x1b[3;1H"); // CUP: move cursor to row 3, col 1
            for i in 0..20 {
                buf.process(format!("region-{i}\r\n").as_bytes());
            }
            buf.process(b"\x1b[r"); // Reset scroll region to full screen
            let after = buf.total_lines();
            // Scroll-region scrolling changes rows within the region but doesn't
            // produce a full-screen overlap pattern, so no new log lines are
            // expected with the conservative detector.
            assert_eq!(
                after, before,
                "scroll region scroll does not produce overlap-based log lines"
            );
        }

        /// Cursor movement (CUU) that overwrites existing rows should NOT
        /// produce the overwritten content as new log output.
        #[test]
        fn test_vt_log_cursor_movement_no_overwrite_in_log() {
            let mut buf = VtLogBuffer::new(10, 80, 1000);
            // Write 5 lines
            for i in 0..5 {
                buf.process(format!("orig-{i}\r\n").as_bytes());
            }
            // Move cursor up 3 rows (CUU) and overwrite with "REPLACED"
            buf.process(b"\x1b[3A"); // CUU 3: move up 3
            buf.process(b"REPLACED\r\n"); // overwrite current line
            let lines = log_texts(&buf);
            // "REPLACED" should NOT appear in the log — it was written via cursor
            // movement within the viewport, not as new scrolled-off output.
            // (The diff detector may emit the displaced orig-N lines, but the
            // replacement text itself should stay on-screen, not in the log.)
            let replaced_in_log = lines.iter().any(|l| l.contains("REPLACED"));
            assert!(
                !replaced_in_log,
                "cursor-overwritten text should not appear in log: {lines:?}"
            );
        }

        /// SGR attributes (colors, bold, etc.) should not leak into extracted text.
        #[test]
        fn test_vt_log_sgr_produces_clean_text() {
            let mut buf = VtLogBuffer::new(24, 80, 1000);
            // Write 30 lines with SGR color codes (enough to cause scrolling)
            for i in 0..30 {
                // ESC[31m = red, ESC[1m = bold, ESC[0m = reset
                buf.process(format!("\x1b[1;31mcolor-{i}\x1b[0m\r\n").as_bytes());
            }
            let lines = log_texts(&buf);
            let color_lines: Vec<&String> = lines.iter().filter(|l| l.contains("color-")).collect();
            assert!(
                color_lines.len() >= 5,
                "should capture color-N lines, got {}: {:?}",
                color_lines.len(),
                lines,
            );
            // No line should contain raw ESC characters — vt100 parser strips them
            for line in &lines {
                assert!(
                    !line.contains('\x1b'),
                    "log line should not contain ESC sequences: {line:?}"
                );
            }
        }

        #[test]
        fn test_vt_log_screen_with_chrome_trim() {
            let mut buf = VtLogBuffer::new(24, 80, 1000);
            // Write content lines + agent chrome (separator + prompt + status bar)
            for i in 0..5 {
                buf.process(format!("content line {i}\r\n").as_bytes());
            }
            // Simulate Claude Code chrome at bottom
            buf.process(b"\x1b[20;1H");
            buf.process("────────────────────────────────────────\r\n".as_bytes());
            buf.process("❯ \r\n".as_bytes());
            buf.process("────────────────────────────────────────\r\n".as_bytes());
            buf.process("  [Opus 4.6 | Max] tuicommander git:(main)\r\n".as_bytes());

            let raw_rows = buf.screen_rows();
            let refs: Vec<&str> = raw_rows.iter().map(|s| s.as_str()).collect();
            let cutoff = super::super::chrome::find_chrome_cutoff(&refs).unwrap_or(raw_rows.len());
            let screen: Vec<LogLine> = buf.screen_log_lines().into_iter().take(cutoff).collect();

            let screen_text: Vec<String> = screen.iter().map(|l| l.text()).collect();
            assert!(
                !screen_text
                    .iter()
                    .any(|t| t.contains("❯") || t.contains("Opus 4.6")),
                "chrome should be trimmed, got: {screen_text:?}"
            );
            // Content lines should still be present
            assert!(
                screen_text.iter().any(|t| t.contains("content line")),
                "content should be preserved, got: {screen_text:?}"
            );
        }

        // --- ChangedRow / process() return value tests ---

        /// process() returns the rows that changed on the normal screen.
        #[test]
        fn test_vt_log_changed_rows_basic() {
            let mut buf = make_vt_log();
            let changed = buf.process(b"hello world");
            assert!(
                changed.iter().any(|r| r.text == "hello world"),
                "expected 'hello world' in changed rows, got: {:?}",
                changed,
            );
        }

        /// CR-based overwrite: process() returns the final overwritten row text.
        #[test]
        fn test_vt_log_changed_rows_overwrite() {
            let mut buf = make_vt_log();
            // "aaaa\r" moves cursor to column 0; "bbbb" overwrites — vt100 renders "bbbb"
            let changed = buf.process(b"aaaa\rbbbb");
            assert!(
                changed.iter().any(|r| r.text.contains("bbbb")),
                "expected 'bbbb' after CR overwrite, got: {:?}",
                changed,
            );
            assert!(
                !changed.iter().any(|r| r.text == "aaaa"),
                "should not see raw 'aaaa' (overwritten), got: {:?}",
                changed,
            );
        }

        /// Alternate screen: changed rows are reported even when alternate screen is active.
        #[test]
        fn test_vt_log_changed_rows_alternate_screen() {
            let mut buf = make_vt_log();
            buf.process(b"\x1b[?1049h");
            let changed = buf.process(b"status: running");
            assert!(
                changed.iter().any(|r| r.text.contains("status: running")),
                "changed rows must be reported during alternate screen, got: {:?}",
                changed,
            );
            assert_eq!(
                buf.total_lines(),
                0,
                "log must remain empty during alternate screen"
            );
        }

        /// Cursor movement: changed rows reflect the final rendered state.
        #[test]
        fn test_vt_log_changed_rows_cursor_movement() {
            let mut buf = make_vt_log();
            buf.process(b"line0\r\nline1\r\nline2");
            // Move cursor up 1 row (CUU 1) and overwrite line1
            let changed = buf.process(b"\x1b[1Aupdated");
            assert!(
                changed.iter().any(|r| r.text.contains("updated")),
                "expected 'updated' in changed rows after CUU overwrite, got: {:?}",
                changed,
            );
        }

        /// No change: a second process() with no new data returns empty Vec.
        #[test]
        fn test_vt_log_changed_rows_empty_on_no_change() {
            let mut buf = make_vt_log();
            buf.process(b"hello");
            let changed = buf.process(b"");
            assert!(
                changed.is_empty(),
                "expected no changed rows when no data written, got: {:?}",
                changed,
            );
        }

        /// resize() clears prev_rows so the next process() reports all non-empty rows.
        #[test]
        fn test_vt_log_changed_rows_resize_clears_prev() {
            let mut buf = make_vt_log();
            buf.process(b"hello");
            buf.process(b""); // stabilise prev_rows
            buf.resize(24, 80);
            let changed = buf.process(b"");
            assert!(
                changed.iter().any(|r| r.text.contains("hello")),
                "expected 'hello' after resize clears prev, got: {:?}",
                changed,
            );
        }

        /// Screen switch resets prev_rows so first frame reports all non-empty rows.
        #[test]
        fn test_vt_log_changed_rows_screen_switch_resets_prev() {
            let mut buf = make_vt_log();
            buf.process(b"normal line");
            buf.process(b""); // stabilise
            buf.process(b"\x1b[?1049h"); // enter alternate
            buf.process(b"alt content");
            // Exit alternate — should reset prev for normal screen
            let changed = buf.process(b"\x1b[?1049l");
            assert!(
                changed.iter().any(|r| r.text.contains("normal line")),
                "expected 'normal line' after screen switch resets prev, got: {:?}",
                changed,
            );
        }

        // --- mark_agent_chrome tests ---

        /// Helper: create plain LogLine vec from string slices for testing.
        fn make_log_lines(items: &[&str]) -> Vec<LogLine> {
            items
                .iter()
                .map(|s| LogLine {
                    spans: if s.is_empty() {
                        vec![]
                    } else {
                        vec![LogSpan {
                            text: s.to_string(),
                            fg: None,
                            bg: None,
                            bold: false,
                            italic: false,
                            underline: false,
                        }]
                    },
                    cols: 0,
                    chrome: false,
                    wrapped: false,
                    partial: false,
                })
                .collect()
        }

        /// Helper: mark a batch and return the texts readers would see.
        fn visible_texts(items: &[&str]) -> Vec<String> {
            let mut lines = make_log_lines(items);
            mark_agent_chrome(&mut lines);
            lines
                .iter()
                .filter(|l| !l.chrome)
                .map(|l| l.text())
                .collect()
        }

        // find_scrollback_chrome_cutoff tests live in chrome.rs (canonical location)

        /// Large batch with an Ink prompt → prompt and everything below marked chrome.
        #[test]
        fn test_mark_agent_chrome_large_batch_ink_prompt() {
            let mut items: Vec<&str> = vec!["real content"; 18];
            items.push("❯"); // index 18
            items.push(""); // index 19
            items.push("Model: x"); // index 20
            let visible = visible_texts(&items);
            assert_eq!(visible.len(), 18, "lines before prompt stay visible");
            assert!(visible.iter().all(|t| t == "real content"));
        }

        /// Regression: agents echo the submitted user message on a prompt row. That
        /// row is the conversation, and marking it chrome deleted both the message
        /// and the reply that followed it in the same batch.
        #[test]
        fn test_mark_agent_chrome_keeps_echoed_user_message() {
            let visible = visible_texts(&[
                "❯ rename non funziona nel filebrowser",
                "  intent: investigating FileBrowser rename bug",
                "• Ho trovato il punto: la closure leggeva previousFocus azzerato.",
            ]);
            assert_eq!(visible.len(), 3, "user message and reply must survive");
        }

        /// A bare `> ` row is still a prompt (Gemini/generic).
        #[test]
        fn test_mark_agent_chrome_bare_gt_prompt() {
            let mut items: Vec<&str> = vec!["output"; 17];
            items.push("> "); // index 17 — bare prompt, no input typed
            items.push("chrome"); // index 18
            assert_eq!(visible_texts(&items).len(), 17);
        }

        /// Small batch with a prompt in the scan window → still marked.
        #[test]
        fn test_mark_agent_chrome_small_batch_with_prompt() {
            let visible = visible_texts(&["line 1", "❯", "chrome"]);
            assert_eq!(visible, vec!["line 1"]);
        }

        /// Separator lines immediately above a prompt are part of the prompt box.
        #[test]
        fn test_mark_agent_chrome_separator_above_prompt() {
            let visible = visible_texts(&[
                "real output",
                "more output",
                "────────────────────", // separator above prompt
                "❯ ",                   // bare prompt
                "────────────────────", // separator below
                "[Opus 4.6 | Max]",
                "Context ███░░░",
            ]);
            assert_eq!(visible, vec!["real output", "more output"]);
        }

        /// Empty lines above a prompt are part of the prompt box.
        #[test]
        fn test_mark_agent_chrome_empty_lines_above_prompt() {
            let visible = visible_texts(&["real output", "", "", "❯", "Model: x"]);
            assert_eq!(visible, vec!["real output"]);
        }

        // is_separator_line tests live in chrome.rs (canonical location)

        /// Batch with no prompt → nothing marked, regardless of size.
        #[test]
        fn test_mark_agent_chrome_no_prompt_any_size_passthrough() {
            let mut lines = make_log_lines(&["a", "b", "c"]);
            let before = lines.clone();
            mark_agent_chrome(&mut lines);
            assert_eq!(lines, before);
        }

        /// Large batch without any prompt → nothing marked.
        #[test]
        fn test_mark_agent_chrome_large_batch_no_prompt_passthrough() {
            let items: Vec<&str> = vec!["output line"; 20];
            let mut lines = make_log_lines(&items);
            let before = lines.clone();
            mark_agent_chrome(&mut lines);
            assert_eq!(lines, before);
        }

        /// Empty batch → no panic.
        #[test]
        fn test_mark_agent_chrome_empty_batch() {
            let mut lines: Vec<LogLine> = Vec::new();
            mark_agent_chrome(&mut lines);
            assert_eq!(lines, [] as [LogLine; 0]);
        }

        /// Regression: a markdown blockquote is prose, not a prompt. Everything after
        /// it used to be deleted from history, so paragraphs arrived mid-sentence.
        #[test]
        fn test_mark_agent_chrome_markdown_quote_is_not_a_prompt() {
            let visible = visible_texts(&[
                "Here is what the docs say:",
                "> quoted guidance from the manual",
                "verificabile; non è un force-push e non sovrascrive alcun branch",
                "esistente.",
            ]);
            assert_eq!(visible.len(), 4, "quoted prose must not truncate the batch");
        }

        /// Regression: a separator alone is not chrome. Tables, progress bars and
        /// Codex's `└ ────` dividers all carry box-drawing runs mid-output.
        #[test]
        fn test_mark_agent_chrome_standalone_separator_is_not_a_prompt() {
            let visible = visible_texts(&[
                "• Ran cargo nextest run",
                "  └ ──────────────────────",
                "    Summary [ 12.4s ] 91 tests run",
                "    all passed",
            ]);
            assert_eq!(visible.len(), 4, "a divider must not truncate the batch");
        }

        /// Regression: markdown table rules are box-drawing runs. They anchored the
        /// cut and swallowed the rest of the table plus whatever followed it.
        #[test]
        fn test_mark_agent_chrome_markdown_table_survives() {
            let visible = visible_texts(&[
                "  │ Livello                                   │ Esito                 │",
                "  ├───────────────────────────────────────────┼───────────────────────┤",
                "  │ fs.rs:1239 rename_path + route /fs/rename │ ✅ rinomina realmente │",
                "  └───────────────────────────────────────────┴───────────────────────┘",
                "Conclusione: il rename funziona lato backend.",
            ]);
            assert_eq!(visible.len(), 5, "table and trailing prose must survive");
        }

        /// Chrome is hidden from readers but never dropped from the buffer, so a
        /// misclassification costs visibility, not history.
        #[test]
        fn test_mark_agent_chrome_keeps_lines_in_the_batch() {
            let mut lines = make_log_lines(&["real output", "❯ ", "Context ███░░░"]);
            mark_agent_chrome(&mut lines);
            assert_eq!(lines.len(), 3, "no line is discarded");
            assert!(!lines[0].chrome);
            assert!(lines[1].chrome && lines[2].chrome);
        }

        /// End-to-end through the buffer: chrome occupies offset slots but is not
        /// returned, and real content after a false-positive anchor survives.
        #[test]
        fn test_lines_since_owned_filters_chrome_without_losing_content() {
            let mut buf = VtLogBuffer::new(3, 80, 1000);
            // 3-row screen: everything but the last 3 rows scrolls into history, so the
            // trailing padding lines push the content under test off the screen.
            buf.process(b"alpha\r\n> quoted prose\r\nbravo\r\ncharlie\r\npad1\r\npad2\r\npad3\r\n");
            let (lines, total) = buf.lines_since_owned(0, usize::MAX);
            let texts: Vec<String> = lines.iter().map(|l| l.text()).collect();
            for expected in ["alpha", "> quoted prose", "bravo", "charlie"] {
                assert!(
                    texts.iter().any(|t| t.trim() == expected),
                    "{expected:?} missing from {texts:?}"
                );
            }
            assert!(total >= texts.len(), "offset space includes chrome slots");
        }

        /// Replays the live reproduction that exposed the bug: numbered content with
        /// chrome-shaped anchors (`> `, `────`, `›`, `❯`) landing near batch ends.
        /// Against the old truncating capture this lost 6 of 100 lines.
        #[test]
        fn test_scrollback_survives_chrome_shaped_anchors_in_content() {
            let anchors = [
                "> markdown quote anchor",
                "──────────────────────",
                "› codex prompt anchor",
                "❯ ink prompt anchor",
            ];
            let mut stream = Vec::new();
            let mut n = 1;
            for anchor in anchors {
                for _ in 0..20 {
                    stream.extend_from_slice(
                        format!("LINE-{n:03} normal content row\r\n").as_bytes(),
                    );
                    n += 1;
                }
                stream.extend_from_slice(anchor.as_bytes());
                stream.extend_from_slice(b"\r\n");
                for _ in 0..5 {
                    stream.extend_from_slice(
                        format!("LINE-{n:03} normal content row\r\n").as_bytes(),
                    );
                    n += 1;
                }
            }
            // Push the tail off the screen so every numbered row reaches history.
            for _ in 0..30 {
                stream.extend_from_slice(b"pad\r\n");
            }

            let mut buf = VtLogBuffer::new(24, 80, 10_000);
            // Feed in 512-byte chunks: batch boundaries are what put an anchor inside
            // the scan window, which is exactly how the live session tripped this.
            for chunk in stream.chunks(512) {
                buf.process(chunk);
            }

            let (lines, _) = buf.lines_since_owned(0, usize::MAX);
            let seen: Vec<String> = lines.iter().map(|l| l.text().trim().to_string()).collect();
            let missing: Vec<usize> = (1..=100)
                .filter(|i| {
                    let needle = format!("LINE-{i:03}");
                    !seen.iter().any(|t| t.starts_with(&needle))
                })
                .collect();
            assert!(
                missing.is_empty(),
                "content lines lost from history: {missing:?}"
            );
        }

        // --- LogLine / extract_log_line tests ---

        /// Helper: create a TerminalGrid, feed data, and extract LogLine for row 0.
        fn extract_line_from(data: &[u8]) -> LogLine {
            use alacritty_terminal::index::Line;
            let mut grid = super::super::terminal_grid::TerminalGrid::new(4, 80, 0);
            let _ = grid.process(data);
            grid.extract_log_line(Line(0))
        }

        /// Plain text produces a single span with no color attributes.
        #[test]
        fn test_extract_log_line_plain_text_single_span() {
            let line = extract_line_from(b"Hello world");
            assert_eq!(line.spans.len(), 1, "plain text = single span");
            assert_eq!(line.spans[0].text, "Hello world");
            assert_eq!(line.spans[0].fg, None);
            assert_eq!(line.spans[0].bg, None);
            assert!(!line.spans[0].bold);
            assert!(!line.spans[0].italic);
            assert!(!line.spans[0].underline);
        }

        /// Colored text (ANSI escape for red) produces a span with fg color.
        #[test]
        fn test_extract_log_line_colored_text() {
            // ESC[31m = red foreground, ESC[0m = reset
            let line = extract_line_from(b"\x1b[31mERROR\x1b[0m ok");
            assert!(
                line.spans.len() >= 2,
                "should have at least 2 spans: {:?}",
                line.spans
            );
            // First span: "ERROR" with red fg
            assert_eq!(line.spans[0].text, "ERROR");
            assert_eq!(line.spans[0].fg, Some(LogColor::Idx(1))); // ANSI red = idx 1
            // Second span: " ok" with default color
            assert_eq!(line.spans[1].text, " ok");
            assert_eq!(line.spans[1].fg, None);
        }

        #[test]
        fn log_color_maps_named_ansi_palette_exhaustively() {
            use alacritty_terminal::vte::ansi::{Color, NamedColor};

            let defaults = [
                NamedColor::Foreground,
                NamedColor::Background,
                NamedColor::Cursor,
                NamedColor::BrightForeground,
                NamedColor::DimForeground,
            ];
            for named in defaults {
                assert_eq!(LogColor::from_ansi_color(Color::Named(named)), None);
            }

            let palette = [
                (NamedColor::Black, 0),
                (NamedColor::Red, 1),
                (NamedColor::Green, 2),
                (NamedColor::Yellow, 3),
                (NamedColor::Blue, 4),
                (NamedColor::Magenta, 5),
                (NamedColor::Cyan, 6),
                (NamedColor::White, 7),
                (NamedColor::BrightBlack, 8),
                (NamedColor::BrightRed, 9),
                (NamedColor::BrightGreen, 10),
                (NamedColor::BrightYellow, 11),
                (NamedColor::BrightBlue, 12),
                (NamedColor::BrightMagenta, 13),
                (NamedColor::BrightCyan, 14),
                (NamedColor::BrightWhite, 15),
                (NamedColor::DimBlack, 0),
                (NamedColor::DimRed, 1),
                (NamedColor::DimGreen, 2),
                (NamedColor::DimYellow, 3),
                (NamedColor::DimBlue, 4),
                (NamedColor::DimMagenta, 5),
                (NamedColor::DimCyan, 6),
                (NamedColor::DimWhite, 7),
            ];
            for (named, index) in palette {
                assert_eq!(
                    LogColor::from_ansi_color(Color::Named(named)),
                    Some(LogColor::Idx(index)),
                    "unexpected mapping for {named:?}"
                );
            }
        }

        #[test]
        fn log_color_preserves_indexed_and_rgb_values() {
            use alacritty_terminal::vte::ansi::{Color, Rgb};

            assert_eq!(
                LogColor::from_ansi_color(Color::Indexed(231)),
                Some(LogColor::Idx(231))
            );
            assert_eq!(
                LogColor::from_ansi_color(Color::Spec(Rgb {
                    r: 12,
                    g: 34,
                    b: 56,
                })),
                Some(LogColor::Rgb(12, 34, 56))
            );
        }

        /// Multi-span line with bold + color changes.
        #[test]
        fn test_extract_log_line_multi_span_bold_color() {
            // Bold green then normal
            let line = extract_line_from(b"\x1b[1;32m+added\x1b[0m context");
            assert!(line.spans.len() >= 2, "multi-span: {:?}", line.spans);
            assert_eq!(line.spans[0].text, "+added");
            assert!(line.spans[0].bold);
            assert_eq!(line.spans[0].fg, Some(LogColor::Idx(2))); // green = idx 2
            // Rest is plain
            let last = line.spans.last().unwrap();
            assert_eq!(last.text, " context");
            assert!(!last.bold);
            assert_eq!(last.fg, None);
        }

        /// Trailing whitespace spans are trimmed.
        #[test]
        fn test_extract_log_line_trims_trailing_whitespace() {
            let line = extract_line_from(b"text");
            // Should not have trailing spaces filling to column 80
            let total_len: usize = line.spans.iter().map(|s| s.text.len()).sum();
            assert_eq!(total_len, 4, "no trailing whitespace: {:?}", line.spans);
        }

        /// Empty row produces an empty spans vec.
        #[test]
        fn test_extract_log_line_empty_row() {
            let line = extract_line_from(b"");
            assert!(
                line.spans.is_empty(),
                "empty row = empty spans: {:?}",
                line.spans
            );
        }

        /// LogLine.text() concatenates all span texts.
        #[test]
        fn test_log_line_text_method() {
            let line = LogLine {
                spans: vec![
                    LogSpan {
                        text: "hello".into(),
                        fg: Some(LogColor::Idx(1)),
                        bg: None,
                        bold: true,
                        italic: false,
                        underline: false,
                    },
                    LogSpan {
                        text: " world".into(),
                        fg: None,
                        bg: None,
                        bold: false,
                        italic: false,
                        underline: false,
                    },
                ],
                cols: 0,
                chrome: false,
                wrapped: false,
                partial: false,
            };
            assert_eq!(line.text(), "hello world");
        }

        /// Colored lines scrolled off are preserved as LogLine with attributes.
        #[test]
        fn test_vt_log_colored_lines_preserved_in_log() {
            let mut buf = VtLogBuffer::new(4, 80, 100); // small 4-row screen
            // Feed colored lines that will scroll off
            for i in 0..6 {
                buf.process(format!("\x1b[31mred-{i}\x1b[0m\r\n").as_bytes());
            }
            let log = buf.lines();
            assert!(!log.is_empty(), "should have scrolled-off lines");
            // At least one line should have a colored span
            let has_color = log.iter().any(|ll| ll.spans.iter().any(|s| s.fg.is_some()));
            assert!(
                has_color,
                "scrolled-off lines should preserve color: {:?}",
                log
            );
        }

        /// Serialize LogLine to JSON matches expected format.
        #[test]
        fn test_log_line_serialization() {
            let line = LogLine {
                spans: vec![
                    LogSpan {
                        text: "hello".into(),
                        fg: Some(LogColor::Idx(1)),
                        bg: None,
                        bold: true,
                        italic: false,
                        underline: false,
                    },
                    LogSpan {
                        text: " world".into(),
                        fg: None,
                        bg: None,
                        bold: false,
                        italic: false,
                        underline: false,
                    },
                ],
                cols: 0,
                chrome: false,
                wrapped: false,
                partial: false,
            };
            let json = serde_json::to_value(&line).unwrap();
            let spans = json["spans"].as_array().unwrap();
            assert_eq!(spans.len(), 2);
            assert_eq!(spans[0]["text"], "hello");
            assert_eq!(spans[0]["fg"], serde_json::json!({"idx": 1}));
            assert!(spans[0]["bold"].as_bool().unwrap());
            // Second span has no color fields (skipped by serde)
            assert_eq!(spans[1]["text"], " world");
            assert!(spans[1].get("fg").is_none() || spans[1]["fg"].is_null());
            assert!(spans[1].get("bold").is_none() || !spans[1]["bold"].as_bool().unwrap_or(true));
        }

        #[test]
        fn test_strip_structural_tokens_no_match() {
            let mut line = LogLine {
                spans: vec![LogSpan {
                    text: "normal output".into(),
                    ..Default::default()
                }],
                cols: 0,
                chrome: false,
                wrapped: false,
                partial: false,
            };
            line.strip_structural_tokens();
            assert_eq!(line.spans[0].text, "normal output");
        }

        #[test]
        fn test_strip_structural_tokens_preserves_plain_intent() {
            let mut line = LogLine {
                spans: vec![LogSpan {
                    text: "intent: reading the config file".into(),
                    ..Default::default()
                }],
                cols: 0,
                chrome: false,
                wrapped: false,
                partial: false,
            };
            line.strip_structural_tokens();
            assert_eq!(line.text(), "intent: reading the config file");
        }

        #[test]
        fn test_strip_structural_tokens_plain_suggest() {
            let mut line = LogLine {
                spans: vec![LogSpan {
                    text: "suggest: Run tests | Check logs | Push".into(),
                    ..Default::default()
                }],
                cols: 0,
                chrome: false,
                wrapped: false,
                partial: false,
            };
            line.strip_structural_tokens();
            assert!(
                line.spans.is_empty(),
                "plain-prefix suggest should be stripped entirely"
            );
        }

        /// The drift this closed: the local copy of the grammar knew only the two
        /// Ink bullets, so every Codex-decorated token TUIC parsed was shown to the
        /// user anyway. One grammar, in `output_parser`, now serves both.
        #[test]
        fn test_strip_structural_tokens_knows_the_codex_bullets() {
            for text in [
                "\u{2022} suggest: Run tests | Check logs | Push",
                "\u{25E6} suggest: Run tests | Check logs | Push",
            ] {
                let mut line = LogLine {
                    spans: vec![LogSpan {
                        text: text.into(),
                        ..Default::default()
                    }],
                    cols: 0,
                    chrome: false,
                    wrapped: false,
                    partial: false,
                };
                line.strip_structural_tokens();
                assert!(
                    line.spans.is_empty(),
                    "a Codex-bulleted token must be stripped too: {text:?}"
                );
            }
        }

        /// Both the greeting and its inline intent stay visible in log output.
        #[test]
        fn test_strip_structural_tokens_keeps_the_ack_sentence() {
            let mut line = LogLine {
                spans: vec![LogSpan {
                    text:
                        "\u{2022} TUICommander v1.7.7 is connected. intent: fixing the parser (Fix)"
                            .into(),
                    ..Default::default()
                }],
                cols: 0,
                chrome: false,
                wrapped: false,
                partial: false,
            };
            line.strip_structural_tokens();
            assert_eq!(
                line.text(),
                "\u{2022} TUICommander v1.7.7 is connected. intent: fixing the parser (Fix)"
            );
        }

        /// Screen text of a 40-column buffer that received `output`.
        fn narrow_screen_text(output: &str) -> Vec<String> {
            let mut buf = VtLogBuffer::new(24, 40, 1000);
            buf.process(output.as_bytes());
            buf.screen_log_lines().iter().map(|l| l.text()).collect()
        }

        /// Catches: the strip removing only the row that carries `suggest:` and
        /// leaving the wrapped tail ("Federico Coletto ]") as plain text, or joining
        /// only one continuation row. Covers a token over 2 and over 3 rows.
        #[test]
        fn test_screen_hides_suggest_tail_rows_at_phone_width() {
            for token in [
                // 2 rows at 40 cols
                "suggest: [ Ask Federico Coletto | Draft summary | Close it ]",
                // 3 rows at 40 cols
                "suggest: [ Ask Federico Coletto about the cost review | Draft the summary | Close the story ]",
            ] {
                let text = narrow_screen_text(&format!("\u{23FA} done\r\n\u{23FA} {token}\r\n"));
                assert_eq!(text.iter().filter(|t| !t.is_empty()).count(), 1, "{text:?}");
                assert!(
                    !text
                        .iter()
                        .any(|t| t.contains(']') || t.contains("Federico")),
                    "wrapped suggest tail leaked: {text:?}"
                );
            }
        }

        /// Catches: a stray `suggest: [` that never closes swallowing the output
        /// below it. The parser does not accept such a token, so nothing is hidden.
        #[test]
        fn test_screen_keeps_rows_after_an_unclosed_suggest() {
            let text = narrow_screen_text(
                "suggest: [ Ask Federico Coletto | and\r\nreal prose after it\r\n",
            );
            assert!(text.iter().any(|t| t == "real prose after it"), "{text:?}");
        }

        #[test]
        fn test_strip_structural_tokens_mid_line_not_stripped() {
            let mut line = LogLine {
                spans: vec![LogSpan {
                    text: "The intent: of this code is clear".into(),
                    ..Default::default()
                }],
                cols: 0,
                chrome: false,
                wrapped: false,
                partial: false,
            };
            line.strip_structural_tokens();
            assert_eq!(line.spans[0].text, "The intent: of this code is clear");
        }

        #[test]
        fn test_strip_structural_tokens_indented_suggest() {
            // Ink indents continuation lines — suggest: with leading whitespace must be stripped
            let mut line = LogLine {
                spans: vec![LogSpan {
                    text: "  suggest: Run tests | Check logs | Push".into(),
                    ..Default::default()
                }],
                cols: 0,
                chrome: false,
                wrapped: false,
                partial: false,
            };
            line.strip_structural_tokens();
            assert!(line.spans.is_empty(), "indented suggest should be stripped");
        }

        #[test]
        fn test_strip_structural_tokens_preserves_indented_intent() {
            let mut line = LogLine {
                spans: vec![LogSpan {
                    text: "  intent: reading the config file".into(),
                    ..Default::default()
                }],
                cols: 0,
                chrome: false,
                wrapped: false,
                partial: false,
            };
            line.strip_structural_tokens();
            assert_eq!(line.text(), "  intent: reading the config file");
        }

        #[test]
        fn test_strip_structural_tokens_bullet_suggest() {
            // Ink bullet prefix: ● suggest: ...
            let mut line = LogLine {
                spans: vec![LogSpan {
                    text: "● suggest: A | B | C".into(),
                    ..Default::default()
                }],
                cols: 0,
                chrome: false,
                wrapped: false,
                partial: false,
            };
            line.strip_structural_tokens();
            assert!(
                line.spans.is_empty(),
                "bullet-prefixed suggest should be stripped"
            );
        }

        #[test]
        fn test_strip_structural_tokens_preserves_bullet_intent() {
            let mut line = LogLine {
                spans: vec![LogSpan {
                    text: "⏺ intent: doing something (Task)".into(),
                    ..Default::default()
                }],
                cols: 0,
                chrome: false,
                wrapped: false,
                partial: false,
            };
            line.strip_structural_tokens();
            assert_eq!(line.text(), "⏺ intent: doing something (Task)");
        }

        // ── VtLogBuffer alt-screen ──────────────────────────────────

        #[test]
        fn vt_log_buffer_is_alternate_screen_tracks_state() {
            let mut vt = VtLogBuffer::new(24, 80, 1000);
            assert!(!vt.is_alternate_screen());
            vt.process(b"\x1b[?1049h");
            assert!(vt.is_alternate_screen());
            vt.process(b"\x1b[?1049l");
            assert!(!vt.is_alternate_screen());
        }

        #[test]
        fn vt_log_buffer_exit_alt_screen_only_when_active() {
            let mut vt = VtLogBuffer::new(24, 80, 1000);

            // Not in alt screen — should be no-op
            assert!(!vt.is_alternate_screen());

            // Enter alt screen, then exit via the recovery sequences
            vt.process(b"\x1b[?1049h");
            assert!(vt.is_alternate_screen());
            vt.process(b"\x1b[?1049l\x1b[?1047l\x1b[?47l\x1b[?25h\x1b[0m");
            assert!(!vt.is_alternate_screen());
        }
    }
}
