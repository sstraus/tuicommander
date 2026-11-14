//! Log text stripping on the workspace's patched VTE parser.

/// Remove terminal escapes, retaining printable UTF-8 and linefeeds.
///
/// This keeps strip-ansi-escapes' performer policy: other executed controls
/// (including tabs and carriage returns) are discarded rather than rendered.
pub fn strip(data: impl AsRef<[u8]>) -> Vec<u8> {
    strip_text(data.as_ref(), false)
}

/// Remove terminal escapes, retaining printable UTF-8, linefeeds and tabs.
pub fn strip_preserving_tabs(data: impl AsRef<[u8]>) -> Vec<u8> {
    strip_text(data.as_ref(), true)
}

fn strip_text(data: &[u8], preserve_tabs: bool) -> Vec<u8> {
    let mut output = PlainText(Vec::with_capacity(data.len()), preserve_tabs);
    vte::Parser::new().advance(&mut output, data.as_ref());
    output.0
}

struct PlainText(Vec<u8>, bool);

impl vte::Perform for PlainText {
    fn print(&mut self, character: char) {
        let mut bytes = [0; 4];
        self.0
            .extend_from_slice(character.encode_utf8(&mut bytes).as_bytes());
    }

    fn execute(&mut self, byte: u8) {
        if byte == b'\n' || (self.1 && byte == b'\t') {
            self.0.push(byte);
        }
    }
}

#[cfg(test)]
mod tests {
    // Catches: preserving tabs by splitting input lets control-string payloads escape the parser.
    #[test]
    fn tabs_survive_only_outside_terminal_control_strings() {
        assert_eq!(
            super::strip_preserving_tabs("a\tb\n\x1b]52;c;hidden\tdata\x07c\t\x1b[31md\x1b[0m"),
            b"a\tb\nc\td"
        );
    }

    // Catches: OSC/CSI payload leaking, UTF-8 loss, or preserving controls the old stripper discarded.
    #[test]
    fn ansi_strip_preserves_text_and_linefeeds_without_terminal_controls() {
        for (input, expected) in [
            ("plain\ntext", "plain\ntext"),
            ("\x1b[31mred\x1b[0m\né界", "red\né界"),
            ("before\x1b]0;title\x07after", "beforeafter"),
            (
                "\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\",
                "link",
            ),
            ("before\x1bP1;2|payload\x1b\\after", "beforeafter"),
            ("\x1b(Btext\t\r\x08\x07\nnext", "text\nnext"),
            ("text\x1b[31", "text"),
            ("text\x1b]0;unfinished", "text"),
            ("", ""),
        ] {
            assert_eq!(super::strip(input), expected.as_bytes(), "{input:?}");
        }
    }
}
