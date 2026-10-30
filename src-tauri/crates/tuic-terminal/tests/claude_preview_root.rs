use tuic_terminal::output_parser::{ParsedEvent, parse_claude_ask_user_question};
use tuic_terminal::pty_capture::{CaptureDirection, decode_capture};
use tuic_terminal::terminal_grid::TerminalGrid;

fn assert_root_preview_labels(bytes: &[u8]) {
    let capture = decode_capture(bytes).unwrap();
    let (rows, cols) = capture.geometry.unwrap();
    assert_eq!((rows, cols), (63, 234));
    let mut grid = TerminalGrid::new(rows, cols, 2000);
    for record in capture.records {
        assert_eq!(record.direction, CaptureDirection::Output);
        grid.process(&record.data);
    }
    let rows = grid.screen_text_rows();
    let Some(ParsedEvent::ChoicePrompt { title, options, .. }) =
        parse_claude_ask_user_question(&rows)
    else {
        panic!("native root preview must remain answerable: {rows:#?}");
    };
    assert!(
        title.contains("Come facciamo girare con privilegi root la parte di smartmem che serve?")
    );
    assert_eq!(
        options
            .iter()
            .map(|option| option.label.as_str())
            .collect::<Vec<_>>(),
        [
            "Helper root ristretto (Recommended)",
            "Tutto pguardd come root",
            "Solo su richiesta, con sudo",
            "Chat about this",
        ],
        "preview columns must not enter labels: {options:#?}; rows: {rows:#?}"
    );
    assert!(options[0].highlighted);
    assert!(
        options[0]
            .hint
            .as_deref()
            .is_some_and(|hint| hint.contains("LaunchDaemon"))
    );
}

// Catches: the root-helper preview contaminates answer labels or loses hanging-indent continuations.
#[test]
fn claude_root_preview_keeps_diagram_out_of_wrapped_labels() {
    assert_root_preview_labels(include_bytes!("fixtures/claude-preview-root-2536.tcap"));
}
