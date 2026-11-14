use super::StreamMask;
use crate::state::EscapeAwareBuffer;
use zeroize::Zeroizing;

fn needles(value: &str) -> Vec<Zeroizing<String>> {
    vec![Zeroizing::new(value.into())]
}

// Catches: a query behind a possible secret prefix waits for another write.
#[test]
fn queries_finalize_the_preceding_token_at_every_read_split() {
    for query in [
        "\x1b[6n",
        "\x1b[5n",
        "\x1b[c",
        "\x1b[>0c",
        "\x1b[?u",
        "\x1b[?4m",
        "\x1b[?25$p",
        "\x1b[4$p",
        "\x1b[14t",
        "\x1b[18t",
        "\x1b]10;?\x1b\\",
        "\x1b]11;?\x07",
        "\x1b]12;?\x1b\\",
        "\x1bZ",
    ] {
        let input = format!("p{query}");
        for split in 0..=input.len() {
            let mut mask = StreamMask::default();
            let mut escapes = EscapeAwareBuffer::new();
            let first = escapes.push(&input[..split]);
            let mut output = mask.push(&first, needles("prefix-private-value")).unwrap();
            output.push_str(&mask.push(&escapes.push(&input[split..]), vec![]).unwrap());
            assert_eq!(output, input, "query={query:?}, split={split}");
        }
    }
}

// Catches: supported palette and combined color queries remain held behind a
// possible secret prefix, so a child waiting for their replies times out.
#[test]
fn palette_and_combined_color_queries_do_not_wait_for_secret_prefix() {
    for query in [
        "\x1b]4;1;?\x07",
        "\x1b]4;1;?;255;?\x1b\\",
        "\x1b]10;?;?\x1b\\",
    ] {
        let input = format!("p{query}");
        for split in 0..=input.len() {
            let mut mask = StreamMask::default();
            let mut escapes = EscapeAwareBuffer::new();
            let first = escapes.push(&input[..split]);
            let mut output = mask.push(&first, needles("prefix-private-value")).unwrap();
            output.push_str(&mask.push(&escapes.push(&input[split..]), vec![]).unwrap());
            assert_eq!(output, input, "query={query:?}, split={split}");
        }
    }
}

// Catches: finishing the token releases a complete secret or old overlap marks.
#[test]
fn queries_preserve_redaction_before_and_after_the_boundary() {
    let mut mask = StreamMask::default();
    let first = mask.push("ababa", needles("ababa")).unwrap();
    let query = mask.push("\x1b[6na", vec![]).unwrap();
    let last = mask.push("baba!", vec![]).unwrap();
    assert_eq!(first + &query + &last, "*****\x1b[6n*****!");
}

// Catches: flushing for every escape leaks a secret interrupted by styling,
// cursor movement, OSC metadata or wraps, including when the read splits there.
#[test]
fn non_query_controls_and_plain_writes_still_mask_spanning_secrets() {
    for control in ["", "\x1b[31m", "\x1b[2C", "\x1b]0;title\x07", "\r\n"] {
        let input = format!("!canary{control}-private!");
        for split in 0..=input.len() {
            let mut mask = StreamMask::default();
            let mut escapes = EscapeAwareBuffer::new();
            let first = escapes.push(&input[..split]);
            let mut output = mask.push(&first, needles("canary-private")).unwrap();
            output.push_str(&mask.push(&escapes.push(&input[split..]), vec![]).unwrap());
            assert_eq!(
                output,
                format!("!{}!", "*".repeat(input.len() - 2)),
                "control={control:?}, split={split}"
            );
        }
    }
}

// Catches: query parameters accidentally redacted as numeric secret values.
#[test]
fn query_protocol_bytes_survive_numeric_secret_matching() {
    let mut mask = StreamMask::default();
    assert_eq!(mask.push("6\x1b[6n", needles("6")).unwrap(), "*\x1b[6n");
}

// Catches: arbitrary OSC payloads treated as queries bypass secret masking.
#[test]
fn query_looking_metadata_is_still_sanitized() {
    for input in [
        "\x1b]0;canary-private\x1b[6n\x07!",
        "\x1b]11;canary-private\x07!",
        "\x1b]4;1;?;2;canary-private\x07!",
        "\x1b]10;?;canary-private\x1b\\!",
    ] {
        let mut mask = StreamMask::default();
        let output = mask.push(input, needles("canary-private")).unwrap();
        assert!(!output.contains("canary-private"), "{output:?}");
    }
}

// Catches: query handling changes ordinary text, loses a following prefix, or
// emits a query twice when that prefix later resolves.
#[test]
fn queries_leave_ordinary_text_unchanged_without_replaying_controls() {
    let mut mask = StreamMask::default();
    assert_eq!(
        mask.push("public\x1b[6np", needles("prefix-private-value"))
            .unwrap(),
        "public\x1b[6n"
    );
    assert_eq!(mask.push("ublic!", vec![]).unwrap(), "public!");
    assert_eq!(mask.push("", vec![]).unwrap(), "");
}
