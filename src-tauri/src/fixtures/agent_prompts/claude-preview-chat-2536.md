# Claude preview Chat action — 2536-8e18

Source: real Claude Code 2.1.314 PTY captured on 2026-10-30 through
`/diagnostics/capture` in an isolated debug `tuic-remote --instance`.
Capture SHA-256: `e09d710179eb62be2e4ce65253158222cae4926f94f61490b12c2f815162ddd7`.
The bytes are unchanged; geometry is in the TUICCAP2 header.

The driver opened a real three-question preview form, then sent Down, Down,
Enter. Native output confirms User declined to answer questions and returns
to conversational clarification. This is the native Chat about this action,
not a numeric answer. The input records retain all three keys in order.
Full driver metadata stays under `~/Gits/.tmp/chat-question-preview/live-chat-action/`.
The parser regression reads the screen immediately before the final Enter,
when the unnumbered Chat action is highlighted.
