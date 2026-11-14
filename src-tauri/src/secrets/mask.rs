//! Match known values before returning any child output, including line wraps.
use base64::Engine;
use std::fmt::Write;
use zeroize::{Zeroize, Zeroizing};

/// JSON string body with `\uXXXX` escapes for the chars `escape` selects:
/// Go's encoding/json escapes `& < >`, Python's `ensure_ascii` and `jq -a`
/// escape non-ASCII.
fn json_variant(inner: &str, escape: impl Fn(char) -> bool) -> Zeroizing<String> {
    // Worst case 6 bytes per input byte, so the buffer never reallocates.
    let mut out = Zeroizing::new(String::with_capacity(inner.len() * 6));
    for c in inner.chars() {
        if escape(c) {
            for unit in c.encode_utf16(&mut [0; 2]) {
                let _ = write!(out, "\\u{unit:04x}");
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub(crate) fn representations(value: &str) -> Vec<Zeroizing<String>> {
    let mut result = vec![Zeroizing::new(value.to_owned())];
    let json =
        Zeroizing::new(serde_json::to_string(value).expect("string serialization cannot fail"));
    let inner = &json[1..json.len() - 1];
    result.push(Zeroizing::new(inner.to_owned()));
    result.push(json_variant(inner, |c| matches!(c, '&' | '<' | '>')));
    result.push(json_variant(inner, |c| !c.is_ascii()));
    for engine in [
        &base64::engine::general_purpose::STANDARD,
        &base64::engine::general_purpose::STANDARD_NO_PAD,
        &base64::engine::general_purpose::URL_SAFE,
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ] {
        result.push(Zeroizing::new(engine.encode(value)));
    }
    // A secret embedded in Basic auth can begin/end inside a base64 unit.
    // Enumerate only the neighbouring bits sharing those two encoded chars
    // (16 combinations per alignment), reusing ordinary streaming matching.
    for prefix in 0..3usize {
        let leading = (prefix * 8) % 6;
        let trailing = (6 - ((prefix + value.len()) * 8) % 6) % 6;
        for left in 0..(1u8 << leading) {
            for right in 0..(1u8 << trailing) {
                let mut bytes = Zeroizing::new(vec![0; prefix]);
                if let Some(last) = bytes.last_mut() {
                    *last = left;
                }
                bytes.extend_from_slice(value.as_bytes());
                if trailing > 0 {
                    bytes.push(right << (8 - trailing));
                }
                for engine in [
                    &base64::engine::general_purpose::STANDARD_NO_PAD,
                    &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                ] {
                    let encoded = Zeroizing::new(engine.encode(&bytes));
                    result.push(Zeroizing::new(
                        encoded[prefix * 8 / 6..((prefix + value.len()) * 8).div_ceil(6)]
                            .to_owned(),
                    ));
                }
            }
        }
    }
    let mut hex = Zeroizing::new(String::with_capacity(value.len() * 2));
    for b in value.bytes() {
        let _ = write!(hex, "{b:02x}");
    }
    result.push(Zeroizing::new(hex.to_uppercase()));
    result.push(hex);
    for lower in [false, true] {
        for plus in [false, true] {
            result.push(Zeroizing::new(
                value
                    .bytes()
                    .map(|b| {
                        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                            (b as char).to_string()
                        } else if plus && b == b' ' {
                            "+".into()
                        } else if lower {
                            format!("%{b:02x}")
                        } else {
                            format!("%{b:02X}")
                        }
                    })
                    .collect(),
            ));
        }
    }
    result
}

pub(crate) fn mask(text: &str, needles: &[Zeroizing<String>]) -> String {
    mask_bytes(text.as_bytes(), needles)
}

fn marked_bytes(bytes: &[u8], needles: &[Zeroizing<String>]) -> Vec<bool> {
    let mut marked = vec![false; bytes.len()];
    let (flat, starts, ends) = flatten(bytes);
    let (decoded, decoded_starts, decoded_ends) = url_decode(&flat, &starts, &ends);
    let (base64, base64_starts, base64_ends) = base64_decode(&flat, &starts, &ends);
    for needle in needles {
        if needle.is_empty() {
            continue;
        }
        for (start, span) in bytes.windows(needle.len()).enumerate() {
            if span == needle.as_bytes() {
                marked[start..start + needle.len()].fill(true);
            }
        }
        let (clean, _, _) = flatten(needle.as_bytes());
        if clean.is_empty() {
            continue;
        }
        mark_matches(&flat, &starts, &ends, &clean, &mut marked);
        mark_matches(
            &decoded,
            &decoded_starts,
            &decoded_ends,
            &clean,
            &mut marked,
        );
        mark_matches(&base64, &base64_starts, &base64_ends, &clean, &mut marked);
    }
    marked
}

/// Full-context matching with stable source-byte positions for terminal pages.
pub(crate) fn mask_preserving_offsets(
    bytes: &[u8],
    needles: &[Zeroizing<String>],
) -> Zeroizing<Vec<u8>> {
    let marked = marked_bytes(bytes, needles);
    Zeroizing::new(
        bytes
            .iter()
            .zip(marked)
            .map(|(byte, hidden)| if hidden { b'*' } else { *byte })
            .collect(),
    )
}

pub(crate) fn mask_bytes(bytes: &[u8], needles: &[Zeroizing<String>]) -> String {
    let marked = marked_bytes(bytes, needles);
    let mut result = Zeroizing::new(Vec::with_capacity(bytes.len()));
    let mut redacted = false;
    for (i, byte) in bytes.iter().enumerate() {
        if marked[i] {
            if !redacted {
                result.extend_from_slice(b"[REDACTED]");
                redacted = true;
            }
        } else {
            result.push(*byte);
            redacted = false;
        }
    }
    // Decode only after masking: a wrap between UTF-8 bytes must not turn the
    // secret into replacement characters and defeat exact-byte matching.
    crate::redaction::redact_secrets(&String::from_utf8_lossy(&result))
}

fn mark_matches(
    haystack: &[u8],
    starts: &[usize],
    ends: &[usize],
    needle: &[u8],
    marked: &mut [bool],
) {
    let hex = needle.len() >= 2 && needle.iter().all(u8::is_ascii_hexdigit);
    for (i, span) in haystack.windows(needle.len()).enumerate() {
        if span == needle || (hex && span.eq_ignore_ascii_case(needle)) {
            marked[starts[i]..ends[i + needle.len() - 1]].fill(true);
        }
    }
}

fn flatten(bytes: &[u8]) -> (Zeroizing<Vec<u8>>, Vec<usize>, Vec<usize>) {
    let mut flat = Zeroizing::new(Vec::with_capacity(bytes.len()));
    let mut starts = Vec::with_capacity(bytes.len());
    let mut ends = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if matches!(bytes[i], b'\r' | b'\n') {
            i += 1;
            continue;
        }
        if bytes[i] == 0x1b && i + 1 < bytes.len() {
            if bytes[i + 1] == b'[' {
                let mut j = i + 2;
                while j < bytes.len() && !(0x40..=0x7e).contains(&bytes[j]) {
                    j += 1;
                }
                if j < bytes.len() {
                    i = j + 1;
                    continue;
                }
            } else if bytes[i + 1] == b']' {
                let mut j = i + 2;
                while j < bytes.len()
                    && bytes[j] != 7
                    && !(bytes[j] == 0x1b && bytes.get(j + 1) == Some(&b'\\'))
                {
                    j += 1;
                }
                if j < bytes.len() {
                    i = j + if bytes[j] == 7 { 1 } else { 2 };
                    continue;
                }
            }
        }
        flat.push(bytes[i]);
        starts.push(i);
        ends.push(i + 1);
        i += 1;
    }
    (flat, starts, ends)
}

/// Map decoded bytes back to whole base64 groups, even when a secret starts
/// inside a group (for example Basic auth's username:password payload).
fn base64_decode(
    bytes: &[u8],
    starts: &[usize],
    ends: &[usize],
) -> (Zeroizing<Vec<u8>>, Vec<usize>, Vec<usize>) {
    let mut decoded = Zeroizing::new(Vec::new());
    let mut ds = Vec::new();
    let mut de = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        // `=` is never part of a token: trailing it is padding, interior it
        // separates `key=<base64>`.
        if !(bytes[i].is_ascii_alphanumeric() || b"+/-_".contains(&bytes[i])) {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || b"+/-_".contains(&bytes[i])) {
            i += 1;
        }
        let end = i;
        let token = &bytes[start..end];
        let value = base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(token)
            .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(token));
        if let Ok(value) = value {
            let value = Zeroizing::new(value);
            for (j, byte) in value.iter().enumerate() {
                let group = start + (j / 3) * 4;
                decoded.push(*byte);
                ds.push(starts[group]);
                de.push(ends[(group + 4).min(end) - 1]);
            }
            // Valid stored values cannot contain NUL. Keep separate tokens
            // separate so matching cannot invent a secret across two blobs.
            decoded.push(0);
            ds.push(starts[start]);
            de.push(ends[i - 1]);
        }
    }
    (decoded, ds, de)
}

fn url_decode(
    bytes: &[u8],
    starts: &[usize],
    ends: &[usize],
) -> (Zeroizing<Vec<u8>>, Vec<usize>, Vec<usize>) {
    let mut decoded = Zeroizing::new(Vec::with_capacity(bytes.len()));
    let mut ds = Vec::new();
    let mut de = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(a), Some(b)) = (digit(bytes[i + 1]), digit(bytes[i + 2]))
        {
            decoded.push(a * 16 + b);
            ds.push(starts[i]);
            de.push(ends[i + 2]);
            i += 3;
        } else {
            decoded.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
            ds.push(starts[i]);
            de.push(ends[i]);
            i += 1;
        }
    }
    (decoded, ds, de)
}
fn digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Hold only a possible secret prefix until the next chunk disambiguates it.
/// Nothing withheld here may reach the terminal grid, streams or captures.
#[derive(Default)]
pub(crate) struct StreamMask {
    needles: Vec<Zeroizing<String>>,
    pending: Zeroizing<String>,
    pending_marks: Vec<bool>,
}

impl StreamMask {
    pub(crate) fn active(&self) -> bool {
        !self.needles.is_empty()
    }
    pub(crate) fn push(&mut self, data: &str, needles: Vec<Zeroizing<String>>) -> Option<String> {
        for needle in needles {
            if !self.needles.iter().any(|known| **known == *needle) {
                self.needles.push(needle);
            }
        }
        if self.needles.is_empty() {
            return None;
        }
        // Queries delimit producer tokens: finish the candidate before the
        // terminal answers, so cursor reports include all preceding text.
        // Assumption: a producer never inserts a query inside a secret token.
        // SGR, wraps and other non-query controls remain transparent to matching.
        let queries = super::terminal_queries::ranges(data);
        if queries.is_empty() {
            return Some(self.push_segment(data, false));
        }
        let mut output = String::new();
        let mut start = 0;
        for query in queries {
            output.push_str(&self.push_segment(&data[start..query.start], true));
            // Only parsed, payload-free protocol queries take this path;
            // arbitrary CSI/OSC content still goes through the sanitizer.
            output.push_str(&data[query.clone()]);
            start = query.end;
        }
        output.push_str(&self.push_segment(&data[start..], false));
        Some(output)
    }

    fn push_segment(&mut self, data: &str, finish: bool) -> String {
        self.pending.push_str(data);
        let bytes = self.pending.as_bytes();
        let (flat, starts, ends) = flatten(bytes);
        let (decoded, decoded_starts, _) = url_decode(&flat, &starts, &ends);
        let mut cutoff = bytes.len();
        for needle in &self.needles {
            let (needle, _, _) = flatten(needle.as_bytes());
            let length = suffix_prefix_len(&flat, &needle);
            if length > 0 {
                cutoff = cutoff.min(starts[flat.len() - length]);
            }
            let length = suffix_prefix_len(&decoded, &needle);
            if length > 0 {
                cutoff = cutoff.min(decoded_starts[decoded.len() - length]);
            }
        }
        // Percent escapes may straddle reads before they decode to a prefix.
        if let Some(i) = flat.iter().rposition(|b| *b == b'%')
            && flat.len() - i < 3
        {
            cutoff = cutoff.min(starts[i]);
        }
        if finish {
            cutoff = bytes.len();
        }
        // A hostile stream can insert arbitrarily many wraps/escapes into a
        // candidate. Withhold rather than retain unbounded sensitive bytes.
        if bytes.len().saturating_sub(cutoff) > 256 * 1024 {
            self.pending.zeroize();
            self.pending_marks.clear();
            return "[REDACTED]".into();
        }
        let mut marked = marked_bytes(bytes, &self.needles);
        for (mark, previous) in marked.iter_mut().zip(&self.pending_marks) {
            *mark |= previous;
        }
        let masked: Zeroizing<Vec<u8>> = Zeroizing::new(
            bytes
                .iter()
                .zip(&marked)
                .map(|(byte, hidden)| if *hidden { b'*' } else { *byte })
                .collect(),
        );
        while !self.pending.is_char_boundary(cutoff) {
            cutoff -= 1;
        }
        let result = String::from_utf8_lossy(&masked[..cutoff]).into_owned();
        let tail = Zeroizing::new(self.pending[cutoff..].to_owned());
        self.pending_marks = marked[cutoff..].to_vec();
        self.pending.zeroize();
        self.pending = tail;
        result
    }
}

// Linear prefix matching: a 16-KiB nearly-repeated secret must not cause
// quadratic work on each terminal chunk.
fn suffix_prefix_len(bytes: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() {
        return 0;
    }
    let hex = needle.len() >= 2 && needle.iter().all(u8::is_ascii_hexdigit);
    let equal = |a: u8, b: u8| a == b || (hex && a.eq_ignore_ascii_case(&b));
    let mut failure = vec![0; needle.len()];
    for i in 1..needle.len() {
        let mut length = failure[i - 1];
        while length > 0 && !equal(needle[i], needle[length]) {
            length = failure[length - 1];
        }
        if equal(needle[i], needle[length]) {
            length += 1;
        }
        failure[i] = length;
    }
    let mut length = 0;
    for byte in bytes {
        while length > 0 && (length == needle.len() || !equal(*byte, needle[length])) {
            length = failure[length - 1];
        }
        if equal(*byte, needle[length]) {
            length += 1;
        }
    }
    // A whole match is already redacted; retain only a proper overlapping prefix.
    if length == needle.len() {
        failure[length - 1]
    } else {
        length
    }
}
