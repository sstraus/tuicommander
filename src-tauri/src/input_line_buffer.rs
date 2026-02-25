/// Virtual line editor that reconstructs user input from raw PTY keystrokes.
///
/// Handles cursor movement, word operations, and editing sequences to maintain
/// an accurate representation of what the user typed, even with readline-style
/// editing (Ctrl+A/E, Ctrl+W, Alt+B/F, arrow keys, etc.).
///
/// The buffer does NOT attempt to track history navigation (Up/Down arrows,
/// Ctrl+P/N) — those replace the entire line in the shell, and we can't know
/// what they replace it with from the write side alone.
/// A line-editing buffer that tracks cursor position and content.
#[derive(Debug)]
pub(crate) struct InputLineBuffer {
    /// The character content of the current line.
    chars: Vec<char>,
    /// Cursor position as a character index (0 = before first char).
    cursor: usize,
    /// State machine for multi-byte escape sequences.
    esc_state: EscState,
    /// Accumulator for CSI parameter bytes (digits and semicolons).
    csi_params: Vec<u8>,
}

/// Escape sequence parser state.
#[derive(Debug, PartialEq)]
enum EscState {
    /// Normal character input.
    Normal,
    /// Received ESC (0x1B), waiting for next byte.
    Esc,
    /// Inside CSI sequence (ESC [), collecting parameter bytes.
    Csi,
    /// Inside SS3 sequence (ESC O), waiting for final byte.
    Ss3,
}

/// Result of feeding data to the buffer.
#[derive(Debug)]
pub(crate) enum InputAction {
    /// A complete line was submitted (Enter pressed). Contains the line text.
    Line(String),
    /// Input was interrupted (Ctrl+C). Buffer was cleared.
    Interrupt,
}

impl InputLineBuffer {
    pub(crate) fn new() -> Self {
        Self {
            chars: Vec::with_capacity(256),
            cursor: 0,
            esc_state: EscState::Normal,
            csi_params: Vec::new(),
        }
    }

    /// Feed raw PTY write data into the buffer.
    /// Returns a list of actions (typically 0 or 1 Line actions per call).
    pub(crate) fn feed(&mut self, data: &str) -> Vec<InputAction> {
        let mut actions = Vec::new();
        for ch in data.chars() {
            if let Some(action) = self.feed_char(ch) {
                actions.push(action);
            }
        }
        actions
    }

    /// Get the current buffer content (for debugging/inspection).
    #[cfg(test)]
    pub(crate) fn content(&self) -> String {
        self.chars.iter().collect()
    }

    /// Get current cursor position.
    #[cfg(test)]
    pub(crate) fn cursor_pos(&self) -> usize {
        self.cursor
    }

    fn feed_char(&mut self, ch: char) -> Option<InputAction> {
        match self.esc_state {
            EscState::Normal => self.handle_normal(ch),
            EscState::Esc => self.handle_esc(ch),
            EscState::Csi => self.handle_csi(ch),
            EscState::Ss3 => self.handle_ss3(ch),
        }
    }

    fn handle_normal(&mut self, ch: char) -> Option<InputAction> {
        match ch {
            // ESC — start escape sequence
            '\x1b' => {
                self.esc_state = EscState::Esc;
                None
            }
            // Enter (CR or LF) — submit line
            '\r' | '\n' => {
                let line: String = self.chars.iter().collect();
                self.chars.clear();
                self.cursor = 0;
                Some(InputAction::Line(line))
            }
            // Backspace (DEL) or Ctrl+H — delete char before cursor
            '\x7f' | '\x08' => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.chars.remove(self.cursor);
                }
                None
            }
            // Ctrl+A — move to beginning of line
            '\x01' => {
                self.cursor = 0;
                None
            }
            // Ctrl+B — move cursor left one char
            '\x02' => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                None
            }
            // Ctrl+C — interrupt, clear buffer
            '\x03' => {
                self.chars.clear();
                self.cursor = 0;
                Some(InputAction::Interrupt)
            }
            // Ctrl+D — delete char at cursor (or ignored if empty)
            '\x04' => {
                if self.cursor < self.chars.len() {
                    self.chars.remove(self.cursor);
                }
                None
            }
            // Ctrl+E — move to end of line
            '\x05' => {
                self.cursor = self.chars.len();
                None
            }
            // Ctrl+F — move cursor right one char
            '\x06' => {
                if self.cursor < self.chars.len() {
                    self.cursor += 1;
                }
                None
            }
            // Ctrl+K — kill to end of line
            '\x0b' => {
                self.chars.truncate(self.cursor);
                None
            }
            // Ctrl+L — clear screen (don't clear buffer, shell redraws)
            '\x0c' => None,
            // Ctrl+T — transpose chars
            '\x14' => {
                if self.cursor > 0 && self.chars.len() >= 2 {
                    let pos = if self.cursor >= self.chars.len() {
                        self.chars.len() - 1
                    } else {
                        self.cursor
                    };
                    if pos > 0 {
                        self.chars.swap(pos - 1, pos);
                        self.cursor = (pos + 1).min(self.chars.len());
                    }
                }
                None
            }
            // Ctrl+U — kill to beginning of line
            '\x15' => {
                self.chars.drain(..self.cursor);
                self.cursor = 0;
                None
            }
            // Ctrl+W — delete word backward
            '\x17' => {
                self.delete_word_backward();
                None
            }
            // Ctrl+Y — yank (we can't track kill ring, so ignore)
            '\x19' => None,
            // Tab, Ctrl+N, Ctrl+P, Ctrl+R, Ctrl+S, etc. — ignore (shell-level ops)
            '\x09' | '\x0e' | '\x10' | '\x12' | '\x13' => None,
            // Other control chars we don't handle
            c if c < '\x20' => None,
            // Regular printable character — insert at cursor
            c => {
                self.chars.insert(self.cursor, c);
                self.cursor += 1;
                None
            }
        }
    }

    fn handle_esc(&mut self, ch: char) -> Option<InputAction> {
        match ch {
            // ESC [ — CSI sequence
            '[' => {
                self.esc_state = EscState::Csi;
                self.csi_params.clear();
                None
            }
            // ESC O — SS3 sequence (application cursor mode)
            'O' => {
                self.esc_state = EscState::Ss3;
                None
            }
            // ESC + CR — Shift+Enter in kitty protocol (multi-line newline)
            '\r' => {
                self.esc_state = EscState::Normal;
                // Insert a literal newline into buffer instead of submitting
                self.chars.insert(self.cursor, '\n');
                self.cursor += 1;
                None
            }
            // Alt+B — word backward
            'b' => {
                self.esc_state = EscState::Normal;
                self.move_word_backward();
                None
            }
            // Alt+F — word forward
            'f' => {
                self.esc_state = EscState::Normal;
                self.move_word_forward();
                None
            }
            // Alt+D — delete word forward
            'd' => {
                self.esc_state = EscState::Normal;
                self.delete_word_forward();
                None
            }
            // Alt+Backspace — delete word backward
            '\x7f' => {
                self.esc_state = EscState::Normal;
                self.delete_word_backward();
                None
            }
            // Any other ESC+char — ignore and return to normal
            _ => {
                self.esc_state = EscState::Normal;
                None
            }
        }
    }

    fn handle_csi(&mut self, ch: char) -> Option<InputAction> {
        match ch {
            // Parameter bytes: digits and semicolons
            '0'..='9' | ';' => {
                self.csi_params.push(ch as u8);
                None
            }
            // Final byte — execute CSI command
            _ => {
                self.esc_state = EscState::Normal;
                self.execute_csi(ch);
                None
            }
        }
    }

    fn handle_ss3(&mut self, ch: char) -> Option<InputAction> {
        self.esc_state = EscState::Normal;
        // SS3 sequences: same navigation as CSI but in application mode
        match ch {
            'C' => {
                // Right arrow
                if self.cursor < self.chars.len() {
                    self.cursor += 1;
                }
            }
            'D' => {
                // Left arrow
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
            }
            'H' => {
                // Home
                self.cursor = 0;
            }
            'F' => {
                // End
                self.cursor = self.chars.len();
            }
            // Up/Down (A/B) — history navigation, ignore
            _ => {}
        }
        None
    }

    fn execute_csi(&mut self, final_byte: char) {
        let params = self.parse_csi_params();

        match final_byte {
            // Arrow keys and Home/End: A=up, B=down, C=right, D=left, H=home, F=end
            'A' | 'B' => {
                // Up/Down — history navigation. We can't track this, so ignore.
                // The shell will replace the entire line content; our buffer
                // will be wrong until the user types again or submits.
            }
            'C' => {
                // Right arrow. Check for modifier (Ctrl+Right = word forward).
                let modifier = params.get(1).copied().unwrap_or(1);
                if modifier == 5 || modifier == 3 {
                    // Ctrl+Right or Alt+Right → word forward
                    self.move_word_forward();
                } else {
                    // Plain right arrow
                    if self.cursor < self.chars.len() {
                        self.cursor += 1;
                    }
                }
            }
            'D' => {
                // Left arrow. Check for modifier (Ctrl+Left = word backward).
                let modifier = params.get(1).copied().unwrap_or(1);
                if modifier == 5 || modifier == 3 {
                    // Ctrl+Left or Alt+Left → word backward
                    self.move_word_backward();
                } else {
                    // Plain left arrow
                    if self.cursor > 0 {
                        self.cursor -= 1;
                    }
                }
            }
            'H' => {
                // Home key
                self.cursor = 0;
            }
            'F' => {
                // End key
                self.cursor = self.chars.len();
            }
            // Tilde sequences: ~  (param determines which key)
            '~' => {
                let key = params.first().copied().unwrap_or(0);
                match key {
                    1 => self.cursor = 0,               // Home (VT220)
                    3 => {                               // Delete key
                        if self.cursor < self.chars.len() {
                            self.chars.remove(self.cursor);
                        }
                    }
                    4 => self.cursor = self.chars.len(), // End (VT220)
                    _ => {} // Page Up/Down, Insert — ignore
                }
            }
            // Kitty CSI u sequences: ESC [ <codepoint> ; <modifier> u
            'u' => {
                let codepoint = params.first().copied().unwrap_or(0);
                let _modifier = params.get(1).copied().unwrap_or(1);
                match codepoint {
                    // Enter (13) with modifier = Shift+Enter → literal newline
                    13 => {
                        self.chars.insert(self.cursor, '\n');
                        self.cursor += 1;
                    }
                    // Backspace (127) with any modifier
                    127 => {
                        if self.cursor > 0 {
                            self.cursor -= 1;
                            self.chars.remove(self.cursor);
                        }
                    }
                    // Escape (27) — ignore
                    27 => {}
                    // Tab (9) — ignore
                    9 => {}
                    _ => {}
                }
            }
            _ => {} // Unknown CSI final byte — ignore
        }
    }

    /// Parse CSI parameter bytes into a list of numeric values.
    /// E.g., "1;5" → [1, 5]
    fn parse_csi_params(&self) -> Vec<u32> {
        if self.csi_params.is_empty() {
            return vec![];
        }
        let s: String = self.csi_params.iter().map(|&b| b as char).collect();
        s.split(';')
            .filter_map(|part| part.parse::<u32>().ok())
            .collect()
    }

    /// Move cursor backward to the start of the previous word.
    fn move_word_backward(&mut self) {
        // Skip whitespace
        while self.cursor > 0 && self.chars[self.cursor - 1].is_whitespace() {
            self.cursor -= 1;
        }
        // Skip word chars
        while self.cursor > 0 && !self.chars[self.cursor - 1].is_whitespace() {
            self.cursor -= 1;
        }
    }

    /// Move cursor forward to the end of the next word.
    fn move_word_forward(&mut self) {
        let len = self.chars.len();
        // Skip word chars
        while self.cursor < len && !self.chars[self.cursor].is_whitespace() {
            self.cursor += 1;
        }
        // Skip whitespace
        while self.cursor < len && self.chars[self.cursor].is_whitespace() {
            self.cursor += 1;
        }
    }

    /// Delete from cursor backward to the start of the previous word.
    fn delete_word_backward(&mut self) {
        let end = self.cursor;
        // Skip whitespace
        while self.cursor > 0 && self.chars[self.cursor - 1].is_whitespace() {
            self.cursor -= 1;
        }
        // Skip word chars
        while self.cursor > 0 && !self.chars[self.cursor - 1].is_whitespace() {
            self.cursor -= 1;
        }
        self.chars.drain(self.cursor..end);
    }

    /// Delete from cursor forward to the end of the next word.
    fn delete_word_forward(&mut self) {
        let start = self.cursor;
        let len = self.chars.len();
        let mut end = self.cursor;
        // Skip word chars
        while end < len && !self.chars[end].is_whitespace() {
            end += 1;
        }
        // Skip whitespace
        while end < len && self.chars[end].is_whitespace() {
            end += 1;
        }
        self.chars.drain(start..end);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_and_get_line(buf: &mut InputLineBuffer, data: &str) -> Option<String> {
        buf.feed(data).into_iter().find_map(|a| match a {
            InputAction::Line(s) => Some(s),
            _ => None,
        })
    }

    #[test]
    fn test_simple_typing_and_submit() {
        let mut buf = InputLineBuffer::new();
        assert_eq!(feed_and_get_line(&mut buf, "hello\r"), Some("hello".into()));
    }

    #[test]
    fn test_char_by_char_typing() {
        let mut buf = InputLineBuffer::new();
        assert!(feed_and_get_line(&mut buf, "h").is_none());
        assert!(feed_and_get_line(&mut buf, "e").is_none());
        assert!(feed_and_get_line(&mut buf, "l").is_none());
        assert!(feed_and_get_line(&mut buf, "l").is_none());
        assert!(feed_and_get_line(&mut buf, "o").is_none());
        assert_eq!(feed_and_get_line(&mut buf, "\r"), Some("hello".into()));
    }

    #[test]
    fn test_backspace_del() {
        let mut buf = InputLineBuffer::new();
        buf.feed("helllo");
        buf.feed("\x7f"); // Backspace (DEL)
        assert_eq!(feed_and_get_line(&mut buf, "\r"), Some("helll".into()));
    }

    #[test]
    fn test_backspace_ctrl_h() {
        let mut buf = InputLineBuffer::new();
        buf.feed("helllo");
        buf.feed("\x08"); // Ctrl+H
        assert_eq!(feed_and_get_line(&mut buf, "\r"), Some("helll".into()));
    }

    #[test]
    fn test_ctrl_a_and_ctrl_e() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x01"); // Ctrl+A (home)
        assert_eq!(buf.cursor_pos(), 0);
        buf.feed("\x05"); // Ctrl+E (end)
        assert_eq!(buf.cursor_pos(), 5);
    }

    #[test]
    fn test_ctrl_b_and_ctrl_f() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        assert_eq!(buf.cursor_pos(), 5);
        buf.feed("\x02"); // Ctrl+B (left)
        assert_eq!(buf.cursor_pos(), 4);
        buf.feed("\x06"); // Ctrl+F (right)
        assert_eq!(buf.cursor_pos(), 5);
    }

    #[test]
    fn test_arrow_keys() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x1b[D"); // Left arrow
        buf.feed("\x1b[D"); // Left arrow
        assert_eq!(buf.cursor_pos(), 3);
        buf.feed("\x1b[C"); // Right arrow
        assert_eq!(buf.cursor_pos(), 4);
    }

    #[test]
    fn test_home_end_keys() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x1b[H"); // Home
        assert_eq!(buf.cursor_pos(), 0);
        buf.feed("\x1b[F"); // End
        assert_eq!(buf.cursor_pos(), 5);
    }

    #[test]
    fn test_vt220_home_end() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x1b[1~"); // Home (VT220)
        assert_eq!(buf.cursor_pos(), 0);
        buf.feed("\x1b[4~"); // End (VT220)
        assert_eq!(buf.cursor_pos(), 5);
    }

    #[test]
    fn test_delete_key() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x01");    // Home
        buf.feed("\x1b[3~"); // Delete key
        assert_eq!(buf.content(), "ello");
        assert_eq!(buf.cursor_pos(), 0);
    }

    #[test]
    fn test_ctrl_d_delete_at_cursor() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x01"); // Home
        buf.feed("\x04"); // Ctrl+D
        assert_eq!(buf.content(), "ello");
    }

    #[test]
    fn test_ctrl_k_kill_to_end() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello world");
        buf.feed("\x01"); // Home
        buf.feed("\x06"); // Ctrl+F (right)
        buf.feed("\x06"); // Ctrl+F (right)
        buf.feed("\x06"); // Ctrl+F (right)
        buf.feed("\x06"); // Ctrl+F (right)
        buf.feed("\x06"); // Ctrl+F (right)
        buf.feed("\x0b"); // Ctrl+K
        assert_eq!(buf.content(), "hello");
    }

    #[test]
    fn test_ctrl_u_kill_to_start() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello world");
        // "hello world" = 11 chars, 5x Ctrl+B → cursor at 6 (before 'w')
        buf.feed("\x02"); // Ctrl+B (left)
        buf.feed("\x02");
        buf.feed("\x02");
        buf.feed("\x02");
        buf.feed("\x02");
        buf.feed("\x15"); // Ctrl+U — kills chars 0..6 ("hello ")
        assert_eq!(buf.content(), "world");
        assert_eq!(buf.cursor_pos(), 0);
    }

    #[test]
    fn test_ctrl_w_delete_word() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello world");
        buf.feed("\x17"); // Ctrl+W
        assert_eq!(buf.content(), "hello ");
    }

    #[test]
    fn test_alt_b_word_backward() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello world");
        buf.feed("\x1bb"); // Alt+B
        assert_eq!(buf.cursor_pos(), 6); // Before "world"
    }

    #[test]
    fn test_alt_f_word_forward() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello world");
        buf.feed("\x01");  // Home
        buf.feed("\x1bf"); // Alt+F — skips word "hello" then space → position 6
        assert_eq!(buf.cursor_pos(), 6); // After "hello " (start of "world")
    }

    #[test]
    fn test_alt_d_delete_word_forward() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello world");
        buf.feed("\x01");  // Home
        buf.feed("\x1bd"); // Alt+D — deletes word "hello" then space → leaves "world"
        assert_eq!(buf.content(), "world");
    }

    #[test]
    fn test_alt_backspace_delete_word_backward() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello world");
        buf.feed("\x1b\x7f"); // Alt+Backspace
        assert_eq!(buf.content(), "hello ");
    }

    #[test]
    fn test_ctrl_right_word_forward() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello world");
        buf.feed("\x01");      // Home
        buf.feed("\x1b[1;5C"); // Ctrl+Right
        assert_eq!(buf.cursor_pos(), 6); // After "hello "
    }

    #[test]
    fn test_ctrl_left_word_backward() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello world");
        buf.feed("\x1b[1;5D"); // Ctrl+Left
        assert_eq!(buf.cursor_pos(), 6); // Before "world"
    }

    #[test]
    fn test_ctrl_t_transpose() {
        let mut buf = InputLineBuffer::new();
        buf.feed("helo");
        buf.feed("\x02"); // Move left once (cursor at 3, before 'o')
        buf.feed("\x14"); // Ctrl+T: swap 'l' and 'o' → "heol"
        // After transpose at cursor=3: chars[2] and chars[3] swapped, cursor moves to 4
        assert_eq!(buf.content(), "heol");
    }

    #[test]
    fn test_ctrl_c_interrupt() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        let actions = buf.feed("\x03");
        assert!(matches!(actions.first(), Some(InputAction::Interrupt)));
        assert_eq!(buf.content(), "");
    }

    #[test]
    fn test_insert_at_cursor() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hllo");
        buf.feed("\x01");  // Home
        buf.feed("\x06");  // Right
        // Now cursor at 1, between 'h' and 'l'
        buf.feed("e");
        assert_eq!(buf.content(), "hello");
    }

    #[test]
    fn test_paste_multiline() {
        let mut buf = InputLineBuffer::new();
        // Pasting text with a newline should submit
        let actions = buf.feed("line one\rline two\r");
        let lines: Vec<_> = actions
            .into_iter()
            .filter_map(|a| match a {
                InputAction::Line(s) => Some(s),
                _ => None,
            })
            .collect();
        assert_eq!(lines, vec!["line one", "line two"]);
    }

    #[test]
    fn test_shift_enter_multiline() {
        let mut buf = InputLineBuffer::new();
        buf.feed("line one");
        buf.feed("\x1b\r"); // Shift+Enter (ESC CR) — literal newline
        buf.feed("line two");
        assert_eq!(feed_and_get_line(&mut buf, "\r"), Some("line one\nline two".into()));
    }

    #[test]
    fn test_utf8_characters() {
        let mut buf = InputLineBuffer::new();
        buf.feed("café");
        assert_eq!(buf.content(), "café");
        assert_eq!(buf.cursor_pos(), 4);
        buf.feed("\x7f"); // Backspace
        assert_eq!(buf.content(), "caf");
    }

    #[test]
    fn test_empty_submit() {
        let mut buf = InputLineBuffer::new();
        assert_eq!(feed_and_get_line(&mut buf, "\r"), Some("".into()));
    }

    #[test]
    fn test_application_cursor_mode() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x1bOD"); // SS3 Left arrow
        buf.feed("\x1bOD"); // SS3 Left arrow
        assert_eq!(buf.cursor_pos(), 3);
        buf.feed("\x1bOC"); // SS3 Right arrow
        assert_eq!(buf.cursor_pos(), 4);
        buf.feed("\x1bOH"); // SS3 Home
        assert_eq!(buf.cursor_pos(), 0);
        buf.feed("\x1bOF"); // SS3 End
        assert_eq!(buf.cursor_pos(), 5);
    }

    #[test]
    fn test_kitty_csi_u_enter() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x1b[13;2u"); // Shift+Enter in kitty protocol → literal newline
        buf.feed("world");
        assert_eq!(feed_and_get_line(&mut buf, "\r"), Some("hello\nworld".into()));
    }

    #[test]
    fn test_kitty_csi_u_backspace() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x1b[127;5u"); // Ctrl+Backspace in kitty protocol
        assert_eq!(buf.content(), "hell");
    }

    #[test]
    fn test_sequential_lines() {
        let mut buf = InputLineBuffer::new();
        assert_eq!(feed_and_get_line(&mut buf, "first\r"), Some("first".into()));
        assert_eq!(feed_and_get_line(&mut buf, "second\r"), Some("second".into()));
        assert_eq!(buf.content(), "");
    }

    #[test]
    fn test_complex_editing_session() {
        let mut buf = InputLineBuffer::new();
        // Type "git commti --amend"
        buf.feed("git commti --amend");
        // Oops, typo in "commti". Move back to fix it.
        buf.feed("\x1b[1;5D"); // Ctrl+Left → before "--amend"
        buf.feed("\x1b[1;5D"); // Ctrl+Left → before "commti"
        // Now cursor is at 4 (before 'c' in 'commti')
        buf.feed("\x1b[1;5C"); // Ctrl+Right → after "commti"
        // Cursor at 10 (after "commti")
        buf.feed("\x02\x02"); // Ctrl+B twice → cursor at 8 (between 't' and 'i')
        buf.feed("\x14"); // Ctrl+T → transpose 't' and 'i' → "commit"
        assert_eq!(
            feed_and_get_line(&mut buf, "\r"),
            Some("git commit --amend".into())
        );
    }

    #[test]
    fn test_backspace_at_beginning_is_noop() {
        let mut buf = InputLineBuffer::new();
        buf.feed("\x7f"); // Backspace at empty buffer
        assert_eq!(buf.content(), "");
        buf.feed("a");
        buf.feed("\x01"); // Home
        buf.feed("\x7f"); // Backspace at beginning
        assert_eq!(buf.content(), "a");
    }

    #[test]
    fn test_alt_right_macos() {
        // On macOS, xterm.js sends Alt+Right as ESC[1;3C (modifier 3)
        let mut buf = InputLineBuffer::new();
        buf.feed("hello world");
        buf.feed("\x01");      // Home
        buf.feed("\x1b[1;3C"); // Alt+Right (macOS)
        // Should move word forward, like Ctrl+Right
        assert!(buf.cursor_pos() > 0);
    }

    #[test]
    fn test_buffer_cap_safety() {
        let mut buf = InputLineBuffer::new();
        // Type a very long line — should not panic or OOM
        let long_input: String = "a".repeat(10_000);
        buf.feed(&long_input);
        assert_eq!(buf.chars.len(), 10_000);
        assert_eq!(feed_and_get_line(&mut buf, "\r"), Some(long_input));
    }

    // -----------------------------------------------------------------------
    // Edge cases: cursor boundary conditions
    // -----------------------------------------------------------------------

    #[test]
    fn test_ctrl_f_at_end_is_noop() {
        let mut buf = InputLineBuffer::new();
        buf.feed("abc");
        assert_eq!(buf.cursor_pos(), 3);
        buf.feed("\x06"); // Ctrl+F at end
        assert_eq!(buf.cursor_pos(), 3); // unchanged
    }

    #[test]
    fn test_ctrl_b_at_beginning_is_noop() {
        let mut buf = InputLineBuffer::new();
        buf.feed("abc");
        buf.feed("\x01"); // Home
        buf.feed("\x02"); // Ctrl+B at beginning
        assert_eq!(buf.cursor_pos(), 0); // unchanged
    }

    #[test]
    fn test_right_arrow_at_end_is_noop() {
        let mut buf = InputLineBuffer::new();
        buf.feed("abc");
        buf.feed("\x1b[C"); // Right arrow at end
        assert_eq!(buf.cursor_pos(), 3); // unchanged
    }

    #[test]
    fn test_left_arrow_at_beginning_is_noop() {
        let mut buf = InputLineBuffer::new();
        buf.feed("abc");
        buf.feed("\x1b[H"); // Home
        buf.feed("\x1b[D"); // Left arrow at beginning
        assert_eq!(buf.cursor_pos(), 0); // unchanged
    }

    #[test]
    fn test_ctrl_d_on_empty_buffer_is_noop() {
        let mut buf = InputLineBuffer::new();
        buf.feed("\x04"); // Ctrl+D on empty buffer
        assert_eq!(buf.content(), "");
    }

    #[test]
    fn test_ctrl_d_at_end_of_line_is_noop() {
        let mut buf = InputLineBuffer::new();
        buf.feed("abc");
        buf.feed("\x04"); // Ctrl+D at end (cursor = 3, len = 3)
        assert_eq!(buf.content(), "abc");
    }

    #[test]
    fn test_delete_key_at_end_is_noop() {
        let mut buf = InputLineBuffer::new();
        buf.feed("abc");
        buf.feed("\x1b[3~"); // Delete at end
        assert_eq!(buf.content(), "abc");
    }

    #[test]
    fn test_ctrl_k_at_end_is_noop() {
        let mut buf = InputLineBuffer::new();
        buf.feed("abc");
        buf.feed("\x0b"); // Ctrl+K at end
        assert_eq!(buf.content(), "abc");
    }

    #[test]
    fn test_ctrl_u_at_beginning_is_noop() {
        let mut buf = InputLineBuffer::new();
        buf.feed("abc");
        buf.feed("\x01"); // Home
        buf.feed("\x15"); // Ctrl+U at beginning
        assert_eq!(buf.content(), "abc");
        assert_eq!(buf.cursor_pos(), 0);
    }

    #[test]
    fn test_ctrl_w_on_empty_is_noop() {
        let mut buf = InputLineBuffer::new();
        buf.feed("\x17"); // Ctrl+W on empty buffer
        assert_eq!(buf.content(), "");
    }

    #[test]
    fn test_ctrl_w_at_beginning_is_noop() {
        let mut buf = InputLineBuffer::new();
        buf.feed("abc");
        buf.feed("\x01"); // Home
        buf.feed("\x17"); // Ctrl+W at beginning
        assert_eq!(buf.content(), "abc");
    }

    #[test]
    fn test_alt_b_at_beginning_is_noop() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x01");  // Home
        buf.feed("\x1bb"); // Alt+B at beginning
        assert_eq!(buf.cursor_pos(), 0);
    }

    #[test]
    fn test_alt_f_at_end_is_noop() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x1bf"); // Alt+F at end
        assert_eq!(buf.cursor_pos(), 5); // unchanged
    }

    // -----------------------------------------------------------------------
    // Multi-word operations
    // -----------------------------------------------------------------------

    #[test]
    fn test_ctrl_w_multiple_words() {
        let mut buf = InputLineBuffer::new();
        buf.feed("one two three");
        buf.feed("\x17"); // Ctrl+W → remove "three"
        assert_eq!(buf.content(), "one two ");
        buf.feed("\x17"); // Ctrl+W → remove "two"
        assert_eq!(buf.content(), "one ");
        buf.feed("\x17"); // Ctrl+W → remove "one"
        assert_eq!(buf.content(), "");
    }

    #[test]
    fn test_alt_b_multiple_words() {
        let mut buf = InputLineBuffer::new();
        buf.feed("one two three");
        buf.feed("\x1bb"); // Alt+B → before "three"
        assert_eq!(buf.cursor_pos(), 8);
        buf.feed("\x1bb"); // Alt+B → before "two"
        assert_eq!(buf.cursor_pos(), 4);
        buf.feed("\x1bb"); // Alt+B → before "one"
        assert_eq!(buf.cursor_pos(), 0);
    }

    #[test]
    fn test_alt_f_multiple_words() {
        let mut buf = InputLineBuffer::new();
        buf.feed("one two three");
        buf.feed("\x01");  // Home
        buf.feed("\x1bf"); // Alt+F → after "one "
        assert_eq!(buf.cursor_pos(), 4);
        buf.feed("\x1bf"); // Alt+F → after "two "
        assert_eq!(buf.cursor_pos(), 8);
        buf.feed("\x1bf"); // Alt+F → after "three"
        assert_eq!(buf.cursor_pos(), 13);
    }

    #[test]
    fn test_ctrl_left_multiple_words() {
        let mut buf = InputLineBuffer::new();
        buf.feed("foo bar baz");
        buf.feed("\x1b[1;5D"); // Ctrl+Left → before "baz"
        assert_eq!(buf.cursor_pos(), 8);
        buf.feed("\x1b[1;5D"); // Ctrl+Left → before "bar"
        assert_eq!(buf.cursor_pos(), 4);
        buf.feed("\x1b[1;5D"); // Ctrl+Left → before "foo"
        assert_eq!(buf.cursor_pos(), 0);
    }

    #[test]
    fn test_ctrl_right_multiple_words() {
        let mut buf = InputLineBuffer::new();
        buf.feed("foo bar baz");
        buf.feed("\x01");      // Home
        buf.feed("\x1b[1;5C"); // Ctrl+Right → after "foo "
        assert_eq!(buf.cursor_pos(), 4);
        buf.feed("\x1b[1;5C"); // Ctrl+Right → after "bar "
        assert_eq!(buf.cursor_pos(), 8);
        buf.feed("\x1b[1;5C"); // Ctrl+Right → after "baz"
        assert_eq!(buf.cursor_pos(), 11);
    }

    // -----------------------------------------------------------------------
    // Consecutive whitespace
    // -----------------------------------------------------------------------

    #[test]
    fn test_ctrl_w_with_trailing_spaces() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello   "); // trailing spaces
        buf.feed("\x17");     // Ctrl+W — should delete "   " + "hello"
        assert_eq!(buf.content(), "");
    }

    #[test]
    fn test_word_backward_over_multiple_spaces() {
        let mut buf = InputLineBuffer::new();
        buf.feed("abc   def");
        buf.feed("\x1bb"); // Alt+B — should skip spaces then word
        assert_eq!(buf.cursor_pos(), 6); // Before "def"
    }

    #[test]
    fn test_word_forward_over_multiple_spaces() {
        let mut buf = InputLineBuffer::new();
        buf.feed("abc   def");
        buf.feed("\x01");  // Home
        buf.feed("\x1bf"); // Alt+F — should skip "abc" then "   "
        assert_eq!(buf.cursor_pos(), 6); // Start of "def"
    }

    // -----------------------------------------------------------------------
    // Ctrl+T (transpose) edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_ctrl_t_at_beginning_is_noop() {
        let mut buf = InputLineBuffer::new();
        buf.feed("ab");
        buf.feed("\x01"); // Home
        buf.feed("\x14"); // Ctrl+T at beginning (cursor=0)
        assert_eq!(buf.content(), "ab");
    }

    #[test]
    fn test_ctrl_t_single_char_is_noop() {
        let mut buf = InputLineBuffer::new();
        buf.feed("a");
        buf.feed("\x14"); // Ctrl+T with single char
        assert_eq!(buf.content(), "a");
    }

    #[test]
    fn test_ctrl_t_at_end_of_line() {
        let mut buf = InputLineBuffer::new();
        buf.feed("ab");
        // Cursor at 2 (end), transpose last two chars
        buf.feed("\x14");
        assert_eq!(buf.content(), "ba");
        assert_eq!(buf.cursor_pos(), 2);
    }

    #[test]
    fn test_ctrl_t_middle_of_line() {
        let mut buf = InputLineBuffer::new();
        buf.feed("abcd");
        buf.feed("\x02"); // Ctrl+B → cursor at 3
        buf.feed("\x14"); // Ctrl+T → swap chars[2] and chars[3] → "abdc"
        assert_eq!(buf.content(), "abdc");
        assert_eq!(buf.cursor_pos(), 4);
    }

    // -----------------------------------------------------------------------
    // LF (newline) as line submit
    // -----------------------------------------------------------------------

    #[test]
    fn test_lf_submits_line() {
        let mut buf = InputLineBuffer::new();
        assert_eq!(feed_and_get_line(&mut buf, "hello\n"), Some("hello".into()));
    }

    #[test]
    fn test_cr_and_lf_both_submit() {
        let mut buf = InputLineBuffer::new();
        let actions = buf.feed("a\rb\n");
        let lines: Vec<_> = actions.into_iter().filter_map(|a| match a {
            InputAction::Line(s) => Some(s),
            _ => None,
        }).collect();
        assert_eq!(lines, vec!["a", "b"]);
    }

    // -----------------------------------------------------------------------
    // Tab, Ctrl+L, Ctrl+Y, and other ignored control chars
    // -----------------------------------------------------------------------

    #[test]
    fn test_tab_is_ignored() {
        let mut buf = InputLineBuffer::new();
        buf.feed("he");
        buf.feed("\x09"); // Tab — ignored (completion is shell-side)
        buf.feed("llo");
        assert_eq!(buf.content(), "hello");
    }

    #[test]
    fn test_ctrl_l_preserves_buffer() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x0c"); // Ctrl+L — clear screen, but buffer persists
        assert_eq!(buf.content(), "hello");
        assert_eq!(buf.cursor_pos(), 5);
    }

    #[test]
    fn test_ctrl_y_is_ignored() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x19"); // Ctrl+Y (yank) — ignored
        assert_eq!(buf.content(), "hello");
    }

    #[test]
    fn test_ctrl_r_is_ignored() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x12"); // Ctrl+R (reverse search) — ignored
        assert_eq!(buf.content(), "hello");
    }

    #[test]
    fn test_ctrl_p_and_ctrl_n_are_ignored() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x10"); // Ctrl+P (history prev) — ignored
        buf.feed("\x0e"); // Ctrl+N (history next) — ignored
        assert_eq!(buf.content(), "hello");
    }

    // -----------------------------------------------------------------------
    // Up/Down arrow (history) ignored
    // -----------------------------------------------------------------------

    #[test]
    fn test_up_down_arrows_are_ignored() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x1b[A"); // Up arrow
        buf.feed("\x1b[B"); // Down arrow
        assert_eq!(buf.content(), "hello");
        assert_eq!(buf.cursor_pos(), 5);
    }

    #[test]
    fn test_ss3_up_down_ignored() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x1bOA"); // SS3 Up
        buf.feed("\x1bOB"); // SS3 Down
        assert_eq!(buf.content(), "hello");
    }

    // -----------------------------------------------------------------------
    // Escape sequence state machine
    // -----------------------------------------------------------------------

    #[test]
    fn test_unknown_esc_sequence_ignored() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x1bX"); // Unknown ESC+X — ignored, back to normal
        buf.feed("!");
        assert_eq!(buf.content(), "hello!");
    }

    #[test]
    fn test_unknown_csi_final_byte_ignored() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x1b[42z"); // CSI 42 z — unknown, ignored
        assert_eq!(buf.content(), "hello");
    }

    #[test]
    fn test_unknown_ss3_final_byte_ignored() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x1bOZ"); // SS3 Z — unknown, ignored
        assert_eq!(buf.content(), "hello");
    }

    #[test]
    fn test_csi_with_semicolons() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello world");
        buf.feed("\x1b[1;5D"); // Ctrl+Left — multi-param CSI
        // Should work normally
        assert!(buf.cursor_pos() < 11);
    }

    #[test]
    fn test_csi_with_empty_params() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x1b[C"); // Right with no params — plain right arrow
        assert_eq!(buf.cursor_pos(), 5); // Already at end, no change
    }

    // -----------------------------------------------------------------------
    // Insert mode (typing in the middle)
    // -----------------------------------------------------------------------

    #[test]
    fn test_insert_at_beginning() {
        let mut buf = InputLineBuffer::new();
        buf.feed("world");
        buf.feed("\x01"); // Home
        buf.feed("hello ");
        assert_eq!(buf.content(), "hello world");
    }

    #[test]
    fn test_insert_multiple_chars_in_middle() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hd");
        buf.feed("\x02"); // Ctrl+B → cursor at 1
        buf.feed("ello worl");
        assert_eq!(buf.content(), "hello world");
    }

    // -----------------------------------------------------------------------
    // Delete operations from various positions
    // -----------------------------------------------------------------------

    #[test]
    fn test_delete_all_chars_with_backspace() {
        let mut buf = InputLineBuffer::new();
        buf.feed("abc");
        buf.feed("\x7f\x7f\x7f"); // 3 backspaces
        assert_eq!(buf.content(), "");
        assert_eq!(buf.cursor_pos(), 0);
    }

    #[test]
    fn test_delete_all_with_ctrl_d() {
        let mut buf = InputLineBuffer::new();
        buf.feed("abc");
        buf.feed("\x01"); // Home
        buf.feed("\x04\x04\x04"); // 3x Ctrl+D
        assert_eq!(buf.content(), "");
    }

    #[test]
    fn test_ctrl_k_from_beginning_clears_all() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello world");
        buf.feed("\x01"); // Home
        buf.feed("\x0b"); // Ctrl+K
        assert_eq!(buf.content(), "");
    }

    #[test]
    fn test_ctrl_u_from_end_clears_all() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello world");
        buf.feed("\x15"); // Ctrl+U from end
        assert_eq!(buf.content(), "");
        assert_eq!(buf.cursor_pos(), 0);
    }

    // -----------------------------------------------------------------------
    // Unicode: CJK, emoji, accented, mixed
    // -----------------------------------------------------------------------

    #[test]
    fn test_cjk_characters() {
        let mut buf = InputLineBuffer::new();
        buf.feed("你好世界");
        assert_eq!(buf.content(), "你好世界");
        assert_eq!(buf.cursor_pos(), 4); // 4 chars
    }

    #[test]
    fn test_emoji_characters() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello 🌍");
        assert_eq!(buf.content(), "hello 🌍");
        assert_eq!(buf.cursor_pos(), 7);
        buf.feed("\x7f"); // Backspace removes the emoji
        assert_eq!(buf.content(), "hello ");
    }

    #[test]
    fn test_mixed_unicode_editing() {
        let mut buf = InputLineBuffer::new();
        buf.feed("café ☕");
        buf.feed("\x02\x02"); // Move back 2
        assert_eq!(buf.cursor_pos(), 4); // Before space
        buf.feed("\x04"); // Ctrl+D — delete space
        assert_eq!(buf.content(), "café☕");
    }

    #[test]
    fn test_accented_characters_word_ops() {
        let mut buf = InputLineBuffer::new();
        buf.feed("naïve résumé");
        buf.feed("\x1bb"); // Alt+B — word backward
        assert_eq!(buf.cursor_pos(), 6); // Before "résumé"
    }

    // -----------------------------------------------------------------------
    // Shift+Enter variations
    // -----------------------------------------------------------------------

    #[test]
    fn test_shift_enter_esc_cr_inserts_newline() {
        let mut buf = InputLineBuffer::new();
        buf.feed("line1");
        buf.feed("\x1b\r"); // ESC CR
        buf.feed("line2");
        buf.feed("\x1b\r"); // ESC CR
        buf.feed("line3");
        assert_eq!(feed_and_get_line(&mut buf, "\r"), Some("line1\nline2\nline3".into()));
    }

    #[test]
    fn test_kitty_shift_enter_inserts_newline() {
        let mut buf = InputLineBuffer::new();
        buf.feed("first");
        buf.feed("\x1b[13;2u"); // Kitty Shift+Enter
        buf.feed("second");
        assert_eq!(feed_and_get_line(&mut buf, "\r"), Some("first\nsecond".into()));
    }

    #[test]
    fn test_kitty_escape_key_ignored() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x1b[27;1u"); // Kitty Escape key
        assert_eq!(buf.content(), "hello");
    }

    #[test]
    fn test_kitty_tab_key_ignored() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x1b[9;1u"); // Kitty Tab
        assert_eq!(buf.content(), "hello");
    }

    // -----------------------------------------------------------------------
    // Page Up/Down, Insert key (should be ignored)
    // -----------------------------------------------------------------------

    #[test]
    fn test_page_up_ignored() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x1b[5~"); // Page Up
        assert_eq!(buf.content(), "hello");
        assert_eq!(buf.cursor_pos(), 5);
    }

    #[test]
    fn test_page_down_ignored() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x1b[6~"); // Page Down
        assert_eq!(buf.content(), "hello");
    }

    #[test]
    fn test_insert_key_ignored() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x1b[2~"); // Insert
        assert_eq!(buf.content(), "hello");
    }

    // -----------------------------------------------------------------------
    // Rapid-fire realistic scenarios
    // -----------------------------------------------------------------------

    #[test]
    fn test_typical_git_command() {
        let mut buf = InputLineBuffer::new();
        buf.feed("git push origin main\r");
        let actions = buf.feed("");
        // The line was already captured by the \r above
        // Let's verify through a clean test:
        let mut buf2 = InputLineBuffer::new();
        assert_eq!(
            feed_and_get_line(&mut buf2, "git push origin main\r"),
            Some("git push origin main".into())
        );
        drop(actions);
    }

    #[test]
    fn test_backspace_retype_pattern() {
        // Common pattern: type, realize mistake, backspace, retype
        let mut buf = InputLineBuffer::new();
        buf.feed("git comit");      // typo
        buf.feed("\x7f\x7f\x7f");   // 3 backspaces → "git co"
        buf.feed("mmit");            // retype correctly
        assert_eq!(feed_and_get_line(&mut buf, "\r"), Some("git commit".into()));
    }

    #[test]
    fn test_ctrl_a_retype_entire_line() {
        let mut buf = InputLineBuffer::new();
        buf.feed("wrong command");
        buf.feed("\x01"); // Home
        buf.feed("\x0b"); // Ctrl+K — kill to end
        buf.feed("right command");
        assert_eq!(feed_and_get_line(&mut buf, "\r"), Some("right command".into()));
    }

    #[test]
    fn test_interrupt_then_new_command() {
        let mut buf = InputLineBuffer::new();
        buf.feed("long running com");
        let actions = buf.feed("\x03"); // Ctrl+C
        assert!(actions.iter().any(|a| matches!(a, InputAction::Interrupt)));
        assert_eq!(buf.content(), "");
        // Now type a new command
        assert_eq!(feed_and_get_line(&mut buf, "ls\r"), Some("ls".into()));
    }

    #[test]
    fn test_multiple_interrupts() {
        let mut buf = InputLineBuffer::new();
        buf.feed("abc");
        buf.feed("\x03"); // Ctrl+C
        buf.feed("def");
        buf.feed("\x03"); // Ctrl+C
        assert_eq!(buf.content(), "");
        assert_eq!(feed_and_get_line(&mut buf, "ghi\r"), Some("ghi".into()));
    }

    #[test]
    fn test_paste_large_block() {
        let mut buf = InputLineBuffer::new();
        let pasted = "echo 'hello'\recho 'world'\recho 'done'\r";
        let actions = buf.feed(pasted);
        let lines: Vec<_> = actions.into_iter().filter_map(|a| match a {
            InputAction::Line(s) => Some(s),
            _ => None,
        }).collect();
        assert_eq!(lines, vec!["echo 'hello'", "echo 'world'", "echo 'done'"]);
    }

    #[test]
    fn test_realistic_claude_code_prompt() {
        let mut buf = InputLineBuffer::new();
        // Simulate typing a multi-line prompt with Shift+Enter
        buf.feed("refactor the auth module");
        buf.feed("\x1b\r"); // Shift+Enter
        buf.feed("make sure to keep backward compatibility");
        buf.feed("\x1b\r"); // Shift+Enter
        buf.feed("add tests for the new behavior");
        assert_eq!(
            feed_and_get_line(&mut buf, "\r"),
            Some("refactor the auth module\nmake sure to keep backward compatibility\nadd tests for the new behavior".into())
        );
    }

    #[test]
    fn test_quick_edit_with_arrow_and_insert() {
        let mut buf = InputLineBuffer::new();
        buf.feed("git diff --staged");
        // User wants to add "--stat" before "--staged"
        buf.feed("\x1b[D".repeat(8).as_str()); // 8 left arrows → cursor at 10
        buf.feed("--stat ");
        assert_eq!(
            feed_and_get_line(&mut buf, "\r"),
            Some("git diff --stat --staged".into())
        );
    }

    // -----------------------------------------------------------------------
    // Buffer reuse across multiple lines
    // -----------------------------------------------------------------------

    #[test]
    fn test_buffer_state_resets_after_submit() {
        let mut buf = InputLineBuffer::new();
        buf.feed("first\r");
        assert_eq!(buf.content(), "");
        assert_eq!(buf.cursor_pos(), 0);
        buf.feed("second\r");
        assert_eq!(buf.content(), "");
        assert_eq!(buf.cursor_pos(), 0);
    }

    #[test]
    fn test_many_sequential_commands() {
        let mut buf = InputLineBuffer::new();
        for i in 0..100 {
            let cmd = format!("command_{}", i);
            let expected = cmd.clone();
            assert_eq!(feed_and_get_line(&mut buf, &format!("{}\r", cmd)), Some(expected));
        }
    }

    // -----------------------------------------------------------------------
    // Alt+Left (macOS modifier 3 for Alt)
    // -----------------------------------------------------------------------

    #[test]
    fn test_alt_left_macos() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello world");
        buf.feed("\x1b[1;3D"); // Alt+Left (macOS) → word backward
        assert_eq!(buf.cursor_pos(), 6); // Before "world"
    }

    // -----------------------------------------------------------------------
    // Rapid escape sequences interspersed with typing
    // -----------------------------------------------------------------------

    #[test]
    fn test_interleaved_typing_and_navigation() {
        let mut buf = InputLineBuffer::new();
        buf.feed("abc");
        buf.feed("\x1b[D"); // Left
        buf.feed("X");      // Insert X before 'c'
        buf.feed("\x1b[C"); // Right
        buf.feed("Y");      // Append Y after 'c'
        assert_eq!(buf.content(), "abXcY");
    }

    #[test]
    fn test_feed_single_bytes_of_escape_sequence() {
        // Escape sequences may arrive split across multiple feed() calls
        let mut buf = InputLineBuffer::new();
        buf.feed("hello");
        buf.feed("\x1b");  // ESC alone
        buf.feed("[");     // CSI
        buf.feed("D");     // Left arrow final byte
        assert_eq!(buf.cursor_pos(), 4);
    }

    #[test]
    fn test_feed_split_csi_params() {
        let mut buf = InputLineBuffer::new();
        buf.feed("hello world");
        buf.feed("\x1b");  // ESC
        buf.feed("[");     // CSI start
        buf.feed("1");     // param byte
        buf.feed(";");     // separator
        buf.feed("5");     // modifier
        buf.feed("D");     // Left arrow with Ctrl modifier
        // Should do Ctrl+Left = word backward
        assert_eq!(buf.cursor_pos(), 6); // Before "world"
    }

    // -----------------------------------------------------------------------
    // Empty and whitespace-only inputs
    // -----------------------------------------------------------------------

    #[test]
    fn test_spaces_only_submit() {
        let mut buf = InputLineBuffer::new();
        assert_eq!(feed_and_get_line(&mut buf, "   \r"), Some("   ".into()));
    }

    #[test]
    fn test_single_char_submit() {
        let mut buf = InputLineBuffer::new();
        assert_eq!(feed_and_get_line(&mut buf, "x\r"), Some("x".into()));
    }

    #[test]
    fn test_empty_feed() {
        let mut buf = InputLineBuffer::new();
        let actions = buf.feed("");
        assert!(actions.is_empty());
        assert_eq!(buf.content(), "");
    }
}
