//! Payload-free terminal queries that delimit streamed secret candidates.
use alacritty_terminal::vte;
use std::ops::Range;

#[derive(Default)]
struct Queries {
    start: usize,
    end: usize,
    osc_st: bool,
    ranges: Vec<Range<usize>>,
}

impl Queries {
    fn record(&mut self) {
        self.ranges.push(self.start..self.end);
    }
}

impl vte::Perform for Queries {
    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        if ignore {
            return;
        }
        let first = params.iter().next().map_or(0, |param| param[0]);
        let query = match (action, intermediates) {
            ('n', []) => matches!(first, 5 | 6),
            ('c', [] | [b'>']) => first == 0,
            ('p', [b'$'] | [b'?', b'$']) => true,
            ('u', [b'?']) => true,
            ('m', [b'?']) => first == 4,
            ('t', []) => matches!(first, 14 | 18),
            _ => false,
        };
        if query {
            self.record();
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        let query = match params {
            [b"4", pairs @ ..] => {
                let (pairs, remainder) = pairs.as_chunks::<2>();
                !pairs.is_empty()
                    && remainder.is_empty()
                    && pairs.iter().all(|pair| {
                        !pair[0].is_empty()
                            && pair[0].iter().all(u8::is_ascii_digit)
                            && std::str::from_utf8(pair[0])
                                .is_ok_and(|index| index.parse::<u8>().is_ok())
                            && pair[1] == b"?"
                    })
            }
            [b"10" | b"11" | b"12", values @ ..] => {
                !values.is_empty() && values.iter().all(|value| *value == b"?")
            }
            _ => false,
        };
        if (bell_terminated || self.osc_st) && query {
            // VTE dispatches OSC on the ESC of ST, before its final backslash.
            self.ranges
                .push(self.start..self.end + usize::from(self.osc_st));
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if !ignore && intermediates.is_empty() && byte == b'Z' {
            self.record();
        }
    }
}

/// The reader's EscapeAwareBuffer supplies complete escape sequences. VTE
/// keeps query-looking text inside OSC/DCS payloads from becoming boundaries.
pub(super) fn ranges(data: &str) -> Vec<Range<usize>> {
    if !data.as_bytes().contains(&0x1b) {
        return Vec::new();
    }
    let mut parser = vte::Parser::new();
    let mut queries = Queries::default();
    let bytes = data.as_bytes();
    for (i, byte) in bytes.iter().enumerate() {
        if *byte == 0x1b && bytes.get(i + 1) != Some(&b'\\') {
            queries.start = i;
        }
        queries.end = i + 1;
        queries.osc_st = *byte == 0x1b && bytes.get(i + 1) == Some(&b'\\');
        parser.advance(&mut queries, std::slice::from_ref(byte));
    }
    queries.ranges
}
