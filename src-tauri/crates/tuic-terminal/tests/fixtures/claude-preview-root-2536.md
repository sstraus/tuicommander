# Root-helper preview question — 2536 follow-up

The owner reported preview borders inside Chat choice labels on 2026-10-30.
The running development app predates the fix in `81a484f852`.

Source: retained raw output from the live smartmem Claude session
`d8451c73-253f-45c4-8b43-44074fdc8d4b`, read through the TUIC session tool.
The question was already answered when verification started. No input was sent.
The private tool response and unchanged extracted dialog bytes are retained under
`~/Gits/.tmp/chat-question-preview/check-2/`.

This is a **derived synthetic snapshot**, not a diagnostic capture with original
PTY chunk boundaries or timing. The snapshot takes the first complete native
question repaint, adds clear/home to put the dialog at row zero, and wraps it in
one output record in the existing TUICCAP2 format. The 63-row full-screen clear
and 234-column separator in the retained output establish the geometry. Moving
the dialog vertically does not change its native horizontal layout. Application
identifiers in the right preview column are replaced by equal-width names.

The quoted question and answer labels stay unchanged. In particular:

- `Helper root ristretto` continues with `(Recommended)` at column 4 (zero based).
- `Solo su richiesta, con` continues with `sudo` at the same hanging indent.
- The preview rectangle starts at column 34 and ends at column 98.
- A vertical connector and downward arrow remain inside the preview.
- The Notes line and unnumbered `Chat about this` action remain below the pane.

The integration test replays these bytes through `TerminalGrid` and calls the
same `parse_claude_ask_user_question` entry point as production. Its independent
oracle is the owner's native CLI screenshot (`cli2.png`) and the retained native
label columns, not the garbled Chat labels.

Synthetic fixture SHA-256: `e75e9f22b6eaa0678200d66dd845b9721bb1834e495967fdcf3496d412fa0536`.
